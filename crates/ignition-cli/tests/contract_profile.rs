//! Golden-file contract tests for `profile add/list/use` (CORE-01/CORE-02)
//! — born contract-complete through the 01-02 envelope.
//!
//! Harness inherited from `contract_version.rs`: every spawn sets
//! `IGNITION_CLI_CONFIG` (research Pitfall 3: `directories` ignores XDG on
//! macOS); snapbox inline goldens via `stdout_for_golden` (strip println's
//! single trailing newline — snapbox `str![]` trims both ends); `[..]`
//! elides dynamic values. Golden-update workflow: `SNAPSHOTS=overwrite
//! cargo test`, then review the diff.
//!
//! All three render modes are golden-tested for `profile list` (Pitfall 6).

use std::path::Path;

use assert_cmd::Command;
use serde_json::Value;

const CANARY: &str = "CANARY-t0k3n-zzz";

/// Isolated config dir + the config path inside it (file need not exist).
fn isolated_config() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.toml");
    (dir, path)
}

/// Spawn `ign` with an isolated config path, extra env vars, and args.
fn ign(config: &Path, envs: &[(&str, &str)], args: &[&str]) -> std::process::Output {
    let mut command = Command::cargo_bin("ign").expect("binary 'ign' not found");
    command.env("IGNITION_CLI_CONFIG", config);
    for (key, value) in envs {
        command.env(key, value);
    }
    command.args(args).output().expect("spawn ign")
}

/// stdout as UTF-8 minus the single trailing newline `println!` appends —
/// inline `str![]` goldens live without it.
fn stdout_for_golden(out: &std::process::Output) -> &str {
    let stdout = std::str::from_utf8(&out.stdout).expect("utf-8 stdout");
    stdout.strip_suffix('\n').unwrap_or(stdout)
}

