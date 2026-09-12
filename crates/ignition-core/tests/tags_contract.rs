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

// ---- Task 2 (05-05, TAGS-06/09): UDTs + export/import ----

use ignition_core::actions::projects::CollisionPolicy;
use ignition_core::actions::tags::{
    ExportFormat, ImportFormat, tags_export, tags_import, tags_udt_def, tags_udt_types,
};

/// The json-arg bridge for the byte-based import signature (the same
/// value a CLI-side `--file` read produces).
fn json_bytes(value: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&value).expect("Value serializes")
}

/// UDT pins: `listUDTTypes` body + row mapping, `getUDTDefinition`
/// body + the stringified re-parse applied to the definition.
#[tokio::test]
async fn udt_types_and_def_ride_the_tagconfig_route() {
    let server = wiremock::MockServer::start().await;
    // TWO actions run (types + def) — the probe answers repeatedly.
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"action": "version"}),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(version_body(ignition_core::webdev::ROUTE_BUNDLE_VERSION)),
        )
        .expect(2)
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tagConfig"))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "action": "listUDTTypes", "provider": "default"
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"results": [
                    {"fullPath": "[default]_types_/Motor", "name": "Motor", "tagType": "UdtType", "hasChildren": true, "dataType": null}
                ]}
            })),
        )
        .expect(1)
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tagConfig"))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "action": "getUDTDefinition", "provider": "default", "name": "Motor"
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"definition": {
                    "name": "Motor", "tagType": "UdtType",
                    "parameters": {"speed": {"defaultValue": "{\"dataType\": \"Float8\", \"value\": 0.0}"}}
                }}
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let types = tags_udt_types(&api, "ign-cli", "default")
        .await
        .expect("udt types through the real client");
    assert_eq!(types.types.len(), 1);
    assert_eq!(types.types[0].name, "Motor");

    let def = tags_udt_def(&api, "ign-cli", "default", "Motor")
        .await
        .expect("udt def through the real client");
    assert_eq!(
        def.definition["parameters"]["speed"]["defaultValue"],
        serde_json::json!({"dataType": "Float8", "value": 0.0}),
        "the SAME stringified re-parse applies to UDT definitions"
    );
}

/// THE export pin: exportTags (kwargs enforced route-side) returns
/// the JSON-STRING payload; the action PARSES it and writes the
/// pretty JSON to the out file.
#[tokio::test]
async fn export_parses_the_payload_and_writes_the_file() {
    let payload = serde_json::json!([
        {"name": "P5", "tagType": "Folder", "tags": [
            {"name": "T1", "tagType": "AtomicTag", "value": "{\"dataType\": \"Int4\", \"value\": 123}"}
        ]}
    ]);
    let server = wiremock::MockServer::start().await;
    mount_precondition_ok(&server).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/tagConfig",
        ))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "action": "exportTags", "paths": ["[p5e2e]P5"]
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"payload": serde_json::to_string(&payload).expect("serializes")}
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("p5.json");
    let result = tags_export(
        &api,
        "ign-cli",
        &["[p5e2e]P5".to_string()],
        Some(&out),
        ExportFormat::Json,
    )
    .await
    .expect("export through the real client");
    assert_eq!(result.tag_count, 1);
    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out).expect("file written"))
            .expect("pretty JSON parses");
    assert_eq!(written, payload, "verbatim round-trip fidelity");
}

/// THE zero-write collision proof at the WIRE level (the 03-02
/// pattern): abort-policy import browses the target provider, finds
/// `T1` existing, and refuses `tag_collision` (exit 6, hint names
/// the overwrite policy) — the configure mock proves ZERO writes
/// ran past the browse read.
#[tokio::test]
async fn import_abort_refuses_collision_with_zero_configure_writes() {
    let server = wiremock::MockServer::start().await;
    mount_precondition_ok(&server).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"action": "browse", "path": "[p5import]"}),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"results": [
                    {"fullPath": "[p5import]T1", "name": "T1", "tagType": "AtomicTag", "hasChildren": false, "dataType": "Int4"}
                ]}
            })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let configure_guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/tagConfig",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true, "data": {"results": ["Good"]}
            })),
        )
        .expect(0)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let err = tags_import(
        &api,
        "ign-cli",
        "p5import",
        &json_bytes(serde_json::json!([{"name": "T1", "tagType": "AtomicTag"}])),
        CollisionPolicy::Abort,
        ImportFormat::Json,
    )
    .await
    .expect_err("collision refuses before any write");
    assert_eq!(err.code(), "tag_collision");
    assert_eq!(err.exit_code(), 6);
    assert!(
        err.hint().unwrap().contains("--collision-policy overwrite"),
        "hint names the fix: {err}"
    );
    assert_eq!(
        configure_guard.received_requests().await.len(),
        0,
        "ZERO configure writes past the refusal"
    );
}

