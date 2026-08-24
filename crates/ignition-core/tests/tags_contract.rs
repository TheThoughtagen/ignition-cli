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

use ignition_core::client::GatewayApi;
use ignition_core::client::ReqwestGatewayApi;
use ignition_core::client::query::ListQuery;
use ignition_core::client::tags::TagProviderCreate;

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
