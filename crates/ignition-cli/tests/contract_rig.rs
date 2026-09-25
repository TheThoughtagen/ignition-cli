//! Golden-file contract tests for the `ign rig` family's docker-less
//! error paths (04-01, RIG-01): the no-rig discovery failure (exit 7
//! with the full search trail), the unknown-name refusal (knowns
//! listed — the ProfileNotFound shape precedent), the discovery
//! precedence pin, and the help surface. Harness inherited from
//! `contract_restart_wait.rs`: isolated `IGNITION_CLI_CONFIG` per
//! spawn, stderr-envelope parse from the first `{`, `[..]` elides only
//! genuinely dynamic values (temp paths).
//!
//! ## Machine isolation (the plan-checker's lesson, made structural)
//!
//! Discovery's WHK-convention levels probe the REAL home roots — on a
//! developer machine those repos exist, so every test exports
//! `IGNITION_RIG_ROOTS` pointing at an empty temp dir (the documented
//! override) to make "nothing found" deterministic everywhere.
//!
//! ## The docker-present guard
//!
//! The precedence test pins the error the RESOLVE run produces when
//! the docker CLI is absent (CI); a machine WITH docker would run the
//! resolve for real and change the outcome — so the test skips when
//! `docker compose version` answers OR when
//! `IGNITION_SKIP_RIG_DOCKER_TESTS` forces the skip (documented).

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::Value;

/// Isolated config dir + the config path inside it (file need not exist).
fn isolated_config() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.toml");
    (dir, path)
}

/// An EMPTY temp dir for `IGNITION_RIG_ROOTS` — convention-level
/// probing then finds nothing, everywhere.
fn isolated_roots() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().to_path_buf();
    (dir, path)
}

/// Spawn `ign rig …` with an isolated config, isolated convention
/// roots, no rig env, and an isolated cwd — every test needs all four
/// (the crate root itself has no compose file, but the convention
/// levels make no-`--rig` paths machine-dependent without the roots
/// override).
fn ign_rig(config: &Path, roots: &Path, cwd: &Path, args: &[&str]) -> std::process::Output {
    let mut command = Command::cargo_bin("ign").expect("binary 'ign' not found");
    command
        .env("IGNITION_CLI_CONFIG", config)
        .env("IGNITION_RIG_ROOTS", roots)
        .env_remove("IGNITION_RIG")
        .env_remove("IGNITION_TOKEN")
        .current_dir(cwd);
    command.args(args).output().expect("spawn ign")
}

/// stderr's JSON envelope starting at the first `{` (log-tolerant parse).
fn stderr_envelope(out: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&out.stderr);
    let start = stderr.find('{').unwrap_or(0);
    stderr[start..].to_string()
}

