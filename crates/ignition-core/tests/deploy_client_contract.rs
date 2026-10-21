//! Wiremock contract tests for the `DeployApi` scan-lock, scan and config
//! resource calls (20-03, DAPI-03 / DAPI-04 / DAPI-06 / DAPI-07; 20-04,
//! DAPI-01 / DAPI-02 / DAPI-06).
//!
//! Capture-pinning rule: every body below is copied verbatim from
//! `.planning/phases/20-config-api-client/20-LIVE-CAPTURES.md` (live 8.3.6)
//! and each test cites the block it rests on. A request is asserted on
//! what was RECEIVED (method, body, header), never only on the answer.
//! Every call goes through `&dyn DeployApi`, which also proves the trait
//! is dyn-compatible.

mod common;

use common::IgnitionMock;
use common::deploy_scenario::{ScenarioTiming, scenario};
use common::fake_deploy::FakeGateway;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use ignition_core::client::config_resources::{ResourceCreate, ResourceUpdate};
use ignition_core::client::deploy_api::{resource_list_in_collection, scan_and_wait};
use ignition_core::client::mode::ModeCreate;
use ignition_core::client::query::ListQuery;
use ignition_core::client::scan::{
    LockObservation, LockVerdict, ScanKind, ScanOutcome, lock_verdict,
};
use ignition_core::client::{DeployApi, ReqwestGatewayApi};
use ignition_core::config::{Credential, Secret};
use ignition_core::error::{
    CoreError, GATEWAY_CLIENT_BODY_CAP_BYTES, GATEWAY_CLIENT_BODY_TRUNCATION_MARKER,
    SignatureReason,
};
use ignition_core::poll::PollConfig;
use wiremock::matchers::{header, method, path, query_param};
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

/// A 409 whose body names no holder takes the one-shot enrichment path: the actor comes from a
/// single follow-up GET (C1c body), and the POST is never re-sent.
#[tokio::test]
async fn lock_held_without_actor_is_enriched_from_lock_info() {
    let mock = IgnitionMock::start().await;
    let post_guard = Mock::given(method("POST"))
        .and(path(LOCK_CONFIG))
        .respond_with(ResponseTemplate::new(409))
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

/// WR-02: a 408 is not capture-proven and a proxy or idle timeout can
/// produce one with no lock held, so it is never `scan_lock_held`: it falls
/// to the generic unexpected-status error, with no enrichment read.
#[tokio::test]
async fn lock_408_is_not_scan_lock_held() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("POST"))
        .and(path(LOCK_CONFIG))
        .respond_with(ResponseTemplate::new(408))
        .expect(1)
        .mount(&mock.server)
        .await;
    let concrete = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let api: &dyn DeployApi = &concrete;
    let err = api
        .scan_lock_acquire(ScanKind::Config, 2, 5)
        .await
        .expect_err("a 408 is not a held lock");
    assert_ne!(err.code(), "scan_lock_held", "{err}");
    assert_eq!(requests_of(&mock).await.len(), 1, "no enrichment read");
}

/// A refusal whose enrichment read fails still reports the held lock,
/// with no actor: the held signal is never masked.
#[tokio::test]
async fn lock_held_survives_a_failed_enrichment() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("POST"))
        .and(path(LOCK_CONFIG))
        .respond_with(ResponseTemplate::new(409))
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

// ---------------------------------------------------------------------------
// 20-04 Task 1: config resource reads (DAPI-01)
// ---------------------------------------------------------------------------

const DB_FIND_BASE: &str = "/data/api/v1/resources/find/ignition/database-connection/";
/// `IGN_DEPLOY_RIG_DB_1791815701` as the client encodes it (the shared
/// `encode_segment` percent-encodes every non-alphanumeric byte).
const DB_NAME_ENC: &str = "IGN%5FDEPLOY%5FRIG%5FDB%5F1791815701";
const DB_NAME: &str = "IGN_DEPLOY_RIG_DB_1791815701";

fn find_path() -> String {
    format!("{DB_FIND_BASE}{DB_NAME_ENC}")
}

/// The captured `config` object of the C3a resource (C3a "GET find"), the
/// same keys in every copy; only `poolMaxActive` differs between copies.
fn db_config(pool_max_active: u32) -> String {
    format!(
        r#"{{"driver":"PostgreSQL","translator":"POSTGRES","includeSchemaInTableName":false,"connectURL":"jdbc:postgresql://mode-host:5432/spike","username":"spike","connectionProps":"","connectionResetParams":"","defaultTransactionLevel":"DEFAULT","poolInitSize":0,"poolMaxActive":{pool_max_active},"poolMaxIdle":8,"poolMinIdle":0,"poolMaxWait":5000,"validationQuery":"SELECT 1","testOnBorrow":true,"testOnReturn":false,"testWhileIdle":false,"evictionRate":-1,"evictionTests":3,"evictionTime":1800000,"failoverMode":"STANDARD","slowQueryLogThreshold":60000,"validationSleepTime":10000}}"#
    )
}

/// C-list "GET find, no collection (effective view)" and "GET
/// find?collection=production": the production copy. The captured
/// `metrics` and `healthchecks` blocks are elided; every other key is
/// verbatim.
fn clist_production_body() -> String {
    format!(
        r#"{{"type":"ignition/database-connection","name":"{DB_NAME}","description":"c-list production override","enabled":true,"version":1,"collection":"production","collections":["core","production"],"signature":"c3ee2b70350a263381b5c1a4f92c3cdacb26ea949f255a48ead9a468aaeafcb9","config":{},"data":["config.json"],"attributes":{{"uuid":"ef39fc66-c7b4-441a-a907-b02df07b17aa","enabled":true}},"metrics":{{}},"healthchecks":{{}}}}"#,
        db_config(20)
    )
}

/// C-list "GET find?collection=core": the core copy, a different
/// description, config and signature than the effective one.
fn clist_core_body() -> String {
    format!(
        r#"{{"type":"ignition/database-connection","name":"{DB_NAME}","description":"c3a update","enabled":true,"version":1,"collection":"core","collections":["core","production"],"signature":"8e01629a0ccc4115d874626df494edf5cdb1cd7510789a4ce9cc39cc85063f14","config":{},"data":["config.json"],"attributes":{{"lastModification":{{"actor":"api-token:cap-a:192.168.97.1","timestamp":"2026-10-12T14:35:03Z"}},"uuid":"0939907f-c2a5-44d8-ac13-2a8d252f312f","lastModificationSignature":"ffc636164e849e022d4eee0e76ab409a129a36e526d354c2609eab2615128172","enabled":true}},"metrics":{{}},"healthchecks":{{}}}}"#,
        db_config(9)
    )
}

/// C-list "GET names", verbatim.
const CLIST_NAMES_BODY: &str = r#"{"items":[{"name":"IGN_DEPLOY_RIG_DB_1791815701","enabled":true,"modes":["core","production"]}],"metadata":{"total":1,"matching":1,"limit":-1,"offset":0}}"#;

/// C-list "GET singleton security-properties (effective)": no `name` key.
/// The config is cut to its first scalar keys; the rest is verbatim shape.
const CLIST_SINGLETON_BODY: &str = r#"{"type":"ignition/security-properties","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"cac5b33f86d5ffca2b1f8ad1486e8da8ccde9bcd14d244bc2d4b970f61cb5de8","config":{"systemAuthProfile":"default","systemIdentityProvider":"default","forceIdpAuth":false,"designerAuthStrategy":"CLASSIC"},"data":["config.json"],"attributes":{},"metrics":{},"healthchecks":{}}"#;

/// C3a "GET find of the unknown name": 404 with Jetty's HTML page.
const C3A_FIND_404_BODY: &str = "<html>\n<head>\n<meta http-equiv=\"Content-Type\" content=\"text/html;charset=ISO-8859-1\"/>\n<title>Error 404</title>\n</head>\n<body><h2>HTTP ERROR 404 Resource not found: ignition/database-connection/IGN_DEPLOY_RIG_DB_1791815701_NOPE</h2>\n<table>\n<tr><th>URI:</th><td>/data/api/v1/resources/find/ignition/database-connection/IGN_DEPLOY_RIG_DB_1791815701_NOPE</td></tr>\n<tr><th>STATUS:</th><td>404</td></tr>\n<tr><th>MESSAGE:</th><td>Resource not found: ignition/database-connection/IGN_DEPLOY_RIG_DB_1791815701_NOPE</td></tr>\n</table>\n\n</body>\n</html>\n";

fn api_for(mock: &IgnitionMock) -> ReqwestGatewayApi {
    ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()))
}

async fn requests_of(mock: &IgnitionMock) -> Vec<wiremock::Request> {
    mock.server
        .received_requests()
        .await
        .expect("request recording is on")
}

