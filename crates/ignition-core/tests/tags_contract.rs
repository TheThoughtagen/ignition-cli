//! Wiremock contract for the tags family (05-04) — TWO seams, one
//! family, all pins against the REAL [`ReqwestGatewayApi`] (trait
//! level; the binary-level goldens live in
//! `ignition-cli/tests/contract_tags.rs`).
//!
//! Task 1 pins — the NATIVE provider CRUD (TAGS-01,
//! `ignition/tag-provider` config-resource REST, live-proven in
//! 05-RESEARCH):
//! - list/find ride the resource paths with the UI's `limit=-1`
//!   convention, records parsing through the passthrough model
//!   (`metrics.tagCount`, `healthchecks.status` ride as raw values);
//! - create POSTs the **JSON ARRAY** body — the live-proven shape,
//!   recorded-request pinned field-for-field (a bare object 400s on
//!   real gateways);
//! - delete embeds BOTH `{name}` and `{signature}` on the PATH (the
//!   find→signature→delete chain) with the locked per-segment
//!   encoder's over-encoding (`-` → `%2D`).
//!
//! Task 2 pins (the deployed-route half: precondition refusal
//! matrix, browse/read/write shapes) extend this file below.

//! Task 2 pins (the deployed-route half, TAGS-02/03/04): the
//! precondition refusal matrix at the ACTION level through the real
//! client (405 → `routes_not_deployed` exit 6 with the
//! `ign webdev deploy` hint; version mismatch →
//! `route_version_mismatch`), and the browse/read/write route-call
//! shapes (read passthrough, the write body pin) riding
//! `/system/webdev/{project}/cli/tags`.

use ignition_core::actions::tags::{tags_browse, tags_read, tags_write};
use ignition_core::client::GatewayApi;
use ignition_core::client::ReqwestGatewayApi;
use ignition_core::client::query::ListQuery;
use ignition_core::client::tags::TagProviderCreate;

/// The version-action 200-ok body every Present fixture answers.
fn version_body(route_version: &str) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "data": {"routeVersion": route_version, "minCli": "1.0"},
    })
}

/// The provider list parses through the passthrough model with the
/// UI's `limit=-1` convention on the query (matcher-pinned — the
/// connections-family precedent).
#[tokio::test]
async fn provider_list_parses_with_limit_minus_one() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/resources/list/ignition/tag-provider",
        ))
        .and(wiremock::matchers::query_param("limit", "-1"))
        .and(wiremock::matchers::query_param("offset", "0"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {
                        "name": "default",
                        "enabled": true,
                        "config": {"profile": {"type": "STANDARD"}},
                        "metrics": {"tagCount": 12},
                        "healthchecks": {"status": "OK"},
                        "collection": "core"
                    },
                    {
                        "name": "System",
                        "enabled": true,
                        "config": {"profile": {"type": "MANAGED"}},
                        "metrics": {"tagCount": 3},
                        "healthchecks": {"status": "OK"}
                    }
                ],
                "metadata": {"total": 2, "matching": 2, "limit": -1, "offset": 0}
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let page = api
        .tag_provider_list(&ListQuery::default())
        .await
        .expect("resource list must parse");
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].name, "default");
    assert_eq!(page.items[0].metrics["tagCount"], 12);
    assert_eq!(page.items[1].healthchecks["status"], "OK");
    assert_eq!(page.metadata.total, 2);
}

/// find rides `/find/{enc}` and carries the `signature` the chained
/// delete needs (the over-encoding encoder: `-` → `%2D`).
#[tokio::test]
async fn provider_find_carries_the_signature() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/resources/find/ignition/tag-provider/p%2D5e2e",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "name": "p-5e2e",
                "enabled": true,
                "signature": "1700000000000",
                "config": {"profile": {"type": "STANDARD"}}
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let record = api
        .tag_provider_find("p-5e2e")
        .await
        .expect("find record parses");
    assert_eq!(record.signature.as_deref(), Some("1700000000000"));
}

/// THE create pin: the body is a JSON **ARRAY** of create records —
/// recorded-request pinned field-for-field (a bare object 400s on
/// real gateways; field order is declaration order, deterministic).
#[tokio::test]
async fn provider_create_posts_the_array_body() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/data/api/v1/resources/ignition/tag-provider",
        ))
        .and(wiremock::matchers::body_json(serde_json::json!([
            {
                "name": "p-5e2e",
                "type": "ignition/tag-provider",
                "collection": "core",
                "enabled": true,
                "config": {"profile": {"type": "STANDARD"}, "settings": {}}
            }
        ])))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    api.tag_provider_create(&[TagProviderCreate::standard("p-5e2e")])
        .await
        .expect("create posts");
    assert_eq!(guard.received_requests().await.len(), 1);
}

