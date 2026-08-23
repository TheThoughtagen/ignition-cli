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
use ignition_core::error::CoreError;

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

// -------------------------------------------------------------------------
// Tier 1 — the native OIDC login flow (client/idp.rs): the full
// happy-path dance as a scripted mock sequence, asserted on the
// REQUESTS (the Phase-2/3 discipline): token threading (each
// next-challenge body carries the PREVIOUS answer's token), cookie
// replay, the CSRF header + session cookie on the final POST, and the
// password confined to exactly ONE request body (redaction proof).
// -------------------------------------------------------------------------

use ignition_core::client::idp::{IdpLoginFlow, GatewaySession, login, trial_reset_via_session};

/// The scripted fixture tokens/cookies (invented values; the SHAPES
/// are the live-captured ones).
const RELAY_COOKIE: &str = "idp-relay-1766878194=relay-value";
const SID_COOKIE: &str = "idp-sid-default-1766878194=sid-value";
const T0: &str = "T0-from-login-redirect";
const T1: &str = "T1-after-first-challenge";
const T2: &str = "T2-after-credentials";
const T3: &str = "T3-complete";
const SESSION_COOKIE_NAME: &str = "webui-sid-1766878194";
const SESSION_COOKIE_VALUE: &str = "session-value";
const CSRF: &str = "csrf-value-from-session-endpoint";

/// The OIDC params step 1 hands out (step 6 must replay them +token).
const OIDC_QUERY: &str = "app=gateway&response_type=code&client_id=ignition&redirect_uri=%2Fdata%2Ffederate%2Fcallback%2Finternal&scope=openid&state=st&nonce=nc&prompt=login&max_age=1";