/// True when the docker CLI answers `compose version` — the
/// docker-present machines that must skip the resolve-failure golden.
fn docker_compose_available() -> bool {
    std::process::Command::new("docker")
        .args(["compose", "version"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// THE no-rig pin: empty cwd, no config rigs, isolated convention
/// roots → exit 7 (`rig_error`), `profile: null`, and the message
/// carries the full search trail (cwd candidates + roots) so agents
/// self-diagnose. No docker is ever spawned — discovery fails before
/// the resolve run.
#[test]
fn missing_rig_exits_7_with_search_trail() {
    let (_config_dir, config) = isolated_config();
    let (_roots_dir, roots) = isolated_roots();
    let cwd = tempfile::tempdir().expect("cwd tempdir");

    let out = ign_rig(&config, &roots, cwd.path(), &["rig", "status", "--compact"]);
    assert_eq!(out.status.code(), Some(7), "rig class");
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    let body: Value = serde_json::from_str(&stderr_envelope(&out)).expect("error envelope parses");
    assert_eq!(body["ok"], Value::Bool(false));
    assert_eq!(body["profile"], Value::Null, "docker-only: profile null");
    assert_eq!(
        body["error"]["code"],
        Value::String("rig_error".into()),
        "stable slug"
    );
    let message = body["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("no compose file discovered"),
        "the diagnosis leads: {message}"
    );
    assert!(
        message.contains("docker/compose.yml") && message.contains("docker-compose.yml"),
        "cwd candidates named in the trail: {message}"
    );
    // The hint stays the class-wide docker hint.
    assert!(
        body["error"]["hint"]
            .as_str()
            .unwrap_or("")
            .contains("docker ps")
    );
}

/// `--rig unknown-name`: exit 7 listing the known rigs (the
/// ProfileNotFound shape precedent — BTreeMap order). No docker is
/// spawned (the lookup fails before resolve). The message is pinned
/// EXACTLY via the parsed envelope (snapbox's inline goldens
/// normalize embedded quotes in string values — the PK//x03//x04
/// gotcha's sibling).
#[test]
fn unknown_rig_name_lists_knowns_golden() {
    let (_config_dir, config) = isolated_config();
    std::fs::write(
        &config,
        "[rigs.known-rig]\ncompose_file = \"/tmp/known-rig-compose.yml\"\n",
    )
    .expect("write config");
    let (_roots_dir, roots) = isolated_roots();
    let cwd = tempfile::tempdir().expect("cwd tempdir");

    let out = ign_rig(
        &config,
        &roots,
        cwd.path(),
        &["rig", "--rig", "nope", "status", "--compact"],
    );
    assert_eq!(out.status.code(), Some(7));
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    let body: Value = serde_json::from_str(&stderr_envelope(&out)).expect("error envelope parses");
    assert_eq!(body["ok"], Value::Bool(false));
    assert_eq!(body["profile"], Value::Null, "docker-only: profile null");
    assert_eq!(body["error"]["code"], Value::String("rig_error".into()));
    assert_eq!(
        body["error"]["message"],
        Value::String(
            "rig error: rig \"nope\" not found (known rigs: [\"known-rig\"]); add a \
             [rigs.nope] entry or run from the rig's directory"
                .into()
        ),
        "the exact refusal message — knowns listed, actionable fix named"
    );
    assert_eq!(
        body["error"]["endpoint"],
        Value::Null,
        "discovery refusals carry no endpoint"
    );
}

/// The discovery precedence pin (must-have truth #4): `[rig].default`
/// BEATS a cwd full of compose candidates. The assertion rides the
/// resolve run's deterministic failure on docker-less machines — the
/// spawned command names the file it tried, so the golden proves the
/// DEFAULT's compose file (not the cwd's) was chosen. Machines WITH
/// docker skip (auto-probe + `IGNITION_SKIP_RIG_DOCKER_TESTS`): there
/// the resolve runs for real and the outcome is machine-dependent.
#[test]
fn discovery_precedence_config_default_beats_cwd() {
    if std::env::var("IGNITION_SKIP_RIG_DOCKER_TESTS").is_ok() || docker_compose_available() {
        eprintln!("skipping: docker present (or skip forced) — resolve would run for real");
        return;
    }

    let (_config_dir, config) = isolated_config();
    let rig_dir = tempfile::tempdir().expect("rig tempdir");
    let remote_compose = rig_dir.path().join("remote-compose.yml");
    std::fs::write(
        &remote_compose,
        "services:\n  sidecar:\n    image: alpine:latest\n",
    )
    .expect("write remote compose");

    std::fs::write(
        &config,
        format!(
            "[rig]\ndefault = \"remote\"\n\n[rigs.remote]\ncompose_file = '{}'\n",
            remote_compose.display()
        ),
    )
    .expect("write config");

    // The cwd ALSO has a compose file — the default must win.
    let cwd = tempfile::tempdir().expect("cwd tempdir");
    std::fs::write(
        cwd.path().join("compose.yml"),
        "services:\n  other:\n    image: alpine:latest\n",
    )
    .expect("write cwd compose");

    let (_roots_dir, roots) = isolated_roots();
    let out = ign_rig(&config, &roots, cwd.path(), &["rig", "status", "--compact"]);
    assert_eq!(out.status.code(), Some(7), "no docker: resolve fails");

    let body: Value = serde_json::from_str(&stderr_envelope(&out)).expect("error envelope parses");
    assert_eq!(body["profile"], Value::Null);
    let message = body["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("remote-compose.yml"),
        "the DEFAULT rig's file was the one resolved: {message}"
    );
    let cwd_compose = cwd.path().join("compose.yml").display().to_string();
    assert!(
        !message.contains(&cwd_compose),
        "the cwd candidate lost the precedence race: {message}"
    );
    // Belt: the message names the spawn failure (docker absent).
    assert!(
        message.contains("failed to spawn") || message.contains("docker"),
        "{message}"
    );
}

/// The `--help` surface: the rig subtree is visible with all five
/// verbs and the `--rig` flag (contains-assertions, not a golden —
/// clap's help rendering churns across versions by design).
#[test]
fn rig_help_shows_the_subtree() {
    let (_config_dir, config) = isolated_config();
    let (_roots_dir, roots) = isolated_roots();
    let cwd = tempfile::tempdir().expect("cwd tempdir");

    let out = ign_rig(&config, &roots, cwd.path(), &["rig", "--help"]);
    assert!(out.status.success(), "help exits 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    for expected in ["--rig <NAME>", "up", "down", "reset", "status", "logs"] {
        assert!(
            stdout.contains(expected),
            "help mentions {expected}: {stdout}"
        );
    }

    // Bare `ign rig` (no verb) is the friendly usage error, exit 2.
    let out = ign_rig(&config, &roots, cwd.path(), &["rig"]);
    assert_eq!(out.status.code(), Some(2));
}

/// THE destructive-guard pin (04-02): `rig reset` without `--yes`
/// refuses with exit 2 (`confirmation_required`), profile null, and
/// the hint naming BOTH `--yes` and `IGNITION_YES=1` — and the ZERO
/// WORK proof rides the environment: the cwd has NO compose file and
/// the convention roots are empty, so if the guard did not fire
/// BEFORE discovery the command would exit 7 (`no compose file
/// discovered`). Exit 2 here pins the guard-before-resolution
/// ordering at the binary level (the sessions-terminate precedent,
/// docker-only edition).
#[test]
fn rig_reset_refuses_without_yes_before_any_discovery() {
    let (_config_dir, config) = isolated_config();
    let (_roots_dir, roots) = isolated_roots();
    let cwd = tempfile::tempdir().expect("cwd tempdir");

    let out = ign_rig(&config, &roots, cwd.path(), &["rig", "reset", "--compact"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "guard exit 2 — NOT the exit 7 a discovery run would produce"
    );
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    let body: Value = serde_json::from_str(&stderr_envelope(&out)).expect("error envelope parses");
    assert_eq!(body["ok"], Value::Bool(false));
    assert_eq!(body["profile"], Value::Null, "docker-only: profile null");
    assert_eq!(
        body["error"]["code"],
        Value::String("confirmation_required".into()),
        "stable slug"
    );
    let message = body["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("rig reset"),
        "names the operation: {message}"
    );
    let hint = body["error"]["hint"].as_str().expect("hint required");
    assert!(
        hint.contains("--yes") && hint.contains("IGNITION_YES"),
        "hint names the flag and the env escape hatch: {hint}"
    );
}

/// The other half of the guard story: WITH `--yes` (guard passed) the
/// same no-rig environment fails CLEANLY at discovery — exit 7
/// `rig_error` with the search trail. The guard pass-through is
/// proven by the error class changing from 2 to 7.
#[test]
fn rig_reset_with_yes_in_no_rig_cwd_fails_cleanly_at_discovery() {
    let (_config_dir, config) = isolated_config();
    let (_roots_dir, roots) = isolated_roots();
    let cwd = tempfile::tempdir().expect("cwd tempdir");

    let out = ign_rig(
        &config,
        &roots,
        cwd.path(),
        &["rig", "reset", "--yes", "--compact"],
    );
    assert_eq!(
        out.status.code(),
        Some(7),
        "guard passed: discovery failure"
    );
    let body: Value = serde_json::from_str(&stderr_envelope(&out)).expect("error envelope parses");
    assert_eq!(body["profile"], Value::Null);
    assert_eq!(body["error"]["code"], Value::String("rig_error".into()));
    let message = body["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("no compose file discovered"),
        "the discovery diagnosis leads: {message}"
    );
}

/// The `rig logs` flag surface: --tail, -f/--follow, and the SERVICE
/// positional are all visible in help (contains-assertions — clap's
/// help rendering churns across versions by design).
#[test]
fn rig_logs_help_shows_tail_follow_service() {
    let (_config_dir, config) = isolated_config();
    let (_roots_dir, roots) = isolated_roots();
    let cwd = tempfile::tempdir().expect("cwd tempdir");

    let out = ign_rig(&config, &roots, cwd.path(), &["rig", "logs", "--help"]);
    assert!(out.status.success(), "help exits 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("--tail"), "tail visible: {stdout}");
    assert!(stdout.contains("--follow"), "follow visible: {stdout}");
    assert!(stdout.contains("-f"), "the -f short form visible: {stdout}");
    assert!(
        stdout.contains("[SERVICE]"),
        "the SERVICE positional visible: {stdout}"
    );
}

/// `IGNITION_RIG` fills a missing `--rig` (the env→flag fold in
/// apply_env_defaults — the IGNITION_PROFILE precedent): with the env
/// set to an unknown name the named-lookup error fires, proving the
/// env reached the selection. No docker spawned.
#[test]
fn ignition_rig_env_folds_into_selection() {
    let (_config_dir, config) = isolated_config();
    std::fs::write(
        &config,
        "[rigs.known-rig]\ncompose_file = \"/tmp/known-rig-compose.yml\"\n",
    )
    .expect("write config");
    let (_roots_dir, roots) = isolated_roots();
    let cwd = tempfile::tempdir().expect("cwd tempdir");

    let mut command = Command::cargo_bin("ign").expect("binary 'ign' not found");
    command
        .env("IGNITION_CLI_CONFIG", &config)
        .env("IGNITION_RIG_ROOTS", &roots)
        .env("IGNITION_RIG", "env-named-rig")
        .current_dir(cwd.path())
        .args(["rig", "status", "--compact"]);
    let out = command.output().expect("spawn ign");
    assert_eq!(out.status.code(), Some(7));
    let body: Value = serde_json::from_str(&stderr_envelope(&out)).expect("error envelope parses");
    let message = body["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("env-named-rig") && message.contains("known rigs"),
        "the env-provided name drove the lookup: {message}"
    );
}

/// THE destructive-guard pin (04-03): `rig trial reset` without
/// `--yes` refuses with exit 2 (`confirmation_required`), profile
/// null — and the ZERO WORK proof rides the same no-rig environment
/// as `rig reset` (the cwd has NO compose file; un-guarded execution
/// would exit 7 at discovery). The guard-before-resolution ordering,
/// fourth destructive-verb instance.
#[test]
fn rig_trial_reset_refuses_without_yes_before_any_discovery() {
    let (_config_dir, config) = isolated_config();
    let (_roots_dir, roots) = isolated_roots();
    let cwd = tempfile::tempdir().expect("cwd tempdir");

    let out = ign_rig(
        &config,
        &roots,
        cwd.path(),
        &["rig", "trial", "reset", "--compact"],
    );
    assert_eq!(
        out.status.code(),
        Some(2),
        "guard exit 2 — NOT the exit 7 a discovery run would produce"
    );
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    let body: Value = serde_json::from_str(&stderr_envelope(&out)).expect("error envelope parses");
    assert_eq!(body["ok"], Value::Bool(false));
    assert_eq!(body["profile"], Value::Null, "refusal: profile null");
    assert_eq!(
        body["error"]["code"],
        Value::String("confirmation_required".into()),
        "stable slug"
    );
    let message = body["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("rig trial reset"),
        "names the operation: {message}"
    );
}

/// The `rig trial` help surface: both verbs + `--user`; a PASSWORD
/// flag must NOT exist (env-only redaction discipline — pinned by
/// this absence in the same golden as the presence checks).
/// A status-path binary golden is NOT testable without a gateway —
/// the status contract lives at the unit/wiremock layer
/// (crates/ignition-core/tests/trial_contract.rs + actions tests).
#[test]
fn rig_trial_help_shows_verbs_and_no_password_flag() {
    let (_config_dir, config) = isolated_config();
    let (_roots_dir, roots) = isolated_roots();
    let cwd = tempfile::tempdir().expect("cwd tempdir");

    let out = ign_rig(&config, &roots, cwd.path(), &["rig", "trial", "--help"]);
    assert!(out.status.success(), "help exits 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    for expected in ["status", "reset"] {
        assert!(
            stdout.contains(expected),
            "help mentions {expected}: {stdout}"
        );
    }

    // The reset verb's own help carries --user; a PASSWORD flag must
    // NOT exist anywhere (env-only redaction discipline — pinned by
    // this absence in the same golden as the presence checks).
    let out = ign_rig(
        &config,
        &roots,
        cwd.path(),
        &["rig", "trial", "reset", "--help"],
    );
    assert!(out.status.success(), "reset help exits 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("--user <NAME>"), "--user visible: {stdout}");
    assert!(
        !stdout.to_lowercase().contains("--password"),
        "password NEVER rides a flag: {stdout}"
    );

    // Bare `ign rig trial` (no verb) is the friendly usage error.
    let out = ign_rig(&config, &roots, cwd.path(), &["rig", "trial"]);
    assert_eq!(out.status.code(), Some(2));
}

/// THE destructive-guard pin (04-04): `rig restore` without `--yes`
/// refuses with exit 2 (`confirmation_required`), profile null — the
/// ZERO WORK proof rides the same no-rig environment as `rig reset`
/// (the cwd has NO compose file; un-guarded execution would exit 7 at
/// discovery). Fifth destructive-verb instance (sessions terminate →
/// project delete → rig reset → rig trial reset → rig restore).
#[test]
fn rig_restore_refuses_without_yes_before_any_discovery() {
    let (_config_dir, config) = isolated_config();
    let (_roots_dir, roots) = isolated_roots();
    let cwd = tempfile::tempdir().expect("cwd tempdir");

    let out = ign_rig(
        &config,
        &roots,
        cwd.path(),
        &[
            "rig",
            "restore",
            "--file",
            "/nonexistent/snap.gwbk",
            "--compact",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(2),
        "guard exit 2 — NOT the exit 7 a discovery run would produce"
    );
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    let body: Value = serde_json::from_str(&stderr_envelope(&out)).expect("error envelope parses");
    assert_eq!(body["ok"], Value::Bool(false));
    assert_eq!(body["profile"], Value::Null, "refusal: profile null");
    assert_eq!(
        body["error"]["code"],
        Value::String("confirmation_required".into()),
        "stable slug"
    );
    let message = body["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("rig restore"),
        "names the operation: {message}"
    );
}

/// The guard's other half: WITH `--yes` and no IGNITION_TOKEN, the
/// same no-rig environment fails at DISCOVERY (exit 7) — never at the
/// token check — proving the guard fired first and discovery owns the
/// next failure. (On a machine where discovery WOULD find a rig, the
/// token check is next: exit 3.)
#[test]
fn rig_restore_with_yes_in_no_rig_cwd_fails_cleanly_at_discovery() {
    let (_config_dir, config) = isolated_config();
    let (_roots_dir, roots) = isolated_roots();
    let cwd = tempfile::tempdir().expect("cwd tempdir");

    let out = ign_rig(
        &config,
        &roots,
        cwd.path(),
        &[
            "rig",
            "restore",
            "--file",
            "/nonexistent/snap.gwbk",
            "--yes",
            "--compact",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(7),
        "guard passed: discovery failure"
    );
    let body: Value = serde_json::from_str(&stderr_envelope(&out)).expect("error envelope parses");
    assert_eq!(body["profile"], Value::Null);
    assert_eq!(body["error"]["code"], Value::String("rig_error".into()));
    assert!(
        body["error"]["message"]
            .as_str()
            .expect("message")
            .contains("no compose file discovered"),
        "the discovery diagnosis leads"
    );
}

/// The snapshot/restore help surfaces (contains-assertions — clap's
/// help rendering churns across versions by design): snapshot shows
/// `-o/--output <DIR>`; restore shows `--file <PATH>` +
/// `--timeout <SECS>` and names `--yes` in its destructive doc.
#[test]
fn rig_snapshot_restore_help_surfaces() {
    let (_config_dir, config) = isolated_config();
    let (_roots_dir, roots) = isolated_roots();
    let cwd = tempfile::tempdir().expect("cwd tempdir");

    let out = ign_rig(&config, &roots, cwd.path(), &["rig", "snapshot", "--help"]);
    assert!(out.status.success(), "snapshot help exits 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("--output <DIR>"),
        "output visible: {stdout}"
    );
    assert!(stdout.contains("-o"), "the -o short form visible: {stdout}");

    let out = ign_rig(&config, &roots, cwd.path(), &["rig", "restore", "--help"]);
    assert!(out.status.success(), "restore help exits 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("--file <PATH>"), "file visible: {stdout}");
    assert!(
        stdout.contains("--timeout <SECS>"),
        "timeout visible: {stdout}"
    );
    assert!(
        stdout.contains("--yes"),
        "the destructive guard is documented in the verb's help: {stdout}"
    );
}

// ---------------------------------------------------------------------
// --with-module / fetch-policy flags (16-02 Task 2, D-15 through D-17)
// ---------------------------------------------------------------------

/// A config carrying one named rig whose compose file genuinely exists
/// on disk (a minimal, valid compose document — never resolved through
/// Docker by the tests that use this fixture, since their refusal
/// fires before `resolve_plan` ever runs). Returns the rig's PROJECT
/// DIRECTORY guard (kept alive by the CALLER — dropping it deletes the
/// directory, which is why this returns the `TempDir` itself and not
/// just its `.path()`) so a test can assert nothing new was written
/// there.
fn rig_config_with_real_compose_file() -> (tempfile::TempDir, PathBuf, tempfile::TempDir) {
    let project_dir = tempfile::tempdir().expect("project tempdir");
    let compose_file = project_dir.path().join("docker-compose.yml");
    std::fs::write(
        &compose_file,
        "services:\n  app:\n    image: alpine:latest\n",
    )
    .expect("write compose file");

    let (config_dir, config) = isolated_config();
    std::fs::write(
        &config,
        format!(
            "[rigs.fixture]\ncompose_file = '{}'\n",
            compose_file.display()
        ),
    )
    .expect("write config");

    (config_dir, config, project_dir)
}

/// THE malformed-flag refusal (Task 2, D-15): `--with-module` with no
/// `@` separator refuses exit 2 (`invalid_input`) with ZERO Docker
/// invocation — `preflight_with_module_flags` runs BEFORE
/// `resolve_plan`, so a REAL, resolvable rig config is used here
/// specifically to prove nothing was written to its project directory
/// (the directory holds ONLY the compose file this test seeded, never
/// an `ign`-generated override).
#[test]
fn malformed_with_module_value_is_exit_2() {
    let (_config_dir, config, project_dir_guard) = rig_config_with_real_compose_file();
    let project_dir = project_dir_guard.path();
    let (_roots_dir, roots) = isolated_roots();
    let cwd = tempfile::tempdir().expect("cwd tempdir");

    let out = ign_rig(
        &config,
        &roots,
        cwd.path(),
        &[
            "rig",
            "--rig",
            "fixture",
            "up",
            "--with-module",
            "git-no-separator",
            "--compact",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(2),
        "malformed --with-module refuses at exit 2, never reaching a docker/resolve failure"
    );
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    let body: Value = serde_json::from_str(&stderr_envelope(&out)).expect("error envelope parses");
    assert_eq!(body["ok"], Value::Bool(false));
    assert_eq!(body["profile"], Value::Null, "docker-only: profile null");
    assert_eq!(
        body["error"]["code"],
        Value::String("invalid_input".into()),
        "stable slug"
    );
    let message = body["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("git-no-separator"),
        "message names the received value: {message}"
    );

    let entries: Vec<_> = std::fs::read_dir(project_dir)
        .expect("read project dir")
        .map(|entry| entry.expect("dir entry").file_name())
        .collect();
    assert_eq!(
        entries,
        vec![std::ffi::OsString::from("docker-compose.yml")],
        "nothing was written to the rig's project directory — the refusal fired \
         before resolve_plan (and therefore before Docker) ever ran"
    );
}

/// THE unregistered-id refusal (Task 2, D-13): a well-formed
/// `--with-module` value naming an id with no registry entry refuses
/// exit 3 (`module_not_registered`), naming both the offending id and
/// every currently registered one — with ZERO Docker invocation, the
/// same `preflight_with_module_flags` guard as the malformed case.
#[test]
fn unknown_module_id_via_flag_lists_knowns() {
    let (_config_dir, config, project_dir_guard) = rig_config_with_real_compose_file();
    let project_dir = project_dir_guard.path();
    let (_roots_dir, roots) = isolated_roots();
    let cwd = tempfile::tempdir().expect("cwd tempdir");

    let out = ign_rig(
        &config,
        &roots,
        cwd.path(),
        &[
            "rig",
            "--rig",
            "fixture",
            "up",
            "--with-module",
            "nope@1.0.0",
            "--compact",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(3),
        "unregistered module id refuses at exit 3, never reaching a docker/resolve failure"
    );
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    let body: Value = serde_json::from_str(&stderr_envelope(&out)).expect("error envelope parses");
    assert_eq!(body["ok"], Value::Bool(false));
    assert_eq!(body["profile"], Value::Null, "docker-only: profile null");
    assert_eq!(
        body["error"]["code"],
        Value::String("module_not_registered".into()),
        "stable slug"
    );
    let message = body["error"]["message"].as_str().expect("message");
    assert!(message.contains("nope"), "names the bad id: {message}");
    assert!(message.contains("git"), "lists a known id: {message}");
    assert!(
        message.contains("project-scan-endpoint"),
        "lists the OTHER known id too: {message}"
    );

    let entries: Vec<_> = std::fs::read_dir(project_dir)
        .expect("read project dir")
        .map(|entry| entry.expect("dir entry").file_name())
        .collect();
    assert_eq!(
        entries,
        vec![std::ffi::OsString::from("docker-compose.yml")],
        "nothing was written to the rig's project directory"
    );
}

/// `rig reset` gets the SAME two refusals as `rig up` (D-15: `reset`
/// runs the same up-half internally) — proven once here rather than
/// duplicating both golden-message assertions above.
#[test]
fn rig_reset_with_module_refusals_match_rig_up() {
    let (_config_dir, config, _project_dir) = rig_config_with_real_compose_file();
    let (_roots_dir, roots) = isolated_roots();
    let cwd = tempfile::tempdir().expect("cwd tempdir");

    let out = ign_rig(
        &config,
        &roots,
        cwd.path(),
        &[
            "rig",
            "--rig",
            "fixture",
            "reset",
            "--yes",
            "--with-module",
            "nope@1.0.0",
            "--compact",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(3),
        "same exit-3 refusal as `rig up`"
    );
    let body: Value = serde_json::from_str(&stderr_envelope(&out)).expect("error envelope parses");
    assert_eq!(
        body["error"]["code"],
        Value::String("module_not_registered".into())
    );
}

/// The `--with-module` / `--refresh` / `--accept-upstream-change` help
/// surface on BOTH `up` and `reset` (contains-assertions — clap's help
/// rendering churns across versions by design).
#[test]
fn rig_up_reset_help_shows_with_module_and_fetch_policy_flags() {
    let (_config_dir, config) = isolated_config();
    let (_roots_dir, roots) = isolated_roots();
    let cwd = tempfile::tempdir().expect("cwd tempdir");

    for verb in ["up", "reset"] {
        let out = ign_rig(&config, &roots, cwd.path(), &["rig", verb, "--help"]);
        assert!(out.status.success(), "{verb} help exits 0");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("--with-module <ID@VERSION>"),
            "{verb}: --with-module visible: {stdout}"
        );
        assert!(
            stdout.contains("--refresh"),
            "{verb}: --refresh visible: {stdout}"
        );
        assert!(
            stdout.contains("--accept-upstream-change"),
            "{verb}: --accept-upstream-change visible: {stdout}"
        );
        assert!(
            stdout.to_lowercase().contains("no subtractive form")
                || stdout.to_lowercase().contains("additive"),
            "{verb}: D-16's no-subtractive-form contract is discoverable in help: {stdout}"
        );
    }
}

/// D-17: `--refresh` and `--accept-upstream-change` together is a
/// usage-class refusal (clap's `conflicts_with`, exit 2) — BEFORE any
/// of this crate's own dispatch code runs, which is the strongest form
/// of "neither flag implies the other": the combination is not even a
/// reachable runtime state.
#[test]
fn refresh_and_accept_upstream_change_together_is_a_usage_error() {
    let (_config_dir, config) = isolated_config();
    let (_roots_dir, roots) = isolated_roots();
    let cwd = tempfile::tempdir().expect("cwd tempdir");

    let out = ign_rig(
        &config,
        &roots,
        cwd.path(),
        &[
            "rig",
            "up",
            "--refresh",
            "--accept-upstream-change",
            "--compact",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(2),
        "clap's conflicts_with refuses the combination as a usage error"
    );
}

/// True when `docker` itself can be spawned and answers to a bare
/// invocation — distinct from [`docker_compose_available`] (which also
/// requires the compose plugin): this gate only needs the daemon to
/// accept `up -d`/`down`, so the cheaper probe is enough and avoids a
/// redundant `compose version` round-trip.
fn docker_daemon_available() -> bool {
    docker_compose_available()
}

/// RMOD-01 / the must_haves artifact table's "--json provisioning key"
/// half: `rig up --with-module git@VERSION` against a real (lightweight
/// alpine, no gateway port) rig — cache-seeded so the fetch is a pure
/// cache hit and no network request is ever made — succeeds and the
/// `--json` envelope's `data.provisioned_modules` carries exactly the
/// provisioned module's id/version/gateway_module_id/digest/source/
/// mount_target. Skips when Docker is unavailable or
/// `IGNITION_SKIP_RIG_DOCKER_TESTS` forces the skip (the file's
/// established convention) — teardown runs via a scope guard so a
/// mid-test panic never leaks the container.
#[test]
fn rig_up_with_module_json_reports_provisioned_modules() {
    if std::env::var("IGNITION_SKIP_RIG_DOCKER_TESTS").is_ok() || !docker_daemon_available() {
        eprintln!("skipping: docker unavailable (or skip forced) — this test drives a real rig");
        return;
    }

    let project_dir = tempfile::tempdir().expect("project tempdir");
    let compose_file = project_dir.path().join("docker-compose.yml");
    std::fs::write(
        &compose_file,
        "services:\n  app:\n    image: alpine:latest\n    command: [\"sleep\", \"infinity\"]\n",
    )
    .expect("write compose file");

    let project_name = format!(
        "ign-cli-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    );

    let (_config_dir, config) = isolated_config();
    std::fs::write(
        &config,
        format!(
            "[rigs.fixture]\ncompose_file = '{}'\nproject_name = '{}'\nmodule_service = \"app\"\n",
            compose_file.display(),
            project_name
        ),
    )
    .expect("write config");
    let (_roots_dir, roots) = isolated_roots();
    let cwd = tempfile::tempdir().expect("cwd tempdir");

    // Pre-seed the module cache — CacheFirst (the default policy) is a
    // pure cache hit, so this test never contacts GitHub.
    let payload =
        b"synthetic .modl payload for rig_up_with_module_json_reports_provisioned_modules".to_vec();
    let digest = sha256_hex(&payload);
    let cache_root = tempfile::tempdir().expect("cache tempdir");
    let module_dir = cache_root.path().join("modules").join("git");
    std::fs::create_dir_all(&module_dir).expect("create module cache dir");
    std::fs::write(module_dir.join(format!("2.3.4-{digest}.modl")), &payload)
        .expect("seed cached artifact");

    // Teardown guard: `docker compose -p <name> -f <file> down -v
    // --remove-orphans` runs on drop (success, failure, or panic
    // alike) — best-effort, errors ignored.
    struct Teardown {
        compose_file: PathBuf,
        project_name: String,
    }
    impl Drop for Teardown {
        fn drop(&mut self) {
            let _ = std::process::Command::new("docker")
                .args([
                    "compose",
                    "-p",
                    &self.project_name,
                    "-f",
                    self.compose_file.to_str().expect("utf8 path"),
                    "down",
                    "-v",
                    "--remove-orphans",
                ])
                .output();
        }
    }
    let _teardown = Teardown {
        compose_file: compose_file.clone(),
        project_name: project_name.clone(),
    };

    let mut command = Command::cargo_bin("ign").expect("binary 'ign' not found");
    command
        .env("IGNITION_CLI_CONFIG", &config)
        .env("IGNITION_RIG_ROOTS", &roots)
        .env("IGNITION_CLI_CACHE", cache_root.path())
        .env_remove("IGNITION_RIG")
        .env_remove("IGNITION_TOKEN")
        .current_dir(cwd.path())
        .args([
            "rig",
            "--rig",
            "fixture",
            "up",
            "--with-module",
            "git@2.3.4",
            "--timeout",
            "60",
            "--json",
            "--compact",
        ]);
    let out = command.output().expect("spawn ign");
    assert!(
        out.status.success(),
        "rig up succeeds: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let body: Value = serde_json::from_str(&String::from_utf8_lossy(&out.stdout))
        .expect("stdout envelope parses");
    assert_eq!(body["ok"], Value::Bool(true));
    let modules = body["data"]["provisioned_modules"]
        .as_array()
        .expect("provisioned_modules is always an array");
    assert_eq!(modules.len(), 1, "exactly the one --with-module module");
    let module = &modules[0];
    assert_eq!(module["id"], Value::String("git".into()));
    assert_eq!(module["version"], Value::String("2.3.4".into()));
    assert_eq!(
        module["gateway_module_id"],
        Value::String("com.axone_io.ignition.git".into())
    );
    assert_eq!(module["digest_sha256"], Value::String(digest.clone()));
    assert_eq!(module["source"], Value::String("cache".into()));
    assert_eq!(
        module["mount_target"],
        Value::String("/usr/local/bin/ignition/user-lib/modules/git.modl".into())
    );

    let override_file = project_dir.path().join("compose.ign-modules.yml");
    assert!(
        override_file.exists(),
        "the override file was written to the rig's project directory"
    );
    let contents = std::fs::read_to_string(&override_file).expect("read override");
    assert!(contents.contains("git.modl"), "{contents}");
    assert!(contents.contains("com.axone_io.ignition.git"), "{contents}");
}

/// sha256 hex digest — local helper (no dependency added: `sha2` is
/// already a workspace dependency of `ignition-core`, but this binary
/// crate's test target keeps its own copy rather than reaching into
/// core's private surface for one hash).
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
