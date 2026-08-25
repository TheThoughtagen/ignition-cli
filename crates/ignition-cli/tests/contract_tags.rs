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

/// `ign` with a JSON document piped to stdin (`--file -`) and a
/// pinned working directory (the default-file export golden).
fn ign_stdin(
    config: &Path,
    url: &str,
    args: &[&str],
    stdin: &str,
    cwd: Option<&Path>,
) -> std::process::Output {
    let mut command = Command::cargo_bin("ign").expect("binary 'ign' not found");
    command
        .env("IGNITION_CLI_CONFIG", config)
        .env("IGNITION_TOKEN", "mock:name-key")
        .env("IGNITION_URL", url);
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }
    command
        .args(args)
        .write_stdin(stdin)
        .output()
        .expect("spawn ign")
}

/// Mount the matching version probe on the TAGS route (the
/// precondition's pass) — repeatable: one probe per action.
async fn mount_tags_probe(server: &wiremock::MockServer) {
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
}

/// Mount ONE scripted action on the tagConfig route (the
/// precondition probe included).
async fn mount_tagconfig_action(server: &wiremock::MockServer, action: &str, data: Value) {
    mount_tags_probe(server).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/tagConfig",
        ))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"action": action}),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"ok": true, "data": data})),
        )
        .expect(1..)
        .mount(server)
        .await;
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

// ---- 05-05 goldens: config CRUD, UDTs, export/import ----

/// config get goldens: human = path+tagType header then PRETTY JSON
/// (the stringified values RE-PARSED and visible); compact = the
/// stable agent envelope.
#[tokio::test]
async fn tags_config_get_golden() {
    let server = wiremock::MockServer::start().await;
    mount_tagconfig_action(
        &server,
        "getConfig",
        serde_json::json!({"config": {
            "name": "T1",
            "tagType": "AtomicTag",
            "value": "{\"dataType\": \"Int4\", \"value\": 123}"
        }}),
    )
    .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(
        &config,
        &server.uri(),
        &["tags", "config", "get", "[default]T1"],
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
[default]T1  AtomicTag
{
  "name": "T1",
  "tagType": "AtomicTag",
  "value": {
    "dataType": "Int4",
    "value": 123
  }
}
"#]],
    );

    let out = ign(
        &config,
        &server.uri(),
        &["tags", "config", "get", "[default]T1", "--compact"],
    );
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[
            r#"{"ok":true,"profile":"dev","data":{"project":"ign-cli","path":"[default]T1","tag_type":"AtomicTag","config":{"name":"T1","tagType":"AtomicTag","value":{"dataType":"Int4","value":123}}}}"#
        ]],
    );
}

/// config create/edit goldens: the definition rides `--file -`
/// (stdin, resource-put precedent), the configure body pins the
/// basePath split + collisionPolicy char, and the success lines name
/// the verb + quality.
#[tokio::test]
async fn tags_config_create_edit_golden() {
    let server = wiremock::MockServer::start().await;
    mount_tags_probe(&server).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/tagConfig",
        ))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "action": "configure",
            "basePath": "[default]P5",
            "tags": [{"tagType": "AtomicTag", "value": 42, "name": "T1"}],
            "collisionPolicy": "a"
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true, "data": {"results": ["Good"]}
            })),
        )
        .expect(1)
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/tagConfig",
        ))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "action": "configure",
            "basePath": "[default]P5",
            "tags": [{"tagType": "AtomicTag", "value": 99, "name": "T1"}],
            "collisionPolicy": "o"
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true, "data": {"results": ["Good"]}
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign_stdin(
        &config,
        &server.uri(),
        &["tags", "config", "create", "[default]P5/T1", "--file", "-"],
        r#"{"tagType": "AtomicTag", "value": 42}"#,
        None,
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
created [default]P5/T1  quality: Good
"#]],
    );

    let out = ign_stdin(
        &config,
        &server.uri(),
        &["tags", "config", "edit", "[default]P5/T1", "--file", "-"],
        r#"{"tagType": "AtomicTag", "value": 99}"#,
        None,
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
edited [default]P5/T1  quality: Good
"#]],
    );
}

