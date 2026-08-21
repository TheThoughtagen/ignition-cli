//! Golden-file contract tests for `ign status` / `ign modules` /
//! `ign metrics` (02-02, HLTH-01/02/07) — every fixture is a live-captured
//! 8.3.6 body (02-RESEARCH §Status/info, §Modules, §Metrics) mounted on
//! wiremock, with the profile URL pointing at the mock server.
//!
//! Harness inherited from `contract_profile.rs` (01-02): isolated
//! `IGNITION_CLI_CONFIG` per spawn, `stdout_for_golden` strips println's
//! single trailing newline, snapbox inline goldens, `[..]` elides
//! dynamic values — here only the mock server's URI (inside the error
//! envelope's `endpoint`) is truly dynamic; the fixture bodies pin
//! every number the goldens show. Golden-update workflow:
//! `SNAPSHOTS=overwrite cargo test -p ignition-cli`, then review the diff.

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

/// The exact live-captured gateway-info body of the research rig.
async fn mount_gateway_info(server: &wiremock::MockServer) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/gateway-info"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "name": "ign-mock",
                "redundancyRole": "Independent",
                "edition": "standard",
                "ignitionVersion": "8.3.6 (b2026042713)",
                "jvmVersion": "17.0.11",
                "license": {"mode": "Trial", "expirationDate": "2026-08-24T19:00:00Z"}
            })),
        )
        .expect(1..)
        .mount(server)
        .await;
}

/// The exact live-captured overview body (uptime ms, cpu fraction,
/// trialRemaining seconds).
async fn mount_overview(server: &wiremock::MockServer) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/overview"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "version": "8.3.6 (b2026042713)",
            "redundancy": {"role": "Independent", "activityLevel": "ACTIVE", "projectState": "RUNNING"},
            "java": {"version": "17.0.11", "vendor": "Azul Systems, Inc.", "name": "OpenJDK 64-Bit Server VM"},
            "os": {"name": "Linux", "arch": "amd64", "version": "5.15.0"},
            "uptime": 338137,
            "memory": [338137088i64, 1073741824i64],
            "cpu": 0.0031,
            "disk": {"total": 62661259264i64, "used": 12272824320i64},
            "license": {"state": "trial", "trialRemaining": 7017}
        })))
        .expect(1..)
        .mount(server)
        .await;
}

/// The unauthenticated readiness probe.
async fn mount_status_ping(server: &wiremock::MockServer) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/StatusPing"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"state": "RUNNING"})),
        )
        .expect(1..)
        .mount(server)
        .await;
}

/// All three endpoints the status action touches.
async fn mount_status_fixtures(server: &wiremock::MockServer) {
    mount_gateway_info(server).await;
    mount_overview(server).await;
    mount_status_ping(server).await;
}

/// The live-captured healthy-modules page (two rows).
async fn mount_modules_healthy(server: &wiremock::MockServer) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/modules/healthy"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {
                        "id": "com.inductiveautomation.perspective",
                        "name": "Perspective",
                        "version": "8.3.6",
                        "state": "ACTIVE",
                        "licenseState": "ACTIVATED",
                        "vendorName": "Inductive Automation",
                        "startupTime": "2026-08-21 22:03:29"
                    },
                    {
                        "id": "com.inductiveautomation.vision",
                        "name": "Vision",
                        "version": "8.3.6",
                        "state": "ACTIVE",
                        "licenseState": "ACTIVATED",
                        "vendorName": "Inductive Automation",
                        "startupTime": "2026-08-21 22:03:31"
                    }
                ],
                "metadata": {"total": 2, "matching": 2, "limit": -1, "offset": 0}
            })),
        )
        .expect(1..)
        .mount(server)
        .await;
}

