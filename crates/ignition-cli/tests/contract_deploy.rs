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

/// 21-09: `--out` writes the rendered tree byte for byte plus `render.json`,
/// and the human mode adds the `wrote <n> files to <dir>` line third.
#[test]
fn render_out_writes_tree_and_marker() {
    let scratch = tempfile::tempdir().expect("tempdir");
    let out_dir = scratch.path().join("rendered");
    let (_dir, mut cmd) = ign();
    let out = cmd
        .args([
            "deploy",
            "render",
            "--env",
            "dev",
            "--repo",
            TRACER_REPO,
            "--out",
        ])
        .arg(&out_dir)
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("utf-8");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 3, "{stdout}");
    assert!(
        lines[0].starts_with("rendered gateways/tracer-gw"),
        "{stdout}"
    );
    assert_eq!(lines[1], format!("render_sha256 {RENDER_GOLDEN}"));
    assert_eq!(
        lines[2],
        format!("wrote 4 files to {}", out_dir.display()),
        "{stdout}"
    );

    // the files equal the tracer sources they were rendered from
    let resources = Path::new(TRACER_REPO).join("gateways/tracer-gw/resources");
    let marker: Value =
        serde_json::from_slice(&std::fs::read(out_dir.join("render.json")).expect("marker"))
            .expect("marker json");
    assert_eq!(marker["schema_version"], 1);
    assert_eq!(marker["gateway"], "tracer-gw");
    assert_eq!(marker["env"], "dev");
    assert_eq!(marker["mode"], "dev");
    assert_eq!(marker["render_sha256"], RENDER_GOLDEN);
    let files = marker["files"].as_array().expect("files");
    assert_eq!(files.len(), 4);
    for entry in files {
        let rel = entry["path"].as_str().expect("path");
        let written = std::fs::read(out_dir.join(rel)).expect("written file");
        let source = std::fs::read(resources.join(rel)).expect("source file");
        assert_eq!(written, source, "{rel}");
        assert_eq!(entry["bytes"], written.len());
    }
}

