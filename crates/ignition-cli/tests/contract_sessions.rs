//! Golden-file contract tests for `ign sessions` / `ign sessions
//! terminate` / `ign connections` (02-03, HLTH-08/05/06) — wiremock
//! fixtures shaped after the openapi item schemas, harness inherited
//! from `contract_status.rs` (01-02): isolated `IGNITION_CLI_CONFIG`,
//! `stdout_for_golden`, `[..]` elides only genuinely dynamic values.
//!
//! Following the 02-02 golden policy: every number in a golden comes
//! from a pinned fixture body, so goldens are EXACT; error envelopes
//! whose `endpoint` embeds the random mock URI stay programmatic
//! (SNAPSHOTS=overwrite would bake the port into a golden).
//!
//! The crown pin: `sessions terminate` WITHOUT `--yes` exits 2 with the
//! `confirmation_required` envelope — the Phase-1 guard's first live
//! exercise — and WITH `--yes` the gateway-side DELETE fires and
//! succeeds.

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

/// stderr's JSON envelope starting at the first `{` (log-tolerant parse).
fn stderr_envelope(out: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&out.stderr);
    let start = stderr.find('{').unwrap_or(0);
    stderr[start..].to_string()
}

/// The three session-family lists, one item each (openapi shapes).
async fn mount_session_families(server: &wiremock::MockServer) {
    for (path, body) in [
        (
            "/data/api/v1/designers",
            serde_json::json!({
                "items": [
                    {
                        "id": "d-1",
                        "user": "admin",
                        "uptime": 600000,
                        "lastcomm": 1787346747022i64,
                        "timeout": 3600000,
                        "memory": {"used": 268435456i64, "max": 1073741824i64},
                        "project": "MyProject",
                        "address": "203.0.113.50:52526",
                        "timezone": "America/New_York"
                    }
                ],
                "metadata": {"total": 1, "matching": 1, "limit": -1, "offset": 0}
            }),
        ),
        (
            "/data/perspective/api/v1/sessions/",
            serde_json::json!({
                "items": [
                    {
                        "id": "psess-1",
                        "username": "admin",
                        "authorized": true,
                        "project": "MyProject",
                        "clientAddress": "192.0.2.5",
                        "lastComm": 1787346747022i64,
                        "sessionScope": "G",
                        "activePages": 2,
                        "pageIds": ["viewA", "viewB"],
                        "recentBytesSent": 1024,
                        "totalBytesSent": 4096,
                        "userAgent": "Mozilla/5.0"
                    }
                ],
                "metadata": {"total": 1, "matching": 1, "limit": -1, "offset": 0}
            }),
        ),
        (
            "/data/vision/api/v1/clients",
            serde_json::json!({
                "items": [
                    {
                        "id": "v-1",
                        "user": "operator",
                        "uptime": 120000,
                        "lastcomm": 1787346747022i64,
                        "timeout": 3600000,
                        "memory": {"used": 134217728i64, "max": 536870912i64},
                        "project": "PlantFloor",
                        "address": "192.0.2.9:443",
                        "timezone": "UTC",
                        "tagCount": 1523
                    }
                ],
                "metadata": {"total": 1, "matching": 1, "limit": -1, "offset": 0}
            }),
        ),
    ] {
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(path))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(body))
            .expect(1..)
            .mount(server)
            .await;
    }
}

/// The connection resource lists (one database item, empty OPC).
async fn mount_connections(server: &wiremock::MockServer) {
    for (path, body) in [
        (
            "/data/api/v1/resources/list/ignition/database-connection",
            serde_json::json!({
                "items": [
                    {
                        "name": "MyPostgres",
                        "enabled": true,
                        "healthchecks": {"jdbc": "FAIR"},
                        "collection": "database-connections"
                    }
                ],
                "metadata": {"total": 1, "matching": 1, "limit": -1, "offset": 0}
            }),
        ),
        (
            "/data/api/v1/resources/list/ignition/opc-connection",
            serde_json::json!({
                "items": [],
                "metadata": {"total": 0, "matching": 0, "limit": -1, "offset": 0}
            }),
        ),
    ] {
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(path))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(body))
            .expect(1..)
            .mount(server)
            .await;
    }
}