/// C-list "GET find, no collection (effective view)": no `collection` key
/// goes out, and the production body parses.
#[tokio::test]
async fn resource_find_effective_view_sends_no_collection() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("GET"))
        .and(path(find_path()))
        .respond_with(json(200, &clist_production_body()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    let record = api
        .resource_find("ignition", "database-connection", DB_NAME, None)
        .await
        .expect("find");

    assert_eq!(record.name.as_deref(), Some(DB_NAME));
    assert_eq!(record.collection.as_deref(), Some("production"));
    assert_eq!(record.collections, vec!["core", "production"]);
    assert_eq!(record.resource_type, "ignition/database-connection");
    assert_eq!(record.config["poolMaxActive"], 20);
    assert!(
        record
            .signature
            .as_deref()
            .is_some_and(|s| s.starts_with("c3ee2b70"))
    );
    let seen = requests_of(&mock).await;
    assert_eq!(seen.len(), 1);
    assert!(
        !seen[0].url.query_pairs().any(|(k, _)| k == "collection"),
        "the effective view sends no collection: {:?}",
        seen[0].url.query()
    );
}

/// C-list "GET find?collection=core" and "...=production": exactly
/// `collection=<c>` goes out, and the two copies differ.
#[tokio::test]
async fn resource_find_named_collection_sends_collection_param() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("GET"))
        .and(path(find_path()))
        .and(query_param("collection", "production"))
        .respond_with(json(200, &clist_production_body()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path(find_path()))
        .and(query_param("collection", "core"))
        .respond_with(json(200, &clist_core_body()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    let production = api
        .resource_find(
            "ignition",
            "database-connection",
            DB_NAME,
            Some("production"),
        )
        .await
        .expect("production copy");
    let core = api
        .resource_find("ignition", "database-connection", DB_NAME, Some("core"))
        .await
        .expect("core copy");

    assert_eq!(production.collection.as_deref(), Some("production"));
    assert_eq!(core.collection.as_deref(), Some("core"));
    assert_ne!(
        production.signature, core.signature,
        "each copy has its own"
    );
    assert_ne!(production.description, core.description);
    assert_eq!(core.config["poolMaxActive"], 9);
    for req in requests_of(&mock).await {
        assert_eq!(
            req.url.query_pairs().count(),
            1,
            "only collection: {:?}",
            req.url.query()
        );
    }
}

/// C-list "GET list?limit=-1": `limit=-1` and `offset=0` go out, and the
/// items keep the gateway's order (Z before A is NOT re-sorted).
#[tokio::test]
async fn resource_list_sends_limit_and_offset() {
    let mock = IgnitionMock::start().await;
    let body = r#"{"items":[{"type":"ignition/database-connection","name":"Z","collection":"core","collections":["core"],"signature":"aa","config":{}},{"type":"ignition/database-connection","name":"A","collection":"core","collections":["core"],"signature":"bb","config":{}}],"metadata":{"total":2,"matching":2,"limit":-1,"offset":0}}"#;
    Mock::given(method("GET"))
        .and(path(
            "/data/api/v1/resources/list/ignition/database-connection",
        ))
        .respond_with(json(200, body))
        .expect(1)
        .mount(&mock.server)
        .await;

    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    let page = api
        .resource_list("ignition", "database-connection", &ListQuery::default())
        .await
        .expect("list");

    let names: Vec<_> = page
        .items
        .iter()
        .map(|r| r.name.as_deref().unwrap())
        .collect();
    assert_eq!(names, vec!["Z", "A"], "gateway order is kept");
    assert_eq!(page.metadata.total, 2);
    let seen = requests_of(&mock).await;
    let pairs: Vec<(String, String)> = seen[0]
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(
        pairs,
        vec![
            ("limit".to_string(), "-1".to_string()),
            ("offset".to_string(), "0".to_string())
        ]
    );
}

/// C-list "GET list?limit=-1&collection[eq]=production (object-style
/// filter)" and Decision D7: a filter goes out as its own `field[op]`
/// parameter, never as `filter=`.
#[tokio::test]
async fn resource_list_filter_is_sent_object_style() {
    let mock = IgnitionMock::start().await;
    let body = format!(
        r#"{{"items":[{}],"metadata":{{"total":1,"matching":1,"limit":-1,"offset":0}}}}"#,
        clist_production_body()
    );
    Mock::given(method("GET"))
        .and(path(
            "/data/api/v1/resources/list/ignition/database-connection",
        ))
        .and(query_param("name[eq]", DB_NAME))
        .respond_with(json(200, &body))
        .expect(1)
        .mount(&mock.server)
        .await;

    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    let q = ListQuery {
        filter: Some(format!("name[eq]={DB_NAME}")),
        ..ListQuery::default()
    };
    let page = api
        .resource_list("ignition", "database-connection", &q)
        .await
        .expect("filtered list");
    assert_eq!(page.items.len(), 1);
    let seen = requests_of(&mock).await;
    assert!(!seen[0].url.query_pairs().any(|(k, _)| k == "filter"));

    // The form that matches nothing on the gateway is refused up front.
    let bad = ListQuery {
        filter: Some("not-a-filter".to_string()),
        ..ListQuery::default()
    };
    let err = api
        .resource_list("ignition", "database-connection", &bad)
        .await
        .expect_err("malformed filter");
    assert_eq!(err.code(), "invalid_input");
    assert_eq!(
        requests_of(&mock).await.len(),
        1,
        "no request for the bad filter"
    );
}

/// An empty type is Ok with no items.
#[tokio::test]
async fn resource_list_empty_is_ok() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/data/api/v1/resources/list/ignition/database-connection",
        ))
        .respond_with(json(
            200,
            r#"{"items":[],"metadata":{"total":0,"matching":0,"limit":-1,"offset":0}}"#,
        ))
        .expect(1)
        .mount(&mock.server)
        .await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    let page = api
        .resource_list("ignition", "database-connection", &ListQuery::default())
        .await
        .expect("empty list is Ok");
    assert!(page.items.is_empty());
}

/// C3a "GET find of the unknown name": a 404 is `not_found`, exit 6.
#[tokio::test]
async fn resource_find_absent_is_not_found() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("GET"))
        .and(path(format!("{DB_FIND_BASE}{DB_NAME_ENC}%5FNOPE")))
        .respond_with(html(404, C3A_FIND_404_BODY))
        .expect(1)
        .mount(&mock.server)
        .await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    let err = api
        .resource_find(
            "ignition",
            "database-connection",
            &format!("{DB_NAME}_NOPE"),
            None,
        )
        .await
        .expect_err("absent name");
    assert_eq!(err.code(), "not_found");
    assert_eq!(err.exit_code(), 6);
}

/// C-list "GET names", verbatim.
#[tokio::test]
async fn resource_names_parses_captured_body() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/data/api/v1/resources/names/ignition/database-connection",
        ))
        .respond_with(json(200, CLIST_NAMES_BODY))
        .expect(1)
        .mount(&mock.server)
        .await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    let names = api
        .resource_names("ignition", "database-connection")
        .await
        .expect("names");
    assert_eq!(names.items.len(), 1);
    assert_eq!(names.items[0].name, DB_NAME);
    assert_eq!(names.items[0].modes, vec!["core", "production"]);
    assert_eq!(names.items[0].enabled, Some(true));
    assert_eq!(names.metadata.matching, 1);
}

/// C-list "GET singleton security-properties?collection=core": the
/// collection goes out and the body (which has no `name`) parses.
#[tokio::test]
async fn resource_singleton_with_collection() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/data/api/v1/resources/singleton/ignition/security-properties",
        ))
        .and(query_param("collection", "core"))
        .respond_with(json(200, CLIST_SINGLETON_BODY))
        .expect(1)
        .mount(&mock.server)
        .await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    let record = api
        .resource_singleton("ignition", "security-properties", Some("core"))
        .await
        .expect("singleton");
    assert_eq!(record.name, None, "a singleton has no name");
    assert_eq!(record.collection.as_deref(), Some("core"));
    assert_eq!(record.config["systemAuthProfile"], "default");
}

/// T-20-16: a malformed module or type is refused with zero requests.
#[tokio::test]
async fn resource_segment_validation_refuses_before_any_request() {
    let mock = IgnitionMock::start().await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    let q = ListQuery::default();
    for (module, ty) in [
        ("../etc", "database-connection"),
        ("ignition", "a/b"),
        ("", "database-connection"),
        ("ignition", "."),
        ("ignition", ".."),
        ("db conn", "database-connection"),
    ] {
        let errs = [
            api.resource_names(module, ty).await.map(|_| ()),
            api.resource_list(module, ty, &q).await.map(|_| ()),
            api.resource_find(module, ty, "x", None).await.map(|_| ()),
            api.resource_singleton(module, ty, None).await.map(|_| ()),
        ];
        for result in errs {
            let err = result.expect_err("refused");
            assert_eq!(err.code(), "invalid_input", "{module:?}/{ty:?}");
            assert_eq!(err.exit_code(), 2);
        }
    }
    let err = api
        .resource_find("ignition", "database-connection", "", None)
        .await
        .expect_err("empty name");
    assert_eq!(err.code(), "invalid_input");
    assert!(requests_of(&mock).await.is_empty(), "zero requests");
}

