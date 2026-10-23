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

mod common;

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use common::*;
use serde_json::Value;

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

fn tracer_copy() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = dir.path().join("repo");
    copy_dir(Path::new(TRACER_REPO), &repo);
    (dir, repo)
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
fn malformed_referenced_warning_names_path_not_value() {
    let (_tmp, repo) = secret_repo(
        &["ignition/database-connection"],
        &[(
            "ignition/database-connection/Main/config.json",
            format!(
                r#"{{"settings":{{"cred":{{"type":"Referenced","data":{{"providerName":"p","secretName":"s","password":"{CANARY_REFERENCED}"}}}}}}}}"#
            ),
        )],
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
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        assert_eq!(out.status.code(), Some(0), "json={json}: {stderr}");
        assert!(
            !stdout.contains(CANARY_REFERENCED),
            "json={json}: canary in {stdout}"
        );
        assert!(
            !stderr.contains(CANARY_REFERENCED),
            "json={json}: canary in {stderr}"
        );
        if json {
            let envelope: Value = serde_json::from_str(&stdout).expect("envelope");
            let warnings = envelope["data"]["warnings"].as_array().expect("warnings");
            assert_eq!(warnings.len(), 2, "{warnings:?}");
            assert!(
                warnings.iter().any(|w| w
                    .as_str()
                    .is_some_and(|w| w.contains("config.json: settings.cred.data.password;"))),
                "{warnings:?}"
            );
        } else {
            assert!(
                format!("{stdout}{stderr}").contains("settings.cred.data.password"),
                "stdout={stdout} stderr={stderr}"
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

// ---- 22-02: `deploy plan` -------------------------------------------------

/// A tracer repo whose `targets/dev.yaml` master is `profile: dev` followed
/// by `master_extra` (raw YAML lines, indented four spaces), plus a scratch
/// directory. Written once; `local_tracer` builds on it. 22-03 reuses it.
fn tracer_with_master(master_extra: &str) -> (tempfile::TempDir, PathBuf) {
    let (tmp, repo) = tracer_copy();
    let yaml = format!(
        "schema_version: 1\ngateway: tracer-gw\nmode: dev\nnodes:\n  master:\n    profile: dev\n{master_extra}"
    );
    std::fs::write(repo.join("targets/dev.yaml"), yaml).expect("target file");
    (tmp, repo)
}

/// The first rendered file in the live mode collection, byte order.
fn first_live_file(data: &Path) -> PathBuf {
    fn walk(dir: &Path, found: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("read_dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                walk(&path, found);
            } else {
                found.push(path);
            }
        }
    }
    let mut found = Vec::new();
    walk(&data.join("config/resources/dev"), &mut found);
    found.sort();
    found.into_iter().next().expect("a live file")
}

#[test]
fn plan_cli_live_equal_to_render_exits_0() {
    let (_scratch, repo, _data) = local_tracer();
    let (_cfg, mut cmd) = ign();
    let out = plan_json(&repo, &mut cmd);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["profile"], "dev");
    let data = envelope["data"].as_object().expect("data object");
    // serde_json maps sort their keys; compare as sets.
    let mut keys: Vec<&str> = data.keys().map(String::as_str).collect();
    keys.sort_unstable();
    let mut expected = vec![
        "env",
        "gateway",
        "mode",
        "node",
        "transport",
        "render_sha256",
        "plan_sha256",
        "summary",
        "entries",
        "delete_set",
        "base",
        "warnings",
    ];
    expected.sort_unstable();
    assert_eq!(keys, expected);
    assert!(data["base"].is_null(), "no --base, no base summary");
    assert_eq!(data["node"], "master");
    assert_eq!(data["transport"], "local");
    assert_eq!(data["summary"]["has_changes"], false);
    let entries = data["entries"].as_array().expect("entries");
    assert!(!entries.is_empty());
    for entry in entries {
        assert_eq!(entry["status"], "same", "{entry}");
    }
}

#[test]
fn plan_cli_byte_change_still_exits_0() {
    let (_scratch, repo, data) = local_tracer();
    let file = first_live_file(&data);
    let mut bytes = std::fs::read(&file).expect("read live file");
    bytes.push(b' ');
    std::fs::write(&file, bytes).expect("edit live file");
    let (_cfg, mut cmd) = ign();
    let out = plan_json(&repo, &mut cmd);
    assert_eq!(
        out.status.code(),
        Some(0),
        "SF-2: a plan with changes still exits 0: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    assert_eq!(envelope["data"]["summary"]["has_changes"], true);
    assert_eq!(envelope["data"]["summary"]["changed"], 1);
    let changed = envelope["data"]["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .filter(|e| e["status"] == "changed")
        .count();
    assert_eq!(changed, 1);
}

#[test]
fn plan_cli_needs_no_secret() {
    let (_scratch, repo, _data) = local_tracer();
    let empty_cfg = tempfile::tempdir().expect("tempdir");
    let mut cmd = Command::cargo_bin("ign").expect("binary 'ign' not found");
    cmd.env("IGNITION_CLI_CONFIG", empty_cfg.path().join("absent.toml"));
    for var in [
        "IGNITION_URL",
        "IGNITION_TOKEN",
        "IGNITION_PROFILE",
        "IGNITION_JSON",
        "IGNITION_USER",
        "IGNITION_PASSWORD",
        "IGNITION_TOKEN_DEV",
    ] {
        cmd.env_remove(var);
    }
    let out = plan_json(&repo, &mut cmd);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn plan_cli_master_without_host_access_exits_2() {
    let (_tmp, repo) = tracer_with_master("");
    let (_cfg, mut cmd) = ign();
    let out = plan_json(&repo, &mut cmd);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty(), "a refusal leaves stdout empty");
    let err = stderr_json(&out);
    assert_eq!(err["error"]["code"], "deploy_tree_invalid");
    let message = err["error"]["message"].as_str().expect("message");
    assert!(message.contains("nodes.master"), "{message}");
}

#[test]
fn plan_cli_unreadable_data_dir_exits_4() {
    let scratch = tempfile::tempdir().expect("tempdir");
    let missing = scratch.path().join("no-such-data-dir");
    let (_tmp, repo) = tracer_with_master(&format!(
        "    local:\n      data_dir: '{}'\n",
        missing.display()
    ));
    let (_cfg, mut cmd) = ign();
    let out = plan_json(&repo, &mut cmd);
    assert_eq!(
        out.status.code(),
        Some(4),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty(), "a refusal leaves stdout empty");
    let err = stderr_json(&out);
    assert_eq!(err["error"]["code"], "transport_failed");
    let message = err["error"]["message"].as_str().expect("message");
    assert!(message.contains("nodes.master"), "{message}");
}

/// 25-16 (D-07, D-14): a gateway-config repo with no `targets/` directory
/// and a separate targets checkout. `local_tracer` builds a `local:` rig;
/// its `targets/` directory and its data directory then move into the
/// targets checkout, and the target's `data_dir` becomes the relative
/// `data`, which must resolve against the targets checkout. Returns
/// `(scratch, repo, targets, data)`.
fn split_tracer() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
    let (scratch, repo, data) = local_tracer();
    let targets = scratch.path().join("iac");
    std::fs::create_dir_all(&targets).expect("targets dir");
    std::fs::rename(repo.join("targets"), targets.join("targets")).expect("move targets");
    let moved_data = targets.join("data");
    std::fs::rename(&data, &moved_data).expect("move data");
    let yaml = "schema_version: 1\ngateway: tracer-gw\nmode: dev\nnodes:\n  master:\n    profile: dev\n    local:\n      data_dir: data\n";
    std::fs::write(targets.join("targets/dev.yaml"), yaml).expect("target file");
    assert!(
        !repo.join("targets").exists(),
        "the gateway repo holds no targets/"
    );
    (scratch, repo, targets, moved_data)
}

#[test]
fn plan_reads_targets_dir() {
    let (_scratch, repo, targets, data) = split_tracer();
    let (_cfg, mut cmd) = ign();
    let out = cmd
        .args(["--json", "deploy", "plan", "--env", "dev", "--repo"])
        .arg(&repo)
        .arg("--targets")
        .arg(&targets)
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    assert_eq!(envelope["data"]["transport"], "local");
    assert_eq!(envelope["data"]["summary"]["has_changes"], false);

    // The data directory is the one under the targets checkout: a byte
    // changed there shows in the plan.
    let file = first_live_file(&data);
    let mut bytes = std::fs::read(&file).expect("read live file");
    bytes.push(b' ');
    std::fs::write(&file, bytes).expect("edit live file");
    let (_cfg, mut cmd) = ign();
    let out = cmd
        .args(["--json", "deploy", "plan", "--env", "dev", "--repo"])
        .arg(&repo)
        .arg("--targets")
        .arg(&targets)
        .output()
        .expect("spawn ign");
    assert_eq!(out.status.code(), Some(0));
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    assert_eq!(envelope["data"]["summary"]["changed"], 1);
}

#[test]
fn plan_without_targets_unchanged() {
    let (_scratch, repo, _targets, _data) = split_tracer();
    let (_cfg, mut cmd) = ign();
    let out = plan_json(&repo, &mut cmd);
    assert_eq!(
        out.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty(), "a refusal leaves stdout empty");
    let err = stderr_json(&out);
    assert_eq!(err["error"]["code"], "deploy_tree_invalid");
    let message = err["error"]["message"].as_str().expect("message");
    assert!(message.contains("targets/dev.yaml"), "{message}");
}

/// The long flags of `ign deploy <leaf>`, sorted, without the global ones.
fn deploy_leaf_flags(leaf: &str) -> Vec<String> {
    use clap::CommandFactory;
    let cli = ignition_cli::cli::Cli::command();
    let deploy = cli.find_subcommand("deploy").expect("deploy");
    let found = deploy.find_subcommand(leaf).expect("deploy leaf");
    let mut longs: Vec<String> = found
        .get_arguments()
        .filter(|a| !a.is_global_set())
        .filter_map(|a| a.get_long())
        .filter(|l| !matches!(*l, "help" | "version"))
        .map(str::to_string)
        .collect();
    longs.sort_unstable();
    longs
}

#[test]
fn plan_flags_include_targets() {
    assert_eq!(
        deploy_leaf_flags("plan"),
        ["base", "diff", "env", "repo", "targets"],
        "the flag set is the contract (D-14)"
    );
}

#[test]
fn plan_cli_human_output_lines() {
    let (_scratch, repo, data) = local_tracer();
    let file = first_live_file(&data);
    let mut bytes = std::fs::read(&file).expect("read live file");
    bytes.push(b' ');
    std::fs::write(&file, bytes).expect("edit live file");
    let (_cfg, mut cmd) = ign();
    let out = cmd
        .args(["deploy", "plan", "--env", "dev", "--repo"])
        .arg(&repo)
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
    assert!(
        lines[0].starts_with(
            "plan tracer-gw (dev, mode dev) via local: 0 added, 1 changed, 0 removed, "
        ),
        "{stdout}"
    );
    assert!(lines[0].ends_with(" unmanaged"), "{stdout}");
    let changed: Vec<&&str> = lines.iter().filter(|l| l.starts_with("changed ")).collect();
    assert_eq!(changed.len(), 1, "{stdout}");
    assert!(
        changed[0].contains(" core=") && changed[0].contains(" live_in="),
        "{stdout}"
    );
    let last = lines.last().expect("last line");
    let hex = last.strip_prefix("plan_sha256 ").expect("plan_sha256 last");
    assert_eq!(hex.len(), 64, "{last}");
    assert!(hex.bytes().all(|b| b.is_ascii_hexdigit()), "{last}");
}

/// Seed a live unit `OLD` and a declared-names ledger listing it, below the
/// local tracer's data directory. Returns the two `OLD` file paths.
fn seed_old_unit_and_ledger(data: &Path) -> [&'static str; 2] {
    let files = [
        "ignition/database-translator/OLD/config.json",
        "ignition/database-translator/OLD/resource.json",
    ];
    for file in files {
        let path = data.join("config/resources/dev").join(file);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("unit dir");
        std::fs::write(path, b"{}").expect("live file");
    }
    let ledger = serde_json::json!({
        "schema_version": 1,
        "gateway": "tracer-gw",
        "mode": "dev",
        "render_sha256": "0".repeat(64),
        "run_id": "cli-test",
        "files": files,
    });
    let dir = data.join("ign-deploy/tracer-gw/dev");
    std::fs::create_dir_all(&dir).expect("ledger dir");
    std::fs::write(
        dir.join("ledger.json"),
        serde_json::to_vec_pretty(&ledger).expect("ledger json"),
    )
    .expect("ledger file");
    files
}

#[test]
fn plan_local_with_ledger_lists_delete_set() {
    let (_scratch, repo, data) = local_tracer();
    let files = seed_old_unit_and_ledger(&data);
    let (_cfg, mut cmd) = ign();
    let out = plan_json(&repo, &mut cmd);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    let data = &envelope["data"];
    let delete: Vec<&str> = data["delete_set"]["files"]
        .as_array()
        .expect("delete_set.files")
        .iter()
        .map(|v| v.as_str().expect("path"))
        .collect();
    assert_eq!(delete, files);
    assert_eq!(data["summary"]["removed"], 1);
    assert_eq!(data["summary"]["has_changes"], true);
    let removed = data["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|e| e["status"] == "removed")
        .expect("a removed entry");
    assert_eq!(removed["key"], "ignition/database-translator/OLD");
    assert_eq!(removed["destructive"], true);
    assert_eq!(removed["destructive_reason"], "resource_removed");
    assert_eq!(removed["lockout_risk"], false);
    assert_eq!(removed["core"], "none");
}

#[test]
fn plan_local_human_lists_delete_lines_and_flags() {
    let (_scratch, repo, data) = local_tracer();
    let files = seed_old_unit_and_ledger(&data);
    let (_cfg, mut cmd) = ign();
    let out = cmd
        .args(["deploy", "plan", "--env", "dev", "--repo"])
        .arg(&repo)
        .output()
        .expect("spawn ign");
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).expect("utf-8");
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(
        lines.iter().any(
            |l| l.starts_with("removed ignition/database-translator/OLD ")
                && l.ends_with(" destructive=resource_removed")
        ),
        "{stdout}"
    );
    for file in files {
        assert!(
            lines.contains(&format!("delete {file}").as_str()),
            "{stdout}"
        );
    }
    assert!(
        lines.last().expect("last").starts_with("plan_sha256 "),
        "plan_sha256 stays the last line: {stdout}"
    );
}

#[test]
fn plan_local_corrupt_ledger_exits_6() {
    let (_scratch, repo, data) = local_tracer();
    seed_old_unit_and_ledger(&data);
    std::fs::write(
        data.join("ign-deploy/tracer-gw/dev/ledger.json"),
        b"not json",
    )
    .expect("corrupt the ledger");
    let (_cfg, mut cmd) = ign();
    let out = plan_json(&repo, &mut cmd);
    assert_eq!(out.status.code(), Some(6));
    assert!(out.stdout.is_empty(), "stdout stays empty on a refusal");
    let err = stderr_json(&out);
    assert_eq!(err["error"]["code"], "deploy_ledger_invalid");
    let message = err["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("ign-deploy/tracer-gw/dev/ledger.json"),
        "{message}"
    );
}

/// Canary seeded into live secret-bearing content (22-04, SC-5 plan half).
const CANARY_PLAN: &str = "IGN-CANARY-22-03";

#[test]
fn plan_diff_canary_cli() {
    let (_scratch, repo, data) = local_tracer();
    let live = data.join("config/resources/dev");
    let db = live.join("ignition/database-translator/POSTGRES/config.json");
    let alarm = live.join("ignition/general-alarm-settings/config.json");
    let leak = live.join("ignition/database-translator/LEAK");
    std::fs::write(
        &db,
        format!(r#"{{"x":{{"type":"Embedded","data":{{"ciphertext":"{CANARY_PLAN}"}}}}}}"#),
    )
    .expect("seed embedded");
    std::fs::write(&alarm, format!(r#"{{"password":"{CANARY_PLAN}"}}"#)).expect("seed password");
    std::fs::create_dir_all(&leak).expect("leak dir");
    std::fs::write(
        leak.join("config.json"),
        format!(r#"{{"x":{{"type":"Embedded","data":{{"ciphertext":"{CANARY_PLAN}"}}}}}}"#),
    )
    .expect("seed unmanaged");

    // --json
    let (_cfg, mut cmd) = ign();
    let out = cmd
        .args([
            "--json", "deploy", "plan", "--env", "dev", "--diff", "--repo",
        ])
        .arg(&repo)
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    for (name, bytes) in [("stdout", &out.stdout), ("stderr", &out.stderr)] {
        let text = String::from_utf8_lossy(bytes);
        assert!(!text.contains(CANARY_PLAN), "canary in json {name}");
        assert!(!text.contains("ciphertext"), "ciphertext in json {name}");
    }
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    let entries = envelope["data"]["entries"].as_array().expect("entries");
    let changed: Vec<&Value> = entries
        .iter()
        .filter(|e| e["status"] == "changed")
        .collect();
    assert_eq!(changed.len(), 2);
    for entry in changed {
        let diff = entry["diff"].as_str().expect("diff text");
        assert!(
            diff.contains("withheld: contains secret-bearing locations"),
            "{diff}"
        );
    }
    let unmanaged = entries
        .iter()
        .find(|e| e["key"] == "ignition/database-translator/LEAK")
        .expect("unmanaged entry");
    assert_eq!(unmanaged["status"], "unmanaged");
    assert!(unmanaged["diff"].is_null());

    // human
    let (_cfg, mut cmd) = ign();
    let out = cmd
        .args(["deploy", "plan", "--env", "dev", "--diff", "--repo"])
        .arg(&repo)
        .output()
        .expect("spawn ign");
    assert_eq!(out.status.code(), Some(0));
    for (name, bytes) in [("stdout", &out.stdout), ("stderr", &out.stderr)] {
        let text = String::from_utf8_lossy(bytes);
        assert!(!text.contains(CANARY_PLAN), "canary in human {name}");
        assert!(!text.contains("ciphertext"), "ciphertext in human {name}");
    }
    let stdout = String::from_utf8(out.stdout).expect("utf-8");
    assert!(
        stdout
            .lines()
            .any(|l| l.starts_with("  ")
                && l.contains("withheld: contains secret-bearing locations")),
        "diff lines are indented two spaces under their entry: {stdout}"
    );
    let last = stdout.lines().last().expect("last line");
    assert!(last.starts_with("plan_sha256 "), "{stdout}");
}

#[test]
fn plan_base_cli_marks_drift_and_refuses_a_tampered_base() {
    let (scratch, repo, data) = local_tracer();
    let base = scratch.path().join("out");
    let live = data.join("config/resources/dev");
    let db = live.join("ignition/database-translator/POSTGRES/config.json");
    let mut bytes = std::fs::read(&db).expect("read live");
    bytes.push(b' ');
    std::fs::write(&db, bytes).expect("edit live");

    let (_cfg, mut cmd) = ign();
    let out = cmd
        .args(["--json", "deploy", "plan", "--env", "dev", "--repo"])
        .arg(&repo)
        .arg("--base")
        .arg(&base)
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    assert_eq!(envelope["data"]["summary"]["drifted"], 1);
    assert_eq!(envelope["data"]["base"]["files"], 4);
    let drifted: Vec<&Value> = envelope["data"]["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .filter(|e| e["drift"] == true)
        .collect();
    assert_eq!(drifted.len(), 1);

    // a modified base is invalid_input (exit 2) and prints nothing on stdout
    std::fs::write(base.join("extra.json"), b"{}").expect("tamper");
    let (_cfg, mut cmd) = ign();
    let out = cmd
        .args(["--json", "deploy", "plan", "--env", "dev", "--repo"])
        .arg(&repo)
        .arg("--base")
        .arg(&base)
        .output()
        .expect("spawn ign");
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    assert_eq!(stderr_json(&out)["error"]["code"], "invalid_input");
}

// ---------------------------------------------------------------------
// `ign deploy pull` (22-08, DPLN-06)
// ---------------------------------------------------------------------

/// Canary seeded into live Embedded ciphertext that pull must never write.
const CANARY_PULL: &str = "IGN-CANARY-22-08-CIPHERTEXT";

/// A local tracer whose live mode collection also holds an Embedded-secret
/// unit; the declared gateway directory is removed so the pull starts from
/// an empty repo.
fn pull_fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let (scratch, repo, data) = local_tracer();
    let secret = data.join("config/resources/dev/ignition/database-translator/SECRET");
    std::fs::create_dir_all(&secret).expect("secret dir");
    std::fs::write(
        secret.join("resource.json"),
        br#"{"scope":"A","version":1,"restricted":false,"overridable":true,"files":["config.json"],"attributes":{"uuid":"7c2e1f90-3b4a-4d5e-8f6a-1b2c3d4e5f60","enabled":true}}"#,
    )
    .expect("seed resource.json");
    std::fs::write(
        secret.join("config.json"),
        format!(r#"{{"password":{{"type":"Embedded","data":{{"ciphertext":"{CANARY_PULL}"}}}}}}"#),
    )
    .expect("seed secret");
    std::fs::remove_dir_all(repo.join("gateways/tracer-gw")).expect("empty the repo");
    (scratch, repo, data)
}

#[test]
fn pull_never_writes_secret() {
    let (_scratch, repo, data) = pull_fixture();
    let live_before = read_tree(&data);

    // --json
    let (_cfg, mut cmd) = ign();
    let out = cmd
        .args(["--json", "deploy", "pull", "--env", "dev", "--repo"])
        .arg(&repo)
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    for (name, bytes) in [("stdout", &out.stdout), ("stderr", &out.stderr)] {
        let text = String::from_utf8_lossy(bytes);
        assert!(!text.contains(CANARY_PULL), "canary in json {name}");
        assert!(!text.contains("ciphertext"), "ciphertext in json {name}");
    }
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["profile"], "dev");
    let data_obj = envelope["data"].as_object().expect("data object");
    let mut keys: Vec<&str> = data_obj.keys().map(String::as_str).collect();
    keys.sort_unstable();
    let mut expected = vec![
        "env",
        "gateway",
        "mode",
        "node",
        "transport",
        "out",
        "files_written",
        "manifest",
        "render_sha256",
        "skipped",
        "warnings",
    ];
    expected.sort_unstable();
    assert_eq!(keys, expected);
    assert_eq!(data_obj["out"], "gateways/tracer-gw");
    assert_eq!(data_obj["manifest"], "generated");
    assert_eq!(data_obj["transport"], "local");
    let skipped = data_obj["skipped"].as_array().expect("skipped");
    assert_eq!(skipped.len(), 1, "{skipped:?}");
    assert_eq!(
        skipped[0],
        serde_json::json!({
            "path": "ignition/database-translator/SECRET",
            "reason": "embedded_secret"
        })
    );

    // not one written file holds the ciphertext or the unit
    let written = read_tree(&repo.join("gateways/tracer-gw"));
    assert!(written.len() >= 5, "{:?}", written.keys());
    for (path, bytes) in &written {
        assert!(
            !String::from_utf8_lossy(bytes).contains(CANARY_PULL),
            "{path} holds the ciphertext"
        );
        assert!(!path.contains("SECRET"), "{path}");
    }
    // read-only against the host
    assert_eq!(read_tree(&data), live_before);

    // pull, render, plan: nothing to do, the secret unit is unmanaged
    let (_cfg, mut cmd) = ign();
    let out = plan_json(&repo, &mut cmd);
    assert_eq!(out.status.code(), Some(0));
    let plan: Value = serde_json::from_slice(&out.stdout).expect("plan envelope");
    assert_eq!(plan["data"]["summary"]["has_changes"], false);
    let unmanaged: Vec<&str> = plan["data"]["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .filter(|e| e["status"] == "unmanaged")
        .map(|e| e["key"].as_str().expect("key"))
        .collect();
    assert_eq!(unmanaged, ["ignition/database-translator/SECRET"]);

    // human mode, second pull over the now non-empty tree (needs --yes; the
    // manifest the first pull generated is now kept)
    let (_cfg, mut cmd) = ign();
    let out = cmd
        .args(["--yes", "deploy", "pull", "--env", "dev", "--repo"])
        .arg(&repo)
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    for (name, bytes) in [("stdout", &out.stdout), ("stderr", &out.stderr)] {
        let text = String::from_utf8_lossy(bytes);
        assert!(!text.contains(CANARY_PULL), "canary in human {name}");
        assert!(!text.contains("ciphertext"), "ciphertext in human {name}");
    }
    let stdout = String::from_utf8(out.stdout).expect("utf-8");
    let first = stdout.lines().next().expect("first line");
    assert!(
        first.starts_with("pulled ")
            && first.contains(
                "into gateways/tracer-gw/ from nodes.master via local (1 skipped, manifest kept)"
            ),
        "{stdout}"
    );
    assert!(
        stdout
            .lines()
            .any(|l| l == "skipped ignition/database-translator/SECRET: embedded_secret"),
        "{stdout}"
    );
    assert!(
        stdout
            .lines()
            .last()
            .expect("last")
            .starts_with("render_sha256 "),
        "{stdout}"
    );
}

#[test]
fn pull_cli_overwrite_needs_yes() {
    let (_scratch, repo, _data) = local_tracer();
    let before = read_tree(&repo);

    let (_cfg, mut cmd) = ign();
    let out = cmd
        .args(["--json", "deploy", "pull", "--env", "dev", "--repo"])
        .arg(&repo)
        .output()
        .expect("spawn ign");
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let err = stderr_json(&out);
    assert_eq!(err["error"]["code"], "confirmation_required");
    let message = err["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("deploy pull would replace gateways/tracer-gw/resources, tags and modes/dev")
            && message.contains("(4 existing files, e.g. resources/ignition/database-translator/POSTGRES/config.json"),
        "{message}"
    );
    assert_eq!(read_tree(&repo), before, "nothing written");

    // with --yes the live tree replaces it, and the manifest is kept
    let manifest = std::fs::read(repo.join("gateways/tracer-gw/manifest.yaml")).expect("manifest");
    let (_cfg, mut cmd) = ign();
    let out = cmd
        .args([
            "--json", "--yes", "deploy", "pull", "--env", "dev", "--repo",
        ])
        .arg(&repo)
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    assert_eq!(envelope["data"]["manifest"], "kept");
    assert_eq!(
        std::fs::read(repo.join("gateways/tracer-gw/manifest.yaml")).expect("manifest"),
        manifest
    );
    // the base resources/ are gone (the live tree is all in the mode collection)
    assert!(!repo.join("gateways/tracer-gw/resources").exists());
    assert!(repo
        .join("gateways/tracer-gw/modes/dev/resources/ignition/database-translator/POSTGRES/config.json")
        .is_file());
}

#[test]
fn pull_cli_master_without_host_access_exits_2() {
    let (_scratch, repo) = tracer_with_master("");
    std::fs::remove_dir_all(repo.join("gateways/tracer-gw")).expect("empty the repo");
    let (_cfg, mut cmd) = ign();
    let out = cmd
        .args(["--json", "deploy", "pull", "--env", "dev", "--repo"])
        .arg(&repo)
        .output()
        .expect("spawn ign");
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    assert_eq!(stderr_json(&out)["error"]["code"], "deploy_tree_invalid");
    assert!(!repo.join("gateways/tracer-gw").exists(), "nothing written");
}

// ---- 22-11: `ign deploy verify` (DPLN-07, DPLN-09, DSURF-04) -----------------

const VERIFY_CANARY: &str = "CANARY-VERIFY-LOG-22";
/// The rendered `config.json` of `key` from the live tree with its null
/// members KEPT: a REST view that echoes unset members as explicit `null`
/// (`ignition/cobranding`; G-22-2).
fn rest_config_raw(data: &Path, key: &str) -> Value {
    let path = data
        .join("config/resources/dev")
        .join(key)
        .join("config.json");
    serde_json::from_slice(&std::fs::read(path).expect("live config")).expect("json")
}

/// A mock gateway that agrees with the live tree. `omit_translator` leaves
/// the declared database-translator out of the REST list (a red read_back);
/// `log_items` are the WARN lines the gateway returns. `rest_keeps_nulls`
/// serves the translator and the singleton with their null members kept.
async fn mount_verify_gateway(
    server: &wiremock::MockServer,
    data: &Path,
    omit_translator: bool,
    rest_keeps_nulls: bool,
    log_items: Vec<Value>,
) {
    let rest_of = |key: &str| {
        if rest_keeps_nulls {
            rest_config_raw(data, key)
        } else {
            rest_config(data, key)
        }
    };
    use serde_json::json;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, ResponseTemplate};
    Mock::given(method("GET"))
        .and(path("/data/api/v1/scan/config"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "scanActive": false,
            "lastScanTimestamp": 1_792_000_000_000_i64,
            "lastScanDuration": 300
        })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/data/api/v1/gateway-info"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"deploymentMode": "dev"})))
        .mount(server)
        .await;
    let items = if omit_translator {
        json!([])
    } else {
        json!([{
            "type": "ignition/database-translator",
            "name": "POSTGRES",
            "collection": "dev",
            "config": rest_of(NAMED_TRANSLATOR)
        }])
    };
    Mock::given(method("GET"))
        .and(path(
            "/data/api/v1/resources/list/ignition/database-translator",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": items,
            "metadata": {"total": 1, "matching": 1, "limit": -1, "offset": 0}
        })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/data/api/v1/resources/singleton/ignition/general-alarm-settings",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type": "ignition/general-alarm-settings",
            "collection": "dev",
            "config": rest_of("ignition/general-alarm-settings")
        })))
        .mount(server)
        .await;
    // The cheap "has the gateway logged since the scan?" probe finds a line,
    // so the log settle wait is skipped.
    Mock::given(method("GET"))
        .and(path("/data/api/v1/logs"))
        .and(query_param("limit", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [{"timestamp": 1_792_999_999_999_i64, "loggerName": "x", "level": "INFO", "message": "m"}],
            "metadata": {"total": 1, "matching": 1, "limit": 1, "offset": 0}
        })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/data/api/v1/logs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": log_items,
            "metadata": {"total": 0, "matching": 0, "limit": 200, "offset": 0}
        })))
        .mount(server)
        .await;
}