/// Mount the entire 9-request login + reset dance; returns the guards
/// for the requests the test asserts on.
async fn mount_login_dance(server: &wiremock::MockServer) -> LoginGuards {
    // 1. GET /data/app/login → 302 into the OIDC flow (+ relay cookie).
    let g1 = wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/app/login"))
        .respond_with(
            wiremock::ResponseTemplate::new(302)
                .insert_header("Location", format!("/idp/default/oidc/auth?{OIDC_QUERY}"))
                .append_header("Set-Cookie", format!("{RELAY_COOKIE}; Path=/; HttpOnly")),
        )
        .expect(1)
        .mount_as_scoped(server)
        .await;
    // 2. GET oidc/auth → 302 to the login challenge (+ idp-sid cookie).
    //    `token` ABSENT is what distinguishes this hop from step 6's
    //    replay of the same URL (same query + token) — wiremock's
    //    stable insertion-order matching needs the negative here.
    let g2 = wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/idp/default/oidc/auth"))
        .and(wiremock::matchers::query_param("state", "st"))
        .and(wiremock::matchers::query_param_is_missing("token"))
        .respond_with(
            wiremock::ResponseTemplate::new(302)
                .insert_header(
                    "Location",
                    format!("/idp/default/authn/login?app=gateway&token={T0}&response_type=code&client_id=ignition"),
                )
                .append_header("Set-Cookie", format!("{SID_COOKIE}; Path=/idp/default; HttpOnly; SameSite=Strict")),
        )
        .expect(1)
        .mount_as_scoped(server)
        .await;
    // 3. POST next-challenge {"token": T0} → T1 (body-EXACT matcher =
    //    the threading proof for this hop).
    let g3 = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/idp/default/authn/next-challenge"))
        .and(wiremock::matchers::body_json(serde_json::json!({ "token": T0 })))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
            serde_json::json!({
                "complete": false,
                "nextChallenge": [{"type": "basic", "config": {}}],
                "rememberMe": false, "passwordExpired": false,
                "token": T1
            }),
        ))
        .expect(1)
        .mount_as_scoped(server)
        .await;
    // 4. POST submit-challenge/basic carrying T1 + the creds → T2.
    let g4 = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/idp/default/authn/submit-challenge/basic"))
        .and(wiremock::matchers::body_partial_json(serde_json::json!({
            "token": T1,
            "challenge": { "username": "admin" }
        })))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
            serde_json::json!({ "success": true, "token": T2 }),
        ))
        .expect(1)
        .mount_as_scoped(server)
        .await;
    // 5. POST next-challenge {"token": T2} → complete + T3.
    let g5 = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/idp/default/authn/next-challenge"))
        .and(wiremock::matchers::body_json(serde_json::json!({ "token": T2 })))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
            serde_json::json!({
                "complete": true,
                "rememberMe": false, "passwordExpired": false,
                "token": T3
            }),
        ))
        .expect(1)
        .mount_as_scoped(server)
        .await;
    // 6. GET oidc/auth?orig&token=T3 → the federate callback.
    let g6 = wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/idp/default/oidc/auth"))
        .and(wiremock::matchers::query_param("token", T3))
        .and(wiremock::matchers::query_param("state", "st"))
        .respond_with(wiremock::ResponseTemplate::new(302).insert_header(
            "Location",
            "/data/federate/callback/internal?code=auth-code&state=st",
        ))
        .expect(1)
        .mount_as_scoped(server)
        .await;
    // 7. GET the callback → /app + the webui-sid session cookie.
    let g7 = wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/federate/callback/internal"))
        .and(wiremock::matchers::query_param("code", "auth-code"))
        .respond_with(
            wiremock::ResponseTemplate::new(302)
                .insert_header("Location", "/app")
                .append_header(
                    "Set-Cookie",
                    format!("{SESSION_COOKIE_NAME}={SESSION_COOKIE_VALUE}; Path=/; HttpOnly; SameSite=Strict"),
                ),
        )
        .expect(1)
        .mount_as_scoped(server)
        .await;
    // 8. GET /data/app/session → the CSRF token (live-resolved field).
    //    The cookie rides as an assertion on the recorded request (the
    //    flow replays ALL captured cookies — not just the session one).
    let g8 = wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/app/session"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
            serde_json::json!({
                "userPayload": { "user": { "userName": "admin" } },
                "csrfToken": CSRF
            }),
        ))
        .expect(1)
        .mount_as_scoped(server)
        .await;
    // 9. POST /data/api/v1/trial (session cookie + X-CSRF-Token) →
    //    the fresh trial (the live-observed 2xx body).
    let g9 = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(TRIAL_PATH))
        .and(wiremock::matchers::header("X-CSRF-Token", CSRF))
        .and(wiremock::matchers::header(
            "Cookie",
            format!("{SESSION_COOKIE_NAME}={SESSION_COOKIE_VALUE}"),
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
            serde_json::json!({
                "licenseMode": "Trial", "trialState": "AllInDemo",
                "trialSecondsLeft": 7199, "expired": false,
                "emergency": false, "emergencySecondsLeft": 0,
                "development": false, "developmentSecondsLeft": 0
            }),
        ))
        .expect(1)
        .mount_as_scoped(server)
        .await;

    LoginGuards {
        login: g1,
        oidc_first: g2,
        challenge0: g3,
        submit: g4,
        challenge_complete: g5,
        oidc_token: g6,
        callback: g7,
        session: g8,
        reset_post: g9,
    }
}

#[allow(clippy::type_complexity)]
struct LoginGuards {
    login: wiremock::MockGuard,
    oidc_first: wiremock::MockGuard,
    challenge0: wiremock::MockGuard,
    submit: wiremock::MockGuard,
    challenge_complete: wiremock::MockGuard,
    oidc_token: wiremock::MockGuard,
    callback: wiremock::MockGuard,
    session: wiremock::MockGuard,
    reset_post: wiremock::MockGuard,
}

fn body_string(request: &wiremock::Request) -> String {
    String::from_utf8_lossy(&request.body).to_string()
}

fn headers_string(request: &wiremock::Request) -> String {
    format!("{:?}", request.headers).to_lowercase()
}