/// THE config-delete pins: the guard refuses WITHOUT --yes (exit 2,
/// profile null, ZERO wire work — no mocks mounted) and the
/// confirmed delete pins the batch deleteTags body.
#[tokio::test]
async fn tags_config_delete_guard_and_body() {
    let server = wiremock::MockServer::start().await;
    mount_tagconfig_action(&server, "deleteTags", serde_json::json!({"deleted": 2})).await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());

    // THE zero-work pin: refusal fires PRE-resolution (the server's
    // mocks only answer the confirmed run).
    let out = ign(
        &config,
        &server.uri(),
        &["tags", "config", "delete", "[default]T1", "--compact"],
    );
    assert_eq!(out.status.code(), Some(2), "guard refuses without --yes");
    let envelope = stderr_envelope(&out);
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["profile"], Value::Null);
    assert_eq!(envelope["error"]["code"], "confirmation_required");

    let out = ign(
        &config,
        &server.uri(),
        &[
            "tags",
            "config",
            "delete",
            "[default]T1",
            "[default]T2",
            "--yes",
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
deleted 2 tag config(s)
"#]],
    );
    // The deleteTags body carried BOTH paths.
    let received = server.received_requests().await.expect("requests recorded");
    let delete = received
        .iter()
        .rev()
        .find(|request| String::from_utf8_lossy(&request.body).contains("deleteTags"))
        .expect("deleteTags request recorded");
    let body: Value = serde_json::from_slice(&delete.body).expect("body parses");
    assert_eq!(
        body["paths"],
        serde_json::json!(["[default]T1", "[default]T2"])
    );
}