fn verify_args(repo: &Path, json: bool) -> Vec<std::ffi::OsString> {
    let mut args: Vec<std::ffi::OsString> = Vec::new();
    if json {
        args.push("--json".into());
    }
    for a in ["deploy", "verify", "--env", "dev", "--repo"] {
        args.push(a.into());
    }
    args.push(repo.as_os_str().to_owned());
    args
}

/// Red: the declared translator is not in the REST list. The success
/// envelope carries the verdict and the process exits 6.
#[tokio::test]
async fn verify_cli_red_exits_6() {
    let (_scratch, repo, data) = local_tracer();
    let server = wiremock::MockServer::start().await;
    mount_verify_gateway(&server, &data, true, false, Vec::new()).await;
    let (_cfg, mut cmd) = verify_ign(&server.uri());
    let out = cmd
        .args(verify_args(&repo, true))
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(6),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope on stdout");
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["profile"], "dev");
    assert!(
        envelope.get("error").is_none(),
        "no error member: {envelope}"
    );
    let data = &envelope["data"];
    assert_eq!(data["ok"], false);
    assert_eq!(data["slug"], "deploy_verify_failed");
    assert_eq!(data["node"], "master");
    assert_eq!(data["project"], "ign-cli");
    let checks = data["checks"].as_array().expect("checks");
    let ids: Vec<&str> = checks.iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(
        ids,
        ["scan", "mode", "read_back", "files", "loaded", "probes"]
    );
    let read_back = &checks[2];
    assert_eq!(read_back["status"], "fail");
    assert_eq!(read_back["findings"][0]["code"], "resource_missing");
    assert_eq!(read_back["findings"][0]["subject"], NAMED_TRANSLATOR);
    // nothing on stderr but (possibly) diagnostics; never a JSON error envelope
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("\"ok\":false") && !stderr.contains("\"ok\": false"),
        "{stderr}"
    );
}

