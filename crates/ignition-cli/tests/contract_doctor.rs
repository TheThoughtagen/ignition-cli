//! Golden-file contract tests for `ign doctor` (02-05, HLTH-10): the
//! four mock scenarios from the research's verified failure taxonomy
//! plus the JSON checks[] shape. Harness inherited from
//! `contract_logs.rs`; the rig row and the summary counts are machine
//! dependent (Docker may or may not be installed) and are `[..]`-
//! elided; the mock's random port elides inside the url row detail.
//!
//! The exit contract is pinned hardest: the doctor exits 0 whenever
//! the diagnosis COMPLETES — even on a fully broken gateway (failing
//! checks are data; agents parse checks[]).

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

/// A healthy gateway-info body (the live-captured field names).
fn healthy_gateway_info() -> serde_json::Value {
    serde_json::json!({
        "name": "GW",
        "redundancyRole": "Independent",
        "edition": "standard",
        "ignitionVersion": "8.3.6 (b2026042713)"
    })
}

/// The default security-properties singleton (the research's verified
/// default wiring: only the Administrator role level).
fn security_properties_body() -> serde_json::Value {
    serde_json::json!({
        "readPermissions": {"anyOf": ["Authenticated/Roles/Administrator"]},
        "writePermissions": {"anyOf": ["Authenticated/Roles/Administrator"]}
    })
}

/// The fixed Jetty HTML error page (shared harness shape).
fn jetty_error_html(status: u16, uri: &str) -> Vec<u8> {
    let message = match status {
        401 => "Unauthorized",
        403 => "Forbidden",
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
    .into_bytes()
}

/// Scenario (a): fully healthy gateway → url/liveness/commissioned/
/// auth/permissions all ok; write/webdev skip (no flags); rig row
/// elided (machine dependent). Exits 0.
#[tokio::test]
async fn doctor_healthy_gateway_golden() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/StatusPing"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"state": "RUNNING"})),
        )
        .expect(1..)
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/gateway-info"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(healthy_gateway_info()))
        .expect(1..)
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/resources/ignition/security-properties",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(security_properties_body()),
        )
        .expect(1..)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(&config, &server.uri(), &["doctor"]);
    assert!(
        out.status.success(),
        "diagnosis completes = exit 0: stderr {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
url           OK    TCP connect to 127.0.0.1:[..] succeeded
liveness      OK    gateway RUNNING (unauthenticated /StatusPing)
commissioned  OK    no /welcome redirect on /data routes
auth          OK    gateway-info read succeeded (HTTP 200, gateway 8.3.6 (b2026042713))
permissions   OK    readPermissions: {"anyOf":["Authenticated/Roles/Administrator"]}; writePermissions: {"anyOf":["Authenticated/Roles/Administrator"]}
write         SKIP  not requested (--check-write)
webdev        SKIP  not requested (--webdev-route NAME)
rig           [..]
8 checks: [..]
"#]],
    );
}

/// Scenario (b): 401 HTML on gateway-info → auth FAIL with the
/// name:key hint (the #1 setup failure); permissions SKIP; still
/// exit 0.
#[tokio::test]
async fn doctor_401_names_the_name_key_format_golden() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/StatusPing"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"state": "RUNNING"})),
        )
        .expect(1..)
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/gateway-info"))
        .respond_with(wiremock::ResponseTemplate::new(401).set_body_raw(
            jetty_error_html(401, "/data/api/v1/gateway-info"),
            "text/html;charset=iso-8859-1",
        ))
        .expect(1..)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(&config, &server.uri(), &["doctor"]);
    assert!(
        out.status.success(),
        "failing checks are data, not CLI errors: exit 0"
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
url           OK    TCP connect to 127.0.0.1:[..] succeeded
liveness      OK    gateway RUNNING (unauthenticated /StatusPing)
commissioned  OK    no /welcome redirect on /data routes
auth          FAIL  token not recognized (HTTP 401 on gateway-info)
  hint: the X-Ignition-API-Token header must be the FULL `name:key` string from the gateway UI (Platform→Security→API Keys); Basic auth does not work on 8.3 /data routes — create an API token
permissions   SKIP  auth read failed — the security-properties read needs a working token
write         SKIP  not requested (--check-write)
webdev        SKIP  not requested (--webdev-route NAME)
rig           [..]
8 checks: [..]
"#]],
    );
}

