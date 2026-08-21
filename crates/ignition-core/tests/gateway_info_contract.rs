//! Wiremock HTTP contract tests for the [`ReqwestGatewayApi`] seam
//! (research Pattern 5): auth-header construction (token XOR basic XOR
//! neither — never both), 200 parse, 401 → Auth (exit 5), unreachable →
//! Network (exit 4), plus the FULL classifier matrix every observed 8.3
//! gateway error shape maps through (02-RESEARCH §Auth Model 4).
//!
//! The base64 literal is `base64("admin:sekret")` — precomputed so the
//! test needs no base64 dependency; the exact-value matcher on the
//! `Authorization` header proves reqwest's `basic_auth` encoding.

mod common;

use common::IgnitionMock;
use ignition_core::client::{GatewayApi, ReqwestGatewayApi};
use ignition_core::config::{Credential, Secret};
use ignition_core::error::CoreError;

const GATEWAY_INFO_PATH: &str = "/data/api/v1/gateway-info";

/// Lowercased Debug dump of a recorded request's headers — a deliberately
/// crude but API-stable way to assert header presence/absence across
/// wiremock's header-map types.
fn headers_debug(request: &wiremock::Request) -> String {
    format!("{:?}", request.headers).to_lowercase()
}

/// Token credential → `X-Ignition-API-Token` sent (exact value, via the
/// matcher + `expect(1)` verified on drop), `Authorization` ABSENT —
/// never both.
#[tokio::test]
async fn token_credential_sends_token_header_only() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(GATEWAY_INFO_PATH))
        .and(wiremock::matchers::header(
            "X-Ignition-API-Token",
            "test-token",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "version": "8.3.2",
                "edition": "Standard",
                "state": "RUNNING"
            })),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(
        &server.uri(),
        Some(Credential::Token(Secret::new("test-token"))),
    );
    let info = api.gateway_info().await.expect("gateway_info succeeds");

    let expected_endpoint = format!("{}{}", server.uri(), GATEWAY_INFO_PATH);
    assert_eq!(info.ignition_version, "8.3.2");
    assert_eq!(info.edition.as_deref(), Some("Standard"));
    assert_eq!(
        info.endpoint.as_deref(),
        Some(expected_endpoint.as_str()),
        "client stamps the request URL for CORE-05 endpoint population"
    );

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1, "exactly one request");
    let headers = headers_debug(&requests[0]);
    assert!(
        headers.contains("x-ignition-api-token"),
        "token header must be sent: {headers}"
    );
    assert!(
        !headers.contains("authorization"),
        "token credential must NOT also send Authorization: {headers}"
    );
}

/// Basic credential → `Authorization: Basic <b64>` sent (exact value),
/// `X-Ignition-API-Token` ABSENT — never both.
#[tokio::test]
async fn basic_credential_sends_authorization_only() {
    let server = wiremock::MockServer::start().await;
    // base64("admin:sekret") — see the module doc.
    let guard = wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(GATEWAY_INFO_PATH))
        .and(wiremock::matchers::header(
            "Authorization",
            "Basic YWRtaW46c2VrcmV0",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"version": "8.3.2"})),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(
        &server.uri(),
        Some(Credential::Basic(
            Secret::new("admin"),
            Secret::new("sekret"),
        )),
    );
    let info = api.gateway_info().await.expect("gateway_info succeeds");
    assert_eq!(info.ignition_version, "8.3.2");

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    let headers = headers_debug(&requests[0]);
    assert!(
        headers.contains("authorization"),
        "basic credential must send Authorization: {headers}"
    );
    assert!(
        !headers.contains("x-ignition-api-token"),
        "basic credential must NOT also send the token header: {headers}"
    );
}

/// No credential → header-less request (gateway-info is `auth: none`),
/// proven inside the 401 test: neither auth header appears.
#[tokio::test]
async fn no_credential_is_header_less() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(GATEWAY_INFO_PATH))
        .respond_with(wiremock::ResponseTemplate::new(401))
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let err = api.gateway_info().await.expect_err("401 must fail");

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    let headers = headers_debug(&requests[0]);
    assert!(
        !headers.contains("authorization") && !headers.contains("x-ignition-api-token"),
        "no credential → no auth headers at all: {headers}"
    );

    match &err {
        CoreError::Auth { status, endpoint } => {
            assert_eq!(*status, 401);
            assert_eq!(
                endpoint.as_deref(),
                Some(format!("{}{}", server.uri(), GATEWAY_INFO_PATH).as_str()),
                "endpoint populated (CORE-05)"
            );
        }
        other => panic!("wrong error class: {other}"),
    }
    assert_eq!(err.exit_code(), 5);
    assert_eq!(err.code(), "auth_rejected");
}

