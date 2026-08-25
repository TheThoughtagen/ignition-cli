//! Golden-file contract tests for `ign tags` (05-04) — provider CRUD
//! (native REST), browse/read/write (deployed routes), the guarded
//! delete, the JSON-scalar write rule, and the precondition refusal
//! — all against the BUILT binary over wiremock, harness inherited
//! from `contract_webdev.rs` (05-03): isolated
//! `IGNITION_CLI_CONFIG`, `stdout_for_golden`, the route envelope
//! shapes.
//!
//! THE crown pins:
//! - provider list/create/delete goldens incl. delete's exit-2
//!   guard zero-work pin (the 6th destructive verb, binary-pinned);
//! - browse human mode renders the INDENTED TREE with Properties
//!   filtered (JSON mode = the flat agent shape);
//! - read single+batch rows verbatim; write pins the JSON-scalar
//!   rule three ways (number / bare string / array refusal exit 2);
//! - the precondition refusal: 405 on the probe → exit 6
//!   `routes_not_deployed` with the `ign webdev deploy` hint.

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

/// stderr parsed from the first `{` (log-tolerant — the Phase-1
/// stderr-envelope convention).
fn stderr_envelope(out: &std::process::Output) -> Value {
    let stderr = String::from_utf8_lossy(&out.stderr);
    let from = stderr.find('{').expect("envelope starts somewhere");
    serde_json::from_str(&stderr[from..]).expect("envelope parses")
}

/// Mount the provider resource list (two providers — one STANDARD,
/// the MANAGED System).
async fn mount_provider_list(server: &wiremock::MockServer) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/resources/list/ignition/tag-provider",
        ))
        .and(wiremock::matchers::query_param("limit", "-1"))
        .and(wiremock::matchers::query_param("offset", "0"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {
                        "name": "default",
                        "enabled": true,
                        "config": {"profile": {"type": "STANDARD"}},
                        "metrics": {"tagCount": 12},
                        "healthchecks": {"status": "OK"}
                    },
                    {
                        "name": "System",
                        "enabled": true,
                        "config": {"profile": {"type": "MANAGED"}},
                        "metrics": {"tagCount": 3},
                        "healthchecks": {"status": "OK"}
                    }
                ],
                "metadata": {"total": 2, "matching": 2, "limit": -1, "offset": 0}
            })),
        )
        .expect(1..)
        .mount(server)
        .await;
}

/// Mount the matching version probe + ONE scripted route action on
/// the tags route (the precondition's pass + the action dispatch).
async fn mount_route_action(server: &wiremock::MockServer, action: &str, data: Value) {
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"action": "version"}),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"routeVersion": ignition_core::webdev::ROUTE_BUNDLE_VERSION, "minCli": "1.0"},
            })),
        )
        .expect(1..)
        .mount(server)
        .await;
    let action_body = serde_json::json!({"action": action});
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .and(wiremock::matchers::body_partial_json(action_body))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"ok": true, "data": data})),
        )
        .expect(1..)
        .mount(server)
        .await;
}

/// Provider list goldens: the human table (tags + health + the
/// managed marker) and the compact agent shape (unit-explicit keys,
/// all keys always).
#[tokio::test]
async fn tags_provider_list_golden() {
    let server = wiremock::MockServer::start().await;
    mount_provider_list(&server).await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(&config, &server.uri(), &["tags", "provider", "list"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
name                 enabled   tags  health
default              true        12  OK
System               true         3  OK  (managed)
"#]],
    );

    let out = ign(
        &config,
        &server.uri(),
        &["tags", "provider", "list", "--compact"],
    );
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"providers":[{"name":"default","enabled":true,"tag_count":12,"health":"OK","managed":false},{"name":"System","enabled":true,"tag_count":3,"health":"OK","managed":true}]}}"#]],
    );
}

