//! Golden-file contract tests for `ign jvm …` — the SPECIAL-MODE
//! profiling family's binary surface. The pins that matter here are
//! the GATES, in order:
//!
//! 1. The intrusive verbs refuse WITHOUT `--i-accept-the-risk` — exit
//!    2, `profiling_risk_not_accepted`, ZERO HTTP requests (the gate
//!    precedes even the secret handshake).
//! 2. The global `--yes` deliberately does NOT satisfy that gate
//!    (the anti-bypass pin — a blanket confirm must never cover
//!    ACTIVE JVM instrumentation).
//! 3. Over-cap durations refuse `invalid_input` (exit 2, profile
//!    null) BEFORE profile resolution — the caps are refused, never
//!    clamped (usage errors lead, the 03-03 put convention).
//! 4. The ungated halves (`jfr status`, `jfr stop`) reach the
//!    gateway with no flag at all — checking and cleaning up must
//!    always be cheap to reach.
//!
//! The success goldens ride the contract_script harness shape:
//! isolated config with a persisted `webdev_secret`, a wiremock
//! scriptExec route, snapbox inline goldens. Every success carries
//! the `warning` field FIRST (the production caution rides every
//! envelope) and the human renders lead with the stderr banner.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use base64::Engine as _;

/// The scriptExec route path inside any wiremock server.
const ROUTE_PATH: &str = "/system/webdev/ign-cli/cli/scriptExec";

/// Isolated config dir + the config path inside it (file need not exist).
fn isolated_config() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.toml");
    (dir, path)
}

/// Write the one-profile dev config whose URL points at `url`, token
/// from `IGNITION_TOKEN`, and the persisted scriptExec secret (the
/// structural opt-in's deployed state).
fn write_profile_config(config: &Path, url: &str) {
    std::fs::write(
        config,
        format!(
            "active = \"dev\"\n\n[profiles.dev]\nurl = \"{url}\"\n\
             auth = {{ token_env = \"IGNITION_TOKEN\" }}\n\
             webdev_secret = \"cafebabe1234\"\n"
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

/// Mount the version handshake + ONE exec answering `payload` (the
/// Jython's jsonEncode string — the route returns it as
/// `data.result`), both gated on the secret header.
async fn mount_exec_round(server: &wiremock::MockServer, payload: &str) {
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
                "data": {"routeVersion": "1.3.0", "minCli": "1.0"},
            })),
        )
        .expect(1)
        .mount(server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(ROUTE_PATH))
        .and(wiremock::matchers::body_string_contains(
            "\"action\":\"exec\"",
        ))
        .and(wiremock::matchers::header(
            "x-ignition-cli-secret",
            "cafebabe1234",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"stdout": "", "result": payload, "elapsedMs": 12},
            })),
        )
        .expect(1)
        .mount(server)
        .await;
}

/// The sample payload the mock route returns (the Jython's
/// jsonEncode output — a JSON STRING riding `data.result`).
const SAMPLE_PAYLOAD: &str = r#"{"samples":100,"threadPeak":42,"observations":100,"wallMs":5012,"hot":[{"frame":"com.foo.Bar.baz","count":61},{"frame":"java.net.SocketInputStream.socketRead0","count":20}],"stacks":[{"stack":"java.lang.Thread.run;com.foo.Bar.baz","count":61}],"folded":"java.lang.Thread.run;com.foo.Bar.baz 61"}"#;

// ── Gate 1: no flag → refusal, ZERO HTTP ────────────────────────────

