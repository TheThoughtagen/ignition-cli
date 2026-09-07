//! Binary-level contract tests for the curated morning-check reads
//! (09-04, EXT-02): `ign license status`, `ign redundancy status`,
//! `ign gan status` — pinned through the real binary over a wiremock
//! gateway serving the 09-LIVE-CAPTURES bodies.
//!
//! **THIS FILE IS ALSO 09-05's HOME** — the diagnostics-bundle
//! contract tests land in this same module (the plan-mandated shared
//! home; keep the doc current as the family grows).
//!
//! Pinned here:
//! 1. `license_status_envelope` — the 8.3.x captured `/licenses` +
//!    `/trial` shapes ride the envelope: `data.license` carries the
//!    typed fields, `data.trial` carries `trialSecondsLeft`, and a
//!    mounted UNKNOWN extra key rides into `data` (the flatten-
//!    passthrough proof at the binary level).
//! 2. `redundancy_status_envelope` — the captured flat body parses;
//!    uptime (ms since gateway start) and the `-1` never-synced
//!    sentinel ride verbatim.
//! 3. `gan_status_envelope` — the zero-connection capture (a non-GAN
//!    gateway's canonical shape) answers with all five fields.
//! 4. `reads_keep_existing_error_classes` — 404 → exit 6
//!    `not_found`; 401 → exit 5 `auth_rejected` (these reads ride
//!    the CURATED pipeline — the 09-01 non-leak guarantee, at the
//!    binary level).
//!
//! Isolation is the contract_api pattern: `IGNITION_CLI_CONFIG` →
//! tempfile whose one profile points at the mock server;
//! `IGNITION_TOKEN` set (the reads resolve through `Session::resolve`
//! — a credential must exist; the mock never validates it).
//! wiremock + assert_cmd are existing dev-deps — NO new deps.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::Value;

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

/// Spawn `ign <family> status --json` with an isolated config; ambient
/// `IGNITION_*` knobs stripped (the stdout-purity-harness rule).
fn ign_status(config: &Path, family: &str) -> std::process::Output {
    let mut command = Command::cargo_bin("ign").expect("binary 'ign' not found");
    command.env("IGNITION_CLI_CONFIG", config);
    command.env("IGNITION_TOKEN", "ign-contract-test:0123456789abcdef");
    for knob in [
        "IGNITION_URL",
        "IGNITION_PROFILE",
        "IGNITION_JSON",
        "IGNITION_YES",
    ] {
        command.env_remove(knob);
    }
    command
        .args([family, "status", "--json"])
        .output()
        .expect("spawn ign")
}

/// The error envelope starts at the first `{` on stderr (tracing
/// shares stderr by design — see contract_api.rs).
fn stderr_envelope(out: &std::process::Output) -> Value {
    let stderr = String::from_utf8_lossy(&out.stderr);
    let start = stderr.find('{').unwrap_or(0);
    serde_json::from_str(&stderr[start..])
        .unwrap_or_else(|err| panic!("stderr envelope parses (full stderr {stderr:?}): {err}"))
}

/// Mount a 200 JSON body on `GET /data/api/v1/<file>`.
async fn mount_get(server: &wiremock::MockServer, file: &str, body: String) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(format!("/data/api/v1/{file}")))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(body))
        .expect(1)
        .mount(server)
        .await;
}

/// THE 8.3.6 captured `/licenses` body (09-LIVE-CAPTURES §1) plus ONE
/// fake future point-release key (the passthrough proof input).
fn licenses_body_with_unknown_key() -> String {
    r#"{
  "cloud": [], "certificate": [], "hardware": [], "embedded": [], "leased": [],
  "effective": { "lastUpdated": 1788748551614 },
  "leasedUnactivateTimeoutMS": 7500,
  "busy": false,
  "zzBrandNewPointReleaseKey": {"future": true}
}"#
    .to_string()
}

/// THE 8.3.6 captured `/trial` body (09-LIVE-CAPTURES §2).
fn trial_body() -> String {
    r#"{
  "licenseMode": "Trial", "trialState": "AllInDemo", "trialSecondsLeft": 6823,
  "expired": false, "emergency": false, "emergencySecondsLeft": 0,
  "development": false, "developmentSecondsLeft": 0
}"#
    .to_string()
}