/// Green: every check agrees. `data.ok` true, `data.slug` null, exit 0.
#[tokio::test]
async fn verify_cli_green_exits_0() {
    let (_scratch, repo, data) = local_tracer();
    let server = wiremock::MockServer::start().await;
    mount_verify_gateway(&server, &data, false, false, Vec::new()).await;
    let (_cfg, mut cmd) = verify_ign(&server.uri());
    let out = cmd
        .args(verify_args(&repo, true))
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    assert_eq!(envelope["ok"], true);
    let data = &envelope["data"];
    assert_eq!(data["ok"], true);
    assert!(data["slug"].is_null());
    assert_eq!(data["resources_checked"], 2);
    assert!(
        envelope.get("error").is_none(),
        "no error member: {envelope}"
    );
}

/// Green when the REST view echoes the tree's own null members (G-22-2): a
/// pulled `ignition/cobranding` is the real case, the tracer's translator and
/// singleton carry the same shape. Null equals absent on both sides.
#[tokio::test]
async fn verify_cli_green_when_rest_echoes_file_nulls() {
    let (_scratch, repo, data) = local_tracer();
    // the shape precondition: the live tree really holds null members
    let has_null = |key: &str| {
        rest_config_raw(&data, key)
            .as_object()
            .is_some_and(|m| m.values().any(Value::is_null))
    };
    assert!(
        has_null(NAMED_TRANSLATOR) || has_null("ignition/general-alarm-settings"),
        "the live tree carries no null member; the test would pass vacuously"
    );
    let server = wiremock::MockServer::start().await;
    mount_verify_gateway(&server, &data, false, true, Vec::new()).await;
    let (_cfg, mut cmd) = verify_ign(&server.uri());
    let out = cmd
        .args(verify_args(&repo, true))
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    assert_eq!(envelope["ok"], true);
    let data = &envelope["data"];
    assert_eq!(data["ok"], true);
    assert!(data["slug"].is_null());
    assert_eq!(data["resources_checked"], 2);
    assert!(
        envelope.get("error").is_none(),
        "no error member: {envelope}"
    );
    assert!(
        out.stderr.is_empty() || !String::from_utf8_lossy(&out.stderr).contains("\"error\""),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A gateway that cannot be reached is an ordinary error envelope with the
/// network exit code, never a verdict and never exit 6.
#[test]
fn verify_cli_unreachable_gateway_is_an_error_envelope() {
    let (_scratch, repo, _data) = local_tracer();
    // port 1 is closed on any sane host
    let (_cfg, mut cmd) = verify_ign("http://127.0.0.1:1");
    let out = cmd
        .args(verify_args(&repo, true))
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(4),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty(), "an error leaves stdout empty");
    let err = stderr_json(&out);
    assert_eq!(err["ok"], false);
    assert_eq!(err["error"]["code"], "network_error");
    assert_ne!(err["error"]["code"], "deploy_verify_failed");
}

/// An unknown profile named by the target resolves through the normal chain:
/// `profile_not_found`, exit 3.
#[test]
fn verify_cli_unknown_target_profile_is_profile_not_found() {
    let (_scratch, repo, _data) = local_tracer();
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("config.toml");
    std::fs::write(
        &config,
        "active = \"other\"\n\n[profiles.other]\nurl = \"http://127.0.0.1:1\"\nauth = { token_env = \"IGNITION_TOKEN\" }\n",
    )
    .expect("config");
    let mut cmd = Command::cargo_bin("ign").expect("binary");
    cmd.env("IGNITION_CLI_CONFIG", &config)
        .env("IGNITION_TOKEN", "mock:name-key");
    for var in ["IGNITION_URL", "IGNITION_PROFILE", "IGNITION_JSON"] {
        cmd.env_remove(var);
    }
    let out = cmd
        .args(verify_args(&repo, true))
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(stderr_json(&out)["error"]["code"], "profile_not_found");
}

/// Human mode: the verdict line, six check lines in fixed order and an
/// indented finding line. A log line that names a declared resource and
/// carries a canary in its message is reported by code, never by text.
#[tokio::test]
async fn verify_cli_human_lines() {
    use serde_json::json;
    let (_scratch, repo, data) = local_tracer();
    let server = wiremock::MockServer::start().await;
    mount_verify_gateway(
        &server,
        &data,
        true,
        false,
        vec![json!({
            "timestamp": 1_792_000_000_050_i64,
            "loggerName": "NamedResourceHandler-ignition/database-translator",
            "level": "WARN",
            "message": format!(
                "Resource {NAMED_TRANSLATOR} does not have required UUID attribute, will not be loaded {VERIFY_CANARY}"
            ),
        })],
    )
    .await;
    let (_cfg, mut cmd) = verify_ign(&server.uri());
    let out = cmd
        .args(verify_args(&repo, false))
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(6),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("utf-8");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stdout.contains(VERIFY_CANARY) && !stderr.contains(VERIFY_CANARY));
    let lines: Vec<&str> = stdout
        .lines()
        .filter(|l| !l.starts_with("[profile:"))
        .collect();
    assert_eq!(
        lines[0], "verify tracer-gw (dev, mode dev): FAIL",
        "{stdout}"
    );
    let check_ids: Vec<&str> = lines[1..]
        .iter()
        .filter(|l| !l.starts_with(' '))
        .map(|l| l.split(':').next().unwrap())
        .collect();
    assert_eq!(
        check_ids,
        ["scan", "mode", "read_back", "files", "loaded", "probes"],
        "{stdout}"
    );
    assert!(
        lines.iter().any(|l| l.starts_with("scan: pass ")),
        "{stdout}"
    );
    assert!(
        lines.iter().any(|l| l.starts_with("read_back: fail ")),
        "{stdout}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("  resource_missing ignition/database-translator/POSTGRES ")),
        "{stdout}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("  unloaded_resource ignition/database-translator/POSTGRES ")),
        "{stdout}"
    );
}

// ============================================================================
// `ign deploy apply` (23-13)
// ============================================================================

#[test]
fn apply_without_yes_prints_plan() {
    let (_scratch, repo, data) = changed_tracer();
    let hash = plan_hash(&repo);
    let before = read_tree(&data);

    let (_cfg, mut cmd) = ign();
    let out = cmd
        .args(["--json", "deploy", "apply", "--env", "dev", "--repo"])
        .arg(&repo)
        .args(["--plan-sha256", &hash])
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty(), "no envelope on stdout");
    let err = stderr_json(&out);
    assert_eq!(err["error"]["code"], "confirmation_required");
    let message = err["error"]["message"].as_str().expect("message");
    assert!(message.contains("plan_sha256"), "{message}");
    assert!(message.contains(&hash), "{message}");
    assert!(message.contains(APPLY_UNIT), "{message}");
    assert!(message.contains("tracer-gw"), "{message}");
    assert!(!repo.join(".ign").exists(), "no run directory");
    assert_eq!(read_tree(&data), before, "nothing written to the host");
}

#[test]
fn apply_flags_are_exactly_these() {
    use clap::CommandFactory;
    let cli = ignition_cli::cli::Cli::command();
    let deploy = cli.find_subcommand("deploy").expect("deploy");
    let apply = deploy.find_subcommand("apply").expect("deploy apply");
    let mut longs: Vec<&str> = apply
        .get_arguments()
        .filter(|a| !a.is_global_set())
        .filter_map(|a| a.get_long())
        .filter(|l| !matches!(*l, "help" | "version"))
        .collect();
    longs.sort_unstable();
    assert_eq!(
        longs,
        [
            "allow-destructive",
            "allow-drift",
            "backup-timeout",
            "base",
            "designers",
            "designers-timeout",
            "env",
            "plan-sha256",
            "project",
            "repo",
            "skip-gwbk",
        ],
        "the flag set is the contract; a flag that skips the plan hash, the verify or the rollback is never added"
    );
    // The global confirmation flag is the only way past the guard.
    assert!(
        cli.get_arguments().any(|a| a.get_long() == Some("yes")),
        "--yes is a global flag"
    );
}

#[tokio::test]
async fn apply_applied_exit_0() {
    let (_scratch, repo, data) = apply_tracer();
    let hash = plan_hash(&repo);
    let server = wiremock::MockServer::start().await;
    mount_apply_gateway(&server, &data, Vec::new()).await;
    let (_cfg, mut cmd) = verify_ign(&server.uri());
    let out = cmd
        .args(apply_args(&repo, Some(&hash), true, true))
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("one envelope on stdout");
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["profile"], "dev");
    let data_json = &envelope["data"];
    assert_eq!(data_json["outcome"], "applied");
    assert_eq!(data_json["ok"], true);
    assert!(data_json["slug"].is_null());
    assert!(data_json["cause"].is_null());
    assert_eq!(data_json["plan_sha256"], hash);
    assert_eq!(data_json["restart_pending"], serde_json::json!([]));
    assert_eq!(data_json["restart_pending_new"], serde_json::json!([]));
    assert_eq!(data_json["skip_gwbk"], false);
    let gwbk = data_json["gwbk"].as_str().expect("a .gwbk was taken");
    assert!(gwbk.ends_with(".gwbk"), "{gwbk}");
    let written: Vec<&str> = data_json["written"]
        .as_array()
        .expect("written")
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        written.iter().any(|p| p.ends_with("POSTGRES/config.json")),
        "{written:?}"
    );
    let run_dir = PathBuf::from(data_json["run_dir"].as_str().expect("run dir"));
    assert!(run_dir.is_dir(), "the run directory exists: {run_dir:?}");
    assert!(run_dir.starts_with(repo.join(".ign")), "{run_dir:?}");
    assert!(run_dir.join(gwbk).is_file());
    // The host now holds the declared bytes: a second plan is empty.
    let (_cfg, mut plan) = ign();
    let out = plan_json(&repo, &mut plan);
    let again: Value = serde_json::from_slice(&out.stdout).expect("plan");
    assert_eq!(again["data"]["summary"]["has_changes"], false, "{again}");
    let seen = seen_requests(&server).await;
    assert!(
        seen.iter().any(|r| r == "POST /data/api/v1/scan/config"),
        "{seen:?}"
    );
}

