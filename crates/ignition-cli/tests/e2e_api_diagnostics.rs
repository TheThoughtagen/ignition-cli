//! Opt-in LIVE gates for the Phase-9 diagnostics surface (09-06) —
//! the phase's verification oracle: the wiremock contract tests prove
//! the CONTRACT; these gates prove the WIRE, against real 8.3.x
//! gateways (the documented failure mode being point-release
//! variance), running the real `ign` binary end to end.
//!
//! ```text
//! # read-only gates (the CI-default quiet contract):
//! cargo test -p ignition-cli --test e2e_api_diagnostics            # green no-op
//! IGNITION_LIVE_URL=… IGNITION_LIVE_TOKEN=… \
//!   cargo test -p ignition-cli --test e2e_api_diagnostics -- --ignored
//! # + the bundle round-trip (the ONE gated mutation):
//! IGNITION_LIVE_MUTATIONS=1 … cargo test -p ignition-cli --test e2e_api_diagnostics -- --ignored
//! ```
//!
//! Skip behavior (the e2e_webdev.rs:14-73 pattern verbatim): every
//! gate reads its env vars at start and returns quietly when they are
//! absent — `-- --ignored` with no envs set is a GREEN no-op, and the
//! plain no-env `cargo test` run is green because the `#[ignore]`
//! gates never execute their bodies at all.
//!
//! ## Environment
//!
//! | var | required by | meaning |
//! |---|---|---|
//! | `IGNITION_LIVE_URL` | every gate | base URL, e.g. `http://localhost:18188` |
//! | `IGNITION_LIVE_TOKEN` | every gate | full `name:key` API-token string |
//! | `IGNITION_LIVE_MUTATIONS` | ONLY the bundle round-trip | `1` to allow `ign diagnostics bundle generate` (creates a support bundle; touches nothing else — the capture-proven benign loop) |
//!
//! ## Read-only discipline (roadmap directive: passthrough can't nuke
//! the rig)
//!
//! Exactly ONE mutating verb appears in this file's spawn list: the
//! bundle generate in `bundle_round_trip_live`, behind
//! `IGNITION_LIVE_MUTATIONS=1`. Gate 5's wrong-method probe is
//! read-only BY CONSTRUCTION (09-LIVE-CAPTURES §6 LOCKED decision 8:
//! the gateway REFUSES the answer before touching any resource —
//! "neither indicates the resource was touched; both are refusals").
//! No other mutating verb appears anywhere in this file.
//!
//! ## Capture-grounded assertions
//!
//! Every expectation asserted here cites 09-LIVE-CAPTURES.md (the
//! 2026-09-07 both-rig capture), never a guess: bundle state
//! vocabulary is the PascalCase `Generating`/`Valid` pair (§5,
//! decision 1), redundancy units are ms-since-gateway-start with the
//! `-1` never-synced sentinel (§3, decision 4), and the 4xx partition
//! is unknown-path 404+HTML vs wrong-method 404+EMPTY (§6, decision
//! 8). Where the plan's pre-capture hypothesis moved, the CAPTURE
//! wins (noted inline at the assertion).

use std::path::{Path, PathBuf};
use std::process::Output;

use assert_cmd::Command;
use serde_json::Value;

/// The gathered env contract for one live-gateway run (the
/// e2e_webdev shape verbatim).
struct LiveEnv {
    url: String,
    token: String,
}

