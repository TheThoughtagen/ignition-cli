//! Golden-file contract tests for `ign webdev` (05-03) — deploy +
//! status against the BUILT binary over wiremock, harness inherited
//! from `contract_resources.rs` (05-02): isolated
//! `IGNITION_CLI_CONFIG`, `stdout_for_golden`, the import + probe
//! wire shapes.
//!
//! THE crown pins:
//! - deploy success (4 routes) and `--with-script-exec` (5 routes +
//!   the secret-generated line WITHOUT the secret value — the
//!   binary-level redaction proof);
//! - status all-present (version handshake table + ok summary) and
//!   status-absent-as-DATA (exit 0, per-route degradation rows — the
//!   doctor precedent; status is a read);
//! - the status sweep rides the EXACT probe paths
//!   `/system/webdev/ign-cli/cli/{route}` (the wire protocol, NOT
//!   `/data/webdev`).

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

/// Mount the import POST (`overwrite=true`, `application/zip`) — the
/// 03-02 machinery's wire shape (the `%2D` is the Phase-3 encoder's
/// deliberate over-encoding of `-`, safe against real gateways).
async fn mount_import(server: &wiremock::MockServer, project: &str) {
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(format!(
            "/data/api/v1/projects/import/{project}"
        )))
        .and(wiremock::matchers::query_param("overwrite", "true"))
        .and(wiremock::matchers::header(
            "content-type",
            "application/zip",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"success": true})),
        )
        .expect(1..)
        .mount(server)
        .await;
}

/// Mount ONE route's version-action probe (200 ok body, the route
/// envelope shape).
async fn mount_probe_present(server: &wiremock::MockServer, route: &str, version: &str) {
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(format!(
            "/system/webdev/ign-cli/cli/{route}"
        )))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"routeVersion": version, "minCli": "1.0"},
            })),
        )
        .expect(1..)
        .mount(server)
        .await;
}

/// Mount ONE route's probe as a bare status answer (405 absent /
/// 402 unlicensed / …).
async fn mount_probe_status(server: &wiremock::MockServer, route: &str, status: u16) {
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(format!(
            "/system/webdev/ign-cli/cli/{route}"
        )))
        .respond_with(wiremock::ResponseTemplate::new(status))
        .expect(1..)
        .mount(server)
        .await;
}

/// The four always-on route folders.
const ALWAYS_ON: [&str; 4] = ["tags", "tagConfig", "alarms", "tagHistory"];

/// `ign webdev deploy` goldens: the 4-route bundle, pretty envelope +
/// human lines, riding the import wire shape.
#[tokio::test]
async fn webdev_deploy_golden() {
    let server = wiremock::MockServer::start().await;
    mount_import(&server, "ign%2Dcli").await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(&config, &server.uri(), &["webdev", "deploy", "--json"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
{
  "ok": true,
  "profile": "dev",
  "data": {
    "project": "ign-cli",
    "routes": [
      "tags",
      "tagConfig",
      "alarms",
      "tagHistory"
    ],
    "script_exec": false,
    "secret_rotated": false,
    "import": {
      "success": true
    }
  }
}
"#]],
    );

    let out = ign(&config, &server.uri(), &["webdev", "deploy"]);
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
deployed 4 routes to project ign-cli (overwrite import)
routes: tags, tagConfig, alarms, tagHistory
import: {"success":true}
"#]],
    );
}

/// `--with-script-exec` deploy: 5 routes, the secret-generated line
/// WITHOUT the value, and the BINARY-level redaction proof — the
/// stored secret (read back from the config) appears in neither
/// stdout nor stderr.
#[tokio::test]
async fn webdev_deploy_with_script_exec_golden_and_redaction() {
    let server = wiremock::MockServer::start().await;
    mount_import(&server, "ign%2Dcli").await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(
        &config,
        &server.uri(),
        &["webdev", "deploy", "--with-script-exec", "--compact"],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"project":"ign-cli","routes":["tags","tagConfig","alarms","tagHistory","scriptExec"],"script_exec":true,"secret_rotated":true,"import":{"success":true}}}"#]],
    );

    let out = ign(
        &config,
        &server.uri(),
        &["webdev", "deploy", "--with-script-exec"],
    );
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
deployed 5 routes to project ign-cli (overwrite import)
routes: tags, tagConfig, alarms, tagHistory, scriptExec
scriptExec: deployed (reusing the stored profile secret)
import: {"success":true}
"#]],
    );

    // The stored secret exists and appears in NO output stream (the
    // second run reused it — still never printed).
    let stored = stored_secret(&config).expect("secret persisted by run 1");
    let all_output = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!all_output.contains(&stored), "redaction: secret leaked");
}

