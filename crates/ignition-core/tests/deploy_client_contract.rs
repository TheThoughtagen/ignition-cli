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
use ignition_core::client::scan::{LockObservation, LockVerdict, ScanKind, lock_verdict};
use ignition_core::client::{DeployApi, ReqwestGatewayApi};
use ignition_core::config::{Credential, Secret};
use ignition_core::error::CoreError;
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