/// The name is percent-encoded from its UTF-8 bytes; a module id keeps its
/// dots.
#[tokio::test]
async fn resource_name_is_percent_encoded() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("GET"))
        .respond_with(json(200, &clist_core_body()))
        .expect(1)
        .mount(&mock.server)
        .await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    api.resource_find("com.inductiveautomation.eam", "eam-tasks", "a/b c é", None)
        .await
        .expect("find");
    let seen = requests_of(&mock).await;
    assert_eq!(
        seen[0].url.path(),
        "/data/api/v1/resources/find/com.inductiveautomation.eam/eam-tasks/a%2Fb%20c%20%C3%A9"
    );
}

/// Decision D7: a collection is listed by `names` (its `modes`) then
/// `find?collection=` per name. A name the collection does not hold is
/// skipped (by `modes`, or by a `not_found`), a copy stamped with another
/// collection is never returned as this one's, and names order is kept.
#[tokio::test]
async fn resource_list_in_collection_composes_names_and_find() {
    let mock = IgnitionMock::start().await;
    let names = r#"{"items":[{"name":"B","enabled":true,"modes":["core","production"]},{"name":"CORE_ONLY","enabled":true,"modes":["core"]},{"name":"GONE","enabled":true,"modes":["production"]},{"name":"FALLBACK","enabled":true,"modes":["production"]},{"name":"A","enabled":true,"modes":["production"]}],"metadata":{"total":5,"matching":5,"limit":-1,"offset":0}}"#;
    Mock::given(method("GET"))
        .and(path(
            "/data/api/v1/resources/names/ignition/database-connection",
        ))
        .respond_with(json(200, names))
        .expect(1)
        .mount(&mock.server)
        .await;
    let record = |name: &str, collection: &str| {
        format!(
            r#"{{"type":"ignition/database-connection","name":"{name}","collection":"{collection}","collections":["core","production"],"signature":"sig-{name}","config":{{}}}}"#
        )
    };
    // `B` is the first name, then `A`: names order, not alphabetical.
    // Alphanumeric names are not altered by the segment encoding.
    for (name, body) in [
        ("B", record("B", "production")),
        ("A", record("A", "production")),
        ("FALLBACK", record("FALLBACK", "core")),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("{DB_FIND_BASE}{name}")))
            .and(query_param("collection", "production"))
            .respond_with(json(200, &body))
            .expect(1)
            .mount(&mock.server)
            .await;
    }
    // `GONE` answers 404: absent from the collection after all.
    Mock::given(method("GET"))
        .and(path(format!("{DB_FIND_BASE}GONE")))
        .respond_with(html(404, C3A_FIND_404_BODY))
        .expect(1)
        .mount(&mock.server)
        .await;

    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    let found = resource_list_in_collection(api, "ignition", "database-connection", "production")
        .await
        .expect("composed list");
    let names: Vec<_> = found.iter().map(|r| r.name.as_deref().unwrap()).collect();
    assert_eq!(
        names,
        vec!["B", "A"],
        "names order kept, absent copies skipped"
    );
    let finds = requests_of(&mock)
        .await
        .iter()
        .filter(|r| r.url.path().contains("/find/"))
        .count();
    assert_eq!(finds, 4, "CORE_ONLY is skipped by modes with no request");

    let err = resource_list_in_collection(api, "ignition", "database-connection", "")
        .await
        .expect_err("empty collection");
    assert_eq!(err.code(), "invalid_input");
}

// ---------------------------------------------------------------------------
// 20-04 Task 2: signature-checked writes (DAPI-02) and the 422 cap (DAPI-06)
// ---------------------------------------------------------------------------

const WRITE_PATH: &str = "/data/api/v1/resources/ignition/database-connection";

/// The signature a test sends: 64 hex characters, none of which may ever
/// appear in an error.
const SENT_SIG: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// C3a "POST create": 200, `success:true`, one change with `newSignature`.
const C3A_CREATE_200: &str = r#"{"success":true,"changes":[{"name":"IGN_DEPLOY_RIG_DB_1791815701","type":"ignition/database-connection","collection":"core","newSignature":"5de3e2f09a55346efa902ca0369b55829507e18e8d64d10c82d8a5e1eb9ea57c"}],"problem":null}"#;

/// C3a "PUT with the right signature": 200 with the rotated signature.
const C3A_PUT_200: &str = r#"{"success":true,"changes":[{"name":"IGN_DEPLOY_RIG_DB_1791815701","type":"ignition/database-connection","collection":"core","newSignature":"8e01629a0ccc4115d874626df494edf5cdb1cd7510789a4ce9cc39cc85063f14"}],"problem":null}"#;

/// C3a "POST the same name again (duplicate)": 409, JSON `problem`. Only the
/// first two of the roughly 55 captured stack frames are kept.
const C3A_DUPLICATE_409: &str = r#"{"success":false,"changes":[],"problem":{"message":"CREATE conflict: 'ResourceId{resourcePath=ignition/database-connection/IGN_DEPLOY_RIG_DB_1791815701, collectionName=core}' already exists","stacktrace":["com.inductiveautomation.ignition.common.resourcecollection.PushConflictException: CREATE conflict: 'ResourceId{resourcePath=ignition/database-connection/IGN_DEPLOY_RIG_DB_1791815701, collectionName=core}' already exists","\tat com.inductiveautomation.ignition.gateway.resourcecollection.ChangeOperationValidationHandler$AtomicPushValidationHandler.throwIfInvalid(ChangeOperationValidationHandler.java:58)"]}}"#;

/// C3a "PUT with a stale (64-hex wrong) signature": 500, JSON `problem`.
/// Only the first two of the roughly 55 captured stack frames are kept.
const C3A_STALE_500: &str = r#"{"success":false,"changes":[],"problem":{"message":"MODIFY illegal: signature mismatch for 'ResourceId{resourcePath=ignition/database-connection/IGN_DEPLOY_RIG_DB_1791815701, collectionName=core}'","stacktrace":["com.inductiveautomation.ignition.common.resourcecollection.PushException: MODIFY illegal: signature mismatch for 'ResourceId{resourcePath=ignition/database-connection/IGN_DEPLOY_RIG_DB_1791815701, collectionName=core}'","\tat com.inductiveautomation.ignition.gateway.resourcecollection.ChangeOperationValidationHandler$AtomicPushValidationHandler.throwIfInvalid(ChangeOperationValidationHandler.java:61)"]}}"#;

/// C3a "PUT with the signature key omitted" (byte-identical to the
/// empty-string case): 422 with `fieldName` `signature`.
const C3A_MISSING_422: &str = "{\n  \"messages\": [],\n  \"fieldMessages\": [\n    {\n      \"fieldName\": \"signature\",\n      \"messages\": [\n        \"Missing \\u0027signature\\u0027 parameter\"\n      ]\n    }\n  ]\n}";

fn config_value() -> serde_json::Value {
    serde_json::from_str(&db_config(9)).expect("captured config parses")
}

fn update_item(name: &str, signature: &str) -> ResourceUpdate {
    ResourceUpdate::new(name, signature, config_value()).expect("a non-empty signature")
}

/// Every text an error exposes: Display, Debug-free hint and endpoint.
fn exposed(err: &CoreError) -> String {
    format!(
        "{err}\n{}\n{}",
        err.hint().unwrap_or_default(),
        err.endpoint().unwrap_or_default()
    )
}

fn assert_no_leak(err: &CoreError) {
    let text = exposed(err);
    for forbidden in [
        SENT_SIG,
        "ResourceId{",
        "MODIFY illegal",
        "stacktrace",
        "com.inductive",
    ] {
        assert!(!text.contains(forbidden), "{forbidden:?} leaked: {text}");
    }
}

fn body_of(req: &wiremock::Request) -> serde_json::Value {
    serde_json::from_slice(&req.body).expect("the request body is JSON")
}

/// C3a "POST create" + D10: a one-element array with `name` and `config`
/// only (no `signature`, no null-valued key); the 200 body parses.
#[tokio::test]
async fn resource_create_posts_an_array_body() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("POST"))
        .and(path(WRITE_PATH))
        .respond_with(json(200, C3A_CREATE_200))
        .expect(1)
        .mount(&mock.server)
        .await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;

    let outcome = api
        .resource_create(
            "ignition",
            "database-connection",
            &[ResourceCreate::new(DB_NAME, config_value())],
        )
        .await
        .expect("create");

    assert!(outcome.success);
    assert!(outcome.changes[0].new_signature.is_some());
    let seen = requests_of(&mock).await;
    assert_eq!(
        body_of(&seen[0]),
        serde_json::json!([{ "name": DB_NAME, "config": config_value() }]),
        "exactly name and config"
    );
    assert!(
        seen[0]
            .headers
            .get("x-ignition-api-token")
            .is_some_and(|v| v.as_bytes() == b"cap-a:tokengeneratedlive"),
        "the write is authenticated"
    );
}