/// THE ladder sequence proof: the dance completes, and every wire
/// subtlety is asserted on the REQUESTS — token rotation threading,
/// cookie replay across steps, the CSRF header + session cookie on the
/// final POST, and the password appearing in exactly ONE body.
#[tokio::test]
async fn tier1_login_flow_pins_the_full_request_chain() {
    let server = wiremock::MockServer::start().await;
    let guards = mount_login_dance(&server).await;

    let flow = IdpLoginFlow::new(&server.uri()).expect("flow builds");
    let password = Secret::new("correct-horse-battery");
    let (flow, session): (IdpLoginFlow, GatewaySession) =
        login(flow, "admin", &password).await.expect("the dance completes");
    assert_eq!(session.cookie_name, SESSION_COOKIE_NAME);
    assert_eq!(session.csrf_token, CSRF);

    let fresh = trial_reset_via_session(&flow, &session)
        .await
        .expect("the reset POST answers the fresh trial");
    assert!(!fresh.expired);
    assert_eq!(fresh.trial_seconds_left, 7199);

    // Token threading: the exact bodies the two next-challenge hops
    // received (body_json matchers already pinned them; the guards
    // verify the hits recorded the same).
    let c0 = guards.challenge0.received_requests().await;
    assert_eq!(c0.len(), 1);
    assert_eq!(body_string(&c0[0]), r#"{"token":"T0-from-login-redirect"}"#);
    let c1 = guards.challenge_complete.received_requests().await;
    assert_eq!(c1.len(), 1);
    assert_eq!(body_string(&c1[0]), r#"{"token":"T2-after-credentials"}"#);

    // The session fetch carried the webui-sid cookie (asserted here —
    // the flow replays every captured cookie, the session one included).
    let session_requests = guards.session.received_requests().await;
    assert_eq!(session_requests.len(), 1);
    assert!(
        headers_string(&session_requests[0])
            .contains(&format!("{SESSION_COOKIE_NAME}={SESSION_COOKIE_VALUE}").to_lowercase()),
        "the session cookie rode the CSRF fetch: {}",
        headers_string(&session_requests[0])
    );

    // Cookie replay: step 4 carried BOTH captured cookies (relay +
    // idp-sid); the values are the ones steps 1–2 set.
    let submit_requests = guards.submit.received_requests().await;
    assert_eq!(submit_requests.len(), 1);
    let submit_headers = headers_string(&submit_requests[0]);
    assert!(
        submit_headers.contains(RELAY_COOKIE.to_lowercase().as_str()),
        "relay cookie replayed: {submit_headers}"
    );
    assert!(
        submit_headers.contains(SID_COOKIE.to_lowercase().as_str()),
        "idp-sid cookie replayed: {submit_headers}"
    );

    // Password redaction discipline: the password rides EXACTLY ONE
    // request body (step 4) and no headers anywhere.
    let all_guards: Vec<(&str, &wiremock::MockGuard)> = vec![
        ("login", &guards.login),
        ("oidc_first", &guards.oidc_first),
        ("challenge0", &guards.challenge0),
        ("submit", &guards.submit),
        ("challenge_complete", &guards.challenge_complete),
        ("oidc_token", &guards.oidc_token),
        ("callback", &guards.callback),
        ("session", &guards.session),
        ("reset_post", &guards.reset_post),
    ];
    for (name, guard) in all_guards {
        for request in guard.received_requests().await {
            let body = body_string(&request);
            let headers = headers_string(&request);
            if name == "submit" {
                assert!(
                    body.contains("correct-horse-battery"),
                    "the ONE password site: {body}"
                );
            } else {
                assert!(
                    !body.contains("correct-horse-battery")
                        && !headers.contains("correct-horse-battery"),
                    "password never appears in {name}"
                );
            }
        }
    }
}

/// Bad credentials (the live-observed 8.3.6 shape:
/// 200 `{"success":false,…}`) → Auth (exit 5) naming the challenge
/// endpoint — NOT a flow crash.
#[tokio::test]
async fn tier1_bad_credentials_classify_auth() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/app/login"))
        .respond_with(
            wiremock::ResponseTemplate::new(302)
                .insert_header("Location", format!("/idp/default/oidc/auth?{OIDC_QUERY}")),
        )
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/idp/default/oidc/auth"))
        .respond_with(
            wiremock::ResponseTemplate::new(302).insert_header(
                "Location",
                format!("/idp/default/authn/login?app=gateway&token={T0}"),
            ),
        )
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/idp/default/authn/next-challenge"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
            serde_json::json!({
                "complete": false,
                "nextChallenge": [{"type": "basic", "config": {}}],
                "token": T1
            }),
        ))
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/idp/default/authn/submit-challenge/basic"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
            serde_json::json!({ "success": false, "token": T2 }),
        ))
        .mount(&server)
        .await;

    let flow = IdpLoginFlow::new(&server.uri()).expect("flow builds");
    let err = match login(flow, "admin", &Secret::new("wrong")).await {
        Err(err) => err,
        Ok(_) => panic!("rejected credentials must error, not succeed"),
    };
    assert!(matches!(err, CoreError::Auth { .. }), "{err}");
    assert_eq!(err.exit_code(), 5);
}