/// Scenario (c): 403 → the three-part hint on auth AND the
/// permissions deep-dive attempting the read (403 again) confirms the
/// wiring diagnosis; still exit 0.
#[tokio::test]
async fn doctor_403_three_part_hint_and_permissions_golden() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/StatusPing"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"state": "RUNNING"})),
        )
        .expect(1..)
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/gateway-info"))
        .respond_with(wiremock::ResponseTemplate::new(403).set_body_raw(
            jetty_error_html(403, "/data/api/v1/gateway-info"),
            "text/html;charset=iso-8859-1",
        ))
        .expect(1..)
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/resources/ignition/security-properties",
        ))
        .respond_with(wiremock::ResponseTemplate::new(403).set_body_raw(
            jetty_error_html(403, "/data/api/v1/resources/ignition/security-properties"),
            "text/html;charset=iso-8859-1",
        ))
        .expect(1..)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(&config, &server.uri(), &["doctor"]);
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
url           OK    TCP connect to 127.0.0.1:[..] succeeded
liveness      OK    gateway RUNNING (unauthenticated /StatusPing)
commissioned  OK    no /welcome redirect on /data routes
auth          FAIL  token recognized but under-permitted (HTTP 403 on gateway-info)
  hint: Ignition token setup is three parts: (1) the token holds an adequate security level, (2) the gateway's read/write permissions include that level (default: only Authenticated/Roles/Administrator), (3) 'Require secure connections' is unchecked for http gateways — the permissions row below helps with part 2
permissions   WARN  this token cannot read security-properties either (HTTP 403) — the gateway's read/write permissions likely exclude the token's security level (three-part cause 2)
write         SKIP  not requested (--check-write)
webdev        SKIP  not requested (--webdev-route NAME)
rig           [..]
8 checks: [..]
"#]],
    );
}

/// Scenario (d): uncommissioned (302→/welcome on gateway-info) →
/// commissioned FAIL with the wizard hint; auth/permissions SKIP;
/// still exit 0.
#[tokio::test]
async fn doctor_uncommissioned_golden() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/StatusPing"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"state": "RUNNING"})),
        )
        .expect(1..)
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/gateway-info"))
        .respond_with(
            wiremock::ResponseTemplate::new(302).insert_header("Location", "/welcome#/home"),
        )
        .expect(1..)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(&config, &server.uri(), &["doctor"]);
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
url           OK    TCP connect to 127.0.0.1:[..] succeeded
liveness      OK    gateway RUNNING (unauthenticated /StatusPing)
commissioned  FAIL  every /data route redirects to /welcome — gateway not commissioned
  hint: open http://<host>:<port>/welcome in a browser and complete the commissioning wizard
auth          SKIP  gateway not commissioned — auth not assessable
permissions   SKIP  auth read failed — the security-properties read needs a working token
write         SKIP  not requested (--check-write)
webdev        SKIP  not requested (--webdev-route NAME)
rig           [..]
8 checks: [..]
"#]],
    );
}