/// Overwrite: NO browse pre-check (server authority) — the configure
/// body is exactly basePath `[provider]` + the payload VERBATIM +
/// collisionPolicy 'o'.
#[tokio::test]
async fn import_overwrite_pins_configure_body_without_precheck() {
    let server = wiremock::MockServer::start().await;
    mount_precondition_ok(&server).await;
    let payload = serde_json::json!([{"name": "T1", "tagType": "AtomicTag"}]);
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/tagConfig",
        ))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "action": "configure",
            "basePath": "[p5import]",
            "tags": payload,
            "collisionPolicy": "o"
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true, "data": {"results": ["Good"]}
            })),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;
    // A browse mock proves the overwrite path never consults it.
    let browse_guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"action": "browse"}),
        ))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(0)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = tags_import(
        &api,
        "ign-cli",
        "p5import",
        &json_bytes(payload),
        CollisionPolicy::Overwrite,
        ImportFormat::Json,
    )
    .await
    .expect("overwrite imports through the real client");
    assert_eq!(result.collision_policy, "overwrite");
    assert_eq!(result.format, "json");
    assert_eq!(guard.received_requests().await.len(), 1);
    assert_eq!(browse_guard.received_requests().await.len(), 0);
}

// ---- Task 1 (05-06, TAGS-07): the alarms route ----

use ignition_core::actions::tags::{tags_alarms_ack, tags_alarms_active, tags_alarms_history};

/// THE active filter pin: only PRESENT filters ride the body (the
/// kwargs passthrough — `source`/`priority`/`state` go to
/// `system.alarm.queryStatus` verbatim), rows mapping under
/// unit-explicit keys.
#[tokio::test]
async fn alarms_active_pins_filter_kwargs_passthrough() {
    let server = wiremock::MockServer::start().await;
    mount_precondition_ok(&server).await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/alarms"))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "action": "active",
            "source": "prov:tagprov",
            "priority": "High",
            "state": "Active, Unacknowledged"
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"results": [
                    {"eventId": "e-1", "source": "prov:tagprov:/T1/HighLimit", "state": "Active, Unacknowledged", "priority": "High", "name": "HighLimit"}
                ], "count": 1}
            })),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = tags_alarms_active(
        &api,
        "ign-cli",
        Some("prov:tagprov"),
        Some("High"),
        Some("Active, Unacknowledged"),
    )
    .await
    .expect("active through the real client");
    assert_eq!(result.count, 1);
    assert_eq!(result.alarms[0].event_id, "e-1");
    assert_eq!(result.alarms[0].name.as_deref(), Some("HighLimit"));
    assert_eq!(guard.received_requests().await.len(), 1);
}

/// THE journal-missing pin (the honest default-rig path): the
/// alarms route's structured `no_alarm_journal` denial — HTTP 200,
/// `{ok:false, error{code,message}}` — maps to the ADDITIVE
/// `alarm_journal_missing` slug (exit 6) with the hint naming the
/// provisioning chain + README section.
#[tokio::test]
async fn alarms_history_no_alarm_journal_maps_to_actionable_refusal() {
    let server = wiremock::MockServer::start().await;
    mount_precondition_ok(&server).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/alarms",
        ))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "action": "history",
            "startDateMs": 1000,
            "endDateMs": 2000
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": false,
                "error": {
                    "code": "no_alarm_journal",
                    "message": "No alarm journal profile specified"
                }
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let err = tags_alarms_history(&api, "ign-cli", 1_000, 2_000)
        .await
        .expect_err("a journal-less rig refuses history");
    assert_eq!(err.code(), "alarm_journal_missing");
    assert_eq!(err.exit_code(), 6);
    let hint = err.hint().unwrap();
    assert!(
        hint.contains("journal profile") && hint.contains("README"),
        "hint names the chain + the README section: {hint}"
    );
}