/// Provider create golden: the array-body POST rides the native path
/// (wiremock pins the body); both render modes one-liners.
#[tokio::test]
async fn tags_provider_create_golden() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/data/api/v1/resources/ignition/tag-provider",
        ))
        .and(wiremock::matchers::body_json(serde_json::json!([
            {
                "name": "p-5e2e",
                "type": "ignition/tag-provider",
                "collection": "core",
                "enabled": true,
                "config": {"profile": {"type": "STANDARD"}, "settings": {}}
            }
        ])))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(
        &config,
        &server.uri(),
        &["tags", "provider", "create", "p-5e2e"],
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
created tag provider p-5e2e
"#]],
    );
}

/// THE delete pins: the guard refuses WITHOUT --yes (exit 2, profile
/// null, ZERO wire work) and the confirmed delete rides the
/// find→signature→delete chain.
#[tokio::test]
async fn tags_provider_delete_guard_and_chain() {
    let server = wiremock::MockServer::start().await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());

    // THE zero-work pin: refusal fires PRE-resolution — the server
    // has NO mocks mounted, so any wire work would have failed the
    // spawn outright; exit 2 + the frozen guard envelope.
    let out = ign(
        &config,
        &server.uri(),
        &["tags", "provider", "delete", "p-5e2e", "--compact"],
    );
    assert_eq!(out.status.code(), Some(2), "guard refuses without --yes");
    let envelope = stderr_envelope(&out);
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["profile"], Value::Null);
    assert_eq!(envelope["error"]["code"], "confirmation_required");
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        envelope["error"]["hint"].to_string(),
        snapbox::str![[
            r#""this operation is destructive; re-run with --yes or set IGNITION_YES=1""#
        ]],
    );

    // The confirmed chain: find (signature) → DELETE name+signature.
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/resources/find/ignition/tag-provider/p%2D5e2e",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "name": "p-5e2e",
                "enabled": true,
                "signature": "1700000000000"
            })),
        )
        .expect(1)
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path(
            "/data/api/v1/resources/ignition/tag-provider/p%2D5e2e/1700000000000",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let out = ign(
        &config,
        &server.uri(),
        &["tags", "provider", "delete", "p-5e2e", "--yes"],
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
deleted tag provider p-5e2e
"#]],
    );
}

/// Browse goldens: human renders the INDENTED TREE with Properties
/// filtered; JSON keeps the flat agent shape.
#[tokio::test]
async fn tags_browse_tree_golden() {
    let server = wiremock::MockServer::start().await;
    mount_route_action(
        &server,
        "browse",
        serde_json::json!({"results": [
            {"fullPath": "[default]", "name": "default", "tagType": "Provider", "hasChildren": true, "dataType": null},
            {"fullPath": "[default]P5", "name": "P5", "tagType": "Folder", "hasChildren": true, "dataType": null},
            {"fullPath": "[default]P5/T1", "name": "T1", "tagType": "AtomicTag", "hasChildren": false, "dataType": "Int4"},
            {"fullPath": "[default]P5/T1.value", "name": "value", "tagType": "Property", "hasChildren": false, "dataType": "Float8"}
        ]}),
    )
    .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(&config, &server.uri(), &["tags", "browse"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
browsing root
default  Provider
  P5  Folder
    T1  AtomicTag Int4
"#]],
    );

    let out = ign(&config, &server.uri(), &["tags", "browse", "--compact"]);
    assert!(out.status.success());
    let envelope: Value = serde_json::from_str(stdout_for_golden(&out)).expect("parses");
    // The flat agent shape: Properties filtered, nesting derivable
    // from `path`.
    assert_eq!(envelope["data"]["entries"].as_array().unwrap().len(), 3);
    assert_eq!(envelope["data"]["entries"][2]["path"], "[default]P5/T1");
    assert_eq!(envelope["data"]["entries"][2]["tag_type"], "AtomicTag");
    assert_eq!(envelope["data"]["include_properties"], false);
}

/// THE precondition refusal at the BINARY level: an undeployed
/// gateway (405 on every probe) → exit 6, `routes_not_deployed`,
/// hint naming `ign webdev deploy`.
#[tokio::test]
async fn tags_browse_predeploy_refusal_exit_six() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .respond_with(wiremock::ResponseTemplate::new(405))
        .expect(1)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());

    let out = ign(&config, &server.uri(), &["tags", "browse", "--compact"]);
    assert_eq!(out.status.code(), Some(6), "pre-deploy refusal is exit 6");
    let envelope = stderr_envelope(&out);
    assert_eq!(envelope["error"]["code"], "routes_not_deployed");
    let hint = envelope["error"]["hint"].as_str().unwrap();
    assert!(
        hint.contains("ign webdev deploy"),
        "hint names the fix: {hint}"
    );
}