/// THE delete pin: BOTH `{name}` and `{signature}` ride the PATH,
/// each through the locked per-segment encoder (the
/// find→signature→delete chain's wire half).
#[tokio::test]
async fn provider_delete_embeds_name_and_signature_on_the_path() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path(
            "/data/api/v1/resources/ignition/tag-provider/p%2D5e2e/1700000000000",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    api.tag_provider_delete("p-5e2e", "1700000000000")
        .await
        .expect("delete-by-signature posts");
}

// ---- Task 2: the deployed-route half (TAGS-02/03/04) ----

/// THE refusal pin: an UNDEPLOYED gateway (405 on the tags probe)
/// refuses `routes_not_deployed` (exit 6) with the hint naming
/// `ign webdev deploy` — and ZERO route calls run past it.
#[tokio::test]
async fn browse_refuses_routes_not_deployed_with_deploy_hint() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"action": "version"}),
        ))
        .respond_with(wiremock::ResponseTemplate::new(405))
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let err = tags_browse(&api, "ign-cli", "", None, false)
        .await
        .expect_err("absent routes refuse pre-deploy");
    assert_eq!(err.code(), "routes_not_deployed");
    assert_eq!(err.exit_code(), 6);
    assert!(
        err.hint().unwrap().contains("ign webdev deploy"),
        "hint names the fix: {err}"
    );
    // Only the VERSION probe hit the wire — the browse never ran.
    assert_eq!(guard.received_requests().await.len(), 1);
}

/// A version-MISMATCHED route refuses `route_version_mismatch`
/// (exit 6) carrying both versions — redeploy or update ign.
#[tokio::test]
async fn browse_refuses_on_route_version_mismatch() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(version_body("0.9.0")))
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let err = tags_browse(&api, "ign-cli", "", None, false)
        .await
        .expect_err("mismatched version refuses");
    assert_eq!(err.code(), "route_version_mismatch");
    assert_eq!(err.exit_code(), 6);
    assert!(
        err.to_string().contains("0.9.0"),
        "deployed version rides the message: {err}"
    );
}

/// Mount the matching version probe (the precondition's pass) on
/// the tags route.
async fn mount_precondition_ok(server: &wiremock::MockServer) {
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"action": "version"}),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(version_body(ignition_core::webdev::ROUTE_BUNDLE_VERSION)),
        )
        .expect(1)
        .mount(server)
        .await;
}

/// browse through the REAL client: precondition passes, the browse
/// action dispatches with the path on the body, entries parse +
/// filter (Property children dropped by default).
#[tokio::test]
async fn browse_dispatches_and_filters_properties() {
    let server = wiremock::MockServer::start().await;
    mount_precondition_ok(&server).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"action": "browse", "path": ""}),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"results": [
                    {"fullPath": "[default]", "name": "default", "tagType": "Provider", "hasChildren": true, "dataType": null},
                    {"fullPath": "[default]T1.value", "name": "value", "tagType": "Property", "hasChildren": false, "dataType": "Float8"}
                ]}
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = tags_browse(&api, "ign-cli", "", None, false)
        .await
        .expect("browse through the real client");
    assert_eq!(result.entries.len(), 1, "Property filtered by default");
    assert_eq!(result.entries[0].path, "[default]");
    assert_eq!(result.entries[0].tag_type, "Provider");
}

/// read through the REAL client: batch body pinned, rows passed
/// through VERBATIM (value raw, quality string never parsed).
#[tokio::test]
async fn read_passes_rows_through_the_real_client() {
    let server = wiremock::MockServer::start().await;
    mount_precondition_ok(&server).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .and(wiremock::matchers::body_partial_json(serde_json::json!({
            "action": "read",
            "paths": ["[default]T1", "[default]Ghost"]
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"results": [
                    {"path": "[default]T1", "value": 7, "quality": "Good", "timestamp": "Mon Aug 24 00:00:00 UTC 2026"},
                    {"path": "[default]Ghost", "value": null, "quality": "Bad_NotFound", "timestamp": "Mon Aug 24 00:00:00 UTC 2026"}
                ]}
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = tags_read(
        &api,
        "ign-cli",
        &["[default]T1".to_string(), "[default]Ghost".to_string()],
    )
    .await
    .expect("read through the real client");
    assert_eq!(result.results.len(), 2);
    assert_eq!(result.results[0].value, 7);
    assert_eq!(result.results[1].quality, "Bad_NotFound");
}

/// THE write body pin through the REAL client: `{action, path,
/// value}` EXACTLY — the scalar riding untyped.
#[tokio::test]
async fn write_body_pins_path_and_value_through_the_real_client() {
    let server = wiremock::MockServer::start().await;
    mount_precondition_ok(&server).await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "action": "write",
            "path": "[default]T1",
            "value": 42
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"results": [{"path": "[default]T1", "quality": "Good"}]}
            })),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = tags_write(&api, "ign-cli", "[default]T1", serde_json::json!(42))
        .await
        .expect("write through the real client");
    assert_eq!(result.quality, "Good");
    assert_eq!(guard.received_requests().await.len(), 1);
}