/// C3a "PUT with the right signature": a PUT whose array item carries
/// `name`, `signature` and `config`.
#[tokio::test]
async fn resource_update_puts_an_array_with_signature() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("PUT"))
        .and(path(WRITE_PATH))
        .respond_with(json(200, C3A_PUT_200))
        .expect(1)
        .mount(&mock.server)
        .await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;

    let outcome = api
        .resource_update(
            "ignition",
            "database-connection",
            &[update_item(DB_NAME, SENT_SIG)],
        )
        .await
        .expect("update");

    assert!(outcome.success);
    assert!(
        outcome.changes[0]
            .new_signature
            .as_deref()
            .is_some_and(|s| s.starts_with("8e01629a"))
    );
    let seen = requests_of(&mock).await;
    let body = body_of(&seen[0]);
    let items = body.as_array().expect("an array body");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["name"], DB_NAME);
    assert_eq!(items[0]["signature"], SENT_SIG);
    assert_eq!(items[0]["config"], config_value());
}

/// D10 (C3a "PUT of an unknown name"): a bare 404 is `not_found`, exit 6.
/// This is the 8.3.6 confirmation of DAPI-02 as amended: update is the
/// signature-checked PUT, and it cannot create.
#[tokio::test]
async fn resource_update_unknown_name_is_not_found() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("PUT"))
        .and(path(WRITE_PATH))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&mock.server)
        .await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    let err = api
        .resource_update(
            "ignition",
            "database-connection",
            &[update_item("NOPE", SENT_SIG)],
        )
        .await
        .expect_err("unknown name");
    assert_eq!(err.code(), "not_found");
    assert_eq!(err.exit_code(), 6);
}

/// D3 stale: 500 with `signature mismatch` in the problem message.
#[tokio::test]
async fn signature_stale_is_resource_signature_rejected() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("PUT"))
        .and(path(WRITE_PATH))
        .respond_with(json(500, C3A_STALE_500))
        .expect(1)
        .mount(&mock.server)
        .await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    let err = api
        .resource_update(
            "ignition",
            "database-connection",
            &[update_item(DB_NAME, SENT_SIG)],
        )
        .await
        .expect_err("stale");

    assert_eq!(err.code(), "resource_signature_rejected");
    assert_eq!(err.exit_code(), 6);
    assert_no_leak(&err);
    match err {
        CoreError::ResourceSignatureRejected {
            name,
            reason,
            endpoint,
        } => {
            assert_eq!(name, DB_NAME, "named from the gateway's resource path");
            assert_eq!(reason, SignatureReason::Stale);
            assert!(endpoint.is_some_and(|e| e.ends_with(WRITE_PATH)));
        }
        other => panic!("expected the typed refusal, got {other:?}"),
    }
}

/// 8.3.3 problem messages carry the live signature; even then it is never
/// echoed (T-20-17).
#[tokio::test]
async fn signature_stale_message_carrying_the_signature_does_not_leak() {
    let mock = IgnitionMock::start().await;
    let body = format!(
        r#"{{"success":false,"changes":[],"problem":{{"message":"MODIFY illegal: signature mismatch for {SENT_SIG} ResourceId{{x}}","stacktrace":[]}}}}"#
    );
    Mock::given(method("PUT"))
        .and(path(WRITE_PATH))
        .respond_with(json(500, &body))
        .expect(1)
        .mount(&mock.server)
        .await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    let err = api
        .resource_update(
            "ignition",
            "database-connection",
            &[update_item("A", SENT_SIG), update_item("B", SENT_SIG)],
        )
        .await
        .expect_err("stale");
    assert_eq!(err.code(), "resource_signature_rejected");
    assert_no_leak(&err);
    assert!(err.to_string().contains("<batch of 2>"), "{err}");
}

/// An empty or whitespace-only signature is refused before any request.
#[tokio::test]
async fn signature_empty_is_refused_before_any_request() {
    let mock = IgnitionMock::start().await;
    for empty in ["", "   ", "\t\n"] {
        let err = ResourceUpdate::new("DB_A", empty, serde_json::json!({}))
            .expect_err("an empty signature cannot build an update");
        assert_eq!(err.code(), "resource_signature_rejected");
        assert_eq!(err.exit_code(), 6);
        assert!(matches!(
            err,
            CoreError::ResourceSignatureRejected {
                reason: SignatureReason::Missing,
                ..
            }
        ));
    }
    let unsigned = ignition_core::client::config_resources::ResourceRecord {
        resource_type: "ignition/database-connection".to_string(),
        name: Some("DB_A".to_string()),
        description: None,
        enabled: None,
        version: None,
        collection: None,
        collections: vec![],
        signature: None,
        config: serde_json::json!({}),
        backup_config: None,
        attributes: None,
        extra: Default::default(),
    };
    let err = ResourceUpdate::from_record(&unsigned).expect_err("no signature on the record");
    assert_eq!(err.code(), "resource_signature_rejected");
    assert!(requests_of(&mock).await.is_empty(), "zero requests");
}

/// D3 omitted/empty on the wire: one byte-identical 422 naming the field
/// `signature`; it is distinguishable from stale and maps to Missing.
#[tokio::test]
async fn signature_missing_on_wire_maps_per_d3() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("PUT"))
        .and(path(WRITE_PATH))
        .respond_with(json(422, C3A_MISSING_422))
        .expect(1)
        .mount(&mock.server)
        .await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    let err = api
        .resource_update(
            "ignition",
            "database-connection",
            &[update_item(DB_NAME, SENT_SIG)],
        )
        .await
        .expect_err("missing");
    assert_eq!(err.code(), "resource_signature_rejected");
    assert!(matches!(
        err,
        CoreError::ResourceSignatureRejected {
            reason: SignatureReason::Missing,
            ..
        }
    ));
    assert_no_leak(&err);
}

/// A 422 that does not name `signature` stays the resource-body
/// `invalid_input` (classify's mapping, unchanged).
#[tokio::test]
async fn resource_update_other_422_is_invalid_input() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("PUT"))
        .and(path(WRITE_PATH))
        .respond_with(json(
            422,
            r#"{"messages":["Settings cannot be null"],"fieldMessages":[]}"#,
        ))
        .expect(1)
        .mount(&mock.server)
        .await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    let err = api
        .resource_update(
            "ignition",
            "database-connection",
            &[update_item(DB_NAME, SENT_SIG)],
        )
        .await
        .expect_err("422");
    assert_eq!(err.code(), "invalid_input");
    assert!(err.to_string().contains("Settings cannot be null"));
}

/// A 2xx whose body says `success:false` is an error (T-20-19): Stale when
/// the problem carries the marker, `internal` otherwise, with no problem
/// text in either.
#[tokio::test]
async fn resource_update_2xx_success_false_is_an_error() {
    let mock = IgnitionMock::start().await;
    let stale_guard = Mock::given(method("PUT"))
        .and(path(WRITE_PATH))
        .respond_with(json(200, C3A_STALE_500))
        .expect(1)
        .mount_as_scoped(&mock.server)
        .await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    let items = [update_item(DB_NAME, SENT_SIG)];
    let err = api
        .resource_update("ignition", "database-connection", &items)
        .await
        .expect_err("success:false is an error");
    assert!(matches!(
        err,
        CoreError::ResourceSignatureRejected {
            reason: SignatureReason::Stale,
            ..
        }
    ));
    assert_no_leak(&err);
    drop(stale_guard);

    Mock::given(method("PUT"))
        .and(path(WRITE_PATH))
        .respond_with(json(
            200,
            r#"{"success":false,"changes":[],"problem":{"message":"boom 0123456789abcdef","stacktrace":[]}}"#,
        ))
        .expect(1)
        .mount(&mock.server)
        .await;
    let err = api
        .resource_update("ignition", "database-connection", &items)
        .await
        .expect_err("success:false is an error");
    assert_eq!(err.code(), "internal");
    assert!(!err.to_string().contains("boom"), "{err}");
    assert_no_leak(&err);
}

/// Any other 500 is `internal`, with the problem text withheld.
#[tokio::test]
async fn resource_update_500_other_is_internal_without_problem_text() {
    let mock = IgnitionMock::start().await;
    let body = format!(
        r#"{{"success":false,"changes":[],"problem":{{"message":"boom {SENT_SIG}","stacktrace":["at x"]}}}}"#
    );
    Mock::given(method("PUT"))
        .and(path(WRITE_PATH))
        .respond_with(json(500, &body))
        .expect(1)
        .mount(&mock.server)
        .await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    let err = api
        .resource_update(
            "ignition",
            "database-connection",
            &[update_item(DB_NAME, SENT_SIG)],
        )
        .await
        .expect_err("500");
    assert_eq!(err.code(), "internal");
    assert!(!err.to_string().contains("boom"), "{err}");
    assert_no_leak(&err);
}

