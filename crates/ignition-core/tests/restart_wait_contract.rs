//! Wiremock lifecycle contract tests for restart + wait (02-05,
//! HLTH-09/11): the restart POST shape, and the StatusPing-anchored
//! wait semantics — the restart-aware floor (Open Question 4) and the
//! deadline taxonomy.
//!
//! The crown pins (recorded-request + sequence proofs):
//! - the restart POST hits the EXACT path with `confirm=true` as a
//!   QUERY param and an EMPTY body (the verified shape);
//! - `restart_and_wait` polls until RUNNING observed (STARTING
//!   observed first, success only after RUNNING);
//! - `wait_restart`'s witnessed-restart path returns as soon as
//!   RUNNING follows non-RUNNING (NO floor wait) while an all-RUNNING
//!   sequence succeeds ONLY once the floor elapsed;
//! - every timeout is the poll engine's Network-class deadline
//!   carrying the last observed state.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ignition_core::actions::restart::{
    RESTART_FLOOR, restart, restart_and_wait, wait_gateway, wait_module, wait_restart,
};
use ignition_core::client::{GatewayApi, ReqwestGatewayApi};
use ignition_core::error::CoreError;

const RESTART_PATH: &str = "/data/api/v1/restart-tasks/restart";
const STATUS_PING_PATH: &str = "/StatusPing";
const MODULES_HEALTHY_PATH: &str = "/data/api/v1/modules/healthy";

/// Lowercased Debug dump of a recorded request's headers (01-04 pattern).
fn headers_debug(request: &wiremock::Request) -> String {
    format!("{:?}", request.headers).to_lowercase()
}

/// A scripted StatusPing: serves `states` in order, the LAST entry
/// repeating forever. Clone keeps the hit counter shared so a test can
/// assert poll counts after the mock consumed its clone.
#[derive(Clone)]
struct StatusPingScript {
    hits: Arc<Mutex<usize>>,
    states: Vec<String>,
}

impl StatusPingScript {
    fn new(states: &[&str]) -> Self {
        Self {
            hits: Arc::new(Mutex::new(0)),
            states: states.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn hit_count(&self) -> usize {
        *self.hits.lock().unwrap()
    }
}

impl wiremock::Respond for StatusPingScript {
    fn respond(&self, _request: &wiremock::Request) -> wiremock::ResponseTemplate {
        let mut hits = self.hits.lock().unwrap();
        *hits += 1;
        let state = self
            .states
            .get(*hits - 1)
            .unwrap_or_else(|| self.states.last().expect("at least one state"));
        wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({ "state": state }))
    }
}

/// Mount a scripted StatusPing responder (any number of hits).
async fn mount_status_ping(server: &wiremock::MockServer, script: StatusPingScript) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(STATUS_PING_PATH))
        .respond_with(script)
        .expect(1..)
        .mount(server)
        .await;
}

/// THE restart-POST pin (recorded-request proof): exact path,
/// `confirm=true` on the QUERY string, EMPTY body, authed — and the
/// literal `true` body classifies Ok. The drift guard: any other 2xx
/// body still succeeds (warn, don't fail).
#[tokio::test]
async fn restart_posts_confirm_true_query_param_with_empty_body() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(RESTART_PATH))
        .and(wiremock::matchers::query_param("confirm", "true"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("true"))
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(
        &server.uri(),
        Some(ignition_core::config::Credential::Token(
            ignition_core::config::Secret::new("name:key"),
        )),
    );
    api.restart().await.expect("200 + `true` classifies Ok");

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0].body.is_empty(),
        "the POST carries NO body — confirm rides the query string"
    );
    assert!(
        requests[0]
            .url
            .query()
            .unwrap_or_default()
            .contains("confirm=true"),
        "confirm=true is a QUERY param: {}",
        requests[0].url
    );
    assert!(
        headers_debug(&requests[0]).contains("x-ignition-api-token"),
        "the restart is authed"
    );

    // Success-shape drift: a non-`true` 2xx body still Ok (warn, not fail).
    let drift_server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(RESTART_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("{\"accepted\":1}"))
        .expect(1)
        .mount(&drift_server)
        .await;
    let drift_api = ReqwestGatewayApi::for_tests(&drift_server.uri(), None);
    drift_api
        .restart()
        .await
        .expect("2xx drift body still succeeds (warn only)");
}

