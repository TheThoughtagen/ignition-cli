//! Golden-file contract tests for the `ign logs` command tree (02-04,
//! HLTH-03/04): list (3 modes), filters (param-level mock matchers),
//! loggers, the --yes guards, the `.idb` download, and the STREAMING
//! tail (`-f --timeout` → entries on stdout, clean exit 0). Harness
//! inherited from `contract_sessions.rs` (01-02): isolated
//! `IGNITION_CLI_CONFIG`, `stdout_for_golden`, `[..]` elides only
//! genuinely dynamic values.
//!
//! The crown pins:
//! - every list request carries `limit=200` AND `sortBy=desc(timestamp)`
//!   (the "recent entries" contract — the newest 200, never the oldest);
//! - set/reset refuse with exit 2 `confirmation_required` BEFORE any
//!   API construction (the sessions-terminate guard precedent);
//! - `logs -f` streams NDJSON under `--json`/`--compact` — ONE compact
//!   entry object per line, NO envelope (the second sanctioned stdout
//!   exception, README-documented).

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::Value;

/// Isolated config dir + the config path inside it (file need not exist).
fn isolated_config() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.toml");
    (dir, path)
}

/// Write the one-profile dev config whose URL points at `url` and whose
/// token comes from `IGNITION_TOKEN`.
fn write_profile_config(config: &Path, url: &str) {
    std::fs::write(
        config,
        format!(
            "active = \"dev\"\n\n[profiles.dev]\nurl = \"{url}\"\nauth = {{ token_env = \"IGNITION_TOKEN\" }}\n"
        ),
    )
    .expect("write config");
}

/// Spawn `ign` with an isolated config, the mock token in the env, and args.
fn ign(config: &Path, url: &str, args: &[&str]) -> std::process::Output {
    let mut command = Command::cargo_bin("ign").expect("binary 'ign' not found");
    command
        .env("IGNITION_CLI_CONFIG", config)
        .env("IGNITION_TOKEN", "mock:name-key")
        .env("IGNITION_URL", url);
    command.args(args).output().expect("spawn ign")
}

/// stdout minus the single trailing newline `println!` appends.
fn stdout_for_golden(out: &std::process::Output) -> &str {
    let stdout = std::str::from_utf8(&out.stdout).expect("utf-8 stdout");
    stdout.strip_suffix('\n').unwrap_or(stdout)
}

/// stderr's JSON envelope starting at the first `{` (log-tolerant parse).
fn stderr_envelope(out: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&out.stderr);
    let start = stderr.find('{').unwrap_or(0);
    stderr[start..].to_string()
}

/// The fixture page: three entries (plain INFO, ERROR with stack+mdc,
/// a later INFO) + metadata. Fixed timestamps keep the ISO rendering
/// deterministic (UTC, no local timezone anywhere).
async fn mount_logs_list(server: &wiremock::MockServer) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/logs"))
        .and(wiremock::matchers::query_param("sortBy", "desc(timestamp)"))
        .and(wiremock::matchers::query_param("limit", "200"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {
                        "timestamp": 1787346748022i64,
                        "loggerName": "Common.BasicExecutionEngine.Thread$",
                        "level": "ERROR",
                        "message": "Execution halted by exception",
                        "stack": [
                            "java.lang.RuntimeException: boom",
                            "at com.inductiveautomation.ignition.common.Sample.run(Sample.java:42)"
                        ],
                        "mdc": {"thread": "Thread-12"}
                    },
                    {
                        "timestamp": 1787346747022i64,
                        "loggerName": "GatewayManager",
                        "level": "INFO",
                        "message": "Gateway is now RUNNING"
                    }
                ],
                "metadata": {"total": 368, "matching": 2, "limit": 200, "offset": 0}
            })),
        )
        .expect(1..)
        .mount(server)
        .await;
}

