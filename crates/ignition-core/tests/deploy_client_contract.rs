//! Wiremock contract tests for the `DeployApi` scan-lock and scan calls
//! (20-03, DAPI-03 / DAPI-04 / DAPI-06 / DAPI-07).
//!
//! Capture-pinning rule: every body below is copied verbatim from
//! `.planning/phases/20-config-api-client/20-LIVE-CAPTURES.md` (live 8.3.6)
//! and each test cites the block it rests on. A request is asserted on
//! what was RECEIVED (method, body, header), never only on the answer.
//! Every call goes through `&dyn DeployApi`, which also proves the trait
//! is dyn-compatible.

mod common;

use common::IgnitionMock;
use std::time::Duration;

use ignition_core::client::deploy_api::scan_and_wait;
use ignition_core::client::scan::{
    LockObservation, LockVerdict, ScanKind, ScanOutcome, lock_verdict,
};
use ignition_core::client::{DeployApi, ReqwestGatewayApi};
use ignition_core::config::{Credential, Secret};
use ignition_core::error::CoreError;
use ignition_core::poll::PollConfig;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

const LOCK_CONFIG: &str = "/data/api/v1/scan-lock/config";

fn token_credential() -> Credential {
    Credential::Token(Secret::new("cap-a:tokengeneratedlive"))
}

/// C1e, sub-block "cap-b competing acquire 2/5": 409 Conflict, Jetty HTML,
/// the holder named in the `<h2>` and in the MESSAGE cell.
const C1E_409_BODY: &str = "<html>\n<head>\n<meta http-equiv=\"Content-Type\" content=\"text/html;charset=ISO-8859-1\"/>\n<title>Error 409</title>\n</head>\n<body><h2>HTTP ERROR 409 Lock already held by api-token:cap-a:192.168.97.1</h2>\n<table>\n<tr><th>URI:</th><td>/data/api/v1/scan-lock/config</td></tr>\n<tr><th>STATUS:</th><td>409</td></tr>\n<tr><th>MESSAGE:</th><td>Lock already held by api-token:cap-a:192.168.97.1</td></tr>\n</table>\n\n</body>\n</html>\n";

/// C1c, sub-block "GET held #1": the held lock body, pretty-printed.
const C1C_HELD_BODY: &str = "{\n  \"timestamp\": 380308558,\n  \"remaining\": 59,\n  \"actor\": \"api-token:cap-a:192.168.97.1\"\n}";

/// C1b, sub-block "POST acquire 10/60": the acquire 200 body (the same
/// shape as the info read).
const C1B_ACQUIRE_BODY: &str = "{\n  \"timestamp\": 380308558,\n  \"remaining\": 59,\n  \"actor\": \"api-token:cap-a:192.168.97.1\"\n}";

const HOLDER: &str = "api-token:cap-a:192.168.97.1";

fn html(status: u16, body: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_raw(body.to_string(), "text/html;charset=iso-8859-1")
}

fn json(status: u16, body: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_raw(body.to_string(), "application/json")
}

/// Holder of the `ScanLockHeld` fields, or a panic naming the real error.
fn held_parts(err: CoreError) -> (Option<String>, String, Option<String>) {
    match err {
        CoreError::ScanLockHeld {
            actor,
            kind,
            endpoint,
        } => (actor, kind, endpoint),
        other => panic!("expected scan_lock_held, got {other:?}"),
    }
}

