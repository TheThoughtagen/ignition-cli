//! Binary-level contract tests for the curated morning-check reads
//! (09-04, EXT-02): `ign license status`, `ign redundancy status`,
//! `ign gan status` — pinned through the real binary over a wiremock
//! gateway serving the 09-LIVE-CAPTURES bodies.
//!
//! **THIS FILE IS ALSO 09-05's HOME** — the diagnostics-bundle
//! contract tests land in this same module (the plan-mandated shared
//! home). The bundle family (09-05):
//! 5. `bundle_generate_status_envelope` — the POST generate's 200
//!    body IS the status wire (`data.state`); the status GET rides
//!    `{state, fileSize}` envelope data.
//! 6. `bundle_wait_flips_to_terminal` — Generating×2 → Valid: exit 0
//!    on the third probe (≥3 status hits on the mock).
//! 7. `bundle_wait_deadline_is_network` — an always-generating
//!    gateway expires the deadline → exit 4 `network_error` (the
//!    poll convention — NO new slug) naming the subject; the last
//!    observation rides the message and "unreachable" is ABSENT (the
//!    gateway ANSWERED — 09-07).
//! 8. `bundle_wait_invalid_exits_immediately` (09-07) — the gateway
//!    answering `Invalid` (the captured TERMINAL steady state, UAT
//!    Gap 3) ends the wait IMMEDIATELY: exit 6, slug
//!    `bundle_not_available`, the observed state + the generate
//!    command named, ONE status hit.
//! 9. `bundle_download_bytes_and_default_name` — bytes land
//!    verbatim (ZIP magic asserted), the default timestamped name
//!    applies when no `Content-Disposition` rides, and a disposition
//!    name wins when present.
//! 10. `bundle_download_timeout_override_present` — the 300 s
//!    per-request override is pinned DETERMINISTICALLY: the
//!    constant is asserted cross-crate where it is born
//!    (`client::diagnostics::BUNDLE_DOWNLOAD_TIMEOUT`), and the
//!    download implementation rides `download_to_file`'s
//!    `RequestBuilder::timeout` (the ONE streaming site — the
//!    backup/export precedent; a sleep-based wiremock is explicitly
//!    not required and would be flaky).
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
use std::sync::{Arc, Mutex};

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

/// Spawn `ign <args...> --json` with an isolated config; ambient
/// `IGNITION_*` knobs stripped (the stdout-purity-harness rule).
fn ign_cmd(config: &Path, args: &[&str]) -> std::process::Output {
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
    let mut all = args.to_vec();
    all.push("--json");
    command.args(&all).output().expect("spawn ign")
}