// ---- Task 1 (05-05, TAGS-05): the tagConfig route's config CRUD ----

use ignition_core::actions::tags::{
    tags_config_create, tags_config_delete, tags_config_edit, tags_config_get,
};

/// THE getConfig pin through the real client: STRING tagPath on the
/// body, and the STRINGIFIED `value`/`defaultValue` sub-dicts are
/// RE-PARSED into real JSON (agents see objects, not
/// JSON-in-a-string — the plan's research-pitfall fixture).
#[tokio::test]
async fn config_get_reparses_stringified_values() {
    let server = wiremock::MockServer::start().await;
    mount_precondition_ok(&server).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/tagConfig",
        ))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "action": "getConfig",
            "tagPath": "[default]P5/T1"
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"config": {
                    "name": "T1",
                    "tagType": "AtomicTag",
                    "value": "{\"dataType\": \"Int4\", \"value\": 123}",
                    "defaultValue": "{\"dataType\": \"Int4\", \"value\": 0}"
                }}
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = tags_config_get(&api, "ign-cli", "[default]P5/T1")
        .await
        .expect("config get through the real client");
    assert_eq!(result.tag_type.as_deref(), Some("AtomicTag"));
    assert_eq!(
        result.config["value"],
        serde_json::json!({"dataType": "Int4", "value": 123}),
        "the stringified value is re-parsed for agents"
    );
    assert_eq!(
        result.config["defaultValue"],
        serde_json::json!({"dataType": "Int4", "value": 0})
    );
}

/// THE create body pin: configure with the SPLIT basePath + the
/// path-derived name riding the definition + collisionPolicy 'a'
/// (create = abort-collision — refusing to clobber an existing
/// node is the server's backstop).
#[tokio::test]
async fn config_create_pins_configure_body_with_abort_policy() {
    let server = wiremock::MockServer::start().await;
    mount_precondition_ok(&server).await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/tagConfig",
        ))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "action": "configure",
            "basePath": "[p5e2e]Area",
            "tags": [{"tagType": "AtomicTag", "value": 42, "name": "Motor1"}],
            "collisionPolicy": "a"
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"results": ["Good"]}
            })),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = tags_config_create(
        &api,
        "ign-cli",
        "[p5e2e]Area/Motor1",
        &serde_json::json!({"tagType": "AtomicTag", "value": 42}),
    )
    .await
    .expect("create through the real client");
    assert_eq!(result.quality, "Good");
    assert_eq!(guard.received_requests().await.len(), 1);
}

/// THE edit body pin: the same configure call with collisionPolicy
/// 'o' (overwrite the single named node — edit semantics).
#[tokio::test]
async fn config_edit_pins_overwrite_policy() {
    let server = wiremock::MockServer::start().await;
    mount_precondition_ok(&server).await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/tagConfig",
        ))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "action": "configure",
            "basePath": "[default]",
            "tags": [{"tagType": "AtomicTag", "value": 99, "name": "T1"}],
            "collisionPolicy": "o"
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"results": ["Good"]}
            })),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    tags_config_edit(
        &api,
        "ign-cli",
        "[default]T1",
        &serde_json::json!({"tagType": "AtomicTag", "value": 99}),
    )
    .await
    .expect("edit through the real client");
    assert_eq!(guard.received_requests().await.len(), 1);
}

/// THE deleteTags pin: batch paths on the body, the echoed count in
/// the result.
#[tokio::test]
async fn config_delete_pins_batch_paths_on_the_wire() {
    let server = wiremock::MockServer::start().await;
    mount_precondition_ok(&server).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/tagConfig",
        ))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "action": "deleteTags",
            "paths": ["[default]T1", "[default]T2"]
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"deleted": 2}
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = tags_config_delete(
        &api,
        "ign-cli",
        &["[default]T1".to_string(), "[default]T2".to_string()],
    )
    .await
    .expect("delete through the real client");
    assert_eq!(result.deleted, 2);
}

/// Precondition-refusal regression pin for the tagConfig half: an
/// undeployed gateway refuses `routes_not_deployed` (exit 6) with
/// ZERO tagConfig route calls — the require_routes inheritance.
#[tokio::test]
async fn config_get_refuses_routes_not_deployed_zero_tagconfig_calls() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .respond_with(wiremock::ResponseTemplate::new(405))
        .expect(1)
        .mount_as_scoped(&server)
        .await;
    let tagconfig_guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/tagConfig",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(0)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let err = tags_config_get(&api, "ign-cli", "[default]T1")
        .await
        .expect_err("absent routes refuse pre-deploy");
    assert_eq!(err.code(), "routes_not_deployed");
    assert_eq!(err.exit_code(), 6);
    assert_eq!(guard.received_requests().await.len(), 1);
    assert_eq!(
        tagconfig_guard.received_requests().await.len(),
        0,
        "zero tagConfig calls past the refusal"
    );
}
