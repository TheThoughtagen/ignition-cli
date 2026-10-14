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

/// A `local:` tracer copy with a live data directory equal to the ACTUAL
/// render (render may mint a uuid, so the live tree is built from
/// `ign deploy render --out`, never from the source files). Returns
/// `(scratch, repo, data_dir)`; the live mode collection is
/// `<data_dir>/config/resources/dev/`.
fn local_tracer() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let scratch = tempfile::tempdir().expect("tempdir");
    let repo = scratch.path().join("repo");
    copy_dir(Path::new(TRACER_REPO), &repo);
    let data = scratch.path().join("data");
    let live = data.join("config").join("resources").join("dev");
    std::fs::create_dir_all(&live).expect("live dir");
    let yaml = format!(
        "schema_version: 1\ngateway: tracer-gw\nmode: dev\nnodes:\n  master:\n    profile: dev\n    local:\n      data_dir: '{}'\n",
        data.display()
    );
    std::fs::write(repo.join("targets/dev.yaml"), yaml).expect("target file");

    let out_dir = scratch.path().join("out");
    let (_cfg, mut cmd) = ign();
    let out = cmd
        .args(["deploy", "render", "--env", "dev", "--repo"])
        .arg(&repo)
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
    copy_dir(&out_dir, &live);
    std::fs::remove_file(live.join("render.json")).expect("drop render.json");
    (scratch, repo, data)
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

fn plan_json(repo: &Path, cmd: &mut Command) -> std::process::Output {
    cmd.args(["--json", "deploy", "plan", "--env", "dev", "--repo"])
        .arg(repo)
        .output()
        .expect("spawn ign")
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

/// Every file below `dir`, relative path to bytes.
fn read_tree(dir: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, rel: &str, out: &mut std::collections::BTreeMap<String, Vec<u8>>) {
        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in read {
            let entry = entry.expect("entry");
            let name = entry.file_name().to_string_lossy().into_owned();
            let child = if rel.is_empty() {
                name
            } else {
                format!("{rel}/{name}")
            };
            if entry.path().is_dir() {
                walk(&entry.path(), &child, out);
            } else {
                out.insert(child, std::fs::read(entry.path()).expect("read"));
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(dir, "", &mut out);
    out
}

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
const NAMED_TRANSLATOR: &str = "ignition/database-translator/POSTGRES";

/// The rendered `config.json` of `key` from the live tree, with JSON null
/// members removed, as the gateway's REST view reports it (CV2 N1).
fn rest_config(data: &Path, key: &str) -> Value {
    fn strip(v: &Value) -> Value {
        match v {
            Value::Object(m) => Value::Object(
                m.iter()
                    .filter(|(_, v)| !v.is_null())
                    .map(|(k, v)| (k.clone(), strip(v)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }
    let path = data
        .join("config/resources/dev")
        .join(key)
        .join("config.json");
    strip(&serde_json::from_slice(&std::fs::read(path).expect("live config")).expect("json"))
}

/// A mock gateway that agrees with the live tree. `omit_translator` leaves
/// the declared database-translator out of the REST list (a red read_back);
/// `log_items` are the WARN lines the gateway returns.
async fn mount_verify_gateway(
    server: &wiremock::MockServer,
    data: &Path,
    omit_translator: bool,
    log_items: Vec<Value>,
) {
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
            "config": rest_config(data, NAMED_TRANSLATOR)
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
            "config": rest_config(data, "ignition/general-alarm-settings")
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

/// An isolated config whose `[profiles.dev]` points at `url`, and `ign` with
/// the mock token in the environment.
fn verify_ign(url: &str) -> (tempfile::TempDir, Command) {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("config.toml");
    std::fs::write(
        &config,
        format!(
            "active = \"dev\"\n\n[profiles.dev]\nurl = \"{url}\"\nauth = {{ token_env = \"IGNITION_TOKEN\" }}\n"
        ),
    )
    .expect("write config");
    let mut cmd = Command::cargo_bin("ign").expect("binary 'ign' not found");
    cmd.env("IGNITION_CLI_CONFIG", &config)
        .env("IGNITION_TOKEN", "mock:name-key");
    for var in ["IGNITION_URL", "IGNITION_PROFILE", "IGNITION_JSON"] {
        cmd.env_remove(var);
    }
    (dir, cmd)
}

fn verify_args<'a>(repo: &'a Path, json: bool) -> Vec<std::ffi::OsString> {
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
    mount_verify_gateway(&server, &data, true, Vec::new()).await;
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
    mount_verify_gateway(&server, &data, false, Vec::new()).await;
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