/// 21-09: a refused `--out` is `invalid_input` (exit 2) and leaves a
/// non-empty unrelated directory exactly as it was.
#[test]
fn render_out_refusal_is_invalid_input() {
    let scratch = tempfile::tempdir().expect("tempdir");
    let out_dir = scratch.path().join("mine");
    std::fs::create_dir(&out_dir).expect("mkdir");
    std::fs::write(out_dir.join("notes.txt"), b"keep").expect("write");

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
            "--out",
        ])
        .arg(&out_dir)
        .output()
        .expect("spawn ign");
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty(), "a refusal leaves stdout empty");
    let err = stderr_json(&out);
    assert_eq!(err["error"]["code"], "invalid_input");
    assert!(
        err["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("notes.txt")),
        "{err}"
    );
    let names: Vec<_> = std::fs::read_dir(&out_dir)
        .expect("read_dir")
        .map(|e| e.expect("entry").file_name())
        .collect();
    assert_eq!(names, ["notes.txt"]);
    assert_eq!(
        std::fs::read(out_dir.join("notes.txt")).expect("read"),
        b"keep"
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

// ---- 21-08: the secret lint never prints a value --------------------------

const CANARY_EMBEDDED: &str = "CANARY-EMBEDDED-21";
const CANARY_PLAINTEXT: &str = "CANARY-PLAINTEXT-21";
const CANARY_KEYNAME: &str = "CANARY-KEYNAME-21";
const CANARY_WARN: &str = "CANARY-WARN-21";
const CANARY_REFERENCED: &str = "CANARY-REFERENCED-WR02";

/// A tracer copy whose manifest also allows `extra_types`, plus the given
/// resource config files (gateway-relative path under `resources/`).
fn secret_repo(extra_types: &[&str], configs: &[(&str, String)]) -> (tempfile::TempDir, PathBuf) {
    let (tmp, repo) = tracer_copy();
    let gateway = repo.join("gateways/tracer-gw");
    let mut manifest =
        std::fs::read_to_string(gateway.join("manifest.yaml")).expect("read manifest");
    for t in extra_types {
        manifest.push_str(&format!("  - {t}\n"));
    }
    std::fs::write(gateway.join("manifest.yaml"), manifest).expect("write manifest");
    for (rel, body) in configs {
        let path = gateway.join("resources").join(rel);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, body).expect("write config");
    }
    (tmp, repo)
}

#[test]
fn embedded_secret_names_path_not_value() {
    let (_tmp, repo) = secret_repo(
        &[
            "ignition/database-connection",
            "com.cirruslink.mqtt.engine.gateway/server",
        ],
        &[
            (
                "ignition/database-connection/Main/config.json",
                format!(
                    r#"{{"password":{{"type":"Embedded","data":{{"ciphertext":"{CANARY_EMBEDDED}"}}}}}}"#
                ),
            ),
            (
                "ignition/database-connection/Plain/config.json",
                format!(r#"{{"password":"{CANARY_PLAINTEXT}"}}"#),
            ),
            (
                "com.cirruslink.mqtt.engine.gateway/server/Broker/config.json",
                format!(r#"{{"settings":{{"clientSecret":"{CANARY_KEYNAME}"}}}}"#),
            ),
        ],
    );
    for json in [false, true] {
        let (_cfg, mut cmd) = ign();
        if json {
            cmd.arg("--json");
        }
        let out = cmd
            .args(["deploy", "render", "--env", "dev", "--repo"])
            .arg(&repo)
            .output()
            .expect("spawn ign");
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        assert_eq!(out.status.code(), Some(2), "json={json}: {stderr}");
        assert!(stdout.is_empty(), "a refusal leaves stdout empty: {stdout}");
        for canary in [
            CANARY_EMBEDDED,
            CANARY_PLAINTEXT,
            CANARY_KEYNAME,
            "SANITIZED",
        ] {
            assert!(
                !stderr.contains(canary),
                "json={json}: {canary} in {stderr}"
            );
            assert!(
                !stdout.contains(canary),
                "json={json}: {canary} in {stdout}"
            );
        }
        let message = if json {
            let err = stderr_json(&out);
            assert_eq!(err["error"]["code"], "embedded_secret");
            err["error"]["message"]
                .as_str()
                .expect("message")
                .to_string()
        } else {
            assert!(
                stderr.contains("embedded_secret")
                    || stderr.contains("embedded or plaintext secret"),
                "{stderr}"
            );
            stderr.clone()
        };
        for want in [
            "ignition/database-connection/Main/config.json: password (embedded)",
            "ignition/database-connection/Plain/config.json: password (plaintext)",
            "server/Broker/config.json: settings.clientSecret (plaintext, by key name)",
        ] {
            assert!(
                message.contains(want),
                "json={json}: {want} missing from {message}"
            );
        }
    }
}

#[test]
fn malformed_referenced_secret_names_path_not_value() {
    let (_tmp, repo) = secret_repo(
        &[
            "ignition/database-connection",
            "com.cirruslink.mqtt.engine.gateway/server",
        ],
        &[
            (
                "ignition/database-connection/Main/config.json",
                format!(
                    r#"{{"password":{{"type":"Referenced","data":{{"providerName":"p","secretName":"s","password":"{CANARY_REFERENCED}"}}}}}}"#
                ),
            ),
            (
                "com.cirruslink.mqtt.engine.gateway/server/Broker/config.json",
                format!(
                    r#"{{"settings":{{"cred":{{"type":"Referenced","data":{{"providerName":"p","secretName":"s","password":"{CANARY_REFERENCED}"}}}}}}}}"#
                ),
            ),
        ],
    );
    for json in [false, true] {
        let (_cfg, mut cmd) = ign();
        if json {
            cmd.arg("--json");
        }
        let out = cmd
            .args(["deploy", "render", "--env", "dev", "--repo"])
            .arg(&repo)
            .output()
            .expect("spawn ign");
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        assert_eq!(out.status.code(), Some(2), "json={json}: {stderr}");
        assert!(stdout.is_empty(), "a refusal leaves stdout empty: {stdout}");
        assert!(
            !stderr.contains(CANARY_REFERENCED),
            "json={json}: canary in {stderr}"
        );
        assert!(
            !stdout.contains(CANARY_REFERENCED),
            "json={json}: canary in {stdout}"
        );
        let message = if json {
            let err = stderr_json(&out);
            assert_eq!(err["error"]["code"], "embedded_secret");
            err["error"]["message"]
                .as_str()
                .expect("message")
                .to_string()
        } else {
            assert!(
                stderr.contains("embedded_secret")
                    || stderr.contains("embedded or plaintext secret"),
                "{stderr}"
            );
            stderr.clone()
        };
        for want in [
            "ignition/database-connection/Main/config.json: password (plaintext)",
            "server/Broker/config.json: settings.cred.data.password (plaintext, by key name)",
        ] {
            assert!(
                message.contains(want),
                "json={json}: {want} missing from {message}"
            );
        }
    }
}

#[test]
fn possible_secret_warning_names_path_not_value() {
    let (_tmp, repo) = secret_repo(
        &["ignition/database-connection"],
        &[(
            "ignition/database-connection/Main/config.json",
            format!(r#"{{"settings":{{"mqttPassword":"{CANARY_WARN}"}}}}"#),
        )],
    );
    let (_cfg, mut cmd) = ign();
    let out = cmd
        .args(["--json", "deploy", "render", "--env", "dev", "--repo"])
        .arg(&repo)
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(!stdout.contains(CANARY_WARN), "{stdout}");
    assert!(!stderr.contains(CANARY_WARN), "{stderr}");
    let envelope: Value = serde_json::from_str(&stdout).expect("envelope");
    let warnings = envelope["data"]["warnings"].as_array().expect("warnings");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0]
            .as_str()
            .expect("string")
            .contains("config.json: settings.mqttPassword;"),
        "{warnings:?}"
    );
}
