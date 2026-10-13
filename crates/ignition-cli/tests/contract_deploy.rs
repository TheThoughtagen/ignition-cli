//! Binary-level contract for `ign deploy render` (21-04).
//!
//! `deploy render` is OFFLINE: every spawn here runs with an isolated,
//! absent config file and with the gateway environment stripped, and must
//! still succeed. The pinned `RENDER_GOLDEN` was cross-checked with an
//! independent shell oracle (see the 21-04 SUMMARY):
//!
//! ```text
//! cd gateways/tracer-gw/resources
//! for p in $(find ignition -type f | LC_ALL=C sort); do
//!   printf '%s\0%s\n' "$p" "$(shasum -a 256 "$p" | cut -d' ' -f1)"
//! done | shasum -a 256
//! ```

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::Value;

const TRACER_REPO: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../ignition-core/tests/fixtures/deploy/repos/tracer"
);

/// `render_sha256` of the tracer repo for `--env dev`. Pinned on purpose:
/// a change here means the listing format or the tracer bytes changed.
const RENDER_GOLDEN: &str = "7c8b41b16aa7680fac4e3cc1b4ac0ba3031973a720f33a625079e408f5b376a1";

const DATA_KEYS: &[&str] = &[
    "env",
    "gateway",
    "mode",
    "out",
    "files",
    "bytes",
    "render_sha256",
    "resources",
    "tag_providers",
    "uuids_minted",
    "opaque_files",
    "warnings",
];

/// `ign` with an absent isolated config and no ambient gateway knobs.
fn ign() -> (tempfile::TempDir, Command) {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cmd = Command::cargo_bin("ign").expect("binary 'ign' not found");
    cmd.env("IGNITION_CLI_CONFIG", dir.path().join("config.toml"));
    for var in [
        "IGNITION_URL",
        "IGNITION_TOKEN",
        "IGNITION_PROFILE",
        "IGNITION_JSON",
    ] {
        cmd.env_remove(var);
    }
    (dir, cmd)
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("mkdir");
    for entry in std::fs::read_dir(from).expect("read_dir") {
        let entry = entry.expect("entry");
        let dest = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_dir(&entry.path(), &dest);
        } else {
            std::fs::copy(entry.path(), &dest).expect("copy");
        }
    }
}

fn tracer_copy() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = dir.path().join("repo");
    copy_dir(Path::new(TRACER_REPO), &repo);
    (dir, repo)
}

fn stderr_json(out: &std::process::Output) -> Value {
    serde_json::from_slice(&out.stderr).unwrap_or_else(|e| {
        panic!(
            "stderr is not one JSON value ({e}): {}",
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

#[test]
fn render_digest_golden() {
    let (_dir, mut cmd) = ign();
    let out = cmd
        .args([
            "--json",
            "deploy",
            "render",
            "--env",
            "dev",
            "--repo",
            TRACER_REPO,
        ])
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    assert_eq!(envelope["ok"], true);
    assert!(
        envelope["profile"].is_null(),
        "offline verb: profile is null"
    );
    let data = envelope["data"].as_object().expect("data object");
    let keys: Vec<&str> = data.keys().map(String::as_str).collect();
    let mut expected: Vec<&str> = DATA_KEYS.to_vec();
    let mut got = keys.clone();
    expected.sort_unstable();
    got.sort_unstable();
    assert_eq!(got, expected, "data key set");
    assert_eq!(data["render_sha256"], RENDER_GOLDEN);
    assert_eq!(data["files"], 4);
    assert_eq!(data["gateway"], "tracer-gw");
    assert_eq!(data["mode"], "dev");
    assert!(data["out"].is_null());
    assert_eq!(data["warnings"], serde_json::json!([]));
}

#[test]
fn render_human_summary() {
    let (_dir, mut cmd) = ign();
    let out = cmd
        .args(["deploy", "render", "--env", "dev", "--repo", TRACER_REPO])
        .output()
        .expect("spawn ign");
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).expect("utf-8");
    assert!(
        stdout.starts_with("rendered gateways/tracer-gw for env dev (mode dev): 4 files, "),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("render_sha256 {RENDER_GOLDEN}\n")),
        "{stdout}"
    );
}

#[test]
fn render_missing_target_is_tree_invalid() {
    let (_dir, mut cmd) = ign();
    let out = cmd
        .args([
            "--json",
            "deploy",
            "render",
            "--env",
            "nope",
            "--repo",
            TRACER_REPO,
        ])
        .output()
        .expect("spawn ign");
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty(), "a refusal leaves stdout empty");
    let err = stderr_json(&out);
    assert_eq!(err["error"]["code"], "deploy_tree_invalid");
    let message = err["error"]["message"].as_str().expect("message");
    assert!(message.contains("targets/nope.yaml"), "{message}");
    assert!(
        message.contains("--env nope selects targets/nope.yaml"),
        "{message}"
    );
}

#[test]
fn render_bad_env_is_invalid_input() {
    for bad in ["../x", "a/b", ".hidden", ""] {
        let (_dir, mut cmd) = ign();
        let out = cmd
            .args([
                "--json",
                "deploy",
                "render",
                "--env",
                bad,
                "--repo",
                TRACER_REPO,
            ])
            .output()
            .expect("spawn ign");
        assert_eq!(out.status.code(), Some(2), "env {bad:?}");
        assert_eq!(
            stderr_json(&out)["error"]["code"],
            "invalid_input",
            "env {bad:?}"
        );
    }
}

#[test]
fn render_unlisted_type_is_refused() {
    let (_tmp, repo) = tracer_copy();
    let dir = repo.join("gateways/tracer-gw/resources/ignition/schedule/Example");
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("config.json"), b"{}").expect("write");
    let (_cfg, mut cmd) = ign();
    let out = cmd
        .args(["--json", "deploy", "render", "--env", "dev", "--repo"])
        .arg(&repo)
        .output()
        .expect("spawn ign");
    assert_eq!(out.status.code(), Some(2));
    let err = stderr_json(&out);
    assert_eq!(err["error"]["code"], "deploy_tree_invalid");
    let message = err["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("gateways/tracer-gw/resources/ignition/schedule/"),
        "{message}"
    );
}