/// UDT goldens: types = provider header + name/tagType rows; def =
/// the `_types_` path header + the recursive definition as pretty
/// JSON (re-parse applied).
#[tokio::test]
async fn tags_udt_goldens() {
    let server = wiremock::MockServer::start().await;
    // TWO actions run (types + def) — the probe answers repeatedly.
    mount_tagconfig_action(
        &server,
        "listUDTTypes",
        serde_json::json!({"results": [
            {"fullPath": "[default]_types_/Motor", "name": "Motor", "tagType": "UdtType", "hasChildren": true, "dataType": null},
            {"fullPath": "[default]_types_/Pump", "name": "Pump", "tagType": "UdtType", "hasChildren": true, "dataType": null}
        ]}),
    )
    .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(&config, &server.uri(), &["tags", "udt", "types"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
provider default
Motor  UdtType
Pump  UdtType
"#]],
    );

    // def: its own server (one probe each keeps the mocks honest).
    let server = wiremock::MockServer::start().await;
    mount_tagconfig_action(
        &server,
        "getUDTDefinition",
        serde_json::json!({"definition": {
            "name": "Motor",
            "tagType": "UdtType",
            "parameters": {"speed": {"defaultValue": "{\"dataType\": \"Float8\", \"value\": 0.0}"}}
        }}),
    )
    .await;
    let out = ign(
        &config,
        &server.uri(),
        &["tags", "udt", "def", "Motor", "--provider", "default"],
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
[default]_types_/Motor
{
  "name": "Motor",
  "parameters": {
    "speed": {
      "defaultValue": {
        "dataType": "Float8",
        "value": 0.0
      }
    }
  },
  "tagType": "UdtType"
}
"#]],
    );
}

/// Export goldens: default mode writes the pretty JSON to
/// `<last-segment>.json` in the cwd (artifact line); `-o -` prints
/// the payload RAW in every mode (the fourth sanctioned stdout
/// exception — no envelope even under --compact).
#[tokio::test]
async fn tags_export_goldens() {
    let payload = serde_json::json!([
        {"name": "P5", "tagType": "Folder", "tags": [
            {"name": "T1", "tagType": "AtomicTag", "value": "{\"dataType\": \"Int4\", \"value\": 123}"}
        ]}
    ]);
    let export_data = serde_json::json!({
        "payload": serde_json::to_string(&payload).expect("serializes")
    });

    let server = wiremock::MockServer::start().await;
    mount_tagconfig_action(&server, "exportTags", export_data.clone()).await;
    let server_stdout = wiremock::MockServer::start().await;
    mount_tagconfig_action(&server_stdout, "exportTags", export_data).await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    // Default file: <last-segment>.json in the cwd (pinned tempdir).
    let cwd = tempfile::tempdir().expect("tempdir");
    let out = ign_stdin(
        &config,
        &server.uri(),
        &["tags", "export", "[p5e2e]P5"],
        "",
        Some(cwd.path()),
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
exported 1 path(s) → P5.json (1 tag(s))
"#]],
    );
    let written: Value = serde_json::from_str(
        &std::fs::read_to_string(cwd.path().join("P5.json")).expect("default file written"),
    )
    .expect("pretty JSON parses");
    assert_eq!(written, payload);

    // `-o -` (stdout): RAW pretty payload in EVERY mode — even
    // --compact prints no envelope.
    let out = ign(
        &config,
        &server_stdout.uri(),
        &["tags", "export", "[p5e2e]P5", "-o", "-", "--compact"],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        // NOTE (the 03-02 snapbox gotcha): str! normalizes
        // backslashes in ACTUAL output to forward slashes — the
        // stringified value's escapes golden as /" not \".
        snapbox::str![[r#"
[
  {
    "name": "P5",
    "tagType": "Folder",
    "tags": [
      {
        "name": "T1",
        "tagType": "AtomicTag",
        "value": "{/"dataType/": /"Int4/", /"value/": 123}"
      }
    ]
  }
]
"#]],
    );
}

/// Import goldens — the LOCKED collision matrix at the binary level:
/// abort-clean imports (browse pre-check + configure 'a'), a
/// collision refuses exit 6 `tag_collision` with the overwrite hint,
/// overwrite guards on --yes (exit 2 zero-work) then succeeds with
/// configure 'o' and no pre-check.
#[tokio::test]
async fn tags_import_collision_matrix_goldens() {
    let payload = serde_json::json!([{"name": "T1", "tagType": "AtomicTag"}]);
    let payload_text = serde_json::to_string(&payload).expect("serializes");

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");
    let file_dir = tempfile::tempdir().expect("tempdir");
    let payload_file = file_dir.path().join("p5.json");
    std::fs::write(&payload_file, &payload_text).expect("write payload");

    // (1) abort + clean target: browse (empty) → configure 'a' → the
    // counts+provider line.
    let server = wiremock::MockServer::start().await;
    mount_tags_probe(&server).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"action": "browse", "path": "[p5import]"}),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true, "data": {"results": []}
            })),
        )
        .expect(1)
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/tagConfig",
        ))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"action": "configure"}),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true, "data": {"results": ["Good"]}
            })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let out = ign(
        &config,
        &server.uri(),
        &[
            "tags",
            "import",
            "--file",
            payload_file.to_str().unwrap(),
            "--provider",
            "p5import",
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
imported 1 tag(s) into p5import (abort)
"#]],
    );

    // (2) abort + collision: browse finds T1 → exit 6 tag_collision
    // with the overwrite hint (the configure mock is absent — zero
    // writes even reach the wire matcher).
    let server = wiremock::MockServer::start().await;
    mount_tags_probe(&server).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"action": "browse", "path": "[p5import]"}),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true, "data": {"results": [
                    {"fullPath": "[p5import]T1", "name": "T1", "tagType": "AtomicTag", "hasChildren": false, "dataType": "Int4"}
                ]}
            })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let out = ign(
        &config,
        &server.uri(),
        &[
            "tags",
            "import",
            "--file",
            payload_file.to_str().unwrap(),
            "--provider",
            "p5import",
            "--compact",
        ],
    );
    assert_eq!(out.status.code(), Some(6), "collision refuses exit 6");
    let envelope = stderr_envelope(&out);
    assert_eq!(envelope["error"]["code"], "tag_collision");
    assert!(
        envelope["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("T1")
    );

    // (3) overwrite guards: without --yes → exit 2, profile null,
    // zero wire work (the server has no mocks mounted).
    let out = ign(
        &config,
        "http://ignored.example.com",
        &[
            "tags",
            "import",
            "--file",
            payload_file.to_str().unwrap(),
            "--provider",
            "p5import",
            "--collision-policy",
            "overwrite",
            "--compact",
        ],
    );
    assert_eq!(out.status.code(), Some(2), "overwrite guards on --yes");
    let envelope = stderr_envelope(&out);
    assert_eq!(envelope["error"]["code"], "confirmation_required");
    assert_eq!(envelope["profile"], Value::Null);

    // (4) overwrite --yes: NO browse pre-check — configure 'o' is
    // the only route call past the probe.
    let server = wiremock::MockServer::start().await;
    mount_tags_probe(&server).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/tagConfig",
        ))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "action": "configure",
            "basePath": "[p5import]",
            "tags": payload,
            "collisionPolicy": "o"
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true, "data": {"results": ["Good"]}
            })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let out = ign(
        &config,
        &server.uri(),
        &[
            "tags",
            "import",
            "--file",
            payload_file.to_str().unwrap(),
            "--provider",
            "p5import",
            "--collision-policy",
            "overwrite",
            "--yes",
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
imported 1 tag(s) into p5import (overwrite)
"#]],
    );
}