/// `ign sessions` goldens in all three modes: human sections per family,
/// pretty JSON, compact — every value fixture-pinned.
#[tokio::test]
async fn sessions_render_modes_golden() {
    let server = wiremock::MockServer::start().await;
    mount_session_families(&server).await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    // Pretty JSON: all three family keys, wire-faithful items.
    let out = ign(&config, &server.uri(), &["sessions", "--json"]);
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
    "designers": [
      {
        "id": "d-1",
        "address": "203.0.113.50:52526",
        "user": "admin",
        "project": "MyProject",
        "memory": {
          "max": 1073741824,
          "used": 268435456
        },
        "uptime": 600000,
        "lastcomm": 1787346747022,
        "timeout": 3600000,
        "timezone": "America/New_York"
      }
    ],
    "perspective": [
      {
        "id": "psess-1",
        "username": "admin",
        "authorized": true,
        "project": "MyProject",
        "clientAddress": "192.0.2.5",
        "lastComm": 1787346747022,
        "activePages": 2,
        "userAgent": "Mozilla/5.0",
        "pageIds": [
          "viewA",
          "viewB"
        ],
        "recentBytesSent": 1024,
        "sessionScope": "G",
        "totalBytesSent": 4096
      }
    ],
    "vision": [
      {
        "id": "v-1",
        "address": "192.0.2.9:443",
        "user": "operator",
        "project": "PlantFloor",
        "memory": {
          "max": 536870912,
          "used": 134217728
        },
        "uptime": 120000,
        "lastcomm": 1787346747022,
        "timeout": 3600000,
        "timezone": "UTC",
        "tagCount": 1523
      }
    ]
  }
}
"#]],
    );

    // Human: the webpage's Sessions pages as terminal sections.
    let out = ign(&config, &server.uri(), &["sessions"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
designers (1)
d-1  admin  MyProject  203.0.113.50:52526  1787346747022
perspective (1)
psess-1  admin  MyProject  192.0.2.5  1787346747022
vision (1)
v-1  operator  PlantFloor  192.0.2.9:443  1787346747022
"#]],
    );

    // Compact: one line, same shape.
    let out = ign(&config, &server.uri(), &["sessions", "--compact"]);
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"designers":[{"id":"d-1","address":"203.0.113.50:52526","user":"admin","project":"MyProject","memory":{"max":1073741824,"used":268435456},"uptime":600000,"lastcomm":1787346747022,"timeout":3600000,"timezone":"America/New_York"}],"perspective":[{"id":"psess-1","username":"admin","authorized":true,"project":"MyProject","clientAddress":"192.0.2.5","lastComm":1787346747022,"activePages":2,"userAgent":"Mozilla/5.0","pageIds":["viewA","viewB"],"recentBytesSent":1024,"sessionScope":"G","totalBytesSent":4096}],"vision":[{"id":"v-1","address":"192.0.2.9:443","user":"operator","project":"PlantFloor","memory":{"max":536870912,"used":134217728},"uptime":120000,"lastcomm":1787346747022,"timeout":3600000,"timezone":"UTC","tagCount":1523}]}}"#]],
    );
}

/// `--type perspective`: ONLY the Perspective endpoint is mounted —
/// exit 0 proves the filtered-out families were never requested — and
/// their keys stay present-but-empty (stable agent shape).
#[tokio::test]
async fn sessions_type_filter_calls_only_the_requested_family() {
    let server = wiremock::MockServer::start().await;
    // ONLY the perspective list is mounted: designers/vision would 404.
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/perspective/api/v1/sessions/",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {
                        "id": "psess-1",
                        "username": "admin",
                        "authorized": true,
                        "project": "MyProject",
                        "clientAddress": "192.0.2.5",
                        "lastComm": 1787346747022i64,
                        "activePages": 2,
                        "userAgent": "Mozilla/5.0"
                    }
                ],
                "metadata": {"total": 1, "matching": 1, "limit": -1, "offset": 0}
            })),
        )
        .expect(1..)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(
        &config,
        &server.uri(),
        &["sessions", "--type", "perspective"],
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
designers (0)
perspective (1)
psess-1  admin  MyProject  192.0.2.5  1787346747022
vision (0)
"#]],
    );

    // JSON keeps all three keys; the filtered-out ones are [].
    let out = ign(
        &config,
        &server.uri(),
        &["sessions", "--type", "perspective", "--compact"],
    );
    let body: Value = serde_json::from_slice(&out.stdout).expect("envelope parses");
    assert_eq!(
        body["data"]["designers"]
            .as_array()
            .expect("designers key present")
            .len(),
        0
    );
    assert_eq!(
        body["data"]["vision"]
            .as_array()
            .expect("vision key present")
            .len(),
        0
    );
    assert_eq!(body["data"]["perspective"][0]["id"], "psess-1");
}