/// History success: journal rows ride VERBATIM (the wire shape is
/// journal-dataset-dependent — never re-modeled).
#[tokio::test]
async fn alarms_history_success_passes_journal_rows_verbatim() {
    let server = wiremock::MockServer::start().await;
    mount_precondition_ok(&server).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/alarms"))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"action": "history"}),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"results": [
                    {"eventId": "e-1", "source": "prov:x", "state": "Active, Unacknowledged", "priority": "High", "name": "HighLimit", "eventData": "{\"a\": 1}"}
                ], "count": 1}
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = tags_alarms_history(&api, "ign-cli", 1_000, 2_000)
        .await
        .expect("history through the real client");
    assert_eq!(result.count, 1);
    assert_eq!(
        result.rows[0]["eventData"],
        serde_json::json!("{\"a\": 1}"),
        "journal rows verbatim — eventData string never re-parsed"
    );
    assert!(result.columns.contains(&"eventData".to_string()));
}

/// THE ack body pin: the gateway-scope 3-arg form rides the body
/// (string ids + note + username), and the return — the
/// UNacknowledged remainder — lands in the result with the honest
/// client-side acknowledged count. Full-UUID ids pass through with
/// NO active lookup (one alarms request total).
#[tokio::test]
async fn alarms_ack_pins_three_arg_body_and_remainder() {
    let server = wiremock::MockServer::start().await;
    mount_precondition_ok(&server).await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/alarms"))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "action": "acknowledge",
            "eventIds": ["11111111-1111-1111-1111-111111111111", "22222222-2222-2222-2222-222222222222"],
            "note": "handled",
            "username": "op"
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"unacknowledged": ["22222222-2222-2222-2222-222222222222"]}
            })),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = tags_alarms_ack(
        &api,
        "ign-cli",
        &[
            "11111111-1111-1111-1111-111111111111".to_string(),
            "22222222-2222-2222-2222-222222222222".to_string(),
        ],
        "handled",
        "op",
    )
    .await
    .expect("ack through the real client");
    assert_eq!(result.acknowledged, 1, "requested 2, remainder 1");
    assert_eq!(
        result.unacknowledged,
        vec!["22222222-2222-2222-2222-222222222222".to_string()]
    );
    assert_eq!(guard.received_requests().await.len(), 1);
}

// ---- Task 2 (05-06, TAGS-08): the tagHistory route ----

use ignition_core::actions::tags::tags_history_query;

/// THE history-query pin through the real client: the epoch-ms body
/// shape + the dataset VERBATIM with `t_stamp` preserved EXACTLY
/// (never renamed) and null cells passing through (the
/// historian-less structural default).
#[tokio::test]
async fn history_query_pins_epoch_ms_body_and_t_stamp_passthrough() {
    let server = wiremock::MockServer::start().await;
    mount_precondition_ok(&server).await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/tagHistory",
        ))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "action": "query",
            "paths": ["[default]T1"],
            "startDateMs": 1000,
            "endDateMs": 2000
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {
                    "columns": ["t_stamp", "[default]T1"],
                    "rows": [["Mon Aug 24 00:00:00 UTC 2026", null]],
                    "rowCount": 1
                }
            })),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = tags_history_query(
        &api,
        "ign-cli",
        &["[default]T1".to_string()],
        1_000,
        2_000,
        None,
        None,
    )
    .await
    .expect("history query through the real client");
    assert_eq!(
        result.columns,
        vec!["t_stamp".to_string(), "[default]T1".to_string()],
        "t_stamp preserved EXACTLY — never renamed"
    );
    assert_eq!(result.rows[0][1], serde_json::Value::Null);
    assert_eq!(result.row_count, 1);
    assert_eq!(guard.received_requests().await.len(), 1);
}

// ---- Task 2 (11-02): the tagConfig route's Phase-11 bulk actions ----
//
// Pins at the RAW [`GatewayApi::webdev_route_call`] layer — the
// Rust-side `tags_export` format param lands in 11-04; this plan pins
// the WIRE those actions will ride. REQUEST-pinning discipline
// (10-02): full-body `body_json` (the match IS the recorded-request
// assertion — any drift fails the match) + expect(1) + guard counts.

/// Base64 of the canned export XML — the 11-01 Probe-1b byte-shape
/// constants (CRLF line endings, 3-space indent, NO `<?xml`
/// declaration, trailing CRLF). Base64 equality ⇔ decoded-byte
/// equality (injective), so pinning the string pins the bytes
/// exactly; no base64 crate in the graph (planner lock).
const CANNED_XML_B64: &str = "PFRhZ3MgTWluVmVyc2lvbj0iOC4wLjAiIGxvY2FsZT0iZW5fVVMiPg0KICAgPFRhZyBuYW1lPSJUMSIgdHlwZT0iQXRvbWljVGFnIj4NCiAgICAgIDxQcm9wZXJ0eSBuYW1lPSJ2YWx1ZVNvdXJjZSI+bWVtb3J5PC9Qcm9wZXJ0eT4NCiAgICAgIDxQcm9wZXJ0eSBuYW1lPSJ2YWx1ZSI+NDI8L1Byb3BlcnR5Pg0KICAgPC9UYWc+DQo8L1RhZ3M+DQo=";