/// The malformed-JSON usage refusals (InvalidInput class,
/// pre-resolution): a bad definition file and a bad payload file
/// both exit 2 with zero wire work.
#[tokio::test]
async fn tags_json_input_usage_refusals() {
    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");
    let dir = tempfile::tempdir().expect("tempdir");
    let bad = dir.path().join("bad.json");
    std::fs::write(&bad, "not json {").expect("write bad file");

    let out = ign_stdin(
        &config,
        "http://ignored.example.com",
        &[
            "tags",
            "config",
            "create",
            "[default]T1",
            "--file",
            "-",
            "--compact",
        ],
        "{not json",
        None,
    );
    assert_eq!(out.status.code(), Some(2));
    let envelope = stderr_envelope(&out);
    assert_eq!(envelope["error"]["code"], "invalid_input");

    let out = ign(
        &config,
        "http://ignored.example.com",
        &[
            "tags",
            "import",
            "--file",
            bad.to_str().unwrap(),
            "--provider",
            "p5import",
            "--compact",
        ],
    );
    assert_eq!(out.status.code(), Some(2));
    let envelope = stderr_envelope(&out);
    assert_eq!(envelope["error"]["code"], "invalid_input");
    assert_eq!(envelope["profile"], Value::Null);
}

// ---- 05-06: alarms + history query goldens ----

/// Mount ONE scripted action on the ALARMS route (the precondition
/// probe included — it always probes the tags route).
async fn mount_alarms_action(server: &wiremock::MockServer, action: &str, data: Value) {
    mount_tags_probe(server).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/alarms"))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"action": action}),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"ok": true, "data": data})),
        )
        .expect(1..)
        .mount(server)
        .await;
}

