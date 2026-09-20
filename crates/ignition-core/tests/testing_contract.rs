//! Wiremock contract for the testing action (QUICK-p0g) — the
//! standalone `ign testing run` verb over the already-deployed
//! gateway testing bundle, exercised through the REAL
//! [`ReqwestGatewayApi`] (trait level; `ign adopt --testing` keeps
//! its own separate path).
//!
//! THE crown pins:
//! - **discover**: `{"discover": true}` rides the POST BODY (the
//!   route is POST-only) and the module list maps into the result
//!   with NO verdict fields set;
//! - **the absent-route discrimination**: 404 / 405 / 501 ALL mean
//!   "the testing bundle is not deployed" and refuse exit 6
//!   `routes_not_deployed` — the raw-status probe exists precisely
//!   because `classify` collapses 405/500/501 into `Internal`
//!   (exit 1), which would make the refusal indistinguishable from
//!   a bug;
//! - **the testing-scoped hint**: names BOTH
//!   `ign webdev deploy --with-testing` and `ign adopt … --testing`;
//! - **the first-touch lazy-compile 500**: retried EXACTLY once
//!   (the `adopt.rs` warmup behavior, live-pinned).

use ignition_core::actions::testing::{TestingRunOptions, testing_run};
use ignition_core::client::ReqwestGatewayApi;

/// The testing run route path inside any wiremock server.
const ROUTE_PATH: &str = "/system/webdev/ign-cli/testing/run";

/// The discover-only options (the Task-1 tracer path).
fn discover_opts() -> TestingRunOptions {
    TestingRunOptions { discover: true }
}

/// Discover: the body is EXACTLY `{"discover": true}` and the answer
/// maps onto `mode == "discover"` + the module list + count, with the
/// verdict fields deliberately unset (a discovery is not a verdict).
#[tokio::test]
async fn discover_lists_modules() {
    let server = wiremock::MockServer::start().await;
    let discover = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(ROUTE_PATH))
        .and(wiremock::matchers::body_json(
            serde_json::json!({"discover": true}),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "discovered_modules": ["testing.__tests__", "proj.foo_test"],
                "count": 2,
            })),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = testing_run(&api, "ign-cli", &discover_opts())
        .await
        .expect("discover succeeds");

    assert_eq!(result.mode, "discover");
    assert_eq!(result.project, "ign-cli");
    assert_eq!(
        result.modules,
        vec!["testing.__tests__".to_string(), "proj.foo_test".to_string()]
    );
    assert_eq!(result.count, 2);
    assert_eq!(result.verdict, None, "a discovery carries NO verdict");
    assert_eq!(result.slug, None);
    assert_eq!(result.failure_exit_code(), None);

    let requests = discover.received_requests().await;
    assert_eq!(requests.len(), 1, "discover is ONE round trip");
    let body: serde_json::Value =
        serde_json::from_slice(&requests[0].body).expect("discover body parses");
    assert_eq!(body, serde_json::json!({"discover": true}));
}

/// The absent-route discrimination: 404, 405 and 501 ALL mean the
/// testing bundle was never deployed. Each refuses exit 6
/// `routes_not_deployed` with the testing-scoped hint naming BOTH
/// remediation commands.
#[tokio::test]
async fn absent_route_refuses_routes_not_deployed() {
    for status in [404_u16, 405, 501] {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path(ROUTE_PATH))
            .respond_with(wiremock::ResponseTemplate::new(status))
            .mount(&server)
            .await;

        let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
        let err = testing_run(&api, "ign-cli", &discover_opts())
            .await
            .expect_err("an absent route refuses");

        assert_eq!(
            err.code(),
            "routes_not_deployed",
            "HTTP {status} is the absent marker"
        );
        assert_eq!(
            err.exit_code(),
            6,
            "target state, not a bug (HTTP {status})"
        );
        let hint = err.hint().expect("hint required");
        assert!(
            hint.contains("ign webdev deploy --with-testing"),
            "the testing-scoped hint names the deploy flag: {hint}"
        );
        assert!(
            hint.contains("ign adopt") && hint.contains("--testing"),
            "the testing-scoped hint names the adopt path: {hint}"
        );
    }
}

/// The first-touch lazy-compile 500: a freshly deployed route can
/// answer 500 on its FIRST request while WebDev compiles the module.
/// ONE bounded retry — the second answer is the real one, and the
/// server saw EXACTLY two requests.
#[tokio::test]
async fn first_touch_500_is_retried_once() {
    let server = wiremock::MockServer::start().await;
    // The lazy-compile blowup, exactly once (scoped + up_to_n_times so
    // the second request falls through to the discover mock below).
    let lazy = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(ROUTE_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "error": "ImportError",
                "traceback": "…",
            })),
        )
        .up_to_n_times(1)
        .expect(1)
        .mount_as_scoped(&server)
        .await;
    let ready = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(ROUTE_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "discovered_modules": ["testing.__tests__"],
                "count": 1,
            })),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = testing_run(&api, "ign-cli", &discover_opts())
        .await
        .expect("the retry recovers");
    assert_eq!(result.count, 1);
    assert_eq!(result.modules, vec!["testing.__tests__".to_string()]);

    assert_eq!(lazy.received_requests().await.len(), 1);
    assert_eq!(ready.received_requests().await.len(), 1);
    assert_eq!(
        server
            .received_requests()
            .await
            .expect("received requests")
            .len(),
        2,
        "EXACTLY one retry — not a loop"
    );
}
