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
    },
    "tag_delete": false,
    "tag_delete_secret_rotated": false,
    "tag_delete_providers": [],
    "warnings": []
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
        snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"project":"ign-cli","routes":["tags","tagConfig","alarms","tagHistory","scriptExec"],"script_exec":true,"secret_rotated":true,"import":{"success":true},"tag_delete":false,"tag_delete_secret_rotated":false,"tag_delete_providers":[],"warnings":[]}}"#]],
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
tags         present          1.3.0
tagConfig    present          1.3.0
alarms       present          1.3.0
tagHistory   present          1.3.0
ok: all always-on routes present with matching versions
"#]],
    );

    let out = ign(&config, &server.uri(), &["webdev", "status", "--compact"]);
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"project":"ign-cli","routes":[{"route":"tags","status":"present","deployed_version":"1.3.0","expected_version":"1.3.0"},{"route":"tagConfig","status":"present","deployed_version":"1.3.0","expected_version":"1.3.0"},{"route":"alarms","status":"present","deployed_version":"1.3.0","expected_version":"1.3.0"},{"route":"tagHistory","status":"present","deployed_version":"1.3.0","expected_version":"1.3.0"}],"ok":true}}"#]],
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

/// The five tag-delete flags of `ign webdev deploy` (26-18).
const TAG_DELETE_FLAGS: [&str; 5] = [
    "--with-tag-delete",
    "--tag-delete-repo",
    "--tag-delete-user-source",
    "--tag-delete-role",
    "--rotate-tag-delete-secret",
];

/// A temp project repo whose `deploy/project.yaml` owns `PlantA` and
/// `PlantB`.
fn tag_delete_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("deploy")).expect("deploy dir");
    std::fs::write(
        dir.path().join("deploy/project.yaml"),
        "schema_version: 1\nproject: Demo\nenvironments: [dev]\ntag_providers: [PlantA, PlantB]\n",
    )
    .expect("project.yaml");
    dir
}

/// `ign webdev deploy --help` lists the five tag-delete flags.
#[test]
fn webdev_deploy_help_lists_the_tag_delete_flags() {
    let (_dir, config) = isolated_config();
    let out = ign(
        &config,
        "http://127.0.0.1:1",
        &["webdev", "deploy", "--help"],
    );
    assert!(out.status.success());
    let help = String::from_utf8_lossy(&out.stdout);
    for flag in TAG_DELETE_FLAGS {
        assert!(help.contains(flag), "help lacks {flag}:\n{help}");
    }
}

/// `--with-tag-delete` needs its three value flags, and each value flag
/// and the rotate flag need `--with-tag-delete`: clap usage, exit 2.
#[test]
fn webdev_deploy_tag_delete_flag_pairing_is_usage() {
    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://127.0.0.1:1");
    for args in [
        vec!["webdev", "deploy", "--with-tag-delete"],
        vec![
            "webdev",
            "deploy",
            "--with-tag-delete",
            "--tag-delete-repo",
            "r",
            "--tag-delete-user-source",
            "default",
        ],
        vec!["webdev", "deploy", "--tag-delete-repo", "r"],
        vec!["webdev", "deploy", "--tag-delete-user-source", "default"],
        vec!["webdev", "deploy", "--tag-delete-role", "Deploy"],
        vec!["webdev", "deploy", "--rotate-tag-delete-secret"],
    ] {
        let out = ign(&config, "http://127.0.0.1:1", &args);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {stderr}");
        assert!(
            !stderr.contains("unexpected argument"),
            "{args:?}: the flags must exist; the refusal is their pairing: {stderr}"
        );
    }
}