/// Writes are sent once per call, a 500 included (T-20-20).
#[tokio::test]
async fn resource_write_is_never_retried() {
    let mock = IgnitionMock::start().await;
    let put = Mock::given(method("PUT"))
        .and(path(WRITE_PATH))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount_as_scoped(&mock.server)
        .await;
    let post = Mock::given(method("POST"))
        .and(path(WRITE_PATH))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount_as_scoped(&mock.server)
        .await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    assert!(
        api.resource_update(
            "ignition",
            "database-connection",
            &[update_item(DB_NAME, SENT_SIG)]
        )
        .await
        .is_err()
    );
    assert!(
        api.resource_create(
            "ignition",
            "database-connection",
            &[ResourceCreate::new(DB_NAME, config_value())]
        )
        .await
        .is_err()
    );
    assert_eq!(put.received_requests().await.len(), 1);
    assert_eq!(post.received_requests().await.len(), 1);
}

/// An empty batch is `invalid_input`, exit 2, with zero requests.
#[tokio::test]
async fn resource_items_empty_is_refused() {
    let mock = IgnitionMock::start().await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    for result in [
        api.resource_create("ignition", "database-connection", &[])
            .await,
        api.resource_update("ignition", "database-connection", &[])
            .await,
    ] {
        let err = result.expect_err("empty batch");
        assert_eq!(err.code(), "invalid_input");
        assert_eq!(err.exit_code(), 2);
    }
    let err = api
        .resource_create(
            "../x",
            "database-connection",
            &[ResourceCreate::new("A", serde_json::json!({}))],
        )
        .await
        .expect_err("bad module");
    assert_eq!(err.code(), "invalid_input");
    let err = api
        .resource_create(
            "ignition",
            "database-connection",
            &[ResourceCreate::new("", serde_json::json!({}))],
        )
        .await
        .expect_err("empty name");
    assert_eq!(err.code(), "invalid_input");
    assert!(requests_of(&mock).await.is_empty(), "zero requests");
}

/// D10: the duplicate create is a 409 JSON problem and is never Ok; the
/// problem text and stack are not echoed.
#[tokio::test]
async fn resource_duplicate_create_is_never_ok() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("POST"))
        .and(path(WRITE_PATH))
        .respond_with(json(409, C3A_DUPLICATE_409))
        .expect(1)
        .mount(&mock.server)
        .await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    let err = api
        .resource_create(
            "ignition",
            "database-connection",
            &[ResourceCreate::new(DB_NAME, config_value())],
        )
        .await
        .expect_err("a duplicate is never Ok");
    assert_eq!(err.code(), "resource_exists");
    assert_eq!(err.exit_code(), 6);
    assert!(
        err.to_string().contains(DB_NAME),
        "names the resource: {err}"
    );
    assert!(!err.to_string().contains("com.inductive"), "{err}");
    assert!(!err.to_string().contains("ResourceId{"), "{err}");
}

/// Each write rotates the signature: PUT with S1 succeeds and answers S2; a
/// second PUT reusing S1 is the stale refusal.
#[tokio::test]
async fn resource_stale_after_successful_write() {
    let mock = IgnitionMock::start().await;
    let written = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&written);
    Mock::given(method("PUT"))
        .and(path(WRITE_PATH))
        .respond_with(move |req: &wiremock::Request| {
            let body: serde_json::Value = serde_json::from_slice(&req.body).expect("json");
            let sent = body[0]["signature"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            if sent == SENT_SIG && !flag.swap(true, Ordering::SeqCst) {
                ResponseTemplate::new(200).set_body_raw(C3A_PUT_200, "application/json")
            } else {
                ResponseTemplate::new(500).set_body_raw(C3A_STALE_500, "application/json")
            }
        })
        .expect(2)
        .mount(&mock.server)
        .await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    let items = [update_item(DB_NAME, SENT_SIG)];

    let first = api
        .resource_update("ignition", "database-connection", &items)
        .await
        .expect("first write");
    let rotated = first.changes[0]
        .new_signature
        .clone()
        .expect("newSignature");
    assert_ne!(rotated, SENT_SIG);

    let err = api
        .resource_update("ignition", "database-connection", &items)
        .await
        .expect_err("reusing the pre-write signature");
    assert!(matches!(
        err,
        CoreError::ResourceSignatureRejected {
            reason: SignatureReason::Stale,
            ..
        }
    ));
}

