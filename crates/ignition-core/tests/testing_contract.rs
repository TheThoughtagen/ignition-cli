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

use ignition_core::actions::testing::{TestingFormat, TestingRunOptions, testing_run};
use ignition_core::client::ReqwestGatewayApi;

/// The testing run route path inside any wiremock server.
const ROUTE_PATH: &str = "/system/webdev/ign-cli/testing/run";

/// The discover-only options.
fn discover_opts() -> TestingRunOptions {
    TestingRunOptions {
        discover: true,
        ..TestingRunOptions::default()
    }
}

/// The probe mock every RUN test needs: the route must prove present
/// before the run leg fires, and the probe's own body is the discover
/// shape.
async fn mount_probe(server: &wiremock::MockServer) {
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(ROUTE_PATH))
        .and(wiremock::matchers::body_json(
            serde_json::json!({"discover": true}),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "discovered_modules": ["proj.foo_test"],
                "count": 1,
            })),
        )
        .mount(server)
        .await;
}

/// A route run answer: 12 passed / 1 skipped, green.
fn green_body() -> serde_json::Value {
    serde_json::json!({
        "passed": 12, "failed": 0, "skipped": 1, "errors": 0,
        "total": 13, "duration_ms": 842,
        "modules": [{"module": "proj.foo_test", "passed": 12, "failed": 0,
                     "skipped": 1, "errors": 0, "duration_ms": 842,
                     "results": [{"name": "test_ok", "status": "passed",
                                  "duration_ms": 4}]}]
    })
}

/// A route run answer: 2 failures + 1 error, RED — answered at 207.
fn red_body() -> serde_json::Value {
    serde_json::json!({
        "passed": 9, "failed": 2, "skipped": 0, "errors": 1,
        "total": 12, "duration_ms": 991,
        "modules": [{"module": "proj.foo_test", "passed": 9, "failed": 2,
                     "skipped": 0, "errors": 1, "duration_ms": 991,
                     "results": [{"name": "test_bad", "status": "failed",
                                  "duration_ms": 6, "message": "1 != 2"}]}]
    })
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

/// A GREEN run: the body is exactly `{"format":"json"}` (the CLI
/// never asks the route for junit/text), the counts map through, the
/// `results` key carries the route answer VERBATIM, and the verb
/// exits 0.
#[tokio::test]
async fn green_run_exits_zero_with_full_results() {
    let server = wiremock::MockServer::start().await;
    mount_probe(&server).await;
    let run = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(ROUTE_PATH))
        .and(wiremock::matchers::body_json(
            serde_json::json!({"format": "json"}),
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(green_body()))
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = testing_run(&api, "ign-cli", &TestingRunOptions::default())
        .await
        .expect("a green run succeeds");

    assert_eq!(result.mode, "run");
    assert_eq!(result.verdict.as_deref(), Some("passed"));
    assert_eq!(result.slug, None);
    assert_eq!(result.failure_exit_code(), None, "green exits 0");
    assert_eq!(result.passed, 12);
    assert_eq!(result.failed, 0);
    assert_eq!(result.skipped, 1);
    assert_eq!(result.errors, 0);
    assert_eq!(result.total, 13);
    assert_eq!(result.duration_ms, 842);
    assert_eq!(result.modules, vec!["proj.foo_test".to_string()]);
    assert_eq!(
        result.results,
        green_body(),
        "the route body rides VERBATIM under `results`"
    );
    assert_eq!(run.received_requests().await.len(), 1);
}

/// A RED run at HTTP **207**: the call returns `Ok` (207 is a success
/// transport shape), the verdict is read from the BODY, the stable
/// slug rides the payload, the exit passthrough is 6 — and EVERY
/// result field survives, because the error envelope has no `data`
/// field to carry them.
#[tokio::test]
async fn red_run_reports_tests_failed_with_data_intact() {
    let server = wiremock::MockServer::start().await;
    mount_probe(&server).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(ROUTE_PATH))
        .and(wiremock::matchers::body_json(
            serde_json::json!({"format": "json"}),
        ))
        .respond_with(wiremock::ResponseTemplate::new(207).set_body_json(red_body()))
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = testing_run(&api, "ign-cli", &TestingRunOptions::default())
        .await
        .expect("207 is a SUCCESS transport shape — not an Err");

    assert_eq!(result.verdict.as_deref(), Some("failed"));
    assert_eq!(result.slug.as_deref(), Some("tests_failed"));
    assert_eq!(
        result.failure_exit_code(),
        Some(6),
        "the lint --strict seam"
    );
    assert_eq!(result.passed, 9);
    assert_eq!(result.failed, 2);
    assert_eq!(result.errors, 1);
    assert_eq!(result.total, 12);
    assert_eq!(
        result.results,
        red_body(),
        "a red run keeps the FULL results — the whole reason it rides the \
         success envelope"
    );
    let serialized = serde_json::to_value(&result).expect("serializes");
    assert_eq!(serialized["slug"], "tests_failed");
    assert!(
        serialized["results"]["modules"][0]["results"].is_array(),
        "per-test detail survives into the envelope: {serialized}"
    );
}