/// restart_and_wait: POST → floor → poll. The script answers STARTING
/// twice then RUNNING — success arrives only after RUNNING was
/// observed (≥3 poll hits), and the POST fired exactly once.
#[tokio::test]
async fn restart_and_wait_polls_until_running() {
    let server = wiremock::MockServer::start().await;
    let ping = StatusPingScript::new(&["STARTING", "STARTING", "RUNNING"]);
    mount_status_ping(&server, ping.clone()).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(RESTART_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("true"))
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = restart_and_wait(
        &api,
        Duration::from_millis(10),
        Duration::from_secs(10),
        Duration::from_millis(10), // tiny floor — the semantic floor test is below
    )
    .await
    .expect("lifecycle completes");

    assert!(result.restarted);
    assert_eq!(result.state, "RUNNING");
    assert!(
        ping.hit_count() >= 3,
        "STARTING observed before the RUNNING success: {} hits",
        ping.hit_count()
    );
}

/// Fast-flip guard (Open Question 4): StatusPing answers RUNNING from
/// the very first poll — restart_and_wait STILL succeeds because the
/// floor slept before the first poll. Proven by timing: the call
/// cannot finish before the (injectable) floor elapsed.
#[tokio::test]
async fn restart_and_wait_fast_flip_succeeds_via_floor() {
    let server = wiremock::MockServer::start().await;
    mount_status_ping(&server, StatusPingScript::new(&["RUNNING"])).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(RESTART_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("true"))
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let floor = Duration::from_millis(250);
    let started = Instant::now();
    let result = restart_and_wait(
        &api,
        Duration::from_millis(20),
        Duration::from_secs(10),
        floor,
    )
    .await
    .expect("all-RUNNING after the floor = success (fast flip)");
    assert_eq!(result.state, "RUNNING");
    assert!(
        started.elapsed() >= floor,
        "the floor slept BEFORE the first poll — elapsed {} < floor {:?}",
        started.elapsed().as_millis(),
        floor
    );
}

/// Timeout taxonomy: StatusPing always STARTING under a short deadline
/// → the poll engine's Network-class error (exit 4, source: None)
/// whose message names STARTING as the last observation.
#[tokio::test]
async fn restart_wait_timeout_names_last_observed_state() {
    let server = wiremock::MockServer::start().await;
    mount_status_ping(&server, StatusPingScript::new(&["STARTING"])).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(RESTART_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("true"))
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let err = restart_and_wait(
        &api,
        Duration::from_millis(20),
        Duration::from_millis(300),
        Duration::from_millis(10),
    )
    .await
    .expect_err("deadline must expire");
    assert!(
        matches!(&err, CoreError::Network { source: None, .. }),
        "deadline = Network with no transport source: {err}"
    );
    assert_eq!(err.exit_code(), 4);
    let message = err.to_string();
    assert!(
        message.contains("STARTING"),
        "last observation named: {message}"
    );
    assert!(message.contains("timed out"), "timeout named: {message}");
}

/// wait_gateway: IMMEDIATE success when already RUNNING is correct —
/// exactly ONE poll (`wait gateway` answers "is it up", not "did it
/// restart").
#[tokio::test]
async fn wait_gateway_immediate_success_is_one_poll() {
    let server = wiremock::MockServer::start().await;
    let ping = StatusPingScript::new(&["RUNNING"]);
    mount_status_ping(&server, ping.clone()).await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = wait_gateway(&api, Duration::from_millis(20), Duration::from_secs(5))
        .await
        .expect("already RUNNING = immediate success");
    assert_eq!(result.target, "gateway");
    assert_eq!(result.state, "RUNNING");
    assert_eq!(ping.hit_count(), 1, "no further polls after success");
}

/// wait_gateway waits through STARTING until RUNNING.
#[tokio::test]
async fn wait_gateway_polls_starting_to_running() {
    let server = wiremock::MockServer::start().await;
    mount_status_ping(&server, StatusPingScript::new(&["STARTING", "RUNNING"])).await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = wait_gateway(&api, Duration::from_millis(10), Duration::from_secs(5))
        .await
        .expect("STARTING then RUNNING completes");
    assert_eq!(result.state, "RUNNING");
}

/// wait_restart, the WITNESSED path: non-RUNNING on poll 1, RUNNING on
/// poll 2 → success WITHOUT waiting out the floor (a huge floor proves
/// the short-circuit: the call returns long before it).
#[tokio::test]
async fn wait_restart_witnessed_restart_short_circuits_floor() {
    let server = wiremock::MockServer::start().await;
    mount_status_ping(&server, StatusPingScript::new(&["STARTING", "RUNNING"])).await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let started = Instant::now();
    let result = wait_restart(
        &api,
        Duration::from_millis(10),
        Duration::from_secs(5),
        Duration::from_secs(600), // huge floor — never awaited on this path
    )
    .await
    .expect("witnessed restart succeeds immediately");
    assert_eq!(result.state, "RUNNING");
    assert_eq!(result.target, "restart");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "no floor wait on the witnessed path: {:?}",
        started.elapsed()
    );
}