/// `ign logs` goldens in all three modes — newest first (desc sort),
/// fixed UTC ISO timestamps, camelCase wire-faithful JSON.
#[tokio::test]
async fn logs_list_render_modes_golden() {
    let server = wiremock::MockServer::start().await;
    mount_logs_list(&server).await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(&config, &server.uri(), &["logs"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
2026-08-21T21:12:28.022Z  ERROR  Common.BasicExecutionEngine.Thread$  Execution halted by exception
2026-08-21T21:12:27.022Z   INFO  GatewayManager  Gateway is now RUNNING
"#]],
    );

    let out = ign(&config, &server.uri(), &["logs", "--json"]);
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
{
  "ok": true,
  "profile": "dev",
  "data": {
    "items": [
      {
        "timestamp": 1787346748022,
        "loggerName": "Common.BasicExecutionEngine.Thread$",
        "level": "ERROR",
        "message": "Execution halted by exception",
        "stack": [
          "java.lang.RuntimeException: boom",
          "at com.inductiveautomation.ignition.common.Sample.run(Sample.java:42)"
        ],
        "mdc": {
          "thread": "Thread-12"
        }
      },
      {
        "timestamp": 1787346747022,
        "loggerName": "GatewayManager",
        "level": "INFO",
        "message": "Gateway is now RUNNING"
      }
    ],
    "metadata": {
      "total": 368,
      "matching": 2,
      "limit": 200,
      "offset": 0
    }
  }
}
"#]],
    );

    let out = ign(&config, &server.uri(), &["logs", "--compact"]);
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[
            r#"{"ok":true,"profile":"dev","data":{"items":[{"timestamp":1787346748022,"loggerName":"Common.BasicExecutionEngine.Thread$","level":"ERROR","message":"Execution halted by exception","stack":["java.lang.RuntimeException: boom","at com.inductiveautomation.ignition.common.Sample.run(Sample.java:42)"],"mdc":{"thread":"Thread-12"}},{"timestamp":1787346747022,"loggerName":"GatewayManager","level":"INFO","message":"Gateway is now RUNNING"}],"metadata":{"total":368,"matching":2,"limit":200,"offset":0}}}"#
        ]],
    );
}

/// The `--logger`/`--min-level` filters ride the query string under
/// their gateway-native names (mock matcher-pinned — the request never
/// fires unless the params match), and `--since 0` sends startTime=0.
#[tokio::test]
async fn logs_filters_and_since_ride_the_query() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/logs"))
        .and(wiremock::matchers::query_param("minLevel", "WARN"))
        .and(wiremock::matchers::query_param("logger", "GatewayManager"))
        .and(wiremock::matchers::query_param("startTime", "0"))
        .and(wiremock::matchers::query_param("limit", "50"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [],
                "metadata": {"total": 0, "matching": 0, "limit": 50, "offset": 0}
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(
        &config,
        &server.uri(),
        &[
            "logs",
            "--min-level",
            "warn",
            "--logger",
            "GatewayManager",
            "--since",
            "0",
            "--limit",
            "50",
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = stdout_for_golden(&out);
    assert_eq!(stdout, "[profile: dev]\n(no matching log entries)");
}

/// A junk `--since` is a clap usage error (exit 2) — validation lives
/// at arg-parse time via the core grammar.
#[tokio::test]
async fn logs_since_junk_is_a_usage_error() {
    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");
    let out = ign(
        &config,
        "http://127.0.0.1:1",
        &["logs", "--since", "banana"],
    );
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("invalid --since"), "stderr: {stderr}");
}

/// `ign logs loggers` goldens: `name  level  context` rows; JSON keeps
/// absent levels omitted.
#[tokio::test]
async fn loggers_list_golden() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/logs/loggers"))
        .and(wiremock::matchers::query_param("limit", "200"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {"name": "GatewayManager", "level": "INFO"},
                    {"name": "Common.SQL"}
                ],
                "metadata": {"total": 1250, "matching": 2, "limit": 200, "offset": 0}
            })),
        )
        .expect(1..)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(&config, &server.uri(), &["logs", "loggers"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
GatewayManager  INFO  -
Common.SQL  -  -
"#]],
    );

    let out = ign(&config, &server.uri(), &["logs", "loggers", "--compact"]);
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[
            r#"{"ok":true,"profile":"dev","data":{"items":[{"name":"GatewayManager","level":"INFO"},{"name":"Common.SQL"}],"metadata":{"total":1250,"matching":2,"limit":200,"offset":0}}}"#
        ]],
    );
}