/// The license envelope contract: `ok:true`, `data.license` carries the
/// typed morning-check fields from the captured shape, `data.trial`
/// carries the trial countdown, the LOCKED envelope keeps exactly
/// {ok, profile, data}, and the mounted UNKNOWN extra key rides into
/// `data.license` (flatten passthrough — the version-tolerance
/// directive at the binary level).
#[tokio::test]
async fn license_status_envelope() {
    let server = wiremock::MockServer::start().await;
    mount_get(&server, "licenses", licenses_body_with_unknown_key()).await;
    mount_get(&server, "trial", trial_body()).await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let out = ign_status(&config, "license");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let body: Value = serde_json::from_str(&stdout).expect("success envelope parses");
    assert_eq!(body["ok"], Value::Bool(true));
    assert_eq!(body["profile"], Value::String("dev".into()));

    // Typed morning-check fields from the capture (renames round-trip:
    // serialize uses the gateway-native camelCase keys).
    assert_eq!(body["data"]["license"]["busy"], Value::Bool(false));
    assert_eq!(
        body["data"]["license"]["effective"]["lastUpdated"],
        Value::Number(1_788_748_551_614i64.into()),
        "epoch-ms stamp rides the gateway-native key"
    );
    assert_eq!(
        body["data"]["license"]["leasedUnactivateTimeoutMS"],
        Value::Number(7500.into())
    );
    assert!(
        body["data"]["license"]["hardware"].is_array(),
        "the typed hardware skeleton serializes"
    );

    // The trial companion: mode + countdown (the mode lives HERE —
    // the licenses payload carries no mode key on the captures).
    assert_eq!(
        body["data"]["trial"]["licenseMode"],
        Value::String("Trial".into())
    );
    assert_eq!(
        body["data"]["trial"]["trialSecondsLeft"],
        Value::Number(6823.into())
    );

    // The passthrough proof: the mounted unknown key rode into data.
    assert_eq!(
        body["data"]["license"]["zzBrandNewPointReleaseKey"],
        serde_json::json!({"future": true}),
        "the unknown key rides the flatten passthrough into the envelope"
    );

    // The LOCKED envelope never grows top-level fields.
    let mut keys: Vec<&str> = body
        .as_object()
        .expect("envelope is an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["data", "ok", "profile"]);
}

/// The redundancy envelope contract: the captured flat body parses and
/// rides `data.status` — role, uptime (ms since gateway start), and
/// the `-1` never-synced sentinel verbatim.
#[tokio::test]
async fn redundancy_status_envelope() {
    let server = wiremock::MockServer::start().await;
    // THE 8.3.6 captured body (09-LIVE-CAPTURES §3) + one unknown key.
    mount_get(
        &server,
        "redundancy",
        r#"{"role":"Independent","projectState":"Unknown","activityLevel":"Active","localId":"192.168.215.2","peerConnected":false,"hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":460959,"lastSyncTimestamp":-1,"zzFutureKey":42}"#
            .to_string(),
    )
    .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let out = ign_status(&config, "redundancy");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let body: Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("envelope parses");
    assert_eq!(body["ok"], Value::Bool(true));
    assert_eq!(body["data"]["status"]["role"], "Independent");
    assert_eq!(
        body["data"]["status"]["uptime"],
        Value::Number(460_959.into()),
        "ms since gateway start (wall-clock-proven capture)"
    );
    assert_eq!(
        body["data"]["status"]["lastSyncTimestamp"],
        Value::Number((-1).into()),
        "the -1 never-synced sentinel rides verbatim"
    );
    assert_eq!(
        body["data"]["status"]["zzFutureKey"],
        Value::Number(42.into()),
        "unknown keys ride the passthrough"
    );
}

/// The gan envelope contract: the zero-connection capture (identical
/// on both rigs — a non-GAN gateway's canonical shape) answers with
/// all five fields.
#[tokio::test]
async fn gan_status_envelope() {
    let server = wiremock::MockServer::start().await;
    // THE captured body (09-LIVE-CAPTURES §4) — counts int, rates float.
    mount_get(
        &server,
        "overview/gan",
        r#"{"totalConnections": 0, "runningConnections": 0, "outgoingByteRate": 0.0, "incomingByteRate": 0.0, "remoteGateways": 0}"#
            .to_string(),
    )
    .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let out = ign_status(&config, "gan");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let body: Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("envelope parses");
    assert_eq!(body["ok"], Value::Bool(true));
    assert_eq!(
        body["data"]["status"]["totalConnections"],
        Value::Number(0.into())
    );
    assert_eq!(
        body["data"]["status"]["runningConnections"],
        Value::Number(0.into())
    );
    assert_eq!(
        body["data"]["status"]["outgoingByteRate"],
        serde_json::json!(0.0)
    );
    assert_eq!(
        body["data"]["status"]["incomingByteRate"],
        serde_json::json!(0.0)
    );
    assert_eq!(
        body["data"]["status"]["remoteGateways"],
        Value::Number(0.into())
    );
}

/// The reads ride the CURATED pipeline (the 09-01 non-leak guarantee,
/// binary level): 404 → exit 6 `not_found`; 401 → exit 5
/// `auth_rejected` — for EVERY family, with stdout empty on each.
#[tokio::test]
async fn reads_keep_existing_error_classes() {
    for family in ["license", "redundancy", "gan"] {
        for (status, exit, slug) in [(404u16, 6i32, "not_found"), (401, 5, "auth_rejected")] {
            let server = wiremock::MockServer::start().await;
            wiremock::Mock::given(wiremock::matchers::method("GET"))
                .respond_with(
                    wiremock::ResponseTemplate::new(status)
                        .set_body_string("<html>HTTP ERROR</html>"),
                )
                .mount(&server)
                .await;

            let (_dir, config) = isolated_config();
            write_profile_config(&config, &server.uri());
            let out = ign_status(&config, family);
            assert_eq!(
                out.status.code(),
                Some(exit),
                "{family} status {status} must exit {exit} ({slug}); stderr: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(out.stdout.is_empty(), "errors never touch stdout");

            let envelope = stderr_envelope(&out);
            assert_eq!(
                envelope["error"]["code"],
                Value::String(slug.to_string()),
                "{family} status {status}"
            );
        }
    }
}
