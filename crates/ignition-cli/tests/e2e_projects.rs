//! Opt-in e2e harness for the project family — dogfoods the BUILT
//! `ign` binary against a real commissioned Ignition 8.3+ gateway via
//! `assert_cmd` (true end-to-end, not trait-level; 03-RESEARCH §e2e
//! Harness Skeleton). Phases 4 (rig) and 5 (webdev deploy) EXTEND
//! this file: shared env helpers here, one test per capability loop.
//!
//! ```text
//! cargo test -p ignition-cli --test e2e_projects -- --ignored
//! ```
//!
//! Skip behavior (the 02-01 live-suite convention): every test reads
//! its env vars at start and returns quietly when they are absent —
//! `-- --ignored` with no envs set is a GREEN no-op, so the suite
//! adds ZERO default CI cost. Reads need `IGNITION_LIVE_URL` +
//! `IGNITION_LIVE_TOKEN`; MUTATIONS additionally require
//! `IGNITION_LIVE_MUTATIONS=1` (the 02-04 precedent — an explicit
//! opt-in before anything touches gateway state).
//!
//! ## Environment
//!
//! | var | required by | meaning |
//! |---|---|---|
//! | `IGNITION_LIVE_URL` | every test | base URL, e.g. `http://localhost:18088` |
//! | `IGNITION_LIVE_TOKEN` | every test | full `name:key` API-token string |
//! | `IGNITION_LIVE_MUTATIONS` | the loop test | `1` to allow create/put/import/rename/copy/delete |
//!
//! ## The loop test's contract pins (the phase's open questions)
//!
//! 05-02 RE-POINT: the resource verbs ride project-export ZIP
//! surgery (export → member surgery → overwrite-import) — this loop
//! is live-runnable against a real 8.3 gateway for the FIRST time
//! since Phase 3 (the `/projects/{p}/resources/**` REST routes never
//! existed; the STATE.md cross-phase blocker is CLOSED by that
//! plan). Resource `put`/`delete` are `--yes`-guarded now — every
//! mutation implicitly re-imports the project.
//!
//! ORDER MATTERS — the loop additionally pins TWO-SIDED put honesty
//! (a get before the second put witnesses the OLD member content —
//! get rides the same export; the put after it flips the content a
//! follow-up get reads back) and the delete surgery (the member is
//! `not_found` after `resource delete`). The PROJECT-level pins stay
//! from 03-02: export happens AFTER the first resource put and
//! BEFORE the second, because overwrite import REPLACES the entire
//! project (Pitfall 4). The two post-import gets pin BOTH halves of
//! the replace-not-merge contract: the pre-export resource SURVIVED
//! (export→import round-trip fidelity) and the post-export resource
//! is GONE (`not_found` — resources absent from the ZIP are deleted;
//! merge is Designer-only). Timestamped project names mean a failure
//! leaves forensic state on the gateway for inspection; cleanup is
//! best-effort.

use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::{SystemTime, UNIX_EPOCH};

use assert_cmd::Command;
use serde_json::Value;

/// The gathered env contract for one live-gateway run.
struct LiveEnv {
    /// `IGNITION_LIVE_URL` — gateway base URL.
    url: String,
    /// `IGNITION_LIVE_TOKEN` — the full `name:key` string.
    token: String,
}