/// Spawn `ign <family> status --json` (the 09-04 morning-check shape).
fn ign_status(config: &Path, family: &str) -> std::process::Output {
    ign_cmd(config, &[family, "status"])
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

/// A scripted bundle-status responder: serves `states` in order, the
/// LAST entry repeating forever (the contract_restart_wait fixture
/// shape). Counts hits for the poll-count assertion.
#[derive(Clone)]
struct BundleStatusScript {
    hits: Arc<Mutex<usize>>,
    states: Vec<String>,
}

impl BundleStatusScript {
    fn new(states: &[&str]) -> Self {
        Self {
            hits: Arc::new(Mutex::new(0)),
            states: states.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn hits(&self) -> usize {
        *self.hits.lock().unwrap()
    }
}

impl wiremock::Respond for BundleStatusScript {
    fn respond(&self, _request: &wiremock::Request) -> wiremock::ResponseTemplate {
        let mut hits = self.hits.lock().unwrap();
        *hits += 1;
        let state = self
            .states
            .get(*hits - 1)
            .unwrap_or_else(|| self.states.last().expect("at least one state"));
        let mut body = serde_json::json!({ "state": state });
        if state == "Valid" {
            // Capture truth: fileSize appears only on the terminal
            // state (61053 = the 8.3.6 rig A download Content-Length).
            body["fileSize"] = serde_json::json!(61053);
        }
        wiremock::ResponseTemplate::new(200).set_body_json(body)
    }
}

/// The bundle family envelope contract (09-05): the POST generate's
/// 200 body IS the status wire (`data.state` — captured vocabulary,
/// no wrapper re-keying), and the status GET rides
/// `{state, fileSize}` envelope data.
#[tokio::test]
async fn bundle_generate_status_envelope() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/data/api/v1/diagnostics/bundle/generate",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(r#"{"state":"Valid"}"#))
        .expect(1)
        .mount(&server)
        .await;
    mount_get(
        &server,
        "diagnostics/bundle/status",
        r#"{"state":"Valid","fileSize":61053}"#.to_string(),
    )
    .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());

    let out = ign_cmd(&config, &["diagnostics", "bundle", "generate"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("envelope parses");
    assert_eq!(body["ok"], Value::Bool(true));
    assert_eq!(body["profile"], Value::String("dev".into()));
    assert_eq!(
        body["data"]["state"],
        Value::String("Valid".into()),
        "data IS the wire — data.state carries the captured vocabulary"
    );

    let out = ign_cmd(&config, &["diagnostics", "bundle", "status"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("envelope parses");
    assert_eq!(body["ok"], Value::Bool(true));
    assert_eq!(body["data"]["state"], Value::String("Valid".into()));
    assert_eq!(
        body["data"]["fileSize"],
        Value::Number(61_053.into()),
        "fileSize (bytes) rides the gateway-native key"
    );
}

/// The wait flips to terminal: Generating on the first two polls,
/// Valid on the third → exit 0 with data.state = Valid and ≥3 status
/// hits on the mock (the live capture's ~2–6 s generation pace,
/// script-compressed).
#[tokio::test]
async fn bundle_wait_flips_to_terminal() {
    let server = wiremock::MockServer::start().await;
    let script = BundleStatusScript::new(&["Generating", "Generating", "Valid"]);
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/diagnostics/bundle/status",
        ))
        .respond_with(script.clone())
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let out = ign_cmd(
        &config,
        &[
            "diagnostics",
            "bundle",
            "wait",
            "--interval",
            "1",
            "--timeout",
            "30",
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("envelope parses");
    assert_eq!(body["ok"], Value::Bool(true));
    assert_eq!(body["data"]["state"], Value::String("Valid".into()));
    assert_eq!(body["data"]["fileSize"], Value::Number(61_053.into()));
    assert!(
        script.hits() >= 3,
        "at least the three scripted polls ran: {}",
        script.hits()
    );
}

/// The deadline convention (09-07 edition): an always-generating
/// gateway expires the wait → exit 4 with the `network_error` slug
/// (the poll engine's Network{source:None} — NOT a new slug) and the
/// envelope message naming the subject; the last observation (the
/// gateway's own "Generating" answer) rides the message, and
/// "unreachable" is ABSENT — the gateway ANSWERED, the deadline
/// message must not mislabel it as a network failure.
#[tokio::test]
async fn bundle_wait_deadline_is_network() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/diagnostics/bundle/status",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_string(r#"{"state":"Generating"}"#),
        )
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let out = ign_cmd(
        &config,
        &[
            "diagnostics",
            "bundle",
            "wait",
            "--interval",
            "1",
            "--timeout",
            "2",
        ],
    );
    assert_eq!(out.status.code(), Some(4), "deadline = exit 4");
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    let envelope = stderr_envelope(&out);
    assert_eq!(
        envelope["error"]["code"],
        Value::String("network_error".into()),
        "the deadline convention — NO new slug"
    );
    let message = envelope["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("diagnostics bundle generation"),
        "the subject names the wait: {message}"
    );
    assert!(
        message.contains("Generating"),
        "the last observation rides the message verbatim: {message}"
    );
    assert!(
        !message.contains("unreachable"),
        "the gateway answered — never claim unreachability (09-07): {message}"
    );
}

/// UAT Gap 3 (09-07): the gateway answering `Invalid` — the captured
/// TERMINAL steady state meaning "no current bundle" — ends the wait
/// IMMEDIATELY: exit 6, envelope slug `bundle_not_available`, the
/// observed state and the generate command named, exactly ONE status
/// hit (no further polls, no deadline wait — polling cannot change
/// this state).
#[tokio::test]
async fn bundle_wait_invalid_exits_immediately() {
    let server = wiremock::MockServer::start().await;
    let script = BundleStatusScript::new(&["Invalid"]);
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/diagnostics/bundle/status",
        ))
        .respond_with(script.clone())
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let out = ign_cmd(
        &config,
        &[
            "diagnostics",
            "bundle",
            "wait",
            "--interval",
            "1",
            "--timeout",
            "30",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(6),
        "Invalid = exit 6 target state; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    let envelope = stderr_envelope(&out);
    assert_eq!(
        envelope["error"]["code"],
        Value::String("bundle_not_available".into()),
        "the Three-Place slug rides the envelope"
    );
    let message = envelope["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("Invalid"),
        "the observed state is named: {message}"
    );
    assert!(
        message.contains("generate"),
        "the fresh-generate fix is named: {message}"
    );
    assert_eq!(
        script.hits(),
        1,
        "EXACTLY one status hit — immediate exit, no further polls"
    );
}

/// The bundle ZIP bytes served verbatim (binary: starts with the
/// ZIP magic `PK\x03\x04` — the 8.3.6 capture's first 4 bytes).
const BUNDLE_ZIP_BYTES: &[u8] = b"PK\x03\x04 diagnostics bundle body bytes \xDE\xAD\xBE\xEF";

/// Mount the bundle download: raw bytes, optional
/// `Content-Disposition`. (No `expect` guard — the client-side
/// asserts below verify the fetch outcome directly; the downloads'
/// byte-verbatim + naming asserts ARE the contract.)
async fn mount_download(server: &wiremock::MockServer, content_disposition: Option<&str>) {
    let mut response = wiremock::ResponseTemplate::new(200)
        .set_body_bytes(BUNDLE_ZIP_BYTES)
        .append_header("Content-Type", "application/zip;charset=utf-8");
    if let Some(disposition) = content_disposition {
        response = response.append_header("Content-Disposition", disposition);
    }
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/diagnostics/bundle/download",
        ))
        .respond_with(response)
        .mount(server)
        .await
}

/// The download contract: bytes land VERBATIM (ZIP magic asserted),
/// the default timestamped name applies when no Content-Disposition
/// rides, and the disposition name wins when present. JSON data
/// carries {file, bytes, content_type}.
#[tokio::test]
async fn bundle_download_bytes_and_default_name() {
    // Default-name run: cwd is a scratch dir so the timestamped
    // default file lands there.
    let server = wiremock::MockServer::start().await;
    mount_download(&server, None).await;
    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let workdir = tempfile::tempdir().expect("scratch cwd");

    let mut command = Command::cargo_bin("ign").expect("binary 'ign' not found");
    command
        .env("IGNITION_CLI_CONFIG", &config)
        .env("IGNITION_TOKEN", "ign-contract-test:0123456789abcdef")
        .env_remove("IGNITION_URL")
        .env_remove("IGNITION_PROFILE")
        .args(["diagnostics", "bundle", "download", "--json"])
        .current_dir(workdir.path());
    let out = command.output().expect("spawn ign");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("envelope parses");
    assert_eq!(body["ok"], Value::Bool(true));

    let file = body["data"]["file"].as_str().expect("file name");
    assert!(
        file.starts_with("ignition-diagnostics-bundle-") && file.ends_with(".zip"),
        "the default timestamped name applied: {file}"
    );
    assert_eq!(
        body["data"]["bytes"],
        Value::Number(BUNDLE_ZIP_BYTES.len().into()),
        "byte count is the response body length"
    );
    assert_eq!(
        body["data"]["content_type"],
        Value::String("application/zip;charset=utf-8".into())
    );

    // Bytes verbatim (the ZIP magic + the exact payload).
    let written = std::fs::read(workdir.path().join(file)).expect("downloaded file");
    assert_eq!(&written[..4], b"PK\x03\x04", "ZIP magic intact");
    assert_eq!(written, BUNDLE_ZIP_BYTES, "byte-for-byte verbatim");

    // Content-Disposition wins when present (8.3 captures always send
    // one; parsed optionally per the Phase-4 version-tolerance rule).
    let server2 = wiremock::MockServer::start().await;
    mount_download(&server2, Some("attachment; filename=\"diag.zip\"")).await;
    let (_dir2, config2) = isolated_config();
    write_profile_config(&config2, &server2.uri());
    let workdir2 = tempfile::tempdir().expect("scratch cwd 2");

    let mut command = Command::cargo_bin("ign").expect("binary 'ign' not found");
    command
        .env("IGNITION_CLI_CONFIG", &config2)
        .env("IGNITION_TOKEN", "ign-contract-test:0123456789abcdef")
        .env_remove("IGNITION_URL")
        .env_remove("IGNITION_PROFILE")
        .args(["diagnostics", "bundle", "download", "--json"])
        .current_dir(workdir2.path());
    let out = command.output().expect("spawn ign");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("envelope parses");
    assert_eq!(
        body["data"]["file"],
        Value::String("diag.zip".into()),
        "the disposition basename wins over the default"
    );
    assert!(workdir2.path().join("diag.zip").exists(), "file landed");
}

/// The download timeout override is pinned DETERMINISTICALLY — this
/// test documents intent: the bundle download rides
/// `download_to_file`'s `RequestBuilder::timeout` with
/// [`BUNDLE_DOWNLOAD_TIMEOUT`] = 300 s (Pitfall 8: the 30 s client
/// default would truncate MB-sized bundles). The 300 s constant is
/// asserted cross-crate here AND unit-pinned at birth in
/// `client::diagnostics` (the REQUIRED phase check). A slow wiremock
/// (sleep >30 s) is deliberately NOT used — sleep-based tests are
/// flaky; the override's RIDE through the request handed to
/// `download_to_file` is pinned by construction (the impl passes the
/// constant as the pipeline's timeout parameter).
#[tokio::test]
async fn bundle_download_timeout_override_present() {
    assert_eq!(
        ignition_core::client::diagnostics::BUNDLE_DOWNLOAD_TIMEOUT,
        std::time::Duration::from_secs(300),
        "the per-request override is 300 s — the 30 s client default would truncate bundles"
    );
}