/// Batch bodies keep caller order; nothing is sorted client-side.
#[tokio::test]
async fn resource_batch_order_is_preserved() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("PUT"))
        .and(path(WRITE_PATH))
        .respond_with(json(
            200,
            r#"{"success":true,"changes":[{"name":"B","type":"t","collection":"core","newSignature":"s1"},{"name":"A","type":"t","collection":"core","newSignature":"s2"}],"problem":null}"#,
        ))
        .expect(1)
        .mount(&mock.server)
        .await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    let outcome = api
        .resource_update(
            "ignition",
            "database-connection",
            &[update_item("B", SENT_SIG), update_item("A", SENT_SIG)],
        )
        .await
        .expect("batch");
    let order: Vec<_> = outcome.changes.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(order, vec!["B", "A"], "changes keep gateway order");
    let seen = requests_of(&mock).await;
    let sent: Vec<_> = body_of(&seen[0])
        .as_array()
        .expect("array")
        .iter()
        .map(|item| item["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(sent, vec!["B", "A"], "the body keeps caller order");
}

/// DAPI-06: the 422 echo is capped on both the joined `messages` and the
/// raw body, and a body of exactly the cap is echoed whole.
#[tokio::test]
async fn cap_422_body_is_truncated() {
    let big = "x".repeat(100 * 1024);
    let joined_body = format!(r#"{{"messages":["{big}"],"fieldMessages":[]}}"#);
    let exact = "y".repeat(GATEWAY_CLIENT_BODY_CAP_BYTES);
    let one_over = "z".repeat(GATEWAY_CLIENT_BODY_CAP_BYTES + 1);

    for (label, body, expect_marker) in [
        ("100 KB messages", joined_body, true),
        ("100 KB raw text", big.clone(), true),
        ("exactly the cap, raw", exact.clone(), false),
        ("one byte over the cap, raw", one_over, true),
    ] {
        let mock = IgnitionMock::start().await;
        Mock::given(method("PUT"))
            .and(path(WRITE_PATH))
            .respond_with(ResponseTemplate::new(422).set_body_raw(body.clone(), "text/plain"))
            .expect(1)
            .mount(&mock.server)
            .await;
        let concrete = api_for(&mock);
        let api: &dyn DeployApi = &concrete;
        let err = api
            .resource_update(
                "ignition",
                "database-connection",
                &[update_item(DB_NAME, SENT_SIG)],
            )
            .await
            .expect_err(label);
        let CoreError::InvalidInput { reason } = err else {
            panic!("{label}: expected invalid_input");
        };
        let (_, echoed) = reason
            .split_once("): ")
            .expect("the reason carries the echo");
        assert!(
            echoed.len()
                <= GATEWAY_CLIENT_BODY_CAP_BYTES + GATEWAY_CLIENT_BODY_TRUNCATION_MARKER.len(),
            "{label}: {} bytes",
            echoed.len()
        );
        assert_eq!(
            echoed.ends_with(GATEWAY_CLIENT_BODY_TRUNCATION_MARKER),
            expect_marker,
            "{label}"
        );
        if !expect_marker {
            assert_eq!(echoed, exact, "{label}: echoed whole");
        }
    }
}

/// The cap lives in classify's arm too: a 422 on a read of a resource path
/// is capped the same way.
#[tokio::test]
async fn cap_422_body_is_truncated_on_a_read() {
    let mock = IgnitionMock::start().await;
    let big = "w".repeat(100 * 1024);
    Mock::given(method("GET"))
        .and(path(
            "/data/api/v1/resources/names/ignition/database-connection",
        ))
        .respond_with(ResponseTemplate::new(422).set_body_raw(big, "text/plain"))
        .expect(1)
        .mount(&mock.server)
        .await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    let err = api
        .resource_names("ignition", "database-connection")
        .await
        .expect_err("422");
    let CoreError::InvalidInput { reason } = err else {
        panic!("expected invalid_input");
    };
    assert!(reason.ends_with(GATEWAY_CLIENT_BODY_TRUNCATION_MARKER));
    assert!(reason.len() < 4096 + 400, "{} bytes", reason.len());
}

// ---------------------------------------------------------------------------
// 20-05: deployment modes (DAPI-05)
// ---------------------------------------------------------------------------

const GATEWAY_INFO: &str = "/data/api/v1/gateway-info";
const MODE: &str = "/data/api/v1/mode";

/// C-null (no mode), "GET gateway-info (raw)": `"deploymentMode":""`.
const CNULL_NO_MODE: &str = r#"{"name":"ign-deploy-rig","redundancyRole":"Independent","edition":"standard","hostname":"localhost","port":"18290","ignitionVersion":"8.3.6 (b2026042713)","deploymentMode":"","timeZone":"Coordinated Universal Time","timeZoneId":"Etc/UTC","jvmVersion":"17.0.18","allowUnsignedModules":false,"license":{"mode":"Trial","validForVersion":8,"expirationDate":"Oct 12, 2026, 4:31:03 PM","licenseRestrictions":[]}}"#;

/// C-null (mode production), "GET gateway-info (raw)".
const CNULL_PRODUCTION: &str = r#"{"name":"ign-deploy-rig","redundancyRole":"Independent","edition":"standard","hostname":"localhost","port":"18290","ignitionVersion":"8.3.6 (b2026042713)","deploymentMode":"production","timeZone":"Coordinated Universal Time","timeZoneId":"Etc/UTC","jvmVersion":"17.0.18","allowUnsignedModules":false,"license":{"mode":"Trial","validForVersion":8,"expirationDate":"Oct 12, 2026, 4:32:16 PM","licenseRestrictions":[]}}"#;

/// C-mode "GET /mode (before)": no modes yet.
const CMODE_EMPTY: &str =
    r#"{"items":[],"metadata":{"total":0,"matching":0,"limit":-1,"offset":0}}"#;

/// C-mode "GET /mode (after)": the one created mode.
const CMODE_AFTER: &str = r#"{"items":[{"name":"production","title":"Production","description":"ign-deploy-rig","resourceCount":0}],"metadata":{"total":1,"matching":1,"limit":-1,"offset":0}}"#;

/// C-mode "POST /mode production": the 200 body (pretty-printed).
const CMODE_CREATED: &str = "{\n  \"name\": \"production\",\n  \"title\": \"Production\",\n  \"description\": \"ign-deploy-rig\",\n  \"resourceCount\": 0\n}";

/// C-mode "POST /mode production again (duplicate)": 409 Jetty HTML.
const CMODE_DUP_409: &str = "<html>\n<head>\n<meta http-equiv=\"Content-Type\" content=\"text/html;charset=ISO-8859-1\"/>\n<title>Error 409</title>\n</head>\n<body><h2>HTTP ERROR 409 Mode already exists</h2>\n<table>\n<tr><th>URI:</th><td>/data/api/v1/mode</td></tr>\n<tr><th>STATUS:</th><td>409</td></tr>\n<tr><th>MESSAGE:</th><td>Mode already exists</td></tr>\n</table>\n\n</body>\n</html>\n";

/// C-mode "POST /mode with the illegal name": 400 Jetty HTML.
const CMODE_ILLEGAL_400: &str = "<html>\n<head>\n<meta http-equiv=\"Content-Type\" content=\"text/html;charset=ISO-8859-1\"/>\n<title>Error 400</title>\n</head>\n<body><h2>HTTP ERROR 400 Invalid mode name</h2>\n<table>\n<tr><th>URI:</th><td>/data/api/v1/mode</td></tr>\n<tr><th>STATUS:</th><td>400</td></tr>\n<tr><th>MESSAGE:</th><td>Invalid mode name</td></tr>\n</table>\n\n</body>\n</html>\n";

async fn mount_gateway_info(mock: &IgnitionMock, body: &str) {
    Mock::given(method("GET"))
        .and(path(GATEWAY_INFO))
        .respond_with(json(200, body))
        .expect(1)
        .mount(&mock.server)
        .await;
}

/// C-null (no mode) verbatim: `""` is no mode.
#[tokio::test]
async fn mode_deployment_mode_no_mode_is_none() {
    let mock = IgnitionMock::start().await;
    mount_gateway_info(&mock, CNULL_NO_MODE).await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    assert_eq!(api.deployment_mode().await.expect("reads"), None);
}

/// D5 covers other versions: `null`, the key absent and a whitespace-only
/// string each read as no mode too.
#[tokio::test]
async fn mode_deployment_mode_null_absent_and_blank_are_none() {
    for body in [
        r#"{"name":"g","deploymentMode":null}"#,
        r#"{"name":"g"}"#,
        r#"{"name":"g","deploymentMode":"   "}"#,
    ] {
        let mock = IgnitionMock::start().await;
        mount_gateway_info(&mock, body).await;
        let concrete = api_for(&mock);
        let api: &dyn DeployApi = &concrete;
        assert_eq!(api.deployment_mode().await.expect("reads"), None, "{body}");
    }
}

/// C-null (mode production) verbatim: the active mode, by name.
#[tokio::test]
async fn mode_deployment_mode_reads_the_active_mode() {
    let mock = IgnitionMock::start().await;
    mount_gateway_info(&mock, CNULL_PRODUCTION).await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    assert_eq!(
        api.deployment_mode().await.expect("reads").as_deref(),
        Some("production")
    );
}

/// C-mode "GET /mode (before)": an empty list is Ok; the request carries
/// `limit=-1` and `offset=0`.
#[tokio::test]
async fn mode_list_empty_is_ok_and_sends_limit_and_offset() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("GET"))
        .and(path(MODE))
        .and(query_param("limit", "-1"))
        .and(query_param("offset", "0"))
        .respond_with(json(200, CMODE_EMPTY))
        .expect(1)
        .mount(&mock.server)
        .await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    let page = api.modes().await.expect("lists");
    assert!(page.items.is_empty());
    assert_eq!(page.metadata.total, 0);
}

/// C-mode "GET /mode (after)" parses, and two modes keep gateway order.
#[tokio::test]
async fn mode_list_keeps_order() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("GET"))
        .and(path(MODE))
        .respond_with(json(
            200,
            r#"{"items":[{"name":"zeta","title":"Z","description":"","resourceCount":2},{"name":"alpha","title":"A","description":"","resourceCount":0}],"metadata":{"total":2,"matching":2,"limit":-1,"offset":0}}"#,
        ))
        .expect(1)
        .mount(&mock.server)
        .await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    let names: Vec<String> = api
        .modes()
        .await
        .expect("lists")
        .items
        .into_iter()
        .map(|mode| mode.name)
        .collect();
    assert_eq!(names, ["zeta", "alpha"]);

    // The captured single-item body parses too.
    let parsed: ignition_core::client::query::ListEnvelope<
        ignition_core::client::mode::ModeRecord,
    > = serde_json::from_str(CMODE_AFTER).expect("C-mode after parses");
    assert_eq!(parsed.items[0].resource_count, Some(0));
}

fn production_mode() -> ModeCreate {
    ModeCreate {
        name: "production".to_string(),
        title: "Production".to_string(),
        description: "x".to_string(),
    }
}

/// C-mode + D6: one POST with exactly the three keys, then a GET list that
/// confirms the mode; the listed record comes back.
#[tokio::test]
async fn mode_create_posts_body_and_confirms_by_list() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("POST"))
        .and(path(MODE))
        .respond_with(json(200, CMODE_CREATED))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path(MODE))
        .respond_with(json(200, CMODE_AFTER))
        .expect(1)
        .mount(&mock.server)
        .await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;

    let record = api.mode_create(&production_mode()).await.expect("created");

    assert_eq!(record.name, "production");
    assert_eq!(record.description.as_deref(), Some("ign-deploy-rig"));
    let seen = requests_of(&mock).await;
    let post = seen
        .iter()
        .find(|req| req.method.as_str() == "POST")
        .expect("a POST was sent");
    assert_eq!(
        post.body,
        br#"{"name":"production","title":"Production","description":"x"}"#
    );
    assert!(
        post.headers
            .get("x-ignition-api-token")
            .is_some_and(|v| v.as_bytes() == b"cap-a:tokengeneratedlive"),
        "the create is authenticated"
    );
}

/// T-20-23: a 2xx whose mode is absent from the follow-up list is an
/// `internal` error naming the method and the mode.
#[tokio::test]
async fn mode_create_absent_from_follow_up_list_is_an_error() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("POST"))
        .and(path(MODE))
        .respond_with(json(200, CMODE_CREATED))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path(MODE))
        .respond_with(json(200, CMODE_EMPTY))
        .expect(1)
        .mount(&mock.server)
        .await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;

    let err = api
        .mode_create(&production_mode())
        .await
        .expect_err("unconfirmed is never Ok");

    assert_eq!(err.code(), "internal");
    let text = err.to_string();
    assert!(text.contains("mode_create"), "{text}");
    assert!(text.contains("production"), "{text}");
}