/// THE guard pins: `loggers set` and `loggers reset` without `--yes`
/// exit 2 with the `confirmation_required` envelope BEFORE any API
/// construction (fully static goldens — no server mounted).
#[tokio::test]
async fn loggers_set_and_reset_without_yes_exit_2_golden() {
    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(
        &config,
        "http://127.0.0.1:1",
        &[
            "logs",
            "loggers",
            "set",
            "GatewayManager",
            "debug",
            "--compact",
        ],
    );
    assert_eq!(out.status.code(), Some(2), "usage class");
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stderr_envelope(&out),
        snapbox::str![[r#"
{"ok":false,"profile":null,"error":{"code":"confirmation_required","message":"logs loggers set is destructive; rerun with --yes to confirm","endpoint":null,"hint":"this operation is destructive; re-run with --yes or set IGNITION_YES=1"}}

"#]],
    );

    let out = ign(&config, "http://127.0.0.1:1", &["logs", "loggers", "reset"]);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("logs loggers reset"),
        "reset names its operation: {stderr}"
    );
}

/// WITH `--yes`: set POSTs to the exact route with the level query
/// param (recorded-request proof) and reset POSTs levelreset — both
/// succeed with one confirmation line / compact data.
#[tokio::test]
async fn loggers_set_and_reset_with_yes_succeed() {
    let server = wiremock::MockServer::start().await;
    let set_guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/data/api/v1/logs/loggers/GatewayManager",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1..)
        .mount_as_scoped(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/data/api/v1/logs/levelreset"))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(
        &config,
        &server.uri(),
        &["logs", "loggers", "set", "GatewayManager", "debug", "--yes"],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
set logger GatewayManager to DEBUG
"#]],
    );

    // The recorded POST: level=DEBUG on the query string, empty body.
    let requests = set_guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    let query = requests[0].url.query().expect("query present");
    assert!(
        query.contains("level=DEBUG"),
        "level rode the query: {query}"
    );
    assert!(requests[0].body.is_empty(), "empty body");

    let out = ign(
        &config,
        &server.uri(),
        &["logs", "loggers", "reset", "--yes", "--compact"],
    );
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"reset":true}}"#]],
    );
}

/// `ign logs download -o FILE`: the SQLite bytes land EXACTLY as
/// received at the given path (never .zip), and the output names the
/// path + byte count (human golden elides the dynamic tmp path; the
/// JSON envelope is asserted programmatically for the same reason).
#[tokio::test]
async fn logs_download_writes_idb_bytes() {
    let server = wiremock::MockServer::start().await;
    let mut sqlite = b"SQLite format 3\0".to_vec();
    sqlite.extend_from_slice(&[0x10, 0x00, 0xAB, 0xCD]);
    let body = sqlite.clone();
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/logs/download"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .insert_header(
                    "Content-Disposition",
                    "attachment; filename=MyGateway_Ignition_logs_20260822-0307.idb",
                )
                .set_body_raw(body, "application/x-sqlite3"),
        )
        .expect(1..)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");
    let outdir = tempfile::tempdir().expect("outdir");
    let target = outdir.path().join("archive.idb");

    let out = ign(
        &config,
        &server.uri(),
        &["logs", "download", "-o", target.to_str().unwrap()],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Byte-for-byte as received, and an .idb extension.
    let written = std::fs::read(&target).expect("archive written");
    assert_eq!(written, sqlite, "bytes exactly as received");
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
wrote [..]/archive.idb (20 bytes)
"#]],
    );

    // JSON mode: {file, bytes, content_type}.
    let out = ign(
        &config,
        &server.uri(),
        &[
            "logs",
            "download",
            "-o",
            target.to_str().unwrap(),
            "--compact",
        ],
    );
    assert!(out.status.success());
    let body: Value = serde_json::from_slice(&out.stdout).expect("envelope parses");
    assert_eq!(body["data"]["bytes"], Value::from(20));
    assert_eq!(
        body["data"]["content_type"],
        Value::String("application/x-sqlite3".into())
    );
    assert!(
        body["data"]["file"]
            .as_str()
            .expect("file path present")
            .ends_with("archive.idb")
    );
}

