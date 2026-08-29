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
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/alarms",
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

/// Alarms active goldens: the human table (the FULL eventId — what
/// `tags alarms ack` accepts verbatim, source, state, priority,
/// name) and the compact agent shape (unit-explicit keys, all keys
/// always — name null degrades).
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
eventId                                source                                       state                    priority name
3f2504e0-4f89-11d3-9a0c-0305e82c3301   prov:tagprov:/T1/HighLimit                   Active, Unacknowledged   High     HighLimit
9b9e9e9e-1111-2222-3333-444455556666   prov:tagprov:/T2/LowLimit                    Active, Unacknowledged   Medium   -
"#]],
    );

    let out = ign(
        &config,
        &server.uri(),
        &["tags", "alarms", "active", "--compact"],
    );
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

/// THE traceback-surfacing golden (the black-box fix, 05-08): a
/// `route_error` denial whose envelope carries `error.traceback`
/// (the "Invalid UUID string" class the CLI used to swallow) → the
/// stderr envelope's message CONTAINS the traceback text. A denial
/// WITHOUT one stays byte-identical (the journal-missing golden
/// above is that proof — its envelope has no traceback key).
#[tokio::test]
async fn tags_route_error_traceback_surfaces_in_the_message() {
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
                "error": {
                    "code": "route_error",
                    "message": "error handling the action",
                    "traceback": "Traceback (most recent call last):\n  File \"doPost.py\", line 42, in doPost\nIllegalArgumentException: Invalid UUID string: 3f2504e0"
                }
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
            "1787000000000",
            "--end",
            "1787659200000",
            "--compact",
        ],
    );
    assert_eq!(out.status.code(), Some(6), "route_error exits 6");
    let envelope = stderr_envelope(&out);
    assert_eq!(envelope["error"]["code"], "webdev_route_error");
    let message = envelope["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("error handling the action"),
        "the route's own message rides first: {message}"
    );
    assert!(
        message.contains("\nroute traceback: Traceback (most recent call last):"),
        "the traceback is APPENDED with its marker: {message}"
    );
    assert!(
        message.contains("Invalid UUID string: 3f2504e0"),
        "the route-side exception text is visible: {message}"
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
/// note + username; full-UUID ids pass through with NO active
/// lookup), and both render modes carry the honest count + the
/// unacknowledged remainder.
#[tokio::test]
async fn tags_alarms_ack_golden() {
    let server = wiremock::MockServer::start().await;
    mount_tags_probe(&server).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/alarms"))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "action": "acknowledge",
            "eventIds": ["11111111-1111-1111-1111-111111111111", "22222222-2222-2222-2222-222222222222"],
            "note": "handled",
            "username": "op"
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"unacknowledged": ["22222222-2222-2222-2222-222222222222"]}
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
            "11111111-1111-1111-1111-111111111111",
            "22222222-2222-2222-2222-222222222222",
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
acknowledged 1 alarm(s); unacknowledged: 22222222-2222-2222-2222-222222222222
"#]],
    );

    // The compact agent shape: the honest count + remainder array.
    let server = wiremock::MockServer::start().await;
    mount_alarms_action(
        &server,
        "acknowledge",
        serde_json::json!({"unacknowledged": []}),
    )
    .await;
    let out = ign(
        &config,
        &server.uri(),
        &[
            "tags",
            "alarms",
            "ack",
            "11111111-1111-1111-1111-111111111111",
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

/// THE short-id expansion loop (the UAT Gap 2 seam, binary-level):
/// ack with the 8-char PREFIX the old table used to print → the
/// recorded acknowledge request body carries the FULL uuid
/// (request-level proof, the surgery/wiremock family convention),
/// and the run exits 0.
#[tokio::test]
async fn tags_alarms_ack_short_id_expands_on_the_wire() {
    let server = wiremock::MockServer::start().await;
    // The active lookup (the expansion source): two alarms, ONE
    // matching the prefix — mount_alarms_action mounts the probe
    // too (its contract), so no separate probe mount here.
    mount_alarms_action(
        &server,
        "active",
        serde_json::json!({"results": [
            {"eventId": "3f2504e0-4f89-11d3-9a0c-0305e82c3301", "source": "prov:tagprov:/T1/HighLimit", "state": "Active, Unacknowledged", "priority": "High", "name": "HighLimit"},
            {"eventId": "9b9e9e9e-1111-2222-3333-444455556666", "source": "prov:tagprov:/T2/LowLimit", "state": "Active, Unacknowledged", "priority": "Medium", "name": null}
        ], "count": 2}),
    )
    .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/alarms",
        ))
        .and(
            wiremock::matchers::body_partial_json(serde_json::json!({"action": "acknowledge"})), // Mount AFTER the active mock: wiremock matches the most
                                                                                                 // recently mounted first — the body discriminator makes it
                                                                                                 // exact anyway.
        )
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"unacknowledged": []}
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
        &["tags", "alarms", "ack", "3f2504e0", "--username", "op"],
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
acknowledged 1 alarm(s)
"#]],
    );
    // The wire proof: the acknowledge body carried the FULL uuid.
    let received = server.received_requests().await.expect("requests recorded");
    let ack = received
        .iter()
        .rev()
        .find(|request| String::from_utf8_lossy(&request.body).contains("acknowledge"))
        .expect("acknowledge request recorded");
    let body: Value = serde_json::from_slice(&ack.body).expect("body parses");
    assert_eq!(
        body["eventIds"],
        serde_json::json!(["3f2504e0-4f89-11d3-9a0c-0305e82c3301"]),
        "the prefix expanded to the FULL uuid before the wire call"
    );
}