/// A plain deploy carries `tag_delete: false` and no providers; a
/// tag-delete deploy carries `tag_delete: true`, the repo's providers and
/// `tagDelete` last in `routes`, and its stored secret appears in no
/// output stream.
#[tokio::test]
async fn webdev_deploy_with_tag_delete_json_and_redaction() {
    let server = wiremock::MockServer::start().await;
    mount_import(&server, "ign%2Dcli").await;
    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(&config, &server.uri(), &["webdev", "deploy", "--json"]);
    assert!(out.status.success());
    let plain: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(plain["data"]["tag_delete"], Value::Bool(false));
    assert_eq!(plain["data"]["tag_delete_providers"], serde_json::json!([]));

    let repo = tag_delete_repo();
    let repo_arg = repo.path().to_str().expect("utf-8 path");
    let out = ign(
        &config,
        &server.uri(),
        &[
            "webdev",
            "deploy",
            "--json",
            "--with-tag-delete",
            "--tag-delete-repo",
            repo_arg,
            "--tag-delete-user-source",
            "default",
            "--tag-delete-role",
            "FakeDeployRole",
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let data: Value = serde_json::from_slice(&out.stdout).expect("json");
    let data = &data["data"];
    assert_eq!(data["tag_delete"], Value::Bool(true));
    assert_eq!(data["tag_delete_secret_rotated"], Value::Bool(true));
    assert_eq!(
        data["tag_delete_providers"],
        serde_json::json!(["PlantA", "PlantB"])
    );
    let routes = data["routes"].as_array().expect("routes");
    assert_eq!(routes.last(), Some(&Value::String("tagDelete".into())));

    let stored = ignition_core::config::load(&config)
        .expect("config reloads")
        .profiles
        .get("dev")
        .and_then(|profile| profile.webdev_tag_delete_secret.clone())
        .expect("tag delete secret stored");
    let all_output = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !all_output.contains(&stored),
        "redaction: tag delete secret leaked"
    );

    // A rotate generates a new secret; still never printed.
    let out = ign(
        &config,
        &server.uri(),
        &[
            "webdev",
            "deploy",
            "--json",
            "--with-tag-delete",
            "--tag-delete-repo",
            repo_arg,
            "--tag-delete-user-source",
            "default",
            "--tag-delete-role",
            "FakeDeployRole",
            "--rotate-tag-delete-secret",
        ],
    );
    assert!(out.status.success());
    let data: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(data["data"]["tag_delete_secret_rotated"], Value::Bool(true));
    let rotated = ignition_core::config::load(&config)
        .expect("config reloads")
        .profiles
        .get("dev")
        .and_then(|profile| profile.webdev_tag_delete_secret.clone())
        .expect("rotated secret stored");
    assert_ne!(rotated, stored, "rotate changed the secret");
    let all_output = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!all_output.contains(&rotated) && !all_output.contains(&stored));
}

/// 26-REVIEW-D WR-D4: a deploy without `--with-tag-delete` on a profile
/// holding the tagDelete secret removes the route and the stored secret, is
/// not refused, and warns naming the removal: in `data.warnings` (JSON) and
/// as a `warning:` line (human). The secret appears in no output stream.
#[tokio::test]
async fn webdev_deploy_without_tag_delete_removes_the_stored_secret_and_warns() {
    let server = wiremock::MockServer::start().await;
    mount_import(&server, "ign%2Dcli").await;
    let (_dir, config) = isolated_config();
    let secret = "b".repeat(64);
    let seed = |config: &Path| {
        write_profile_config(config, "http://ignored.example.com");
        let mut loaded = ignition_core::config::load(config).expect("config loads");
        loaded
            .profiles
            .get_mut("dev")
            .expect("dev")
            .webdev_tag_delete_secret = Some(secret.clone());
        ignition_core::config::save(config, &loaded).expect("seed");
    };
    let stored = |config: &Path| {
        ignition_core::config::load(config)
            .expect("config reloads")
            .profiles
            .get("dev")
            .and_then(|profile| profile.webdev_tag_delete_secret.clone())
    };

    seed(&config);
    let out = ign(&config, &server.uri(), &["webdev", "deploy", "--json"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let data: Value = serde_json::from_slice(&out.stdout).expect("json");
    let warnings = data["data"]["warnings"].as_array().expect("warnings");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    let warning = warnings[0].as_str().expect("text");
    assert!(warning.contains("tagDelete") && warning.contains("removed"));
    assert_eq!(stored(&config), None, "the stored secret is removed");
    let all_output = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!all_output.contains(&secret), "redaction");

    seed(&config);
    let out = ign(&config, &server.uri(), &["webdev", "deploy"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout
        .lines()
        .find(|line| line.starts_with("warning: "))
        .unwrap_or_else(|| panic!("no warning line: {stdout}"));
    assert!(line.contains("tagDelete") && line.contains("--with-tag-delete"));
    assert!(!stdout.contains(&secret));
    assert_eq!(stored(&config), None);
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
