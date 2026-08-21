//! Wiremock contract tests for the inspection capabilities (02-02):
//! overview / StatusPing / modules here, systemPerformance metrics
//! appended by Task 2. Fixtures are the EXACT live-captured bodies
//! (02-RESEARCH §Status/info, §Modules) driven through the IgnitionMock
//! harness, so path/shape drift fails loudly.
//!
//! The crown jewel is `status_ping_sends_no_auth_headers`: the client is
//! constructed WITH a token credential, yet the recorded `/StatusPing`
//! request carries NO auth header at all — the header-absence proof that
//! the readiness anchor survives broken/absent credentials (the 01-04
//! recorded-request pattern).

mod common;

use common::IgnitionMock;
use ignition_core::client::status::{ModuleInfo, Overview, StatusPing};
use ignition_core::client::{GatewayApi, ReqwestGatewayApi};
use ignition_core::config::{Credential, Secret};
use ignition_core::error::CoreError;

const OVERVIEW_PATH: &str = "/data/api/v1/overview";
const STATUS_PING_PATH: &str = "/StatusPing";
const MODULES_HEALTHY_PATH: &str = "/data/api/v1/modules/healthy";

/// Lowercased Debug dump of a recorded request's headers — crude but
/// API-stable presence/absence assertions (01-04 pattern).
fn headers_debug(request: &wiremock::Request) -> String {
    format!("{:?}", request.headers).to_lowercase()
}

/// The exact live-captured overview body of the research rig
/// (02-RESEARCH §Status/info) — parsed end-to-end through the pipeline.
#[tokio::test]
async fn overview_parses_the_live_capture() {
    let mock = IgnitionMock::start().await;
    mock.list_json("GET", OVERVIEW_PATH, serde_json::json!({
        "version": "8.3.6 (b2026042713)",
        "redundancy": {"role": "Independent", "activityLevel": "ACTIVE", "projectState": "RUNNING"},
        "java": {"version": "17.0.11", "vendor": "Azul Systems, Inc.", "name": "OpenJDK 64-Bit Server VM"},
        "os": {"name": "Linux", "arch": "amd64", "version": "5.15.0"},
        "cloudEnv": "unknown",
        "uptime": 338137,
        "timezone": "America/New_York",
        "locale": "en-US",
        "time": 1787346747022i64,
        "memory": [338137088i64, 1073741824i64],
        "cpu": 0.0031,
        "disk": {"total": 62661259264i64, "used": 12272824320i64},
        "license": {"state": "trial", "trialRemaining": 7017}
    }))
    .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let overview: Overview = api.overview().await.expect("live shape must parse");
    assert_eq!(overview.version, "8.3.6 (b2026042713)");
    assert_eq!(overview.uptime, 338137, "uptime in epoch ms");
    assert!(
        (overview.cpu - 0.0031).abs() < f64::EPSILON,
        "cpu 0–1 fraction"
    );
    assert_eq!(
        overview
            .license
            .as_ref()
            .expect("license block")
            .trial_remaining_s,
        Some(7017),
        "trial countdown in seconds"
    );
    assert_eq!(
        overview.java.as_ref().expect("java").vendor,
        "Azul Systems, Inc."
    );
    assert_eq!(overview.disk.as_ref().expect("disk").used, 12272824320i64);
}

/// HTML 401 on overview (the Jetty page every `/data/api/v1/*` failure
/// answers with) → Auth (exit 5) — overview is an authed read.
#[tokio::test]
async fn overview_html_401_classifies_auth() {
    let mock = IgnitionMock::start().await;
    mock.html_error("GET", OVERVIEW_PATH, 401).await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let err = api.overview().await.expect_err("401 must fail");
    assert!(
        matches!(&err, CoreError::Auth { status: 401, .. }),
        "wrong class: {err}"
    );
    assert_eq!(err.exit_code(), 5);
    assert_eq!(err.code(), "auth_rejected");
}

/// THE header-absence proof: the client HOLDS a token credential, yet
/// the recorded `/StatusPing` request carries NO auth header at all —
/// auth=false through the pipeline (the unauthenticated readiness
/// anchor; works with broken credentials and mid-restart).
#[tokio::test]
async fn status_ping_sends_no_auth_headers() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(STATUS_PING_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"state": "RUNNING"})),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    // A credential IS configured — the ping must STILL go out header-less.
    let api = ReqwestGatewayApi::for_tests(
        &server.uri(),
        Some(Credential::Token(Secret::new("name:key"))),
    );
    let ping: StatusPing = api.status_ping().await.expect("ping parses");
    assert_eq!(ping.state, "RUNNING");

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1, "exactly one request");
    let headers = headers_debug(&requests[0]);
    assert!(
        !headers.contains("x-ignition-api-token"),
        "status_ping must NOT send the token header: {headers}"
    );
    assert!(
        !headers.contains("authorization"),
        "status_ping must NOT send any Authorization header: {headers}"
    );
}

/// StatusPing STARTING (the mid-restart state, verified lifecycle)
/// surfaces verbatim — unknown states are stringed, never guessed.
#[tokio::test]
async fn status_ping_starting_surfaces_verbatim() {
    let mock = IgnitionMock::start().await;
    mock.status_json(
        "GET",
        STATUS_PING_PATH,
        200,
        serde_json::json!({"state": "STARTING"}),
    )
    .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let ping = api.status_ping().await.expect("STARTING parses");
    assert_eq!(ping.state, "STARTING");
}

/// The healthy-modules list parses the live capture (incl.
/// `state: "ACTIVE"`, `licenseState`, passthrough `onStartup`), and the
/// pipeline passes the UI's `limit=-1`/`offset=0` convention — the
/// matchers only fire when those exact params are present, so a missing
/// param fails the test (mock drop verifies `expect(1)`).
#[tokio::test]
async fn modules_parses_live_capture_and_passes_limit_minus_one() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(MODULES_HEALTHY_PATH))
        .and(wiremock::matchers::query_param("limit", "-1"))
        .and(wiremock::matchers::query_param("offset", "0"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {
                        "id": "com.inductiveautomation.perspective",
                        "name": "Perspective",
                        "version": "8.3.6",
                        "state": "ACTIVE",
                        "licenseState": "ACTIVATED",
                        "vendorName": "Inductive Automation",
                        "startupTime": "2026-08-21 22:03:29",
                        "onStartup": "ENABLE",
                        "shouldUpgrade": false
                    }
                ],
                "metadata": {"total": 1, "matching": 1, "limit": -1, "offset": 0}
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let page = api
        .modules(false, &Default::default())
        .await
        .expect("healthy list parses");
    assert_eq!(page.items.len(), 1);
    let module: &ModuleInfo = &page.items[0];
    assert_eq!(module.id, "com.inductiveautomation.perspective");
    assert_eq!(module.state.as_deref(), Some("ACTIVE"));
    assert_eq!(module.license_state.as_deref(), Some("ACTIVATED"));
    assert_eq!(
        module.extra.get("onStartup"),
        Some(&serde_json::json!("ENABLE")),
        "unknown module keys round-trip"
    );
    assert_eq!(page.metadata.total, 1);
}
