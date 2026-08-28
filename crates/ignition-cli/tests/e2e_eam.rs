//! Opt-in LIVE gate for `ign eam` (07-02, BKUP-02) — dogfoods the
//! BUILT `ign` binary against a real commissioned Ignition 8.3+
//! gateway, HONESTLY FORKING on the gateway's controller state:
//!
//! ```text
//! cargo test -p ignition-cli --test e2e_eam -- --ignored
//! ```
//!
//! - **non-controller gateway** (the stock shape): the runtime seam
//!   refuses `eam_not_controller` — the gate asserts that refusal's
//!   envelope verbatim (slug + manual-flip hint), never
//!   `auth_rejected`.
//! - **controller gateway** (`IGNITION_LIVE_EAM_CONTROLLER=1` — the
//!   manual flip is a config-resource PUT documented in the README;
//!   the CLI deliberately does not automate it): the full
//!   definitions create (OnDemand backup, unguarded) → force →
//!   history loop, with the GNET/trial execution outcomes surfaced
//!   as DATA.
//!
//! Skip behavior (the 02-01 live-suite convention): every test reads
//! its env vars at start and returns quietly when absent —
//! `-- --ignored` with no envs is a GREEN no-op. The controller-loop
//! test MUTATES the gateway (it creates + dispatches a task
//! definition), so it additionally requires
//! `IGNITION_LIVE_MUTATIONS=1`.
//!
//! ## Environment
//!
//! | var | required by | meaning |
//! |---|---|---|
//! | `IGNITION_LIVE_URL` | every test | base URL, e.g. `http://localhost:9088` |
//! | `IGNITION_LIVE_TOKEN` | every test | full `name:key` API-token string |
//! | `IGNITION_LIVE_EAM_CONTROLLER` | the loop test | `1` when the gateway's EAM installMode is Controller |
//! | `IGNITION_LIVE_MUTATIONS` | the loop test | `1` to allow the create + force |
//!
//! ## Trial/GNET prerequisites (env-notes, NOT automated here)
//!
//! Task EXECUTION needs a live trial (`Trial timer is expired`
//! blocks runs — `ign rig trial reset --yes` restarts an expired
//! rig trial, the README recipe) AND a GNET-connected agent target
//! (even `_controller` self-targets fail until Gateway Network is
//! configured). The gate stays READ-honest on stock rigs: those
//! outcomes surface in history rows as data.

use std::path::PathBuf;
use std::process::Output;

use assert_cmd::Command;
use serde_json::Value;

fn live_env() -> Option<(String, String)> {
    let url = std::env::var("IGNITION_LIVE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())?;
    let token = std::env::var("IGNITION_LIVE_TOKEN")
        .ok()
        .filter(|value| !value.trim().is_empty())?;
    Some((url, token))
}

fn mutations_allowed() -> bool {
    std::env::var("IGNITION_LIVE_MUTATIONS").is_ok_and(|value| value == "1")
}

fn controller_mode() -> bool {
    std::env::var("IGNITION_LIVE_EAM_CONTROLLER").is_ok_and(|value| value == "1")
}

fn isolated_config() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.toml");
    (dir, path)
}

fn ign(config: &std::path::Path, url: &str, token: &str, args: &[&str]) -> Output {
    let mut command = Command::cargo_bin("ign").expect("binary 'ign' not found");
    command
        .env("IGNITION_CLI_CONFIG", config)
        .env("IGNITION_TOKEN", token)
        .env("IGNITION_URL", url);
    command.args(args).output().expect("spawn ign")
}

/// stderr's JSON envelope starting at the first `{` (log-tolerant parse).
fn stderr_envelope(out: &Output) -> Value {
    let stderr = String::from_utf8_lossy(&out.stderr);
    let start = stderr.find('{').unwrap_or(0);
    serde_json::from_str(&stderr[start..]).expect("error envelope parses")
}

