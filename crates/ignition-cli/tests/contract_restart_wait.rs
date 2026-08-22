//! Golden-file contract tests for `ign restart` + `ign wait` (02-05,
//! HLTH-09/11): the ALWAYS-guarded restart, the full restart --wait
//! lifecycle, the three wait targets, and the timeout taxonomy.
//! Harness inherited from `contract_logs.rs` (01-02): isolated
//! `IGNITION_CLI_CONFIG`, `stdout_for_golden`, `[..]` elides only
//! genuinely dynamic values (elapsed seconds, timeout durations).
//!
//! The crown pins:
//! - `ign restart` is unreachable without `--yes` in EVERY mode (exit
//!   2 `confirmation_required`, BEFORE any network work — no server
//!   mounted);
//! - `restart --yes --wait` completes the full mock lifecycle (POST →
//!   5 s floor → STARTING → RUNNING) and the golden elides the elapsed
//!   seconds;
//! - `wait restart` (RUNNING→STARTING→RUNNING) succeeds via the
//!   WITNESSED path — returns as soon as RUNNING follows non-RUNNING,
//!   no floor wait (the golden stays fast; the all-RUNNING floor path
//!   is core-tested via the injected floor in restart_wait_contract);
//! - a wait timeout exits 4 with the last observed state in the
//!   message.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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

/// Spawn `ign` WITHOUT the token env — the secret degrades to None and
/// the header-less waits must still work (the whole point of
/// `wait gateway`/`wait restart`).
fn ign_tokenless(config: &Path, url: &str, args: &[&str]) -> std::process::Output {
    let mut command = Command::cargo_bin("ign").expect("binary 'ign' not found");
    command
        .env("IGNITION_CLI_CONFIG", config)
        .env_remove("IGNITION_TOKEN")
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

/// A scripted StatusPing: serves `states` in order, the LAST entry
/// repeating forever (the same fixture shape as the core contract
/// suite).
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

/// Mount the restart POST (200 + literal `true`, recorded).
async fn mount_restart_post(server: &wiremock::MockServer) -> wiremock::MockGuard {
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/data/api/v1/restart-tasks/restart",
        ))
        .and(wiremock::matchers::query_param("confirm", "true"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("true"))
        .expect(1..)
        .mount_as_scoped(server)
        .await
}

/// Mount a scripted StatusPing responder (any number of hits).
async fn mount_status_ping(server: &wiremock::MockServer, script: StatusPingScript) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/StatusPing"))
        .respond_with(script)
        .expect(1..)
        .mount(server)
        .await
}

/// THE guard pin: `ign restart` without `--yes` exits 2 with the
/// `confirmation_required` envelope BEFORE any network work — no
/// server is mounted; the unreachable URL proves the refusal never
/// fired a request.
#[tokio::test]
async fn restart_without_yes_exits_2_before_any_network() {
    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://127.0.0.1:1");

    let out = ign(&config, "http://127.0.0.1:1", &["restart", "--compact"]);
    assert_eq!(out.status.code(), Some(2), "usage class");
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stderr_envelope(&out),
        snapbox::str![[r#"
{"ok":false,"profile":null,"error":{"code":"confirmation_required","message":"restart is destructive; rerun with --yes to confirm","endpoint":null,"hint":"this operation is destructive; re-run with --yes or set IGNITION_YES=1"}}

"#]],
    );

    // The flag is wait-independent: `restart --wait` guards too.
    let out = ign(&config, "http://127.0.0.1:1", &["restart", "--wait"]);
    assert_eq!(out.status.code(), Some(2));
}

/// `ign restart --yes` (no --wait): the POST fires exactly once and
/// the human line carries the READY-in-~1-min advisory + the --wait
/// suggestion; JSON reports `{restarted: true}`.
#[tokio::test]
async fn restart_with_yes_fires_post_and_advises() {
    let server = wiremock::MockServer::start().await;
    let guard = mount_restart_post(&server).await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(&config, &server.uri(), &["restart", "--yes"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
restarting; gateway READY in ~1 min; consider `ign restart --wait`
"#]],
    );

    let out = ign(&config, &server.uri(), &["restart", "--yes", "--compact"]);
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"restarted":true}}"#]],
    );

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 2, "one POST per invocation");
    for request in &requests {
        assert!(request.body.is_empty(), "empty body");
        assert!(
            request
                .url
                .query()
                .unwrap_or_default()
                .contains("confirm=true"),
            "confirm=true on the query string"
        );
    }
}