/// THE crown pin: terminate WITHOUT `--yes` exits 2 with the
/// `confirmation_required` envelope — the Phase-1 guard's first live
/// exercise. Fully static content (the guard fires before any API
/// construction — no mock, no dynamic values), so a snapbox golden on
/// stderr is exact.
#[tokio::test]
async fn sessions_terminate_without_yes_exits_2_golden() {
    // No server mounted: a request would fail loudly — proving the guard
    // fires BEFORE any API construction.
    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(
        &config,
        "http://127.0.0.1:1",
        &[
            "sessions",
            "terminate",
            "--type",
            "perspective",
            "--id",
            "psess-1",
            "--compact",
        ],
    );
    assert_eq!(out.status.code(), Some(2), "usage class");
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stderr_envelope(&out),
        snapbox::str![[r#"
{"ok":false,"profile":null,"error":{"code":"confirmation_required","message":"sessions terminate is destructive; rerun with --yes to confirm","endpoint":null,"hint":"this operation is destructive; re-run with --yes or set IGNITION_YES=1"}}

"#]],
    );

    // The same refusal fires via the env escape hatch's absence even
    // with a completely broken profile URL (usage-class errors lead).
    let out = ign(
        &config,
        "http://127.0.0.1:1",
        &["sessions", "terminate", "--type", "vision", "--id", "v-1"],
    );
    assert_eq!(out.status.code(), Some(2));
    // Human mode: the message + hint on stderr.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("destructive"), "human message: {stderr}");
    assert!(stderr.contains("--yes"), "human hint names --yes: {stderr}");
}

/// Terminate WITH `--yes` (also proving IGNITION_YES=1 merges): the
/// gateway-side DELETE fires and the success line/data golden-pins.
#[tokio::test]
async fn sessions_terminate_with_yes_golden() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path(
            "/data/perspective/api/v1/sessions",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"terminated": 1})),
        )
        .expect(1..)
        .mount_as_scoped(&server)
        .await;
    // The IGNITION_YES probe terminates a VISION client — mount its route.
    wiremock::Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path("/data/vision/api/v1/client/v-9"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"message": "Session terminated"})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(
        &config,
        &server.uri(),
        &[
            "sessions",
            "terminate",
            "--type",
            "perspective",
            "--id",
            "psess-1",
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
terminated perspective session psess-1
"#]],
    );

    // JSON data shape: {kind, id}, kind kebab-case.
    let out = ign(
        &config,
        &server.uri(),
        &[
            "sessions",
            "terminate",
            "--type",
            "perspective",
            "--id",
            "psess-1",
            "--yes",
            "--compact",
        ],
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[
            r#"{"ok":true,"profile":"dev","data":{"kind":"perspective","id":"psess-1"}}"#
        ]],
    );

    // IGNITION_YES=1 confirms too (the env escape hatch; no --yes flag).
    let out = {
        let mut command = Command::cargo_bin("ign").expect("binary 'ign' not found");
        command
            .env("IGNITION_CLI_CONFIG", &config)
            .env("IGNITION_TOKEN", "mock:name-key")
            .env("IGNITION_URL", server.uri())
            .env("IGNITION_YES", "1")
            .args(["sessions", "terminate", "--type", "vision", "--id", "v-9"])
            .output()
            .expect("spawn ign")
    };
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // The Perspective DELETE carried sessionId as a query param.
    let requests = guard.received_requests().await;
    assert!(!requests.is_empty());
    let query = requests[0].url.query().expect("query present");
    assert!(
        query.contains("sessionId=psess-1"),
        "sessionId rode the DELETE query: {query}"
    );
}

/// Terminating a nonexistent id: the gateway answers 404 → exit 6,
/// `not_found` (endpoint embeds the dynamic mock URI — programmatic
/// envelope assertions, the version_gateway_contract pattern).
#[tokio::test]
async fn sessions_terminate_nonexistent_id_exits_6() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path(
            "/data/perspective/api/v1/sessions",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "message": "No valid sessions found to close."
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");
    let out = ign(
        &config,
        &server.uri(),
        &[
            "sessions",
            "terminate",
            "--type",
            "perspective",
            "--id",
            "nope",
            "--yes",
            "--compact",
        ],
    );
    assert_eq!(out.status.code(), Some(6), "target-state class");
    assert!(out.stdout.is_empty(), "errors never touch stdout");

    let body: Value = serde_json::from_str(&stderr_envelope(&out))
        .unwrap_or_else(|err| panic!("stderr envelope parses: {err}"));
    assert_eq!(body["ok"], Value::Bool(false));
    assert_eq!(body["profile"], Value::String("dev".into()));
    assert_eq!(body["error"]["code"], Value::String("not_found".into()));
    assert_eq!(
        body["error"]["endpoint"],
        Value::String(format!("{}/data/perspective/api/v1/sessions", server.uri()))
    );
}