/// C1e (D1): the competing acquire is an immediate 409 whose HTML body
/// names the holder. The POST is sent exactly once.
#[tokio::test]
async fn lock_held_names_the_holder_from_the_captured_body() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("POST"))
        .and(path(LOCK_CONFIG))
        .respond_with(html(409, C1E_409_BODY))
        .expect(1)
        .mount(&mock.server)
        .await;

    let concrete = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let api: &dyn DeployApi = &concrete;
    let err = api
        .scan_lock_acquire(ScanKind::Config, 2, 5)
        .await
        .expect_err("a held lock refuses");

    assert_eq!(err.code(), "scan_lock_held");
    assert_eq!(err.exit_code(), 6);
    assert!(
        err.to_string().contains(HOLDER),
        "Display names the holder: {err}"
    );
    let (actor, kind, endpoint) = held_parts(err);
    assert_eq!(actor.as_deref(), Some(HOLDER));
    assert_eq!(kind, "config");
    assert!(
        endpoint
            .as_deref()
            .is_some_and(|e| e.ends_with("/scan-lock/config")),
        "endpoint names the refused route: {endpoint:?}"
    );
}

/// D1 declares 408 as well as 409 on this route, so a refusal with an
/// empty body takes the one-shot enrichment path: the actor comes from a
/// single follow-up GET (C1c body), and the POST is never re-sent.
#[tokio::test]
async fn lock_held_without_actor_is_enriched_from_lock_info() {
    let mock = IgnitionMock::start().await;
    let post_guard = Mock::given(method("POST"))
        .and(path(LOCK_CONFIG))
        .respond_with(ResponseTemplate::new(408))
        .expect(1)
        .mount_as_scoped(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path(LOCK_CONFIG))
        .respond_with(json(200, C1C_HELD_BODY))
        .expect(1)
        .mount(&mock.server)
        .await;

    let concrete = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let api: &dyn DeployApi = &concrete;
    let err = api
        .scan_lock_acquire(ScanKind::Config, 2, 5)
        .await
        .expect_err("a held lock refuses");

    let (actor, kind, _) = held_parts(err);
    assert_eq!(actor.as_deref(), Some(HOLDER), "enriched from C1c");
    assert_eq!(kind, "config");
    assert_eq!(
        post_guard.received_requests().await.len(),
        1,
        "the acquire POST is never retried"
    );
}

/// A refusal whose enrichment read fails still reports the held lock,
/// with no actor: the held signal is never masked.
#[tokio::test]
async fn lock_held_survives_a_failed_enrichment() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("POST"))
        .and(path(LOCK_CONFIG))
        .respond_with(ResponseTemplate::new(408))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path(LOCK_CONFIG))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&mock.server)
        .await;

    let concrete = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let api: &dyn DeployApi = &concrete;
    let err = api
        .scan_lock_acquire(ScanKind::Config, 2, 5)
        .await
        .expect_err("a held lock refuses");
    let (actor, _, _) = held_parts(err);
    assert_eq!(actor, None);
}

/// C1b: the acquire request carries exactly the two timeouts and the FULL
/// `name:key` token (T-20-10).
#[tokio::test]
async fn lock_acquire_sends_timeouts_and_full_token() {
    let mock = IgnitionMock::start().await;
    let guard = Mock::given(method("POST"))
        .and(path(LOCK_CONFIG))
        .and(header("X-Ignition-API-Token", "cap-a:tokengeneratedlive"))
        .respond_with(json(200, C1B_ACQUIRE_BODY))
        .expect(1)
        .mount_as_scoped(&mock.server)
        .await;

    let concrete = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let api: &dyn DeployApi = &concrete;
    api.scan_lock_acquire(ScanKind::Config, 10, 5)
        .await
        .expect("the acquire succeeds");

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method.as_str(), "POST");
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).expect("JSON body");
    assert_eq!(
        body,
        serde_json::json!({"acquireTimeout": 10, "holdTimeout": 5})
    );
    assert_eq!(
        std::str::from_utf8(&requests[0].body).expect("utf8"),
        r#"{"acquireTimeout":10,"holdTimeout":5}"#,
        "field order is the wire order"
    );
    let token = requests[0]
        .headers
        .get("x-ignition-api-token")
        .expect("token header sent");
    assert_eq!(token.to_str().expect("ascii"), "cap-a:tokengeneratedlive");
}

