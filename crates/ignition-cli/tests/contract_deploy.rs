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
/// members removed. Most types' REST view omits null members, and this helper
/// models that shape. `ignition/cobranding` is the exception: it echoes unset
/// members as explicit `null`, which `rest_config_raw` models.
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

const APPLY_UNIT: &str = "ignition/database-translator/POSTGRES";

/// `local_tracer()` with one live unit changed by a trailing space (a JSON
/// document that reads the same, bytes that differ), so the plan has one
/// `changed` entry. Returns `(scratch, repo, data_dir)`.
fn changed_tracer() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let (scratch, repo, data) = local_tracer();
    let config = data
        .join("config/resources/dev")
        .join(APPLY_UNIT)
        .join("config.json");
    let mut bytes = std::fs::read(&config).expect("live config");
    bytes.push(b' ');
    std::fs::write(&config, bytes).expect("change the live unit");
    (scratch, repo, data)
}

/// `plan_sha256` of `deploy plan` over `repo`, from the binary.
fn plan_hash(repo: &Path) -> String {
    let (_cfg, mut cmd) = ign();
    let out = plan_json(repo, &mut cmd);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("plan envelope");
    envelope["data"]["plan_sha256"]
        .as_str()
        .expect("plan_sha256")
        .to_string()
}

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

/// The system name the apply gateway's REST answer and the host's
/// `system-properties` share (the `wrong_target` preflight compares them).
const APPLY_SYSTEM_NAME: &str = "apply-gw";

/// A small valid ZIP standing in for a `.gwbk`, the stored member carrying
/// `secret` when given (a `.gwbk` holds the gateway's secrets, so the 23-15
/// canary seeds one).
fn apply_gwbk_zip(secret: Option<&str>) -> Vec<u8> {
    use std::io::Write as _;
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    writer.start_file("data.bin", options).expect("member");
    writer.write_all(&[7u8; 4096]).expect("write");
    if let Some(secret) = secret {
        writer.write_all(secret.as_bytes()).expect("write secret");
    }
    writer.finish().expect("finish").into_inner()
}

/// What the stateful apply gateway remembers between requests.
struct ApplyGwState {
    lock_held: bool,
    lock_stamp: i64,
    scan_ts: i64,
    pending_scan_ts: Option<i64>,
    active_reads: u32,
}

/// `changed_tracer()` made ready for an apply: the host's
/// `system-properties` carry [`APPLY_SYSTEM_NAME`], the repo is a git repo
/// whose `.gitignore` holds `.ign/`. Returns `(scratch, repo, data_dir)`.
fn apply_tracer() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let (scratch, repo, data) = changed_tracer();
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
    (scratch, repo, data)
}

/// A stateful mock gateway for `deploy apply` (23-13; 23-21 reuses it). Every
/// route the engine calls is served: the scan-lock (acquire, then info with a
/// remaining counter, free again once a scan is requested), the config scan
/// (inactive, then active for one read after a trigger, then inactive with an
/// advanced timestamp), gateway identity and mode, an empty Designer list, a
/// small valid `.gwbk`, an empty restart-pending list, the project scan, and
/// the REST view of the translator and the alarm singleton, read from the
/// local data directory at the time of the request (so a restore is seen).
/// `log_items` are the lines the gateway log returns; a "will not be
/// loaded" line for a written resource turns the verify red.
async fn mount_apply_gateway(server: &wiremock::MockServer, data: &Path, log_items: Vec<Value>) {
    mount_apply_gateway_seeded(server, data, log_items, None).await;
}