/// D6: the duplicate (409 HTML) answered by the gateway is `mode_exists`,
/// never Ok, and no list read follows.
#[tokio::test]
async fn mode_create_duplicate_is_never_ok() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("POST"))
        .and(path(MODE))
        .respond_with(html(409, CMODE_DUP_409))
        .expect(1)
        .mount(&mock.server)
        .await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;

    let err = api
        .mode_create(&production_mode())
        .await
        .expect_err("a duplicate is never Ok");

    assert_eq!(err.code(), "mode_exists");
    assert_eq!(err.exit_code(), 6);
    assert!(
        err.to_string().contains("production"),
        "names the mode: {err}"
    );
    assert_eq!(requests_of(&mock).await.len(), 1, "no confirming read");
}

/// D6: an illegal name the gateway refuses with a 400 HTML page is
/// `invalid_input` (exit 2), not a bug report, and no list read follows.
#[tokio::test]
async fn mode_create_gateway_400_is_invalid_input() {
    let mock = IgnitionMock::start().await;
    Mock::given(method("POST"))
        .and(path(MODE))
        .respond_with(html(400, CMODE_ILLEGAL_400))
        .expect(1)
        .mount(&mock.server)
        .await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;

    let err = api
        .mode_create(&production_mode())
        .await
        .expect_err("an illegal name is never Ok");

    assert_eq!(err.code(), "invalid_input");
    assert_eq!(err.exit_code(), 2);
    assert!(
        !err.to_string().contains("<html"),
        "the page is not echoed: {err}"
    );
    assert_eq!(requests_of(&mock).await.len(), 1, "no confirming read");
}

/// T-20-22: a name outside `[A-Za-z0-9._-]` is `invalid_input`, exit 2,
/// with zero requests.
#[tokio::test]
async fn mode_name_validation_refuses_before_any_request() {
    let mock = IgnitionMock::start().await;
    let concrete = api_for(&mock);
    let api: &dyn DeployApi = &concrete;
    for bad in ["a b/", "", "..", "é"] {
        let err = api
            .mode_create(&ModeCreate {
                name: bad.to_string(),
                title: "T".to_string(),
                description: "d".to_string(),
            })
            .await
            .expect_err(bad);
        assert_eq!(err.code(), "invalid_input", "{bad:?}");
        assert_eq!(err.exit_code(), 2, "{bad:?}");
    }
    assert!(requests_of(&mock).await.is_empty(), "nothing was sent");
}

/// WR-06: the `DEPLOY_ENDPOINTS` method and path columns are tied to the
/// wire. Each trait method runs once against a catch-all mock, and the FIRST
/// request it sent must carry the verb and path of its own row (templates
/// filled with the values below). Every row must be exercised, so a new row
/// without a call here, or a method that drifts from its row, fails.
#[tokio::test]
async fn wire_matches_the_endpoint_table() {
    use ignition_core::client::deploy_api::{DEPLOY_ENDPOINTS, endpoint_decl};

    const MODULE: &str = "ignition";
    const TY: &str = "database-connection";
    const NAME: &str = "DB";

    macro_rules! row {
        ($seen:ident, $id:literal, $api:ident => $call:expr) => {{
            let mock = IgnitionMock::start().await;
            Mock::given(wiremock::matchers::any())
                .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
                .mount(&mock.server)
                .await;
            let concrete = api_for(&mock);
            let $api: &dyn DeployApi = &concrete;
            // Only the request matters; the answer may well fail to parse.
            let _ = $call.await;
            let first = requests_of(&mock)
                .await
                .into_iter()
                .next()
                .unwrap_or_else(|| panic!("{} sent no request", $id));
            let row = endpoint_decl($id).unwrap_or_else(|| panic!("no row {}", $id));
            let expected = row
                .path_template
                .replace("{module}", MODULE)
                .replace("{type}", TY)
                .replace("{name}", NAME)
                .replace("{kind}", "config");
            assert_eq!(first.method.as_str(), row.method, "{} verb", $id);
            assert_eq!(first.url.path(), expected, "{} path", $id);
            $seen.push($id);
        }};
    }

    let mut seen: Vec<&str> = Vec::new();
    row!(seen, "resource_names", api => api.resource_names(MODULE, TY));
    row!(seen, "resource_list", api => api.resource_list(MODULE, TY, &ListQuery::default()));
    row!(seen, "resource_find", api => api.resource_find(MODULE, TY, NAME, None));
    row!(seen, "resource_singleton", api => api.resource_singleton(MODULE, TY, None));
    row!(
        seen,
        "resource_create",
        api => api.resource_create(MODULE, TY, &[ResourceCreate::new(NAME, config_value())])
    );
    row!(
        seen,
        "resource_update",
        api => api.resource_update(MODULE, TY, &[update_item(NAME, SENT_SIG)])
    );
    row!(seen, "scan_lock_acquire", api => api.scan_lock_acquire(ScanKind::Config, 2, 5));
    row!(seen, "scan_lock_info", api => api.scan_lock_info(ScanKind::Config));
    row!(seen, "scan_status", api => api.scan_status(ScanKind::Config));
    row!(seen, "scan_request", api => api.scan_request(ScanKind::Config));
    row!(seen, "modes", api => api.modes());
    row!(seen, "mode_create", api => api.mode_create(&production_mode()));
    row!(seen, "deployment_mode", api => api.deployment_mode());
    row!(seen, "restart_pending", api => api.restart_pending());

    let mut declared: Vec<&str> = DEPLOY_ENDPOINTS.iter().map(|row| row.id).collect();
    declared.sort_unstable();
    seen.sort_unstable();
    assert_eq!(
        seen, declared,
        "every declared row is exercised exactly once"
    );
}

// ---------------------------------------------------------------------------
// 20-05: the stateful fake gateway (FakeDeploy) and the shared scenario
// ---------------------------------------------------------------------------

/// D9 + C1c: cap-a acquires with a hold of 3; successive info reads count
/// down 3, 2, 1, then the lock is free. The actor is `api-token:cap-a:...`.
#[tokio::test]
async fn fake_lock_counts_down_then_frees() {
    let gateway = FakeGateway::new();
    let a = gateway.client("cap-a");
    a.scan_lock_acquire(ScanKind::Config, 2, 3)
        .await
        .expect("acquires");

    let mut remaining = Vec::new();
    for _ in 0..3 {
        match a.scan_lock_info(ScanKind::Config).await.expect("reads") {
            LockObservation::Held(info) => {
                assert_eq!(info.actor.as_deref(), Some("api-token:cap-a:127.0.0.1"));
                remaining.push(info.remaining);
            }
            LockObservation::Free => panic!("freed too early"),
        }
    }
    assert_eq!(remaining, [3, 2, 1]);
    assert_eq!(
        a.scan_lock_info(ScanKind::Config).await.expect("reads"),
        LockObservation::Free
    );
}

/// D1: a competing acquire is `scan_lock_held` (exit 6) naming the holder;
/// D2: the holder's own re-acquire is refused too.
#[tokio::test]
async fn fake_lock_held_by_other_is_scan_lock_held() {
    let gateway = FakeGateway::new();
    let a = gateway.client("cap-a");
    let b = gateway.client("cap-b");
    a.scan_lock_acquire(ScanKind::Config, 2, 30)
        .await
        .expect("acquires");

    for (who, err) in [
        ("cap-b", b.scan_lock_acquire(ScanKind::Config, 2, 5).await),
        ("cap-a", a.scan_lock_acquire(ScanKind::Config, 2, 5).await),
    ] {
        let err = err.expect_err(who);
        assert_eq!(err.code(), "scan_lock_held", "{who}");
        assert_eq!(err.exit_code(), 6, "{who}");
        let (actor, kind, _) = held_parts(err);
        assert_eq!(actor.as_deref(), Some("api-token:cap-a:127.0.0.1"));
        assert_eq!(kind, "config");
    }
}

/// D12: a scan releases the lock at trigger time; D4: a non-holder's scan
/// is accepted while the lock is held.
#[tokio::test]
async fn fake_scan_by_any_actor_releases_the_lock() {
    let gateway = FakeGateway::new();
    let a = gateway.client("cap-a");
    let b = gateway.client("cap-b");
    a.scan_lock_acquire(ScanKind::Config, 2, 30)
        .await
        .expect("acquires");
    b.scan_request(ScanKind::Config)
        .await
        .expect("a non-holder's scan is accepted");
    assert_eq!(
        a.scan_lock_info(ScanKind::Config).await.expect("reads"),
        LockObservation::Free
    );
}