/// The live-captured metrics trio.
async fn mount_metrics(server: &wiremock::MockServer) {
    for (path, body) in [
        (
            "/data/api/v1/systemPerformance/currentGauges",
            serde_json::json!({"cpu": 4.88, "heapMemory": 240000000i64, "maxMemory": 1073741824i64}),
        ),
        (
            "/data/api/v1/systemPerformance/threads",
            serde_json::json!({"running": 32, "waiting": 39, "timedWaiting": 51, "blocked": 0}),
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

/// Historic charts (nested wire shape), one datapoint per series.
async fn mount_charts(server: &wiremock::MockServer) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/systemPerformance/charts",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "cpuChartDatapoints": [
                    {"histId": 1, "timestamp": 1787346747022i64, "value": 4.88},
                    {"histId": 1, "timestamp": 1787347347022i64, "value": 4.90}
                ],
                "memoryChartDatapoints": {
                    "heapMemoryDatapoints": [
                        {"histId": 2, "timestamp": 1787346747022i64, "value": 240000000.0},
                        {"histId": 2, "timestamp": 1787347347022i64, "value": 240640000.0}
                    ],
                    "nonHeapMemoryDatapoints": [
                        {"histId": 3, "timestamp": 1787346747022i64, "value": 52000000.0}
                    ]
                }
            })),
        )
        .expect(1..)
        .mount(server)
        .await;
}

