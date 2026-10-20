//! The apply/rollback harness shared by `contract_deploy.rs` and
//! `contract_stdout_purity.rs` (23-15): the tracer repo with a live local
//! host, the stateful mock gateway `deploy apply` talks to, and the argument
//! builders. Moved here unchanged so both binaries drive one fixture.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::Value;

pub const TRACER_REPO: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../ignition-core/tests/fixtures/deploy/repos/tracer"
);

/// `ign` with an absent isolated config and no ambient gateway knobs.
pub fn ign() -> (tempfile::TempDir, Command) {
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

pub fn copy_dir(from: &Path, to: &Path) {
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

pub fn stderr_json(out: &std::process::Output) -> Value {
    serde_json::from_slice(&out.stderr).unwrap_or_else(|e| {
        panic!(
            "stderr is not one JSON value ({e}): {}",
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

/// A `local:` tracer copy with a live data directory equal to the ACTUAL
/// render (render may mint a uuid, so the live tree is built from
/// `ign deploy render --out`, never from the source files). Returns
/// `(scratch, repo, data_dir)`; the live mode collection is
/// `<data_dir>/config/resources/dev/`.
pub fn local_tracer() -> (tempfile::TempDir, PathBuf, PathBuf) {
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

pub fn plan_json(repo: &Path, cmd: &mut Command) -> std::process::Output {
    cmd.args(["--json", "deploy", "plan", "--env", "dev", "--repo"])
        .arg(repo)
        .output()
        .expect("spawn ign")
}

/// Every file below `dir`, relative path to bytes.
pub fn read_tree(dir: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
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

pub const NAMED_TRANSLATOR: &str = "ignition/database-translator/POSTGRES";

/// The rendered `config.json` of `key` from the live tree, with JSON null
/// members removed. Most types' REST view omits null members, and this helper
/// models that shape. `ignition/cobranding` is the exception: it echoes unset
/// members as explicit `null`, which `rest_config_raw` models.
pub fn rest_config(data: &Path, key: &str) -> Value {
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

/// An isolated config whose `[profiles.dev]` points at `url`, and `ign` with
/// the mock token in the environment.
pub fn verify_ign(url: &str) -> (tempfile::TempDir, Command) {
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

pub const APPLY_UNIT: &str = "ignition/database-translator/POSTGRES";

/// `local_tracer()` with one live unit changed by a trailing space (a JSON
/// document that reads the same, bytes that differ), so the plan has one
/// `changed` entry. Returns `(scratch, repo, data_dir)`.
pub fn changed_tracer() -> (tempfile::TempDir, PathBuf, PathBuf) {
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
pub fn plan_hash(repo: &Path) -> String {
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

/// The system name the apply gateway's REST answer and the host's
/// `system-properties` share (the `wrong_target` preflight compares them).
pub const APPLY_SYSTEM_NAME: &str = "apply-gw";

/// A small valid ZIP standing in for a `.gwbk`, the stored member carrying
/// `secret` when given (a `.gwbk` holds the gateway's secrets, so the 23-15
/// canary seeds one).
pub fn apply_gwbk_zip(secret: Option<&str>) -> Vec<u8> {
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
pub struct ApplyGwState {
    lock_held: bool,
    lock_stamp: i64,
    scan_ts: i64,
    pending_scan_ts: Option<i64>,
    active_reads: u32,
}

/// `changed_tracer()` made ready for an apply: the host's
/// `system-properties` carry [`APPLY_SYSTEM_NAME`], the repo is a git repo
/// whose `.gitignore` holds `.ign/`. Returns `(scratch, repo, data_dir)`.
pub fn apply_tracer() -> (tempfile::TempDir, PathBuf, PathBuf) {
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
pub async fn mount_apply_gateway(
    server: &wiremock::MockServer,
    data: &Path,
    log_items: Vec<Value>,
) {
    mount_apply_gateway_seeded(server, data, log_items, None).await;
}

/// [`mount_apply_gateway`] with a secret planted where a gateway could leak
/// one (23-15): the `.gwbk` bytes carry `canary`, and the optional
/// restart-pending route answers 500 with a body that names it.
pub async fn mount_apply_gateway_seeded(
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
pub async fn seen_requests(server: &wiremock::MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .map(|r| format!("{} {}", r.method, r.url.path()))
        .collect()
}

/// `ign --json --yes deploy apply` over `repo`, the mock at `url`.
pub fn apply_args(
    repo: &Path,
    hash: Option<&str>,
    json: bool,
    yes: bool,
) -> Vec<std::ffi::OsString> {
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

/// Run `ign deploy apply` over `repo` against the mock gateway with `extra`
/// flags after the standard ones (`--json --yes` and the plan hash).
pub async fn run_apply(
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

/// The tracer made a redundant pair (24-08): `nodes.backup` names the profile
/// `bk` and no file transport, so the backup is REST only (D-04). Call after
/// [`apply_tracer`] and before [`plan_hash`].
pub fn pair_target(repo: &Path) {
    let target = repo.join("targets/dev.yaml");
    let mut yaml = std::fs::read_to_string(&target).expect("target file");
    yaml.push_str("  backup:\n    profile: bk\n");
    std::fs::write(&target, yaml).expect("target file with a backup");
}

/// An isolated config with the master profile `dev` at `master_url` and the
/// backup profile `bk` at `backup_url`, and `ign` with the mock token in the
/// environment.
pub fn pair_ign(master_url: &str, backup_url: &str) -> (tempfile::TempDir, Command) {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("config.toml");
    std::fs::write(
        &config,
        format!(
            "active = \"dev\"\n\n[profiles.dev]\nurl = \"{master_url}\"\nauth = {{ token_env = \"IGNITION_TOKEN\" }}\n\n[profiles.bk]\nurl = \"{backup_url}\"\nauth = {{ token_env = \"IGNITION_TOKEN\" }}\n"
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

/// The master of a pair (24-08): the apply gateway plus its redundancy
/// status (`Active`, the node that takes the writes) and the resync route a
/// backup that does not converge is asked through.
pub async fn mount_pair_master(server: &wiremock::MockServer, data: &Path) {
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, ResponseTemplate};

    mount_apply_gateway(server, data, Vec::new()).await;
    Mock::given(method("GET"))
        .and(path("/data/api/v1/redundancy"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "role": "Master", "projectState": "Good", "activityLevel": "Active",
            "localId": "10.0.0.1", "peerId": "bk", "peerConnected": true,
            "hasConfigAccess": true, "syncPending": false, "failoverPending": false,
            "uptime": 100000, "lastSyncTimestamp": 1_792_000_000_000_i64
        })))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/data/api/v1/redundancy/gwaction/resync"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"resync_state": "started"})))
        .mount(server)
        .await;
}

/// The backup of a pair (24-08), read only. With `syncs` false it never
/// links (`peerConnected` false, the timestamp never moves). With `syncs`
/// true its `lastSyncTimestamp` moves on from the third status read (the
/// `master_active` preflight and the pre-write sample are the first two) and
/// its REST view serves what the live tree holds.
pub async fn mount_pair_backup(server: &wiremock::MockServer, data: &Path, syncs: bool) {
    use serde_json::json;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, Request, ResponseTemplate};

    let reads = Arc::new(AtomicUsize::new(0));
    Mock::given(method("GET"))
        .and(path("/data/api/v1/redundancy"))
        .respond_with(move |_: &Request| {
            let n = reads.fetch_add(1, Ordering::SeqCst) + 1;
            let moved = syncs && n >= 3;
            ResponseTemplate::new(200).set_body_json(json!({
                "role": "Backup", "projectState": "Good", "activityLevel": "Cold",
                "localId": "10.0.0.2", "peerId": "dev", "peerConnected": syncs,
                "hasConfigAccess": true, "syncPending": false, "failoverPending": false,
                "uptime": -1,
                "lastSyncTimestamp": 1_792_000_000_000_i64 + if moved { 5_000 } else { 0 }
            }))
        })
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
        .and(path("/data/api/v1/logs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [],
            "metadata": {"total": 0, "matching": 0, "limit": 200, "offset": 0}
        })))
        .mount(server)
        .await;
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
}

/// `ign [--json] [--yes] deploy rollback --env dev --run RUN --repo REPO`.
pub fn rollback_args(repo: &Path, run: &str, json: bool, yes: bool) -> Vec<std::ffi::OsString> {
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
pub fn config_tree(
    tree: std::collections::BTreeMap<String, Vec<u8>>,
) -> std::collections::BTreeMap<String, Vec<u8>> {
    tree.into_iter()
        .filter(|(path, _)| path.starts_with("config/"))
        .collect()
}

/// Run `ign deploy apply --yes` through the binary over `repo` and return the
/// applied run's id.
pub async fn apply_through_binary(
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

/// Plant the journal a killed apply leaves for `run` on the local host: the
/// run's own record supplies the touched set and the pre-run fingerprint, the
/// phase is `written` (every move done, not finalized). The snapshot the
/// recovery restores from is the one the apply took.
pub fn plant_journal(data: &Path, run: &str) {
    plant_journal_at(
        data,
        run,
        ignition_core::deploy::apply::journal::JournalPhase::Written,
    );
}

/// [`plant_journal`] at an explicit phase.
pub fn plant_journal_at(
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

/// Change the live unit once more after the apply (a Designer edit, say).
pub fn edit_live_unit(data: &Path) {
    let config = data
        .join("config/resources/dev")
        .join(APPLY_UNIT)
        .join("config.json");
    let mut bytes = std::fs::read(&config).expect("live config");
    bytes.extend_from_slice(b"\n\n");
    std::fs::write(config, bytes).expect("edit the live unit");
}
