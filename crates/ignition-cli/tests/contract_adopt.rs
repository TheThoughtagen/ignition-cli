//! Offline CLI contract for `ign adopt`'s write-scope flags (G-25-11,
//! plan 25-24). No gateway is involved: the profile points at a port nothing
//! listens on, so a pass proves the refusal happens BEFORE any login.
//!
//! - `--read-only` conflicts with `--write-level` and `--restrict-write`
//!   (clap usage error, exit 2).
//! - `--write-level Authenticated` and `--write-level public` are refused as
//!   `invalid_input` (exit 2) because a bare Authenticated or Public write
//!   level would let every API key write.

mod common;

use std::time::{Duration, Instant};

use common::{ign, stderr_json};

/// `ign` with a one-profile config pointing at a dead port and the password
/// the adopt dispatch demands before the action runs.
fn adopt_ign() -> (tempfile::TempDir, assert_cmd::Command) {
    let (dir, mut cmd) = ign();
    std::fs::write(
        dir.path().join("config.toml"),
        "active = \"dev\"\n\n[profiles.dev]\nurl = \"http://127.0.0.1:9\"\n",
    )
    .expect("write config");
    cmd.env("IGNITION_PASSWORD", "x");
    (dir, cmd)
}

#[test]
fn read_only_conflicts_with_write_level() {
    let (_dir, mut cmd) = adopt_ign();
    let out = cmd
        .args(["adopt", "--read-only", "--write-level", "Authenticated/X"])
        .output()
        .expect("spawn ign");
    assert_eq!(out.status.code(), Some(2), "clap usage error");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--read-only"), "{stderr}");
}

#[test]
fn read_only_conflicts_with_restrict_write() {
    let (_dir, mut cmd) = adopt_ign();
    let out = cmd
        .args(["adopt", "--read-only", "--restrict-write"])
        .output()
        .expect("spawn ign");
    assert_eq!(out.status.code(), Some(2), "clap usage error");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--read-only"), "{stderr}");
}

fn assert_bare_write_level_refused(level: &str) {
    let (_dir, mut cmd) = adopt_ign();
    let started = Instant::now();
    let out = cmd
        .args(["--json", "adopt", "--write-level", level])
        .output()
        .expect("spawn ign");
    assert_eq!(out.status.code(), Some(2), "invalid_input is exit 2");
    let body = stderr_json(&out);
    assert_eq!(body["error"]["code"], "invalid_input", "{body}");
    let message = body["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("--write-level"),
        "names the flag: {message}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the refusal must come before any login (took {:?})",
        started.elapsed()
    );
}

#[test]
fn bare_authenticated_write_level_is_refused_before_login() {
    assert_bare_write_level_refused("Authenticated");
}

#[test]
fn bare_public_write_level_is_refused_before_login() {
    assert_bare_write_level_refused("public");
}