#[tokio::test]
async fn apply_rolled_back_exit_6() {
    use serde_json::json;
    let (_scratch, repo, data) = apply_tracer();
    let hash = plan_hash(&repo);
    let before = read_tree(&data);
    let server = wiremock::MockServer::start().await;
    mount_apply_gateway(
        &server,
        &data,
        vec![json!({
            "timestamp": 1_792_999_999_000_i64,
            "loggerName": "NamedResourceHandler-ignition/database-translator",
            "level": "WARN",
            "message": format!(
                "Resource {NAMED_TRANSLATOR} does not have required UUID attribute, will not be loaded {VERIFY_CANARY}"
            ),
        })],
    )
    .await;
    let (_cfg, mut cmd) = verify_ign(&server.uri());
    let out = cmd
        .args(apply_args(&repo, Some(&hash), true, true))
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(6),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope on stdout");
    assert_eq!(envelope["ok"], true, "a verdict, not an error envelope");
    assert!(envelope.get("error").is_none());
    let data_json = &envelope["data"];
    assert_eq!(data_json["outcome"], "rolled_back");
    assert_eq!(data_json["ok"], false);
    assert_eq!(data_json["slug"], "deploy_rolled_back");
    assert_eq!(data_json["cause"], "verify_failed");
    assert!(data_json["run_id"].is_string());
    // The verdict names no log text.
    assert!(!String::from_utf8_lossy(&out.stdout).contains(VERIFY_CANARY));
    assert!(!String::from_utf8_lossy(&out.stderr).contains(VERIFY_CANARY));
    // The snapshot was restored: the configuration tree is what it was. The
    // host keeps the run's record and snapshot below `ign-deploy/` (the
    // record says `rolled_back`), which is not configuration.
    let config_of = |tree: std::collections::BTreeMap<String, Vec<u8>>| {
        tree.into_iter()
            .filter(|(path, _)| path.starts_with("config/"))
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    assert_eq!(
        config_of(read_tree(&data)),
        config_of(before),
        "the config tree equals its pre-apply listing"
    );
    let after = read_tree(&data);
    let record = after
        .iter()
        .find(|(path, _)| path.starts_with("ign-deploy/") && path.ends_with("/record.json"))
        .map(|(_, bytes)| String::from_utf8_lossy(bytes).into_owned())
        .expect("the host keeps the run record");
    assert!(record.contains("\"state\": \"rolled_back\""), "{record}");
}

// ----------------------------------------------------------------------------
// The backup check through the binary (24-08, DPAIR-01, DPAIR-02)
// ----------------------------------------------------------------------------

/// A pair: the tracer with `nodes.backup` (profile `bk`, REST only), the
/// master mock, and the backup mock, `syncs` deciding whether the backup
/// ever links and syncs. Returns what `apply` needs.
struct PairRig {
    _scratch: tempfile::TempDir,
    repo: PathBuf,
    hash: String,
    master: wiremock::MockServer,
    backup: wiremock::MockServer,
}

async fn pair_rig(syncs: bool) -> PairRig {
    let (scratch, repo, data) = apply_tracer();
    pair_target(&repo);
    let hash = plan_hash(&repo);
    let master = wiremock::MockServer::start().await;
    let backup = wiremock::MockServer::start().await;
    mount_pair_master(&master, &data).await;
    mount_pair_backup(&backup, &data, syncs).await;
    PairRig {
        _scratch: scratch,
        repo,
        hash,
        master,
        backup,
    }
}

fn run_pair_apply(rig: &PairRig, json: bool, extra: &[&str]) -> std::process::Output {
    let (_cfg, mut cmd) = pair_ign(&rig.master.uri(), &rig.backup.uri());
    cmd.args(apply_args(&rig.repo, Some(&rig.hash), json, true))
        .args(extra)
        .output()
        .expect("spawn ign")
}

#[tokio::test]
async fn apply_backup_unsynced_exit_6() {
    let rig = pair_rig(false).await;
    let out = run_pair_apply(&rig, true, &["--backup-timeout", "1"]);
    assert_eq!(
        out.status.code(),
        Some(6),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.stderr.is_empty(),
        "under --json stderr stays empty: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope on stdout");
    assert_eq!(envelope["ok"], true, "a verdict, not an error envelope");
    assert!(envelope.get("error").is_none());
    let data_json = &envelope["data"];
    assert_eq!(data_json["outcome"], "applied_backup_unsynced");
    assert_eq!(data_json["ok"], false);
    assert_eq!(data_json["slug"], "deploy_backup_unsynced");
    assert!(data_json["cause"].is_null());
    let backup = &data_json["backup"];
    assert_eq!(backup["converged"], false);
    assert_eq!(backup["reason"], "not_syncing");
    assert_eq!(backup["resync"]["ok"], true);
    assert_eq!(backup["budget_s"], 1);
    let checks = backup["checks"].as_array().expect("checks");
    let files = checks
        .iter()
        .find(|c| c["id"] == "files")
        .expect("a files check");
    assert_eq!(files["status"], "warn", "{files}");
    // The master keeps what was applied: no restore.
    assert!(data_json["run_id"].is_string());
    // The backup is read only; the master got exactly one resync.
    let backup_seen = seen_requests(&rig.backup).await;
    assert!(!backup_seen.is_empty());
    assert!(
        backup_seen.iter().all(|r| r.starts_with("GET ")),
        "the backup node is only read: {backup_seen:?}"
    );
    let master_seen = seen_requests(&rig.master).await;
    let resyncs = master_seen
        .iter()
        .filter(|r| *r == "POST /data/api/v1/redundancy/gwaction/resync")
        .count();
    assert_eq!(resyncs, 1, "{master_seen:?}");
}

#[tokio::test]
async fn apply_backup_converged_exit_0() {
    let rig = pair_rig(true).await;
    let out = run_pair_apply(&rig, true, &["--backup-timeout", "30"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    let data_json = &envelope["data"];
    assert_eq!(data_json["outcome"], "applied");
    assert_eq!(data_json["ok"], true);
    assert!(data_json["slug"].is_null());
    let backup = &data_json["backup"];
    assert_eq!(backup["converged"], true, "{backup}");
    assert!(backup["reason"].is_null());
    assert!(backup["resync"].is_null(), "no resync when it converged");
    let master_seen = seen_requests(&rig.master).await;
    assert!(
        !master_seen
            .iter()
            .any(|r| r == "POST /data/api/v1/redundancy/gwaction/resync"),
        "{master_seen:?}"
    );
    let backup_seen = seen_requests(&rig.backup).await;
    assert!(
        backup_seen.iter().all(|r| r.starts_with("GET ")),
        "{backup_seen:?}"
    );
}

#[tokio::test]
async fn apply_backup_timeout_bounds() {
    let rig = pair_rig(false).await;
    for bad in ["0", "86401"] {
        let out = run_pair_apply(&rig, true, &["--backup-timeout", bad]);
        assert_eq!(
            out.status.code(),
            Some(2),
            "--backup-timeout {bad}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stdout.is_empty());
    }
    assert_eq!(
        seen_requests(&rig.master).await,
        Vec::<String>::new(),
        "clap refuses before any request"
    );
    assert_eq!(seen_requests(&rig.backup).await, Vec::<String>::new());
}

#[tokio::test]
async fn apply_backup_human_block() {
    let rig = pair_rig(false).await;
    let out = run_pair_apply(&rig, false, &["--backup-timeout", "1"]);
    assert_eq!(
        out.status.code(),
        Some(6),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("utf-8");
    let stderr = String::from_utf8(out.stderr).expect("utf-8");
    let lines: Vec<&str> = stdout
        .lines()
        .filter(|l| !l.starts_with("[profile:"))
        .collect();
    assert_eq!(
        lines[0], "apply tracer-gw (dev, mode dev) via local: applied_backup_unsynced",
        "{stdout}"
    );
    assert!(
        lines.contains(&"backup: NOT converged (not_syncing)"),
        "{stdout}"
    );
    assert!(
        lines.iter().any(|l| l.starts_with("backup waited ")),
        "{stdout}"
    );
    assert!(
        lines.iter().any(|l| l.starts_with("backup resync sent: ")),
        "{stdout}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("  files: warn ") && l.contains("file-hash check did not run")),
        "{stdout}"
    );
    assert!(
        lines.iter().any(|l| l.starts_with("  redundancy: fail ")),
        "{stdout}"
    );
    assert!(
        stderr.contains("ign deploy apply: waiting up to 1 s for the backup node"),
        "{stderr}"
    );
}

#[tokio::test]
async fn apply_noop_exit_0() {
    let (_scratch, repo, data) = apply_tracer();
    // Make the host equal to the render: drop the extra byte.
    let config = data
        .join("config/resources/dev")
        .join(APPLY_UNIT)
        .join("config.json");
    let mut bytes = std::fs::read(&config).expect("live config");
    bytes.pop();
    std::fs::write(&config, bytes).expect("equal again");
    let hash = plan_hash(&repo);
    let server = wiremock::MockServer::start().await;
    mount_apply_gateway(&server, &data, Vec::new()).await;
    let (_cfg, mut cmd) = verify_ign(&server.uri());
    let out = cmd
        .args(apply_args(&repo, Some(&hash), true, true))
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    let data_json = &envelope["data"];
    assert_eq!(data_json["outcome"], "noop");
    assert_eq!(data_json["ok"], true);
    assert!(data_json["run_id"].is_null());
    assert!(data_json["run_dir"].is_null());
    assert!(!repo.join(".ign").exists(), "no .ign/ directory");
    let seen = seen_requests(&server).await;
    assert!(
        seen.is_empty(),
        "a no-op sends no lock, backup, scan or restart request: {seen:?}"
    );
}

#[tokio::test]
async fn apply_requires_plan_hash_when_noninteractive() {
    let (_scratch, repo, data) = apply_tracer();
    let server = wiremock::MockServer::start().await;
    mount_apply_gateway(&server, &data, Vec::new()).await;
    let before = read_tree(&data);
    // No terminal under assert_cmd; --json is given as well.
    for json in [true, false] {
        let (_cfg, mut cmd) = verify_ign(&server.uri());
        let out = cmd
            .args(apply_args(&repo, None, json, true))
            .output()
            .expect("spawn ign");
        assert_eq!(
            out.status.code(),
            Some(2),
            "json={json}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stdout.is_empty());
        let stderr = String::from_utf8_lossy(&out.stderr);
        if json {
            assert_eq!(stderr_json(&out)["error"]["code"], "invalid_input");
        } else {
            assert!(stderr.contains("invalid input"), "{stderr}");
        }
        assert!(stderr.contains("Decision C"), "json={json}: {stderr}");
    }
    assert_eq!(
        seen_requests(&server).await,
        Vec::<String>::new(),
        "no gateway contact"
    );
    assert_eq!(read_tree(&data), before);
    assert!(!repo.join(".ign").exists());
}

#[tokio::test]
async fn apply_human_output() {
    let (_scratch, repo, data) = apply_tracer();
    let hash = plan_hash(&repo);
    let server = wiremock::MockServer::start().await;
    mount_apply_gateway(&server, &data, Vec::new()).await;
    let (_cfg, mut cmd) = verify_ign(&server.uri());
    let out = cmd
        .args(apply_args(&repo, Some(&hash), false, true))
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("utf-8");
    let stderr = String::from_utf8(out.stderr).expect("utf-8");
    let lines: Vec<&str> = stdout
        .lines()
        .filter(|l| !l.starts_with("[profile:"))
        .collect();
    assert_eq!(
        lines[0], "apply tracer-gw (dev, mode dev) via local: applied",
        "{stdout}"
    );
    assert!(lines[1].starts_with("run "), "{stdout}");
    assert!(lines[2].starts_with("run directory "), "{stdout}");
    assert!(
        lines
            .iter()
            .any(|l| l.contains(" written, ") && l.ends_with(" deleted")),
        "{stdout}"
    );
    assert!(lines.iter().any(|l| l.starts_with("backup ")), "{stdout}");
    assert!(
        lines.iter().any(|l| *l == format!("plan_sha256 {hash}")),
        "{stdout}"
    );
    // Progress goes to stderr, prefixed; stdout carries none of it.
    assert!(
        stderr.lines().any(|l| l.starts_with("ign deploy apply: ")),
        "{stderr}"
    );
    assert!(stderr.contains("scan-lock taken for 176 s"), "{stderr}");
    assert!(
        stderr.contains("staging "),
        "the staging notice precedes the upload: {stderr}"
    );
    assert!(!stdout.contains("ign deploy apply:"), "{stdout}");
}

// ============================================================================
// `ign deploy apply` gate exits through the binary (23-21, DAPL-01, DAPL-02)
// ============================================================================

/// A gate refusal through the binary: the exit code and error slug, one JSON
/// value on stderr, nothing on stdout, no request to the gateway (so no lock,
/// backup or scan), no `.ign/` directory and the host tree untouched.
async fn assert_refused_before_any_write(
    out: &std::process::Output,
    exit: i32,
    slug: &str,
    server: &wiremock::MockServer,
    repo: &Path,
    data: &Path,
    before: &std::collections::BTreeMap<String, Vec<u8>>,
) {
    assert_eq!(
        out.status.code(),
        Some(exit),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty(), "a refusal prints no stdout");
    assert_eq!(stderr_json(out)["error"]["code"], slug);
    let seen = seen_requests(server).await;
    assert!(
        seen.is_empty(),
        "no lock, backup, scan or any other request: {seen:?}"
    );
    assert!(!repo.join(".ign").exists(), "no .ign/ directory");
    assert_eq!(&read_tree(data), before, "the host tree is unchanged");
}

/// Add a core copy of the `OLD` unit that `seed_old_unit_and_ledger` leaves
/// in the mode collection, so removing it would not take effect.
fn shadow_old_unit_in_core(data: &Path) {
    let file = data.join("config/resources/core/ignition/database-translator/OLD/config.json");
    std::fs::create_dir_all(file.parent().expect("parent")).expect("core unit dir");
    std::fs::write(file, b"{}").expect("core copy");
}

#[tokio::test]
async fn apply_stale_hash_exit_6() {
    let (_scratch, repo, data) = apply_tracer();
    let reviewed = plan_hash(&repo);
    // The live file moves on after the plan was reviewed.
    let config = data
        .join("config/resources/dev")
        .join(APPLY_UNIT)
        .join("config.json");
    let mut bytes = std::fs::read(&config).expect("live config");
    bytes.extend_from_slice(b"  ");
    std::fs::write(&config, bytes).expect("change the live unit again");
    assert_ne!(plan_hash(&repo), reviewed, "the plan changed");
    let before = read_tree(&data);
    let server = wiremock::MockServer::start().await;
    mount_apply_gateway(&server, &data, Vec::new()).await;

    let out = run_apply(&server, &repo, &reviewed, &[]).await;
    assert_refused_before_any_write(&out, 6, "plan_stale", &server, &repo, &data, &before).await;
    // The refusal names both hashes, so the reviewer sees what moved.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains(&reviewed), "{stderr}");
}

#[tokio::test]
async fn apply_destructive_without_flag_exit_2() {
    let (_scratch, repo, data) = apply_tracer();
    seed_old_unit_and_ledger(&data);
    let hash = plan_hash(&repo);
    let before = read_tree(&data);
    let server = wiremock::MockServer::start().await;
    mount_apply_gateway(&server, &data, Vec::new()).await;

    let out = run_apply(&server, &repo, &hash, &[]).await;
    assert_refused_before_any_write(
        &out,
        2,
        "destructive_change_refused",
        &server,
        &repo,
        &data,
        &before,
    )
    .await;
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("ignition/database-translator/OLD"),
        "the refusal names the entry"
    );

    // With the flag (and --yes) the same plan applies and the unit is gone.
    let out = run_apply(&server, &repo, &hash, &["--allow-destructive"]).await;
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    assert_eq!(envelope["data"]["outcome"], "applied");
    assert!(
        !data
            .join("config/resources/dev/ignition/database-translator/OLD/config.json")
            .exists(),
        "the removed unit was deleted from the mode collection"
    );
}

#[tokio::test]
async fn apply_shadows_removal_exit_6() {
    let (_scratch, repo, data) = apply_tracer();
    seed_old_unit_and_ledger(&data);
    shadow_old_unit_in_core(&data);
    let hash = plan_hash(&repo);
    let before = read_tree(&data);
    let server = wiremock::MockServer::start().await;
    mount_apply_gateway(&server, &data, Vec::new()).await;

    // Whatever the flags: no override exists for a removal a core copy shadows.
    for extra in [
        &[][..],
        &["--allow-destructive"][..],
        &["--allow-destructive", "--allow-drift", "reviewed in PR 12"][..],
    ] {
        let out = run_apply(&server, &repo, &hash, extra).await;
        assert_refused_before_any_write(&out, 6, "deploy_blocked", &server, &repo, &data, &before)
            .await;
    }
}

#[tokio::test]
async fn apply_drift_with_reason_records_it() {
    let (scratch, repo, data) = apply_tracer();
    // `local_tracer` left the render of the declared tree in `out`; the live
    // unit was then edited, so against that base the entry is drift.
    let base = scratch.path().join("out");
    let base_arg = base.to_str().expect("utf-8 path");
    let hash = plan_hash(&repo);
    let before = read_tree(&data);
    let server = wiremock::MockServer::start().await;
    mount_apply_gateway(&server, &data, Vec::new()).await;

    let out = run_apply(&server, &repo, &hash, &["--base", base_arg]).await;
    assert_refused_before_any_write(
        &out,
        2,
        "destructive_change_refused",
        &server,
        &repo,
        &data,
        &before,
    )
    .await;

    let reason = "reviewed in PR 12";
    let out = run_apply(
        &server,
        &repo,
        &hash,
        &["--base", base_arg, "--allow-drift", reason],
    )
    .await;
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    assert_eq!(envelope["data"]["outcome"], "applied");
    let run_dir = PathBuf::from(envelope["data"]["run_dir"].as_str().expect("run dir"));
    let run_json: Value =
        serde_json::from_slice(&std::fs::read(run_dir.join("run.json")).expect("run.json"))
            .expect("run.json is JSON");
    assert_eq!(run_json["allow_drift"], reason);
    assert_eq!(run_json["allow_destructive"], false);
}

// ============================================================================
// `ign deploy rollback` through the binary (23-14, DAPL-08, DAPL-09, DSURF-02)
// ============================================================================

#[tokio::test]
async fn rollback_after_apply_restores() {
    let (_scratch, repo, data) = apply_tracer();
    let before = config_tree(read_tree(&data));
    let server = wiremock::MockServer::start().await;
    mount_apply_gateway(&server, &data, Vec::new()).await;
    let run = apply_through_binary(&server, &repo, &[]).await;
    assert_ne!(
        config_tree(read_tree(&data)),
        before,
        "the apply changed it"
    );

    // A fresh process with no run directory: only the target file and the
    // run id remain.
    std::fs::remove_dir_all(repo.join(".ign")).expect("delete the run directories");

    let (_cfg, mut cmd) = verify_ign(&server.uri());
    let out = cmd
        .args(rollback_args(&repo, &run, true, true))
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("one envelope on stdout");
    assert_eq!(envelope["ok"], true);
    let result = &envelope["data"];
    assert_eq!(result["outcome"], "restored", "{result}");
    assert_eq!(result["ok"], true);
    assert_eq!(result["target_run_id"], run);
    assert_ne!(result["run_id"], run, "a rollback has its own run id");
    assert_eq!(
        config_tree(read_tree(&data)),
        before,
        "the configuration tree equals its pre-apply listing"
    );
    let run_dir = PathBuf::from(result["run_dir"].as_str().expect("run dir"));
    assert!(run_dir.starts_with(repo.join(".ign")), "{run_dir:?}");
    let run_json: Value =
        serde_json::from_slice(&std::fs::read(run_dir.join("run.json")).expect("run.json"))
            .expect("run.json is JSON");
    assert_eq!(run_json["kind"], "rollback");
    assert_eq!(run_json["target_run_id"], run);
    let seen = seen_requests(&server).await;
    assert!(
        seen.iter().any(|r| r == "POST /data/api/v1/scan/config"),
        "the rollback scans: {seen:?}"
    );
}

#[tokio::test]
async fn rollback_without_yes_prints_what_it_restores() {
    let (_scratch, repo, data) = apply_tracer();
    let server = wiremock::MockServer::start().await;
    mount_apply_gateway(&server, &data, Vec::new()).await;
    let run = apply_through_binary(&server, &repo, &[]).await;
    let after = read_tree(&data);
    let requests_before = seen_requests(&server).await.len();

    let (_cfg, mut cmd) = verify_ign(&server.uri());
    let out = cmd
        .args(rollback_args(&repo, &run, true, false))
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty(), "no envelope on stdout");
    let err = stderr_json(&out);
    assert_eq!(err["error"]["code"], "confirmation_required");
    let message = err["error"]["message"].as_str().expect("message");
    assert!(message.contains(&run), "names the run: {message}");
    assert!(
        message.contains("apply-gw") || message.contains("tracer-gw"),
        "{message}"
    );
    assert!(message.contains("mode dev"), "{message}");
    let restore = message
        .split("restore ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or_else(|| panic!("a restore count in: {message}"));
    assert!(restore >= 1, "{message}");
    assert!(
        message.contains("file(s) from the on-box snapshot and delete "),
        "names the delete count: {message}"
    );
    assert_eq!(read_tree(&data), after, "nothing written to the host");
    assert_eq!(
        seen_requests(&server).await.len(),
        requests_before,
        "the refusal sent no lock, scan or other request"
    );
}

#[tokio::test]
async fn rollback_bad_run_id_exit_2() {
    let (_scratch, repo, data) = apply_tracer();
    let before = read_tree(&data);
    let server = wiremock::MockServer::start().await;
    mount_apply_gateway(&server, &data, Vec::new()).await;
    for bad in ["../x", "", "a/b"] {
        let (_cfg, mut cmd) = verify_ign(&server.uri());
        let out = cmd
            .args(rollback_args(&repo, bad, true, true))
            .output()
            .expect("spawn ign");
        assert_eq!(
            out.status.code(),
            Some(2),
            "run {bad:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stdout.is_empty());
        assert_eq!(
            stderr_json(&out)["error"]["code"],
            "invalid_input",
            "{bad:?}"
        );
    }
    assert_eq!(seen_requests(&server).await, Vec::<String>::new());
    assert_eq!(read_tree(&data), before);
    assert!(!repo.join(".ign").exists());
}

#[tokio::test]
async fn journal_blocks_cli_then_recovers() {
    let (_scratch, repo, data) = apply_tracer();
    let before = config_tree(read_tree(&data));
    let server = wiremock::MockServer::start().await;
    mount_apply_gateway(&server, &data, Vec::new()).await;
    let run = apply_through_binary(&server, &repo, &[]).await;
    plant_journal(&data, &run);
    let planted = read_tree(&data);
    let requests = seen_requests(&server).await.len();
    let command = format!("ign deploy rollback --run {run} --env dev");

    // `deploy plan` refuses.
    let (_cfg, mut cmd) = ign();
    let out = plan_json(&repo, &mut cmd);
    assert_eq!(
        out.status.code(),
        Some(6),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty());
    let err = stderr_json(&out);
    assert_eq!(err["error"]["code"], "deploy_journal_present");
    let message = err["error"]["message"].as_str().expect("message");
    assert!(message.contains(&command), "{message}");

    // `deploy apply` refuses (Decision C is met by the flag; the journal is
    // checked before the hash).
    let out = run_apply(&server, &repo, &"0".repeat(64), &[]).await;
    assert_eq!(out.status.code(), Some(6));
    let err = stderr_json(&out);
    assert_eq!(err["error"]["code"], "deploy_journal_present");
    assert!(
        err["error"]["message"]
            .as_str()
            .expect("message")
            .contains(&command)
    );

    // A rollback of another run refuses the same way.
    let (_cfg, mut cmd) = verify_ign(&server.uri());
    let out = cmd
        .args(rollback_args(&repo, "20261016T000000Z-aaaaaa", true, true))
        .output()
        .expect("spawn ign");
    assert_eq!(out.status.code(), Some(6));
    let err = stderr_json(&out);
    assert_eq!(err["error"]["code"], "deploy_journal_present");
    assert!(
        err["error"]["message"]
            .as_str()
            .expect("message")
            .contains(&command)
    );

    assert_eq!(read_tree(&data), planted, "the refusals wrote nothing");
    assert_eq!(
        seen_requests(&server).await.len(),
        requests,
        "and sent nothing to the gateway"
    );

    // The recovery command the message printed recovers the run.
    let (_cfg, mut cmd) = verify_ign(&server.uri());
    let out = cmd
        .args(rollback_args(&repo, &run, true, true))
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    assert_eq!(envelope["data"]["outcome"], "restored", "{envelope}");
    assert_eq!(envelope["data"]["target_run_id"], run);
    assert_eq!(config_tree(read_tree(&data)), before);
    assert!(
        !data.join("ign-deploy/tracer-gw/dev/journal.json").exists(),
        "the journal is gone"
    );

    // The next plan exits 0 and sees the pre-apply difference again.
    let (_cfg, mut cmd) = ign();
    let out = plan_json(&repo, &mut cmd);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let plan: Value = serde_json::from_slice(&out.stdout).expect("plan");
    assert_eq!(plan["data"]["summary"]["has_changes"], true, "{plan}");
}

#[tokio::test]
async fn rollback_live_changed_exit_6() {
    let (_scratch, repo, data) = apply_tracer();
    let server = wiremock::MockServer::start().await;
    mount_apply_gateway(&server, &data, Vec::new()).await;
    let run = apply_through_binary(&server, &repo, &[]).await;
    edit_live_unit(&data);
    let edited = read_tree(&data);
    let requests = seen_requests(&server).await.len();

    let (_cfg, mut cmd) = verify_ign(&server.uri());
    let out = cmd
        .args(rollback_args(&repo, &run, true, true))
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(6),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty(), "a refusal prints no stdout");
    let err = stderr_json(&out);
    assert_eq!(err["error"]["code"], "deploy_live_changed");
    assert!(
        err["error"]["message"]
            .as_str()
            .expect("message")
            .contains("POSTGRES"),
        "names the changed file: {err}"
    );
    assert_eq!(read_tree(&data), edited, "nothing was restored");
    assert_eq!(
        seen_requests(&server).await.len(),
        requests,
        "no lock or scan request"
    );
}

#[tokio::test]
async fn rollback_discard_live_changes_exit_0() {
    let (_scratch, repo, data) = apply_tracer();
    let before = config_tree(read_tree(&data));
    let server = wiremock::MockServer::start().await;
    mount_apply_gateway(&server, &data, Vec::new()).await;
    let run = apply_through_binary(&server, &repo, &[]).await;
    edit_live_unit(&data);

    let (_cfg, mut cmd) = verify_ign(&server.uri());
    let out = cmd
        .args(rollback_args(&repo, &run, true, true))
        .arg("--discard-live-changes")
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    assert_eq!(envelope["data"]["outcome"], "restored", "{envelope}");
    let discarded: Vec<&str> = envelope["data"]["discarded_live_changes"]
        .as_array()
        .expect("discarded_live_changes")
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        discarded.iter().any(|p| p.contains("POSTGRES")),
        "{discarded:?}"
    );
    assert_eq!(
        config_tree(read_tree(&data)),
        before,
        "restored over the edit"
    );
    // The discarded paths are recorded in the rollback's run directory.
    let run_dir = PathBuf::from(envelope["data"]["run_dir"].as_str().expect("run dir"));
    let run_json: Value =
        serde_json::from_slice(&std::fs::read(run_dir.join("run.json")).expect("run.json"))
            .expect("run.json is JSON");
    assert!(
        run_json["discarded_live_changes"]
            .as_array()
            .is_some_and(|a| !a.is_empty()),
        "{run_json}"
    );
}

#[tokio::test]
async fn rollback_human_output() {
    let (_scratch, repo, data) = apply_tracer();
    let server = wiremock::MockServer::start().await;
    mount_apply_gateway(&server, &data, Vec::new()).await;
    let run = apply_through_binary(&server, &repo, &[]).await;

    let (_cfg, mut cmd) = verify_ign(&server.uri());
    let out = cmd
        .args(rollback_args(&repo, &run, false, true))
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("utf-8");
    let stderr = String::from_utf8(out.stderr).expect("utf-8");
    let lines: Vec<&str> = stdout
        .lines()
        .filter(|l| !l.starts_with("[profile:"))
        .collect();
    assert_eq!(
        lines[0], "rollback tracer-gw (dev, mode dev): restored",
        "{stdout}"
    );
    assert_eq!(lines[1], format!("rolled back run {run}"), "{stdout}");
    assert!(lines[2].starts_with("run "), "{stdout}");
    assert!(lines[3].starts_with("run directory "), "{stdout}");
    assert!(
        lines
            .iter()
            .any(|l| l.contains(" restored, ") && l.ends_with(" deleted")),
        "{stdout}"
    );
    // Progress rides stderr under its own prefix; stdout carries none of it.
    assert!(
        stderr
            .lines()
            .any(|l| l.starts_with("ign deploy rollback: ")),
        "{stderr}"
    );
    assert!(!stderr.contains("ign deploy apply:"), "{stderr}");
    assert!(stderr.contains("scan-lock taken for 176 s"), "{stderr}");
    assert!(stderr.contains("snapshot restored"), "{stderr}");
    assert!(!stdout.contains("ign deploy rollback:"), "{stdout}");
}

#[tokio::test]
async fn journal_recovery_clears_a_run_that_changed_nothing() {
    use ignition_core::deploy::apply::journal::JournalPhase;
    let (_scratch, repo, data) = apply_tracer();
    let before = config_tree(read_tree(&data));
    let server = wiremock::MockServer::start().await;
    mount_apply_gateway(&server, &data, Vec::new()).await;
    let run = apply_through_binary(&server, &repo, &[]).await;
    // Put the host back to its pre-apply state, then leave the journal a run
    // killed before its first move would leave (D-10).
    let (_cfg, mut cmd) = verify_ign(&server.uri());
    let out = cmd
        .args(rollback_args(&repo, &run, true, true))
        .output()
        .expect("spawn ign");
    assert_eq!(out.status.code(), Some(0));
    plant_journal_at(&data, &run, JournalPhase::Ready);
    let journal = data.join("ign-deploy/tracer-gw/dev/journal.json");
    assert!(journal.is_file());
    let requests = seen_requests(&server).await.len();

    let (_cfg, mut cmd) = verify_ign(&server.uri());
    let out = cmd
        .args(rollback_args(&repo, &run, false, true))
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("utf-8");
    assert!(
        stdout.contains("tracer-gw (dev, mode dev): cleared"),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("cleared interrupted run {run}")),
        "{stdout}"
    );
    assert!(!journal.exists(), "the journal is removed");
    assert_eq!(
        config_tree(read_tree(&data)),
        before,
        "nothing was restored"
    );
    let seen = seen_requests(&server).await;
    assert!(
        !seen[requests..].iter().any(|r| r.contains("scan")),
        "a cleared run is not scanned: {seen:?}"
    );
}

// ============================================================================
// `deploy verify --node` (24-09, DPAIR-01, DPAIR-03, D-12)
// ============================================================================

/// A verified pair: the tracer with `nodes.backup` (profile `bk`) whose
/// `local:` data dir holds a copy of the master's live files, the master mock
/// of the verify tests and a backup mock that is linked and settled.
struct VerifyPair {
    _scratch: tempfile::TempDir,
    repo: PathBuf,
    backup_data: PathBuf,
    master: wiremock::MockServer,
    backup: wiremock::MockServer,
}

async fn verify_pair() -> VerifyPair {
    verify_pair_with(&[]).await
}

/// Make the tracer's POSTGRES config reference `refs` (provider, name) and
/// put the new render in the live tree of `data`, as `local_tracer` does.
fn reference_secrets(scratch: &Path, repo: &Path, data: &Path, refs: &[(&str, &str)]) {
    if refs.is_empty() {
        return;
    }
    let config = repo
        .join("gateways/tracer-gw/resources")
        .join(NAMED_TRANSLATOR)
        .join("config.json");
    let mut value: Value =
        serde_json::from_slice(&std::fs::read(&config).expect("config")).expect("json");
    for (index, (provider, name)) in refs.iter().enumerate() {
        value[format!("cred{index}")] = serde_json::json!({
            "type": "Referenced",
            "data": {"providerName": provider, "secretName": name}
        });
    }
    std::fs::write(&config, serde_json::to_vec_pretty(&value).expect("json")).expect("write");
    let out_dir = scratch.join("out-refs");
    let (_cfg, mut cmd) = ign();
    let out = cmd
        .args(["deploy", "render", "--env", "dev", "--repo"])
        .arg(repo)
        .arg("--out")
        .arg(&out_dir)
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let live = data.join("config/resources/dev");
    copy_dir(&out_dir, &live);
    std::fs::remove_file(live.join("render.json")).expect("drop render.json");
}

/// `GET /data/api/v1/secret-providers/<provider>/secrets` answers `status`
/// with `body` (names only, C-P7).
async fn mount_secret_names(
    server: &wiremock::MockServer,
    provider: &str,
    status: u16,
    body: &str,
) {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, ResponseTemplate};
    Mock::given(method("GET"))
        .and(path(format!(
            "/data/api/v1/secret-providers/{provider}/secrets"
        )))
        .respond_with(
            ResponseTemplate::new(status).set_body_raw(body.to_string(), "application/json"),
        )
        .mount(server)
        .await;
}

fn names_body(names: &[&str]) -> String {
    serde_json::json!({ "secrets": names }).to_string()
}

async fn verify_pair_with(refs: &[(&str, &str)]) -> VerifyPair {
    verify_pair_seeded(refs, None).await
}

/// [`verify_pair_with`] whose backup carries `canary` where a backup could
/// leak it (24-09): a backup-only resource unit, the status body and a log
/// entry of an unrelated logger.
async fn verify_pair_seeded(refs: &[(&str, &str)], canary: Option<&str>) -> VerifyPair {
    let (scratch, repo, data) = local_tracer();
    reference_secrets(scratch.path(), &repo, &data, refs);
    let backup_data = scratch.path().join("data-bk");
    copy_dir(&data, &backup_data);
    let target = repo.join("targets/dev.yaml");
    let mut yaml = std::fs::read_to_string(&target).expect("target file");
    yaml.push_str(&format!(
        "  backup:\n    profile: bk\n    local:\n      data_dir: '{}'\n",
        backup_data.display()
    ));
    std::fs::write(&target, yaml).expect("target file with a backup");
    let master = wiremock::MockServer::start().await;
    let backup = wiremock::MockServer::start().await;
    mount_verify_gateway(&master, &data, false, false, Vec::new()).await;
    if let Some(canary) = canary {
        seed_backup_leak(&backup_data, canary);
    }
    mount_pair_backup_seeded(&backup, &data, true, canary).await;
    VerifyPair {
        _scratch: scratch,
        repo,
        backup_data,
        master,
        backup,
    }
}

fn run_verify_node(pair: &VerifyPair, node: &str, json: bool) -> std::process::Output {
    let (_cfg, mut cmd) = pair_ign(&pair.master.uri(), &pair.backup.uri());
    cmd.args(verify_args(&pair.repo, json))
        .args(["--node", node])
        .output()
        .expect("spawn ign")
}

fn assert_only_gets(seen: &[String], what: &str) {
    assert!(
        seen.iter().all(|r| r.starts_with("GET ")),
        "{what} is only read: {seen:?}"
    );
}

/// The default and `--node master` print what `verify` printed before Phase
/// 24: the same key set, node `master`, the six checks, no `backup` and no
/// `secrets` key (T-24-34).
#[tokio::test]
async fn verify_cli_node_master_unchanged() {
    let (_scratch, repo, data) = local_tracer();
    let server = wiremock::MockServer::start().await;
    mount_verify_gateway(&server, &data, false, false, Vec::new()).await;
    let mut outputs = Vec::new();
    for extra in [&[][..], &["--node", "master"][..]] {
        let (_cfg, mut cmd) = verify_ign(&server.uri());
        let out = cmd
            .args(verify_args(&repo, true))
            .args(extra)
            .output()
            .expect("spawn ign");
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        outputs.push(out.stdout);
    }
    assert_eq!(
        outputs[0], outputs[1],
        "the explicit default is the default"
    );
    let envelope: Value = serde_json::from_slice(&outputs[0]).expect("envelope");
    assert_eq!(envelope["profile"], "dev");
    let data = envelope["data"].as_object().expect("data object");
    let mut keys: Vec<&str> = data.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "checks",
            "env",
            "gateway",
            "mode",
            "node",
            "ok",
            "project",
            "resources_checked",
            "slug",
            "warnings"
        ]
    );
    assert_eq!(data["node"], "master");
    let ids: Vec<&str> = data["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        ["scan", "mode", "read_back", "files", "loaded", "probes"]
    );
}

