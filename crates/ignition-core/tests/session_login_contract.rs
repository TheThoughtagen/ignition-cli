//! Wiremock contract tests for `ign session login` (QUICK-tg4) — the
//! IdP dance projected into the five-field envelope and the Playwright
//! `storageState` document.
//!
//! The nine-request login sequence is mounted here rather than shared
//! with `trial_contract.rs`: that file's `mount_login_dance` is
//! test-local (an integration test's items are not importable across
//! test binaries), so duplicating the mounts is the honest move —
//! refactoring the trial suite to export a fixture is out of this
//! task's scope and would put a test-only seam in the library.
//!
//! What is deliberately NOT duplicated: the trial suite owns the
//! token-threading and password-redaction proofs for the dance itself.
//! These tests own the PROJECTION — the envelope's shape, the cookie
//! rules (D7), and the auth-rejection exit class.

use ignition_core::actions::login::{SessionLoginOptions, session_login};
use ignition_core::config::Secret;

/// The scripted fixture tokens/cookies (invented values; the SHAPES are
/// the live-captured ones — see `client/idp.rs`'s module docs).
const RELAY_COOKIE: &str = "idp-relay-1766878194=relay-value";
const SID_COOKIE: &str = "idp-sid-default-1766878194=sid-value";
const T0: &str = "T0-from-login-redirect";
const T1: &str = "T1-after-first-challenge";
const T2: &str = "T2-after-credentials";
const T3: &str = "T3-complete";
const SESSION_COOKIE_NAME: &str = "webui-sid-1766878194";
const SESSION_COOKIE_VALUE: &str = "session-value-must-not-leak";
const CSRF: &str = "csrf-value-must-not-leak";

/// The OIDC params step 1 hands out (step 6 must replay them + token).
const OIDC_QUERY: &str = "app=gateway&response_type=code&client_id=ignition&redirect_uri=%2Fdata%2Ffederate%2Fcallback%2Finternal&scope=openid&state=st&nonce=nc&prompt=login&max_age=1";

/// Mount steps 1–8 (login → session/CSRF). `credentials_accepted`
/// drives step 4's verdict: `false` is the live-observed rejection
/// shape (HTTP 200 with `success:false`).
async fn mount_login_dance(server: &wiremock::MockServer, credentials_accepted: bool) {
    // 1. GET /data/app/login → 302 into the OIDC flow (+ relay cookie).
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/app/login"))
        .respond_with(
            wiremock::ResponseTemplate::new(302)
                .insert_header("Location", format!("/idp/default/oidc/auth?{OIDC_QUERY}"))
                .append_header("Set-Cookie", format!("{RELAY_COOKIE}; Path=/; HttpOnly")),
        )
        .mount(server)
        .await;
    // 2. GET oidc/auth (token ABSENT — the negative is what separates
    //    this hop from step 6's replay of the same path).
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/idp/default/oidc/auth"))
        .and(wiremock::matchers::query_param("state", "st"))
        .and(wiremock::matchers::query_param_is_missing("token"))
        .respond_with(
            wiremock::ResponseTemplate::new(302)
                .insert_header(
                    "Location",
                    format!(
                        "/idp/default/authn/login?app=gateway&token={T0}&response_type=code&client_id=ignition"
                    ),
                )
                .append_header(
                    "Set-Cookie",
                    format!("{SID_COOKIE}; Path=/idp/default; HttpOnly; SameSite=Strict"),
                ),
        )
        .mount(server)
        .await;
    // 3. POST next-challenge {"token": T0} → T1.
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/idp/default/authn/next-challenge",
        ))
        .and(wiremock::matchers::body_json(
            serde_json::json!({ "token": T0 }),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "complete": false,
                "nextChallenge": [{"type": "basic", "config": {}}],
                "rememberMe": false, "passwordExpired": false,
                "token": T1
            })),
        )
        .mount(server)
        .await;
    // 4. POST submit-challenge/basic carrying T1 + the creds → T2.
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/idp/default/authn/submit-challenge/basic",
        ))
        .and(wiremock::matchers::body_partial_json(serde_json::json!({
            "token": T1,
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "success": credentials_accepted, "token": T2 })),
        )
        .mount(server)
        .await;
    // 5. POST next-challenge {"token": T2} → complete + T3.
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/idp/default/authn/next-challenge",
        ))
        .and(wiremock::matchers::body_json(
            serde_json::json!({ "token": T2 }),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "complete": true,
                "rememberMe": false, "passwordExpired": false,
                "token": T3
            })),
        )
        .mount(server)
        .await;
    // 6. GET oidc/auth?orig&token=T3 → the federate callback.
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/idp/default/oidc/auth"))
        .and(wiremock::matchers::query_param("token", T3))
        .and(wiremock::matchers::query_param("state", "st"))
        .respond_with(wiremock::ResponseTemplate::new(302).insert_header(
            "Location",
            "/data/federate/callback/internal?code=auth-code&state=st",
        ))
        .mount(server)
        .await;
    // 7. GET the callback → /app + the webui-sid session cookie.
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/federate/callback/internal"))
        .and(wiremock::matchers::query_param("code", "auth-code"))
        .respond_with(
            wiremock::ResponseTemplate::new(302)
                .insert_header("Location", "/app")
                .append_header(
                    "Set-Cookie",
                    format!(
                        "{SESSION_COOKIE_NAME}={SESSION_COOKIE_VALUE}; Path=/; HttpOnly; SameSite=Strict"
                    ),
                ),
        )
        .mount(server)
        .await;
    // 8. GET /data/app/session → the CSRF token.
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/app/session"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "userPayload": { "user": { "userName": "admin" } },
                "csrfToken": CSRF
            })),
        )
        .mount(server)
        .await;
}