/// Alarms active goldens: the human table (SHORT eventId, source,
/// state, priority, name) and the compact agent shape
/// (unit-explicit keys, all keys always — name null degrades).
#[tokio::test]
async fn tags_alarms_active_golden() {
    let server = wiremock::MockServer::start().await;
    mount_alarms_action(
        &server,
        "active",
        serde_json::json!({"results": [
            {"eventId": "3f2504e0-4f89-11d3-9a0c-0305e82c3301", "source": "prov:tagprov:/T1/HighLimit", "state": "Active, Unacknowledged", "priority": "High", "name": "HighLimit"},
            {"eventId": "9b9e9e9e-1111-2222-3333-444455556666", "source": "prov:tagprov:/T2/LowLimit", "state": "Active, Unacknowledged", "priority": "Medium", "name": null}
        ], "count": 2}),
    )
    .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());

    let out = ign(
        &config,
        &server.uri(),
        &["tags", "alarms", "active", "--source", "prov:tagprov"],
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
eventId    source                                       state                    priority name
3f2504e0   prov:tagprov:/T1/HighLimit                   Active, Unacknowledged   High     HighLimit
9b9e9e9e   prov:tagprov:/T2/LowLimit                    Active, Unacknowledged   Medium   -
"#]],
    );

    let out = ign(&config, &server.uri(), &["tags", "alarms", "active", "--compact"]);
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"project":"ign-cli","alarms":[{"event_id":"3f2504e0-4f89-11d3-9a0c-0305e82c3301","source":"prov:tagprov:/T1/HighLimit","state":"Active, Unacknowledged","priority":"High","name":"HighLimit"},{"event_id":"9b9e9e9e-1111-2222-3333-444455556666","source":"prov:tagprov:/T2/LowLimit","state":"Active, Unacknowledged","priority":"Medium","name":null}],"count":2}}"#]],
    );
}

/// THE journal-missing refusal golden: a default rig (the alarms
/// route denies history with the structured code) → exit 6,
/// `alarm_journal_missing`, hint naming the provisioning chain +
/// README section. (Also proves the denial mapping at the BINARY
/// level.)
#[tokio::test]
async fn tags_alarms_journal_missing_refusal_golden() {
    let server = wiremock::MockServer::start().await;
    mount_tags_probe(&server).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/alarms"))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"action": "history"}),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": false,
                "error": {"code": "no_alarm_journal", "message": "No alarm journal profile specified"}
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());

    let out = ign(
        &config,
        &server.uri(),
        &[
            "tags",
            "alarms",
            "history",
            "--start",
            "2026-08-25T00:00:00Z",
            "--end",
            "1787659200000",
            "--compact",
        ],
    );
    assert_eq!(out.status.code(), Some(6), "journal-less rig exits 6");
    let envelope = stderr_envelope(&out);
    assert_eq!(envelope["error"]["code"], "alarm_journal_missing");
    let hint = envelope["error"]["hint"].as_str().expect("hint present");
    assert!(
        hint.contains("journal profile") && hint.contains("README"),
        "hint names the chain + README section: {hint}"
    );
}

/// Alarm history SUCCESS golden: journal rows render as the aligned
/// columns/rows table (the journal wire shape is dataset-dependent —
/// the header IS the column list).
#[tokio::test]
async fn tags_alarms_history_golden() {
    let server = wiremock::MockServer::start().await;
    mount_alarms_action(
        &server,
        "history",
        serde_json::json!({"results": [
            {"eventId": "e-1", "source": "prov:tagprov:/T1/HighLimit", "state": "Active, Unacknowledged", "priority": "High", "name": "HighLimit", "eventData": null}
        ], "count": 1}),
    )
    .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());

    let out = ign(
        &config,
        &server.uri(),
        &[
            "tags",
            "alarms",
            "history",
            "--start",
            "1787000000000",
            "--end",
            "1787659200000",
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
eventData  eventId  name       priority  source                      state
null       e-1      HighLimit  High      prov:tagprov:/T1/HighLimit  Active, Unacknowledged
1 row(s)
"#]],
    );
}

/// THE ack golden: the 3-arg body pins on the wire (string ids +
/// note + username), and both render modes carry the honest count +
/// the unacknowledged remainder.
#[tokio::test]
async fn tags_alarms_ack_golden() {
    let server = wiremock::MockServer::start().await;
    mount_tags_probe(&server).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/alarms"))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "action": "acknowledge",
            "eventIds": ["e-1", "e-2"],
            "note": "handled",
            "username": "op"
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"unacknowledged": ["e-2"]}
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());

    let out = ign(
        &config,
        &server.uri(),
        &[
            "tags",
            "alarms",
            "ack",
            "e-1",
            "e-2",
            "--note",
            "handled",
            "--username",
            "op",
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
acknowledged 1 alarm(s); unacknowledged: e-2
"#]],
    );

    // The compact agent shape: the honest count + remainder array.
    let server = wiremock::MockServer::start().await;
    mount_alarms_action(&server, "acknowledge", serde_json::json!({"unacknowledged": []}))
        .await;
    let out = ign(
        &config,
        &server.uri(),
        &[
            "tags",
            "alarms",
            "ack",
            "e-1",
            "--username",
            "op",
            "--compact",
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"project":"ign-cli","acknowledged":1,"unacknowledged":[]}}"#]],
    );
}

