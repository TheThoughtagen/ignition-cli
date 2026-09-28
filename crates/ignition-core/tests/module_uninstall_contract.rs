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

use std::collections::BTreeMap;

use common::IgnitionMock;
use ignition_core::actions::rig::rig_module_uninstall;
use ignition_core::client::{GatewayApi, ReqwestGatewayApi};
use ignition_core::config::{Credential, ModuleDeclaration, Secret};
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

/// SC-4's structural half (D-19-05): an id the registry does not know
/// has no `&'static ModuleSpec` to name — [`rig_module_uninstall`]
/// takes the spec, never a raw id, so an unregistered id is
/// UNREPRESENTABLE at this call boundary, not merely refused at
/// runtime. The `.expect(0)` catch-all is mounted anyway (the
/// `unsafe_version_is_refused_before_any_request` pattern): if a
/// future refactor ever widened this function to accept a raw id and
/// forgot to re-check the registry, this is the belt that would catch
/// the regression the moment such a caller tried to reach the wire.
#[tokio::test]
async fn unregistered_id_has_no_representable_spec() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::any())
        .respond_with(wiremock::ResponseTemplate::new(500))
        .expect(0)
        .mount_as_scoped(&server)
        .await;

    assert!(
        ignition_core::module::spec_for("not-a-real-module").is_none(),
        "an unregistered id must never resolve to a spec"
    );

    let requests = guard.received_requests().await;
    assert_eq!(
        requests.len(),
        0,
        "zero HTTP requests — there was never a spec to send"
    );
}

/// The still-declared refusal (Task 2): a registry id the rig's
/// `[rigs.NAME.modules]` table STILL names refuses via
/// [`ignition_core::actions::rig::module_still_declared_error`] BEFORE
/// [`rig_module_uninstall`] ever calls `api.uninstall_module` — proven
/// with a REAL [`ReqwestGatewayApi`] pointed at a live mock server
/// carrying a whole-server `.expect(0)` catch-all: if the guard were
/// missing, this exact call would reach the wire and the mock's
/// drop-time assertion would fail the test.
#[tokio::test]
async fn still_declared_id_refuses_before_any_http_request() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::any())
        .respond_with(wiremock::ResponseTemplate::new(500))
        .expect(0)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(
        &server.uri(),
        Some(Credential::Token(Secret::new("name:key"))),
    );
    let spec = ignition_core::module::spec_for(GIT_REGISTRY_ID).expect("git is registered");
    let mut declared = BTreeMap::new();
    declared.insert(
        GIT_REGISTRY_ID.to_string(),
        ModuleDeclaration {
            version: "2.3.4".to_string(),
        },
    );

    let err = rig_module_uninstall(&api, "myrig", spec, &declared)
        .await
        .expect_err("a still-declared module must refuse, never reach the gateway");
    assert_eq!(err.code(), "rig_error");
    assert_eq!(err.exit_code(), 7);
    let message = err.to_string();
    assert!(message.contains(GIT_REGISTRY_ID), "names the id: {message}");
    assert!(message.contains("myrig"), "names the rig: {message}");
    assert!(
        message.contains("[rigs.myrig.modules.git]"),
        "names the config table to edit: {message}"
    );
    assert!(message.contains("ign rig up"), "names the fix: {message}");

    let requests = guard.received_requests().await;
    assert_eq!(
        requests.len(),
        0,
        "the still-declared guard fired before any HTTP request"
    );
}

/// The same refusal when the module is UNDECLARED (the normal case)
/// does NOT fire — the guard is scoped to `declared`, not every call.
#[tokio::test]
async fn undeclared_id_proceeds_past_the_still_declared_guard() {
    let mock = IgnitionMock::start().await;
    mock.list_json(
        "DELETE",
        UNINSTALL_PATH,
        serde_json::json!({
            "success": true,
            "failedUninstalls": {"uninstall": []}
        }),
    )
    .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let spec = ignition_core::module::spec_for(GIT_REGISTRY_ID).expect("git is registered");
    let declared = BTreeMap::new();

    let result = rig_module_uninstall(&api, "myrig", spec, &declared)
        .await
        .expect("an undeclared module must proceed to the gateway call");
    assert!(result.uninstalled);
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
