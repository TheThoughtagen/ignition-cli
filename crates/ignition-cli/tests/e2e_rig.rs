//! Opt-in e2e harness for the rig family's snapshot/restore loop
//! (04-04, RIG-04) — dogfoods the BUILT `ign` binary against a real
//! compose rig, pinning the round-trip TWO-SIDED (the 03-03
//! replace-not-merge precedent translated to gwbk): a pre-snapshot
//! project SURVIVES the restore, a post-snapshot project is GONE —
//! restore returns the rig to its snapshotted state, proven in BOTH
//! directions.
//!
//! ```text
//! cargo test -p ignition-cli --test e2e_rig -- --ignored
//! ```
//!
//! Skip behavior (the 02-01 live-suite convention): the test reads
//! its env at start and returns quietly when absent — `-- --ignored`
//! with no envs is a GREEN no-op. Mutations additionally require
//! `IGNITION_LIVE_MUTATIONS=1` (an explicit opt-in before anything
//! touches gateway state — a restore REPLACES the gateway's state).
//!
//! ## Environment
//!
//! | var | required | meaning |
//! |---|---|---|
//! | `IGNITION_LIVE_URL` | yes | the RIG's gateway URL (e.g. `http://localhost:9088`) — the marker commands address it |
//! | `IGNITION_LIVE_TOKEN` | yes | a working API token on the rig (rides both the profile-addressed markers and `IGNITION_TOKEN` for the rig verbs) |
//! | `IGNITION_LIVE_MUTATIONS` | yes | `1` to allow the round-trip (restore is maximally mutating) |
//! | `IGNITION_RIG` | optional | names the rig when the cwd/convention scan should not decide (the rig verbs need the rig DISCOVERABLE — on the dev machine the git-module convention resolves it) |
//!
//! ## The Pitfall-5 wrinkle (why failures here are informative)
//!
//! gwbk restores "modify/clear often" API tokens stored under CORE
//! config (83-api) — the snapshot CONTAINS the token definition, so
//! the restore should bring it back, but if the gateway resets it the
//! post-restore reads 401 and the test FAILS with the honest
//! diagnostic (re-provision the token, re-run). That outcome is a
//! valid observation of the pitfall, not a harness bug. The trial
//! clock's behavior across restores (the 04-03 open observation
//! point) is PRINTED before/after — observed, not asserted.
//!
//! A failure mid-loop leaves forensic state on the gateway (the
//! timestamped projects) and a snapshot directory under the test
//! tempdir for inspection; cleanup is best-effort.

use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::{SystemTime, UNIX_EPOCH};

use assert_cmd::Command;
use serde_json::Value;

/// The gathered env contract for one live-rig run.
struct LiveEnv {
    /// `IGNITION_LIVE_URL` — the RIG's gateway URL.
    url: String,
    /// `IGNITION_LIVE_TOKEN` — the full `name:key` string.
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

fn live_mutations() -> bool {
    std::env::var("IGNITION_LIVE_MUTATIONS").is_ok_and(|value| value == "1")
}

fn live_env() -> Option<LiveEnv> {
    if !live_mutations() {
        return None;
    }
    match (live_url(), live_token()) {
        (Some(url), Some(token)) => Some(LiveEnv { url, token }),
        _ => None,
    }
}

fn skip(message: &str) {
    eprintln!("skipping: {message}");
}

/// Isolated config dir + an EMPTY config — the rig verbs read config
/// for `[rig]`/`[rigs.*]` but discovery may legitimately land on the
/// cwd/convention scan; isolation just keeps THIS run hermetic.
fn isolated_config() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("config.toml");
    (dir, config)
}

/// Spawn `ign rig …` — the rig family: isolated config, the live
/// token in `IGNITION_TOKEN` (the rig-family cred source), everything
/// else inherited (IGNITION_RIG flows through when set).
fn ign_rig(config: &Path, env: &LiveEnv, args: &[&str]) -> Output {
    let mut command = Command::cargo_bin("ign").expect("binary 'ign' not found");
    command
        .env("IGNITION_CLI_CONFIG", config)
        .env("IGNITION_TOKEN", &env.token);
    command.args(args).output().expect("spawn ign")
}

/// Spawn `ign …` for the PROFILE-addressed marker commands (project /
/// resource): the e2e_projects pattern — isolated config with a `dev`
/// profile pointing at the live rig via env overlay.
fn ign_profile(workdir: &Path, env: &LiveEnv, args: &[&str]) -> Output {
    let config = workdir.join("marker-config.toml");
    std::fs::write(
        &config,
        format!(
            "active = \"dev\"\n\n[profiles.dev]\nurl = \"{}\"\nauth = {{ token_env = \"IGNITION_TOKEN\" }}\n",
            env.url
        ),
    )
    .expect("write marker config");
    let mut command = Command::cargo_bin("ign").expect("binary 'ign' not found");
    command
        .env("IGNITION_CLI_CONFIG", &config)
        .env("IGNITION_TOKEN", &env.token)
        .env("IGNITION_URL", &env.url);
    command.args(args).output().expect("spawn ign")
}