/// D3 + D10: a wrong signature is the typed stale refusal, a successful
/// update rotates the signature so the old one is stale, and an unknown
/// name is `not_found`.
#[tokio::test]
async fn fake_stale_signature_is_the_client_error() {
    let gateway = FakeGateway::new();
    let api = gateway.client("cap-a");
    api.resource_create(
        "ignition",
        "database-connection",
        &[ResourceCreate::new(DB_NAME, config_value())],
    )
    .await
    .expect("creates");
    let record = api
        .resource_find("ignition", "database-connection", DB_NAME, None)
        .await
        .expect("finds");
    let good = record.signature.clone().expect("a signature");

    let wrong = ResourceUpdate::new(DB_NAME, SENT_SIG, config_value()).expect("non-empty");
    let err = api
        .resource_update("ignition", "database-connection", &[wrong])
        .await
        .expect_err("a wrong signature");
    assert_eq!(err.code(), "resource_signature_rejected");
    assert_eq!(err.exit_code(), 6);
    assert!(matches!(
        err,
        CoreError::ResourceSignatureRejected {
            reason: SignatureReason::Stale,
            ..
        }
    ));

    let first = update_item(DB_NAME, &good);
    let outcome = api
        .resource_update("ignition", "database-connection", &[first])
        .await
        .expect("the current signature is accepted");
    assert_ne!(
        outcome.changes[0].new_signature.as_deref(),
        Some(good.as_str()),
        "the write rotated the signature"
    );
    let again = update_item(DB_NAME, &good);
    let err = api
        .resource_update("ignition", "database-connection", &[again])
        .await
        .expect_err("re-using the old signature");
    assert_eq!(err.code(), "resource_signature_rejected");

    let unknown = update_item("NO_SUCH_RESOURCE", &good);
    let err = api
        .resource_update("ignition", "database-connection", &[unknown])
        .await
        .expect_err("an unknown name");
    assert_eq!(err.code(), "not_found");
}

/// D10: a duplicate create is never Ok; the fake reports it as the same
/// `resource_exists` the real client gives a 409 there.
#[tokio::test]
async fn fake_duplicate_create_is_resource_exists() {
    let gateway = FakeGateway::new();
    let api = gateway.client("cap-a");
    let item = || [ResourceCreate::new(DB_NAME, config_value())];
    api.resource_create("ignition", "database-connection", &item())
        .await
        .expect("creates");
    let err = api
        .resource_create("ignition", "database-connection", &item())
        .await
        .expect_err("duplicate");
    assert_eq!(err.code(), "resource_exists");
    assert_eq!(err.exit_code(), 6);
}

/// C-list + D7: with the mode `production` active and a copy in each
/// collection, find with no collection is the production copy and
/// `Some("core")` the core copy; the list shows the effective copy only.
#[tokio::test]
async fn fake_effective_view_prefers_the_active_mode_copy() {
    let gateway = FakeGateway::new();
    let api = gateway.client("cap-a");
    for collection in ["core", "production"] {
        api.resource_create(
            "ignition",
            "database-connection",
            &[ResourceCreate::new(DB_NAME, config_value()).with_collection(collection)],
        )
        .await
        .expect("creates");
    }
    gateway.set_deployment_mode(Some("production".to_string()));

    let effective = api
        .resource_find("ignition", "database-connection", DB_NAME, None)
        .await
        .expect("effective");
    assert_eq!(effective.collection.as_deref(), Some("production"));
    assert_eq!(effective.collections, ["core", "production"]);
    let core = api
        .resource_find("ignition", "database-connection", DB_NAME, Some("core"))
        .await
        .expect("core copy");
    assert_eq!(core.collection.as_deref(), Some("core"));
    assert_ne!(core.signature, effective.signature, "copies differ (D7)");

    let list = api
        .resource_list("ignition", "database-connection", &ListQuery::default())
        .await
        .expect("lists");
    assert_eq!(list.items.len(), 1, "D7: one item per name");
    assert_eq!(list.items[0].collection.as_deref(), Some("production"));

    let names = api
        .resource_names("ignition", "database-connection")
        .await
        .expect("names");
    assert_eq!(names.items[0].modes, ["core", "production"]);

    // With no active mode the effective view is `core`.
    gateway.set_deployment_mode(None);
    let effective = api
        .resource_find("ignition", "database-connection", DB_NAME, None)
        .await
        .expect("effective");
    assert_eq!(effective.collection.as_deref(), Some("core"));
}

/// The shared scan wait, driven against the fake, ends Complete with the
/// timestamp advanced.
#[tokio::test]
async fn fake_scan_completes_by_the_shared_predicate() {
    let gateway = FakeGateway::new();
    let api = gateway.client("cap-a");
    let before = api.scan_status(ScanKind::Config).await.expect("status");
    let cfg = ScenarioTiming::fake().scan;
    match scan_and_wait(&api, ScanKind::Config, cfg)
        .await
        .expect("waits")
    {
        ScanOutcome::Complete(now) => {
            assert!(!now.scan_active);
            assert!(now.last_scan_timestamp > before.last_scan_timestamp);
        }
        ScanOutcome::NotComplete { .. } => panic!("the fake scan completes"),
    }
}

/// D6 + T-20-26: modes keep insertion order, a duplicate is an error, and
/// creating a mode never activates it.
#[tokio::test]
async fn fake_modes_never_activate_and_reject_duplicates() {
    let gateway = FakeGateway::new();
    let api = gateway.client("cap-a");
    assert_eq!(api.deployment_mode().await.expect("reads"), None);
    api.mode_create(&production_mode()).await.expect("created");
    api.mode_create(&ModeCreate {
        name: "alpha".to_string(),
        title: "A".to_string(),
        description: "d".to_string(),
    })
    .await
    .expect("created");
    let names: Vec<String> = api
        .modes()
        .await
        .expect("lists")
        .items
        .into_iter()
        .map(|mode| mode.name)
        .collect();
    assert_eq!(names, ["production", "alpha"]);
    assert_eq!(
        api.deployment_mode().await.expect("reads"),
        None,
        "defining a mode does not activate it"
    );
    let err = api
        .mode_create(&production_mode())
        .await
        .expect_err("duplicate mode");
    assert_eq!(err.code(), "mode_exists");
    let err = api
        .mode_create(&ModeCreate {
            name: "a b/".to_string(),
            title: "T".to_string(),
            description: "d".to_string(),
        })
        .await
        .expect_err("invalid name");
    assert_eq!(err.code(), "invalid_input");

    // `""` and a blank string are no mode, a name is the mode (D5).
    gateway.set_deployment_mode(Some("  ".to_string()));
    assert_eq!(api.deployment_mode().await.expect("reads"), None);
    gateway.set_deployment_mode(Some("production".to_string()));
    assert_eq!(
        api.deployment_mode().await.expect("reads").as_deref(),
        Some("production")
    );
}

/// The segment guard is the library's: a bad module or an empty batch is
/// refused with no state change.
#[tokio::test]
async fn fake_refuses_invalid_segments_and_empty_batches() {
    let gateway = FakeGateway::new();
    let api = gateway.client("cap-a");
    let err = api
        .resource_names("ig/nition", "database-connection")
        .await
        .expect_err("bad module");
    assert_eq!(err.code(), "invalid_input");
    let err = api
        .resource_create("ignition", "database-connection", &[])
        .await
        .expect_err("empty batch");
    assert_eq!(err.code(), "invalid_input");
    let err = api
        .resource_find("ignition", "database-connection", "x", None)
        .await
        .expect_err("absent");
    assert_eq!(err.code(), "not_found");
}

/// The shared scenario run against two fake handles yields exactly the
/// trace the 20-LIVE-CAPTURES Decisions predict. 20-06 diffs the live trace
/// against this literal; each tag names the Decision that fixes it.
#[tokio::test]
async fn fake_scenario_trace_matches_the_capture_decisions() {
    let gateway = FakeGateway::new();
    let a = gateway.client("cap-a");
    let b = gateway.client("cap-b");

    let trace = scenario(&a, &b, "cap-a", "t", &ScenarioTiming::fake()).await;

    let expected = [
        // D1: the acquire answers at once; D9: the info read names the actor.
        "lock:acquired",
        "lock:held-by-a",
        // D1 + C1e: the competing acquire is refused, naming a.
        "lock:refused-held-by-a",
        // D9: the hold lapses, the info read is a 204.
        "lock:free",
        // D4: a no-op scan advances lastScanTimestamp.
        "scan:complete",
        // D10: create needs only name and config.
        "create:ok",
        "find:ok",
        // D3: the current signature is accepted, the rotated-away one stale.
        "update:ok",
        "update:stale",
        // D10: the duplicate create is a 409, typed `resource_exists`.
        "create-duplicate:resource_exists",
        // D6: create answers 200 and the list confirms it.
        "mode:created",
        // D6: the duplicate mode is a 409, typed `mode_exists`.
        "mode-duplicate:mode_exists",
        "modes:listed",
        // D5 + T-20-26: a defined mode is not an active mode.
        "deployment-mode:none",
    ];
    assert_eq!(trace, expected);
}
