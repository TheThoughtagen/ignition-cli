//! Wiremock contract tests for the [`Session`] seam (08-02): the
//! `for_url` rig constructor and the Deref reach into the free-fn
//! actions. These two need NO config/env, so they live here as an
//! integration test binary — the env-dependent precedence and
//! secret-chain proofs live as unit tests inside `src/session.rs`,
//! where the crate's `#[cfg(test)] ENV_LOCK` is reachable.
//!
//! The `ssl_verify` propagation path (`for_url` → anonymous `Profile` →
//! `ReqwestGatewayApi::new` → `build_client(false)` →
//! `danger_accept_invalid_certs(true)`) is exercised here over plain
//! http (the flag is accepted and the client works); the WIRE-level
//! https-skip proof needs wiremock's optional `tls` feature, which this
//! workspace does not enable — the builder behavior itself is pinned by
//! the client's own tests.

mod common;

use common::IgnitionMock;
use ignition_core::actions::version::version;
use ignition_core::client::GatewayApi;
use ignition_core::config::{Credential, Secret};
use ignition_core::session::Session;

/// The gateway-info JSON body — the one field every 8.3 gateway
/// answers with (`ignitionVersion`, the `version` alias tolerated).
fn info_body() -> serde_json::Value {
    serde_json::json!({ "ignitionVersion": "8.3.6 (b2026042713)" })
}

/// Lowercased Debug dump of a recorded request's headers — the
/// status_contract.rs presence/absence assertion pattern.
fn headers_debug(request: &wiremock::Request) -> String {
    format!("{:?}", request.headers).to_lowercase()
}

/// A scoped gateway-info mock on the harness's server.
async fn mount_info(mock: &IgnitionMock, expected: u64) -> wiremock::MockGuard {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/gateway-info"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(info_body()))
        .expect(expected)
        .mount_as_scoped(&mock.server)
        .await
}

/// for_url: the rig client carries the GIVEN url + credential (the
/// token rides `X-Ignition-API-Token`), and default auth (no secret —
/// the fresh-rig condition) is tolerated header-less. Rig sessions
/// carry no profile name.
#[tokio::test]
async fn for_url_carries_url_credential_and_tolerates_none() {
    let mock = IgnitionMock::start().await;
    let guard = mount_info(&mock, 2).await;

    // Token-bearing rig client (the trial-reset/snapshot rung).
    let url: url::Url = mock.uri().parse().expect("mock uri parses");
    let session = Session::for_url(
        url,
        Some(Credential::Token(Secret::new("rig-token"))),
        false,
    )
    .expect("for_url builds");
    assert_eq!(session.profile_name(), "", "rig sessions carry no profile");
    let info = session.gateway_info().await.expect("token rig answers");
    assert_eq!(info.ignition_version, "8.3.6 (b2026042713)");

    // Default auth: no credential at all — the fresh-rig condition —
    // still reaches the endpoint, header-less.
    let url: url::Url = mock.uri().parse().expect("mock uri parses");
    let session = Session::for_url(url, None, false).expect("for_url builds");
    let info = session
        .gateway_info()
        .await
        .expect("headerless rig answers");
    assert_eq!(info.ignition_version, "8.3.6 (b2026042713)");

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 2, "both rig requests arrived");
    let headers = headers_debug(&requests[0]);
    assert!(
        headers.contains("x-ignition-api-token"),
        "the explicit credential must ride the token header: {headers}"
    );
    assert!(
        !headers.contains("authorization"),
        "token credential never doubles as basic: {headers}"
    );
    let headers = headers_debug(&requests[1]);
    assert!(
        !headers.contains("x-ignition-api-token") && !headers.contains("authorization"),
        "no-secret rig client must be header-less: {headers}"
    );
}

/// api() / Deref / api_handle() all reach the existing free-fn actions
/// over `&GatewayApi` UNCHANGED — `version(Some(&*session), …)` is the
/// dyn precedent working through the concrete-with-deref handle: the
/// bare deref, the borrowed accessor, and the Arc handle are
/// interchangeable at every existing call shape.
#[tokio::test]
async fn session_handle_feeds_free_fn_actions_unchanged() {
    let mock = IgnitionMock::start().await;
    let guard = mount_info(&mock, 3).await;

    let url: url::Url = mock.uri().parse().expect("mock uri parses");
    let session = Session::for_url(
        url,
        Some(Credential::Token(Secret::new("rig-token"))),
        false,
    )
    .expect("for_url builds");

    // 1. The bare Deref — exactly the call shape a migrated dispatch
    //    site writes (`version(Some(&*session), …)`).
    let result = version(Some(&*session), "test-version")
        .await
        .expect("version");
    assert_eq!(result.cli_version, "test-version");
    let info = result.gateway.expect("gateway info reached the action");
    assert_eq!(info.ignition_version, "8.3.6 (b2026042713)");

    // 2. The borrowed accessor.
    let result = version(Some(session.api()), "test-version")
        .await
        .expect("version");
    assert!(result.gateway.is_some());

    // 3. The Arc handle (the TUI workers/ClientHandle shape).
    let handle = session.api_handle();
    let result = version(Some(&*handle), "test-version")
        .await
        .expect("version");
    assert!(result.gateway.is_some());

    assert_eq!(
        guard.received_requests().await.len(),
        3,
        "every handle shape reached the wire unchanged"
    );
}