/// Add the two fixture profiles: dev (labeled, token_env, active) and prod
/// (unlabeled, keyring).
fn add_fixture_profiles(config: &Path) {
    let out = ign(
        config,
        &[],
        &[
            "profile",
            "add",
            "dev",
            "http://localhost:9088",
            "--label",
            "Dev rig",
            "--token-env",
            "IGNITION_TOKEN",
            "--active",
        ],
    );
    assert!(
        out.status.success(),
        "add dev failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = ign(
        config,
        &[],
        &[
            "profile",
            "add",
            "prod",
            "https://gw.example.com:8443",
            "--keyring",
            "profile:prod",
        ],
    );
    assert!(
        out.status.success(),
        "add prod failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// CORE-01 golden lifecycle: two profiles (dev labeled, prod not) →
/// `profile list` goldens in ALL THREE render modes. The human golden pins
/// the first stdout line as exactly `[profile: dev]` and the label fallback
/// (prod shows its name); the JSON goldens prove `label` is OMITTED when
/// unset (skip_serializing_if) and that rows are name-sorted.
#[test]
fn profile_lifecycle_golden() {
    let (_dir, config) = isolated_config();
    add_fixture_profiles(&config);

    // Pretty JSON.
    let out = ign(&config, &[], &["profile", "list", "--json"]);
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
    "active": "dev",
    "profiles": [
      {
        "name": "dev",
        "label": "Dev rig",
        "url": "http://localhost:9088/",
        "auth_kind": "token_env"
      },
      {
        "name": "prod",
        "url": "https://gw.example.com:8443/",
        "auth_kind": "keyring"
      }
    ]
  }
}
"#]],
    );

    // Human: header line first (exact literal), label falls back to name.
    let out = ign(&config, &[], &["profile", "list"]);
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
dev  Dev rig  http://localhost:9088/  token_env
prod  prod  https://gw.example.com:8443/  keyring
"#]],
    );

    // Compact: one line, same field set.
    let out = ign(&config, &[], &["profile", "list", "--compact"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![r#"{"ok":true,"profile":"dev","data":{"active":"dev","profiles":[{"name":"dev","label":"Dev rig","url":"http://localhost:9088/","auth_kind":"token_env"},{"name":"prod","url":"https://gw.example.com:8443/","auth_kind":"keyring"}]}}"#],
    );
}

/// CORE-01 switch truth: `profile use prod` changes the active profile and
/// every subsequent envelope echoes it.
#[test]
fn profile_use_switches_active() {
    let (_dir, config) = isolated_config();
    add_fixture_profiles(&config);

    let out = ign(&config, &[], &["profile", "use", "prod", "--json"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let use_body: Value = serde_json::from_slice(&out.stdout).expect("use envelope parses");
    assert_eq!(use_body["profile"], Value::String("prod".into()));

    let out = ign(&config, &[], &["profile", "list", "--json"]);
    let list_body: Value = serde_json::from_slice(&out.stdout).expect("list envelope parses");
    assert_eq!(list_body["profile"], Value::String("prod".into()));
    assert_eq!(list_body["data"]["active"], Value::String("prod".into()));
}

/// `IGNITION_PROFILE` selects the profile; an explicit `--profile` flag
/// beats the env (LOCKED precedence).
#[test]
fn env_profile_selection() {
    let (_dir, config) = isolated_config();
    add_fixture_profiles(&config);

    let out = ign(
        &config,
        &[("IGNITION_PROFILE", "prod")],
        &["profile", "list", "--json"],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: Value = serde_json::from_slice(&out.stdout).expect("envelope parses");
    assert_eq!(body["profile"], Value::String("prod".into()));

    let out = ign(
        &config,
        &[("IGNITION_PROFILE", "prod")],
        &["--profile", "dev", "profile", "list", "--json"],
    );
    let body: Value = serde_json::from_slice(&out.stdout).expect("envelope parses");
    assert_eq!(
        body["profile"],
        Value::String("dev".into()),
        "flag beats env"
    );
}

/// Unknown profile: exit 3 with the `profile_not_found` slug on STDERR
/// (stdout untouched), hint naming the known profiles (CORE-05).
#[test]
fn unknown_profile_exit_3_golden() {
    let (_dir, config) = isolated_config();
    add_fixture_profiles(&config);

    let out = ign(
        &config,
        &[],
        &["--profile", "nope", "profile", "list", "--json"],
    );
    assert_eq!(out.status.code(), Some(3), "config error class");
    assert!(out.stdout.is_empty(), "errors never touch stdout");

    let body: Value = serde_json::from_slice(&out.stderr).expect("stderr envelope parses");
    assert_eq!(body["ok"], Value::Bool(false));
    assert_eq!(body["profile"], Value::Null);
    assert_eq!(
        body["error"]["code"],
        Value::String("profile_not_found".into())
    );
    let hint = body["error"]["hint"]
        .as_str()
        .expect("hint is a string (CORE-05)");
    assert!(
        hint.contains("dev") && hint.contains("prod"),
        "hint names knowns: {hint}"
    );
}

/// CORE-02 acceptance: a canary token in the environment NEVER appears in
/// stdout or stderr of any command — including `--verbose` and `-vv`
/// (tracing runs at debug/trace). Sanity: the canary IS in the environment,
/// so the test could have failed.
#[test]
fn secret_redaction_canary() {
    let (_dir, config) = isolated_config();
    let out = ign(
        &config,
        &[],
        &[
            "profile",
            "add",
            "dev",
            "http://localhost:9088",
            "--token-env",
            "IGNITION_TOKEN",
            "--active",
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Sanity: the canary really is set in this process's env (the child
    // inherits + overrides the same value via `.env`).
    // SAFETY: no other test in this binary reads or writes IGNITION_TOKEN
    // in-process; the child gets its own copy via `.env`.
    unsafe { std::env::set_var("IGNITION_TOKEN", CANARY) };
    assert_eq!(std::env::var("IGNITION_TOKEN").expect("canary set"), CANARY);

    for verbosity in ["--verbose", "-vv"] {
        let out = ign(
            &config,
            &[("IGNITION_TOKEN", CANARY)],
            &["profile", "list", "--json", verbosity],
        );
        assert!(
            out.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !stdout.contains(CANARY),
            "canary leaked to stdout at {verbosity}: {stdout}"
        );
        assert!(
            !stderr.contains(CANARY),
            "canary leaked to stderr at {verbosity}: {stderr}"
        );
    }

    // SAFETY: restore for subsequent runs of the test binary.
    unsafe { std::env::remove_var("IGNITION_TOKEN") };
}

/// Fresh-install truth: with NO config file, `version` exits 0 and the
/// envelope echoes `"profile": null` (CORE-01 tolerates nothing-resolved).
#[test]
fn no_config_version_exit_0() {
    let (_dir, config) = isolated_config();
    assert!(!config.exists(), "fixture sanity: no config file");

    let out = ign(&config, &[], &["version", "--json"]);
    assert!(
        out.status.success(),
        "version must work on a fresh install; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: Value = serde_json::from_slice(&out.stdout).expect("envelope parses");
    assert_eq!(body["ok"], Value::Bool(true));
    assert_eq!(body["profile"], Value::Null);
}