/// Read goldens: batch rows verbatim (value raw, quality/timestamp
/// strings untouched), aligned human rows.
#[tokio::test]
async fn tags_read_single_and_batch_golden() {
    let server = wiremock::MockServer::start().await;
    mount_route_action(
        &server,
        "read",
        serde_json::json!({"results": [
            {"path": "[default]T1", "value": 7, "quality": "Good", "timestamp": "Mon Aug 24 00:00:00 UTC 2026"},
            {"path": "[default]Ghost", "value": null, "quality": "Bad_NotFound", "timestamp": "Mon Aug 24 00:00:00 UTC 2026"}
        ]}),
    )
    .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(
        &config,
        &server.uri(),
        &["tags", "read", "[default]T1", "[default]Ghost"],
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
[default]T1  =  7  [Good]  Mon Aug 24 00:00:00 UTC 2026
[default]Ghost  =  null  [Bad_NotFound]  Mon Aug 24 00:00:00 UTC 2026
"#]],
    );

    let out = ign(
        &config,
        &server.uri(),
        &["tags", "read", "[default]T1", "--compact"],
    );
    assert!(out.status.success());
    let envelope: Value = serde_json::from_str(stdout_for_golden(&out)).expect("parses");
    assert_eq!(envelope["data"]["results"][0]["value"], 7);
    assert_eq!(envelope["data"]["results"][0]["quality"], "Good");
}

/// THE write JSON-scalar rule, three ways: a number rides as a
/// number (body-pinned), a bare unparseable string rides as a
/// string, an array/objects refuses invalid_input (exit 2,
/// pre-resolution — zero wire work).
#[tokio::test]
async fn tags_write_json_scalar_rule() {
    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    // Array → exit 2 BEFORE any resolution (no server → any wire
    // work would fail; the refusal is the usage error).
    let out = ign(
        &config,
        "http://ignored.example.com",
        &[
            "tags",
            "write",
            "[default]T1",
            "--value",
            "[1,2]",
            "--compact",
        ],
    );
    assert_eq!(out.status.code(), Some(2), "non-scalar JSON refuses exit 2");
    let envelope = stderr_envelope(&out);
    assert_eq!(envelope["error"]["code"], "invalid_input");
    assert_eq!(envelope["profile"], Value::Null);

    // Number: body-pinned through the route.
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"action": "version"}),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"routeVersion": ignition_core::webdev::ROUTE_BUNDLE_VERSION, "minCli": "1.0"},
            })),
        )
        .expect(1)
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "action": "write", "path": "[default]T1", "value": 42
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"results": [{"path": "[default]T1", "quality": "Good"}]}
            })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let out = ign(
        &config,
        &server.uri(),
        &["tags", "write", "[default]T1", "--value", "42"],
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
wrote [default]T1  quality: Good
"#]],
    );

    // Bare string: unparseable text rides as the JSON string.
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"action": "version"}),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"routeVersion": ignition_core::webdev::ROUTE_BUNDLE_VERSION, "minCli": "1.0"},
            })),
        )
        .expect(1)
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "action": "write", "path": "[default]S1", "value": "hello world"
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"results": [{"path": "[default]S1", "quality": "Good"}]}
            })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let out = ign(
        &config,
        &server.uri(),
        &["tags", "write", "[default]S1", "--value", "hello world"],
    );
    assert!(
        out.status.success(),
        "bare strings ride as strings; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