/// THE exportTags format pin: the recorded request carries BOTH
/// `format:"xml"` AND the paths (the new body vocabulary), and the
/// response's base64 payload decodes to the canned XML bytes exactly.
#[tokio::test]
async fn export_tags_xml_format_pin_at_the_raw_call_layer() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/tagConfig",
        ))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "action": "exportTags",
            "paths": ["[default]P5"],
            "format": "xml"
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"payload_b64": CANNED_XML_B64, "format": "xml"}
            })),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let data = api
        .webdev_route_call(
            "ign-cli",
            "tagConfig",
            &serde_json::json!({"action": "exportTags", "paths": ["[default]P5"], "format": "xml"}),
            &[],
        )
        .await
        .expect("xml export through the raw call layer");
    assert_eq!(data["format"], "xml");
    assert_eq!(
        data["payload_b64"], CANNED_XML_B64,
        "base64 pinned verbatim — decodes to the canned XML bytes exactly"
    );
    assert_eq!(guard.received_requests().await.len(), 1);
}

/// THE importTagsFile body pin: `file_b64`/`basePath`/`collisionPolicy`
/// ride the request body VERBATIM, and the QualityCode strings parse
/// through the envelope untouched (str() rendering, never re-modeled).
#[tokio::test]
async fn import_tags_file_body_pins_file_b64_base_path_and_policy() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/tagConfig",
        ))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "action": "importTagsFile",
            "file_b64": CANNED_XML_B64,
            "basePath": "[default]Tgt",
            "collisionPolicy": "a"
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"results": ["Good", "Good"]}
            })),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let data = api
        .webdev_route_call(
            "ign-cli",
            "tagConfig",
            &serde_json::json!({
                "action": "importTagsFile",
                "file_b64": CANNED_XML_B64,
                "basePath": "[default]Tgt",
                "collisionPolicy": "a"
            }),
            &[],
        )
        .await
        .expect("importTagsFile through the raw call layer");
    assert_eq!(data["results"], serde_json::json!(["Good", "Good"]));
    assert_eq!(guard.received_requests().await.len(), 1);
}

/// The provider-root denial shape: the route's
/// `provider_root_unsupported` envelope maps onto the NAMED taxonomy
/// slug (exit 6) through the raw call layer — the same mapping the
/// existing tagConfig contract asserts.
#[tokio::test]
async fn import_tags_file_provider_root_denial_maps_to_the_named_slug() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/tagConfig",
        ))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"action": "importTagsFile"}),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": false,
                "error": {
                    "code": "provider_root_unsupported",
                    "message": "provider-root tag paths are not supported on WebDev threads (no RpcContext) -- use a subtree path like [provider]folder"
                }
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let err = api
        .webdev_route_call(
            "ign-cli",
            "tagConfig",
            &serde_json::json!({
                "action": "importTagsFile",
                "file_b64": CANNED_XML_B64,
                "basePath": "[default]",
                "collisionPolicy": "a"
            }),
            &[],
        )
        .await
        .expect_err("the provider-root refusal parses");
    assert_eq!(err.code(), "provider_root_unsupported");
    assert_eq!(err.exit_code(), 6);
}

/// The invalid-policy denial shape: `invalid_collision_policy` is a
/// route-level contract string, NOT a CoreError slug — it rides the
/// `webdev_route_error` verbatim contract agents branch on (exit 6).
#[tokio::test]
async fn import_tags_file_invalid_policy_rides_the_route_error_contract() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/tagConfig",
        ))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"action": "importTagsFile"}),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": false,
                "error": {
                    "code": "invalid_collision_policy",
                    "message": "collisionPolicy must be 'a' (abort) or 'o' (overwrite)"
                }
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let err = api
        .webdev_route_call(
            "ign-cli",
            "tagConfig",
            &serde_json::json!({
                "action": "importTagsFile",
                "file_b64": CANNED_XML_B64,
                "collisionPolicy": "i"
            }),
            &[],
        )
        .await
        .expect_err("the policy refusal parses");
    assert_eq!(err.code(), "webdev_route_error");
    assert_eq!(err.exit_code(), 6);
    assert!(
        err.to_string().contains("invalid_collision_policy"),
        "the route's code rides the verbatim contract: {err}"
    );
}