/// Connection refused (dead loopback port — instant TCP refusal) →
/// `CoreError::Network` (exit 4) with the full request URL.
#[tokio::test]
async fn connection_refused_maps_to_network_exit_4() {
    let api = ReqwestGatewayApi::for_tests("http://127.0.0.1:1", None);
    let err = api.gateway_info().await.expect_err("dead port must fail");
    match &err {
        CoreError::Network { url, .. } => {
            assert_eq!(url, "http://127.0.0.1:1/data/api/v1/gateway-info");
        }
        other => panic!("wrong error class: {other}"),
    }
    assert_eq!(err.exit_code(), 4);
    assert_eq!(err.code(), "network_error");
    assert_eq!(
        err.endpoint().as_deref(),
        Some("http://127.0.0.1:1/data/api/v1/gateway-info"),
        "Network's url doubles as its endpoint (CORE-05)"
    );
}

/// THE live-capture regression (02-RESEARCH §Status/info): a real 8.3.6
/// gateway answers gateway-info with `ignitionVersion` (NOT `version`) —
/// Phase 1's model failed deserialization against every live gateway.
/// This golden body carries the captured shape; unknown-to-the-model keys
/// (hostname, deploymentMode, …) are tolerated by design.
#[tokio::test]
async fn live_capture_gateway_info_parses() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(GATEWAY_INFO_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "name": "ign-live-rig",
                "redundancyRole": "Independent",
                "edition": "standard",
                "hostname": "localhost",
                "port": "8088",
                "ignitionVersion": "8.3.6 (b2026042713)",
                "deploymentMode": "STANDARD",
                "timeZone": "America/New_York",
                "timeZoneId": "America/New_York",
                "jvmVersion": "17.0.11",
                "allowUnsignedModules": false,
                "license": {
                    "mode": "Trial",
                    "validForVersion": 8.3,
                    "expirationDate": "2026-08-24T19:00:00Z",
                    "licenseRestrictions": []
                }
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let info = api.gateway_info().await.expect("live shape must parse");
    assert_eq!(info.ignition_version, "8.3.6 (b2026042713)");
    assert_eq!(info.name.as_deref(), Some("ign-live-rig"));
    assert_eq!(
        info.license.as_ref().expect("license present").mode,
        "Trial"
    );
    assert!(!ignition_core::client::version::below_minimum(
        &info.ignition_version
    ));
}

/// Uncommissioned gateway = 302 → `/welcome` on EVERY route (02-RESEARCH
/// Pitfall 6). The client MUST NOT follow the redirect (Policy::none) —
/// the wizard HTML can never masquerade as a 200 — and classifies into
/// `gateway_not_commissioned` (exit 6) with the commissioning hint.
#[tokio::test]
async fn redirect_to_welcome_maps_to_not_commissioned() {
    let mock = IgnitionMock::start().await;
    mock.redirect(GATEWAY_INFO_PATH, "/welcome;jsessionid=abc?foo=bar")
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let err = api.gateway_info().await.expect_err("302 must fail");
    match &err {
        CoreError::GatewayNotCommissioned { endpoint } => {
            assert_eq!(
                endpoint.as_deref(),
                Some(format!("{}{}", mock.uri(), GATEWAY_INFO_PATH).as_str()),
                "endpoint populated (CORE-05)"
            );
        }
        other => panic!("wrong error class: {other}"),
    }
    assert_eq!(err.exit_code(), 6);
    assert_eq!(err.code(), "gateway_not_commissioned");
    assert!(
        err.hint()
            .expect("hint required")
            .contains("commissioning wizard"),
        "hint names the wizard: {:?}",
        err.hint()
    );
}

// ---------------------------------------------------------------------------
// The classifier matrix (02-RESEARCH §Auth Model 4) — every observed
// live-gateway error shape, each pinned to its LOCKED class + slug + exit
// code + endpoint + hint substring, driven through IgnitionMock so later
// plans' scenarios stay 3-liners.
// ---------------------------------------------------------------------------

/// Shared assertions for a classified matrix scenario.
async fn classify_scenario(mock: IgnitionMock) -> CoreError {
    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let err = api.gateway_info().await.expect_err("scenario must fail");
    // Internal carries its URL inside the message text instead (the only
    // variant without an endpoint field); every other class populates
    // endpoint (CORE-05).
    if !matches!(err, CoreError::Internal(_)) {
        assert_eq!(
            err.endpoint().as_deref(),
            Some(format!("{}{}", mock.uri(), GATEWAY_INFO_PATH).as_str()),
            "endpoint populated for every classified scenario (CORE-05)"
        );
    }
    err
}