/// `ign webdev status` all-present: the handshake table + ok summary
/// (human) and the per-route agent shape (compact).
#[tokio::test]
async fn webdev_status_all_present_golden() {
    let server = wiremock::MockServer::start().await;
    for route in ALWAYS_ON {
        mount_probe_present(&server, route, ignition_core::webdev::ROUTE_BUNDLE_VERSION).await;
    }

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(&config, &server.uri(), &["webdev", "status"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
tags         present          1.1.0
tagConfig    present          1.1.0
alarms       present          1.1.0
tagHistory   present          1.1.0
ok: all always-on routes present with matching versions
"#]],
    );

    let out = ign(&config, &server.uri(), &["webdev", "status", "--compact"]);
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"project":"ign-cli","routes":[{"route":"tags","status":"present","deployed_version":"1.1.0","expected_version":"1.1.0"},{"route":"tagConfig","status":"present","deployed_version":"1.1.0","expected_version":"1.1.0"},{"route":"alarms","status":"present","deployed_version":"1.1.0","expected_version":"1.1.0"},{"route":"tagHistory","status":"present","deployed_version":"1.1.0","expected_version":"1.1.0"}],"ok":true}}"#]],
    );
}

/// THE status-is-a-read pin: an UNDEPLOYED gateway (405 on every
/// probe) still exits 0 — per-route degradation is data, the ok flag
/// carries the verdict (the doctor precedent; the family-undeployed
/// exit-6 refusal belongs to WebDev-DEPENDENT commands, pinned at
/// the action level in ignition-core).
#[tokio::test]
async fn webdev_status_absent_is_data_exit_zero() {
    let server = wiremock::MockServer::start().await;
    for route in ALWAYS_ON {
        mount_probe_status(&server, route, 405).await;
    }

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(&config, &server.uri(), &["webdev", "status", "--compact"]);
    assert!(
        out.status.success(),
        "status with absent routes EXITS 0 (degradation is data); stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: Value = serde_json::from_str(stdout_for_golden(&out)).expect("envelope parses");
    assert_eq!(envelope["ok"], true, "the SWEEP succeeded: {envelope}");
    assert_eq!(envelope["data"]["ok"], false, "the verdict is data");
    for route in ALWAYS_ON {
        let row = envelope["data"]["routes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["route"] == route)
            .unwrap_or_else(|| panic!("{route} row"));
        assert_eq!(row["status"], "absent", "{route}: {row}");
        assert_eq!(row["deployed_version"], Value::Null);
        assert_eq!(
            row["expected_version"],
            ignition_core::webdev::ROUTE_BUNDLE_VERSION
        );
    }

    let out = ign(&config, &server.uri(), &["webdev", "status"]);
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
tags         absent           -
tagConfig    absent           -
alarms       absent           -
tagHistory   absent           -
degraded: run `ign webdev deploy` to install/refresh the routes
"#]],
    );
}

/// A mismatched deployed version reports version_mismatch rows
/// (data) — the refusal itself is the precondition's job; status
/// surfaces it for humans.
#[tokio::test]
async fn webdev_status_version_mismatch_rows() {
    let server = wiremock::MockServer::start().await;
    mount_probe_present(&server, "tags", "0.9.0").await;
    for route in &ALWAYS_ON[1..] {
        mount_probe_present(&server, route, ignition_core::webdev::ROUTE_BUNDLE_VERSION).await;
    }

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(&config, &server.uri(), &["webdev", "status", "--compact"]);
    assert!(out.status.success());
    let envelope: Value = serde_json::from_str(stdout_for_golden(&out)).expect("envelope parses");
    assert_eq!(envelope["data"]["ok"], false);
    let tags = envelope["data"]["routes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["route"] == "tags")
        .unwrap();
    assert_eq!(tags["status"], "version_mismatch");
    assert_eq!(tags["deployed_version"], "0.9.0");
    assert_eq!(
        tags["expected_version"],
        ignition_core::webdev::ROUTE_BUNDLE_VERSION
    );
}

/// Read the stored webdev secret back out of an isolated config
/// (test eyes only — the redaction proof's oracle).
fn stored_secret(config: &Path) -> Option<String> {
    ignition_core::config::load(config)
        .expect("config reloads")
        .profiles
        .get("dev")
        .and_then(|profile| profile.webdev_secret.clone())
}