// ---- 11-04 Task 1: the ACTION-layer xml export (raw-byte passthrough) ----
//
// The decoded canned XML — byte-for-byte the 11-01 Probe-1b constants
// (CRLF line endings, 3-space indent, NO <?xml declaration, trailing
// CRLF). Byte equality of decode(payload_b64) against THIS string is
// the transport-fidelity assertion (base64 decode is exact by
// construction, so string equality ⇔ byte equality here).
const CANNED_XML: &str = "<Tags MinVersion=\"8.0.0\" locale=\"en_US\">\r\n   <Tag name=\"T1\" type=\"AtomicTag\">\r\n      <Property name=\"valueSource\">memory</Property>\r\n      <Property name=\"value\">42</Property>\r\n   </Tag>\r\n</Tags>\r\n";

/// THE xml export action pin: the recorded request carries
/// `format:"xml"` + paths, and the action's decoded payload equals
/// the canned XML bytes EXACTLY (the transport-fidelity seam — zero
/// parse/normalize/re-serialize between the envelope and the caller).
#[tokio::test]
async fn export_xml_action_decodes_gateway_bytes_unchanged() {
    let server = wiremock::MockServer::start().await;
    mount_precondition_ok(&server).await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/tagConfig",
        ))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "action": "exportTags",
            "paths": ["[default]P11Seed"],
            "format": "xml"
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"payload_b64": CANNED_XML_B64, "format": "xml"}
            })),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = tags_export(
        &api,
        "ign-cli",
        &["[default]P11Seed".to_string()],
        None,
        ExportFormat::Xml,
    )
    .await
    .expect("xml export through the real client");
    assert_eq!(result.format, "xml");
    assert_eq!(result.tag_count, 1, "the tally rides the 11-03 scan");
    assert_eq!(
        result.raw.as_deref(),
        Some(CANNED_XML),
        "decode(payload_b64) == canned bytes EXACTLY (CRLF + trailing CRLF intact)"
    );
    assert_eq!(guard.received_requests().await.len(), 1);
}

/// THE xml file-mode pin: the written file IS the gateway bytes —
/// NO trailing newline appended (research Pitfall 3: the sha256
/// oracle breaks otherwise; json mode's trailing newline is json-
/// mode-only).
#[tokio::test]
async fn export_xml_file_mode_writes_gateway_bytes_with_no_trailing_newline() {
    let server = wiremock::MockServer::start().await;
    mount_precondition_ok(&server).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/tagConfig",
        ))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"action": "exportTags", "format": "xml"}),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"payload_b64": CANNED_XML_B64, "format": "xml"}
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("seed.xml");
    let result = tags_export(
        &api,
        "ign-cli",
        &["[default]P11Seed".to_string()],
        Some(&out),
        ExportFormat::Xml,
    )
    .await
    .expect("xml file export through the real client");
    assert_eq!(result.file.as_deref(), Some(out.to_str().unwrap()));
    let written = std::fs::read(&out).expect("file written");
    assert_eq!(
        written,
        CANNED_XML.as_bytes(),
        "file bytes == gateway bytes — no transformation, no trailing newline"
    );
}

// ---- 11-04 Task 2: the ACTION-layer importTagsFile (byte-faithful upload) ----
//
// REQUEST-pinning discipline (10-02): full-body body_json IS the
// recorded-request assertion — any drift fails the match — plus
// expect(1) + scoped guard counts for the zero-write proofs.

/// THE importTagsFile action pin: input bytes ride `file_b64`
/// VERBATIM (the round-trip proof — encode(decode(CANNED_XML_B64)) ==
/// CANNED_XML_B64 by injectivity), basePath/collisionPolicy exactly
/// `[{provider}]`/`a`, and the scan output feeds the result (names +
/// facts — never a re-parse). The canned XML's MinVersion root attr
/// fires the advisory `xml_export_edited_only` fact, which rides the
/// result for 11-05's report.
#[tokio::test]
async fn import_xml_action_pins_file_b64_and_scan_fed_result() {
    let server = wiremock::MockServer::start().await;
    mount_precondition_ok(&server).await;
    // Clean target: the abort pre-check browses (empty) before the
    // import — the collision matrix itself is pinned separately.
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"action": "browse", "path": "[p11target]"}),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"results": []}
            })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/tagConfig",
        ))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "action": "importTagsFile",
            "file_b64": CANNED_XML_B64,
            "basePath": "[p11target]",
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
    let result = tags_import(
        &api,
        "ign-cli",
        "p11target",
        CANNED_XML.as_bytes(),
        CollisionPolicy::Abort,
        ImportFormat::Xml,
    )
    .await
    .expect("xml import through the real client");
    assert_eq!(result.format, "xml");
    assert_eq!(
        result.top_level_names,
        vec!["T1".to_string()],
        "names come from the 11-03 scan — no re-parse"
    );
    assert_eq!(result.imported, 1);
    assert_eq!(result.collision_policy, "abort");
    assert!(
        result.loss_facts.iter().any(
            |fact| fact.code == ignition_core::actions::tag_loss::codes::XML_EXPORT_EDITED_ONLY
        ),
        "the scan's advisory facts ride the result: {:?}",
        result.loss_facts
    );
    assert!(result.failed.is_empty(), "clean import: no Bad_* elements");
    assert_eq!(guard.received_requests().await.len(), 1);
}