/// C1c then C1a: a held lock reads Held (with remaining and actor), then
/// a 204 reads Free.
#[tokio::test]
async fn lock_info_held_then_free() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("GET"))
        .and(path(LOCK_CONFIG))
        .respond_with(json(200, C1C_HELD_BODY))
        .up_to_n_times(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path(LOCK_CONFIG))
        .respond_with(ResponseTemplate::new(204))
        .mount(&mock.server)
        .await;

    let concrete = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let api: &dyn DeployApi = &concrete;

    let first = api.scan_lock_info(ScanKind::Config).await.expect("held");
    match &first {
        LockObservation::Held(info) => {
            assert_eq!(info.remaining, 59);
            assert_eq!(info.actor.as_deref(), Some(HOLDER));
        }
        other => panic!("expected Held, got {other:?}"),
    }
    assert_eq!(
        lock_verdict(&first, 2),
        LockVerdict::Held { remaining_s: 59 }
    );

    let second = api.scan_lock_info(ScanKind::Config).await.expect("free");
    assert_eq!(second, LockObservation::Free);
    assert_eq!(lock_verdict(&second, 2), LockVerdict::NotHeld);
}

/// DAPI-06 empty edge: a 200 with an empty body is an error naming the
/// endpoint id. It is never read as Free (only 204 is).
#[tokio::test]
async fn shape_lock_info_200_empty_body_is_an_error_not_free() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("GET"))
        .and(path(LOCK_CONFIG))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock.server)
        .await;

    let concrete = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let api: &dyn DeployApi = &concrete;
    let err = api
        .scan_lock_info(ScanKind::Config)
        .await
        .expect_err("an empty 200 is not Free");
    assert_eq!(err.code(), "internal");
    assert!(
        err.to_string().contains("scan_lock_info"),
        "names the endpoint id: {err}"
    );
}

/// The `projects` kind is wiremock-pinned only (Drift and limits): it is
/// declared by the live OpenAPI (C1h) but was not live-captured.
#[tokio::test]
async fn lock_projects_kind_uses_the_projects_path() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("GET"))
        .and(path("/data/api/v1/scan-lock/projects"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&mock.server)
        .await;

    let concrete = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let api: &dyn DeployApi = &concrete;
    let observed = api
        .scan_lock_info(ScanKind::Projects)
        .await
        .expect("projects lock info");
    assert_eq!(observed, LockObservation::Free);
}

/// D1/T-20-12: a refusal on the projects kind names the projects kind.
#[tokio::test]
async fn lock_projects_refusal_names_the_projects_kind() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("POST"))
        .and(path("/data/api/v1/scan-lock/projects"))
        .respond_with(html(409, C1E_409_BODY))
        .expect(1)
        .mount(&mock.server)
        .await;

    let concrete = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let api: &dyn DeployApi = &concrete;
    let err = api
        .scan_lock_acquire(ScanKind::Projects, 2, 5)
        .await
        .expect_err("refused");
    let (_, kind, _) = held_parts(err);
    assert_eq!(kind, "projects");
}

// ---- scan completion (Task 2) ----

const SCAN_CONFIG: &str = "/data/api/v1/scan/config";

/// C1i, sub-block "GET idle": no scan running, timestamp 1791815654089.
const C1I_IDLE: &str = "{\n  \"scanActive\": false,\n  \"lastScanTimestamp\": 1791815654089,\n  \"lastScanDuration\": 2081\n}";

/// C1i, sub-block "POST scan #1": the trigger-time snapshot, `scanActive`
/// true with the OLD timestamp. Never completion (D4).
const C1I_POST_SNAPSHOT: &str = "{\n  \"scanActive\": true,\n  \"lastScanTimestamp\": 1791815654089,\n  \"lastScanDuration\": 2081\n}";

/// C1i, poll sub-block `t+1s`: the scan finished, timestamp advanced.
const C1I_DONE: &str = "{\n  \"scanActive\": false,\n  \"lastScanTimestamp\": 1791815676996,\n  \"lastScanDuration\": 582\n}";