/// A consumed-token replay (the Jetty-HTML 400 — research Pitfall 2)
/// surfaces as a flow failure carrying the extracted HTML title, not
/// a parse crash.
#[tokio::test]
async fn tier1_consumed_token_replay_names_the_html_title() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/app/login"))
        .respond_with(
            wiremock::ResponseTemplate::new(302)
                .insert_header("Location", format!("/idp/default/oidc/auth?{OIDC_QUERY}")),
        )
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/idp/default/oidc/auth"))
        .respond_with(
            wiremock::ResponseTemplate::new(302).insert_header(
                "Location",
                format!("/idp/default/authn/login?app=gateway&token={T0}"),
            ),
        )
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/idp/default/authn/next-challenge"))
        .respond_with(
            wiremock::ResponseTemplate::new(400).set_body_raw(
                common::jetty_error_html(400, "/idp/default/authn/next-challenge"),
                "text/html;charset=iso-8859-1",
            ),
        )
        .mount(&server)
        .await;

    let flow = IdpLoginFlow::new(&server.uri()).expect("flow builds");
    let err = match login(flow, "admin", &Secret::new("any")).await {
        Err(err) => err,
        Ok(_) => panic!("a consumed-token replay must error, not succeed"),
    };
    let message = err.to_string();
    assert!(
        message.contains("step 3"),
        "the failure names the step: {message}"
    );
    assert!(
        message.contains("Error 400"),
        "the extracted Jetty title rides the message: {message}"
    );
}

// -------------------------------------------------------------------------
// Live e2e (#[ignore], env-gated, quiet-skip — the 03-03 harness
// convention; mutations additionally gated by IGNITION_LIVE_MUTATIONS=1)
//
// Run against a rig whose trial is EXPIRED (a reset subject):
//   IGNITION_LIVE_URL=http://localhost:9088 \
//   IGNITION_LIVE_TOKEN='name:key' \
//   IGNITION_LIVE_USER=admin IGNITION_LIVE_PASSWORD=… \
//   IGNITION_LIVE_MUTATIONS=1 \
//   cargo test -p ignition-core --test trial_contract -- --ignored
//
// During 04-03's spike (2026-08-22) the tier-1 flow was verified
// END-TO-END by hand on the ignition-devops rig (8.3.3, expired):
// expired:true → false, 0 → 7199s. These tests keep that proof
// repeatable. The tier-0 token question on an EXPIRED rig remains
// formally open (no token could be provisioned headlessly — the
// api-token resource create's `collection` value is undiscovered);
// the probe below decides it the moment a token exists.
// -------------------------------------------------------------------------

fn live_url() -> Option<String> {
    std::env::var("IGNITION_LIVE_URL")
        .ok()
        .filter(|value| !value.is_empty())
}

fn skip(reason: &str) {
    eprintln!("skipping: {reason}");
}

fn mutations_armed() -> bool {
    std::env::var("IGNITION_LIVE_MUTATIONS").ok().as_deref() == Some("1")
}