/// wait_restart, the ALL-RUNNING path: success is accepted ONLY once
/// the floor elapsed — timing-proven (no sequence of pre-floor
/// successes can return), then the terminal state is RUNNING.
#[tokio::test]
async fn wait_restart_all_running_requires_floor_elapsed() {
    let server = wiremock::MockServer::start().await;
    let ping = StatusPingScript::new(&["RUNNING"]);
    mount_status_ping(&server, ping.clone()).await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let floor = Duration::from_millis(300);
    let started = Instant::now();
    let result = wait_restart(
        &api,
        Duration::from_millis(20),
        Duration::from_secs(10),
        floor,
    )
    .await
    .expect("all-RUNNING past the floor = sanctioned success");
    assert_eq!(result.state, "RUNNING");
    assert!(
        started.elapsed() >= floor,
        "no success before the floor elapsed: {:?} < {:?}",
        started.elapsed(),
        floor
    );
    assert!(
        ping.hit_count() >= 2,
        "polled repeatedly while inside the floor: {}",
        ping.hit_count()
    );
}

/// wait_restart timeout: always STARTING + short deadline → the
/// Network-class deadline naming STARTING.
#[tokio::test]
async fn wait_restart_timeout_names_last_observed_state() {
    let server = wiremock::MockServer::start().await;
    mount_status_ping(&server, StatusPingScript::new(&["STARTING"])).await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let err = wait_restart(
        &api,
        Duration::from_millis(20),
        Duration::from_millis(300),
        Duration::from_millis(10),
    )
    .await
    .expect_err("deadline must expire");
    assert!(
        matches!(&err, CoreError::Network { source: None, .. }),
        "deadline class: {err}"
    );
    assert_eq!(err.exit_code(), 4);
    assert!(
        err.to_string().contains("STARTING"),
        "last observation named: {err}"
    );
}

/// wait_module: the target module present and ACTIVE → immediate Ok;
/// the search param rode the query string (recorded-request proof).
#[tokio::test]
async fn wait_module_succeeds_when_active() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(MODULES_HEALTHY_PATH))
        .and(wiremock::matchers::query_param(
            "search",
            "com.inductiveautomation.perspective",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {
                        "id": "com.inductiveautomation.perspective",
                        "name": "Perspective",
                        "version": "8.3.6",
                        "state": "ACTIVE"
                    }
                ],
                "metadata": {"total": 1, "matching": 1, "limit": -1, "offset": 0}
            })),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = wait_module(
        &api,
        "com.inductiveautomation.perspective",
        Duration::from_millis(20),
        Duration::from_secs(5),
    )
    .await
    .expect("module ACTIVE = success");
    assert_eq!(result.state, "ACTIVE");
    assert_eq!(result.target, "module com.inductiveautomation.perspective");
    assert_eq!(guard.received_requests().await.len(), 1);
}

/// wait_module: module never reaches ACTIVE → the Network-class
/// deadline naming the module id (subject) and the observed state.
#[tokio::test]
async fn wait_module_timeout_names_the_id() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(MODULES_HEALTHY_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {
                        "id": "com.example.slow",
                        "name": "Slow Module",
                        "version": "1.0.0",
                        "state": "LOADING"
                    }
                ],
                "metadata": {"total": 1, "matching": 1, "limit": -1, "offset": 0}
            })),
        )
        .expect(1..)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let err = wait_module(
        &api,
        "com.example.slow",
        Duration::from_millis(20),
        Duration::from_millis(300),
    )
    .await
    .expect_err("deadline must expire");
    assert_eq!(err.exit_code(), 4);
    let message = err.to_string();
    assert!(
        message.contains("com.example.slow"),
        "module id named: {message}"
    );
    assert!(
        message.contains("LOADING"),
        "last observed state named: {message}"
    );
}

/// The bare `restart` action (no wait): POST fires exactly once and
/// the result reports restarted.
#[tokio::test]
async fn bare_restart_fires_once() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(RESTART_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("true"))
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = restart(&api).await.expect("POST accepted");
    assert!(result.restarted);
}

/// Deliberately ABSENT capability proof: the trait exposes no
/// restart-tasks/pending method and no wait code path reads it — pinned
/// by asserting the floor constant is the ONE shared mitigation (the
/// grep-level guarantee lives in the plan's key_link check).
#[test]
fn restart_floor_constant_is_the_one_shared_literal() {
    assert_eq!(RESTART_FLOOR, Duration::from_secs(5));
}