/// `ign connections` goldens: human sections with healthchecks as
/// compact JSON on the row; compact JSON wire-faithful.
#[tokio::test]
async fn connections_render_modes_golden() {
    let server = wiremock::MockServer::start().await;
    mount_connections(&server).await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(&config, &server.uri(), &["connections"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
database (1)
MyPostgres  true  {"jdbc":"FAIR"}
opc (0)
"#]],
    );

    let out = ign(&config, &server.uri(), &["connections", "--compact"]);
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"database":[{"name":"MyPostgres","enabled":true,"healthchecks":{"jdbc":"FAIR"},"collection":"database-connections"}],"opc":[]}}"#]],
    );

    // The --type database filter keeps both keys, empties opc, and never
    // requests the OPC list (only the database mock is fresh here — the
    // shared mount used expect(1..), so a second connections call would
    // still match; the filter proof lives in the core unit tests).
    let out = ign(
        &config,
        &server.uri(),
        &["connections", "--type", "database", "--compact"],
    );
    let body: Value = serde_json::from_slice(&out.stdout).expect("envelope parses");
    assert_eq!(body["data"]["opc"].as_array().unwrap().len(), 0);
    assert_eq!(body["data"]["database"][0]["name"], "MyPostgres");
}

/// The OPC UA device resource list: healthy, unhealthy ("Connecting"),
/// no healthchecks, disabled — each with `config.settings` carrying a
/// documentation-range host and a secret-shaped field that must never
/// reach `ign` output.
async fn mount_devices(server: &wiremock::MockServer) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/resources/list/com.inductiveautomation.opcua/device",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {
                        "name": "Device-A",
                        "description": "Primary line controller",
                        "enabled": true,
                        "collection": "core",
                        "signature": "sig-a",
                        "config": {
                            "profile": {"type": "LogixDriver"},
                            "settings": {
                                "connectivity": {"hostname": "192.0.2.10"},
                                "advanced": {"password": {"type": "Embedded", "data": "secret-a"}}
                            }
                        },
                        "healthchecks": {"status": {
                            "name": "devices.device-Device-A.status",
                            "result": {"healthy": true, "time": 1700000000000_i64, "message": "Connected"}
                        }}
                    },
                    {
                        "name": "Device-B",
                        "description": null,
                        "enabled": true,
                        "config": {
                            "profile": {"type": "ModbusTcp"},
                            "settings": {"connectivity": {"hostname": "192.0.2.11"}}
                        },
                        "healthchecks": {"status": {
                            "name": "devices.device-Device-B.status",
                            "result": {"healthy": false, "time": 1700000000000_i64, "message": "Connecting"}
                        }}
                    },
                    {
                        "name": "Device-C",
                        "enabled": true,
                        "config": {
                            "profile": {"type": "TCPDriver"},
                            "settings": {"connectivity": {"hostname": "192.0.2.12"}}
                        }
                    },
                    {
                        "name": "Device-D",
                        "enabled": false,
                        "config": {
                            "profile": {"type": "com.inductiveautomation.Micro800DeviceType"},
                            "settings": {"connectivity": {"hostname": "192.0.2.13"}}
                        }
                    }
                ],
                "metadata": {"total": 4, "matching": 4, "limit": -1, "offset": 0}
            })),
        )
        .expect(1..)
        .mount(server)
        .await;
}

/// Request paths the mock recorded, in order.
async fn requested_paths(server: &wiremock::MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .expect("request recording is on by default")
        .iter()
        .map(|request| request.url.path().to_string())
        .collect()
}

