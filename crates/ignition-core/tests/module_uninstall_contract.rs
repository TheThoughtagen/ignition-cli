//! Wiremock contract tests for `GatewayApi::uninstall_module` (Phase 19,
//! D-19-03/D-19-04) — the guarded, denial-rides-200 gateway write.
//!
//! Live-verified facts pinned here (19-RESEARCH.md): `DELETE
//! /data/api/v1/modules/uninstall` ALWAYS answers HTTP 200; the verdict
//! rides the body's `success`/`failedUninstalls` fields, never the
//! status alone. Exactly ONE request per call (D-19-04: never a batch)
//! — every test's mock carries `.expect(1)`, whose drop-time assertion
//! IS the falsifiable proof.

mod common;

use common::IgnitionMock;
use ignition_core::client::{GatewayApi, ReqwestGatewayApi};
use ignition_core::config::{Credential, Secret};
use ignition_core::error::CoreError;

const UNINSTALL_PATH: &str = "/data/api/v1/modules/uninstall";
const GIT_GATEWAY_ID: &str = "com.axone_io.ignition.git";
const GIT_REGISTRY_ID: &str = "git";

/// Lowercased Debug dump of a recorded request's headers (the 01-04
/// header presence/absence pattern).
fn headers_debug(request: &wiremock::Request) -> String {
    format!("{:?}", request.headers).to_lowercase()
}

/// `success:true` + an empty `failedUninstalls.uninstall` → `Ok(())`.
/// The mock's `.expect(1)` drop-time assertion is D-19-04's proof:
/// exactly one request lands.
#[tokio::test]
async fn success_true_returns_ok() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path(UNINSTALL_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true,
                "failedUninstalls": {"uninstall": []}
            })),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(
        &server.uri(),
        Some(Credential::Token(Secret::new("name:key"))),
    );
    api.uninstall_module(GIT_GATEWAY_ID, GIT_REGISTRY_ID)
        .await
        .expect("success:true must return Ok");

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1, "exactly one request (D-19-04)");
}

/// The request is asserted EXACTLY: DELETE, the uninstall path, and a
/// JSON body of `{"uninstall": ["<gateway id>"]}` — the GATEWAY id, one
/// element, never the registry slug.
#[tokio::test]
async fn request_carries_exact_method_path_and_body() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path(UNINSTALL_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true,
                "failedUninstalls": {"uninstall": []}
            })),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    api.uninstall_module(GIT_GATEWAY_ID, GIT_REGISTRY_ID)
        .await
        .expect("success:true must return Ok");

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method.as_str(), "DELETE");
    assert_eq!(requests[0].url.path(), UNINSTALL_PATH);
    let body: serde_json::Value =
        serde_json::from_slice(&requests[0].body).expect("body must be JSON");
    assert_eq!(
        body,
        serde_json::json!({"uninstall": [GIT_GATEWAY_ID]}),
        "body must carry the GATEWAY id, one element, never the registry slug"
    );
}

/// `success:false` with `failedUninstalls.uninstall == [<gateway id>]`
/// over HTTP 200 → `Err`; the error's slug/exit and hint carry the
/// mounted-`.modl` cause and `ign rig up` as the fix.
#[tokio::test]
async fn success_false_with_failed_id_denies() {
    let mock = IgnitionMock::start().await;
    mock.list_json(
        "DELETE",
        UNINSTALL_PATH,
        serde_json::json!({
            "success": false,
            "failedUninstalls": {"uninstall": [GIT_GATEWAY_ID]}
        }),
    )
    .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let err = api
        .uninstall_module(GIT_GATEWAY_ID, GIT_REGISTRY_ID)
        .await
        .expect_err("success:false must deny");
    assert_eq!(err.code(), "module_uninstall_denied");
    assert_eq!(err.exit_code(), 6);
    assert!(
        err.to_string().contains(GIT_GATEWAY_ID),
        "message must name the failed id: {err}"
    );
    let hint = err.hint().expect("hint required");
    assert!(
        hint.contains(".modl"),
        "hint must name the mounted-.modl cause: {hint}"
    );
    assert!(
        hint.contains("ign rig up"),
        "hint must name ign rig up as the fix: {hint}"
    );
    assert!(
        hint.contains("rig reset"),
        "hint must name the irreversibility recovery path: {hint}"
    );
}