fn opts() -> SessionLoginOptions {
    SessionLoginOptions {
        username: "admin".into(),
    }
}

/// THE envelope proof: all five fields, the session material verbatim
/// from the dance, and a `storageState` with exactly two keys.
#[tokio::test]
async fn login_returns_the_session_and_a_playwright_storage_state() {
    let server = wiremock::MockServer::start().await;
    mount_login_dance(&server, true).await;
    let base = server.uri();

    let result = session_login(&base, &opts(), &Secret::new("correct-horse"))
        .await
        .expect("the dance completes");

    assert_eq!(result.cookie_name, SESSION_COOKIE_NAME);
    assert_eq!(result.cookie_value, SESSION_COOKIE_VALUE);
    assert_eq!(result.csrf_token, CSRF);
    assert_eq!(result.gateway_url, base, "gateway_url IS the profile URL");

    // The wire shape is what a globalSetup writes to disk untouched.
    let wire = serde_json::to_value(&result).expect("serializes");
    for key in [
        "cookie_name",
        "cookie_value",
        "csrf_token",
        "gateway_url",
        "storage_state",
    ] {
        assert!(wire.get(key).is_some(), "{key} is always present");
    }
    let state = &wire["storage_state"];
    assert_eq!(
        state.as_object().expect("an object").len(),
        2,
        "storageState has exactly {{cookies, origins}}"
    );
    assert_eq!(state["cookies"].as_array().expect("an array").len(), 1);
    assert_eq!(state["origins"].as_array().expect("an array").len(), 0);
}

/// The cookie rules (D7) on the wire, in Playwright's own spelling.
#[tokio::test]
async fn storage_state_cookie_shape_is_playwright_exact() {
    let server = wiremock::MockServer::start().await;
    mount_login_dance(&server, true).await;
    let base = server.uri();

    let result = session_login(&base, &opts(), &Secret::new("correct-horse"))
        .await
        .expect("the dance completes");
    let wire = serde_json::to_value(&result).expect("serializes");
    let cookie = &wire["storage_state"]["cookies"][0];

    assert_eq!(cookie["name"], SESSION_COOKIE_NAME);
    assert_eq!(cookie["value"], SESSION_COOKIE_VALUE);
    // The mock server binds 127.0.0.1:<random> — the PORT must not
    // reach `domain` or Playwright rejects the storage state.
    assert_eq!(cookie["domain"], "127.0.0.1");
    assert!(
        !cookie["domain"].as_str().unwrap().contains(':'),
        "no port inside domain"
    );
    assert_eq!(cookie["path"], "/");
    assert_eq!(cookie["httpOnly"], true, "Playwright's spelling");
    assert_eq!(cookie["sameSite"], "Strict");
    assert_eq!(cookie["secure"], false, "the mock speaks http");
    assert_eq!(cookie["expires"], -1.0);
    assert!(cookie["expires"].is_number(), "unquoted");
}

/// An `https` profile URL marks the cookie secure. Driven through the
/// pure projection rather than a TLS mock — the rule is a scheme read,
/// and standing up a certificate chain to assert one boolean would test
/// rustls, not this code.
#[test]
fn https_profile_marks_the_cookie_secure() {
    use ignition_core::actions::login::storage_state_for;
    use ignition_core::client::idp::GatewaySession;

    let session = GatewaySession {
        cookie_name: SESSION_COOKIE_NAME.into(),
        cookie_value: SESSION_COOKIE_VALUE.into(),
        csrf_token: CSRF.into(),
    };
    let secure = storage_state_for(&session, "https://gateway.example.com:8043").unwrap();
    assert!(secure.cookies[0].secure);
    assert_eq!(secure.cookies[0].domain, "gateway.example.com");

    let plain = storage_state_for(&session, "http://gateway.example.com:8088").unwrap();
    assert!(!plain.cookies[0].secure);
}

/// Rejected credentials: the live-observed 200 `success:false` shape →
/// exit 5, and NO result of any kind.
#[tokio::test]
async fn rejected_credentials_surface_as_auth_exit_five() {
    let server = wiremock::MockServer::start().await;
    mount_login_dance(&server, false).await;

    let err = session_login(&server.uri(), &opts(), &Secret::new("wrong-password"))
        .await
        .expect_err("a rejected credential refuses");

    assert_eq!(err.exit_code(), 5, "auth class");
    assert_eq!(err.code(), "auth_rejected");
    let rendered = err.to_string();
    assert!(
        !rendered.contains(SESSION_COOKIE_VALUE) && !rendered.contains(CSRF),
        "a refusal carries no session material: {rendered}"
    );
}