/// [`mount_apply_gateway`] with a secret planted where a gateway could leak
/// one (23-15): the `.gwbk` bytes carry `canary`, and the optional
/// restart-pending route answers 500 with a body that names it.
async fn mount_apply_gateway_seeded(
    server: &wiremock::MockServer,
    data: &Path,
    log_items: Vec<Value>,
    canary: Option<&str>,
) {
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, Request, ResponseTemplate};

    let state = Arc::new(Mutex::new(ApplyGwState {
        lock_held: false,
        lock_stamp: 1_000,
        scan_ts: 1_792_000_000_000,
        pending_scan_ts: None,
        active_reads: 0,
    }));

    let s = state.clone();
    Mock::given(method("POST"))
        .and(path("/data/api/v1/scan-lock/config"))
        .respond_with(move |_: &Request| {
            let mut st = s.lock().expect("state");
            st.lock_held = true;
            st.lock_stamp += 1;
            ResponseTemplate::new(200)
        })
        .mount(server)
        .await;
    let s = state.clone();
    Mock::given(method("GET"))
        .and(path("/data/api/v1/scan-lock/config"))
        .respond_with(move |_: &Request| {
            let st = s.lock().expect("state");
            if st.lock_held {
                ResponseTemplate::new(200).set_body_json(json!({
                    "timestamp": st.lock_stamp,
                    "remaining": 170,
                    "actor": "apply-test"
                }))
            } else {
                ResponseTemplate::new(204)
            }
        })
        .mount(server)
        .await;
    let s = state.clone();
    Mock::given(method("POST"))
        .and(path("/data/api/v1/scan/config"))
        .respond_with(move |_: &Request| {
            let mut st = s.lock().expect("state");
            st.lock_held = false;
            st.pending_scan_ts = Some(st.scan_ts + 1_000);
            st.active_reads = 1;
            ResponseTemplate::new(200)
        })
        .mount(server)
        .await;
    let s = state.clone();
    Mock::given(method("GET"))
        .and(path("/data/api/v1/scan/config"))
        .respond_with(move |_: &Request| {
            let mut st = s.lock().expect("state");
            if st.active_reads > 0 {
                st.active_reads -= 1;
                return ResponseTemplate::new(200).set_body_json(json!({
                    "scanActive": true,
                    "lastScanTimestamp": st.scan_ts,
                    "lastScanDuration": 300
                }));
            }
            if let Some(next) = st.pending_scan_ts.take() {
                st.scan_ts = next;
            }
            ResponseTemplate::new(200).set_body_json(json!({
                "scanActive": false,
                "lastScanTimestamp": st.scan_ts,
                "lastScanDuration": 300
            }))
        })
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/data/api/v1/scan/projects"))
        .respond_with(ResponseTemplate::new(200))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/data/api/v1/gateway-info"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": APPLY_SYSTEM_NAME,
            "deploymentMode": "dev",
            "ignitionVersion": "8.3.6 (b2026042713)"
        })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/data/api/v1/designers"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [],
            "metadata": {"total": 0, "matching": 0, "limit": -1, "offset": 0}
        })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/data/api/v1/backup"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "content-type",
                    "application/x-ignition-gateway-backup;charset=utf-8",
                )
                .set_body_bytes(apply_gwbk_zip(canary)),
        )
        .mount(server)
        .await;
    let pending = match canary {
        Some(secret) => ResponseTemplate::new(500).set_body_string(format!(
            "restart route failed: password={secret} rejected by the gateway"
        )),
        None => ResponseTemplate::new(200).set_body_json(json!({"pending": []})),
    };
    Mock::given(method("GET"))
        .and(path("/data/api/v1/restart-tasks/pending"))
        .respond_with(pending)
        .mount(server)
        .await;

    // The REST view of the live tree, read when the request arrives.
    let live = data.to_path_buf();
    Mock::given(method("GET"))
        .and(path(
            "/data/api/v1/resources/list/ignition/database-translator",
        ))
        .respond_with(move |_: &Request| {
            ResponseTemplate::new(200).set_body_json(json!({
                "items": [{
                    "type": "ignition/database-translator",
                    "name": "POSTGRES",
                    "collection": "dev",
                    "config": rest_config(&live, NAMED_TRANSLATOR)
                }],
                "metadata": {"total": 1, "matching": 1, "limit": -1, "offset": 0}
            }))
        })
        .mount(server)
        .await;
    let live = data.to_path_buf();
    Mock::given(method("GET"))
        .and(path(
            "/data/api/v1/resources/singleton/ignition/general-alarm-settings",
        ))
        .respond_with(move |_: &Request| {
            ResponseTemplate::new(200).set_body_json(json!({
                "type": "ignition/general-alarm-settings",
                "collection": "dev",
                "config": rest_config(&live, "ignition/general-alarm-settings")
            }))
        })
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

/// The request paths the mock gateway saw, as `METHOD /path`.
async fn seen_requests(server: &wiremock::MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .map(|r| format!("{} {}", r.method, r.url.path()))
        .collect()
}