/// HTML 401 (the Jetty page /data/api/v1 always answers with) → Auth
/// (exit 5), hint naming the FULL `name:key` token format — never an
/// internal decode error on the HTML body.
#[tokio::test]
async fn html_401_classifies_auth_with_name_key_hint() {
    let mock = IgnitionMock::start().await;
    mock.html_error("GET", GATEWAY_INFO_PATH, 401).await;

    let err = classify_scenario(mock).await;
    assert!(
        matches!(&err, CoreError::Auth { status: 401, .. }),
        "wrong class: {err}"
    );
    assert_eq!(err.exit_code(), 5);
    assert_eq!(err.code(), "auth_rejected");
    assert!(
        err.hint().expect("hint").contains("name:key"),
        "401 hint names the name:key format: {:?}",
        err.hint()
    );
}

/// HTML 403 → Auth (exit 5) with the three-part setup hint.
#[tokio::test]
async fn html_403_classifies_auth_with_three_parts_hint() {
    let mock = IgnitionMock::start().await;
    mock.html_error("GET", GATEWAY_INFO_PATH, 403).await;

    let err = classify_scenario(mock).await;
    assert!(
        matches!(&err, CoreError::Auth { status: 403, .. }),
        "wrong class: {err}"
    );
    assert_eq!(err.exit_code(), 5);
    let hint = err.hint().expect("hint");
    assert!(
        hint.contains("three parts") && hint.contains("secure connections"),
        "403 hint carries the three-part setup: {hint}"
    );
}

/// 302 → `/idp/…` (not logged in — shouldn't happen for token auth, but
/// observed on `/data/app/*`) → Auth class.
#[tokio::test]
async fn redirect_to_idp_classifies_auth() {
    let mock = IgnitionMock::start().await;
    mock.redirect(GATEWAY_INFO_PATH, "/idp/default/authn/login")
        .await;

    let err = classify_scenario(mock).await;
    assert!(
        matches!(&err, CoreError::Auth { status: 302, .. }),
        "wrong class: {err}"
    );
    assert_eq!(err.exit_code(), 5);
}

/// 503 during the restart window (webserver up, services down — verified
/// lifecycle) → `gateway_restarting` (exit 6) with the `ign wait restart`
/// hint, never a fatal Network.
#[tokio::test]
async fn service_unavailable_classifies_gateway_restarting() {
    let mock = IgnitionMock::start().await;
    mock.status_json(
        "GET",
        GATEWAY_INFO_PATH,
        503,
        serde_json::json!({"message": "Service Unavailable"}),
    )
    .await;

    let err = classify_scenario(mock).await;
    assert!(
        matches!(&err, CoreError::GatewayRestarting { .. }),
        "wrong class: {err}"
    );
    assert_eq!(err.exit_code(), 6);
    assert_eq!(err.code(), "gateway_restarting");
    assert!(
        err.hint().expect("hint").contains("ign wait restart"),
        "hint names the wait command: {:?}",
        err.hint()
    );
}

/// 404 JSON `{"message": "No route match for path: …"}` → `not_found`
/// (exit 6) — the shape a pre-8.3 gateway also answers with.
#[tokio::test]
async fn not_found_json_classifies_not_found() {
    let mock = IgnitionMock::start().await;
    mock.status_json(
        "GET",
        GATEWAY_INFO_PATH,
        404,
        serde_json::json!({"message": "No route match for path: /data/api/v1/gateway-info"}),
    )
    .await;

    let err = classify_scenario(mock).await;
    assert!(
        matches!(&err, CoreError::NotFound { .. }),
        "wrong class: {err}"
    );
    assert_eq!(err.exit_code(), 6);
    assert_eq!(err.code(), "not_found");
    assert!(
        err.hint().expect("hint").contains("pre-8.3"),
        "hint mentions the pre-8.3 possibility: {:?}",
        err.hint()
    );
}

/// Unclassifiable 500 HTML → Internal (exit 1) with the Jetty page's own
/// sniffed `Error 500` detail folded into the message (not a bare
/// `.json()` decode crash).
#[tokio::test]
async fn internal_500_html_sniffs_detail() {
    let mock = IgnitionMock::start().await;
    mock.html_error("GET", GATEWAY_INFO_PATH, 500).await;

    let err = classify_scenario(mock).await;
    let CoreError::Internal(message) = &err else {
        panic!("wrong class: {err}")
    };
    assert_eq!(err.exit_code(), 1);
    assert!(
        message.contains("Error 500"),
        "message carries the sniffed Jetty title: {message}"
    );
    assert!(
        message.contains("Server Error"),
        "message carries the sniffed MESSAGE row: {message}"
    );
}