/// Ack refusal goldens: an AMBIGUOUS prefix (two active matches)
/// and an UNKNOWN prefix both exit 2 invalid_input — the candidates
/// / the miss named honestly.
#[tokio::test]
async fn tags_alarms_ack_prefix_refusal_goldens() {
    let server = wiremock::MockServer::start().await;
    mount_alarms_action(
        &server,
        "active",
        serde_json::json!({"results": [
            {"eventId": "aaaaaaaa-1111-1111-1111-111111111111", "source": "prov:x:/T1", "state": "Active, Unacknowledged", "priority": "High", "name": null},
            {"eventId": "aaaaaaaa-2222-2222-2222-222222222222", "source": "prov:x:/T2", "state": "Active, Unacknowledged", "priority": "High", "name": null}
        ], "count": 2}),
    )
    .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());

    // Ambiguous: exit 2, both FULL candidates in the message.
    let out = ign(
        &config,
        &server.uri(),
        &[
            "tags",
            "alarms",
            "ack",
            "aaaaaaaa",
            "--username",
            "op",
            "--compact",
        ],
    );
    assert_eq!(out.status.code(), Some(2), "ambiguous prefix exits 2");
    let envelope = stderr_envelope(&out);
    assert_eq!(envelope["error"]["code"], "invalid_input");
    let message = envelope["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("aaaaaaaa-1111-1111-1111-111111111111")
            && message.contains("aaaaaaaa-2222-2222-2222-222222222222"),
        "the refusal names both candidates: {message}"
    );

    // Unknown: exit 2, the miss named + the full-id source hint.
    let out = ign(
        &config,
        &server.uri(),
        &[
            "tags",
            "alarms",
            "ack",
            "deadbeef",
            "--username",
            "op",
            "--compact",
        ],
    );
    assert_eq!(out.status.code(), Some(2), "unknown prefix exits 2");
    let envelope = stderr_envelope(&out);
    assert_eq!(envelope["error"]["code"], "invalid_input");
    let message = envelope["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("deadbeef") && message.contains("tags alarms active --json"),
        "the refusal names the miss + where full ids ride: {message}"
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

// ---- offline export browsing (07-04, INTR-03) ----

/// stdout minus the single trailing newline.
fn stdout_trimmed(out: &std::process::Output) -> &str {
    let stdout = std::str::from_utf8(&out.stdout).expect("utf-8 stdout");
    stdout.strip_suffix('\n').unwrap_or(stdout)
}

/// A git-module export tree fixture: provider `default` with a leaf
/// tag, an encoded filename, a nested folder, `_types_`, plus the
/// skipped set (`.tag-config.json`, dot-entry, `System/`).
fn git_module_export(root: &std::path::Path) {
    let tags = root.join("tags");
    let prov = tags.join("default");
    std::fs::create_dir_all(prov.join("Area1")).expect("dirs");
    std::fs::create_dir_all(prov.join("_types_")).expect("types");
    std::fs::write(tags.join(".tag-config.json"), b"{}").expect("config");
    std::fs::write(prov.join(".gitkeep"), b"").expect("dot entry");
    std::fs::create_dir_all(tags.join("System")).expect("system");
    std::fs::write(tags.join("System").join("managed.json"), b"{}").expect("managed");
    std::fs::write(prov.join("T1.json"), br#"{"tagType":"AtomicTag"}"#).expect("t1");
    std::fs::write(prov.join("Tag%2F1.json"), br#"{"tagType":"AtomicTag"}"#).expect("encoded");
    std::fs::write(
        prov.join("Area1").join("Deep.json"),
        br#"{"tagType":"AtomicTag"}"#,
    )
    .expect("deep");
    std::fs::write(prov.join("_types_").join("Motor.json"), br#"{"tags":[]}"#).expect("udt");
}

/// (a)+(f) THE offline proof: a git-module tree browses against a
/// DEAD gateway URL — no HTTP possible, exit 0, the existing tree
/// render (the flag short-circuits before any resolution).
#[test]
fn from_export_git_module_layout_browses_offline() {
    let export = tempfile::tempdir().expect("export dir");
    git_module_export(export.path());
    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://127.0.0.1:1"); // dead URL: any HTTP exits 4

    let out = ign(
        &config,
        "http://127.0.0.1:1",
        &[
            "tags",
            "browse",
            "--from-export",
            export.path().to_str().unwrap(),
        ],
    );
    assert!(
        out.status.success(),
        "offline: no HTTP — stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_trimmed(&out),
        snapbox::str![[r#"
browsing export [..]
default  Provider
  Area1  Folder
    Deep  AtomicTag
  T1  AtomicTag
    Tag/1  AtomicTag
  _types_  Folder
    Motor  UdtType
"#]],
    );
    // NB: `Tag/1` (the DECODED %2F name) renders one depth deeper —
    // the tree derives nesting from path slashes, and a tag named
    // with a literal slash genuinely carries one.
}

/// (e) THE profile-null JSON golden (the offline contract) over the
/// CLI's own interchange file — the flat agent shape.
#[test]
fn from_export_interchange_json_golden_profile_null() {
    let export = tempfile::tempdir().expect("export dir");
    let file = export.path().join("default.json");
    std::fs::write(
        &file,
        br#"[{"name":"","tagType":"Provider","tags":[
            {"name":"T1","tagType":"AtomicTag","dataType":"Int4"},
            {"name":"Pump","tagType":"AtomicTag"}
        ]}]"#,
    )
    .expect("interchange");
    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://127.0.0.1:1");

    let out = ign(
        &config,
        "http://127.0.0.1:1",
        &[
            "tags",
            "browse",
            "--from-export",
            file.to_str().unwrap(),
            "--compact",
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_trimmed(&out),
        snapbox::str![[r#"{"ok":true,"profile":null,"data":{"source":"export","origin":"[..]default.json","entries":[{"path":"[default]","name":"default","tag_type":"Provider","has_children":true,"data_type":null},{"path":"[default]T1","name":"T1","tag_type":"AtomicTag","has_children":false,"data_type":"Int4"},{"path":"[default]Pump","name":"Pump","tag_type":"AtomicTag","has_children":false,"data_type":null}]}}"#]],
    );
}

/// (b) The legacy single-file layout (a `<provider>.json` whole tree
/// under tags/) + (d) the --filter applies client-side.
#[test]
fn from_export_legacy_layout_and_filter() {
    let export = tempfile::tempdir().expect("export dir");
    let tags = export.path().join("tags");
    std::fs::create_dir_all(&tags).expect("tags dir");
    std::fs::write(
        tags.join("default.json"),
        br#"{"name":"default","tagType":"Provider","tags":[
            {"name":"T1","tagType":"AtomicTag"},
            {"name":"Folder1","tagType":"Folder","tags":[
                {"name":"Pump","tagType":"AtomicTag"}
            ]}
        ]}"#,
    )
    .expect("legacy");
    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://127.0.0.1:1");

    let out = ign(
        &config,
        "http://127.0.0.1:1",
        &[
            "tags",
            "browse",
            "--from-export",
            export.path().to_str().unwrap(),
            "--filter",
            "pump",
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Filtered rows only; the origin line prints the passed path.
    let stdout = stdout_trimmed(&out);
    assert!(
        stdout.starts_with("browsing export "),
        "origin header: {stdout}"
    );
    assert!(
        stdout.ends_with("    Pump  AtomicTag"),
        "the filtered row: {stdout}"
    );
    assert!(!stdout.contains("T1"), "non-matching rows drop");
    assert_eq!(stdout.lines().count(), 2, "header + the one row: {stdout}");
}

/// The positional browse path and --from-export are mutually
/// exclusive (clap usage error, exit 2).
#[test]
fn from_export_conflicts_with_the_browse_path() {
    let export = tempfile::tempdir().expect("export dir");
    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://127.0.0.1:1");
    let out = ign(
        &config,
        "http://127.0.0.1:1",
        &[
            "tags",
            "browse",
            "[default]",
            "--from-export",
            export.path().to_str().unwrap(),
        ],
    );
    assert_eq!(out.status.code(), Some(2), "clap usage error");
}

/// A nonexistent path refuses usage-class with profile null (offline
/// errors lead — zero network).
#[test]
fn from_export_missing_path_refuses() {
    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://127.0.0.1:1");
    let out = ign(
        &config,
        "http://127.0.0.1:1",
        &[
            "tags",
            "browse",
            "--from-export",
            "/definitely/not/here",
            "--compact",
        ],
    );
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    let start = stderr.find('{').unwrap_or(0);
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stderr[start..].trim_end_matches('\n'),
        snapbox::str![[r#"{"ok":false,"profile":null,"error":{"code":"invalid_input","message":"invalid input: cannot read /definitely/not/here: [..]","endpoint":null,"hint":"fix the input source — a readable file path via --file, or `-` to pipe the content on stdin"}}"#]],
    );
}

// ---- 07-06: provider-ROOT tag paths refuse honestly ----

/// Mount the tagConfig route answering the provider-root DENIAL
/// envelope (HTTP 200 — WebDev denials ride 200) for ONE action;
/// the tags-route precondition probe included.
async fn mount_tagconfig_denial(server: &wiremock::MockServer, action: &str) {
    mount_tags_probe(server).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/tagConfig",
        ))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"action": action}),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": false,
                "error": {
                    "code": "provider_root_unsupported",
                    "message": "provider-root tag paths are not supported on WebDev threads (no RpcContext) -- use a subtree path like [provider]folder",
                },
            })),
        )
        .expect(1..)
        .mount(server)
        .await;
}