/// A stateful fixture: the FIRST logs GET serves one entry, every later
/// one serves silence — the tail's two-phase world.
#[derive(Debug)]
struct ServeFirstThenEmpty {
    hits: std::sync::Mutex<usize>,
    page: serde_json::Value,
}

impl wiremock::Respond for ServeFirstThenEmpty {
    fn respond(&self, _request: &wiremock::Request) -> wiremock::ResponseTemplate {
        let mut hits = self.hits.lock().unwrap();
        *hits += 1;
        if *hits == 1 {
            wiremock::ResponseTemplate::new(200).set_body_json(self.page.clone())
        } else {
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [],
                "metadata": {"total": 1, "matching": 0, "limit": 200, "offset": 0}
            }))
        }
    }
}

/// THE streaming pin: `logs -f --timeout 2` against a mock serving one
/// page then silence — the entry STREAMS to stdout (human: profile
/// header + entry line; NDJSON under --compact: ONE compact entry
/// object per line, NO envelope) and the timeout expiry ends the tail
/// CLEANLY (exit 0).
#[tokio::test]
async fn logs_follow_streams_entries_and_ends_cleanly() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/logs"))
        .respond_with(ServeFirstThenEmpty {
            hits: std::sync::Mutex::new(0),
            page: serde_json::json!({
                "items": [
                    {
                        "timestamp": 1787346747022i64,
                        "loggerName": "GatewayManager",
                        "level": "INFO",
                        "message": "Gateway is now RUNNING"
                    }
                ],
                "metadata": {"total": 1, "matching": 1, "limit": 200, "offset": 0}
            }),
        })
        .expect(1..)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    // Human: header + the streamed entry line; clean exit 0.
    let out = ign(&config, &server.uri(), &["logs", "-f", "--timeout", "2"]);
    assert!(
        out.status.success(),
        "timeout expiry must end cleanly (exit 0): stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("[profile: dev]"), "header: {stdout}");
    assert!(
        stdout.contains("GatewayManager  Gateway is now RUNNING"),
        "entry streamed as it arrived: {stdout}"
    );
    assert!(
        stdout.contains("2026-08-21T21:12:27.022Z"),
        "ISO ts: {stdout}"
    );

    // NDJSON (compact): one compact entry object per line, no envelope.
    // A FRESH server: the stateful fixture serves the entry to the
    // first request each server ever sees.
    let server2 = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/logs"))
        .respond_with(ServeFirstThenEmpty {
            hits: std::sync::Mutex::new(0),
            page: serde_json::json!({
                "items": [
                    {
                        "timestamp": 1787346747022i64,
                        "loggerName": "GatewayManager",
                        "level": "INFO",
                        "message": "Gateway is now RUNNING"
                    }
                ],
                "metadata": {"total": 1, "matching": 1, "limit": 200, "offset": 0}
            }),
        })
        .expect(1..)
        .mount(&server2)
        .await;
    let out = ign(
        &config,
        &server2.uri(),
        &["logs", "--follow", "--timeout", "2", "--compact"],
    );
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(!lines.is_empty(), "the entry streamed: {stdout}");
    for line in lines {
        let entry: Value = serde_json::from_str(line)
            .unwrap_or_else(|err| panic!("each line is a compact entry object ({err}): {line}"));
        assert!(
            entry.get("loggerName").is_some(),
            "entry objects, never an envelope: {line}"
        );
        assert!(
            entry.get("ok").is_none(),
            "no envelope fields in NDJSON: {line}"
        );
    }
    assert!(stdout.contains("\"timestamp\":1787346747022"));
}
