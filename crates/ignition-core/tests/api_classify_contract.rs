//! The api-call path's exit-partition contract (09-01): every status the
//! gateway can answer a raw `ign api call` with maps to its LOCKED class
//! through [`ReqwestGatewayApi::send_and_classify_for_api`] — pinned as
//! an exhaustive table, not spot-checked.
//!
//! Two invariants this file exists to enforce:
//!
//! 1. **The partition** (research OQ7): an UNCLASSIFIED 4xx on the
//!    api-call path is `GatewayClientError` (exit 2, verbatim body) —
//!    structurally impossible to render as internal/exit-1. The earlier
//!    classifier arms keep their meanings ON THE API PATH TOO: 401/403 →
//!    Auth (exit 5), 404 → NotFound (exit 6), 503 → GatewayRestarting
//!    (exit 6); 500 stays `Internal` (a server error is never the
//!    caller's usage problem).
//! 2. **The non-leak regression** (research Pitfall 1): a 400 through the
//!    CURATED pipeline (get_json) still yields `CoreError::Internal` —
//!    the catch-all cannot fire without the `api_call` parameter, so
//!    v1.0-era commands' exit-1 semantics are byte-for-byte unchanged.

mod common;

use common::{IgnitionMock, jetty_error_html};
use ignition_core::client::{GatewayApi, ReqwestGatewayApi};
use ignition_core::config::{Credential, Secret};
use ignition_core::error::{
    CoreError, GATEWAY_CLIENT_BODY_CAP_BYTES, GATEWAY_CLIENT_BODY_TRUNCATION_MARKER,
    truncate_api_body,
};

/// The curated capability GET the non-leak regression drives (a real
/// trait method, so the test exercises the true production pipeline —
/// get_json → send_and_classify → classify(api_call=false)).
const GATEWAY_INFO_PATH: &str = "/data/api/v1/gateway-info";

/// A NON-resource, non-EAM, non-designer path — outside every
/// route-scoped classifier arm, so each status lands in the final
/// fallback where the api_call guard lives.
fn api_path(case: &str) -> String {
    format!("/data/api/v1/nonexistent-{case}")
}

/// An api client with a token credential (the 09-03 production shape:
/// `ign api call` always carries the profile credential).
fn api_for(mock: &IgnitionMock) -> ReqwestGatewayApi {
    ReqwestGatewayApi::for_tests(
        &mock.uri(),
        Some(Credential::Token(Secret::new("api:contract-token"))),
    )
}

/// Send one request down the api-call pipeline and return the error.
async fn api_call_error(mock: &IgnitionMock, api: &ReqwestGatewayApi, path: &str) -> CoreError {
    let url = format!("{}{}", mock.uri(), path);
    let parsed = url::Url::parse(&url).expect("test URL parses");
    let request = reqwest::Client::new().get(&url);
    api.send_and_classify_for_api(request, &parsed)
        .await
        .expect_err("a non-2xx must classify to Err")
}

/// Mount an EXACT body at `path` (set_body_raw, not set_body_json — the
/// serialized bytes must be byte-identical to the string the verbatim
/// assertions compare against).
async fn mount_exact(mock: &IgnitionMock, method: &str, path: &str, status: u16, body: &str) {
    wiremock::Mock::given(wiremock::matchers::method(method))
        .and(wiremock::matchers::path(path))
        .respond_with(
            wiremock::ResponseTemplate::new(status)
                .set_body_raw(body.to_string(), "application/json"),
        )
        .expect(1)
        .mount(&mock.server)
        .await;
}

/// PARTITION, catch-all half (research OQ7): 400/405/409/415 — statuses
/// no curated arm claims — map to `GatewayClientError`, exit 2, slug
/// `gateway_client_error`, with the gateway's body VERBATIM (exact
/// mounted JSON text) and the endpoint = the full request URL.
#[tokio::test]
async fn unclassified_4xx_map_to_gateway_client_error_exit_2() {
    let mock = IgnitionMock::start().await;
    let api = api_for(&mock);

    let cases: Vec<(u16, String)> = vec![
        (
            400,
            r#"{"error":{"code":"BAD_REQUEST","message":"the caller broke it"}}"#.to_string(),
        ),
        (
            405,
            r#"{"error":{"code":"METHOD_NOT_ALLOWED"}}"#.to_string(),
        ),
        (
            409,
            r#"{"error":{"code":"GENERIC_CONFLICT","detail":"not a prune, not a force"}}"#
                .to_string(),
        ),
        (
            415,
            r#"{"error":{"code":"UNSUPPORTED_MEDIA_TYPE"}}"#.to_string(),
        ),
    ];
    for (status, body) in &cases {
        let path = api_path(&format!("client-{status}"));
        mount_exact(&mock, "GET", &path, *status, body).await;
        let err = api_call_error(&mock, &api, &path).await;
        match &err {
            CoreError::GatewayClientError {
                status: got_status,
                endpoint,
                body: got_body,
            } => {
                assert_eq!(*got_status, *status, "status rides verbatim: {err}");
                assert_eq!(
                    got_body.as_str(),
                    body.as_str(),
                    "body rides VERBATIM: {err}"
                );
                assert_eq!(
                    endpoint.as_str(),
                    format!("{}{}", mock.uri(), path),
                    "endpoint = the full request URL"
                );
            }
            other => panic!("HTTP {status} must classify GatewayClientError, got: {other}"),
        }
        assert_eq!(err.exit_code(), 2, "usage class: {err}");
        assert_eq!(err.code(), "gateway_client_error", "slug: {err}");
    }
}

