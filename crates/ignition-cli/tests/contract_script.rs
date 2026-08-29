//! Golden-file contract tests for `ign script run` (07-03, SCRPT-01)
//! — the binary surface of the scriptExec verb. The success goldens
//! ride a wiremock gateway (the contract_backup harness: isolated
//! `IGNITION_CLI_CONFIG` per spawn, the profile URL pointed at the
//! mock, snapbox inline goldens); the refusals need NO server — the
//! missing-secret gate refuses before any HTTP (the structural
//! opt-in's zero-cost proof at the binary level), and the
//! both-inputs usage error leads before resolution (exit 2, profile
//! null — the 03-03 put convention). The redaction canary extends
//! here: the secret hex appears in NO output path.

use std::path::{Path, PathBuf};

use assert_cmd::Command;

/// The scriptExec route path inside any wiremock server.
const ROUTE_PATH: &str = "/system/webdev/ign-cli/cli/scriptExec";

/// Isolated config dir + the config path inside it (file need not exist).
fn isolated_config() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.toml");
    (dir, path)
}

/// Write the one-profile dev config whose URL points at `url`, token
/// from `IGNITION_TOKEN`, and — the scriptExec structural gate's two
/// states — an optional persisted `webdev_secret`.
fn write_profile_config(config: &Path, url: &str, secret: Option<&str>) {
    let secret_line = secret
        .map(|secret| format!("webdev_secret = \"{secret}\"\n"))
        .unwrap_or_default();
    std::fs::write(
        config,
        format!(
            "active = \"dev\"\n\n[profiles.dev]\nurl = \"{url}\"\n\
             auth = {{ token_env = \"IGNITION_TOKEN\" }}\n{secret_line}"
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

/// Mount the probe + exec pair: the version handshake then the exec
/// POST, BOTH gated on the secret header (recorded-request proof at
/// the binary level too — the trait-level pins live in
/// ignition-core's script_contract.rs).
async fn mount_script_round(server: &wiremock::MockServer) {
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(ROUTE_PATH))
        .and(wiremock::matchers::body_json(
            serde_json::json!({"action": "version"}),
        ))
        .and(wiremock::matchers::header(
            "x-ignition-cli-secret",
            "cafebabe1234",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"routeVersion": "1.0.0", "minCli": "1.0"},
            })),
        )
        .expect(1)
        .mount(server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(ROUTE_PATH))
        .and(wiremock::matchers::body_json(
            serde_json::json!({"action": "exec", "code": "print 'hello'\n2+2"}),
        ))
        .and(wiremock::matchers::header(
            "x-ignition-cli-secret",
            "cafebabe1234",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"stdout": "hello\n", "result": 4, "elapsedMs": 12},
            })),
        )
        .expect(1)
        .mount(server)
        .await;
}

/// Success COMPACT JSON golden: the agent envelope's data carries
/// {stdout, result, elapsedMs} in the struct's declaration order —
/// `[..]` elides the dynamic elapsedMs. The redaction canary rides
/// the same spawn: the secret hex appears in NO output path.
#[tokio::test]
async fn script_run_success_json_golden() {
    let server = wiremock::MockServer::start().await;
    mount_script_round(&server).await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri(), Some("cafebabe1234"));
    let out = ign(
        &config,
        &server.uri(),
        &["script", "run", "--code", "print 'hello'\n2+2", "--compact"],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rendered = stdout_for_golden(&out);
    assert!(
        !rendered.contains("cafebabe1234"),
        "redaction: stdout never carries the secret"
    );
    assert!(
        !String::from_utf8_lossy(&out.stderr).contains("cafebabe1234"),
        "redaction: stderr never carries the secret"
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        rendered,
        // NB: snapbox normalizes backslashes in ACTUAL output to
        // forward slashes (the 03-02 gotcha) — the JSON escape
        // `\n` inside the stdout string goldens as `/n`.
        snapbox::str![[
            r#"{"ok":true,"profile":"dev","data":{"stdout":"hello/n","result":4,"elapsedMs":[..]}}"#
        ]],
    );
}

/// Success HUMAN golden: the profile header, the stdout block
/// VERBATIM, then the `result:` and `elapsed:` lines.
#[tokio::test]
async fn script_run_success_human_golden() {
    let server = wiremock::MockServer::start().await;
    mount_script_round(&server).await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri(), Some("cafebabe1234"));
    let out = ign(
        &config,
        &server.uri(),
        &["script", "run", "--code", "print 'hello'\n2+2"],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
stdout:
hello
result: 4
elapsed: 12 ms"#]],
    );
}

/// THE structural gate at the binary level: a profile with NO
/// persisted `webdev_secret` (the route was never deployed through
/// the opt-in flag) refuses exit 6 `script_exec_not_configured`
/// whose hint names the deploy flag verbatim — with ZERO HTTP
/// requests (an empty server: any request would have failed the
/// assert on received_requests).
#[tokio::test]
async fn script_run_missing_secret_refuses_exit_6_zero_http() {
    let server = wiremock::MockServer::start().await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri(), None);
    let out = ign(
        &config,
        &server.uri(),
        &["script", "run", "--code", "2+2", "--compact"],
    );
    assert_eq!(out.status.code(), Some(6), "target state");
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    let body: serde_json::Value =
        serde_json::from_str(&stderr_envelope(&out)).expect("error envelope parses");
    assert_eq!(body["ok"], serde_json::Value::Bool(false));
    assert_eq!(body["profile"], serde_json::Value::String("dev".into()));
    assert_eq!(
        body["error"]["code"],
        serde_json::Value::String("script_exec_not_configured".into())
    );
    let hint = body["error"]["hint"].as_str().expect("hint rides");
    assert!(
        hint.contains("ign webdev deploy --with-script-exec"),
        "the hint names the deploy flag verbatim: {hint}"
    );
    assert!(
        server
            .received_requests()
            .await
            .expect("received requests")
            .is_empty(),
        "the refusal performs ZERO HTTP requests"
    );
}

/// The missing-secret refusal's HUMAN shape: error + hint lines, no
/// envelope.
#[test]
fn script_run_missing_secret_human_shape() {
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, "http://127.0.0.1:1", None);
    let out = ign(
        &config,
        "http://127.0.0.1:1",
        &["script", "run", "--code", "2+2"],
    );
    assert_eq!(out.status.code(), Some(6));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("scriptExec is not configured"),
        "the refusal names the state: {stderr}"
    );
    assert!(
        stderr.contains("ign webdev deploy --with-script-exec"),
        "the hint names the deploy flag: {stderr}"
    );
}

/// Both input forms given → `invalid_input` exit 2 with profile
/// null, BEFORE any resolution (no config file exists, no server is
/// reachable — the usage error leads, the 03-03 put convention).
#[test]
fn script_run_both_inputs_refuse_invalid_input() {
    let (_config_dir, config) = isolated_config();
    // NO config written, unreachable URL: any resolution work would
    // fail differently (profile_not_found / network).
    let out = ign(
        &config,
        "http://127.0.0.1:1",
        &[
            "script",
            "run",
            "--code",
            "2+2",
            "--file",
            "snippet.py",
            "--compact",
        ],
    );
    assert_eq!(out.status.code(), Some(2), "usage class");
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    let body: serde_json::Value =
        serde_json::from_str(&stderr_envelope(&out)).expect("error envelope parses");
    assert_eq!(body["profile"], serde_json::Value::Null, "usage leads");
    assert_eq!(
        body["error"]["code"],
        serde_json::Value::String("invalid_input".into())
    );
    let message = body["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("--code") && message.contains("--file"),
        "the reason names both flags: {message}"
    );
}