/// C1j, poll sub-block `t+1s`: a later finished scan, timestamp advanced again.
const C1J_DONE: &str = "{\n  \"scanActive\": false,\n  \"lastScanTimestamp\": 1791815677748,\n  \"lastScanDuration\": 246\n}";

const TS_IDLE: i64 = 1_791_815_654_089;
const TS_DONE: i64 = 1_791_815_676_996;
const TS_DONE_2: i64 = 1_791_815_677_748;

fn fast_poll(deadline_ms: u64) -> PollConfig {
    PollConfig {
        subject: "scan test".to_string(),
        interval: Duration::from_millis(20),
        max: Duration::from_millis(20),
        deadline: Duration::from_millis(deadline_ms),
    }
}

/// Count the received requests of one method on one path.
async fn count_requests(mock: &IgnitionMock, verb: &str, on: &str) -> usize {
    mock.server
        .received_requests()
        .await
        .expect("request recording is on")
        .iter()
        .filter(|req| req.method.as_str() == verb && req.url.path() == on)
        .count()
}

/// DAPI-04 / T-20-11 (C1i, D4): the trigger answers 200 but the status
/// never advances. The wait reports NotComplete, never Complete, never Err,
/// and the POST was sent exactly once.
#[tokio::test]
async fn scan_trigger_200_but_status_never_advances_is_not_complete() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("GET"))
        .and(path(SCAN_CONFIG))
        .respond_with(json(200, C1I_IDLE))
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path(SCAN_CONFIG))
        .respond_with(json(200, C1I_POST_SNAPSHOT))
        .expect(1)
        .mount(&mock.server)
        .await;

    let concrete = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let api: &dyn DeployApi = &concrete;
    let outcome = scan_and_wait(api, ScanKind::Config, fast_poll(300))
        .await
        .expect("a slow scan is not an error");

    match outcome {
        ScanOutcome::NotComplete { last, waited } => {
            assert_eq!(last.last_scan_timestamp, TS_IDLE);
            assert!(!last.scan_active);
            assert!(
                waited >= Duration::from_millis(300),
                "waited the whole budget: {waited:?}"
            );
        }
        other => panic!("expected NotComplete, got {other:?}"),
    }
    assert_eq!(count_requests(&mock, "POST", SCAN_CONFIG).await, 1);
}

/// DAPI-04: idle(T), active(T), then inactive with an advanced timestamp
/// is Complete carrying that final reading. The sequence is C1i bodies.
#[tokio::test]
async fn scan_complete_requires_inactive_and_advanced() {
    let mock = IgnitionMock::start().await;
    for (body, times) in [(C1I_IDLE, 1), (C1I_POST_SNAPSHOT, 1)] {
        Mock::given(method("GET"))
            .and(path(SCAN_CONFIG))
            .respond_with(json(200, body))
            .up_to_n_times(times)
            .mount(&mock.server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path(SCAN_CONFIG))
        .respond_with(json(200, C1I_DONE))
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path(SCAN_CONFIG))
        .respond_with(json(200, C1I_POST_SNAPSHOT))
        .expect(1)
        .mount(&mock.server)
        .await;

    let concrete = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let api: &dyn DeployApi = &concrete;
    let outcome = scan_and_wait(api, ScanKind::Config, fast_poll(5_000))
        .await
        .expect("scan completes");
    match outcome {
        ScanOutcome::Complete(done) => assert_eq!(done.last_scan_timestamp, TS_DONE),
        other => panic!("expected Complete, got {other:?}"),
    }
}