fn timestamped_name(suffix: &str) -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after the unix epoch")
        .as_secs();
    format!("ign-e2e-rig-{ts}{suffix}")
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

fn data_envelope(out: &Output) -> Value {
    let stdout = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(stdout.trim())
        .unwrap_or_else(|err| panic!("stdout envelope parses ({err}): {stdout}"))
}

/// Assert a failure exit + slug, returning the full error envelope.
/// (Currently unused by the round-trip — the two-sided pin asserts
/// via list absence — but the harness keeps it for the rig family's
/// future gates; the 01-01 CI lesson: expect on test targets, allow
/// here instead.)
#[allow(dead_code)]
fn expect_exit(out: &Output, code: i32, slug: &str, what: &str) -> Value {
    assert_eq!(
        out.status.code(),
        Some(code),
        "{what} must exit {code} (got {:?}):\nstdout: {}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    let envelope: Value = serde_json::from_str(stderr[stderr.find('{').unwrap_or(0)..].trim())
        .unwrap_or_else(|err| panic!("stderr envelope parses ({err}): {stderr}"));
    assert_eq!(
        envelope["error"]["code"], slug,
        "{what} must be {slug}: {envelope}"
    );
    envelope
}

/// The scratch script resource the witnesses write (the e2e_projects
/// family path).
const SCRATCH_PATH: &str = "ignition/script-python/e2e/snapshot";

/// THE round-trip gate (order is the contract — see the module docs):
/// pre-witness project + resource → snapshot (gwbk + manifest
/// asserted) → post-snapshot marker project → `rig restore --yes`
/// (witnessed RUNNING + the token warning in data) → TWO-SIDED pin
/// (pre-witness SURVIVED, marker GONE) → doctor's read-only tail.
/// The trial clock is observed (printed) before/after, never
/// asserted — its restore behavior is a documented unknown.
#[test]
#[ignore = "opt-in e2e: set IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN + IGNITION_LIVE_MUTATIONS=1 (the rig must be discoverable — see module docs)"]
fn snapshot_mutate_restore_round_trip() {
    let Some(env) = live_env() else {
        skip(
            "IGNITION_LIVE_MUTATIONS=1 (with URL+TOKEN) not set — refusing to restore over a live rig",
        );
        return;
    };
    let work = tempfile::tempdir().expect("workdir");
    let (_config_dir, rig_config) = isolated_config();
    let snap_dir = work.path().join("snap");

    // 0. The trial-clock observation point (printed, not asserted).
    let trial_before = ign_rig(&rig_config, &env, &["rig", "trial", "status", "--compact"]);
    if trial_before.status.success() {
        let trial = data_envelope(&trial_before)["data"].clone();
        eprintln!(
            "trial BEFORE: expired={}, {}s left",
            trial["expired"], trial["trial_remaining_s"]
        );
    }

    // 1. The PRE-snapshot witness: a project + resource that must
    //    SURVIVE the restore (it rides inside the gwbk).
    let pre_name = timestamped_name("-pre");
    expect_ok(
        "project new (pre-witness)",
        &ign_profile(
            work.path(),
            &env,
            &["project", "new", &pre_name, "--compact"],
        ),
    );
    let scratch_file = work.path().join("scratch.json");
    std::fs::write(
        &scratch_file,
        r#"{"scope":"G","code":"print('survives-restore')"}"#,
    )
    .expect("write scratch body");
    expect_ok(
        "resource put (pre-witness)",
        &ign_profile(
            work.path(),
            &env,
            &[
                "resource",
                "put",
                &pre_name,
                SCRATCH_PATH,
                "--file",
                scratch_file.to_str().unwrap(),
                "--compact",
            ],
        ),
    );

    // 2. THE snapshot: gwbk + per-project exports + manifest.
    expect_ok(
        "rig snapshot",
        &ign_rig(
            &rig_config,
            &env,
            &["rig", "snapshot", "-o", snap_dir.to_str().unwrap(), "--compact"],
        ),
    );
    let manifest = read_manifest(&snap_dir);

    // 3. The POST-snapshot marker: a project the gwbk does NOT
    //    contain — restore must REMOVE it (the two-sided second half).
    let post_name = timestamped_name("-post");
    expect_ok(
        "project new (post-snapshot marker)",
        &ign_profile(
            work.path(),
            &env,
            &["project", "new", &post_name, "--compact"],
        ),
    );

    // 4. THE restore (guarded; the action waits for the witnessed
    //    RUNNING — this is the slow leg, the gateway restarts).
    let gwbk_path = snap_dir.join(manifest["gwbk"].as_str().expect("gwbk file name"));
    assert!(gwbk_path.exists(), "the gwbk landed: {}", gwbk_path.display());
    let restored = ign_rig(
        &rig_config,
        &env,
        &[
            "rig",
            "restore",
            "--file",
            gwbk_path.to_str().unwrap(),
            "--yes",
            "--compact",
        ],
    );
    if !restored.status.success() {
        let stderr = String::from_utf8_lossy(&restored.stderr);
        panic!(
            "rig restore failed (exit {:?}):\nstdout: {}\nstderr: {}",
            restored.status.code(),
            String::from_utf8_lossy(&restored.stdout),
            stderr
        );
    }
    let restore_data = data_envelope(&restored)["data"].clone();
    assert_eq!(
        restore_data["state"], Value::String("running".into()),
        "success is a WITNESSED RUNNING, never a bare 2xx: {restore_data}"
    );
    let warnings = restore_data["warnings"]
        .as_array()
        .expect("warnings array (all keys always)");
    assert!(
        warnings
            .iter()
            .any(|warning| warning.as_str().is_some_and(|warning| warning.contains("API tokens"))),
        "the Pitfall-5 token warning rides data: {restore_data}"
    );

    // 5. THE two-sided pin.
    //    Side A — the pre-witness SURVIVED (rode the gwbk)…
    let survived = ign_profile(
        work.path(),
        &env,
        &["resource", "get", &pre_name, SCRATCH_PATH, "--compact"],
    );
    if !survived.status.success() && survived.status.code() == Some(5) {
        panic!(
            "post-restore reads 401'd — the restore CLOBBERED the API token (Pitfall 5 \
             observed live): re-provision the token via the gateway UI, then re-run. \
             stderr: {}",
            String::from_utf8_lossy(&survived.stderr)
        );
    }
    expect_ok("pre-witness resource survived", &survived);
    let got = data_envelope(&survived)["data"].clone();
    assert_eq!(got["content_kind"], Value::String("json".into()));
    //    …side B — the post-snapshot marker is GONE (absent from the
    //    gwbk → removed by the restore): the list no longer names it
    //    (the e2e_projects list-contains pin, inverted).
    let listed = ign_profile(work.path(), &env, &["project", "list", "--compact"]);
    expect_ok("project list post-restore", &listed);
    let names = data_envelope(&listed)["data"]["projects"]
        .as_array()
        .expect("projects array")
        .iter()
        .map(|project| project["name"].as_str().expect("name").to_string())
        .collect::<Vec<_>>();
    assert!(
        !names.contains(&post_name),
        "the post-snapshot project must be GONE after the restore — found: {names:?}"
    );
    assert!(
        names.contains(&pre_name),
        "the pre-witness project itself also survived: {names:?}"
    );

    // 6. The trial-clock observation, after half — printed only.
    let trial_after = ign_rig(&rig_config, &env, &["rig", "trial", "status", "--compact"]);
    if trial_after.status.success() {
        let trial = data_envelope(&trial_after)["data"].clone();
        eprintln!(
            "trial AFTER: expired={}, {}s left (restore's clock behavior is observational)",
            trial["expired"], trial["trial_remaining_s"]
        );
    }

    // 7. The read-only doctor tail — the post-restore health check.
    let doctor = ign_profile(work.path(), &env, &["doctor", "--compact"]);
    expect_ok("doctor post-restore", &doctor);

    // 8. Cleanup (best-effort — the marker is already gone via the
    //    restore; the pre-witness needs an explicit delete).
    let _ = ign_profile(
        work.path(),
        &env,
        &["project", "delete", &pre_name, "--yes", "--compact"],
    )
    .status
    .success();
}

/// The manifest IS the composition record — read it from the snapshot
/// dir and sanity-check the shape (the exact manifest shape is
/// unit-pinned in ignition-core; here we only need the gwbk name to
/// restore and evidence the composition).
fn read_manifest(snap_dir: &Path) -> Value {
    let manifest_path = snap_dir.join("manifest.json");
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(&manifest_path)
            .unwrap_or_else(|err| panic!("read {}: {err}", manifest_path.display())),
    )
    .expect("manifest parses");
    assert!(
        manifest["gwbk"].as_str().is_some_and(|name| !name.is_empty()),
        "manifest carries the gwbk file name: {manifest}"
    );
    assert!(
        manifest["projects"].as_array().is_some(),
        "manifest carries the projects array: {manifest}"
    );
    assert!(
        manifest["notes"].as_array().is_some_and(|notes| notes.len() == 2),
        "manifest carries BOTH composition notes: {manifest}"
    );
    manifest
}