/// The tier-0 probe: token-auth POST /trial against a live expired
/// rig — ONE call decides whether the token rung suffices without the
/// login dance. Quiet-skips without env; skips (with the reason) when
/// the trial is not currently expired (the live-discovered state
/// gate) or mutations are not armed.
#[tokio::test]
#[ignore = "opt-in: set IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN + IGNITION_LIVE_MUTATIONS=1 against an EXPIRED rig"]
async fn trial_reset_tier0_probe() {
    let Some(url) = live_url() else {
        skip("IGNITION_LIVE_URL not set");
        return;
    };
    let Ok(token) = std::env::var("IGNITION_LIVE_TOKEN") else {
        skip("IGNITION_LIVE_TOKEN not set");
        return;
    };
    if !mutations_armed() {
        skip("IGNITION_LIVE_MUTATIONS=1 not set (mutations stay off)");
        return;
    }
    let credential = Credential::Token(Secret::new(token));
    let api = ReqwestGatewayApi::for_tests(&url, Some(credential));
    let before = api.trial_status_wire().await.expect("live trial GET");
    if !before.expired {
        skip(&format!(
            "trial not expired ({}s left) — the state gate makes the probe meaningless",
            before.trial_seconds_left
        ));
        return;
    }
    match api.trial_reset_wire().await {
        Ok(fresh) => {
            let after = api.trial_status_wire().await.expect("read-back");
            assert!(!after.expired, "the flip verified by read-back");
            eprintln!(
                "TIER 0 WORKS on a live expired rig: expired {}→{}, {}s left (mechanism candidate confirmed)",
                before.expired, fresh.expired, fresh.trial_seconds_left
            );
        }
        Err(err) => {
            // Not a test failure — the probe's ANSWER: tier 0 does not
            // satisfy the reset on this gateway; tier 1 owns it.
            eprintln!("TIER 0 REJECTED by the live gateway: {err} — tier 1 (login) remains the mechanism");
        }
    }
}

/// The tier-1 live e2e: the full login dance + reset + read-back flip
/// against a live expired rig with known admin credentials.
#[tokio::test]
#[ignore = "opt-in: set IGNITION_LIVE_URL + IGNITION_LIVE_USER + IGNITION_LIVE_PASSWORD + IGNITION_LIVE_MUTATIONS=1 against an EXPIRED rig"]
async fn trial_reset_tier1_live() {
    let Some(url) = live_url() else {
        skip("IGNITION_LIVE_URL not set");
        return;
    };
    let (Ok(user), Ok(password)) = (
        std::env::var("IGNITION_LIVE_USER"),
        std::env::var("IGNITION_LIVE_PASSWORD"),
    ) else {
        skip("IGNITION_LIVE_USER / IGNITION_LIVE_PASSWORD not both set");
        return;
    };
    if !mutations_armed() {
        skip("IGNITION_LIVE_MUTATIONS=1 not set (mutations stay off)");
        return;
    }
    let api = ReqwestGatewayApi::for_tests(&url, None);
    let before = api.trial_status_wire().await.expect("live trial GET");
    if !before.expired {
        skip(&format!(
            "trial not expired ({}s left) — the gateway refuses resets until it expires",
            before.trial_seconds_left
        ));
        return;
    }
    let flow = IdpLoginFlow::new(&url).expect("flow builds");
    let (flow, session) = login(flow, &user, &Secret::new(&password))
        .await
        .expect("the live login dance completes");
    live_flip::assert_reset_flip(&flow, &session).await;
}

/// The live test's reset + flip tail (read-back REQUIRED — the 03-01
/// mutation precedent).
mod live_flip {
    use super::*;

    pub(crate) async fn assert_reset_flip(flow: &IdpLoginFlow, session: &GatewaySession) {
        let url = std::env::var("IGNITION_LIVE_URL").expect("checked by the caller");
        let fresh = trial_reset_via_session(flow, session)
            .await
            .expect("the live reset POST answers 200 + the fresh trial");
        assert!(!fresh.expired);
        let api = ReqwestGatewayApi::for_tests(&url, None);
        let after = api.trial_status_wire().await.expect("live read-back");
        assert!(
            !after.expired,
            "the flip verified by read-back ({}s left)",
            after.trial_seconds_left
        );
    }
}
