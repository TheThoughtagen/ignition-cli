//! Wiremock contract tests for the log capabilities (02-04,
//! HLTH-03/04): the query (tail primitive), the `.idb` download, and
//! the logger registry + level mutations. Fixtures follow the live
//! 8.3.6 captures (02-RESEARCH §Logs + loggers) and the gateway's
//! openapi schema.
//!
//! The crown pins are the RECORDED-REQUEST proofs:
//! - every logs query carries an EXPLICIT `limit` (Pitfall 9 — the
//!   server default is unlimited) and `startTime` rides the query
//!   string (the tail cursor);
//! - `set_logger_level` POSTs to the EXACT
//!   `/data/api/v1/logs/loggers/{name}?level=X` path with an EMPTY
//!   body (token-auth mutations need no CSRF — verified).

mod common;

use common::IgnitionMock;
use ignition_core::client::logs::{DEFAULT_LOG_LIMIT, LogQuery};
use ignition_core::client::{GatewayApi, ReqwestGatewayApi};
use ignition_core::error::CoreError;

const LOGS_PATH: &str = "/data/api/v1/logs";
const LOGS_DOWNLOAD_PATH: &str = "/data/api/v1/logs/download";
const LOGGERS_PATH: &str = "/data/api/v1/logs/loggers";
const LEVEL_RESET_PATH: &str = "/data/api/v1/logs/levelreset";

/// Lowercased Debug dump of a recorded request's headers (01-04 pattern).
fn headers_debug(request: &wiremock::Request) -> String {
    format!("{:?}", request.headers).to_lowercase()
}

/// THE Pitfall-9 + tail-cursor pin (recorded-request proof): a query
/// with `start_time` sends `startTime` on the QUERY string and the
/// EXPLICIT default limit 200 — and the live-captured entry shape
/// (camelCase, a stack trace, MDC) parses.
#[tokio::test]
async fn logs_query_sends_explicit_limit_and_start_time() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(LOGS_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {
                        "timestamp": 1787346747022i64,
                        "loggerName": "GatewayManager",
                        "level": "INFO",
                        "message": "Gateway is now RUNNING"
                    },
                    {
                        "timestamp": 1787346748022i64,
                        "loggerName": "Common.BasicExecutionEngine.Thread$",
                        "level": "ERROR",
                        "message": "Execution halted by exception",
                        "stack": [
                            "java.lang.RuntimeException: boom",
                            "\tat com.inductiveautomation.ignition.common.Sample.run(Sample.java:42)"
                        ],
                        "mdc": {"thread": "Thread-12"}
                    }
                ],
                "metadata": {"total": 2, "matching": 2, "limit": 200, "offset": 0}
            })),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let page = api
        .logs(&LogQuery {
            start_time: Some(1787346747022),
            ..LogQuery::default()
        })
        .await
        .expect("live capture shape must parse");

    // The captured entries parse through the renames — incl. the stack
    // trace entry.
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].logger_name, "GatewayManager");
    assert_eq!(page.items[1].stack.len(), 2, "stack trace lines parse");
    assert_eq!(page.items[1].mdc["thread"], "Thread-12");
    assert_eq!(page.metadata.limit, DEFAULT_LOG_LIMIT);

    // Recorded-request proof: startTime + EXPLICIT limit on the query.
    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    let query = requests[0].url.query().expect("query string present");
    assert!(
        query.contains("startTime=1787346747022"),
        "startTime rides the query string (the tail cursor): {query}"
    );
    assert!(
        query.contains(&format!("limit={DEFAULT_LOG_LIMIT}")),
        "explicit limit ALWAYS present (Pitfall 9): {query}"
    );
}

/// The minLevel/logger filters ride the query string under their
/// gateway-native names (mock matcher-pinned).
#[tokio::test]
async fn logs_filters_ride_native_param_names() {
    let mock = IgnitionMock::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(LOGS_PATH))
        .and(wiremock::matchers::query_param("minLevel", "WARN"))
        .and(wiremock::matchers::query_param("logger", "GatewayManager"))
        .and(wiremock::matchers::query_param("sortBy", "desc(timestamp)"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [],
                "metadata": {"total": 0, "matching": 0, "limit": 200, "offset": 0}
            })),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let page = api
        .logs(&LogQuery {
            min_level: Some("WARN".into()),
            logger: Some("GatewayManager".into()),
            sort_by: Some("desc(timestamp)".into()),
            ..LogQuery::default()
        })
        .await
        .expect("matcher-pinned filters must fire");
    assert!(page.items.is_empty());
}

