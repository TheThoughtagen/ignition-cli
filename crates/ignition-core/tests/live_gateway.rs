//! Opt-in live-gateway suite — `#[ignore]`-gated because it needs a real
//! commissioned Ignition 8.3+ gateway. NOT run by CI and not required for
//! plan execution (wiremock covers the contract); run it to close the
//! live-auth gap empirically on a gateway you control:
//!
//! ```text
//! cargo test -p ignition-core --test live_gateway -- --ignored
//! ```
//!
//! Skip behavior: each test reads its env vars at start and returns
//! quietly when they are absent — `-- --ignored` with no envs set is a
//! green no-op, never a failure.
//!
//! ## Rig recipe (verified end-to-end, 02-RESEARCH §Test architecture)
//!
//! ```bash
//! docker run -d --name ign-research -p 18088:8088 \
//!   -e ACCEPT_IGNITION_EULA=Y inductiveautomation/ignition:8.3.6
//! ```
//!
//! 1. Commission via `http://localhost:18088/welcome` (browser): pick
//!    "Ignition" standard → trial mode, create the admin user, Finish
//!    Setup → Start Gateway.
//! 2. UI: Platform → Security → API Keys → Create: **Basic Token**, name
//!    it, **UNCHECK "Require secure connections"** (http rig!), pick a
//!    security level with admin.
//! 3. Copy the FULL `name:key` string the dialog shows — both halves.
//!
//! ## Environment
//!
//! | var | required by | meaning |
//! |---|---|---|
//! | `IGNITION_LIVE_URL` | every test | base URL, e.g. `http://localhost:18088` |
//! | `IGNITION_LIVE_TOKEN` | auth tests | full `name:key` API-token string |
//! | `IGNITION_LIVE_USER` / `IGNITION_LIVE_PASSWORD` | Basic-rejection test | a VALID commissioned user |
//!
//! Later Phase-2 plans APPEND their live checks to this file.

use ignition_core::client::version::below_minimum;
use ignition_core::client::{GatewayApi, ReqwestGatewayApi};
use ignition_core::config::{Credential, Secret};
use ignition_core::error::CoreError;