/// Non-empty `IGNITION_LIVE_URL`, when set.
fn live_url() -> Option<String> {
    std::env::var("IGNITION_LIVE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// Non-empty `IGNITION_LIVE_TOKEN`, when set.
fn live_token() -> Option<String> {
    std::env::var("IGNITION_LIVE_TOKEN")
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// Whether `IGNITION_LIVE_MUTATIONS=1` opted the mutations in.
fn live_mutations() -> bool {
    std::env::var("IGNITION_LIVE_MUTATIONS").is_ok_and(|value| value == "1")
}

fn skip(message: &str) {
    eprintln!("skipping: {message}");
}

/// Read the read-suite env contract; `None` (caller skips quietly)
/// when either half is absent.
fn live_env() -> Option<LiveEnv> {
    match (live_url(), live_token()) {
        (Some(url), Some(token)) => Some(LiveEnv { url, token }),
        _ => None,
    }
}

/// The mutation gate: read env + the explicit mutations opt-in.
fn live_env_mutations() -> Option<LiveEnv> {
    if !live_mutations() {
        return None;
    }
    live_env()
}

/// An isolated config dir + a one-profile config pointing the `dev`
/// profile at the LIVE gateway with the token from `IGNITION_TOKEN`.
/// `--profile`-free: every spawn carries `IGNITION_URL` +
/// `IGNITION_TOKEN` env instead (the snapbox lesson — isolated
/// `IGNITION_CLI_CONFIG` per run so nothing leaks between tests).
fn isolated_live_config(env: &LiveEnv) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("config.toml");
    std::fs::write(
        &config,
        format!(
            "active = \"dev\"\n\n[profiles.dev]\nurl = \"{}\"\nauth = {{ token_env = \"IGNITION_TOKEN\" }}\n",
            env.url
        ),
    )
    .expect("write config");
    (dir, config)
}

/// Spawn the built `ign` binary at the live gateway with args.
fn ign(config: &Path, env: &LiveEnv, args: &[&str]) -> Output {
    let mut command = Command::cargo_bin("ign").expect("binary 'ign' not found");
    command
        .env("IGNITION_CLI_CONFIG", config)
        .env("IGNITION_TOKEN", &env.token)
        .env("IGNITION_URL", &env.url);
    command.args(args).output().expect("spawn ign")
}

/// A timestamped project name — failures leave forensic state on the
/// gateway (deliberate: `ign project list` shows what a run left
/// behind) and parallel runs never collide.
fn timestamped_name(suffix: &str) -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after the unix epoch")
        .as_secs();
    format!("ign-e2e-{ts}{suffix}")
}

/// Assert success, printing stderr for diagnosis on failure.
fn expect_ok(what: &str, out: &Output) {
    assert!(
        out.status.success(),
        "{what} failed (exit {:?}):\nstdout: {}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Parse the compact JSON success envelope from stdout.
fn data_envelope(out: &Output) -> Value {
    let stdout = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(stdout.trim())
        .unwrap_or_else(|err| panic!("stdout envelope parses ({err}): {stdout}"))
}

/// Assert a failure exit + slug, returning the full error envelope
/// (callers assert the endpoint/context they care about).
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

/// The scratch script resource the loop writes — a `code.py`-style
/// JSON body under the CORE module's real folder name (`ignition/`,
/// not the mcp docs' `com.inductiveautomation.ignition`).
const SCRATCH_PATH: &str = "ignition/script-python/e2e/scratch";
/// The SECOND scratch, written only AFTER the export so it exists on
/// the gateway but NOT in the ZIP (the replace-not-merge witness).
const SCRATCH2_PATH: &str = "ignition/script-python/e2e/scratch2";

/// THE full loop (order is the contract — see the module docs):
/// new → list-contains → resource list EMPTY → put scratch --yes
/// (surgery) → get round-trip → resource list CONTAINS → TWO-SIDED
/// put honesty (get OLD → export (scope metadata) → put scratch NEW
/// --yes → get NEW) → put scratch2 --yes (gateway-only) → abort
/// import (`project_exists`) → overwrite import (+`--yes`) →
/// scratch SURVIVED with OLD content + scratch2 `not_found`
/// (replace-not-merge, Pitfall 4) → resource delete scratch --yes →
/// get `not_found` (the delete surgery witness) → rename → copy →
/// delete both (cleanup).
#[test]
#[ignore = "opt-in e2e: set IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN + IGNITION_LIVE_MUTATIONS=1"]
fn full_project_resource_loop() {
    let Some(env) = live_env_mutations() else {
        skip(
            "IGNITION_LIVE_MUTATIONS=1 (with URL+TOKEN) not set — refusing to touch a live gateway",
        );
        return;
    };
    let (_dir, config) = isolated_live_config(&env);
    let name = timestamped_name("");
    let workdir = tempfile::tempdir().expect("workdir");
    let zip_path = workdir.path().join("loop.zip");

    // 1. project new — the loop's substrate.
    expect_ok(
        "project new",
        &ign(&config, &env, &["project", "new", &name, "--compact"]),
    );

    // 2. project list contains it.
    let out = ign(&config, &env, &["project", "list", "--compact"]);
    expect_ok("project list", &out);
    let listing = data_envelope(&out);
    let listed = listing["data"]["projects"]
        .as_array()
        .expect("projects array")
        .iter()
        .any(|project| project["name"] == Value::String(name.clone()));
    assert!(listed, "the new project appears in `project list`");

    // 3. resource list on the FRESH project is EMPTY (all-keys
    // always: resources [], zero members under the resource roots —
    // the surgery list over a real export).
    let out = ign(&config, &env, &["resource", "list", &name, "--compact"]);
    expect_ok("resource list (fresh)", &out);
    let fresh = data_envelope(&out)["data"].clone();
    assert_eq!(
        fresh["resources"],
        Value::Array(vec![]),
        "a brand-new project exports zero resource members: {fresh}"
    );

    // 4. resource put a scratch script (--yes: the put re-imports
    // the whole project — the 05-02 guarded-verb set).
    let scratch_body = format!(
        r#"{{"scope":"G","code":"print('{}')", "e2e": true}}"#,
        name.replace('\'', "")
    );
    let scratch_file = workdir.path().join("scratch.json");
    std::fs::write(&scratch_file, &scratch_body).expect("write scratch body");
    expect_ok(
        "resource put scratch",
        &ign(
            &config,
            &env,
            &[
                "resource",
                "put",
                &name,
                SCRATCH_PATH,
                "--file",
                scratch_file.to_str().unwrap(),
                "--yes",
                "--compact",
            ],
        ),
    );

    // 5. resource get verifies the round-trip (content survives the
    // surgery write path byte-honestly — sniffed json, same code).
    let out = ign(
        &config,
        &env,
        &["resource", "get", &name, SCRATCH_PATH, "--compact"],
    );
    expect_ok("resource get scratch", &out);
    let got = data_envelope(&out)["data"].clone();
    assert_eq!(got["content_kind"], Value::String("json".into()));
    assert_eq!(
        got["content"]["e2e"],
        Value::Bool(true),
        "the written content reads back: {got}"
    );

    // 6. resource list now CONTAINS the scratch path (the surgery
    // list sees the member the put injected).
    let out = ign(&config, &env, &["resource", "list", &name, "--compact"]);
    expect_ok("resource list (populated)", &out);
    let populated = data_envelope(&out)["data"].clone();
    assert!(
        populated["resources"]
            .as_array()
            .expect("resources array")
            .iter()
            .any(|entry| entry["path"] == Value::String(SCRATCH_PATH.into())),
        "the put member appears in the surgery list: {populated}"
    );

    // 7. TWO-SIDED PUT HONESTY, first half: a get BEFORE the second
    // put witnesses the OLD content (get rides the same export the
    // put's surgery will operate on).
    let out = ign(
        &config,
        &env,
        &["resource", "get", &name, SCRATCH_PATH, "--compact"],
    );
    expect_ok("resource get scratch (pre-second-put)", &out);
    let old_content = data_envelope(&out)["data"]["content"].clone();
    assert_eq!(
        old_content["e2e"],
        Value::Bool(true),
        "the export BEFORE the second put carries the member with OLD content: {old_content}"
    );

    // 8. export AFTER the first put — the ZIP must CONTAIN scratch
    // (this is also the replace-not-merge substrate below).
    let out = ign(
        &config,
        &env,
        &[
            "project",
            "export",
            &name,
            "-o",
            zip_path.to_str().unwrap(),
            "--compact",
        ],
    );
    expect_ok("project export", &out);
    let export = data_envelope(&out)["data"].clone();
    assert!(zip_path.exists(), "the export file landed");
    assert!(export["bytes"].as_u64().expect("bytes") > 0);
    assert!(
        export["scope"]["includes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "scripts"),
        "scope.includes carries scripts: {export}"
    );
    assert!(
        export["scope"]["excludes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "tags"),
        "scope.excludes carries tags: {export}"
    );

    // 9. TWO-SIDED PUT HONESTY, second half: put the scratch with
    // NEW content, then a get reads the NEW content back.
    let scratch_new = workdir.path().join("scratch-new.json");
    std::fs::write(
        &scratch_new,
        r#"{"scope":"G","code":"print('second-edition')", "e2e": true, "edited": true}"#,
    )
    .expect("write scratch-new body");
    expect_ok(
        "resource put scratch (new content)",
        &ign(
            &config,
            &env,
            &[
                "resource",
                "put",
                &name,
                SCRATCH_PATH,
                "--file",
                scratch_new.to_str().unwrap(),
                "--yes",
                "--compact",
            ],
        ),
    );
    let out = ign(
        &config,
        &env,
        &["resource", "get", &name, SCRATCH_PATH, "--compact"],
    );
    expect_ok("resource get scratch (post-second-put)", &out);
    let new_content = data_envelope(&out)["data"]["content"].clone();
    assert_eq!(
        new_content["edited"],
        Value::Bool(true),
        "the export AFTER the second put carries the member with NEW content: {new_content}"
    );

    // 10. put a SECOND scratch that exists ONLY on the gateway — the
    // ZIP on disk predates it.
    let scratch2_file = workdir.path().join("scratch2.json");
    std::fs::write(&scratch2_file, r#"{"scope":"G","code":"print('second')"}"#)
        .expect("write scratch2 body");
    expect_ok(
        "resource put scratch2",
        &ign(
            &config,
            &env,
            &[
                "resource",
                "put",
                &name,
                SCRATCH2_PATH,
                "--file",
                scratch2_file.to_str().unwrap(),
                "--yes",
                "--compact",
            ],
        ),
    );

    // 11. abort-policy import into the SAME name → project_exists,
    // BEFORE any upload.
    let abort = ign(
        &config,
        &env,
        &[
            "project",
            "import",
            &name,
            "--file",
            zip_path.to_str().unwrap(),
            "--compact",
        ],
    );
    expect_exit(&abort, 6, "project_exists", "abort import");

    // 12. overwrite import (guarded) → success.
    expect_ok(
        "overwrite import",
        &ign(
            &config,
            &env,
            &[
                "project",
                "import",
                &name,
                "--file",
                zip_path.to_str().unwrap(),
                "--collision-policy",
                "overwrite",
                "--yes",
                "--compact",
            ],
        ),
    );

    // 13. THE two-sided replace-not-merge pin (Pitfall 4):
    //     scratch SURVIVED with the OLD (in-ZIP) content — the
    //     overwrite import replaced the second put's edit…
    let out = ign(
        &config,
        &env,
        &["resource", "get", &name, SCRATCH_PATH, "--compact"],
    );
    expect_ok("resource get scratch post-import", &out);
    let survived = data_envelope(&out)["data"].clone();
    assert_eq!(
        survived["content"]["e2e"],
        Value::Bool(true),
        "the pre-export resource survived the overwrite import: {survived}"
    );
    assert_eq!(
        survived["content"]["edited"],
        Value::Null,
        "…and carries the IN-ZIP (old) content — the import replaced the later edit: {survived}"
    );
    //    …and scratch2 is GONE (absent from the ZIP → deleted).
    let wiped = ign(
        &config,
        &env,
        &["resource", "get", &name, SCRATCH2_PATH, "--compact"],
    );
    expect_exit(&wiped, 6, "not_found", "resource get scratch2 post-import");

    // 14. THE delete-surgery witness: `resource delete --yes` then a
    // get is `not_found` (remove_member → overwrite-import).
    expect_ok(
        "resource delete scratch",
        &ign(
            &config,
            &env,
            &[
                "resource",
                "delete",
                &name,
                SCRATCH_PATH,
                "--yes",
                "--compact",
            ],
        ),
    );
    let deleted = ign(
        &config,
        &env,
        &["resource", "get", &name, SCRATCH_PATH, "--compact"],
    );
    expect_exit(&deleted, 6, "not_found", "resource get scratch post-delete");

    // 15. rename + copy (the family's remaining verbs).
    let renamed = timestamped_name("-renamed");
    expect_ok(
        "project rename",
        &ign(
            &config,
            &env,
            &["project", "rename", &name, &renamed, "--compact"],
        ),
    );
    let copied = timestamped_name("-copy");
    expect_ok(
        "project copy",
        &ign(
            &config,
            &env,
            &["project", "copy", &renamed, &copied, "--compact"],
        ),
    );

    // 16. cleanup (best-effort by design — a failure above leaves
    // forensic state under the timestamped names).
    for candidate in [name.as_str(), renamed.as_str(), copied.as_str()] {
        let _ = ign(
            &config,
            &env,
            &["project", "delete", candidate, "--yes", "--compact"],
        )
        .status
        .success();
    }
}

/// The openapi-capture gate (03-RESEARCH Open Question 1): GET
/// `{base}/openapi.json` (authed) and write a trimmed extract of
/// every `/data/api/v1/(projects|scan|resources)` path into the phase
/// dir — the authoritative artifact that settles the resource-family
/// wire-truth question the moment a token exists (02's
/// `openapi-8.3.6-phase2-extract.json` is the precedent file). The
/// projects family is soft-asserted present; whether
/// `/projects/{project}/resources` appears is PRINTED (not asserted)
/// — a negative is the answer, not a failure.
#[tokio::test]
#[ignore = "opt-in: set IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN"]
async fn openapi_capture_writes_phase3_extract() {
    let Some(env) = live_env() else {
        skip("IGNITION_LIVE_URL / IGNITION_LIVE_TOKEN not both set");
        return;
    };

    let url = format!("{}/openapi.json", env.url.trim_end_matches('/'));
    let response = reqwest::Client::new()
        .get(&url)
        .header("X-Ignition-API-Token", &env.token)
        .send()
        .await
        .unwrap_or_else(|err| panic!("openapi fetch from {url}: {err}"));
    assert!(
        response.status().is_success(),
        "openapi fetch answered {} — check the token's permissions",
        response.status()
    );
    let spec: Value = response.json().await.expect("openapi.json parses as JSON");

    // The phase-3 families, verbatim from the spec's paths map.
    let families = [
        "/data/api/v1/projects",
        "/data/api/v1/scan",
        "/data/api/v1/resources",
    ];
    let mut trimmed = serde_json::Map::new();
    if let Some(paths) = spec["paths"].as_object() {
        for (path, operations) in paths {
            if families.iter().any(|prefix| path.starts_with(prefix)) {
                trimmed.insert(path.clone(), operations.clone());
            }
        }
    }

    // Soft-assert the projects family; PRINT the resource-endpoint
    // verdict (Open Question 1's answer, whatever it is).
    let projects_present = trimmed
        .keys()
        .any(|path| path.starts_with("/data/api/v1/projects"));
    assert!(
        projects_present,
        "the official openapi carries the projects family — a negative means the spec moved"
    );
    let resources_present = trimmed
        .keys()
        .any(|path| path.contains("/projects/{project}/resources") || path.contains("/resources"));
    println!(
        "project-resources family present in openapi.json: {resources_present} \
         (FALSE settles 03-RESEARCH Open Question 1 against ignition-mcp — \
         re-plan the resource family from the extract)"
    );

    let extract = serde_json::json!({
        "captured_from": url,
        "families": families,
        "path_count": trimmed.len(),
        "project_resources_present": resources_present,
        "info": spec["info"],
        "paths": Value::Object(trimmed),
    });
    let out: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(".planning/phases/03-project-operations/openapi-8.3.6-phase3-extract.json");
    std::fs::write(
        &out,
        serde_json::to_vec_pretty(&extract).expect("extract serializes"),
    )
    .unwrap_or_else(|err| panic!("write {}: {err}", out.display()));
    println!(
        "wrote {} ({} phase-3 paths)",
        out.display(),
        extract["path_count"]
    );
}
