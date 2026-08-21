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