/// Non-empty `IGNITION_LIVE_URL`, when set.
fn live_url() -> Option<String> {
    std::env::var("IGNITION_LIVE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// Non-empty `IGNITION_LIVE_TOKEN`, when set.
fn live_token() -> Option<String> {
    std::env::var("IGNITION_LIVE_TOKEN")
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn skip(message: &str) {
    eprintln!("skipping: {message}");
}

/// THE live regression for the 02-01 `ignitionVersion` fix: a real 8.3
/// gateway's gateway-info body must deserialize (`ignition_version`
/// non-empty, at/above the 8.3.1 minimum). Requires the token — a
/// commissioned gateway answers header-less gateway-info with 401
/// (verified live 2026-08-21; the 83-api collection's `auth: none` tag
/// does not hold under 8.3 default security).
#[tokio::test]
#[ignore = "opt-in: set IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN to run against a live gateway"]
async fn live_gateway_info_parses() {
    let (Some(url), Some(token)) = (live_url(), live_token()) else {
        skip("IGNITION_LIVE_URL / IGNITION_LIVE_TOKEN not both set");
        return;
    };
    let api = ReqwestGatewayApi::for_tests(&url, Some(Credential::Token(Secret::new(token))));

    let info = api
        .gateway_info()
        .await
        .expect("live gateway-info must deserialize into the corrected GatewayInfo");
    assert!(
        !info.ignition_version.is_empty(),
        "ignitionVersion non-empty: {:?}",
        info.ignition_version
    );
    assert!(
        !below_minimum(&info.ignition_version),
        "live rig below the supported minimum: {}",
        info.ignition_version
    );
}

/// Closes STATE.md's flagged live-auth gap with an executable proof: the
/// `X-Ignition-API-Token: name:key` header authenticates `/data` routes
/// on a real 8.3 gateway (key-only would 401 — the format IS the
/// contract).
#[tokio::test]
#[ignore = "opt-in: set IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN (full name:key string)"]
async fn live_token_auth_works() {
    let (Some(url), Some(token)) = (live_url(), live_token()) else {
        skip("IGNITION_LIVE_URL / IGNITION_LIVE_TOKEN not both set");
        return;
    };
    let api = ReqwestGatewayApi::for_tests(&url, Some(Credential::Token(Secret::new(token))));

    let info = api
        .gateway_info()
        .await
        .expect("token name:key must authenticate 8.3 /data routes (200, not 401/403)");
    assert!(!info.ignition_version.is_empty());
}

/// Documents the verified reality: valid Basic credentials are REJECTED
/// (401) on 8.3 `/data` routes — the enum arm exists for future/legacy
/// surfaces only, and the client warns loudly on every use.
#[tokio::test]
#[ignore = "opt-in: set IGNITION_LIVE_URL + IGNITION_LIVE_USER + IGNITION_LIVE_PASSWORD"]
async fn live_basic_is_rejected() {
    let Some(url) = live_url() else {
        skip("IGNITION_LIVE_URL not set");
        return;
    };
    let (Some(user), Some(password)) = (
        std::env::var("IGNITION_LIVE_USER").ok(),
        std::env::var("IGNITION_LIVE_PASSWORD").ok(),
    ) else {
        skip("IGNITION_LIVE_USER / IGNITION_LIVE_PASSWORD not both set");
        return;
    };
    let api = ReqwestGatewayApi::for_tests(
        &url,
        Some(Credential::Basic(Secret::new(user), Secret::new(password))),
    );

    let err = api
        .gateway_info()
        .await
        .expect_err("Basic auth must be rejected on 8.3 /data routes");
    assert!(
        matches!(err, CoreError::Auth { status: 401, .. }),
        "expected a 401 Auth rejection, got: {err}"
    );
    assert_eq!(err.exit_code(), 5);
    assert!(
        err.hint().expect("hint").contains("API token"),
        "hint steers toward an API token: {:?}",
        err.hint()
    );
}

// ---------------------------------------------------------------------------
// 02-02 inspection additions: read-only live checks (skip gracefully when
// no rig is configured).
// ---------------------------------------------------------------------------

/// `/StatusPing` needs NO token (the unauthenticated readiness anchor):
/// a URL alone must yield a state string — this is the same primitive
/// `ign wait` (02-05) will poll.
#[tokio::test]
#[ignore = "opt-in: set IGNITION_LIVE_URL (no token needed — StatusPing is unauthenticated)"]
async fn live_status_ping_unauthenticated() {
    let Some(url) = live_url() else {
        skip("IGNITION_LIVE_URL not set");
        return;
    };
    let api = ReqwestGatewayApi::for_tests(&url, None);
    let ping = api
        .status_ping()
        .await
        .expect("StatusPing answers without any credential");
    assert!(
        !ping.state.is_empty(),
        "state surfaces verbatim: {:?}",
        ping.state
    );
}

/// The three authed inspection reads against a live gateway: overview
/// parses (uptime ms ≥ 0, cpu a 0–1 fraction), healthy modules are
/// non-empty (the standard image ships dozens), current gauges parse
/// (cpu percent ≥ 0).
#[tokio::test]
#[ignore = "opt-in: set IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN"]
async fn live_inspection_endpoints_parse() {
    let (Some(url), Some(token)) = (live_url(), live_token()) else {
        skip("IGNITION_LIVE_URL / IGNITION_LIVE_TOKEN not both set");
        return;
    };
    let api = ReqwestGatewayApi::for_tests(&url, Some(Credential::Token(Secret::new(token))));

    let overview = api
        .overview()
        .await
        .expect("live overview must deserialize");
    assert!(overview.uptime >= 0, "uptime ms: {}", overview.uptime);
    assert!(
        (0.0..=1.0).contains(&overview.cpu),
        "cpu is a 0–1 fraction: {}",
        overview.cpu
    );

    let modules = api
        .modules(false, &Default::default())
        .await
        .expect("live modules/healthy must deserialize");
    assert!(
        !modules.items.is_empty(),
        "a standard gateway ships healthy modules"
    );
    assert!(modules.items.iter().all(|m| m.state.as_deref() != Some("")));
    let active = modules
        .items
        .iter()
        .filter(|m| m.state.as_deref() == Some("ACTIVE"))
        .count();
    assert!(active > 0, "{active} modules are ACTIVE");

    let gauges = api
        .metrics_current()
        .await
        .expect("live currentGauges must deserialize");
    assert!(gauges.cpu >= 0.0, "cpu percent: {}", gauges.cpu);
}

// ---------------------------------------------------------------------------
// 02-03 additions: sessions + connections live checks (read-only; empty
// is the expected state on a fresh rig — the checks exist to run against
// a gateway WITH sessions/connections, capturing the populated shapes).
// ---------------------------------------------------------------------------

/// The three session-family lists against a live gateway: all must
/// answer the standard envelope (items may be empty on a headless rig —
/// a connected Designer/Perspective session makes them non-empty).
#[tokio::test]
#[ignore = "opt-in: set IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN"]
async fn live_session_families_parse() {
    let (Some(url), Some(token)) = (live_url(), live_token()) else {
        skip("IGNITION_LIVE_URL / IGNITION_LIVE_TOKEN not both set");
        return;
    };
    let api = ReqwestGatewayApi::for_tests(&url, Some(Credential::Token(Secret::new(token))));

    let designers = api
        .designers(&Default::default())
        .await
        .expect("live designers list must deserialize");
    let sessions = api
        .perspective_sessions(&Default::default())
        .await
        .expect("live perspective list must deserialize (trailing-slash path)");
    let clients = api
        .vision_clients(&Default::default())
        .await
        .expect("live vision list must deserialize");
    eprintln!(
        "live sessions: {} designers, {} perspective, {} vision",
        designers.items.len(),
        sessions.items.len(),
        clients.items.len()
    );
}

/// HLTH-05/06 verification step (research Open Question 1): list the
/// connection resource families against a live gateway. EMPTY is fine
/// (the research rig had zero); when a gateway HAS connections, dump the
/// populated `healthchecks` shapes so the passthrough can be upgraded to
/// a typed model — until then the shape stays LOW-confidence.
#[tokio::test]
#[ignore = "opt-in: set IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN (empty lists OK)"]
async fn live_connections() {
    let (Some(url), Some(token)) = (live_url(), live_token()) else {
        skip("IGNITION_LIVE_URL / IGNITION_LIVE_TOKEN not both set");
        return;
    };
    let api = ReqwestGatewayApi::for_tests(&url, Some(Credential::Token(Secret::new(token))));

    let database = api
        .database_connections()
        .await
        .expect("live database-connection resource list must deserialize");
    let opc = api
        .opc_connections()
        .await
        .expect("live opc-connection resource list must deserialize");
    for connection in database.items.iter().chain(opc.items.iter()) {
        // Capture hook: the FIRST gateway with a configured connection
        // prints the populated healthchecks shape here.
        eprintln!(
            "live connection {:?} enabled={} healthchecks={}",
            connection.name, connection.enabled, connection.healthchecks
        );
    }
}
