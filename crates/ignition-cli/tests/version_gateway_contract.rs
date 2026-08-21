//! Binary-level exit-class contract tests for `ign version` against a
//! wiremock gateway (CORE-04 classes 4/5/6 + the LOCKED unreachable→
//! exit-0 degradation + the IGNITION_URL overlay precedence proof).
//!
//! Profiles point at the mock server via `IGNITION_CLI_CONFIG` → tempfile
//! (Pitfall 3 isolation). Together with 01-02/01-03 this completes
//! binary-level golden coverage of every exit class (CORE-04/05).
//!
//! A dead loopback port (`http://127.0.0.1:1` — nothing listens there)
//! provides instant TCP refusal for the unreachable paths; it is chosen
//! over `localhost:<real-port>` deliberately so a developer machine
//! running an actual Ignition gateway can never make these tests flaky.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::Value;

const GATEWAY_INFO_PATH: &str = "/data/api/v1/gateway-info";
/// A dead loopback port (instant refusal — see the module doc).
const DEAD_URL: &str = "http://127.0.0.1:1";

fn isolated_config() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.toml");
    (dir, path)
}

/// Write a one-profile config (`active = "dev"`) whose URL points at `url`.
fn write_profile_config(config: &Path, url: &str) {
    std::fs::write(
        config,
        format!("active = \"dev\"\n\n[profiles.dev]\nurl = \"{url}\"\n"),
    )
    .expect("write config");
}

/// Spawn `ign version --json` with an isolated config plus extra env.
fn ign_version(config: &Path, envs: &[(&str, &str)]) -> std::process::Output {
    let mut command = Command::cargo_bin("ign").expect("binary 'ign' not found");
    command.env("IGNITION_CLI_CONFIG", config);
    for (key, value) in envs {
        command.env(key, value);
    }
    command
        .args(["version", "--json"])
        .output()
        .expect("spawn ign")
}

/// Mount a 200 gateway-info mock answering with `version`.
async fn mount_gateway_info(server: &wiremock::MockServer, version: &str) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(GATEWAY_INFO_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"version": version, "edition": "Standard", "state": "RUNNING"}),
        ))
        .expect(1)
        .mount(server)
        .await;
}

/// Matrix row 2: gateway answered ≥ 8.3.1 → both versions in data, exit 0.
#[tokio::test]
async fn reachable_modern_gateway_reports_version_exit_0() {
    let server = wiremock::MockServer::start().await;
    mount_gateway_info(&server, "8.3.2").await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let out = ign_version(&config, &[]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let body: Value = serde_json::from_slice(&out.stdout).expect("envelope parses");
    assert_eq!(body["ok"], Value::Bool(true));
    assert_eq!(body["profile"], Value::String("dev".into()));
    assert_eq!(
        body["data"]["gateway"]["version"],
        Value::String("8.3.2".into())
    );
    assert_eq!(
        body["data"]["gateway"]["edition"],
        Value::String("Standard".into())
    );
    assert_eq!(
        body["data"]["gateway"]["state"],
        Value::String("RUNNING".into())
    );
    assert_eq!(body["data"]["warnings"], Value::Null, "no warnings");
}

/// Matrix row 3: gateway ANSWERED below 8.3.1 → exit 6, `gateway_too_old`
/// slug, hint naming 8.3.1, endpoint populated (CORE-05).
#[tokio::test]
async fn answered_below_minimum_refuses_exit_6() {
    let server = wiremock::MockServer::start().await;
    mount_gateway_info(&server, "8.1.14").await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let out = ign_version(&config, &[]);
    assert_eq!(out.status.code(), Some(6), "target_state class");
    assert!(out.stdout.is_empty(), "errors never touch stdout");

    let body: Value = serde_json::from_slice(&out.stderr).expect("stderr envelope parses");
    assert_eq!(body["ok"], Value::Bool(false));
    assert_eq!(
        body["error"]["code"],
        Value::String("gateway_too_old".into())
    );
    let hint = body["error"]["hint"].as_str().expect("hint (CORE-05)");
    assert!(hint.contains("8.3.1"), "hint names the minimum: {hint}");
    let message = body["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("8.1.14"),
        "message reports what was found: {message}"
    );
    assert_eq!(
        body["error"]["endpoint"],
        Value::String(format!("{}{}", server.uri(), GATEWAY_INFO_PATH)),
        "endpoint populated (CORE-05)"
    );
}

/// Exit class 5: 401 → `auth_rejected` on stderr, endpoint populated.
#[tokio::test]
async fn auth_rejected_exit_5() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(GATEWAY_INFO_PATH))
        .respond_with(wiremock::ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let out = ign_version(&config, &[]);
    assert_eq!(out.status.code(), Some(5), "auth class");
    assert!(out.stdout.is_empty(), "errors never touch stdout");

    let body: Value = serde_json::from_slice(&out.stderr).expect("stderr envelope parses");
    assert_eq!(body["error"]["code"], Value::String("auth_rejected".into()));
    assert_eq!(
        body["error"]["endpoint"],
        Value::String(format!("{}{}", server.uri(), GATEWAY_INFO_PATH))
    );
    assert!(body["error"]["hint"].is_string(), "hint present (CORE-05)");
}

