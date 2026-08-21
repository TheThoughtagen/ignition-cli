//! Wiremock HTTP contract tests for the [`ReqwestGatewayApi`] seam
//! (research Pattern 5): auth-header construction (token XOR basic XOR
//! neither — never both), 200 parse, 401 → Auth (exit 5), unreachable →
//! Network (exit 4).
//!
//! The base64 literal is `base64("admin:sekret")` — precomputed so the
//! test needs no base64 dependency; the exact-value matcher on the
//! `Authorization` header proves reqwest's `basic_auth` encoding.

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
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(GATEWAY_INFO_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(302)
                .insert_header("Location", "/welcome;jsessionid=abc?foo=bar"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let err = api.gateway_info().await.expect_err("302 must fail");
    match &err {
        CoreError::GatewayNotCommissioned { endpoint } => {
            assert_eq!(
                endpoint.as_deref(),
                Some(format!("{}{}", server.uri(), GATEWAY_INFO_PATH).as_str()),
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