/// THE csv arm pin: legacy CSV bytes ride the SAME importTagsFile
/// body (basePath/policy verbatim), and the scan-derived top-level
/// names come from `scan_csv` (empty Path ⇒ the row's own Name).
#[tokio::test]
async fn import_csv_action_pins_file_b64_and_csv_scan_names() {
    // 8-cell docs-sample-shaped CSV (the scan is width-lenient).
    let csv_input = "Path,Name,Owner,TagType,DataType,Value,Enabled,AccessRights\r\n\
                     # version=1,,,,,,,,\r\n\
                     ,T1,,1,7,42,TRUE,Read_Write\r\n";
    let server = wiremock::MockServer::start().await;
    mount_precondition_ok(&server).await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/tagConfig",
        ))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "action": "importTagsFile",
            "file_b64": "UGF0aCxOYW1lLE93bmVyLFRhZ1R5cGUsRGF0YVR5cGUsVmFsdWUsRW5hYmxlZCxBY2Nlc3NSaWdodHMNCiMgdmVyc2lvbj0xLCwsLCwsLCwNCixUMSwsMSw3LDQyLFRSVUUsUmVhZF9Xcml0ZQ0K",
            "basePath": "[p11target]",
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
    let result = tags_import(
        &api,
        "ign-cli",
        "p11target",
        csv_input.as_bytes(),
        CollisionPolicy::Overwrite,
        ImportFormat::Csv,
    )
    .await
    .expect("csv import through the real client");
    assert_eq!(result.format, "csv");
    assert_eq!(
        result.top_level_names,
        vec!["T1".to_string()],
        "csv scan names: empty Path ⇒ the row's own Name"
    );
    assert_eq!(result.imported, 1);
    assert_eq!(guard.received_requests().await.len(), 1);
}

/// THE zero-write collision proof for the bulk arm: the scan-derived
/// name (`T1`) collides with the browse answer, the action refuses
/// `tag_collision` (exit 6, the overwrite hint) — the importTagsFile
/// mock proves ZERO imports ran past the browse read.
#[tokio::test]
async fn import_xml_abort_refuses_collision_with_zero_import_writes() {
    let server = wiremock::MockServer::start().await;
    mount_precondition_ok(&server).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"action": "browse", "path": "[p11target]"}),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"results": [
                    {"fullPath": "[p11target]T1", "name": "T1", "tagType": "AtomicTag", "hasChildren": false, "dataType": "Int4"}
                ]}
            })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let import_guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/tagConfig",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true, "data": {"results": ["Good"]}
            })),
        )
        .expect(0)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let err = tags_import(
        &api,
        "ign-cli",
        "p11target",
        CANNED_XML.as_bytes(),
        CollisionPolicy::Abort,
        ImportFormat::Xml,
    )
    .await
    .expect_err("scan-fed collision refuses before any write");
    assert_eq!(err.code(), "tag_collision");
    assert_eq!(err.exit_code(), 6);
    assert!(
        err.hint().unwrap().contains("--collision-policy overwrite"),
        "hint names the fix: {err}"
    );
    assert_eq!(
        import_guard.received_requests().await.len(),
        0,
        "ZERO importTagsFile writes past the refusal"
    );
}

/// Overwrite on the bulk arm: NO browse pre-check (server authority)
/// — importTagsFile is the ONLY wire call.
#[tokio::test]
async fn import_xml_overwrite_skips_the_precheck() {
    let server = wiremock::MockServer::start().await;
    mount_precondition_ok(&server).await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/tagConfig",
        ))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"action": "importTagsFile"}),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"results": ["Good"]}
            })),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;
    let browse_guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"action": "browse"}),
        ))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(0)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = tags_import(
        &api,
        "ign-cli",
        "p11target",
        CANNED_XML.as_bytes(),
        CollisionPolicy::Overwrite,
        ImportFormat::Xml,
    )
    .await
    .expect("overwrite imports through the real client");
    assert_eq!(result.collision_policy, "overwrite");
    assert_eq!(guard.received_requests().await.len(), 1);
    assert_eq!(browse_guard.received_requests().await.len(), 0);
}