/// `--node backup`: only the backup checks, `data.node` `backup`, the
/// envelope names the backup's profile, and nothing reaches the master.
#[tokio::test]
async fn verify_cli_node_backup_green() {
    let pair = verify_pair().await;
    let out = run_verify_node(&pair, "backup", true);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["profile"], "bk");
    let data = &envelope["data"];
    assert_eq!(data["node"], "backup");
    assert_eq!(data["ok"], true);
    assert!(data["slug"].is_null());
    assert_eq!(data["checks"], serde_json::json!([]));
    let backup = &data["backup"];
    assert_eq!(backup["converged"], true, "{backup}");
    assert!(backup["reason"].is_null());
    assert_eq!(backup["budget_s"], 0);
    assert_eq!(backup["waited_s"], 0);
    assert!(backup["resync"].is_null());
    let checks = backup["checks"].as_array().expect("backup checks");
    let ids: Vec<&str> = checks.iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["redundancy", "mode", "read_back", "files"]);
    for check in checks {
        assert_eq!(check["status"], "pass", "{check}");
    }
    assert!(backup["files_checked"].as_u64().unwrap() > 0);
    // Verify never waits, resyncs or writes.
    assert_only_gets(&seen_requests(&pair.backup).await, "the backup");
    assert_only_gets(&seen_requests(&pair.master).await, "the master");
}