fn live_url() -> Option<String> {
    std::env::var("IGNITION_LIVE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn live_token() -> Option<String> {
    std::env::var("IGNITION_LIVE_TOKEN")
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// The read-only gates' env: URL + TOKEN (mutations NOT required).
fn live_env_reads() -> Option<LiveEnv> {
    match (live_url(), live_token()) {
        (Some(url), Some(token)) => Some(LiveEnv { url, token }),
        _ => None,
    }
}

/// The bundle round-trip's env: URL + TOKEN + the explicit mutations
/// opt-in (the e2e_webdev refusal shape — the loop MUTATES, so it
/// additionally requires `IGNITION_LIVE_MUTATIONS=1`).
fn live_env_mutations() -> Option<LiveEnv> {
    if std::env::var("IGNITION_LIVE_MUTATIONS").as_deref() != Ok("1") {
        return None;
    }
    live_env_reads()
}

fn skip(message: &str) {
    eprintln!("skipping: {message}");
}

/// ONE isolated config per gate (the version_gateway_contract.rs
/// isolation shape): IGNITION_CLI_CONFIG → a tempfile whose single
/// profile points at the LIVE URL.
fn isolated_live_config(env: &LiveEnv) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("config.toml");
    std::fs::write(
        &config,
        format!(
            "active = \"dev\"\n\n[profiles.dev]\nurl = \"{}\"\n",
            env.url
        ),
    )
    .expect("write config");
    (dir, config)
}

/// Spawn the built `ign` binary at the live gateway (isolated config;
/// the token rides the config's `token_env` slot; ambient IGNITION_*
/// knobs stripped for determinism — the stdout-purity-harness rule).
fn ign(config: &Path, env: &LiveEnv, args: &[&str]) -> Output {
    let mut command = Command::cargo_bin("ign").expect("binary 'ign' not found");
    command
        .env("IGNITION_CLI_CONFIG", config)
        .env("IGNITION_TOKEN", &env.token);
    for knob in [
        "IGNITION_URL",
        "IGNITION_PROFILE",
        "IGNITION_JSON",
        "IGNITION_YES",
        "IGNITION_LIVE_URL",
        "IGNITION_LIVE_TOKEN",
        "IGNITION_LIVE_MUTATIONS",
    ] {
        command.env_remove(knob);
    }
    let mut all = args.to_vec();
    all.push("--json");
    command.args(&all).output().expect("spawn ign")
}

/// Parse the compact JSON success envelope from stdout.
fn data_envelope(out: &Output) -> Value {
    let stdout = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(stdout.trim())
        .unwrap_or_else(|err| panic!("stdout envelope parses ({err}): {stdout}"))
}

/// The error envelope starts at the first `{` on stderr (tracing
/// shares stderr by design — the contract-file convention).
fn err_envelope(out: &Output) -> Value {
    let stderr = String::from_utf8_lossy(&out.stderr);
    let from = stderr.find('{').expect("envelope starts somewhere");
    serde_json::from_str(&stderr[from..]).expect("envelope parses")
}

fn expect_ok(what: &str, out: &Output) {
    assert!(
        out.status.success(),
        "{what} failed (exit {:?}):\nstdout: {}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Gate 1 — `ign license status` (READ-ONLY). The merged
/// licenses+trial envelope answers ok with the typed fields from
/// 09-LIVE-CAPTURES §1+§2: `data.license.busy` (bool), the five
/// capture-named arrays (asserted as arrays — element shapes are NOT
/// capturable on a fresh rig and ride passthrough by design,
/// decision 5), and `data.trial.trialSecondsLeft` present (the trial
/// merge: mode + countdown live under `data.trial`, decision 6).
#[test]
#[ignore = "opt-in e2e: set IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN"]
fn license_status_live() {
    let Some(env) = live_env_reads() else {
        skip("IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN not set — live gate idle");
        return;
    };
    let (_dir, config) = isolated_live_config(&env);
    let _gate = LIVE_GATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let out = ign(&config, &env, &["license", "status"]);
    expect_ok("license status", &out);
    let body = data_envelope(&out);
    assert_eq!(body["ok"], Value::Bool(true), "{body}");
    assert!(
        body["data"]["license"].is_object(),
        "data.license parses: {body}"
    );
    // Capture §1: `busy` is a scalar bool on both rigs (fresh rig
    // = false; live truth may differ — the TYPE is the pin).
    assert!(
        body["data"]["license"]["busy"].is_boolean(),
        "busy rides as a bool: {body}"
    );
    // Capture §1 decision 5: the morning-check arrays exist (empty on
    // a fresh rig); elements are passthrough — never asserted here.
    for array in ["cloud", "certificate", "hardware", "embedded", "leased"] {
        assert!(
            body["data"]["license"][array].is_array(),
            "{array} rides as an array: {body}"
        );
    }
    // Capture §2 decision 6: the trial companion carries the
    // countdown (a fresh rig is mid-trial; only presence + numeric
    // type is pinned — the value ticks down live).
    assert!(
        body["data"]["trial"]["trialSecondsLeft"].is_number(),
        "trialSecondsLeft present: {body}"
    );
}

/// Gate 2 — `ign redundancy status` (READ-ONLY). The captured flat
/// model rides `data.status` (09-LIVE-CAPTURES §3): `role` within the
/// captured vocabulary, `uptime` ms-since-gateway-start (wall-clock
/// PROVEN twice on the captures — decision 4; a live value is
/// therefore a non-negative number of millisecond magnitude), and
/// `lastSyncTimestamp` where `-1` is the never-synced sentinel (unit
/// NOT capture-proven — never interpreted here, only type-pinned).
#[test]
#[ignore = "opt-in e2e: set IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN"]
fn redundancy_status_live() {
    let Some(env) = live_env_reads() else {
        skip("IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN not set — live gate idle");
        return;
    };
    let (_dir, config) = isolated_live_config(&env);
    let _gate = LIVE_GATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let out = ign(&config, &env, &["redundancy", "status"]);
    expect_ok("redundancy status", &out);
    let body = data_envelope(&out);
    assert_eq!(body["ok"], Value::Bool(true), "{body}");
    let status = &body["data"]["status"];
    assert!(status.is_object(), "data.status parses: {body}");
    // Captured role vocabulary (§3): `Independent` observed live on
    // both rigs; Primary/Backup are the configured-peer shapes that
    // were not capturable on a fresh rig — the gate accepts the full
    // captured set and refuses anything outside it (a NEW role string
    // is point-release variance worth a capture, not a silent pass).
    let role = status["role"].as_str().expect("role is a string");
    assert!(
        ["Independent", "Primary", "Backup"].contains(&role),
        "role within the captured vocabulary: {role}"
    );
    // UNITS per capture decision 4: uptime = ms since gateway start
    // (a minutes-old rig read ~460959 — the magnitude is the proof);
    // assert the type and non-negativity, never a hardcoded value.
    let uptime = status["uptime"].as_i64().expect("uptime is an integer");
    assert!(uptime >= 0, "uptime non-negative: {uptime}");
    // The `-1` never-synced sentinel (§3): type-only pin — the unit
    // is flagged INFERENCE in the model docs, so the gate never
    // interprets the value.
    assert!(
        status["lastSyncTimestamp"].is_i64(),
        "lastSyncTimestamp rides as an integer: {body}"
    );
}

/// Gate 3 — `ign gan status` (READ-ONLY). The zero-connection capture
/// (09-LIVE-CAPTURES §4, decision 7) is the canonical 5-scalar shape;
/// the gate pins the LIVE-TRUTH sanity invariant (running ≤ total,
/// both non-negative integers) rather than a wire-format guess — byte
/// rates are f64 per the captures (JSON `0.0`) but only type-checked
/// here (a busy rig's rates move).
#[test]
#[ignore = "opt-in e2e: set IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN"]
fn gan_status_live() {
    let Some(env) = live_env_reads() else {
        skip("IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN not set — live gate idle");
        return;
    };
    let (_dir, config) = isolated_live_config(&env);
    let _gate = LIVE_GATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let out = ign(&config, &env, &["gan", "status"]);
    expect_ok("gan status", &out);
    let body = data_envelope(&out);
    assert_eq!(body["ok"], Value::Bool(true), "{body}");
    let status = &body["data"]["status"];
    assert!(status.is_object(), "data.status parses: {body}");
    let total = status["totalConnections"]
        .as_i64()
        .expect("totalConnections is an integer");
    let running = status["runningConnections"]
        .as_i64()
        .expect("runningConnections is an integer");
    assert!(total >= 0, "total non-negative: {total}");
    assert!(running >= 0, "running non-negative: {running}");
    assert!(
        running <= total,
        "live-truth sanity: running ({running}) ≤ total ({total})"
    );
}

/// Gate 4 — `ign api call` envelope (READ-ONLY: one GET). The
/// 09-03 contract rides LIVE: `data.method`/`data.path` echo, the
/// gateway's status, and `data.result.data` carrying the gateway's
/// OWN JSON verbatim (RawValue passthrough — no field dropped, no
/// value coerced). The spot-check key is `version` — the field the
/// version-gateway contract itself parses out of gateway-info.
#[test]
#[ignore = "opt-in e2e: set IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN"]
fn api_call_envelope_live() {
    let Some(env) = live_env_reads() else {
        skip("IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN not set — live gate idle");
        return;
    };
    let (_dir, config) = isolated_live_config(&env);
    let _gate = LIVE_GATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let out = ign(
        &config,
        &env,
        &[
            "api",
            "call",
            "--method",
            "GET",
            "--path",
            "/data/api/v1/gateway-info",
        ],
    );
    expect_ok("api call gateway-info", &out);
    let body = data_envelope(&out);
    assert_eq!(body["ok"], Value::Bool(true), "{body}");
    assert_eq!(
        body["data"]["method"],
        Value::String("GET".into()),
        "{body}"
    );
    assert_eq!(
        body["data"]["path"],
        Value::String("/data/api/v1/gateway-info".into()),
        "{body}"
    );
    assert_eq!(body["data"]["result"]["status"], Value::Number(200.into()));
    let result = &body["data"]["result"]["data"];
    assert!(
        result.is_object(),
        "the gateway's own JSON parses as an object: {body}"
    );
    // Spot-check a capture-named key: the LIVE gateway-info body
    // (recorded 09-06 run, both rigs) carries the version under the
    // gateway-native key `ignitionVersion` ("8.3.6 (b2026042713)"
    // shape) — the same key `GatewayInfo` renames in (its `version`
    // name is a legacy alias only). The 09-02 captures recorded the
    // 200 status but not this body; the spot-key choice is therefore
    // grounded in the run's own capture, noted in 09-RIG-NOTES.
    assert!(
        result["ignitionVersion"].is_string(),
        "gateway-info.ignitionVersion rides verbatim: {body}"
    );
}

/// Gate 5 — the READ-ONLY 4xx partition (both probes are refusals BY
/// CONSTRUCTION — 09-LIVE-CAPTURES §6 LOCKED decision 8: "neither
/// indicates the resource was touched"). This is the gate where the
/// plan's pre-capture hypothesis moved, and the CAPTURE wins
/// (plan-mandated: "assert the captured answer"):
///
/// - unknown path (`GET /data/api/v1/definitely-not-a-real-endpoint`)
///   answered **404 + Jetty HTML** on both rigs → classifier exit 6
///   `not_found` (the 404 hypothesis held).
/// - wrong method (`DELETE /data/api/v1/gateway-info` — the pinned
///   read-only probe; DELETE on a read-only route touches nothing)
///   answered **404 + EMPTY body, NOT 405** on both rigs — the
///   plan's 405→exit-2 hypothesis is FALSIFIED by capture. The
///   classifier (classify.rs dispatch order) maps every 404 to
///   `NotFound` BEFORE the api-call catch-all could fire, so the
///   captured live answer through the binary is **exit 6
///   `not_found`** — asserted HERE, never the hypothesis.
#[test]
#[ignore = "opt-in e2e: set IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN"]
fn api_call_4xx_partition_live() {
    let Some(env) = live_env_reads() else {
        skip("IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN not set — live gate idle");
        return;
    };
    let (_dir, config) = isolated_live_config(&env);
    let _gate = LIVE_GATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    // Probe A — unknown path (capture §6: 404 + Jetty HTML page).
    let out = ign(
        &config,
        &env,
        &[
            "api",
            "call",
            "--method",
            "GET",
            "--path",
            "/data/api/v1/definitely-not-a-real-endpoint",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(6),
        "unknown path exits 6 (not_found); stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    let envelope = err_envelope(&out);
    assert_eq!(envelope["ok"], Value::Bool(false));
    assert_eq!(
        envelope["error"]["code"],
        Value::String("not_found".into()),
        "the capture-backed slug: {envelope}"
    );

    // Probe B — wrong method on a real path (capture §6 LOCKED
    // decision 8: 404 + EMPTY body, NOT 405; read-only refusal).
    let out = ign(
        &config,
        &env,
        &[
            "api",
            "call",
            "--method",
            "DELETE",
            "--path",
            "/data/api/v1/gateway-info",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(6),
        "the CAPTURED answer (404-empty) exits 6 — the plan's 405/exit-2 \
         hypothesis was falsified by the 09-02 capture; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    let envelope = err_envelope(&out);
    assert_eq!(envelope["ok"], Value::Bool(false));
    assert_eq!(
        envelope["error"]["code"],
        Value::String("not_found".into()),
        "404 classifies before the api-call catch-all: {envelope}"
    );
}

/// Gate 6 — the diagnostics-bundle round-trip. THE FILE'S ONLY
/// MUTATION (`ign diagnostics bundle generate`): gated behind
/// `IGNITION_LIVE_MUTATIONS=1` with the e2e_webdev refusal
/// early-return (the loop MUTATES, so it requires the explicit
/// opt-in). Order per plan: generate → wait → download → re-read
/// status, everything inside a tempdir, assertions against the
/// CAPTURED state machine (09-LIVE-CAPTURES §5, decisions 1-3):
/// PascalCase `Generating` → `Valid`, `fileSize` appears only at the
/// terminal state and equals the download exactly, the ZIP magic
/// opens the body, and the ready state persists after download
/// (repeatable).
#[test]
#[ignore = "opt-in e2e: set IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN + IGNITION_LIVE_MUTATIONS=1"]
fn bundle_round_trip_live() {
    let Some(env) = live_env_mutations() else {
        skip(
            "IGNITION_LIVE_MUTATIONS=1 (with URL+TOKEN) not set — refusing to touch a live gateway",
        );
        return;
    };
    let (_dir, config) = isolated_live_config(&env);
    let _gate = LIVE_GATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let tmp = tempfile::tempdir().expect("download tempdir");

    // (1) generate — the 200 answer IS the fresh status: captured
    // `Generating` at birth on both rigs (a very fast rig could
    // answer `Valid` already — both are captured-vocabulary states).
    let out = ign(&config, &env, &["diagnostics", "bundle", "generate"]);
    expect_ok("bundle generate", &out);
    let body = data_envelope(&out);
    assert_eq!(body["ok"], Value::Bool(true), "{body}");
    let born = body["data"]["state"].as_str().expect("state is a string");
    assert!(
        ["Generating", "Valid"].contains(&born),
        "the birth state is within the CAPTURED vocabulary \
         (Generating observed at birth on both rigs): {born}"
    );

    // (2) wait — polls honestly until the captured terminal state.
    let out = ign(
        &config,
        &env,
        &[
            "diagnostics",
            "bundle",
            "wait",
            "--interval",
            "2",
            "--timeout",
            "300",
        ],
    );
    expect_ok("bundle wait", &out);
    let body = data_envelope(&out);
    assert_eq!(
        body["data"]["state"],
        Value::String("Valid".into()),
        "the captured terminal state: {body}"
    );

    // (3) download — the streamed ZIP lands verbatim.
    let zip_path = tmp.path().join("b.zip");
    let out = ign(
        &config,
        &env,
        &[
            "diagnostics",
            "bundle",
            "download",
            "--output",
            zip_path.to_str().expect("utf-8 temp path"),
        ],
    );
    expect_ok("bundle download", &out);
    let bytes = std::fs::read(&zip_path).expect("the bundle file landed");
    assert!(!bytes.is_empty(), "the bundle is non-empty");
    assert_eq!(
        &bytes[..4],
        b"PK\x03\x04",
        "the ZIP magic opens the body (capture §5)"
    );

    // (4) re-read status — the ready state PERSISTS after download
    // (repeatable per capture) and `fileSize` equals the downloaded
    // byte count exactly (decision 2: fileSize == Content-Length).
    let out = ign(&config, &env, &["diagnostics", "bundle", "status"]);
    expect_ok("bundle status after download", &out);
    let body = data_envelope(&out);
    assert_eq!(body["ok"], Value::Bool(true), "{body}");
    assert_eq!(
        body["data"]["state"],
        Value::String("Valid".into()),
        "the ready state persists after download: {body}"
    );
    let file_size = body["data"]["fileSize"]
        .as_u64()
        .expect("fileSize present at Valid (bytes)");
    assert_eq!(
        file_size,
        bytes.len() as u64,
        "fileSize equals the downloaded bytes exactly (capture decision 2)"
    );
}

/// THE live-gate serializer: every gate shares one live gateway, so
/// they run ONE AT A TIME within the process (the e2e_webdev
/// LIVE_GATE discipline; cargo's default parallelism would interleave
/// the round-trip with the reads — harmless in theory, hostile to
/// timing-sensitive assertions like fileSize equality).
static LIVE_GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());