/// `ign --json --yes deploy apply` over `repo`, the mock at `url`.
fn apply_args(repo: &Path, hash: Option<&str>, json: bool, yes: bool) -> Vec<std::ffi::OsString> {
    let mut args: Vec<std::ffi::OsString> = Vec::new();
    if json {
        args.push("--json".into());
    }
    if yes {
        args.push("--yes".into());
    }
    for a in ["deploy", "apply", "--env", "dev", "--repo"] {
        args.push(a.into());
    }
    args.push(repo.as_os_str().to_owned());
    if let Some(hash) = hash {
        args.push("--plan-sha256".into());
        args.push(hash.into());
    }
    args
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

/// Run `ign deploy apply` over `repo` against the mock gateway with `extra`
/// flags after the standard ones (`--json --yes` and the plan hash).
async fn run_apply(
    server: &wiremock::MockServer,
    repo: &Path,
    hash: &str,
    extra: &[&str],
) -> std::process::Output {
    let (_cfg, mut cmd) = verify_ign(&server.uri());
    cmd.args(apply_args(repo, Some(hash), true, true))
        .args(extra)
        .output()
        .expect("spawn ign")
}

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

/// `ign [--json] [--yes] deploy rollback --env dev --run RUN --repo REPO`.
fn rollback_args(repo: &Path, run: &str, json: bool, yes: bool) -> Vec<std::ffi::OsString> {
    let mut args: Vec<std::ffi::OsString> = Vec::new();
    if json {
        args.push("--json".into());
    }
    if yes {
        args.push("--yes".into());
    }
    for a in ["deploy", "rollback", "--env", "dev", "--run", run, "--repo"] {
        args.push(a.into());
    }
    args.push(repo.as_os_str().to_owned());
    args
}

/// The configuration tree of a host listing: a run keeps its record and
/// snapshot below `ign-deploy/`, which is not configuration.
fn config_tree(
    tree: std::collections::BTreeMap<String, Vec<u8>>,
) -> std::collections::BTreeMap<String, Vec<u8>> {
    tree.into_iter()
        .filter(|(path, _)| path.starts_with("config/"))
        .collect()
}

/// Run `ign deploy apply --yes` through the binary over `repo` and return the
/// applied run's id.
async fn apply_through_binary(
    server: &wiremock::MockServer,
    repo: &Path,
    extra: &[&str],
) -> String {
    let hash = plan_hash(repo);
    let out = run_apply(server, repo, &hash, extra).await;
    assert_eq!(
        out.status.code(),
        Some(0),
        "apply stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("apply envelope");
    assert_eq!(envelope["data"]["outcome"], "applied", "{envelope}");
    envelope["data"]["run_id"]
        .as_str()
        .expect("run id")
        .to_string()
}

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

/// Plant the journal a killed apply leaves for `run` on the local host: the
/// run's own record supplies the touched set and the pre-run fingerprint, the
/// phase is `written` (every move done, not finalized). The snapshot the
/// recovery restores from is the one the apply took.
fn plant_journal(data: &Path, run: &str) {
    plant_journal_at(
        data,
        run,
        ignition_core::deploy::apply::journal::JournalPhase::Written,
    );
}

/// [`plant_journal`] at an explicit phase.
fn plant_journal_at(
    data: &Path,
    run: &str,
    phase: ignition_core::deploy::apply::journal::JournalPhase,
) {
    use ignition_core::deploy::apply::journal::Journal;
    let deploy_dir = data.join("ign-deploy/tracer-gw/dev");
    let record: Value = serde_json::from_slice(
        &std::fs::read(deploy_dir.join("snapshots").join(run).join("record.json"))
            .expect("the run's record"),
    )
    .expect("record.json is JSON");
    let mut touched: Vec<String> = record["touched"]
        .as_array()
        .expect("touched")
        .iter()
        .map(|entry| entry["path"].as_str().expect("path").to_string())
        .collect();
    touched.sort();
    let journal = Journal {
        schema_version: 1,
        run_id: run.to_string(),
        gateway: "tracer-gw".to_string(),
        mode: "dev".to_string(),
        phase,
        pre_fingerprint: record["pre_fingerprint"]
            .as_str()
            .expect("pre_fingerprint")
            .to_string(),
        touched,
    };
    std::fs::write(deploy_dir.join("journal.json"), journal.to_bytes()).expect("plant journal");
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

/// Change the live unit once more after the apply (a Designer edit, say).
fn edit_live_unit(data: &Path) {
    let config = data
        .join("config/resources/dev")
        .join(APPLY_UNIT)
        .join("config.json");
    let mut bytes = std::fs::read(&config).expect("live config");
    bytes.extend_from_slice(b"\n\n");
    std::fs::write(config, bytes).expect("edit the live unit");
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