/// One backup file differs from the master's live copy by a byte.
#[tokio::test]
async fn verify_cli_node_backup_red_files() {
    let pair = verify_pair().await;
    let file = pair
        .backup_data
        .join("config/resources/dev")
        .join(NAMED_TRANSLATOR)
        .join("config.json");
    let mut bytes = std::fs::read(&file).expect("backup file");
    bytes.push(b' ');
    std::fs::write(&file, bytes).expect("change the backup file");
    let out = run_verify_node(&pair, "backup", true);
    assert_eq!(
        out.status.code(),
        Some(6),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    assert_eq!(envelope["ok"], true);
    assert!(envelope.get("error").is_none());
    let data = &envelope["data"];
    assert_eq!(data["ok"], false);
    assert_eq!(data["slug"], "deploy_verify_failed");
    assert_eq!(data["backup"]["converged"], false);
    assert_eq!(data["backup"]["reason"], "content_differs");
    let files = data["backup"]["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .find(|c| c["id"] == "files")
        .expect("files check");
    assert_eq!(files["status"], "fail");
    assert_eq!(files["findings"][0]["code"], "file_differs");
    assert!(
        files["findings"][0]["subject"]
            .as_str()
            .unwrap()
            .ends_with("database-translator/POSTGRES/config.json"),
        "{files}"
    );
    assert_only_gets(&seen_requests(&pair.backup).await, "the backup");
}

/// `--node backup` and `--node both` need `nodes.backup`: `invalid_input`,
/// exit 2, before any gateway is contacted.
#[tokio::test]
async fn verify_cli_node_backup_without_backup_is_invalid_input() {
    let (_scratch, repo, data) = local_tracer();
    let server = wiremock::MockServer::start().await;
    mount_verify_gateway(&server, &data, false, false, Vec::new()).await;
    for node in ["backup", "both"] {
        let (_cfg, mut cmd) = verify_ign(&server.uri());
        let out = cmd
            .args(verify_args(&repo, true))
            .args(["--node", node])
            .output()
            .expect("spawn ign");
        assert_eq!(
            out.status.code(),
            Some(2),
            "--node {node}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stdout.is_empty());
        assert_eq!(stderr_json(&out)["error"]["code"], "invalid_input");
    }
    assert_eq!(seen_requests(&server).await, Vec::<String>::new());
}

/// A word that is not a node is a usage error.
#[test]
fn verify_cli_node_rejects_an_unknown_word() {
    let (_scratch, repo, _data) = local_tracer();
    let (_cfg, mut cmd) = verify_ign("http://127.0.0.1:1");
    let out = cmd
        .args(verify_args(&repo, true))
        .args(["--node", "neither"])
        .output()
        .expect("spawn ign");
    assert_eq!(out.status.code(), Some(2));
}

/// Both runs of the green and red cases wrote nothing: the master and the
/// backup each saw GET requests only (T-24-33).
#[tokio::test]
async fn verify_cli_node_is_read_only() {
    let pair = verify_pair().await;
    let out = run_verify_node(&pair, "both", true);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    let master_seen = seen_requests(&pair.master).await;
    assert!(!master_seen.is_empty());
    assert_only_gets(&master_seen, "the master");
    let backup_seen = seen_requests(&pair.backup).await;
    assert!(!backup_seen.is_empty());
    assert_only_gets(&backup_seen, "the backup");
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    assert_eq!(envelope["profile"], "dev");
    assert_eq!(envelope["data"]["node"], "both");
    assert_eq!(envelope["data"]["checks"].as_array().unwrap().len(), 6);
    assert_eq!(envelope["data"]["backup"]["converged"], true);
}

/// Human mode prints the backup block of `--node backup` with the verify
/// wording: no wait or resync lines, the four check lines.
#[tokio::test]
async fn verify_cli_node_backup_human_lines() {
    let pair = verify_pair().await;
    let out = run_verify_node(&pair, "backup", false);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("utf-8");
    let lines: Vec<&str> = stdout
        .lines()
        .filter(|l| !l.starts_with("[profile:"))
        .collect();
    assert_eq!(
        lines[0], "verify tracer-gw (dev, mode dev): PASS",
        "{stdout}"
    );
    assert!(lines.contains(&"backup: converged"), "{stdout}");
    assert!(!stdout.contains("backup waited"), "{stdout}");
    for id in ["redundancy", "mode", "read_back", "files"] {
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with(&format!("  {id}: pass "))),
            "{id}: {stdout}"
        );
    }
}

/// `--node both` fails when a referenced secret name is missing on the
/// backup: exit 6 after the envelope, the missing provider/name pair in
/// stdout, stderr empty under `--json`; human mode names the node and pair.
#[tokio::test]
async fn verify_cli_node_both_red_exits_6() {
    let pair = verify_pair_with(&[("cap24-file", "CAP24-Name")]).await;
    mount_secret_names(
        &pair.master,
        "cap24-file",
        200,
        &names_body(&["CAP24-Name"]),
    )
    .await;
    mount_secret_names(&pair.backup, "cap24-file", 200, &names_body(&["Other"])).await;

    let out = run_verify_node(&pair, "both", true);
    assert_eq!(
        out.status.code(),
        Some(6),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.stderr.is_empty(),
        "under --json stderr stays empty: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    assert_eq!(envelope["ok"], true, "a verdict, not an error envelope");
    assert!(envelope.get("error").is_none());
    let data = &envelope["data"];
    assert_eq!(data["node"], "both");
    assert_eq!(data["ok"], false);
    assert_eq!(data["slug"], "deploy_verify_failed");
    assert_eq!(data["backup"]["converged"], true);
    let nodes = data["secrets"]["nodes"].as_array().expect("secret nodes");
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0]["node"], "master");
    assert_eq!(nodes[0]["status"], "pass");
    assert_eq!(nodes[0]["providers_checked"], 1);
    assert_eq!(nodes[1]["node"], "backup");
    assert_eq!(nodes[1]["status"], "fail");
    assert_eq!(
        nodes[1]["missing"],
        serde_json::json!([{"provider": "cap24-file", "name": "CAP24-Name"}])
    );
    assert_eq!(nodes[1]["unverifiable"], serde_json::json!([]));
    // Names only: the value never travels, and nothing was written.
    assert_only_gets(&seen_requests(&pair.master).await, "the master");
    assert_only_gets(&seen_requests(&pair.backup).await, "the backup");

    let out = run_verify_node(&pair, "both", false);
    assert_eq!(out.status.code(), Some(6));
    let stdout = String::from_utf8(out.stdout).expect("utf-8");
    let lines: Vec<&str> = stdout
        .lines()
        .filter(|l| !l.starts_with("[profile:"))
        .collect();
    assert_eq!(
        lines[0], "verify tracer-gw (dev, mode dev): FAIL",
        "{stdout}"
    );
    assert!(
        lines.contains(&"secrets on master: pass (1 providers)"),
        "{stdout}"
    );
    assert!(
        lines.contains(&"secrets on backup: fail (1 providers)"),
        "{stdout}"
    );
    assert!(
        lines.contains(&"  missing: cap24-file/CAP24-Name"),
        "{stdout}"
    );
}

/// A provider that answers 501 or 500 cannot list: named `unverifiable`,
/// status `warn`, exit 0 (K7).
#[tokio::test]
async fn verify_cli_node_both_unverifiable_provider_is_a_warning() {
    let pair = verify_pair_with(&[("cap24-remote", "Anything")]).await;
    mount_secret_names(
        &pair.master,
        "cap24-remote",
        501,
        "<html>Not Implemented</html>",
    )
    .await;
    mount_secret_names(
        &pair.backup,
        "cap24-remote",
        500,
        "<html>Server Error</html>",
    )
    .await;
    let out = run_verify_node(&pair, "both", true);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    assert_eq!(envelope["data"]["ok"], true);
    for node in envelope["data"]["secrets"]["nodes"].as_array().unwrap() {
        assert_eq!(node["status"], "warn", "{node}");
        assert_eq!(node["unverifiable"], serde_json::json!(["cap24-remote"]));
    }
    let out = run_verify_node(&pair, "both", false);
    let stdout = String::from_utf8(out.stdout).expect("utf-8");
    assert!(
        stdout.contains("secrets on backup: warn (1 providers)"),
        "{stdout}"
    );
    assert!(stdout.contains("  unverifiable: cap24-remote"), "{stdout}");
}

/// The default and `--node backup` list only what they run: the master's
/// secret listing is not asked unless the master is verified together with
/// the secrets (`both`), and `--node master` asks for no listing at all.
#[tokio::test]
async fn verify_cli_node_secret_listing_follows_the_node() {
    let pair = verify_pair_with(&[("cap24-file", "CAP24-Name")]).await;
    mount_secret_names(&pair.master, "cap24-file", 200, &names_body(&[])).await;
    mount_secret_names(
        &pair.backup,
        "cap24-file",
        200,
        &names_body(&["CAP24-Name"]),
    )
    .await;
    let listing = "GET /data/api/v1/secret-providers/cap24-file/secrets";

    let out = run_verify_node(&pair, "master", true);
    assert_eq!(out.status.code(), Some(0));
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    assert!(envelope["data"].get("secrets").is_none());
    assert!(
        !seen_requests(&pair.master)
            .await
            .iter()
            .any(|r| r == listing)
    );
    assert!(
        !seen_requests(&pair.backup)
            .await
            .iter()
            .any(|r| r == listing)
    );

    let out = run_verify_node(&pair, "backup", true);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    let nodes = envelope["data"]["secrets"]["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["node"], "backup");
    assert!(
        !seen_requests(&pair.master)
            .await
            .iter()
            .any(|r| r == listing)
    );
    assert!(
        seen_requests(&pair.backup)
            .await
            .iter()
            .any(|r| r == listing)
    );
}

// ============================================================================
// The secret canary over apply and rollback (23-15, DSURF-03)
// ============================================================================

/// The 23-15 canary. It is seeded wherever the gateway or the host could hand
/// a secret to the runner: a secret field of the live mode-collection
/// resources the apply replaces, the `.gwbk` bytes the gateway serves, a log
/// entry of an unrelated logger and the error body of an optional route.
const CANARY_APPLY: &str = "IGN-CANARY-23-15";

/// The two run-directory entries that legitimately hold secrets (D-05): the
/// pre-apply `snapshot/` subtree (copies of the gateway's files) and the
/// `<run-id>.gwbk` backup. Everything else a run writes (`run.json`,
/// `result.json`, `plan.json`, `events.ndjson`) is value-free and is scanned.
/// The exemption is not a gap: [`assert_exempt_private`] asserts that these
/// entries exist, are owner-only on unix and are ignored by git, which is the
/// reason they may hold secrets.
const CANARY_EXEMPT: &[&str] = &["snapshot", "*.gwbk"];

/// Whether `rel` (a path below the run directory) is named by
/// [`CANARY_EXEMPT`]. A bare name matches the top-level entry and everything
/// below it; `*.suffix` matches a top-level file with that suffix.
fn is_canary_exempt(rel: &Path) -> bool {
    let mut parts = rel.components();
    let Some(first) = parts.next() else {
        return false;
    };
    let first = first.as_os_str().to_string_lossy();
    let is_top_level_file = parts.next().is_none();
    CANARY_EXEMPT
        .iter()
        .any(|pattern| match pattern.strip_prefix('*') {
            Some(suffix) => is_top_level_file && first.ends_with(suffix),
            None => first == *pattern,
        })
}

/// Every file below `dir` (recursive), as paths relative to `dir`.
fn files_relative(dir: &Path) -> Vec<PathBuf> {
    fn walk(root: &Path, dir: &Path, found: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("read_dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                walk(root, &path, found);
            } else {
                found.push(path.strip_prefix(root).expect("below root").to_path_buf());
            }
        }
    }
    let mut found = Vec::new();
    walk(dir, dir, &mut found);
    found.sort();
    found
}