/// THE special-mode gate at the binary level: `ign jvm sample`
/// WITHOUT `--i-accept-the-risk` refuses exit 2
/// `profiling_risk_not_accepted` — BEFORE any HTTP (the gate
/// precedes even the secret handshake), with the hint stating that
/// --yes cannot satisfy it.
#[tokio::test]
async fn sample_without_risk_flag_refuses_zero_http() {
    let server = wiremock::MockServer::start().await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let out = ign(&config, &server.uri(), &["jvm", "sample", "--compact"]);
    assert_eq!(out.status.code(), Some(2), "usage class");
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    let body: serde_json::Value =
        serde_json::from_str(&stderr_envelope(&out)).expect("error envelope parses");
    assert_eq!(
        body["error"]["code"],
        serde_json::Value::String("profiling_risk_not_accepted".into())
    );
    let hint = body["error"]["hint"].as_str().expect("hint rides");
    assert!(
        hint.contains("--i-accept-the-risk"),
        "the hint names the flag: {hint}"
    );
    assert!(
        hint.contains("--yes deliberately does NOT"),
        "the hint states the --yes non-equivalence: {hint}"
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

/// Gate 2 — THE ANTI-BYPASS PIN: the global `--yes` does NOT satisfy
/// the risk gate. A generic destructive-ops blanket must never cover
/// ACTIVE JVM instrumentation.
#[tokio::test]
async fn sample_yes_flag_does_not_satisfy_the_gate() {
    let server = wiremock::MockServer::start().await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let out = ign(
        &config,
        &server.uri(),
        &["jvm", "sample", "--yes", "--compact"],
    );
    assert_eq!(out.status.code(), Some(2), "still the usage class");
    let body: serde_json::Value =
        serde_json::from_str(&stderr_envelope(&out)).expect("error envelope parses");
    assert_eq!(
        body["error"]["code"],
        serde_json::Value::String("profiling_risk_not_accepted".into()),
        "--yes never satisfies the profiling gate"
    );
    assert!(
        server
            .received_requests()
            .await
            .expect("received requests")
            .is_empty(),
        "still ZERO HTTP requests"
    );
}

/// `ign jvm jfr start` carries the same gate (the other intrusive verb).
#[tokio::test]
async fn jfr_start_without_risk_flag_refuses() {
    let server = wiremock::MockServer::start().await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let out = ign(
        &config,
        &server.uri(),
        &["jvm", "jfr", "start", "--compact"],
    );
    assert_eq!(out.status.code(), Some(2));
    let body: serde_json::Value =
        serde_json::from_str(&stderr_envelope(&out)).expect("error envelope parses");
    assert_eq!(
        body["error"]["code"],
        serde_json::Value::String("profiling_risk_not_accepted".into())
    );
    assert!(
        server
            .received_requests()
            .await
            .expect("received requests")
            .is_empty(),
        "ZERO HTTP requests"
    );
}

// ── Gate 3: the caps refuse (never clamp), usage leads ─────────────

/// An over-cap duration refuses `invalid_input` (exit 2, profile
/// NULL) BEFORE profile resolution — no config file exists and no
/// server is reachable, so any resolution work would fail differently.
#[test]
fn sample_above_the_hard_cap_refuses_invalid_input_profile_null() {
    let (_config_dir, config) = isolated_config();
    let out = ign(
        &config,
        "http://127.0.0.1:1",
        &[
            "jvm",
            "sample",
            "--duration-secs",
            "600",
            "--i-accept-the-risk",
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
        message.contains("600s") && message.contains("120s"),
        "the refusal names the ask AND the cap: {message}"
    );
}

/// The JFR cap refuses the same way.
#[test]
fn jfr_start_above_the_hard_cap_refuses_invalid_input() {
    let (_config_dir, config) = isolated_config();
    let out = ign(
        &config,
        "http://127.0.0.1:1",
        &[
            "jvm",
            "jfr",
            "start",
            "--duration-secs",
            "999",
            "--i-accept-the-risk",
            "--compact",
        ],
    );
    assert_eq!(out.status.code(), Some(2));
    let body: serde_json::Value =
        serde_json::from_str(&stderr_envelope(&out)).expect("error envelope parses");
    assert_eq!(body["profile"], serde_json::Value::Null);
    assert_eq!(
        body["error"]["code"],
        serde_json::Value::String("invalid_input".into())
    );
    assert!(
        body["error"]["message"]
            .as_str()
            .expect("message")
            .contains("300s"),
        "names the JFR cap"
    );
}

/// `--follow` + `--folded-out` refuse together — the sidecar writes
/// ONE window's stacks; in continuous mode each window would
/// overwrite it. Exit 2 `invalid_input`, profile null, BEFORE any
/// resolution (no config, no reachable server).
#[test]
fn sample_follow_and_folded_out_refuse_together() {
    let (_config_dir, config) = isolated_config();
    let out = ign(
        &config,
        "http://127.0.0.1:1",
        &[
            "jvm",
            "sample",
            "--follow",
            "--folded-out",
            "profile.folded",
            "--i-accept-the-risk",
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
        message.contains("--folded-out") && message.contains("--follow"),
        "the reason names both flags: {message}"
    );
}

// ── Success paths ───────────────────────────────────────────────────

/// `ign jvm sample` success COMPACT JSON golden: `warning` leads the
/// data (the production caution rides every envelope), the rollup
/// fields follow in declaration order — `[..]` elides the dynamic
/// elapsedMs.
#[tokio::test]
async fn sample_success_compact_json_golden() {
    let server = wiremock::MockServer::start().await;
    mount_exec_round(&server, SAMPLE_PAYLOAD).await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let out = ign(
        &config,
        &server.uri(),
        &["jvm", "sample", "--i-accept-the-risk", "--compact"],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[
            r#"{"ok":true,"profile":"dev","data":{"warning":"JVM PROFILING ACTIVELY INSTRUMENTS THE GATEWAY: sampling adds CPU overhead proportional to thread count x frequency; flight recordings buffer in JVM memory and dump to gateway disk. NEVER use this on a production server outside an approved change window with a rollback plan — and always use the shortest duration that answers the question.","duration_secs":30,"interval_ms":20,"samples":100,"thread_peak":42,"observations":100,"wall_ms":5012,"hot_methods":[{"frame":"com.foo.Bar.baz","count":61,"pct":61.0},{"frame":"java.net.SocketInputStream.socketRead0","count":20,"pct":20.0}],"top_stacks":[{"stack":"java.lang.Thread.run;com.foo.Bar.baz","count":61,"pct":61.0}],"folded":"java.lang.Thread.run;com.foo.Bar.baz 61","elapsedMs":[..]}}"#
        ]],
    );
}

/// The success HUMAN golden: the report on stdout, the banner on
/// STDERR (the pipe never swallows the caution).
#[tokio::test]
async fn sample_success_human_golden_and_stderr_banner() {
    let server = wiremock::MockServer::start().await;
    mount_exec_round(&server, SAMPLE_PAYLOAD).await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let out = ign(
        &config,
        &server.uri(),
        &["jvm", "sample", "--i-accept-the-risk"],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("JVM PROFILING — ACTIVE GATEWAY INSTRUMENTATION"),
        "the stderr banner rides the human render: {stderr}"
    );
    assert!(
        stderr.contains("production"),
        "the banner carries the production caution: {stderr}"
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
sampled 100 times over a 30s window (wall 5012 ms — the 20ms interval rides on dump cost; 100 thread stacks observed, peak 42 live)

hot methods (self):
   61.0%      61  com.foo.Bar.baz
   20.0%      20  java.net.SocketInputStream.socketRead0

heaviest stacks:
   61.0%      61  java.lang.Thread.run;com.foo.Bar.baz"#]],
    );
}

/// The folded sidecar: `--folded-out` writes the flamegraph input
/// byte-exactly.
#[tokio::test]
async fn sample_folded_out_writes_the_sidecar() {
    let server = wiremock::MockServer::start().await;
    mount_exec_round(&server, SAMPLE_PAYLOAD).await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let out_dir = tempfile::tempdir().expect("tempdir");
    let folded = out_dir.path().join("profile.folded");
    let out = ign(
        &config,
        &server.uri(),
        &[
            "jvm",
            "sample",
            "--i-accept-the-risk",
            "--folded-out",
            folded.to_str().expect("utf-8 path"),
            "--compact",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(&folded).expect("sidecar written"),
        "java.lang.Thread.run;com.foo.Bar.baz 61"
    );
}

/// Gate 4: `ign jvm jfr status` is UNGATED — no flag, straight to
/// the gateway (checking is how you confirm cleanup).
#[tokio::test]
async fn jfr_status_is_ungated_human_golden() {
    let server = wiremock::MockServer::start().await;
    mount_exec_round(
        &server,
        r#"{"recordings":"Recording 1: name=ign-cli duration=60s (running)","active":true}"#,
    )
    .await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let out = ign(&config, &server.uri(), &["jvm", "jfr", "status"]);
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
ign-cli recording: ACTIVE
recordings (jfrCheck verbatim):
  Recording 1: name=ign-cli duration=60s (running)"#]],
    );
}

/// `ign jvm jfr start` success: the recording's terms + the cleanup
/// path named.
#[tokio::test]
async fn jfr_start_success_human_golden() {
    let server = wiremock::MockServer::start().await;
    mount_exec_round(
        &server,
        r#"{"output":"Started recording 1.","recordings":"Recording 1: name=ign-cli (running)"}"#,
    )
    .await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let out = ign(
        &config,
        &server.uri(),
        &["jvm", "jfr", "start", "--i-accept-the-risk"],
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
started flight recording 'ign-cli' (60s, settings=default) — memory-buffered, auto-stops at the deadline
jfr: Started recording 1.
fetch + clean up with: ign jvm jfr stop   (check: ign jvm jfr status)"#]],
    );
}

/// `ign jvm jfr stop` (UNGATED): the dump streams back base64, lands
/// byte-exact at `-o`, and the remote delete is reported.
#[tokio::test]
async fn jfr_stop_fetches_byte_exact_and_reports_remote_delete() {
    let payload = format!(
        r#"{{"remoteFile":"/tmp/ign-cli-123.jfr","bytes":11,"tooLarge":false,"data":"{}","deleted":true}}"#,
        base64::engine::general_purpose::STANDARD.encode(b"hello jfr!!")
    );
    let server = wiremock::MockServer::start().await;
    mount_exec_round(&server, &payload).await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let out_dir = tempfile::tempdir().expect("tempdir");
    let jfr = out_dir.path().join("capture.jfr");
    let out = ign(
        &config,
        &server.uri(),
        &[
            "jvm",
            "jfr",
            "stop",
            "-o",
            jfr.to_str().expect("utf-8 path"),
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        std::fs::read(&jfr).expect("dump written"),
        b"hello jfr!!",
        "the .jfr lands byte-exact"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("stopped flight recording 'ign-cli'"),
        "the stop reports: {stdout}"
    );
    assert!(
        stdout.contains("/tmp/ign-cli-123.jfr: deleted"),
        "the remote delete is reported: {stdout}"
    );
}

/// The over-cap stop: exit 0, `fetched: false`, the dump LEFT IN
/// PLACE with its path reported on stderr — never silently truncated.
#[tokio::test]
async fn jfr_stop_over_cap_leaves_the_dump_in_place() {
    let server = wiremock::MockServer::start().await;
    mount_exec_round(
        &server,
        r#"{"remoteFile":"/tmp/ign-cli-big.jfr","bytes":40000000,"tooLarge":true,"data":null,"deleted":false}"#,
    )
    .await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let out_dir = tempfile::tempdir().expect("tempdir");
    let jfr = out_dir.path().join("capture.jfr");
    let out = ign(
        &config,
        &server.uri(),
        &[
            "jvm",
            "jfr",
            "stop",
            "-o",
            jfr.to_str().expect("utf-8 path"),
            "--compact",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!jfr.exists(), "no local file — nothing was fetched");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(r#""fetched":false"#),
        "the envelope says fetched:false: {stdout}"
    );
    assert!(
        stdout.contains("/tmp/ign-cli-big.jfr"),
        "the remote path rides the envelope: {stdout}"
    );
}

/// The structural gate reaches this family too: no persisted secret
/// (the route was never deployed) refuses exit 6
/// `script_exec_not_configured` — WITH the risk flag passed (the
/// deploy opt-in is the outer gate; zero HTTP).
#[tokio::test]
async fn sample_missing_secret_refuses_exit_6_zero_http() {
    let server = wiremock::MockServer::start().await;
    let (_config_dir, config) = isolated_config();
    std::fs::write(
        &config,
        format!(
            "active = \"dev\"\n\n[profiles.dev]\nurl = \"{}\"\n\
             auth = {{ token_env = \"IGNITION_TOKEN\" }}\n",
            server.uri()
        ),
    )
    .expect("write config");
    let out = ign(
        &config,
        &server.uri(),
        &["jvm", "sample", "--i-accept-the-risk", "--compact"],
    );
    assert_eq!(out.status.code(), Some(6), "target state");
    let body: serde_json::Value =
        serde_json::from_str(&stderr_envelope(&out)).expect("error envelope parses");
    assert_eq!(
        body["error"]["code"],
        serde_json::Value::String("script_exec_not_configured".into())
    );
    assert!(
        server
            .received_requests()
            .await
            .expect("received requests")
            .is_empty(),
        "ZERO HTTP requests"
    );
}