/// Gap 5, the Rust mapping half: wiremock cannot execute the route's
/// Python (the route-side refusal is source-pinned in
/// ignition-core's webdev tests), so this contract pins the
/// denial_to_error seam — the route's `provider_root_unsupported`
/// body denial maps to the dedicated exit-6 slug whose fixed Display
/// names the subtree workaround. The bracket provider ROOT
/// (`[default]` alone — the route's pre-call detection shape).
#[tokio::test]
async fn tags_config_get_provider_root_refuses_honestly() {
    let server = wiremock::MockServer::start().await;
    mount_tagconfig_denial(&server, "getConfig").await;
    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());

    let out = ign(
        &config,
        &server.uri(),
        &["tags", "config", "get", "[default]", "--compact"],
    );
    assert_eq!(out.status.code(), Some(6), "target state, not route_error");
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    let body = stderr_envelope(&out);
    assert_eq!(
        body["error"]["code"],
        serde_json::Value::String("provider_root_unsupported".into())
    );
    let message = body["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("subtree like [provider]folder"),
        "the fixed Display names the subtree workaround: {message}"
    );
    let hint = body["error"]["hint"].as_str().expect("hint");
    assert!(
        hint.contains("[provider]folder"),
        "the hint names the supported form: {hint}"
    );
}

/// The export-path variant (the bare `default` form — the shape the
/// route's RpcContext translation produces on the wire): same
/// denial envelope, same dedicated mapping, and NO default file
/// lands (the refusal precedes any write).
#[tokio::test]
async fn tags_export_provider_root_refuses_honestly() {
    let server = wiremock::MockServer::start().await;
    mount_tagconfig_denial(&server, "exportTags").await;
    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());

    let cwd = tempfile::tempdir().expect("tempdir");
    let out = ign_stdin(
        &config,
        &server.uri(),
        &["tags", "export", "default", "--compact"],
        "",
        Some(cwd.path()),
    );
    assert_eq!(out.status.code(), Some(6), "target state, not route_error");
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    let body = stderr_envelope(&out);
    assert_eq!(
        body["error"]["code"],
        serde_json::Value::String("provider_root_unsupported".into())
    );
    assert!(
        body["error"]["message"]
            .as_str()
            .expect("message")
            .contains("subtree like [provider]folder"),
        "the fixed Display names the subtree workaround"
    );
    assert!(
        !cwd.path().join("default.json").exists(),
        "the refusal precedes any file write"
    );
}