/// THE history query golden: RFC3339 + epoch-ms time args parse to
/// the epoch-ms body (pinned on the wire), the dataset renders with
/// `t_stamp` visible (preserved EXACTLY), and the compact shape
/// carries {columns, rows, row_count} verbatim.
#[tokio::test]
async fn tags_history_query_golden() {
    let server = wiremock::MockServer::start().await;
    mount_tags_probe(&server).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tagHistory"))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "action": "query",
            "paths": ["[default]T1"],
            "startDateMs": 1787659200000_i64,
            "endDateMs": 1787659260000_i64,
            "returnSize": 10
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {
                    "columns": ["t_stamp", "[default]T1"],
                    "rows": [["Mon Aug 24 00:00:00 UTC 2026", 7], ["Mon Aug 24 00:01:00 UTC 2026", null]],
                    "rowCount": 2
                }
            })),
        )
        .expect(1..)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());

    // RFC3339 start (→ 1787659200000) + epoch-ms end, --return-size
    // riding the body.
    let out = ign(
        &config,
        &server.uri(),
        &[
            "tags",
            "history",
            "query",
            "[default]T1",
            "--start",
            "2026-08-25T12:00:00Z",
            "--end",
            "1787659260000",
            "--return-size",
            "10",
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
t_stamp                       [default]T1
Mon Aug 24 00:00:00 UTC 2026  7
Mon Aug 24 00:01:00 UTC 2026  null
2 row(s)
"#]],
    );

    let out = ign(
        &config,
        &server.uri(),
        &[
            "tags",
            "history",
            "query",
            "[default]T1",
            "--start",
            "2026-08-25T12:00:00Z",
            "--end",
            "1787659260000",
            "--return-size",
            "10",
            "--compact",
        ],
    );
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"project":"ign-cli","paths":["[default]T1"],"columns":["t_stamp","[default]T1"],"rows":[["Mon Aug 24 00:00:00 UTC 2026",7],["Mon Aug 24 00:01:00 UTC 2026",null]],"row_count":2}}"#]],
    );
}

/// The time-arg usage refusals (invalid_input, pre-resolution, ZERO
/// wire work) and the missing-required-username shape: an
/// unparseable --start/--end exits 2 before ANY resolution; ack
/// without --username is a clap usage error.
#[tokio::test]
async fn tags_time_and_username_usage_refusals() {
    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    // Garbage time: exit 2 invalid_input, profile null — no mocks
    // mounted, so any wire work would have failed the spawn.
    for args in [
        vec![
            "tags",
            "alarms",
            "history",
            "--start",
            "yesterday",
            "--end",
            "1787659200000",
            "--compact",
        ],
        vec![
            "tags",
            "history",
            "query",
            "[default]T1",
            "--start",
            "1787659200000",
            "--end",
            "soon",
            "--compact",
        ],
    ] {
        let out = ign(&config, "http://ignored.example.com", &args);
        assert_eq!(out.status.code(), Some(2));
        let envelope = stderr_envelope(&out);
        assert_eq!(envelope["error"]["code"], "invalid_input");
        assert_eq!(envelope["profile"], Value::Null);
    }

    // Ack without --username: clap usage error (exit 2).
    let out = ign(
        &config,
        "http://ignored.example.com",
        &["tags", "alarms", "ack", "e-1", "--compact"],
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("--username"),
        "the usage error names the required flag"
    );
}