/// The provider-root denial rides the ACTION layer verbatim: the
/// route's `provider_root_unsupported` envelope (the honest
/// WebDev-thread translation kept from 11-02 — 11-06 proves the
/// thread-class truth live) surfaces as the named slug (exit 6).
#[tokio::test]
async fn import_xml_provider_root_refusal_surfaces_through_the_action() {
    let server = wiremock::MockServer::start().await;
    mount_precondition_ok(&server).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/tagConfig",
        ))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"action": "importTagsFile"}),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": false,
                "error": {
                    "code": "provider_root_unsupported",
                    "message": "provider-root tag paths are not supported on WebDev threads (no RpcContext) -- use a subtree path like [provider]folder"
                }
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let err = tags_import(
        &api,
        "ign-cli",
        "default",
        CANNED_XML.as_bytes(),
        CollisionPolicy::Overwrite,
        ImportFormat::Xml,
    )
    .await
    .expect_err("the provider-root refusal parses through the action");
    assert_eq!(err.code(), "provider_root_unsupported");
    assert_eq!(err.exit_code(), 6);
}

// ---- 11-04 Task 3: generate_legacy_csv (the honest lossy conversion) ----

use ignition_core::actions::tags::{LEGACY_CSV_HEADER, generate_legacy_csv};

/// THE full-fixture generation pin: folder + memory + opc + expression
/// + UDT type + instance (mirroring the probe-5 rig coverage) —
///
/// - numeric TagType/DataType/ExpressionType enums (integers, NEVER
///   strings)
/// - the `# version=1` marker row
/// - folder rows with EMPTY Path cells (capture-wins: non-empty Path
///   NPEs the importer)
/// - the UDT-type TagType-13 placeholder
/// - the instance's UDTParentType verbatim
///
/// Every drop/coercion rides the report.
#[test]
fn generate_legacy_csv_emits_marker_rowed_numeric_enum_rows_and_reports_losses() {
    let subtrees = serde_json::json!([
        {"name": "Motors", "tagType": "Folder", "tags": [
            {"name": "T1", "tagType": "AtomicTag", "valueSource": "memory", "dataType": "Int4", "value": 42},
            {"name": "T2", "tagType": "AtomicTag", "valueSource": "opc", "dataType": "Float8", "value": 3.5,
             "opcServer": "Ignition OPC-UA Server", "opcItemPath": "[dev]a/b"},
            {"name": "T3", "tagType": "AtomicTag", "valueSource": "expression", "expression": "1+1"}
        ]},
        {"name": "MotorType", "tagType": "UdtType", "tags": [
            {"name": "Amps", "tagType": "AtomicTag"}
        ]},
        {"name": "M1", "tagType": "UdtInstance", "udtParentType": "[default]_types_/MotorType"}
    ]);
    let report = generate_legacy_csv(subtrees.as_array().unwrap()).expect("generates");
    assert_eq!(
        report.rows, 6,
        "folder + 3 leaves + type placeholder + instance"
    );

    let text = String::from_utf8(report.csv.clone()).expect("utf-8");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 8, "header + marker + 6 rows");
    assert_eq!(
        lines[0],
        LEGACY_CSV_HEADER.join(","),
        "the header is the docs sample VERBATIM"
    );
    assert_eq!(
        lines[1],
        format!("# version=1{}", ",".repeat(47)),
        "the marker row is required by the importer"
    );
    // Folder row: EMPTY Path (capture-wins), TagType 6.
    assert!(
        lines[2].starts_with(",Motors,,6,"),
        "folder row: empty Path + TagType 6: {}",
        lines[2]
    );
    // Memory leaf: TagType 1 (memory default), DataType 2 = Int4,
    // Value verbatim, Enabled/AccessRights defaults.
    assert!(
        lines[3].starts_with(",T1,,1,2,42,TRUE,Read_Write"),
        "memory tag row is numeric-enum + verbatim value: {}",
        lines[3]
    );
    // OPC leaf: TagType 0, DataType 5 = Float8, OPC columns land.
    let t2: Vec<&str> = lines[4].split(',').collect();
    assert_eq!(t2[0], "", "Path empty");
    assert_eq!(t2[1], "T2");
    assert_eq!(t2[3], "0", "opc → TagType 0");
    assert_eq!(t2[4], "5", "Float8 → DataType 5");
    assert_eq!(t2[8], "Ignition OPC-UA Server");
    assert_eq!(t2[9], "[dev]a/b");
    // Expression leaf: TagType 1 + ExpressionType 1 + Expression cell.
    let t3: Vec<&str> = lines[5].split(',').collect();
    assert_eq!(t3[3], "1");
    assert_eq!(t3[28], "1", "ExpressionType 1 = expression");
    assert_eq!(t3[29], "1+1");
    // UDT type placeholder: TagType 13, EMPTY Path (never `_types_/`).
    assert!(
        lines[6].starts_with(",MotorType,,13,"),
        "the type row is a TagType-13 placeholder with empty Path: {}",
        lines[6]
    );
    // Instance: TagType 10 + UDTParentType verbatim.
    let m1: Vec<&str> = lines[7].split(',').collect();
    assert_eq!(m1[1], "M1");
    assert_eq!(m1[3], "10", "UdtInstance → TagType 10");
    assert_eq!(
        m1[42], "[default]_types_/MotorType",
        "UDTParentType verbatim"
    );

    // The honesty core: drops and coercions are REPORTED.
    assert!(
        report.dropped_keys.iter().any(|key| key == "tags"),
        "the UDT type's members are reported dropped: {:?}",
        report.dropped_keys
    );
    assert!(
        report
            .coerced
            .iter()
            .any(|c| c.contains("folder nesting flattened")),
        "the folder-flattening loss is reported: {:?}",
        report.coerced
    );
    assert!(
        report
            .coerced
            .iter()
            .any(|c| c.contains("UDT type definition 'MotorType' is inexpressible")),
        "the type-definition coercion is reported: {:?}",
        report.coerced
    );
    assert!(
        report
            .coerced
            .iter()
            .any(|c| c.contains("valueSource 'opc'") && c.contains("TagType 0")),
        "the numeric-enum coercion is reported: {:?}",
        report.coerced
    );
}