/// Matrix row 4 (LOCKED): unreachable gateway → exit 0, warning inside
/// `data`, gateway null/absent — never a hard fail on a sleeping rig.
#[test]
fn unreachable_gateway_degrades_to_warning_exit_0() {
    let (_dir, config) = isolated_config();
    write_profile_config(&config, DEAD_URL);
    let out = ign_version(&config, &[]);
    assert!(
        out.status.success(),
        "unreachable must exit 0 (LOCKED); stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let body: Value = serde_json::from_slice(&out.stdout).expect("envelope parses");
    assert_eq!(body["data"]["gateway"], Value::Null, "no gateway report");
    let warnings = body["data"]["warnings"]
        .as_array()
        .expect("warnings array present");
    assert!(!warnings.is_empty(), "at least one warning");
    assert!(
        warnings[0]
            .as_str()
            .expect("warning is a string")
            .contains("gateway unreachable"),
        "warning names the problem: {warnings:?}"
    );
    // The LOCKED top-level shape never grows fields.
    let mut keys: Vec<&str> = body
        .as_object()
        .expect("envelope is an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["data", "ok", "profile"]);
}

/// Matrix row 1 regression guard: fresh install (no config) → exit 0,
/// `profile: null`, cli_version only.
#[test]
fn fresh_install_version_exit_0_profile_null() {
    let (_dir, config) = isolated_config();
    assert!(!config.exists(), "fixture sanity: no config file");
    let out = ign_version(&config, &[]);
    assert!(
        out.status.success(),
        "version must work on a fresh install; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: Value = serde_json::from_slice(&out.stdout).expect("envelope parses");
    assert_eq!(body["ok"], Value::Bool(true));
    assert_eq!(body["profile"], Value::Null);
    assert_eq!(body["data"]["gateway"], Value::Null);
    assert_eq!(body["data"]["warnings"], Value::Null);
}

/// `IGNITION_URL` overrides the profile's URL BEFORE client construction
/// (research-locked precedence flag > env > profile): the config points at
/// a dead port, the env points at a live 8.3.2 mock — the mock must win.
#[tokio::test]
async fn ignition_url_overlay_beats_profile_url_before_client_construction() {
    let server = wiremock::MockServer::start().await;
    mount_gateway_info(&server, "8.3.2").await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, DEAD_URL);
    let out = ign_version(&config, &[("IGNITION_URL", server.uri().as_str())]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let body: Value = serde_json::from_slice(&out.stdout).expect("envelope parses");
    assert_eq!(
        body["data"]["gateway"]["version"],
        Value::String("8.3.2".into()),
        "the overlay URL (live mock) must have been used, not the dead profile URL"
    );
    assert_eq!(body["data"]["warnings"], Value::Null, "no degradation");
}