/// `ign status` goldens in ALL THREE render modes: the human banner
/// (identity / state / platform / uptime / cpu+memory+disk / license
/// with trial countdown), pretty JSON, and the one-line compact form —
/// all values pinned by the fixture bodies.
#[tokio::test]
async fn status_render_modes_golden() {
    let server = wiremock::MockServer::start().await;
    mount_status_fixtures(&server).await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    // Pretty JSON.
    let out = ign(&config, &server.uri(), &["status", "--json"]);
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
    "gateway": {
      "name": "ign-mock",
      "ignition_version": "8.3.6 (b2026042713)",
      "edition": "standard",
      "license": {
        "mode": "Trial",
        "expirationDate": "2026-08-24T19:00:00Z"
      }
    },
    "state": "RUNNING",
    "overview": {
      "java": {
        "version": "17.0.11",
        "vendor": "Azul Systems, Inc.",
        "name": "OpenJDK 64-Bit Server VM"
      },
      "os": {
        "name": "Linux",
        "arch": "amd64",
        "version": "5.15.0"
      },
      "uptime_ms": 338137,
      "memory": [
        338137088,
        1073741824
      ],
      "cpu_fraction": 0.0031,
      "disk": {
        "total": 62661259264,
        "used": 12272824320
      },
      "license": {
        "state": "trial",
        "trial_remaining_s": 7017
      }
    }
  }
}
"#]],
    );

    // Human: the webpage-status-page replacement.
    let out = ign(&config, &server.uri(), &["status"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = stdout_for_golden(&out);
    assert_eq!(
        stdout.lines().next(),
        Some("[profile: dev]"),
        "human output must lead with the active-profile header: {stdout}",
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout,
        snapbox::str![[r#"
[profile: dev]
ign-mock  8.3.6 (b2026042713)  standard
state: RUNNING
platform: Java 17.0.11 (Azul Systems, Inc.) on Linux (amd64)
uptime: 5m 38s
cpu 0.3%  memory 322.5MB/1GB  disk 11.4GB/58.4GB
license: trial, 1h 56m remaining
"#]],
    );

    // Compact: one line, same field set.
    let out = ign(&config, &server.uri(), &["status", "--compact"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"gateway":{"name":"ign-mock","ignition_version":"8.3.6 (b2026042713)","edition":"standard","license":{"mode":"Trial","expirationDate":"2026-08-24T19:00:00Z"}},"state":"RUNNING","overview":{"java":{"version":"17.0.11","vendor":"Azul Systems, Inc.","name":"OpenJDK 64-Bit Server VM"},"os":{"name":"Linux","arch":"amd64","version":"5.15.0"},"uptime_ms":338137,"memory":[338137088,1073741824],"cpu_fraction":0.0031,"disk":{"total":62661259264,"used":12272824320},"license":{"state":"trial","trial_remaining_s":7017}}}}"#]],
    );
}

/// The documented data keys, exactly — agents depend on this shape.
#[tokio::test]
async fn status_json_data_keys_are_the_documented_set() {
    let server = wiremock::MockServer::start().await;
    mount_status_fixtures(&server).await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");
    let out = ign(&config, &server.uri(), &["status", "--json"]);
    assert!(out.status.success());

    let body: Value = serde_json::from_slice(&out.stdout).expect("envelope parses");
    let data = &body["data"];
    let mut top: Vec<&str> = data
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    top.sort_unstable();
    assert_eq!(top, ["gateway", "overview", "state"]);

    let mut gateway: Vec<&str> = data["gateway"]
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    gateway.sort_unstable();
    assert_eq!(gateway, ["edition", "ignition_version", "license", "name"]);

    let mut overview: Vec<&str> = data["overview"]
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    overview.sort_unstable();
    assert_eq!(
        overview,
        [
            "cpu_fraction",
            "disk",
            "java",
            "license",
            "memory",
            "os",
            "uptime_ms"
        ]
    );
}

/// `ign modules` human golden: one row per module, plus the `--quarantined`
/// empty-list variant (quarantined is usually empty on a healthy gateway).
#[tokio::test]
async fn modules_human_and_quarantined_golden() {
    let server = wiremock::MockServer::start().await;
    mount_modules_healthy(&server).await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/modules/quarantined"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [],
                "metadata": {"total": 0, "matching": 0, "limit": -1, "offset": 0}
            })),
        )
        .expect(1..)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(&config, &server.uri(), &["modules"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
com.inductiveautomation.perspective  Perspective  8.3.6  ACTIVE  ACTIVATED
com.inductiveautomation.vision  Vision  8.3.6  ACTIVE  ACTIVATED
"#]],
    );

    let out = ign(&config, &server.uri(), &["modules", "--quarantined"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
(no quarantined modules)
"#]],
    );

    // The quarantined flag rides the JSON data too.
    let out = ign(
        &config,
        &server.uri(),
        &["modules", "--quarantined", "--compact"],
    );
    let body: Value = serde_json::from_slice(&out.stdout).expect("envelope parses");
    assert_eq!(body["data"]["quarantined"], Value::Bool(true));
    assert_eq!(body["data"]["items"].as_array().expect("items").len(), 0);
}

/// `ign metrics` human golden: gauges + threads; `--history` appends a
/// first/last summary per series.
#[tokio::test]
async fn metrics_human_with_history_golden() {
    let server = wiremock::MockServer::start().await;
    mount_metrics(&server).await;
    mount_charts(&server).await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(&config, &server.uri(), &["metrics"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
cpu 4.9%  heap 228.9MB/1GB
threads: 32 running, 39 waiting, 51 timed-waiting, 0 blocked
"#]],
    );

    let out = ign(&config, &server.uri(), &["metrics", "--history"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
cpu 4.9%  heap 228.9MB/1GB
threads: 32 running, 39 waiting, 51 timed-waiting, 0 blocked
history cpu: first 4.9% @ 1787346747022, last 4.9% @ 1787347347022
history heap: first 228.9MB @ 1787346747022, last 229.5MB @ 1787347347022
history non-heap: first 49.6MB @ 1787346747022, last 49.6MB @ 1787346747022
"#]],
    );

    // Without --history the JSON data carries exactly current + threads.
    let out = ign(&config, &server.uri(), &["metrics", "--compact"]);
    let body: Value = serde_json::from_slice(&out.stdout).expect("envelope parses");
    let mut keys: Vec<&str> = body["data"]
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["current", "threads"]);
    // With --history the charts ride along (flat, gateway-native series).
    let out = ign(
        &config,
        &server.uri(),
        &["metrics", "--history", "--compact"],
    );
    let body: Value = serde_json::from_slice(&out.stdout).expect("envelope parses");
    assert!(
        body["data"]["history"]["heapMemoryDatapoints"].is_array(),
        "history carries the gateway-native series names"
    );
}

/// Error golden: HTML 401 on overview (Jetty page) → exit 5, the
/// auth_rejected envelope on stderr. `[..]` elides the dynamic mock URI
/// inside `endpoint` (and the long hint).
#[tokio::test]
async fn status_html_401_error_golden() {
    let server = wiremock::MockServer::start().await;
    mount_gateway_info(&server).await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/overview"))
        .respond_with(wiremock::ResponseTemplate::new(401).set_body_raw(
            jetty_error_html(401, "/data/api/v1/overview"),
            "text/html;charset=iso-8859-1",
        ))
        .expect(1)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");
    let out = ign(&config, &server.uri(), &["status", "--compact"]);
    assert_eq!(out.status.code(), Some(5), "auth class");
    assert!(out.stdout.is_empty(), "errors never touch stdout");

    // Programmatic envelope assertions (the version_gateway_contract
    // pattern) rather than a snapbox golden: the endpoint carries the
    // DYNAMIC mock URI, which SNAPSHOTS=overwrite would bake into any
    // golden. The envelope shape itself is already golden-pinned by the
    // 01-02/02-01 error tests; this pins the status-command wiring.
    let body: Value = serde_json::from_str(&stderr_envelope(&out))
        .unwrap_or_else(|err| panic!("stderr envelope parses: {err}"));
    assert_eq!(body["ok"], Value::Bool(false));
    assert_eq!(body["profile"], Value::String("dev".into()));
    assert_eq!(body["error"]["code"], Value::String("auth_rejected".into()));
    assert_eq!(
        body["error"]["message"],
        Value::String("gateway rejected credentials (HTTP 401)".into())
    );
    assert_eq!(
        body["error"]["endpoint"],
        Value::String(format!("{}/data/api/v1/overview", server.uri())),
        "endpoint names the failing sub-call (CORE-05)"
    );
    let hint = body["error"]["hint"].as_str().expect("hint (CORE-05)");
    assert!(
        hint.contains("name:key"),
        "401 hint names the token format: {hint}"
    );
}

/// The inspection commands are authed reads: no resolvable secret →
/// `SecretUnavailable` (exit 3) BEFORE any request fires — the correct
/// taxonomy for a command that cannot work unauthenticated (contrast
/// `version`, which degrades to header-less).
#[tokio::test]
async fn status_without_secret_exits_3() {
    let server = wiremock::MockServer::start().await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");
    let out = {
        let mut command = Command::cargo_bin("ign").expect("binary 'ign' not found");
        command
            .env("IGNITION_CLI_CONFIG", &config)
            .env("IGNITION_URL", server.uri())
            .env_remove("IGNITION_TOKEN")
            .args(["status", "--json"])
            .output()
            .expect("spawn ign")
    };
    // (Command::output inherits this process's env; IGNITION_TOKEN is
    // deliberately NOT set in this test process, and env_remove guards
    // a developer shell that exports it.)
    assert_eq!(out.status.code(), Some(3), "config error class");
    assert!(out.stdout.is_empty(), "errors never touch stdout");

    let body: Value = serde_json::from_str(&stderr_envelope(&out)).expect("stderr envelope parses");
    assert_eq!(
        body["error"]["code"],
        Value::String("secret_unavailable".into())
    );
    assert_eq!(body["profile"], Value::String("dev".into()));
}

/// The fixed Jetty error-page template (byte-shaped after the live
/// capture; the classifier's sniffer keys on `<title>Error NNN</title>`).
fn jetty_error_html(status: u16, uri: &str) -> String {
    let message = match status {
        401 => "Unauthorized",
        _ => "Error",
    };
    format!(
        concat!(
            r#"<html><head><meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>"#,
            r#"<title>Error {status}</title></head><body><h2>HTTP ERROR {status} {message}</h2><table>"#,
            r#"<tr><th>URI:</th><td>{uri}</td></tr><tr><th>STATUS:</th><td>{status}</td></tr>"#,
            r#"<tr><th>MESSAGE:</th><td>{message}</td></tr></table></body></html>"#
        ),
        status = status,
        message = message,
        uri = uri,
    )
}
