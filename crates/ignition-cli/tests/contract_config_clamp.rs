//! Binary contract for the sub-second clamp (08-01, TUIX-05): a config
//! whose profile carries `poll_interval_secs = 0` is refused at load
//! time with exit 3 and the additive `poll_interval_too_small` slug on
//! the stderr envelope. Assert-based (no golden pin — this slug surface
//! is new and later additive slugs must not bump it).
//!
//! Harness inherited from `contract_profile.rs`: every spawn sets
//! `IGNITION_CLI_CONFIG` (research Pitfall 3: `directories` ignores XDG
//! on macOS).

use std::path::Path;

use assert_cmd::Command;

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

/// `poll_interval_secs = 0` → exit 3, stderr envelope carries the
/// `poll_interval_too_small` slug, the message names the rule, and the
/// hint points at the profile key. Exit-1-to-7 taxonomy otherwise
/// untouched (the enumerated unit test pins the rest).
#[test]
fn poll_interval_zero_refused_exit_3_with_slug() {
    let (_dir, config) = isolated_config();
    std::fs::write(
        &config,
        r#"
[profiles.dev]
url = "http://localhost:9088/"
poll_interval_secs = 0
"#,
    )
    .expect("write config");

    let out = ign(&config, &[], &["profile", "list", "--json"]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "config class refusal must exit 3: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let envelope: serde_json::Value =
        serde_json::from_slice(&out.stderr).expect("stderr must carry the JSON error envelope");
    assert_eq!(envelope["ok"], serde_json::Value::Bool(false));
    assert_eq!(
        envelope["error"]["code"],
        serde_json::Value::String("poll_interval_too_small".into()),
        "envelope: {envelope}"
    );
    let message = envelope["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("dev") && message.contains("sub-second"),
        "message must name the profile + the rule: {message}"
    );
    let hint = envelope["error"]["hint"].as_str().expect("hint");
    assert!(
        hint.contains("[profiles.dev]") && hint.contains("poll_interval_secs"),
        "hint must point at the offending profile key: {hint}"
    );
}

/// Control: the floor is 1 — `poll_interval_secs = 1` loads and `ign
/// profile list` succeeds locally (no gateway contact; the command is
/// config-only).
#[test]
fn poll_interval_one_passes_and_list_succeeds() {
    let (_dir, config) = isolated_config();
    std::fs::write(
        &config,
        r#"
[profiles.dev]
url = "http://localhost:9088/"
poll_interval_secs = 1
"#,
    )
    .expect("write config");

    let out = ign(&config, &[], &["profile", "list", "--json"]);
    assert!(
        out.status.success(),
        "1 is the floor — must pass the clamp: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout must carry the success envelope");
    assert_eq!(envelope["ok"], serde_json::Value::Bool(true));
}