/// The JSON shape contract: checks[] keys are EXACTLY
/// {name, status, detail, hint} (hint null when absent — agents can
/// key on it unconditionally), statuses are the lowercase four, and
/// the check names run in the documented order. `--check-write` and
/// `--webdev-route` flip their rows on a healthy gateway.
#[tokio::test]
async fn doctor_json_shape_and_flags() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/StatusPing"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"state": "RUNNING"})),
        )
        .expect(1..)
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/gateway-info"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(healthy_gateway_info()))
        .expect(1..)
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/resources/ignition/security-properties",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(security_properties_body()),
        )
        .expect(1..)
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/data/api/v1/scan/projects"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("true"))
        .expect(1..)
        .mount(&server)
        .await;
    // THE 05-03 re-pin: doctor probes the route's version action via
    // POST `/system/webdev/ign-cli/cli/{route}` — 405 is the absent
    // marker (the Phase-2 404 assumption was research-Pitfall-1 WRONG).
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/stacked",
        ))
        .respond_with(wiremock::ResponseTemplate::new(405))
        .expect(1..)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(
        &config,
        &server.uri(),
        &[
            "doctor",
            "--check-write",
            "--webdev-route",
            "stacked",
            "--compact",
        ],
    );
    assert!(out.status.success());
    let body: Value = serde_json::from_slice(&out.stdout).expect("envelope parses");
    let checks = body["data"]["checks"].as_array().expect("checks[] present");
    let names: Vec<&str> = checks
        .iter()
        .map(|check| check["name"].as_str().expect("name"))
        .collect();
    assert_eq!(
        names,
        vec![
            "url",
            "liveness",
            "commissioned",
            "auth",
            "permissions",
            "write",
            "webdev",
            "rig"
        ],
        "documented order"
    );
    for check in checks {
        let mut keys = check
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec!["detail", "hint", "name", "status"],
            "exactly the four contract keys"
        );
    }
    let by_name = |name: &str| {
        checks
            .iter()
            .find(|check| check["name"] == name)
            .unwrap_or_else(|| panic!("{name} row"))
            .clone()
    };
    assert_eq!(by_name("write")["status"], Value::String("ok".into()));
    assert!(
        by_name("write")["detail"]
            .as_str()
            .expect("detail")
            .contains("write permitted"),
    );
    assert_eq!(by_name("webdev")["status"], Value::String("warn".into()));
    assert!(
        by_name("webdev")["detail"]
            .as_str()
            .expect("detail")
            .contains("absent"),
        "405 = route absent (warn)"
    );
    assert_eq!(by_name("url")["hint"], Value::Null, "hint null-able");
    // The rig row is ok-or-skip depending on the machine — both valid.
    let rig = by_name("rig");
    let rig_status = rig["status"].as_str().expect("rig status").to_string();
    assert!(
        rig_status == "ok" || rig_status == "skip",
        "rig row machine-dependent: {rig_status}"
    );
}

/// The pretty --json golden for the healthy scenario (versions and the
/// machine-dependent rows elided).
#[tokio::test]
async fn doctor_healthy_json_golden() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/StatusPing"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"state": "RUNNING"})),
        )
        .expect(1..)
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/gateway-info"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(healthy_gateway_info()))
        .expect(1..)
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/resources/ignition/security-properties",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(security_properties_body()),
        )
        .expect(1..)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(&config, &server.uri(), &["doctor", "--json"]);
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
{
  "ok": true,
  "profile": "dev",
  "data": {
    "checks": [
      {
        "name": "url",
        "status": "ok",
        "detail": "TCP connect to 127.0.0.1:[..] succeeded",
        "hint": null
      },
      {
        "name": "liveness",
        "status": "ok",
        "detail": "gateway RUNNING (unauthenticated /StatusPing)",
        "hint": null
      },
      {
        "name": "commissioned",
        "status": "ok",
        "detail": "no /welcome redirect on /data routes",
        "hint": null
      },
      {
        "name": "auth",
        "status": "ok",
        "detail": "gateway-info read succeeded (HTTP 200, gateway [..])",
        "hint": null
      },
      {
        "name": "permissions",
        "status": "ok",
        "detail": "readPermissions: {/"anyOf/":[/"Authenticated/Roles/Administrator/"]}; writePermissions: {/"anyOf/":[/"Authenticated/Roles/Administrator/"]}",
        "hint": null
      },
      {
        "name": "write",
        "status": "skip",
        "detail": "not requested (--check-write)",
        "hint": null
      },
      {
        "name": "webdev",
        "status": "skip",
        "detail": "not requested (--webdev-route NAME)",
        "hint": null
      },
      {
        "name": "rig",
        "status": "[..]",
        "detail": "[..]",
        "hint": [..]
      }
    ]
  }
}
"#]],
    );
}