/// PARTITION, preserved-arms half: the earlier classifier arms keep
/// their meanings ON THE API-CALL PATH — auth stays exit 5, target
/// state stays exit 6, and a 500 stays internal/exit-1 (a server error
/// is never the caller's usage problem).
#[tokio::test]
async fn classified_arms_keep_their_meanings_on_the_api_path() {
    let mock = IgnitionMock::start().await;
    let api = api_for(&mock);

    // 401 → Auth (exit 5).
    let path = api_path("unauthorized");
    mount_exact(&mock, "GET", &path, 401, r#"{"error":"Unauthorized"}"#).await;
    let err = api_call_error(&mock, &api, &path).await;
    assert_eq!(err.exit_code(), 5, "401 stays auth: {err}");
    assert_eq!(err.code(), "auth_rejected");

    // 403 → Auth (exit 5) — a NON-EAM path, so the generic arm applies.
    let path = api_path("forbidden");
    mount_exact(&mock, "GET", &path, 403, r#"{"error":"Forbidden"}"#).await;
    let err = api_call_error(&mock, &api, &path).await;
    assert_eq!(err.exit_code(), 5, "403 stays auth: {err}");
    assert_eq!(err.code(), "auth_rejected");

    // 404 → NotFound (exit 6).
    let path = api_path("missing");
    mount_exact(&mock, "GET", &path, 404, r#"{"message":"No route match"}"#).await;
    let err = api_call_error(&mock, &api, &path).await;
    assert_eq!(err.exit_code(), 6, "404 stays target state: {err}");
    assert_eq!(err.code(), "not_found");

    // 503 → GatewayRestarting (exit 6).
    let path = api_path("restarting");
    mount_exact(&mock, "GET", &path, 503, r#"{}"#).await;
    let err = api_call_error(&mock, &api, &path).await;
    assert_eq!(err.exit_code(), 6, "503 stays restarting: {err}");
    assert_eq!(err.code(), "gateway_restarting");

    // 500 → Internal (exit 1): a SERVER error is never the caller's
    // usage problem — the catch-all is 4xx-scoped.
    let path = api_path("server-error");
    mount_exact(&mock, "GET", &path, 500, r#"{"error":"boom"}"#).await;
    let err = api_call_error(&mock, &api, &path).await;
    assert_eq!(err.exit_code(), 1, "500 stays internal: {err}");
    assert_eq!(err.code(), "internal");
}

/// The api-call contract carries the RAW body: a Jetty HTML 400 page
/// rides verbatim in `body` — the HTML-sniffed-message behavior stays
/// curated-path-only (the Internal enrichment), the api path never
/// rewrites the gateway's answer.
#[tokio::test]
async fn jetty_html_400_rides_verbatim_on_the_api_path() {
    let mock = IgnitionMock::start().await;
    let api = api_for(&mock);

    let path = api_path("html-400");
    mock.html_error("GET", &path, 400).await;
    let err = api_call_error(&mock, &api, &path).await;
    match &err {
        CoreError::GatewayClientError { body, .. } => {
            assert_eq!(
                body.as_str(),
                jetty_error_html(400, &path),
                "the RAW HTML page rides verbatim — no sniffing on the api path"
            );
        }
        other => panic!("HTML 400 must classify GatewayClientError, got: {other}"),
    }
    assert_eq!(err.exit_code(), 2);
}

/// An over-cap 400 body is truncated to the cap with the explicit
/// marker appended — the verbatim prefix is preserved, and the total
/// size never exceeds cap + marker (the envelope cannot be flooded by
/// a pathological gateway page).
#[tokio::test]
async fn oversized_api_body_is_truncated_with_marker() {
    let mock = IgnitionMock::start().await;
    let api = api_for(&mock);

    let mounted = "y".repeat(GATEWAY_CLIENT_BODY_CAP_BYTES + 500);
    let path = api_path("oversized");
    mount_exact(&mock, "GET", &path, 400, &mounted).await;
    let err = api_call_error(&mock, &api, &path).await;
    match &err {
        CoreError::GatewayClientError { body, .. } => {
            assert_eq!(
                body.as_str(),
                truncate_api_body(&mounted),
                "the carried body IS the capped construction"
            );
            assert!(body.ends_with(GATEWAY_CLIENT_BODY_TRUNCATION_MARKER));
            assert!(
                body.as_str().len()
                    <= GATEWAY_CLIENT_BODY_CAP_BYTES + GATEWAY_CLIENT_BODY_TRUNCATION_MARKER.len()
            );
            assert_eq!(
                &body.as_str()[..GATEWAY_CLIENT_BODY_CAP_BYTES],
                &mounted[..GATEWAY_CLIENT_BODY_CAP_BYTES],
                "the verbatim prefix is preserved"
            );
        }
        other => panic!("oversized 400 must classify GatewayClientError, got: {other}"),
    }
}

/// NON-LEAK REGRESSION (research Pitfall 1): the same shape of 4xx
/// through the CURATED pipeline (a real trait method on get_json)
/// still yields `CoreError::Internal` — the catch-all cannot fire
/// without the `api_call` parameter, so v1.0-era commands' exit-1
/// semantics are byte-for-byte unchanged.
#[tokio::test]
async fn curated_pipeline_keeps_internal_on_unclassified_400() {
    let mock = IgnitionMock::start().await;
    let api = api_for(&mock);

    mock.status_json(
        "GET",
        GATEWAY_INFO_PATH,
        400,
        serde_json::json!({"error": {"code": "BAD_REQUEST"}}),
    )
    .await;
    let err = api.gateway_info().await.expect_err("400 must error");
    assert_eq!(err.code(), "internal", "curated 400 keeps exit-1: {err}");
    assert_eq!(err.exit_code(), 1, "curated 400 stays internal: {err}");
    let message = err.to_string();
    assert!(
        message.contains("unexpected HTTP 400"),
        "the curated Internal enrichment is untouched: {message}"
    );
}