/// `ign connections --type device` goldens: the NAME/DRIVER/ENABLED/
/// HEALTHY/MESSAGE table (healthy, "Connecting", missing healthchecks as
/// `unknown`, disabled), compact JSON with the gateway's healthchecks as
/// reported, and no settings/host/secret anywhere in stdout or stderr.
#[tokio::test]
async fn connections_type_device_golden_and_never_leaks_settings() {
    let server = wiremock::MockServer::start().await;
    mount_devices(&server).await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let human = ign(&config, &server.uri(), &["connections", "--type", "device"]);
    assert!(
        human.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&human.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&human),
        snapbox::str![[r#"
[profile: dev]
database (0)
opc (0)
device (4)
NAME      DRIVER                                      ENABLED  HEALTHY  MESSAGE
Device-A  LogixDriver                                 true     true     Connected
Device-B  ModbusTcp                                   true     false    Connecting
Device-C  TCPDriver                                   true     unknown  -
Device-D  com.inductiveautomation.Micro800DeviceType  false    unknown  -
"#]],
    );

    let compact = ign(
        &config,
        &server.uri(),
        &["connections", "--type", "device", "--compact"],
    );
    assert!(compact.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&compact),
        snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"database":[],"opc":[],"device":[{"name":"Device-A","description":"Primary line controller","enabled":true,"driver":"LogixDriver","healthchecks":{"status":{"name":"devices.device-Device-A.status","result":{"healthy":true,"message":"Connected","time":1700000000000}}}},{"name":"Device-B","description":null,"enabled":true,"driver":"ModbusTcp","healthchecks":{"status":{"name":"devices.device-Device-B.status","result":{"healthy":false,"message":"Connecting","time":1700000000000}}}},{"name":"Device-C","description":null,"enabled":true,"driver":"TCPDriver","healthchecks":null},{"name":"Device-D","description":null,"enabled":false,"driver":"com.inductiveautomation.Micro800DeviceType","healthchecks":null}]}}"#]],
    );

    // Pretty JSON too: no mode may surface the gateway's settings.
    let pretty = ign(
        &config,
        &server.uri(),
        &["connections", "--type", "device", "--json"],
    );
    for out in [&human, &compact, &pretty] {
        let all = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        for leaked in [
            "192.0.2.", "secret-a", "hostname", "password", "settings", "sig-a",
        ] {
            assert!(!all.contains(leaked), "{leaked} leaked into: {all}");
        }
    }

    // Only the device list was requested — neither connection family.
    let paths = requested_paths(&server).await;
    assert!(
        paths.iter().all(|path| path.contains("opcua/device")),
        "unexpected requests: {paths:?}"
    );
}

/// An empty device list: `device (0)` and a `device: []` key (asked,
/// none configured) — distinct from the key being absent.
#[tokio::test]
async fn connections_type_device_empty_list() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/resources/list/com.inductiveautomation.opcua/device",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [],
                "metadata": {"total": 0, "matching": 0, "limit": -1, "offset": 0}
            })),
        )
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let human = ign(&config, &server.uri(), &["connections", "--type", "device"]);
    assert!(human.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&human),
        snapbox::str![[r#"
[profile: dev]
database (0)
opc (0)
device (0)
"#]],
    );

    let compact = ign(
        &config,
        &server.uri(),
        &["connections", "--type", "device", "--compact"],
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&compact),
        snapbox::str![[
            r#"{"ok":true,"profile":"dev","data":{"database":[],"opc":[],"device":[]}}"#
        ]],
    );
}

/// Devices are opt-in: the default listing and the family filters never
/// request the device endpoint and carry no `device` key (the existing
/// `ign connections` shape is unchanged).
#[tokio::test]
async fn connections_default_and_family_filters_never_request_devices() {
    let server = wiremock::MockServer::start().await;
    mount_connections(&server).await;
    // The device list answers 200 but must never be asked: `expect(0)`
    // fails the test on drop if any command below requests it.
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/resources/list/com.inductiveautomation.opcua/device",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [],
                "metadata": {"total": 0, "matching": 0, "limit": -1, "offset": 0}
            })),
        )
        .expect(0)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    for args in [
        &["connections", "--compact"][..],
        &["connections", "--type", "database", "--compact"][..],
        &["connections", "--type", "opc", "--compact"][..],
    ] {
        let out = ign(&config, &server.uri(), args);
        assert!(out.status.success(), "{args:?} failed");
        let body: Value = serde_json::from_slice(&out.stdout).expect("envelope parses");
        assert!(
            body["data"].get("device").is_none(),
            "{args:?} grew a device key: {body}"
        );
    }
    let paths = requested_paths(&server).await;
    assert!(
        paths.iter().all(|path| !path.contains("opcua")),
        "device endpoint was requested: {paths:?}"
    );
}