/// The download: bytes arrive EXACTLY as received with the
/// Content-Disposition filename and the verified SQLite content type —
/// never transformed (Pitfall 7: it is an `.idb`, not a zip).
#[tokio::test]
async fn logs_download_returns_bytes_and_content_disposition() {
    let server = wiremock::MockServer::start().await;
    // A real SQLite file starts with the magic header "SQLite format 3\0".
    let mut sqlite = b"SQLite format 3\0".to_vec();
    sqlite.extend_from_slice(&[0x10, 0x00, 0x00, 0x00, 0xAB, 0xCD]);
    let body = sqlite.clone();
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(LOGS_DOWNLOAD_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .insert_header(
                    "Content-Disposition",
                    "attachment; filename=MyGateway_Ignition_logs_20260822-0307.idb",
                )
                .insert_header("Content-Type", "application/x-sqlite3")
                .set_body_raw(body, "application/x-sqlite3"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let download = api.logs_download().await.expect("download classifies Ok");
    assert_eq!(download.bytes, sqlite, "bytes EXACTLY as received");
    assert_eq!(
        download.filename.as_deref(),
        Some("MyGateway_Ignition_logs_20260822-0307.idb"),
        "Content-Disposition filename surfaces"
    );
    assert_eq!(
        download.content_type.as_deref(),
        Some("application/x-sqlite3"),
        "the verified content type (it is SQLite, not a zip — Pitfall 7)"
    );
}

/// THE set-level pin (recorded-request proof): the POST hits the EXACT
/// `/logs/loggers/{loggerName}` path with `level` as a QUERY param and
/// an EMPTY body, authed — and every spec-documented level is accepted.
#[tokio::test]
async fn set_logger_level_posts_exact_path_with_level_query_and_empty_body() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/data/api/v1/logs/loggers/Common.BasicExecutionEngine",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(7) // every spec-documented level, one POST each
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(
        &server.uri(),
        Some(ignition_core::config::Credential::Token(
            ignition_core::config::Secret::new("name:key"),
        )),
    );
    const LEVELS: [&str; 7] = ["TRACE", "DEBUG", "INFO", "WARN", "ERROR", "FATAL", "OFF"];
    for level in LEVELS {
        api.set_logger_level("Common.BasicExecutionEngine", level)
            .await
            .unwrap_or_else(|err| panic!("{level} must be accepted: {err}"));
    }

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), LEVELS.len());
    let sent: std::collections::HashSet<&str> = requests
        .iter()
        .map(|request| {
            request
                .url
                .query()
                .and_then(|query| query.strip_prefix("level="))
        })
        .collect::<Option<_>>()
        .expect("every POST carries a level query param");
    assert_eq!(
        sent,
        LEVELS.into_iter().collect(),
        "each level rode the query string exactly once"
    );
    for request in &requests {
        assert!(
            request.body.is_empty(),
            "the set-level POST carries NO body (spec)"
        );
        assert!(
            headers_debug(request).contains("x-ignition-api-token"),
            "the mutation is authed"
        );
    }
}

/// The level reset POSTs to the exact `/logs/levelreset` path, empty
/// body (recorded-request proof).
#[tokio::test]
async fn reset_logger_levels_posts_levelreset() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(LEVEL_RESET_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    api.reset_logger_levels().await.expect("200 classifies Ok");

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    assert!(requests[0].body.is_empty(), "no body on the reset POST");
}

/// The logger registry parses `{name, level, context}` items — level
/// absent for inherited loggers, context passthrough.
#[tokio::test]
async fn loggers_parse_the_registry_shape() {
    let mock = IgnitionMock::start().await;
    mock.list_json(
        "GET",
        LOGGERS_PATH,
        serde_json::json!({
            "items": [
                {"name": "GatewayManager", "level": "INFO"},
                {"name": "Common.SQL", "level": null, "context": {}}
            ],
            "metadata": {"total": 1250, "matching": 2, "limit": 200, "offset": 0}
        }),
    )
    .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let page = api
        .loggers(&ignition_core::client::query::ListQuery {
            limit: 200,
            ..Default::default()
        })
        .await
        .expect("registry shape must parse");
    assert_eq!(page.items[0].name, "GatewayManager");
    assert_eq!(page.items[0].level.as_deref(), Some("INFO"));
    assert_eq!(
        page.items[1].level, None,
        "inherited loggers carry no level"
    );
    assert_eq!(
        page.metadata.total, 1250,
        "~1250 loggers on a fresh gateway"
    );
}

/// HTML 401 on the logs query (Jetty page) → Auth (exit 5) — the
/// standard error-body shape for `/data/api/v1/*`.
#[tokio::test]
async fn logs_html_401_classifies_auth() {
    let mock = IgnitionMock::start().await;
    mock.html_error("GET", LOGS_PATH, 401).await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let err = api
        .logs(&LogQuery::default())
        .await
        .expect_err("401 must fail");
    assert!(
        matches!(&err, CoreError::Auth { status: 401, .. }),
        "wrong class: {err}"
    );
    assert_eq!(err.exit_code(), 5);
    assert_eq!(err.code(), "auth_rejected");
}