/// `--module` / `--package` ride the POST BODY exactly, asserted with
/// `body_json` (not a substring match).
#[tokio::test]
async fn module_selection_rides_the_body() {
    for (opts, expected) in [
        (
            TestingRunOptions {
                module: Some("a.b.c_test".into()),
                ..TestingRunOptions::default()
            },
            serde_json::json!({"module": "a.b.c_test", "format": "json"}),
        ),
        (
            TestingRunOptions {
                package: Some("proj.".into()),
                ..TestingRunOptions::default()
            },
            serde_json::json!({"package": "proj.", "format": "json"}),
        ),
    ] {
        let server = wiremock::MockServer::start().await;
        mount_probe(&server).await;
        let run = wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path(ROUTE_PATH))
            .and(wiremock::matchers::body_json(expected.clone()))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(green_body()))
            .expect(1)
            .mount_as_scoped(&server)
            .await;

        let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
        testing_run(&api, "ign-cli", &opts)
            .await
            .expect("the selection runs");

        let requests = run.received_requests().await;
        assert_eq!(requests.len(), 1);
        let body: serde_json::Value =
            serde_json::from_slice(&requests[0].body).expect("run body parses");
        assert_eq!(body, expected, "ONLY the keys the caller set ride the wire");
    }
}

/// The three-way selector gate refuses BEFORE any request — an empty
/// server proves it (any HTTP call would hit an unmatched mock).
#[tokio::test]
async fn exclusive_flags_refuse_before_any_request() {
    let cases = [
        (
            TestingRunOptions {
                discover: true,
                module: Some("a.b".into()),
                ..TestingRunOptions::default()
            },
            ["--discover", "--module"],
        ),
        (
            TestingRunOptions {
                discover: true,
                package: Some("a.".into()),
                ..TestingRunOptions::default()
            },
            ["--discover", "--package"],
        ),
        (
            TestingRunOptions {
                module: Some("a.b".into()),
                package: Some("a.".into()),
                ..TestingRunOptions::default()
            },
            ["--module", "--package"],
        ),
    ];
    for (opts, flags) in cases {
        let server = wiremock::MockServer::start().await;
        let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
        let err = testing_run(&api, "ign-cli", &opts)
            .await
            .expect_err("the pair refuses");

        assert_eq!(err.code(), "invalid_input");
        assert_eq!(err.exit_code(), 2, "usage class, not gateway state");
        let message = err.to_string();
        for flag in flags {
            assert!(message.contains(flag), "message names {flag}: {message}");
        }
        assert!(
            server
                .received_requests()
                .await
                .expect("received requests")
                .is_empty(),
            "the refusal performs ZERO HTTP requests"
        );
    }
}

/// A 500 on the RUN leg (after a successful probe) gets the same
/// single warmup retry — the run leg can be the first touch of a test
/// module's own imports.
#[tokio::test]
async fn run_leg_500_is_retried_once() {
    let server = wiremock::MockServer::start().await;
    mount_probe(&server).await;
    let blowup = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(ROUTE_PATH))
        .and(wiremock::matchers::body_json(
            serde_json::json!({"format": "json"}),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "error": "ImportError", "traceback": "...",
            })),
        )
        .up_to_n_times(1)
        .expect(1)
        .mount_as_scoped(&server)
        .await;
    let ready = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(ROUTE_PATH))
        .and(wiremock::matchers::body_json(
            serde_json::json!({"format": "json"}),
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(green_body()))
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = testing_run(&api, "ign-cli", &TestingRunOptions::default())
        .await
        .expect("the retry recovers");
    assert_eq!(result.verdict.as_deref(), Some("passed"));
    assert_eq!(blowup.received_requests().await.len(), 1);
    assert_eq!(ready.received_requests().await.len(), 1);
}

/// `--format junit|text` NEVER changes the wire body (the route is
/// always asked for json) and the report is rendered client-side —
/// so the verdict and the exit code are correct in EVERY format,
/// which a passthrough could not deliver (the route's junit/text
/// answers stay HTTP 200 and carry no counts).
#[tokio::test]
async fn format_renders_locally_and_keeps_the_red_verdict() {
    for format in [TestingFormat::Junit, TestingFormat::Text] {
        let server = wiremock::MockServer::start().await;
        mount_probe(&server).await;
        let run = wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path(ROUTE_PATH))
            .and(wiremock::matchers::body_json(
                serde_json::json!({"format": "json"}),
            ))
            .respond_with(wiremock::ResponseTemplate::new(207).set_body_json(red_body()))
            .expect(1)
            .mount_as_scoped(&server)
            .await;

        let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
        let result = testing_run(
            &api,
            "ign-cli",
            &TestingRunOptions {
                format,
                ..TestingRunOptions::default()
            },
        )
        .await
        .expect("207 stays a success transport shape in every format");

        assert!(
            !result.report.is_empty(),
            "{} renders a report into data.report",
            format.as_str()
        );
        assert_eq!(
            result.verdict.as_deref(),
            Some("failed"),
            "{} keeps the verdict the json path found",
            format.as_str()
        );
        assert_eq!(result.failure_exit_code(), Some(6));
        assert_eq!(
            result.results,
            red_body(),
            "the structured truth rides alongside the rendered report"
        );
        let body: serde_json::Value =
            serde_json::from_slice(&run.received_requests().await[0].body).expect("body parses");
        assert_eq!(
            body,
            serde_json::json!({"format": "json"}),
            "the route is ALWAYS asked for json — the report is local"
        );
    }
}
