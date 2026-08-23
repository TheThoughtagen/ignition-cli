//! Wiremock contract tests for the trial capability (04-03, RIG-02/03):
//! the live-captured trial/banners bodies driven through the client
//! pipeline, the conditional-auth proof (header-less client succeeds —
//! the endpoints are unauth-verified live on BOTH minor versions; a
//! credentialed client rides its token along harmlessly), and the
//! tier-0 reset POST's exact request shape (empty body, authed).
//!
//! Fixtures are the EXACT bodies captured live during this plan's
//! spike (2026-08-22): expired state from ign-research 8.3.6, active
//! state from the ignition-devops rig 8.3.3. The login-flow
//! request-sequence proofs (tier 1) live here too — added by Task 2.

mod common;

use common::IgnitionMock;
use ignition_core::client::trial::{BannerSet, TrialWire};
use ignition_core::client::{GatewayApi, ReqwestGatewayApi};
use ignition_core::config::{Credential, Secret};

const TRIAL_PATH: &str = "/data/api/v1/trial";
const BANNERS_PATH: &str = "/data/api/v1/overview/banners";

/// THE live expired capture (ign-research 8.3.6, unauthenticated) —
/// parsed end-to-end through the pipeline by a HEADER-LESS client,
/// with the recorded requests carrying NO auth header (the
/// StatusPing-precedent proof: a fresh rig with no token reads its
/// trial state fine).
#[tokio::test]
async fn trial_and_banners_fetch_header_less() {
    let mock = IgnitionMock::start().await;
    let trial_guard = wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(TRIAL_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
            serde_json::json!({
                "licenseMode": "Trial", "trialState": "AllInDemo",
                "trialSecondsLeft": 0, "expired": true,
                "emergency": false, "emergencySecondsLeft": 0,
                "development": false, "developmentSecondsLeft": 0
            }),
        ))
        .expect(1)
        .mount_as_scoped(&mock.server)
        .await;
    let banners_guard = wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(BANNERS_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
            serde_json::json!({
                "banners": [{
                    "order": 0, "type": "trial",
                    "data": { "severity": "warning", "expireTime": null,
                              "toolTips": [], "actions": [] }
                }]
            }),
        ))
        .expect(1)
        .mount_as_scoped(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let trial: TrialWire = api.trial_status_wire().await.expect("unauth works (live fact)");
    assert!(trial.expired);
    assert_eq!(trial.trial_seconds_left, 0);
    let banners: BannerSet = api.banners().await.expect("unauth works (live fact)");
    assert_eq!(banners.banners.len(), 1);

    for (guard, name) in [(trial_guard, "trial"), (banners_guard, "banners")] {
        let requests = guard.received_requests().await;
        assert_eq!(requests.len(), 1, "exactly one {name} fetch");
        let headers = format!("{:?}", requests[0].headers).to_lowercase();
        assert!(
            !headers.contains("x-ignition-api-token") && !headers.contains("authorization"),
            "header-less client sends NO auth header: {headers}"
        );
    }
}

/// The conditional-auth other half: a client WITH a token credential
/// rides it along on both fetches (harmless — live-verified the
/// endpoints answer either way; future-proof if a gateway starts
/// gating them).
#[tokio::test]
async fn credential_rides_along_on_trial_fetches() {
    let mock = IgnitionMock::start().await;
    for path in [TRIAL_PATH, BANNERS_PATH] {
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(path))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
                serde_json::json!({
                    "licenseMode": "Trial", "trialState": "AllInDemo",
                    "trialSecondsLeft": 6590, "expired": false,
                    "emergency": false, "emergencySecondsLeft": 0,
                    "development": false, "developmentSecondsLeft": 0
                }),
            ))
            .expect(1)
            .mount(&mock.server)
            .await;
    }

    let credential = Credential::Token(Secret::new("spike:tokengeneratedlive"));
    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(credential));
    api.trial_status_wire().await.expect("credentialed fetch works");
    api.banners().await.expect("banners parse under the token too (unused fields)");
}

/// The tier-0 reset POST's exact request shape: EMPTY body, authed
/// (the decompiled UI mutation's shape — `{method:"POST",
/// url:"/data/api/v1/trial"}`). The 2xx body IS the fresh TrialWire
/// (live-observed on the 8.3.3 rig: expired true → false, 7199s).
#[tokio::test]
async fn trial_reset_tier0_posts_empty_body_with_token() {
    let mock = IgnitionMock::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(TRIAL_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
            serde_json::json!({
                "licenseMode": "Trial", "trialState": "AllInDemo",
                "trialSecondsLeft": 7199, "expired": false,
                "emergency": false, "emergencySecondsLeft": 0,
                "development": false, "developmentSecondsLeft": 0
            }),
        ))
        .expect(1)
        .mount_as_scoped(&mock.server)
        .await;

    let credential = Credential::Token(Secret::new("spike:tokengeneratedlive"));
    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(credential));
    let fresh = api.trial_reset_wire().await.expect("200 parses as the fresh trial");
    assert!(!fresh.expired, "the reset response IS the fresh state");
    assert_eq!(fresh.trial_seconds_left, 7199);

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0].body.is_empty(),
        "the POST carries NO body — the UI mutation's exact shape"
    );
    let headers = format!("{:?}", requests[0].headers).to_lowercase();
    assert!(
        headers.contains("x-ignition-api-token"),
        "tier 0 is the TOKEN rung: {headers}"
    );
}

/// A 403 reset answer classifies Auth (the live-discovered state gate
/// on non-expired trials surfaces through the standard classifier;
/// the action layer's expiry pre-check exists to keep this from
/// masquerading as the common case).
#[tokio::test]
async fn trial_reset_403_classifies_auth() {
    let mock = IgnitionMock::start().await;
    mock.html_error("POST", TRIAL_PATH, 403).await;
    let credential = Credential::Token(Secret::new("spike:tokengeneratedlive"));
    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(credential));
    let err = api
        .trial_reset_wire()
        .await
        .expect_err("403 classifies Auth");
    assert_eq!(err.exit_code(), 5);
}