/// **Fork (a) — the stock-gateway refusal.** `ign eam history`
/// against a non-controller gateway exits 6 with
/// `eam_not_controller` + the manual-flip hint — the honest state
/// gate, live-witnessed (07-RESEARCH). Skipped quietly when the
/// gateway IS a controller (that shape has its own loop test).
#[test]
#[ignore = "live gate: needs IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN"]
fn non_controller_gateway_refuses_eam_not_controller() {
    let Some((url, token)) = live_env() else {
        return; // quiet skip (the live-suite convention)
    };
    if controller_mode() {
        return; // the fork's other arm owns that gateway
    }

    let (_dir, config) = isolated_config();
    let out = ign(&config, &url, &token, &["eam", "history", "--compact"]);
    assert_eq!(
        out.status.code(),
        Some(6),
        "the state gate is target-state, not auth"
    );
    let body = stderr_envelope(&out);
    assert_eq!(
        body["error"]["code"],
        Value::String("eam_not_controller".into()),
        "never a misleading auth_rejected"
    );
    let hint = body["error"]["hint"].as_str().expect("hint");
    assert!(
        hint.contains("installMode") && hint.contains("Controller"),
        "the hint names the manual flip: {hint}"
    );
}

/// **Fork (b) — the controller loop.** On a controller-configured
/// gateway (the env-declared state): create an OnDemand eam_backup
/// definition (UNGUARDED per the ladder), force-dispatch it
/// (--yes), and read history — the run's outcome (Failed on an
/// unconfigured-GNET rig, Success on a provisioned one) surfaces as
/// DATA either way. Requires IGNITION_LIVE_MUTATIONS=1.
#[test]
#[ignore = "live gate: needs IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN + controller + mutations opt-in"]
fn controller_gateway_creates_forces_and_reads_history() {
    let Some((url, token)) = live_env() else {
        return; // quiet skip
    };
    if !controller_mode() || !mutations_allowed() {
        return; // the state gate / opt-in refuses honestly
    }

    let (_dir, config) = isolated_config();
    let name = format!("e2e-backup-{}", std::process::id());

    // 1. Create — eam_backup + OnDemand is the ladder's unguarded
    //    cell (no --yes needed).
    let out = ign(
        &config,
        &url,
        &token,
        &[
            "eam",
            "task",
            "new",
            &name,
            "eam_backup",
            "--setting",
            "controllerIsTargetKey=true",
            "--compact",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: Value = serde_json::from_slice(&out.stdout).expect("create envelope parses");
    assert_eq!(
        body["data"]["schedule_mode"],
        Value::String("OnDemand".into())
    );

    // 2. Force — always guarded; --yes rides.
    let out = ign(
        &config,
        &url,
        &token,
        &["eam", "task", "force", &name, "--yes", "--compact"],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "dispatch acceptance — outcomes land in history: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: Value = serde_json::from_slice(&out.stdout).expect("force envelope parses");
    assert_eq!(body["data"]["dispatched"], Value::Bool(true));
    // The history entry (when visible) carries its outcome as data —
    // GNET-not-connected / trial-expired are honest Failed runs.
    if let Some(entry) = body["data"]["history"].as_object() {
        eprintln!("live history entry: {entry:?}");
    }

    // 3. History read-back names the forced run.
    let out = ign(&config, &url, &token, &["eam", "history", "--compact"]);
    assert_eq!(out.status.code(), Some(0));
    let body: Value = serde_json::from_slice(&out.stdout).expect("history envelope parses");
    let items = body["data"]["items"].as_array().expect("items array");
    assert!(
        items.iter().any(|item| item["taskName"]
            .as_str()
            .is_some_and(|n| n.starts_with(&name))),
        "the forced run's entry is visible in history"
    );

    // 4. The definition lists (config-resource seam — worked before
    //    the controller flip and after).
    let out = ign(&config, &url, &token, &["eam", "tasks", "--compact"]);
    assert_eq!(out.status.code(), Some(0));
    let body: Value = serde_json::from_slice(&out.stdout).expect("tasks envelope parses");
    assert!(
        body["data"]["tasks"]
            .as_array()
            .expect("tasks array")
            .iter()
            .any(|task| task["name"].as_str() == Some(name.as_str())),
        "the created definition lists"
    );
}
