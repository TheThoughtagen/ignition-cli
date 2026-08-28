//! Golden-file contract tests for `ign backup` (07-02, BKUP-01): the
//! standalone gwbk verbs' binary surface. The restore REFUSAL is the
//! 8th-guarded-verb pin (exit 2, `profile: null`, consequence-naming
//! operation string, ZERO network — no mock needed for refusal); the
//! download goldens ride a wiremock gateway (the contract_status
//! harness: isolated `IGNITION_CLI_CONFIG` per spawn, the profile URL
//! pointed at the mock, snapbox inline goldens). Binary content never
//! goldens into snapbox — the download goldens carry the ACTION
//! envelope, the byte-exactness proof lives in ignition-core's
//! backup_contract.rs (read-back compare).

use std::path::{Path, PathBuf};

use assert_cmd::Command;

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

/// Mount the roaming download fixture (deterministic binary-ish body
/// + a Content-Disposition name).
async fn mount_backup_download(server: &wiremock::MockServer) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/backup"))
        .and(wiremock::matchers::query_param("type", "roaming"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .insert_header(
                    "Content-Disposition",
                    "attachment; filename=\"mock-gateway.gwbk\"",
                )
                .set_body_raw(b"PK\x03\x04-mock-gwbk".to_vec(), "application/octet-stream"),
        )
        .expect(1..)
        .mount(server)
        .await;
}

/// THE 8th-guarded-verb pin: `ign backup restore` without `--yes`
/// refuses at exit 2 with `profile: null` BEFORE any resolution — no
/// config exists, no gateway is contacted, the operation string names
/// the whole-gateway consequence + restart block. The envelope's
/// parsed message is pinned EXACTLY (snapbox normalizes embedded
/// quotes — the PK//x03//x04 gotcha's sibling discipline).
#[tokio::test]
async fn backup_restore_refuses_without_yes_before_resolution() {
    let (_config_dir, config) = isolated_config();
    // NO config file written, NO server mounted: a refusal that did
    // any work would fail differently (profile_not_found / network).
    let out = ign(
        &config,
        "http://127.0.0.1:1",
        &["backup", "restore", "x.gwbk", "--compact"],
    );
    assert_eq!(out.status.code(), Some(2), "usage class");
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    let body: serde_json::Value =
        serde_json::from_str(&stderr_envelope(&out)).expect("error envelope parses");
    assert_eq!(body["ok"], serde_json::Value::Bool(false));
    assert_eq!(
        body["profile"],
        serde_json::Value::Null,
        "guard fires pre-resolution"
    );
    assert_eq!(
        body["error"]["code"],
        serde_json::Value::String("confirmation_required".into())
    );
    let message = body["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("backup restore"),
        "message names the verb: {message}"
    );
    assert!(
        message.contains("overwrites this gateway's state"),
        "message names the consequence: {message}"
    );
    assert!(
        message.contains("restarts"),
        "message names the restart block: {message}"
    );
}

/// The restore refusal's HUMAN shape (the speed-bump line agents'
/// operators see): error + hint, no envelope.
#[test]
fn backup_restore_refusal_human_shape() {
    let (_config_dir, config) = isolated_config();
    let out = ign(
        &config,
        "http://127.0.0.1:1",
        &["backup", "restore", "x.gwbk"],
    );
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("is destructive; rerun with --yes"),
        "the guard line leads: {stderr}"
    );
    assert!(
        stderr.contains("backup restore"),
        "the verb is named: {stderr}"
    );
}

/// `--yes` passes the guard; the action's own file pre-check then
/// refuses `invalid_input` (exit 2, zero network — the nonexistent
/// gwbk). This proves guard-passthrough + the pre-check ordering.
#[tokio::test]
async fn backup_restore_with_yes_reaches_the_precheck() {
    let server = wiremock::MockServer::start().await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let out = ign(
        &config,
        &server.uri(),
        &["backup", "restore", "missing.gwbk", "--yes", "--compact"],
    );
    assert_eq!(out.status.code(), Some(2), "usage class (the pre-check)");
    let body: serde_json::Value =
        serde_json::from_str(&stderr_envelope(&out)).expect("error envelope parses");
    assert_eq!(body["profile"], serde_json::Value::String("dev".into()));
    assert_eq!(
        body["error"]["code"],
        serde_json::Value::String("invalid_input".into())
    );
    let message = body["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("missing.gwbk"),
        "names the file: {message}"
    );
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty(),
        "a missing file never touches the wire"
    );
}