/// THE alarm-loss pin (the docs' "CSV format does not include support
/// for alarm configurations", capture-proven silent drop): the
/// generator surfaces it as a REPORTED drop, never silence.
#[test]
fn generate_legacy_csv_reports_dropped_alarms() {
    let subtrees = serde_json::json!([
        {"name": "Tank", "tagType": "AtomicTag", "valueSource": "opc",
         "alarms": [{"name": "Low Amps", "priority": "High", "setpointA": 25}],
         "tagGroup": "MyTagGroup", "engLow": 0, "engHigh": 100}
    ]);
    let report = generate_legacy_csv(subtrees.as_array().unwrap()).expect("generates");
    for key in ["alarms", "tagGroup", "engLow", "engHigh"] {
        assert!(
            report.dropped_keys.iter().any(|k| k == key),
            "'{key}' has no legacy column and MUST be reported: {:?}",
            report.dropped_keys
        );
    }
    // And the cells truly stayed empty.
    let text = String::from_utf8(report.csv).expect("utf-8");
    let row: Vec<&str> = text.lines().nth(2).unwrap().split(',').collect();
    assert_eq!(
        row[2], "",
        "Owner empty (non-empty = Bad_Unsupported abort)"
    );
    assert_eq!(row[10], "", "ScanClass dropped (capture: lands nothing)");
    assert_eq!(row[23], "", "EngLow dropped (capture: lands nothing)");
}

/// THE quoting pin (the captured hand-rolled failure): an embedded
/// newline/quote Expression cell round-trips through the csv crate's
/// RFC-4180 quoting byte-exactly.
#[test]
fn generate_legacy_csv_quotes_embedded_newlines_and_quotes() {
    let tricky = "if(currentValue > 1,\n  \"high\",\n  \"low\")";
    let subtrees = serde_json::json!([
        {"name": "T1", "tagType": "AtomicTag", "valueSource": "expression", "expression": tricky}
    ]);
    let report = generate_legacy_csv(subtrees.as_array().unwrap()).expect("generates");
    // Round-trip: the csv reader must restore the cell EXACTLY.
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(report.csv.as_slice());
    let records: Vec<Vec<String>> = reader
        .records()
        .map(|r| r.expect("parses").into_iter().map(String::from).collect())
        .collect();
    assert_eq!(records.len(), 3, "header + marker + 1 row");
    assert_eq!(
        records[2][29], tricky,
        "the Expression cell round-trips exactly"
    );
}
