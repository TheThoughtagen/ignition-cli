//! Golden-file contract tests for the version output envelope (the Phase-1
//! API freeze). Every subcommand added in later plans inherits this harness.
//!
//! Golden-update workflow: run `SNAPSHOTS=overwrite cargo test` and review
//! the source diff before committing — never hand-edit expected strings to
//! force a pass, and never bake dynamic values into goldens (the version
//! string uses a `[..]` elision).
//!
//! Every spawn sets `IGNITION_CLI_CONFIG` to a tempfile (research Pitfall 3:
//! `directories` ignores XDG on macOS — tests must never touch the real
//! config dir), even though `version` does not read config yet.

use assert_cmd::Command;
use serde_json::Value;

fn ign_with_isolated_config(args: &[&str]) -> std::process::Output {
    let cfg = tempfile::NamedTempFile::new().expect("tempfile");
    Command::cargo_bin("ign")
        .expect("binary 'ign' not found")
        .env("IGNITION_CLI_CONFIG", cfg.path())
        .args(args)
        .output()
        .expect("spawn ign")
}

/// stdout as UTF-8 minus the single trailing newline `println!` appends —
/// inline `str![]` goldens live without it (the macro trims leading/trailing
/// newlines from the literal).
fn stdout_for_golden(out: &std::process::Output) -> &str {
    let stdout = std::str::from_utf8(&out.stdout).expect("utf-8 stdout");
    stdout.strip_suffix('\n').unwrap_or(stdout)
}

/// LOCKED envelope shape: `ign version --json` prints exactly
/// `{ok, profile, data}` in that order, pretty-formatted, on stdout.
#[test]
fn version_json_envelope_shape() {
    let out = ign_with_isolated_config(&["version", "--json"]);
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
  "profile": null,
  "data": {
    "cli_version": "[..]"
  }
}
"#]],
    );
}

/// `--compact` renders one line that parses as JSON with the same field set.
#[test]
fn version_compact_one_line() {
    let out = ign_with_isolated_config(&["version", "--compact"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.trim_end().contains('\n'),
        "compact output must be a single line: {stdout}"
    );

    let value: Value = serde_json::from_str(&stdout).expect("compact output parses as JSON");
    let mut keys: Vec<&str> = value
        .as_object()
        .expect("envelope is a JSON object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["data", "ok", "profile"], "exact top-level field set");
    assert_eq!(value["ok"], Value::Bool(true));
    assert_eq!(value["profile"], Value::Null);
    assert_eq!(
        value["data"]["cli_version"],
        Value::String(env!("CARGO_PKG_VERSION").to_string())
    );
}

/// LOCKED precedence (Pitfall 6): `--compact` WITHOUT `--json` still yields
/// the JSON envelope — one-line form.
#[test]
fn version_compact_implies_json() {
    let out = ign_with_isolated_config(&["version", "--compact"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![r#"{"ok":true,"profile":null,"data":{"cli_version":"[..]"}}"#],
    );
}

/// Usage errors exit 2 from clap's own renderer; stderr is NOT required to
/// be JSON (clap cannot know about --json when parsing itself failed — the
/// documented exception). stdout stays empty.
#[test]
fn usage_error_exit_2_from_clap() {
    let out = ign_with_isolated_config(&["--bogus"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty(), "usage errors never touch stdout");
    assert!(!out.stderr.is_empty(), "clap renders the usage error");
}

/// Default mode is human-readable, never JSON — guards mode separation so
/// agents can rely on: JSON only when --json/--compact asked for it.
#[test]
fn human_mode_is_not_json() {
    let out = ign_with_isolated_config(&["version"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        serde_json::from_str::<Value>(stdout.trim_end()).is_err(),
        "human mode must not parse as JSON: {stdout}"
    );
}

/// With a config present, the envelope's `profile` echo is REAL (01-03
/// threads the resolved name through) — goldens change value, never shape.
/// Since 01-04 a resolved profile triggers the gateway check: this profile
/// points at a dead loopback port (nothing listens on port 1) so the check
/// degrades deterministically to the LOCKED exit-0 warning inside `data`
/// regardless of what runs on the developer's machine. The same command
/// with no config keeps `"profile": null` (covered by
/// `version_json_envelope_shape` above and `no_config_version_exit_0` in
/// `contract_profile.rs`).
#[test]
fn version_json_envelope_with_config() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("config.toml");
    std::fs::write(
        &config,
        r#"
active = "dev"

[profiles.dev]
url = "http://127.0.0.1:1/"
"#,
    )
    .expect("write config");

    let out = Command::cargo_bin("ign")
        .expect("binary 'ign' not found")
        .env("IGNITION_CLI_CONFIG", &config)
        .args(["version", "--json"])
        .output()
        .expect("spawn ign");
    assert!(
        out.status.success(),
        "unreachable gateway must exit 0 (LOCKED); stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
{
  "ok": true,
  "profile": "dev",
  "data": {
    "cli_version": "[..]",
    "warnings": [
      "gateway unreachable: http://127.0.0.1:1/data/api/v1/gateway-info"
    ]
  }
}
"#]],
    );
}