/// `success:false` whose `failedUninstalls` key is ABSENT still denies
/// — absence of detail must never degrade into a reported success.
#[tokio::test]
async fn success_false_with_absent_failed_uninstalls_still_denies() {
    let mock = IgnitionMock::start().await;
    mock.list_json(
        "DELETE",
        UNINSTALL_PATH,
        serde_json::json!({"success": false}),
    )
    .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let err = api
        .uninstall_module(GIT_GATEWAY_ID, GIT_REGISTRY_ID)
        .await
        .expect_err("success:false with no detail must still deny");
    assert_eq!(err.code(), "module_uninstall_denied");
    assert_eq!(err.exit_code(), 6);
}

/// `success:false` whose `failedUninstalls` is present but NOT an
/// object (a shape the gateway must never actually send, but the
/// client must not treat as success either) still denies.
#[tokio::test]
async fn success_false_with_non_object_failed_uninstalls_still_denies() {
    let mock = IgnitionMock::start().await;
    mock.list_json(
        "DELETE",
        UNINSTALL_PATH,
        serde_json::json!({"success": false, "failedUninstalls": "not-an-object"}),
    )
    .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let err = api
        .uninstall_module(GIT_GATEWAY_ID, GIT_REGISTRY_ID)
        .await
        .expect_err("a non-object failedUninstalls must still deny, never Ok");
    assert_eq!(err.code(), "module_uninstall_denied");
    assert_eq!(err.exit_code(), 6);
}

/// A 200 whose body is not JSON at all returns an error, never
/// `Ok(())`.
#[tokio::test]
async fn non_json_200_body_denies_not_ok() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path(UNINSTALL_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_raw("not json at all", "text/plain"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let err = api
        .uninstall_module(GIT_GATEWAY_ID, GIT_REGISTRY_ID)
        .await
        .expect_err("a non-JSON 200 body must never parse as Ok");
    assert_eq!(err.code(), "module_uninstall_denied");
    assert_eq!(err.exit_code(), 6);
}

/// An authenticated call carries the auth header — uninstall is never
/// header-less (contrast `status_ping`'s deliberate header-less proof).
#[tokio::test]
async fn authenticated_call_carries_auth_header() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path(UNINSTALL_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true,
                "failedUninstalls": {"uninstall": []}
            })),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(
        &server.uri(),
        Some(Credential::Token(Secret::new("name:key"))),
    );
    api.uninstall_module(GIT_GATEWAY_ID, GIT_REGISTRY_ID)
        .await
        .expect("success:true must return Ok");

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    let headers = headers_debug(&requests[0]);
    assert!(
        headers.contains("x-ignition-api-token"),
        "uninstall must send the token header: {headers}"
    );
}

/// The denial names BOTH the registry id and the gateway id — D-19-05's
/// error-message half: a caller reading the failure can see which
/// registry entry mapped to which gateway id without a reverse lookup.
#[tokio::test]
async fn denial_names_both_registry_and_gateway_ids() {
    let mock = IgnitionMock::start().await;
    mock.list_json(
        "DELETE",
        UNINSTALL_PATH,
        serde_json::json!({
            "success": false,
            "failedUninstalls": {"uninstall": [GIT_GATEWAY_ID]}
        }),
    )
    .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let err = api
        .uninstall_module(GIT_GATEWAY_ID, GIT_REGISTRY_ID)
        .await
        .expect_err("success:false must deny");
    match &err {
        CoreError::ModuleUninstallDenied {
            module_id,
            gateway_module_id,
            failed,
            endpoint,
        } => {
            assert_eq!(module_id, GIT_REGISTRY_ID);
            assert_eq!(gateway_module_id, GIT_GATEWAY_ID);
            assert_eq!(failed, &vec![GIT_GATEWAY_ID.to_string()]);
            assert!(
                endpoint
                    .as_deref()
                    .is_some_and(|e| e.ends_with(UNINSTALL_PATH))
            );
        }
        other => panic!("wrong variant: {other:?}"),
    }
}