/// The files of `run_dir` that contain `canary`, skipping exactly the entries
/// [`CANARY_EXEMPT`] names. A scan that walked nothing would pass vacuously,
/// so the caller also asserts the files it expects exist.
fn scan_run_dir_for(canary: &str, run_dir: &Path) -> Vec<PathBuf> {
    files_relative(run_dir)
        .into_iter()
        .filter(|rel| !is_canary_exempt(rel))
        .filter(|rel| {
            std::fs::read(run_dir.join(rel))
                .expect("read run-dir file")
                .windows(canary.len())
                .any(|w| w == canary.as_bytes())
        })
        .collect()
}

/// The exempt entries of an applied run's directory exist, are private and
/// are ignored by git. `repo` is the git repository the run lives in.
fn assert_exempt_private(repo: &Path, run_dir: &Path) {
    let snapshot = run_dir.join("snapshot");
    assert!(snapshot.is_dir(), "the exempt snapshot/ exists");
    let exempt: Vec<PathBuf> = files_relative(run_dir)
        .into_iter()
        .filter(|rel| is_canary_exempt(rel))
        .collect();
    assert!(
        exempt
            .iter()
            .any(|rel| rel.extension().is_some_and(|e| e == "gwbk")),
        "the exempt .gwbk exists: {exempt:?}"
    );
    assert!(
        exempt.iter().any(|rel| rel.starts_with("snapshot")),
        "the snapshot holds files: {exempt:?}"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| std::fs::metadata(p).expect("stat").permissions().mode() & 0o777;
        // Every exempt file is owner-only, below a run directory only the
        // owner can enter. (The directories inside `snapshot/` keep the
        // umask's mode; the 0700 run directory above them is the barrier.)
        assert_eq!(mode(run_dir), 0o700, "the run directory is 0700");
        for rel in &exempt {
            assert_eq!(mode(&run_dir.join(rel)), 0o600, "{rel:?} is 0600");
        }
    }
    for rel in std::iter::once(PathBuf::from("snapshot")).chain(exempt) {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["check-ignore", "-q", "--"])
            .arg(run_dir.join(&rel))
            .status()
            .expect("git check-ignore");
        assert_eq!(
            status.code(),
            Some(0),
            "git ignores {rel:?} (exit 0 from `git check-ignore -q`)"
        );
    }
}

/// Every run directory below `repo/.ign/deploy/<gateway>/`.
fn run_dirs(repo: &Path) -> Vec<PathBuf> {
    let root = repo.join(".ign/deploy");
    let Ok(gateways) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut dirs = Vec::new();
    for gateway in gateways {
        for run in std::fs::read_dir(gateway.expect("gateway dir").path()).expect("read_dir") {
            dirs.push(run.expect("run dir").path());
        }
    }
    dirs.sort();
    dirs
}

/// The canary is on neither stream of `out`.
fn assert_streams_canary_free(out: &std::process::Output, what: &str) {
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains(CANARY_APPLY),
        "canary on stdout: {what}"
    );
    assert!(
        !String::from_utf8_lossy(&out.stderr).contains(CANARY_APPLY),
        "canary on stderr: {what}"
    );
}

/// The canary is on neither stream and in no scanned file of any run
/// directory below `repo`. Returns the number of run directories scanned.
fn assert_canary_free(out: &std::process::Output, repo: &Path, what: &str) -> usize {
    assert_streams_canary_free(out, what);
    let dirs = run_dirs(repo);
    for dir in &dirs {
        let leaks = scan_run_dir_for(CANARY_APPLY, dir);
        assert!(leaks.is_empty(), "canary in {leaks:?} of {dir:?}: {what}");
        for file in ["run.json", "events.ndjson"] {
            assert!(dir.join(file).is_file(), "{file} was scanned in {dir:?}");
        }
    }
    dirs.len()
}

/// `apply_tracer()` whose live `POSTGRES` and alarm-settings files carry the
/// canary in secret fields. The apply replaces both, so the pre-images (and
/// only the pre-images, in the exempt `snapshot/`) hold it afterwards.
fn canary_tracer() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let (scratch, repo, data) = apply_tracer();
    let live = data.join("config/resources/dev");
    std::fs::write(
        live.join(APPLY_UNIT).join("config.json"),
        format!(r#"{{"x":{{"type":"Embedded","data":{{"ciphertext":"{CANARY_APPLY}"}}}}}}"#),
    )
    .expect("seed embedded secret");
    std::fs::write(
        live.join("ignition/general-alarm-settings/config.json"),
        format!(r#"{{"password":"{CANARY_APPLY}"}}"#),
    )
    .expect("seed password");
    (scratch, repo, data)
}

/// A log entry of an unrelated logger whose message holds the canary.
fn canary_log_items() -> Vec<Value> {
    vec![serde_json::json!({
        "timestamp": 1_792_999_999_000_i64,
        "loggerName": "Unrelated.Logger",
        "level": "WARN",
        "message": format!("connection refused for password={CANARY_APPLY}"),
    })]
}

/// Mount the canary-seeded apply gateway (`.gwbk`, logs, restart route).
async fn mount_canary_gateway(server: &wiremock::MockServer, data: &Path) {
    mount_apply_gateway_seeded(server, data, canary_log_items(), Some(CANARY_APPLY)).await;
}

#[tokio::test]
async fn apply_canary() {
    let (_scratch, repo, data) = canary_tracer();
    let hash = plan_hash(&repo);
    let server = wiremock::MockServer::start().await;
    mount_canary_gateway(&server, &data).await;

    let out = run_apply(&server, &repo, &hash, &[]).await;
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.stderr.is_empty(),
        "a --json apply leaves stderr silent: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("one envelope on stdout");
    assert_eq!(envelope["data"]["outcome"], "applied", "{envelope}");
    let run_dir = PathBuf::from(envelope["data"]["run_dir"].as_str().expect("run dir"));

    // The seeds were real: the host held the canary before the apply, and the
    // gateway served it in the .gwbk the run kept.
    let gwbk = run_dir.join(envelope["data"]["gwbk"].as_str().expect("gwbk"));
    assert!(
        std::fs::read(&gwbk)
            .expect("gwbk")
            .windows(CANARY_APPLY.len())
            .any(|w| w == CANARY_APPLY.as_bytes()),
        "the .gwbk carries the canary, so the scan skips something real"
    );
    let pre_image = run_dir
        .join("snapshot/files")
        .join(format!("{APPLY_UNIT}/config.json"));
    assert!(
        std::fs::read_to_string(&pre_image)
            .expect("pre-image")
            .contains(CANARY_APPLY),
        "the snapshot holds the replaced secret"
    );
    assert!(
        !std::fs::read_to_string(
            data.join("config/resources/dev")
                .join(APPLY_UNIT)
                .join("config.json")
        )
        .expect("live")
        .contains(CANARY_APPLY),
        "the apply replaced the secret field"
    );

    // Nothing else holds it: both streams and every non-exempt file.
    let seen = seen_requests(&server).await;
    for route in [
        "GET /data/api/v1/backup",
        "GET /data/api/v1/restart-tasks/pending",
        "GET /data/api/v1/logs",
    ] {
        assert!(
            seen.iter().any(|r| r == route),
            "the seeded route {route} was served: {seen:?}"
        );
    }
    assert_eq!(assert_canary_free(&out, &repo, "applied"), 1);
    for file in ["run.json", "result.json", "plan.json", "events.ndjson"] {
        assert!(run_dir.join(file).is_file(), "{file} was scanned");
    }
    // The exempt entries are private and ignored: that is why they may hold it.
    assert_exempt_private(&repo, &run_dir);
}

/// A canary-seeded repo, host and gateway, ready for a run.
struct CanaryRun {
    _scratch: tempfile::TempDir,
    repo: PathBuf,
    data: PathBuf,
    server: wiremock::MockServer,
}

impl CanaryRun {
    async fn new() -> CanaryRun {
        let (scratch, repo, data) = canary_tracer();
        let server = wiremock::MockServer::start().await;
        mount_canary_gateway(&server, &data).await;
        CanaryRun {
            _scratch: scratch,
            repo,
            data,
            server,
        }
    }

    /// `ign deploy rollback --yes --json` (plus `extra`) of `run`.
    async fn rollback(&self, run: &str, extra: &[&str]) -> std::process::Output {
        let (_cfg, mut cmd) = verify_ign(&self.server.uri());
        cmd.args(rollback_args(&self.repo, run, true, true))
            .args(extra)
            .output()
            .expect("spawn ign")
    }
}

/// An error envelope: exit `exit`, `code` on stderr as one JSON value, and
/// nothing on stdout.
fn assert_error_envelope(out: &std::process::Output, exit: i32, code: &str) {
    assert_eq!(
        out.status.code(),
        Some(exit),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty(), "an error prints no stdout");
    assert_eq!(stderr_json(out)["error"]["code"], code);
}

#[tokio::test]
async fn apply_canary_all_outcomes() {
    use serde_json::json;

    // noop: the host already equals the render. The only secret in reach is
    // an unmanaged unit the apply never touches. A no-op contacts nothing.
    {
        let (_scratch, repo, data) = apply_tracer();
        let config = data
            .join("config/resources/dev")
            .join(APPLY_UNIT)
            .join("config.json");
        let mut bytes = std::fs::read(&config).expect("live config");
        bytes.pop();
        std::fs::write(&config, bytes).expect("equal again");
        let leak = data.join("config/resources/dev/ignition/database-translator/LEAK");
        std::fs::create_dir_all(&leak).expect("leak dir");
        std::fs::write(
            leak.join("config.json"),
            format!(r#"{{"x":{{"type":"Embedded","data":{{"ciphertext":"{CANARY_APPLY}"}}}}}}"#),
        )
        .expect("seed unmanaged secret");
        let hash = plan_hash(&repo);
        let server = wiremock::MockServer::start().await;
        mount_canary_gateway(&server, &data).await;
        let out = run_apply(&server, &repo, &hash, &[]).await;
        assert_eq!(out.status.code(), Some(0));
        let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
        assert_eq!(envelope["data"]["outcome"], "noop", "{envelope}");
        assert_eq!(assert_canary_free(&out, &repo, "noop"), 0);
    }

    // rolled_back: a red verify. The log line that turns it red carries the
    // canary, as does an unrelated one.
    {
        let (_scratch, repo, data) = canary_tracer();
        let hash = plan_hash(&repo);
        let server = wiremock::MockServer::start().await;
        let mut logs = canary_log_items();
        logs.push(json!({
            "timestamp": 1_792_999_999_000_i64,
            "loggerName": "NamedResourceHandler-ignition/database-translator",
            "level": "WARN",
            "message": format!(
                "Resource {NAMED_TRANSLATOR} does not have required UUID attribute, will not be loaded {CANARY_APPLY}"
            ),
        }));
        mount_apply_gateway_seeded(&server, &data, logs, Some(CANARY_APPLY)).await;
        let out = run_apply(&server, &repo, &hash, &[]).await;
        assert_eq!(
            out.status.code(),
            Some(6),
            "stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
        assert_eq!(envelope["data"]["outcome"], "rolled_back", "{envelope}");
        assert_eq!(assert_canary_free(&out, &repo, "rolled_back"), 1);
        let run_dir = PathBuf::from(envelope["data"]["run_dir"].as_str().expect("run dir"));
        assert_exempt_private(&repo, &run_dir);
    }

    // confirmation_required: no --yes. The prose names counts and paths.
    {
        let run = CanaryRun::new().await;
        let hash = plan_hash(&run.repo);
        let (_cfg, mut cmd) = verify_ign(&run.server.uri());
        let out = cmd
            .args(apply_args(&run.repo, Some(&hash), true, false))
            .output()
            .expect("spawn ign");
        assert_error_envelope(&out, 2, "confirmation_required");
        assert_eq!(assert_canary_free(&out, &run.repo, "confirmation"), 0);
        assert!(seen_requests(&run.server).await.is_empty());
    }

    // plan_stale: the live file moved on after review, still holding it.
    {
        let run = CanaryRun::new().await;
        let reviewed = plan_hash(&run.repo);
        let config = run
            .data
            .join("config/resources/dev")
            .join(APPLY_UNIT)
            .join("config.json");
        let mut bytes = std::fs::read(&config).expect("live config");
        bytes.extend_from_slice(b"  ");
        std::fs::write(&config, bytes).expect("move the live file on");
        let out = run_apply(&run.server, &run.repo, &reviewed, &[]).await;
        assert_error_envelope(&out, 6, "plan_stale");
        assert_eq!(assert_canary_free(&out, &run.repo, "plan_stale"), 0);
    }

    // deploy_preflight_failed: a Designer is open. The run directory exists
    // (run.json, result.json) and is scanned.
    {
        let run = CanaryRun::new().await;
        let hash = plan_hash(&run.repo);
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/data/api/v1/designers"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
                "items": [{
                    "id": "d-0", "user": "dev1", "project": "main",
                    "address": "10.0.0.7", "uptime": 1, "lastcomm": 1,
                    "timeout": 1, "memory": {}
                }],
                "metadata": {"total": 1, "matching": 1, "limit": -1, "offset": 0}
            })))
            .with_priority(1)
            .mount(&run.server)
            .await;
        let out = run_apply(&run.server, &run.repo, &hash, &[]).await;
        assert_error_envelope(&out, 6, "deploy_preflight_failed");
        assert_eq!(assert_canary_free(&out, &run.repo, "preflight_failed"), 1);
    }
}

#[tokio::test]
async fn rollback_canary() {
    let run = CanaryRun::new().await;
    let applied = apply_through_binary(&run.server, &run.repo, &[]).await;

    // A Designer edit then puts a secret of its own into the live file: the
    // refusal and the discard record paths, never content.
    let live = run
        .data
        .join("config/resources/dev")
        .join(APPLY_UNIT)
        .join("config.json");
    std::fs::write(
        &live,
        format!(r#"{{"x":{{"type":"Embedded","data":{{"ciphertext":"{CANARY_APPLY}"}}}}}}"#),
    )
    .expect("a later edit holding a secret");

    // deploy_live_changed: a refusal.
    let out = run.rollback(&applied, &[]).await;
    assert_error_envelope(&out, 6, "deploy_live_changed");
    assert_eq!(assert_canary_free(&out, &run.repo, "live_changed"), 1);

    // restored, over the later edit: the discarded path is recorded.
    let out = run.rollback(&applied, &["--discard-live-changes"]).await;
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    assert_eq!(envelope["data"]["outcome"], "restored", "{envelope}");
    assert!(
        envelope["data"]["discarded_live_changes"]
            .as_array()
            .is_some_and(|a| !a.is_empty()),
        "{envelope}"
    );
    assert_eq!(assert_canary_free(&out, &run.repo, "restored"), 2);
    // A rollback takes no .gwbk; only the apply's run directory has one.
    let rollback_dir = PathBuf::from(envelope["data"]["run_dir"].as_str().expect("run dir"));
    assert!(
        !files_relative(&rollback_dir)
            .iter()
            .any(|rel| rel.extension().is_some_and(|e| e == "gwbk")),
        "a rollback has no .gwbk"
    );
    let run_json: Value =
        serde_json::from_slice(&std::fs::read(rollback_dir.join("run.json")).expect("run.json"))
            .expect("json");
    assert_eq!(run_json["kind"], "rollback");
    assert!(run_json["discarded_live_changes"].is_array());

    // The interrupted-run recovery: a journal left for the applied run, a
    // rollback of it. Its events name every touched path and its live hash.
    plant_journal(&run.data, &applied);
    let out = run.rollback(&applied, &[]).await;
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    assert_eq!(envelope["data"]["outcome"], "restored", "{envelope}");
    let recovery_dir = PathBuf::from(envelope["data"]["run_dir"].as_str().expect("run dir"));
    let events = std::fs::read_to_string(recovery_dir.join("events.ndjson")).expect("events");
    assert!(
        events.contains("interrupted_run"),
        "the recovery recorded the torn state: {events}"
    );
    assert_eq!(assert_canary_free(&out, &run.repo, "recovered"), 3);
    // The apply's own directory is still the one that holds the exempt files.
    let apply_dir = run_dirs(&run.repo)
        .into_iter()
        .find(|dir| dir.ends_with(&applied))
        .expect("the apply's run directory");
    assert_exempt_private(&run.repo, &apply_dir);
}

#[tokio::test]
async fn canary_repeat_runs_clean() {
    // Edge DSURF-03 idempotency: apply, rollback and a second apply in one
    // repo. Each run directory scans clean, as do all the outputs.
    let run = CanaryRun::new().await;
    let first = apply_through_binary(&run.server, &run.repo, &[]).await;
    assert_eq!(run_dirs(&run.repo).len(), 1);

    let out = run.rollback(&first, &[]).await;
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(assert_canary_free(&out, &run.repo, "rollback"), 2);

    // The second apply, in human mode this time (progress on stderr).
    let hash = plan_hash(&run.repo);
    let (_cfg, mut cmd) = verify_ign(&run.server.uri());
    let out = cmd
        .args(apply_args(&run.repo, Some(&hash), false, true))
        .output()
        .expect("spawn ign");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("ign deploy apply: "),
        "human mode printed its progress, so stderr was scanned"
    );
    assert_eq!(assert_canary_free(&out, &run.repo, "second apply"), 3);
    for dir in run_dirs(&run.repo) {
        assert!(dir.join("run.json").is_file());
    }
}