/// `ign restart --yes --wait` against the full mock lifecycle (POST →
/// 5 s floor → STARTING → STARTING → RUNNING): success golden with the
/// elapsed seconds elided (the floor makes them dynamic ≥ 5).
#[tokio::test]
async fn restart_yes_wait_completes_lifecycle_golden() {
    let server = wiremock::MockServer::start().await;
    // Bind the guard: a dropped scoped guard UNMOUNTS the fixture
    // (the 02-01 wiremock gotcha) and the verify would fail at 0 hits.
    let _post_guard = mount_restart_post(&server).await;
    mount_status_ping(
        &server,
        StatusPingScript::new(&["STARTING", "STARTING", "RUNNING"]),
    )
    .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(
        &config,
        &server.uri(),
        &["restart", "--yes", "--wait", "--interval", "1"],
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
gateway RUNNING after [..]s
"#]],
    );

    let out = ign(
        &config,
        &server.uri(),
        &["restart", "--yes", "--wait", "--interval", "1", "--compact"],
    );
    assert!(out.status.success());
    let body: Value = serde_json::from_slice(&out.stdout).expect("envelope parses");
    assert_eq!(body["ok"], Value::Bool(true));
    assert_eq!(body["data"]["restarted"], Value::Bool(true));
    assert_eq!(body["data"]["state"], Value::String("RUNNING".into()));
    assert!(
        body["data"]["elapsed_secs"].as_u64().expect("elapsed") >= 5,
        "the 5 s floor rode the wait: {}",
        body["data"]["elapsed_secs"]
    );
}

/// `ign wait gateway` against STARTING→RUNNING: the unauth StatusPing
/// poll succeeds — proven header-less by running WITHOUT any token in
/// the env (the secret degrades to None).
#[tokio::test]
async fn wait_gateway_polls_starting_to_running_headerless() {
    let server = wiremock::MockServer::start().await;
    mount_status_ping(&server, StatusPingScript::new(&["STARTING", "RUNNING"])).await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign_tokenless(
        &config,
        &server.uri(),
        &["wait", "gateway", "--interval", "1", "--compact"],
    );
    assert!(
        out.status.success(),
        "no token needed — stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: Value = serde_json::from_slice(&out.stdout).expect("envelope parses");
    assert_eq!(body["data"]["target"], Value::String("gateway".into()));
    assert_eq!(body["data"]["state"], Value::String("RUNNING".into()));

    // Human mode: final state + elapsed.
    let out = ign_tokenless(
        &config,
        &server.uri(),
        &["wait", "gateway", "--interval", "1"],
    );
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
gateway RUNNING after [..]s
"#]],
    );
}

/// `ign wait restart` against RUNNING→STARTING→RUNNING: the WITNESSED
/// path returns as soon as RUNNING follows non-RUNNING — no floor wait
/// (fast golden); the all-RUNNING floor path is core-tested with an
/// injected floor.
#[tokio::test]
async fn wait_restart_witnessed_path_golden() {
    let server = wiremock::MockServer::start().await;
    mount_status_ping(
        &server,
        StatusPingScript::new(&["RUNNING", "STARTING", "RUNNING"]),
    )
    .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let started = std::time::Instant::now();
    let out = ign(
        &config,
        &server.uri(),
        &["wait", "restart", "--interval", "1", "--compact"],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Witnessed restart completes without waiting out the 5 s floor.
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "witnessed path is floor-free: {:?}",
        started.elapsed()
    );
    let body: Value = serde_json::from_slice(&out.stdout).expect("envelope parses");
    assert_eq!(body["data"]["target"], Value::String("restart".into()));
    assert_eq!(body["data"]["state"], Value::String("RUNNING".into()));
}

/// `ign wait module <id>`: the modules poll until ACTIVE.
#[tokio::test]
async fn wait_module_polls_to_active() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/modules/healthy"))
        .and(wiremock::matchers::query_param(
            "search",
            "com.inductiveautomation.perspective",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [
                {"id": "com.inductiveautomation.perspective", "name": "Perspective", "version": "8.3.6", "state": "ACTIVE"}
            ],
            "metadata": {"total": 1, "matching": 1, "limit": -1, "offset": 0}
        })))
        .expect(1..)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(
        &config,
        &server.uri(),
        &[
            "wait",
            "module",
            "com.inductiveautomation.perspective",
            "--interval",
            "1",
        ],
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
module com.inductiveautomation.perspective ACTIVE after [..]s
"#]],
    );
}

/// Wait timeout taxonomy: `wait gateway` against an always-STARTING
/// mock with a short deadline exits 4 (`network_error`) and the
/// message carries the last observed state.
#[tokio::test]
async fn wait_timeout_exits_4_with_last_state() {
    let server = wiremock::MockServer::start().await;
    mount_status_ping(&server, StatusPingScript::new(&["STARTING"])).await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign_tokenless(
        &config,
        &server.uri(),
        &[
            "wait",
            "gateway",
            "--interval",
            "1",
            "--timeout",
            "2",
            "--compact",
        ],
    );
    assert_eq!(out.status.code(), Some(4), "network class");
    let body: Value = serde_json::from_str(&stderr_envelope(&out)).expect("error envelope parses");
    assert_eq!(body["error"]["code"], Value::String("network_error".into()));
    let message = body["error"]["message"]
        .as_str()
        .expect("message present")
        .to_string();
    assert!(
        message.contains("STARTING"),
        "last observed state named: {message}"
    );
    assert!(message.contains("timed out"), "timeout named: {message}");
}