/// D4 / Pitfall 3: a scan already active when the pre-trigger status is
/// read is waited out BEFORE the trigger, and its completion is never
/// counted as ours. Sequence: active(T0), inactive(T1), [POST], inactive(T1),
/// inactive(T2); the result carries T2, and the POST follows the first
/// inactive read.
#[tokio::test]
async fn scan_foreign_scan_active_at_before_is_not_counted() {
    let mock = IgnitionMock::start().await;
    for (body, times) in [(C1I_POST_SNAPSHOT, 1), (C1I_DONE, 2)] {
        Mock::given(method("GET"))
            .and(path(SCAN_CONFIG))
            .respond_with(json(200, body))
            .up_to_n_times(times)
            .mount(&mock.server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path(SCAN_CONFIG))
        .respond_with(json(200, C1J_DONE))
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path(SCAN_CONFIG))
        .respond_with(json(200, C1I_POST_SNAPSHOT))
        .expect(1)
        .mount(&mock.server)
        .await;

    let concrete = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let api: &dyn DeployApi = &concrete;
    let outcome = scan_and_wait(api, ScanKind::Config, fast_poll(5_000))
        .await
        .expect("scan completes");
    match outcome {
        ScanOutcome::Complete(done) => assert_eq!(
            done.last_scan_timestamp, TS_DONE_2,
            "the foreign scan's completion (T1) was not counted"
        ),
        other => panic!("expected Complete, got {other:?}"),
    }

    let verbs: Vec<String> = mock
        .server
        .received_requests()
        .await
        .expect("request recording is on")
        .iter()
        .map(|req| req.method.to_string())
        .collect();
    assert_eq!(
        verbs[..3],
        ["GET", "GET", "POST"],
        "the POST follows the first inactive read: {verbs:?}"
    );
}

/// D4 (C1i, "cap-b POST scan while cap-a holds the lock"): the lock does
/// not block another actor's scan. The POST answers 200 and is accepted
/// (no classify arm on the scan route), the trigger body is ignored, and
/// the lock then reads free because a scan releases it (D12).
#[tokio::test]
async fn scan_by_other_actor_while_lock_held() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("POST"))
        .and(path(SCAN_CONFIG))
        .and(header("X-Ignition-API-Token", "cap-b:tokengeneratedlive"))
        .respond_with(json(200, C1I_POST_SNAPSHOT))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path(LOCK_CONFIG))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&mock.server)
        .await;

    let cap_b = ReqwestGatewayApi::for_tests(
        &mock.uri(),
        Some(Credential::Token(Secret::new("cap-b:tokengeneratedlive"))),
    );
    let api: &dyn DeployApi = &cap_b;
    api.scan_request(ScanKind::Config)
        .await
        .expect("the held lock does not refuse a scan");
    assert_eq!(
        api.scan_lock_info(ScanKind::Config).await.expect("lock"),
        LockObservation::Free,
        "the scan released the lock"
    );
}

/// The status read parses the captured idle body and rides the token.
#[tokio::test]
async fn scan_status_parses_the_captured_body() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("GET"))
        .and(path("/data/api/v1/scan/projects"))
        .and(header("X-Ignition-API-Token", "cap-a:tokengeneratedlive"))
        .respond_with(json(200, C1I_IDLE))
        .expect(1)
        .mount(&mock.server)
        .await;

    let concrete = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let api: &dyn DeployApi = &concrete;
    let status = api.scan_status(ScanKind::Projects).await.expect("status");
    assert!(!status.scan_active);
    assert_eq!(status.last_scan_timestamp, TS_IDLE);
    assert_eq!(status.last_scan_duration, Some(2081));
}

/// An auth failure during the wait fails fast and never reaches the
/// trigger POST.
#[tokio::test]
async fn scan_wait_auth_failure_fails_fast_without_posting() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("GET"))
        .and(path(SCAN_CONFIG))
        .respond_with(ResponseTemplate::new(401))
        .mount(&mock.server)
        .await;

    let concrete = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let api: &dyn DeployApi = &concrete;
    let err = scan_and_wait(api, ScanKind::Config, fast_poll(5_000))
        .await
        .expect_err("a rejected token cannot succeed");
    assert!(matches!(err, CoreError::Auth { .. }), "{err:?}");
    assert_eq!(count_requests(&mock, "POST", SCAN_CONFIG).await, 0);
}