/// Download HUMAN golden: the action envelope line (`Downloaded
/// <file> (roaming)`) under the profile header.
#[tokio::test]
async fn backup_download_human_golden() {
    let server = wiremock::MockServer::start().await;
    mount_backup_download(&server).await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let cwd = tempfile::tempdir().expect("cwd tempdir");
    let out = {
        let mut command = Command::cargo_bin("ign").expect("binary 'ign' not found");
        command
            .env("IGNITION_CLI_CONFIG", &config)
            .env("IGNITION_TOKEN", "mock:name-key")
            .env("IGNITION_URL", server.uri())
            .current_dir(cwd.path())
            .args(["backup", "download"])
            .output()
            .expect("spawn ign")
    };
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The disposition basename won the default naming.
    assert_eq!(
        std::fs::read(cwd.path().join("mock-gateway.gwbk")).unwrap(),
        b"PK\x03\x04-mock-gwbk",
        "bytes land on disk (read-back)"
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
Downloaded mock-gateway.gwbk (roaming)"#]],
    );
}

/// Download COMPACT JSON golden: the action envelope {file, type}.
/// Runs in an isolated cwd — the default-naming download writes its
/// file wherever the process stands.
#[tokio::test]
async fn backup_download_json_golden() {
    let server = wiremock::MockServer::start().await;
    mount_backup_download(&server).await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let cwd = tempfile::tempdir().expect("cwd tempdir");
    let out = {
        let mut command = Command::cargo_bin("ign").expect("binary 'ign' not found");
        command
            .env("IGNITION_CLI_CONFIG", &config)
            .env("IGNITION_TOKEN", "mock:name-key")
            .env("IGNITION_URL", server.uri())
            .current_dir(cwd.path())
            .args(["backup", "download", "--compact"])
            .output()
            .expect("spawn ign")
    };
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[
            r#"{"ok":true,"profile":"dev","data":{"file":"mock-gateway.gwbk","type":"roaming"}}"#
        ]],
    );
}

/// The `--type all` param reaches the WIRE (query pinned at the
/// binary level) and rides the result's type field.
#[tokio::test]
async fn backup_download_all_type_binary_pin() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/backup"))
        .and(wiremock::matchers::query_param("type", "all"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_raw(b"PK-all".to_vec(), "application/octet-stream"),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let cwd = tempfile::tempdir().expect("cwd tempdir");
    let out = {
        let mut command = Command::cargo_bin("ign").expect("binary 'ign' not found");
        command
            .env("IGNITION_CLI_CONFIG", &config)
            .env("IGNITION_TOKEN", "mock:name-key")
            .env("IGNITION_URL", server.uri())
            .current_dir(cwd.path())
            .args(["backup", "download", "--type", "all", "--compact"])
            .output()
            .expect("spawn ign")
    };
    assert_eq!(out.status.code(), Some(0));
    let body: serde_json::Value =
        serde_json::from_str(stdout_for_golden(&out)).expect("envelope parses");
    assert_eq!(
        body["data"]["type"],
        serde_json::Value::String("all".into())
    );
    assert_eq!(
        guard.received_requests().await.len(),
        1,
        "the wire saw ?type=all"
    );
}

/// Restore SUCCESS golden (mock gateway accepts the POST): the flat
/// `{restored: true}` envelope + the human line naming the restart.
#[tokio::test]
async fn backup_restore_success_golden() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/data/api/v1/backup"))
        .and(wiremock::matchers::query_param("restoreDisabled", "false"))
        .and(wiremock::matchers::query_param(
            "disableTempProjectBackup",
            "false",
        ))
        .and(wiremock::matchers::query_param("renameEnabled", "false"))
        .and(wiremock::matchers::query_param("restoreLocal", "false"))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1..)
        .mount(&server)
        .await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let gwbk = tempfile::tempdir().expect("tempdir");
    let file = gwbk.path().join("ok.gwbk");
    std::fs::write(&file, b"PK\x03\x04-restore-me").expect("write gwbk");

    let out = ign(
        &config,
        &server.uri(),
        &[
            "backup",
            "restore",
            file.to_str().expect("utf8 path"),
            "--yes",
            "--compact",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"restored":true}}"#]],
    );

    let out = ign(
        &config,
        &server.uri(),
        &[
            "backup",
            "restore",
            file.to_str().expect("utf8 path"),
            "--yes",
        ],
    );
    assert_eq!(out.status.code(), Some(0));
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
Restored — the gateway restarts now (blocked for ~minutes)"#]],
    );
}