#[tokio::test]
async fn apply_canary_drift_reason() {
    // A `--allow-drift` reason is free text a person typed. It is recorded
    // once, in `run.json` (the audit record, 23-21), and nowhere else: not on
    // either stream, not in the events, the result or the plan.
    let (scratch, repo, data) = apply_tracer();
    let base = scratch.path().join("out");
    let base_arg = base.to_str().expect("utf-8 path");
    let hash = plan_hash(&repo);
    let server = wiremock::MockServer::start().await;
    mount_apply_gateway(&server, &data, Vec::new()).await;
    let reason = format!("reviewed in PR 12 {CANARY_APPLY}");

    let out = run_apply(
        &server,
        &repo,
        &hash,
        &["--base", base_arg, "--allow-drift", &reason],
    )
    .await;
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.stderr.is_empty(),
        "a --json apply leaves stderr silent: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_streams_canary_free(&out, "drift reason");
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    let run_dir = PathBuf::from(envelope["data"]["run_dir"].as_str().expect("run dir"));

    let holders: Vec<PathBuf> = files_relative(&run_dir)
        .into_iter()
        .filter(|rel| !is_canary_exempt(rel))
        .filter(|rel| {
            std::fs::read_to_string(run_dir.join(rel))
                .expect("read")
                .contains(CANARY_APPLY)
        })
        .collect();
    assert_eq!(
        holders,
        [PathBuf::from("run.json")],
        "only the run record holds the reason"
    );
    let run_json: Value =
        serde_json::from_slice(&std::fs::read(run_dir.join("run.json")).expect("run.json"))
            .expect("json");
    assert_eq!(run_json["allow_drift"], reason.as_str());
}

// ============================================================================
// The secret canary over the backup's surfaces (24-09, DSURF-03)
// ============================================================================

/// The 24-09 canary: seeded in a secret field of the backup's live resource
/// files, an extra member of the backup's redundancy status, an entry of the
/// backup's log from an unrelated logger and the HTML body of a 403 the
/// backup answers for a secret listing.
const CANARY_PAIR: &str = "IGN-CANARY-24";

/// A secret-bearing backup-only resource unit below the backup's data dir.
fn seed_backup_leak(backup_data: &Path, canary: &str) {
    let leak = backup_data.join("config/resources/dev/ignition/database-translator/LEAK");
    std::fs::create_dir_all(&leak).expect("leak dir");
    std::fs::write(
        leak.join("config.json"),
        format!(r#"{{"x":{{"type":"Embedded","data":{{"ciphertext":"{canary}"}}}}}}"#),
    )
    .expect("seed the backup-only secret");
}

/// The backup's copy of the translator config becomes a secret-bearing
/// document that differs from the master's.
fn seed_backup_differing_file(backup_data: &Path, canary: &str) -> PathBuf {
    let file = backup_data
        .join("config/resources/dev")
        .join(NAMED_TRANSLATOR)
        .join("config.json");
    std::fs::write(
        &file,
        format!(r#"{{"x":{{"type":"Embedded","data":{{"ciphertext":"{canary}"}}}}}}"#),
    )
    .expect("seed the differing backup file");
    file
}

/// What the backup answers for the one referenced secret provider.
#[derive(Clone, Copy)]
enum BackupListing {
    /// 200 with these names.
    Names(&'static [&'static str]),
    /// 403 with an HTML body that names the canary.
    Refused,
}

async fn mount_backup_listing(server: &wiremock::MockServer, listing: BackupListing) {
    match listing {
        BackupListing::Names(names) => {
            mount_secret_names(server, "cap24-file", 200, &names_body(names)).await;
        }
        BackupListing::Refused => {
            mount_secret_names(
                server,
                "cap24-file",
                403,
                &format!(
                    "<html><body><h2>HTTP ERROR 403</h2><p>token={CANARY_PAIR} lacks the permission</p></body></html>"
                ),
            )
            .await;
        }
    }
}

/// The canary is on neither stream of `out`.
fn assert_pair_streams_free(out: &std::process::Output, what: &str) {
    for (name, bytes) in [("stdout", &out.stdout), ("stderr", &out.stderr)] {
        assert!(
            !String::from_utf8_lossy(bytes).contains(CANARY_PAIR),
            "canary on {name}: {what}\n{}",
            String::from_utf8_lossy(bytes)
        );
    }
}

/// The canary is on neither stream and in no scanned file of any run
/// directory below `repo`. Returns the number of run directories scanned.
fn assert_pair_canary_free(out: &std::process::Output, repo: &Path, what: &str) -> usize {
    assert_pair_streams_free(out, what);
    let dirs = run_dirs(repo);
    for dir in &dirs {
        let leaks = scan_run_dir_for(CANARY_PAIR, dir);
        assert!(leaks.is_empty(), "canary in {leaks:?} of {dir:?}: {what}");
        for file in ["run.json", "events.ndjson"] {
            assert!(dir.join(file).is_file(), "{file} was scanned in {dir:?}");
        }
    }
    dirs.len()
}

/// A pair ready for an apply: the tracer referencing `cap24-file/CAP24-Name`
/// with a changed master, a `local:` backup whose data dir equals the render
/// and carries the canary in a backup-only unit (and, with `differ`, in a
/// rendered file that differs), and the two seeded gateways.
struct CanaryPair {
    _scratch: tempfile::TempDir,
    repo: PathBuf,
    hash: String,
    backup_data: PathBuf,
    differing: Option<PathBuf>,
    master: wiremock::MockServer,
    backup: wiremock::MockServer,
}

async fn canary_pair(syncs: bool, differ: bool, listing: BackupListing) -> CanaryPair {
    let (scratch, repo, data) = local_tracer();
    reference_secrets(
        scratch.path(),
        &repo,
        &data,
        &[("cap24-file", "CAP24-Name")],
    );
    let backup_data = scratch.path().join("data-bk");
    copy_dir(&data, &backup_data);
    let target = repo.join("targets/dev.yaml");
    let mut yaml = std::fs::read_to_string(&target).expect("target file");
    yaml.push_str(&format!(
        "  backup:\n    profile: bk\n    local:\n      data_dir: '{}'\n",
        backup_data.display()
    ));
    std::fs::write(&target, yaml).expect("target file with a backup");

    // What `apply_tracer` adds to `changed_tracer`: a changed master unit, the
    // system name the wrong_target preflight compares, a git repo ignoring
    // `.ign/`.
    let config = data
        .join("config/resources/dev")
        .join(APPLY_UNIT)
        .join("config.json");
    let mut bytes = std::fs::read(&config).expect("live config");
    bytes.push(b' ');
    std::fs::write(&config, bytes).expect("change the live unit");
    let props = data.join("config/resources/core/ignition/system-properties");
    std::fs::create_dir_all(&props).expect("system-properties dir");
    std::fs::write(
        props.join("config.json"),
        format!("{{\"systemName\":\"{APPLY_SYSTEM_NAME}\"}}"),
    )
    .expect("system-properties config");
    std::fs::write(repo.join(".gitignore"), ".ign/\n").expect(".gitignore");
    let init = std::process::Command::new("git")
        .args(["init", "-q"])
        .arg(&repo)
        .status()
        .expect("git init");
    assert!(init.success());

    seed_backup_leak(&backup_data, CANARY_PAIR);
    let differing = differ.then(|| seed_backup_differing_file(&backup_data, CANARY_PAIR));
    let hash = plan_hash(&repo);
    let master = wiremock::MockServer::start().await;
    let backup = wiremock::MockServer::start().await;
    mount_pair_master(&master, &data).await;
    mount_secret_names(&master, "cap24-file", 200, &names_body(&["CAP24-Name"])).await;
    mount_pair_backup_seeded(&backup, &data, syncs, Some(CANARY_PAIR)).await;
    mount_backup_listing(&backup, listing).await;
    CanaryPair {
        _scratch: scratch,
        repo,
        hash,
        backup_data,
        differing,
        master,
        backup,
    }
}

fn run_canary_apply(rig: &CanaryPair, json: bool, extra: &[&str]) -> std::process::Output {
    let (_cfg, mut cmd) = pair_ign(&rig.master.uri(), &rig.backup.uri());
    cmd.args(apply_args(&rig.repo, Some(&rig.hash), json, true))
        .args(extra)
        .output()
        .expect("spawn ign")
}

/// The seeds were real: the backup's files still hold the canary and its
/// status was served.
async fn assert_pair_seeds_real(rig: &CanaryPair) {
    let leak = rig
        .backup_data
        .join("config/resources/dev/ignition/database-translator/LEAK/config.json");
    assert!(
        std::fs::read_to_string(&leak)
            .expect("backup-only unit")
            .contains(CANARY_PAIR)
    );
    if let Some(differing) = &rig.differing {
        assert!(
            std::fs::read_to_string(differing)
                .expect("differing file")
                .contains(CANARY_PAIR)
        );
    }
    let seen = seen_requests(&rig.backup).await;
    for route in [
        "GET /data/api/v1/redundancy",
        "GET /data/api/v1/secret-providers/cap24-file/secrets",
    ] {
        assert!(
            seen.iter().any(|r| r == route),
            "{route} was served: {seen:?}"
        );
    }
}

#[tokio::test]
async fn apply_canary_backup() {
    // (syncs, differ, outcome, exit, --backup-timeout)
    let cases = [
        (false, true, "applied_backup_unsynced", 6, "1"),
        (true, false, "applied", 0, "30"),
    ];
    for (syncs, differ, outcome, exit, timeout) in cases {
        for json in [true, false] {
            let what = format!("{outcome} (json {json})");
            let rig = canary_pair(syncs, differ, BackupListing::Names(&["CAP24-Name"])).await;
            let out = run_canary_apply(&rig, json, &["--backup-timeout", timeout]);
            assert_eq!(
                out.status.code(),
                Some(exit),
                "{what}: stderr: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            if json {
                assert!(out.stderr.is_empty(), "a --json apply leaves stderr silent");
                let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
                assert_eq!(envelope["data"]["outcome"], outcome, "{envelope}");
                if differ {
                    let files = envelope["data"]["backup"]["checks"]
                        .as_array()
                        .expect("backup checks")
                        .iter()
                        .find(|c| c["id"] == "files")
                        .expect("a files check")
                        .clone();
                    assert_eq!(files["status"], "fail", "the canary file was read: {files}");
                    assert_eq!(files["findings"][0]["code"], "file_differs");
                }
            }
            assert_pair_seeds_real(&rig).await;
            if !syncs {
                let seen = seen_requests(&rig.backup).await;
                assert!(
                    seen.iter().any(|r| r == "GET /data/api/v1/logs"),
                    "the log that carries the canary was read: {seen:?}"
                );
            }
            assert_eq!(assert_pair_canary_free(&out, &rig.repo, &what), 1);
        }
    }
}

/// A backup that refuses a secret listing with a body naming the canary: the
/// apply stops with an error envelope and nothing it prints or keeps holds
/// the body.
#[tokio::test]
async fn apply_canary_backup_refused_listing() {
    for json in [true, false] {
        let rig = canary_pair(true, false, BackupListing::Refused).await;
        let out = run_canary_apply(&rig, json, &["--backup-timeout", "1"]);
        assert_eq!(
            out.status.code(),
            Some(5),
            "a refused listing is an auth failure: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_pair_canary_free(&out, &rig.repo, &format!("refused listing (json {json})"));
        let seen = seen_requests(&rig.backup).await;
        assert!(
            seen.iter()
                .any(|r| r == "GET /data/api/v1/secret-providers/cap24-file/secrets"),
            "the refused listing was asked for: {seen:?}"
        );
        assert_only_gets(&seen, "the backup");
    }
}

#[tokio::test]
async fn verify_canary_both() {
    let green = BackupListing::Names(&["CAP24-Name"]);
    let red = BackupListing::Names(&[]);
    // (listing, differing backup file, expected exit)
    for (what, listing, differ, exit) in [
        ("green", green, false, Some(0)),
        ("red", red, true, Some(6)),
        ("refused", BackupListing::Refused, false, None),
    ] {
        for node in ["both", "backup"] {
            for json in [true, false] {
                let pair =
                    verify_pair_seeded(&[("cap24-file", "CAP24-Name")], Some(CANARY_PAIR)).await;
                mount_secret_names(
                    &pair.master,
                    "cap24-file",
                    200,
                    &names_body(&["CAP24-Name"]),
                )
                .await;
                mount_backup_listing(&pair.backup, listing).await;
                if differ {
                    seed_backup_differing_file(&pair.backup_data, CANARY_PAIR);
                }
                let out = run_verify_node(&pair, node, json);
                let label = format!("{what}, --node {node}, json {json}");
                match exit {
                    Some(code) => assert_eq!(
                        out.status.code(),
                        Some(code),
                        "{label}: stderr: {}\nstdout: {}",
                        String::from_utf8_lossy(&out.stderr),
                        String::from_utf8_lossy(&out.stdout)
                    ),
                    None => assert_eq!(out.status.code(), Some(5), "{label}"),
                }
                assert_pair_streams_free(&out, &label);
                // Seeds were real: the backup answered its status and its
                // secret listing, and it still holds the canary.
                let seen = seen_requests(&pair.backup).await;
                assert!(
                    seen.iter().any(|r| r == "GET /data/api/v1/redundancy"),
                    "{label}: {seen:?}"
                );
                assert!(
                    seen.iter()
                        .any(|r| r == "GET /data/api/v1/secret-providers/cap24-file/secrets"),
                    "{label}: {seen:?}"
                );
                assert_only_gets(&seen, "the backup");
                assert_only_gets(&seen_requests(&pair.master).await, "the master");
                assert!(
                    std::fs::read_to_string(pair.backup_data.join(
                        "config/resources/dev/ignition/database-translator/LEAK/config.json"
                    ))
                    .expect("backup-only unit")
                    .contains(CANARY_PAIR)
                );
            }
        }
    }
}
