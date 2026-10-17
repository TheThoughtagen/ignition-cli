//! Phase 22 success criterion 1 against a real Windows OpenSSH host (22-13,
//! D-15): the read half, the write half on a scratch collection, PowerShell
//! execution and per-call timeouts, as the non-admin service account with the
//! pinned host key.
//!
//! The capture host is the Azure VM `ign-win-836` that stands in for WFLWVM01
//! by the user decision of 2026-10-14 (22-CONTEXT amendment A-01). The tests
//! drive the core API directly: nothing here goes through a verb, and the
//! write half (`mirror_for_node`, `default_staging_mode`) has no other caller.
//!
//! ```text
//! source ~/.config/ign-azure-win/env
//! export IGNITION_LIVE_WFLWVM01=1 IGN_WFLWVM01_HOST=$IGN_WINVM_HOST \
//!   IGN_WFLWVM01_KEY=$IGN_WINVM_KEY IGN_WFLWVM01_PROFILE=$IGN_WINVM_PROFILE
//! cargo test -p ignition-core --test live_transport_wflwvm01 -- --ignored --nocapture --test-threads=1
//! ```
//!
//! | var | meaning |
//! |---|---|
//! | `IGNITION_LIVE_WFLWVM01` | must equal `1`; anything else is a quiet green skip |
//! | `IGN_WFLWVM01_HOST` | the host name or address |
//! | `IGN_WFLWVM01_USER` | the login (default `svc-ignition-deploy`) |
//! | `IGN_WFLWVM01_KEY` | the private key TEXT; read by the transport from this variable, never printed |
//! | `IGN_WFLWVM01_PROFILE` | the `ign` profile of the gateway (only named in the target; no gateway request is made) |
//! | `IGN_WFLWVM01_KNOWN_HOSTS` | the pinned known_hosts file (default `$HOME/.cache/ign-wflwvm01/known_hosts`) |
//! | `IGN_WFLWVM01_DATA_DIR` | the gateway data directory (default the captured `C:\Program Files\Inductive Automation\Ignition\data`) |
//!
//! ## What is written, and where
//!
//! The read tests only read. The write test is bound to a scratch gateway
//! data directory under the account's own home, `ign-live-scratch\data`, never
//! to the production collection (T-22-58), and `default_staging_mode` puts
//! the separate-mode staging in `ign-deploy\live-scratch\...` of the same
//! home. Both are removed at the start (leftovers) and at the end of every
//! test that creates them; the long-path and junction test uses PowerShell
//! with the `\\?\` prefix to create and remove what sftp cannot. Output
//! carries counts, paths and timings, never a key.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ignition_core::actions::deploy::{
    PlanOptions, deploy_plan, deploy_pull, load_declared, load_target,
};
use ignition_core::client::DeployApi;
use ignition_core::client::scan::{ScanKind, scan_complete};
use ignition_core::deploy::apply::fingerprint::{LEDGER_KEY, fingerprint};
use ignition_core::deploy::apply::host::{ApplyHost, BatchSpec, DeleteOp, WriteOp};
use ignition_core::deploy::apply::journal::{Journal, JournalPhase};
use ignition_core::deploy::apply::record::{RecordState, RunRecord, TouchedEntry};
use ignition_core::deploy::apply::run_store::RunId;
use ignition_core::deploy::layout::RelPath;
use ignition_core::deploy::ledger::Ledger;
use ignition_core::deploy::target::Ssh;
use ignition_core::deploy::transport::batch::script_ps1_sha256;
use ignition_core::deploy::transport::process::TokioProcessRunner;
use ignition_core::deploy::transport::remote::{RemoteShell, SftpOp};
use ignition_core::deploy::transport::sftp::{SftpTransport, windows_sftp_root};
use ignition_core::deploy::transport::winps::{self, LONG_PATH_PREFIX, WinScript};
use ignition_core::deploy::transport::winps::{COMMAND_BUDGET, StubRoots, script_stub};
use ignition_core::deploy::transport::{self, DeleteSet, Scope, StagingMode, TreeReader};
use ignition_core::deploy::tree::{RenderedTree, sha256_hex};
use ignition_core::error::CoreError;
use ignition_core::session::Session;

const DEFAULT_DATA_DIR: &str = "C:\\Program Files\\Inductive Automation\\Ignition\\data";
const KEY_ENV: &str = "IGN_WFLWVM01_KEY";
/// The scratch tree below the account home; the only thing the write test
/// may create (besides `ign-deploy\live-scratch` through the default staging).
const SCRATCH: &str = "ign-live-scratch";
const GATEWAY: &str = "live-scratch";
const MODE: &str = "production";
const DC: &str = "ignition/database-connection";

fn skip(message: &str) {
    eprintln!("skipping: {message}");
}

/// What a live test needs once the gate is open.
struct Live {
    host: String,
    user: String,
    known_hosts: PathBuf,
    data_dir: String,
}

fn require_env(name: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| panic!("{name} must be set to run the live Windows transport gate"))
}

/// `None` is the quiet green skip.
fn live() -> Option<Live> {
    if std::env::var("IGNITION_LIVE_WFLWVM01").as_deref() != Ok("1") {
        skip("IGNITION_LIVE_WFLWVM01 is not 1");
        return None;
    }
    let host = require_env("IGN_WFLWVM01_HOST");
    require_env(KEY_ENV);
    require_env("IGN_WFLWVM01_PROFILE");
    let user = std::env::var("IGN_WFLWVM01_USER").unwrap_or_else(|_| "svc-ignition-deploy".into());
    let known_hosts = std::env::var("IGN_WFLWVM01_KNOWN_HOSTS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(std::env::var("HOME").expect("HOME"))
                .join(".cache/ign-wflwvm01/known_hosts")
        });
    assert!(
        known_hosts.is_file(),
        "the pinned known_hosts file {} must exist before anything touches the network",
        known_hosts.display()
    );
    let data_dir =
        std::env::var("IGN_WFLWVM01_DATA_DIR").unwrap_or_else(|_| DEFAULT_DATA_DIR.to_string());
    Some(Live {
        host,
        user,
        known_hosts,
        data_dir,
    })
}

/// A temp repo with the pinned `known_hosts` and `targets/<env>.yaml` whose
/// master is an ssh node with `os: windows` and `data_dir`.
fn repo(live: &Live, env: &str, data_dir: &str, op_timeout_s: Option<u32>) -> tempfile::TempDir {
    let repo = tempfile::tempdir().expect("temp repo");
    std::fs::create_dir_all(repo.path().join("targets")).expect("targets dir");
    std::fs::create_dir_all(repo.path().join("gateways")).expect("gateways dir");
    std::fs::copy(&live.known_hosts, repo.path().join("known_hosts")).expect("copy known_hosts");
    let timeout = op_timeout_s
        .map(|s| format!("      op_timeout_s: {s}\n"))
        .unwrap_or_default();
    let yaml = format!(
        "schema_version: 1\ngateway: {GATEWAY}\nmode: {MODE}\nnodes:\n  master:\n    profile: azure-win\n    ssh:\n      host: {host}\n      user: {user}\n      key_env: {KEY_ENV}\n      os: windows\n      data_dir: '{data_dir}'\n      known_hosts: known_hosts\n{timeout}",
        host = live.host,
        user = live.user,
    );
    std::fs::write(
        repo.path().join("targets").join(format!("{env}.yaml")),
        yaml,
    )
    .expect("target file");
    repo
}

fn ssh_of(repo: &Path, env: &str) -> (String, Ssh, ignition_core::deploy::target::NodeTarget) {
    let (target_file, target) = load_target(repo, env).expect("target");
    let node = target.nodes.master.clone();
    (target_file, node.ssh.clone().expect("ssh node"), node)
}

fn shell_of(repo: &Path, env: &str) -> RemoteShell {
    let (target_file, ssh, _) = ssh_of(repo, env);
    RemoteShell::from_ssh(
        repo,
        &target_file,
        "nodes.master",
        &ssh,
        Arc::new(TokioProcessRunner),
    )
    .expect("remote shell")
}

fn reader_of(repo: &Path, env: &str) -> Box<dyn TreeReader> {
    let (target_file, _, node) = ssh_of(repo, env);
    transport::for_node(repo, &target_file, "nodes.master", &node)
        .expect("a Windows ssh node builds a reader")
}

// ---- offline checks, run on every OS ----------------------------------------

#[test]
fn live_target_yaml_loads_as_a_windows_ssh_node() {
    let live = Live {
        host: "203.0.113.9".to_string(),
        user: "svc-ignition-deploy".to_string(),
        known_hosts: PathBuf::from("/nonexistent"),
        data_dir: DEFAULT_DATA_DIR.to_string(),
    };
    let repo = tempfile::tempdir().expect("repo");
    std::fs::create_dir_all(repo.path().join("targets")).expect("targets");
    std::fs::write(
        repo.path().join("known_hosts"),
        "203.0.113.9 ssh-ed25519 AAAA\n",
    )
    .expect("known_hosts");
    let yaml = format!(
        "schema_version: 1\ngateway: {GATEWAY}\nmode: {MODE}\nnodes:\n  master:\n    profile: azure-win\n    ssh:\n      host: {}\n      user: {}\n      key_env: {KEY_ENV}\n      os: windows\n      data_dir: '{}'\n      known_hosts: known_hosts\n      op_timeout_s: 1\n",
        live.host, live.user, live.data_dir
    );
    std::fs::write(repo.path().join("targets/live.yaml"), yaml).expect("target");
    let (_, target) = load_target(repo.path(), "live").expect("loads");
    let ssh = target.nodes.master.ssh.expect("ssh");
    assert_eq!(ssh.data_dir, DEFAULT_DATA_DIR);
    assert_eq!(ssh.op_timeout_s, Some(1));
    assert!(
        winps::enumerate_chunks(LONG_PATH_PREFIX, &[ssh.data_dir]).is_ok(),
        "the data dir fits an enumeration command"
    );
}

#[test]
fn the_scratch_data_dir_is_never_the_gateways() {
    // the write test builds its data dir from the account home; this pins the
    // shape so a refactor cannot point it at Program Files
    let scratch = scratch_native("C:\\Users\\svc-ignition-deploy");
    assert_eq!(
        scratch,
        "C:\\Users\\svc-ignition-deploy\\ign-live-scratch\\data"
    );
    assert!(!scratch.to_ascii_lowercase().contains("program files"));
    assert!(scratch.contains(SCRATCH));
}

// ---- helpers over the live host ---------------------------------------------

/// `Remote working directory:` of the account, as sftp spells it.
async fn home_sftp(shell: &RemoteShell) -> String {
    let out = shell
        .require_success(
            "pwd",
            shell.sftp_batch(&[SftpOp::Pwd]).await.expect("pwd batch"),
        )
        .expect("pwd");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.lines()
        .find_map(|l| l.strip_prefix("Remote working directory: "))
        .expect("a working directory line")
        .trim_end_matches('/')
        .to_string()
}

/// `/C:/Users/u` to `C:\Users\u`.
fn native(sftp: &str) -> String {
    sftp.trim_start_matches('/').replace('/', "\\")
}

fn scratch_native(home_native: &str) -> String {
    format!("{home_native}\\{SCRATCH}\\data")
}

async fn sftp(shell: &RemoteShell, ops: &[SftpOp]) -> String {
    let out = shell.sftp_batch(ops).await.expect("sftp spawn");
    let out = shell.require_success("live", out).expect("sftp batch");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Test-local scripts: fixed `const` bodies run through the same typed
/// `exec_powershell` as the product's own. They create and remove what sftp
/// cannot (a path of 260 characters or more, a junction).
const MAKE_LONG: &str = "$d=$p.x+$p.path;[IO.Directory]::CreateDirectory($d)|Out-Null;[IO.File]::WriteAllText($d+'\\config.json','{}');E @{ok=1}";
const MAKE_JUNCTION: &str = "$j=$p.x+$p.link;$t=$p.x+$p.target;[IO.Directory]::CreateDirectory($t)|Out-Null;cmd /c mklink /J $j $t|Out-Null;if($LASTEXITCODE -ne 0){throw 'mklink failed'};E @{ok=1}";
const DROP_JUNCTION: &str =
    "$j=$p.x+$p.link;if([IO.Directory]::Exists($j)){[IO.Directory]::Delete($j)};E @{ok=1}";
const REMOVE_TREE: &str = "$d=$p.x+$p.path;if([IO.Directory]::Exists($d)){Remove-Item -LiteralPath $d -Recurse -Force};E @{ok=1;gone=[int](-not [IO.Directory]::Exists($d))}";
const REMOVE_IF_EMPTY: &str = "$d=$p.x+$p.path;if([IO.Directory]::Exists($d) -and -not (Get-ChildItem -LiteralPath $d -Force)){Remove-Item -LiteralPath $d};E @{ok=1}";
const SLEEP: &str = "Start-Sleep 10;E @{ok=1}";
const FACTS: &str =
    "E @{ok=1;user=$env:USERNAME;major=$PSVersionTable.PSVersion.Major;s=('caf'+[char]233)}";
const EXIT_SEVEN: &str = "exit 7";
const THROW: &str = "throw 'boom from the script'";

fn script(name: &'static str, body: &'static str, params: serde_json::Value) -> WinScript {
    WinScript { name, body, params }
}

fn path_params(path: &str) -> serde_json::Value {
    serde_json::json!({ "x": LONG_PATH_PREFIX, "path": path })
}

/// Remove `path` (native) with the long-path prefix. Returns whether the
/// host reports it gone.
async fn remove_tree(shell: &RemoteShell, path: &str) -> bool {
    let value = shell
        .exec_powershell(&script("remove_tree", REMOVE_TREE, path_params(path)))
        .await
        .expect("remove_tree");
    value["gone"] == 1
}

/// Every entry below `root` (native) as `(rel, is_dir, size)`; `None` when
/// the root is missing.
async fn enumerate(shell: &RemoteShell, root: &str) -> Option<Vec<(String, bool, u64)>> {
    let value = shell
        .exec_powershell(&WinScript::enumerate(LONG_PATH_PREFIX, &[root.to_string()]))
        .await
        .expect("enumerate");
    let mut got = winps::parse_enumeration(&value, 1).expect("enumeration");
    got.remove(0).map(|entries| {
        entries
            .into_iter()
            .map(|e| (e.rel, e.dir, e.size))
            .collect()
    })
}

fn rel(text: &str) -> RelPath {
    RelPath::new(text).expect("rel path")
}

fn tree(files: &[(&str, &str)]) -> RenderedTree {
    RenderedTree {
        files: files
            .iter()
            .map(|(p, b)| (rel(p), b.as_bytes().to_vec()))
            .collect(),
    }
}

fn scope() -> Scope {
    Scope {
        mode: MODE.to_string(),
        types: vec![("ignition".to_string(), "database-connection".to_string())],
        tag_providers: Vec::new(),
    }
}

fn mode_files(observed: &transport::ObservedFiles) -> Vec<(String, String)> {
    observed
        .mode
        .iter()
        .map(|(path, blob)| {
            (
                path.to_string(),
                String::from_utf8_lossy(&blob.bytes).into_owned(),
            )
        })
        .collect()
}

fn timed<T>(name: &str, started: Instant, value: T) -> T {
    eprintln!("TIMING {name}: {:.2}s", started.elapsed().as_secs_f64());
    value
}

/// Remove the scratch tree and the B(i) staging tree (and `ign-deploy` when
/// nothing else is in it). Returns whether the host reports each gone.
async fn cleanup_scratch(
    shell: &RemoteShell,
    scratch_native: &str,
    staging_native: &str,
    home_native: &str,
) -> (bool, bool) {
    let a = remove_tree(shell, scratch_native).await;
    let b = remove_tree(shell, staging_native).await;
    shell
        .exec_powershell(&script(
            "remove_if_empty",
            REMOVE_IF_EMPTY,
            path_params(&format!("{home_native}\\ign-deploy")),
        ))
        .await
        .expect("remove ign-deploy if empty");
    (a, b)
}

/// After a commit: `config-mode.json` is byte-identical and nothing below
/// the collection is dot-named. Returns the number of entries below it.
async fn check_invariants(
    shell: &RemoteShell,
    prod_sftp: &str,
    prod_native: &str,
    local_copy: &Path,
    mode_sha: &str,
    label: &str,
) -> usize {
    sftp(
        shell,
        &[SftpOp::Get {
            remote: format!("{prod_sftp}/config-mode.json"),
            local: local_copy.to_path_buf(),
        }],
    )
    .await;
    let bytes = std::fs::read(local_copy).expect("get-back");
    assert_eq!(
        sha256_hex(&bytes),
        mode_sha,
        "{label}: config-mode.json changed"
    );
    let entries = enumerate(shell, prod_native)
        .await
        .expect("production exists");
    let dotted: Vec<&String> = entries
        .iter()
        .map(|(rel, _, _)| rel)
        .filter(|rel| rel.split('/').any(|c| c.starts_with('.')))
        .collect();
    assert!(dotted.is_empty(), "{label}: dot-named entries: {dotted:?}");
    entries.len()
}

// ---- the tests --------------------------------------------------------------

#[tokio::test]
#[ignore = "live: needs the Windows capture host and IGNITION_LIVE_WFLWVM01=1"]
async fn wflwvm01_pull_render_plan_is_empty() {
    let Some(live) = live() else { return };
    let started = Instant::now();
    let repo = repo(&live, "live", &live.data_dir, None);
    let reader = reader_of(repo.path(), "live");

    let t = Instant::now();
    let pulled = deploy_pull(repo.path(), "live", reader.as_ref(), false)
        .await
        .expect("deploy pull over ssh from the production collection");
    timed("pull", t, ());
    eprintln!(
        "pull: {} files written, manifest {}, {} skipped",
        pulled.files_written,
        pulled.manifest,
        pulled.skipped.len()
    );
    for skipped in &pulled.skipped {
        eprintln!("  skipped {} ({})", skipped.path, skipped.reason);
    }
    assert_eq!(pulled.manifest, "generated");
    assert!(
        pulled.files_written > 0,
        "a commissioned gateway holds resources; pull wrote none"
    );

    let t = Instant::now();
    let declared = load_declared(repo.path(), "live").expect("render of the pulled repo");
    timed("render", t, ());
    let t = Instant::now();
    let result = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect("plan over ssh");
    timed("plan", t, ());
    eprintln!(
        "plan: transport {} added {} changed {} removed {} same {} unmanaged {} has_changes {}",
        result.transport,
        result.summary.added,
        result.summary.changed,
        result.summary.removed,
        result.summary.same,
        result.summary.unmanaged,
        result.summary.has_changes
    );
    assert_eq!(result.transport, "ssh");
    assert!(
        !result.summary.has_changes,
        "pull, render, plan over ssh must report no changes"
    );
    assert!(result.summary.same > 0, "no unit was compared");
    timed("wflwvm01_pull_render_plan_is_empty total", started, ());
    eprintln!("PASS wflwvm01_pull_render_plan_is_empty");
}

#[tokio::test]
#[ignore = "live: needs the Windows capture host and IGNITION_LIVE_WFLWVM01=1"]
async fn wflwvm01_write_half_scratch() {
    let Some(live) = live() else { return };
    let started = Instant::now();
    // the account home decides the scratch data dir; nothing here names the
    // gateway's own data dir (T-22-58)
    let probe_repo = repo(&live, "probe", &live.data_dir, None);
    let shell = shell_of(probe_repo.path(), "probe");
    let home = home_sftp(&shell).await;
    let home_native = native(&home);
    let data_native = scratch_native(&home_native);
    assert!(
        data_native.contains(SCRATCH)
            && !data_native.to_ascii_lowercase().contains("program files"),
        "{data_native}"
    );
    let staging_native = format!("{home_native}\\ign-deploy\\{GATEWAY}");
    let scratch_root_native = format!("{home_native}\\{SCRATCH}");
    // leftovers of a failed earlier run
    let (a, b) = cleanup_scratch(&shell, &scratch_root_native, &staging_native, &home_native).await;
    assert!(a && b, "leftovers removed before the run");

    // the scratch gateway data dir: config/resources/production with the
    // gateway-owned config-mode.json and two pre-existing resources
    let res = format!("{home}/{SCRATCH}/data/config/resources");
    let prod = format!("{res}/production");
    let local = tempfile::tempdir().expect("local temp");
    let write = |name: &str, bytes: &str| -> PathBuf {
        let path = local.path().join(name);
        std::fs::write(&path, bytes).expect("local file");
        path
    };
    let mode_json = "{\"mode\":\"production\",\"scratch\":true}";
    let setup_ops = vec![
        SftpOp::Mkdir {
            path: format!("{home}/{SCRATCH}"),
        },
        SftpOp::Mkdir {
            path: format!("{home}/{SCRATCH}/data"),
        },
        SftpOp::Mkdir {
            path: format!("{home}/{SCRATCH}/data/config"),
        },
        SftpOp::Mkdir { path: res.clone() },
        SftpOp::Mkdir { path: prod.clone() },
        SftpOp::Put {
            local: write("mode.json", mode_json),
            remote: format!("{prod}/config-mode.json"),
        },
        SftpOp::Mkdir {
            path: format!("{prod}/ignition"),
        },
        SftpOp::Mkdir {
            path: format!("{prod}/ignition/database-connection"),
        },
        SftpOp::Mkdir {
            path: format!("{prod}/ignition/database-connection/Existing"),
        },
        SftpOp::Put {
            local: write("existing.json", "{\"pre\":\"existing\"}"),
            remote: format!("{prod}/ignition/database-connection/Existing/config.json"),
        },
        SftpOp::Mkdir {
            path: format!("{prod}/ignition/database-connection/ReplaceMe"),
        },
        SftpOp::Put {
            local: write("old.json", "{\"pre\":\"old\"}"),
            remote: format!("{prod}/ignition/database-connection/ReplaceMe/config.json"),
        },
        SftpOp::Mkdir {
            path: format!("{prod}/ignition/database-connection/Doomed"),
        },
        SftpOp::Put {
            local: write("doomed.json", "{\"pre\":\"doomed\"}"),
            remote: format!("{prod}/ignition/database-connection/Doomed/config.json"),
        },
    ];
    sftp(&shell, &setup_ops).await;

    let repo = repo(&live, "scratch", &data_native, None);
    let (target_file, _, node) = ssh_of(repo.path(), "scratch");
    let reader = reader_of(repo.path(), "scratch");
    let prod_native = format!("{data_native}\\config\\resources\\production");
    let mode_sha = sha256_hex(mode_json.as_bytes());

    // ---- round 1: Separate staging at the Decision B(i) location
    let t = Instant::now();
    let staging = transport::default_staging_mode(
        repo.path(),
        &target_file,
        "nodes.master",
        &node,
        GATEWAY,
        MODE,
        "live-run-1",
    )
    .await
    .expect("default staging");
    let StagingMode::Separate { root } = &staging else {
        panic!("Decision B(i) default is a separate staging root");
    };
    assert_eq!(
        root,
        &format!("{home}/ign-deploy/{GATEWAY}/{MODE}/staging/live-run-1"),
        "the default staging root is in the account home"
    );
    let mirror = transport::mirror_for_node(
        repo.path(),
        &target_file,
        "nodes.master",
        &node,
        MODE,
        staging,
    )
    .expect("mirror");
    let round1 = tree(&[
        (&format!("{DC}/Caf\u{e9} DB,1/config.json"), "{\"round\":1}"),
        (
            &format!("{DC}/Caf\u{e9} DB,1/resource.json"),
            "{\"scope\":\"A\"}",
        ),
        (&format!("{DC}/ReplaceMe/config.json"), "{\"pre\":\"new\"}"),
    ]);
    let staged = mirror.stage(&round1).await.expect("stage");
    let report = mirror
        .commit(
            &staged,
            &DeleteSet {
                files: vec![rel(&format!("{DC}/Doomed/config.json"))],
            },
        )
        .await
        .expect("commit (separate)");
    timed("write half, separate: stage + commit", t, ());
    eprintln!(
        "commit(separate): written {:?} deleted {:?} removed_dirs {:?}",
        report.written, report.deleted, report.removed_dirs
    );
    assert_eq!(report.written.len(), 3);
    assert_eq!(report.deleted, [format!("{DC}/Doomed/config.json")]);
    assert_eq!(report.removed_dirs, [format!("{DC}/Doomed")]);

    let t = Instant::now();
    let observed = reader.read(&scope()).await.expect("read back over sftp");
    timed("read back (listing + walk + download)", t, ());
    assert_eq!(
        mode_files(&observed),
        [
            (
                format!("{DC}/Caf\u{e9} DB,1/config.json"),
                "{\"round\":1}".to_string()
            ),
            (
                format!("{DC}/Caf\u{e9} DB,1/resource.json"),
                "{\"scope\":\"A\"}".to_string()
            ),
            (
                format!("{DC}/Existing/config.json"),
                "{\"pre\":\"existing\"}".to_string()
            ),
            (
                format!("{DC}/ReplaceMe/config.json"),
                "{\"pre\":\"new\"}".to_string()
            ),
        ],
        "replaced, written, kept and deleted exactly as asked"
    );
    let entries1 = check_invariants(
        &shell,
        &prod,
        &prod_native,
        &local.path().join("mode-after-separate.json"),
        &mode_sha,
        "after-separate",
    )
    .await;

    // ---- round 2: InPlace
    let t = Instant::now();
    let mirror = transport::mirror_for_node(
        repo.path(),
        &target_file,
        "nodes.master",
        &node,
        MODE,
        StagingMode::InPlace,
    )
    .expect("in-place mirror");
    let round2 = tree(&[
        (&format!("{DC}/Caf\u{e9} DB,1/config.json"), "{\"round\":2}"),
        (&format!("{DC}/Another One/config.json"), "{\"round\":2}"),
    ]);
    let staged = mirror.stage(&round2).await.expect("stage");
    let report = mirror
        .commit(&staged, &DeleteSet::default())
        .await
        .expect("commit (in place)");
    timed("write half, in place: stage + commit", t, ());
    eprintln!("commit(in place): written {:?}", report.written);
    let observed = reader.read(&scope()).await.expect("read back");
    let files = mode_files(&observed);
    assert!(files.contains(&(
        format!("{DC}/Caf\u{e9} DB,1/config.json"),
        "{\"round\":2}".to_string()
    )));
    assert!(files.contains(&(
        format!("{DC}/Another One/config.json"),
        "{\"round\":2}".to_string()
    )));
    assert!(files.contains(&(
        format!("{DC}/Existing/config.json"),
        "{\"pre\":\"existing\"}".to_string()
    )));
    assert_eq!(files.len(), 5);
    let entries2 = check_invariants(
        &shell,
        &prod,
        &prod_native,
        &local.path().join("mode-after-in-place.json"),
        &mode_sha,
        "after-in-place",
    )
    .await;
    assert!(entries2 > entries1);

    // ---- cleanup, with evidence
    let (scratch_gone, staging_gone) =
        cleanup_scratch(&shell, &scratch_root_native, &staging_native, &home_native).await;
    eprintln!("cleanup: scratch gone {scratch_gone}, staging gone {staging_gone}");
    assert!(scratch_gone && staging_gone);
    assert!(
        enumerate(&shell, &scratch_root_native).await.is_none(),
        "the scratch tree is gone"
    );
    assert!(
        enumerate(&shell, &staging_native).await.is_none(),
        "the staging tree is gone"
    );
    timed("wflwvm01_write_half_scratch total", started, ());
    eprintln!("PASS wflwvm01_write_half_scratch");
}

#[tokio::test]
#[ignore = "live: needs the Windows capture host and IGNITION_LIVE_WFLWVM01=1"]
async fn wflwvm01_exec_powershell() {
    let Some(live) = live() else { return };
    let started = Instant::now();
    let repo = repo(&live, "exec", &live.data_dir, None);
    let shell = shell_of(repo.path(), "exec");

    // a fixed read-only script returns its sentinel; PowerShell 5.1 writes
    // OEM code page 437 on stdout, so the result must arrive intact as ASCII
    let t = Instant::now();
    let facts = shell
        .exec_powershell(&script("facts", FACTS, serde_json::Value::Null))
        .await
        .expect("facts");
    timed("exec_powershell (read-only)", t, ());
    eprintln!("facts: {facts}");
    assert_eq!(facts["ok"], 1);
    assert_eq!(facts["s"], "caf\u{e9}", "non-ASCII survives code page 437");
    assert!(
        facts["user"]
            .as_str()
            .is_some_and(|u| u.eq_ignore_ascii_case(&live.user)),
        "runs as the service account"
    );

    // a script that exits 7 is transport_failed
    let err = shell
        .exec_powershell(&script("exit_seven", EXIT_SEVEN, serde_json::Value::Null))
        .await
        .expect_err("exit 7");
    let CoreError::TransportFailed { detail, .. } = &err else {
        panic!("{err:?}");
    };
    eprintln!("exit 7 -> {err}");
    assert!(detail.contains("exit code 7"), "{detail}");

    // a terminating error: error CLIXML on stderr, decoded for display
    let err = shell
        .exec_powershell(&script("throw", THROW, serde_json::Value::Null))
        .await
        .expect_err("throw");
    let CoreError::TransportFailed { detail, .. } = &err else {
        panic!("{err:?}");
    };
    eprintln!("throw -> {err}");
    assert!(detail.contains("boom from the script"), "{detail}");
    assert!(!detail.contains("<S S="), "{detail}");

    // a hostile parameter is data, never script text
    let home = home_sftp(&shell).await;
    let scratch = format!("{}\\ign-live-exec", native(&home));
    remove_tree(&shell, &scratch).await;
    let src = format!("{home}/ign-live-exec/src file;'$(x).txt");
    let dst = format!("{home}/ign-live-exec/dst.txt");
    let local = tempfile::tempdir().expect("local");
    std::fs::write(local.path().join("a.txt"), "alpha").expect("a");
    std::fs::write(local.path().join("b.txt"), "bravo").expect("b");
    sftp(
        &shell,
        &[
            SftpOp::Mkdir {
                path: format!("{home}/ign-live-exec"),
            },
            SftpOp::Put {
                local: local.path().join("a.txt"),
                remote: src.clone(),
            },
            SftpOp::Put {
                local: local.path().join("b.txt"),
                remote: dst.clone(),
            },
        ],
    )
    .await;
    let t = Instant::now();
    let moved = shell
        .exec_powershell(&WinScript::move_replace(
            &format!("{scratch}\\src file;'$(x).txt"),
            &format!("{scratch}\\dst.txt"),
        ))
        .await
        .expect("move_replace");
    timed("MOVE_REPLACE", t, ());
    assert_eq!(moved["ok"], 1);
    sftp(
        &shell,
        &[SftpOp::Get {
            remote: dst.clone(),
            local: local.path().join("back.txt"),
        }],
    )
    .await;
    assert_eq!(
        std::fs::read_to_string(local.path().join("back.txt")).expect("back"),
        "alpha",
        "Move-Item -Force replaced the existing target"
    );
    let listing = sftp(
        &shell,
        &[
            SftpOp::Pwd,
            SftpOp::LsLongAll {
                path: format!("{home}/ign-live-exec"),
            },
        ],
    )
    .await;
    assert!(
        !listing.contains("src file"),
        "the source was moved: {listing}"
    );
    // a missing source is transport_failed
    let err = shell
        .exec_powershell(&WinScript::move_replace(
            &format!("{scratch}\\absent.txt"),
            &format!("{scratch}\\dst.txt"),
        ))
        .await
        .expect_err("missing source");
    assert!(matches!(err, CoreError::TransportFailed { .. }));
    let gone = remove_tree(&shell, &scratch).await;
    eprintln!("cleanup: ign-live-exec gone {gone}");
    assert!(gone);
    timed("wflwvm01_exec_powershell total", started, ());
    eprintln!("PASS wflwvm01_exec_powershell");
}

#[tokio::test]
#[ignore = "live: needs the Windows capture host and IGNITION_LIVE_WFLWVM01=1"]
async fn wflwvm01_timeout_is_transport_failed() {
    let Some(live) = live() else { return };
    let started = Instant::now();
    // one second per call: a remote `Start-Sleep 10` cannot finish
    let repo = repo(&live, "slow", &live.data_dir, Some(1));
    let shell = shell_of(repo.path(), "slow");
    assert_eq!(shell.op_timeout(), Duration::from_secs(1));
    let t = Instant::now();
    let err = shell
        .exec_powershell(&script("sleep", SLEEP, serde_json::Value::Null))
        .await
        .expect_err("a forced timeout");
    let elapsed = t.elapsed();
    timed("forced timeout", t, ());
    let CoreError::TransportFailed { detail, .. } = &err else {
        panic!("{err:?}");
    };
    eprintln!("timeout -> {err}");
    assert!(detail.contains("timed out after 1s"), "{detail}");
    assert!(
        elapsed < Duration::from_secs(6),
        "the call was killed at the timeout, not at the end of the sleep: {elapsed:?}"
    );
    // the same on an sftp call: nothing can finish a handshake and a listing
    // in one second reliably, so only the classification is asserted when it
    // does time out
    let out = shell.sftp_batch(&[SftpOp::Pwd]).await.expect("spawn");
    if out.timed_out {
        let err = shell.require_success("read", out).expect_err("timed out");
        assert!(matches!(err, CoreError::TransportFailed { .. }));
    }
    timed("wflwvm01_timeout_is_transport_failed total", started, ());
    eprintln!("PASS wflwvm01_timeout_is_transport_failed");
}

#[tokio::test]
#[ignore = "live: needs the Windows capture host and IGNITION_LIVE_WFLWVM01=1"]
async fn wflwvm01_long_paths_and_junctions_fail_closed() {
    let Some(live) = live() else { return };
    let started = Instant::now();
    let probe_repo = repo(&live, "probe", &live.data_dir, None);
    let shell = shell_of(probe_repo.path(), "probe");
    let home = home_sftp(&shell).await;
    let home_native = native(&home);
    let scratch_root = format!("{home_native}\\{SCRATCH}-long");
    let data_native = format!("{scratch_root}\\data");
    assert!(remove_tree(&shell, &scratch_root).await);

    // the scratch gateway data dir
    let res = format!("{home}/{SCRATCH}-long/data/config/resources");
    sftp(
        &shell,
        &[
            SftpOp::Mkdir {
                path: format!("{home}/{SCRATCH}-long"),
            },
            SftpOp::Mkdir {
                path: format!("{home}/{SCRATCH}-long/data"),
            },
            SftpOp::Mkdir {
                path: format!("{home}/{SCRATCH}-long/data/config"),
            },
            SftpOp::Mkdir { path: res.clone() },
            SftpOp::Mkdir {
                path: format!("{res}/production"),
            },
            SftpOp::Mkdir {
                path: format!("{res}/production/ignition"),
            },
            SftpOp::Mkdir {
                path: format!("{res}/production/ignition/database-connection"),
            },
        ],
    )
    .await;
    let repo = repo(&live, "longs", &data_native, None);
    let reader = reader_of(repo.path(), "longs");
    let dc_native =
        format!("{data_native}\\config\\resources\\production\\ignition\\database-connection");

    // a clean tree reads
    assert!(
        reader.read(&scope()).await.is_ok(),
        "an empty collection reads"
    );

    // (1) a 200 character resource name: the full path of its config.json is
    // over 300 characters, so the sftp listing silently omits it (C7a-8)
    let long_name = "L".repeat(200);
    let long_path = format!("{dc_native}\\{long_name}");
    shell
        .exec_powershell(&script("make_long", MAKE_LONG, path_params(&long_path)))
        .await
        .expect("create the long path with the \\\\?\\ prefix");
    let t = Instant::now();
    let err = reader
        .read(&scope())
        .await
        .expect_err("a read must not be reported complete");
    timed("read refused (long path)", t, ());
    let CoreError::TransportFailed { detail, .. } = &err else {
        panic!("{err:?}");
    };
    eprintln!(
        "long path -> {}",
        detail.chars().take(300).collect::<String>()
    );
    assert!(
        detail.contains("did not return") || detail.contains("characters"),
        "{detail}"
    );
    assert!(remove_tree(&shell, &long_path).await, "long path removed");
    assert!(
        reader.read(&scope()).await.is_ok(),
        "reads again once it is gone"
    );

    // (2) a junction: sftp shows a plain directory and get -R would follow it
    shell
        .exec_powershell(&script(
            "make_junction",
            MAKE_JUNCTION,
            serde_json::json!({
                "x": LONG_PATH_PREFIX,
                "link": format!("{dc_native}\\Linked"),
                "target": format!("{scratch_root}\\junction-target"),
            }),
        ))
        .await
        .expect("create the junction");
    let err = reader
        .read(&scope())
        .await
        .expect_err("a junction is refused");
    let CoreError::TransportFailed { detail, .. } = &err else {
        panic!("{err:?}");
    };
    eprintln!("junction -> {detail}");
    assert!(detail.contains("reparse point"), "{detail}");
    // the link is removed first, so the recursive removal below cannot follow it
    shell
        .exec_powershell(&script(
            "drop_junction",
            DROP_JUNCTION,
            serde_json::json!({ "x": LONG_PATH_PREFIX, "link": format!("{dc_native}\\Linked") }),
        ))
        .await
        .expect("drop the junction");
    assert!(
        reader.read(&scope()).await.is_ok(),
        "reads again once it is gone"
    );

    let gone = remove_tree(&shell, &scratch_root).await;
    eprintln!("cleanup: {SCRATCH}-long gone {gone}");
    assert!(gone);
    assert!(enumerate(&shell, &scratch_root).await.is_none());
    timed(
        "wflwvm01_long_paths_and_junctions_fail_closed total",
        started,
        (),
    );
    eprintln!("PASS wflwvm01_long_paths_and_junctions_fail_closed");
}

// ---- OQ5: transport and scan timings (22-14) --------------------------------
//
// A capture-only harness. It drives the transport library directly (never a
// verb): `mirror_for_node` stage and commit in the Decision B(i) staging mode,
// `SftpTransport` reads, and the gateway's own config scan through the
// profile's `DeployApi`. The write half it uses has no other caller.
//
// ```text
// IGNITION_LIVE_WFLWVM01=1 IGNITION_LIVE_OQ5=1 IGN_OQ5_LABEL=A \
//   cargo test -p ignition-core --test live_transport_wflwvm01 oq5_timings -- --ignored --nocapture
// ```
//
// | var | meaning |
// |---|---|
// | `IGNITION_LIVE_OQ5` | must equal `1` |
// | `IGN_OQ5_LABEL` | `A` (Defender exclusion off) or `B` (on); printed on every line |
// | `IGN_OQ5_SOURCE_REPO`, `IGN_OQ5_SOURCE_ENV` | optional D-16 source: a repo and env whose target is pulled into a temp copy and rendered; absent, the sanitized corpus scaled up is the payload |
// | `IGN_OQ5_SCALE` | corpus copies (default 10) |
// | `IGN_OQ5_ITERS` | stage / commit / scan iterations (default 10) |
// | `IGN_OQ5_READS` | timed reads with the payload present (default 20) |
// | `IGN_OQ5_CONNECT_SAMPLES` | timed `pwd` sessions (default 20) |
// | `IGN_OQ5_OP_TIMEOUT_S` | per-call timeout of the harness target (default 1800, so a slow call is measured, not killed) |
//
// What it writes: the payload into the `production` collection (through
// staging in the account home), and removes it again through the delete set.
// Singleton units are never written (a copy would override the live gateway's
// own settings); only named units, each copy under a distinct unit name. The
// restore is checked against a hash listing taken before anything is written
// (T-22-59).

type Res<T> = Result<T, String>;

fn err_s(e: impl std::fmt::Debug) -> String {
    format!("{e:?}")
}

/// What the harness writes, and how it is described in the captures.
struct Payload {
    tree: RenderedTree,
    files: usize,
    bytes: usize,
    units: usize,
    source: String,
    types: Vec<(String, String)>,
}

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/deploy/corpus")
}

fn walk_corpus(dir: &Path, base: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .expect("corpus dir")
        .map(|e| e.expect("entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            walk_corpus(&path, base, out);
        } else {
            let rel = path
                .strip_prefix(base)
                .expect("below base")
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, std::fs::read(&path).expect("corpus file")));
        }
    }
}

/// A uuid for copy `k` of `original`, derived so every copy is distinct and
/// every run produces the same one.
fn derived_uuid(original: &str, k: usize) -> String {
    let h = sha256_hex(format!("{original}:{k}").as_bytes());
    format!(
        "{}-{}-4{}-a{}-{}",
        &h[0..8],
        &h[8..12],
        &h[13..16],
        &h[17..20],
        &h[20..32]
    )
}

/// `"uuid": "<36 chars>"` values replaced for copy `k`; other bytes untouched.
fn retag_uuids(bytes: &[u8], k: usize) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return bytes.to_vec();
    };
    const MARK: &str = "\"uuid\": \"";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(MARK) {
        let value_at = at + MARK.len();
        out.push_str(&rest[..value_at]);
        match rest.get(value_at..value_at + 36) {
            Some(original) if rest[value_at + 36..].starts_with('"') => {
                out.push_str(&derived_uuid(original, k));
                rest = &rest[value_at + 36..];
            }
            _ => rest = &rest[value_at..],
        }
    }
    out.push_str(rest);
    out.into_bytes()
}

/// The sanitized corpus scaled `scale` times: every named unit
/// (`module/type/name/<file>`) once per copy as `name-oq5-NN`, with its uuids
/// re-derived. Singletons and the gateway-owned `config-mode.json` are left
/// out.
fn corpus_payload(scale: usize) -> Payload {
    let base = corpus_dir();
    let mut files = Vec::new();
    walk_corpus(&base, &base, &mut files);
    let mut tree = RenderedTree::default();
    let mut units: BTreeSet<String> = BTreeSet::new();
    let mut types: BTreeSet<(String, String)> = BTreeSet::new();
    for k in 1..=scale {
        for (path, bytes) in &files {
            let parts: Vec<&str> = path.split('/').collect();
            if parts.len() != 4 {
                continue;
            }
            let name = format!("{}-oq5-{k:02}", parts[2]);
            let new_path = format!("{}/{}/{}/{}", parts[0], parts[1], name, parts[3]);
            let body = if parts[3] == "resource.json" {
                retag_uuids(bytes, k)
            } else {
                bytes.clone()
            };
            units.insert(format!("{}/{}/{}", parts[0], parts[1], name));
            types.insert((parts[0].to_string(), parts[1].to_string()));
            tree.files.insert(rel(&new_path), body);
        }
    }
    Payload {
        files: tree.files.len(),
        bytes: tree.bytes(),
        units: units.len(),
        source: format!("sanitized corpus (183 .json, 195 files) x {scale}, named units only"),
        types: types.into_iter().collect(),
        tree,
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create");
    for entry in std::fs::read_dir(from).expect("read_dir") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy");
        }
    }
}

/// The D-16 payload from a real source: `deploy_pull` of the source target
/// into a temp copy of its repo, rendered.
async fn pulled_payload(source_repo: &Path, env: &str) -> Payload {
    let tmp = tempfile::tempdir().expect("temp repo");
    copy_dir(source_repo, tmp.path());
    let reader = reader_of(tmp.path(), env);
    let pulled = deploy_pull(tmp.path(), env, reader.as_ref(), false)
        .await
        .expect("deploy pull from the D-16 source");
    let declared = load_declared(tmp.path(), env).expect("render of the pulled tree");
    let tree = declared.outcome.tree;
    let mut types: BTreeSet<(String, String)> = BTreeSet::new();
    let mut units: BTreeSet<String> = BTreeSet::new();
    for path in tree.files.keys() {
        let parts: Vec<&str> = path.as_str().split('/').collect();
        if parts.len() >= 3 {
            types.insert((parts[0].to_string(), parts[1].to_string()));
            units.insert(parts[..3].join("/"));
        }
    }
    Payload {
        files: tree.files.len(),
        bytes: tree.bytes(),
        units: units.len(),
        source: format!(
            "deploy pull of {env} from the D-16 source ({} files written)",
            pulled.files_written
        ),
        types: types.into_iter().collect(),
        tree,
    }
}

/// Nearest-rank percentile of `values` (sorted in place).
fn percentile(values: &mut [f64], p: f64) -> f64 {
    values.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
    let rank = ((p / 100.0) * values.len() as f64).ceil() as usize;
    values[rank.clamp(1, values.len()) - 1]
}

/// Measurements by phase; each is also printed as one JSON line.
struct Recorder {
    label: String,
    series: BTreeMap<String, Vec<f64>>,
}

impl Recorder {
    fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            series: BTreeMap::new(),
        }
    }
    fn record(&mut self, phase: &str, iter: usize, value: f64, extra: serde_json::Value) {
        println!(
            "OQ5 {}",
            serde_json::json!({
                "label": self.label, "phase": phase, "iter": iter,
                "value": (value * 1000.0).round() / 1000.0, "extra": extra,
            })
        );
        self.series
            .entry(phase.to_string())
            .or_default()
            .push(value);
    }
    fn summary(&mut self, payload: &Payload) -> serde_json::Value {
        let mut phases = serde_json::Map::new();
        for (phase, values) in &mut self.series {
            let n = values.len();
            let p50 = percentile(values, 50.0);
            let p95 = percentile(values, 95.0);
            phases.insert(
                phase.clone(),
                serde_json::json!({
                    "n": n, "p50": p50, "p95": p95,
                    "min": values[0], "max": values[n - 1],
                }),
            );
        }
        serde_json::json!({
            "label": self.label, "files": payload.files, "bytes": payload.bytes,
            "units": payload.units, "source": payload.source,
            "percentile": "nearest rank", "unit": "seconds unless the phase name ends in _ms",
            "phases": phases,
        })
    }
}

/// A config scan, triggered through the profile's `DeployApi` and waited on
/// until the gateway reports it inactive with an advanced timestamp. Returns
/// the wall time from the trigger and the gateway's `lastScanDuration` (ms).
async fn scan_timed(api: &dyn DeployApi) -> Res<(f64, Option<i64>)> {
    let before = api.scan_status(ScanKind::Config).await.map_err(err_s)?;
    if before.scan_active {
        return Err("a scan was already active before the trigger".to_string());
    }
    let started = Instant::now();
    api.scan_request(ScanKind::Config).await.map_err(err_s)?;
    loop {
        let now = api.scan_status(ScanKind::Config).await.map_err(err_s)?;
        if scan_complete(&before, &now) {
            return Ok((started.elapsed().as_secs_f64(), now.last_scan_duration));
        }
        if started.elapsed() > Duration::from_secs(600) {
            return Err("the scan did not complete in 600 s".to_string());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// `remove_tree` for the restore: a transient ssh failure (the host closing or
/// timing out a connection) is retried, because a restore that gives up leaves
/// the host changed.
async fn remove_tree_retry(shell: &RemoteShell, path: &str) -> Res<bool> {
    let mut last = String::new();
    for attempt in 1..=5u64 {
        match shell
            .exec_powershell(&script("remove_tree", REMOVE_TREE, path_params(path)))
            .await
        {
            Ok(value) => return Ok(value["gone"] == 1),
            Err(e) => {
                last = format!("{e:?}");
                tokio::time::sleep(Duration::from_secs(10 * attempt)).await;
            }
        }
    }
    Err(format!("remove_tree of {path} failed 5 times: {last}"))
}

/// One entry of the production collection: `(is_dir, size, sha256 of a file)`.
type Snapshot = BTreeMap<String, (bool, u64, String)>;

/// List the collection and hash every file by downloading it (the collection
/// is expected to hold a handful of files; more than 300 is refused).
async fn snapshot(
    shell: &RemoteShell,
    prod_sftp: &str,
    prod_native: &str,
    tmp: &Path,
) -> Res<Snapshot> {
    let entries = enumerate(shell, prod_native)
        .await
        .ok_or("the production collection is missing")?;
    if entries.len() > 300 {
        return Err(format!("production holds {} entries", entries.len()));
    }
    let mut ops = Vec::new();
    let mut local = Vec::new();
    for (i, (rel_path, is_dir, _)) in entries.iter().enumerate() {
        if !is_dir {
            let dest = tmp.join(format!("snap-{i}"));
            ops.push(SftpOp::Get {
                remote: format!("{prod_sftp}/{rel_path}"),
                local: dest.clone(),
            });
            local.push((rel_path.clone(), dest));
        }
    }
    if !ops.is_empty() {
        sftp(shell, &ops).await;
    }
    let mut hashes: BTreeMap<String, String> = BTreeMap::new();
    for (rel_path, dest) in local {
        hashes.insert(rel_path, sha256_hex(&std::fs::read(dest).map_err(err_s)?));
    }
    Ok(entries
        .into_iter()
        .map(|(rel_path, is_dir, size)| {
            let sha = hashes.get(&rel_path).cloned().unwrap_or_default();
            (rel_path, (is_dir, size, sha))
        })
        .collect())
}

struct Oq5 {
    repo: PathBuf,
    target_file: String,
    node: ignition_core::deploy::target::NodeTarget,
    session: Session,
    shell: RemoteShell,
    payload: Payload,
    delete_payload: DeleteSet,
    prod_native: String,
}

impl Oq5 {
    fn mirror(&self, staging: StagingMode) -> Res<Box<dyn transport::TreeMirror>> {
        transport::mirror_for_node(
            &self.repo,
            &self.target_file,
            "nodes.master",
            &self.node,
            MODE,
            staging,
        )
        .map_err(err_s)
    }

    async fn staging(&self, run_id: &str) -> Res<StagingMode> {
        transport::default_staging_mode(
            &self.repo,
            &self.target_file,
            "nodes.master",
            &self.node,
            GATEWAY,
            MODE,
            run_id,
        )
        .await
        .map_err(err_s)
    }

    /// Number of files below production, from a PowerShell walk.
    async fn production_files(&self) -> Res<usize> {
        let entries = enumerate(&self.shell, &self.prod_native)
            .await
            .ok_or("production is missing")?;
        Ok(entries.iter().filter(|(_, dir, _)| !dir).count())
    }

    /// Stage the payload and commit it, timing both.
    async fn write_payload(&self, run_id: &str) -> Res<(f64, f64)> {
        let mirror = self.mirror(self.staging(run_id).await?)?;
        let t = Instant::now();
        let staged = mirror.stage(&self.payload.tree).await.map_err(err_s)?;
        let stage_s = t.elapsed().as_secs_f64();
        let t = Instant::now();
        let report = mirror
            .commit(&staged, &DeleteSet::default())
            .await
            .map_err(err_s)?;
        let commit_s = t.elapsed().as_secs_f64();
        if report.written.len() != self.payload.files {
            return Err(format!(
                "commit wrote {} of {} files",
                report.written.len(),
                self.payload.files
            ));
        }
        Ok((stage_s, commit_s))
    }

    /// Remove `delete` through the delete set (an empty staging, then a commit
    /// that only deletes), timing the commit.
    async fn delete_files(&self, run_id: &str, delete: &DeleteSet) -> Res<(f64, usize)> {
        let mirror = self.mirror(self.staging(run_id).await?)?;
        let staged = mirror
            .stage(&RenderedTree::default())
            .await
            .map_err(err_s)?;
        let t = Instant::now();
        let report = mirror.commit(&staged, delete).await.map_err(err_s)?;
        Ok((t.elapsed().as_secs_f64(), report.removed_dirs.len()))
    }

    async fn scan(&self) -> Res<(f64, Option<i64>)> {
        scan_timed(&*self.session.api_handle()).await
    }
}

/// The timed part. Any error returns to the caller, which restores the host
/// before it reports.
async fn oq5_measure(
    ctx: &Oq5,
    rec: &mut Recorder,
    iters: usize,
    reads: usize,
    connects: usize,
) -> Res<()> {
    let label = rec.label.clone();
    for i in 1..=connects {
        let t = Instant::now();
        let out = ctx.shell.sftp_batch(&[SftpOp::Pwd]).await.map_err(err_s)?;
        ctx.shell.require_success("connect", out).map_err(err_s)?;
        rec.record(
            "connect_pwd",
            i,
            t.elapsed().as_secs_f64(),
            serde_json::json!({}),
        );
    }

    for i in 1..=iters {
        let run = format!("oq5-{label}-{i}");
        let (stage_s, commit_s) = ctx.write_payload(&run).await?;
        rec.record(
            "stage",
            i,
            stage_s,
            serde_json::json!({"files": ctx.payload.files}),
        );
        rec.record(
            "commit",
            i,
            commit_s,
            serde_json::json!({"files": ctx.payload.files}),
        );
        let present = ctx.production_files().await?;
        if present != ctx.payload.files + 1 {
            return Err(format!(
                "production holds {present} files after the commit, expected {}",
                ctx.payload.files + 1
            ));
        }
        let (scan_s, scan_ms) = ctx.scan().await?;
        rec.record("scan_with_payload", i, scan_s, serde_json::json!({}));
        if let Some(ms) = scan_ms {
            rec.record(
                "scan_with_payload_gateway_ms",
                i,
                ms as f64,
                serde_json::json!({}),
            );
        }
        let (delete_s, dirs) = ctx
            .delete_files(&format!("{run}-del"), &ctx.delete_payload)
            .await?;
        rec.record(
            "commit_delete",
            i,
            delete_s,
            serde_json::json!({"files": ctx.payload.files, "removed_dirs": dirs}),
        );
        let (scan_s, scan_ms) = ctx.scan().await?;
        rec.record("scan_after_delete", i, scan_s, serde_json::json!({}));
        if let Some(ms) = scan_ms {
            rec.record(
                "scan_after_delete_gateway_ms",
                i,
                ms as f64,
                serde_json::json!({}),
            );
        }
        let left = ctx.production_files().await?;
        if left != 1 {
            return Err(format!("production holds {left} files after the delete"));
        }
    }

    if reads == 0 {
        return Ok(());
    }
    // reads with the payload present
    ctx.write_payload(&format!("oq5-{label}-reads")).await?;
    let scope = Scope {
        mode: MODE.to_string(),
        types: ctx.payload.types.clone(),
        tag_providers: Vec::new(),
    };
    let reader = reader_of(&ctx.repo, "oq5");
    for i in 1..=reads {
        let t = Instant::now();
        let observed = reader.read(&scope).await.map_err(err_s)?;
        let secs = t.elapsed().as_secs_f64();
        let bytes: usize = observed
            .core
            .values()
            .chain(observed.mode.values())
            .map(|b| b.bytes.len())
            .sum();
        rec.record(
            "read",
            i,
            secs,
            serde_json::json!({
                "core_files": observed.core.len(), "mode_files": observed.mode.len(),
                "bytes": bytes,
            }),
        );
        if observed.mode.len() != ctx.payload.files {
            return Err(format!(
                "the read returned {} mode files, expected {}",
                observed.mode.len(),
                ctx.payload.files
            ));
        }
    }
    Ok(())
}

#[tokio::test]
#[ignore = "live: OQ5 timings; needs the Windows capture host, IGNITION_LIVE_WFLWVM01=1 and IGNITION_LIVE_OQ5=1"]
async fn oq5_timings() {
    let Some(live) = live() else { return };
    if std::env::var("IGNITION_LIVE_OQ5").as_deref() != Ok("1") {
        skip("IGNITION_LIVE_OQ5 is not 1");
        return;
    }
    let label = require_env("IGN_OQ5_LABEL");
    assert!(
        label.starts_with('A') || label.starts_with('B'),
        "IGN_OQ5_LABEL starts with A (exclusion off) or B (exclusion on)"
    );
    let num = |name: &str, default: usize| -> usize {
        std::env::var(name)
            .ok()
            .map(|v| {
                v.parse()
                    .unwrap_or_else(|_| panic!("{name} must be a number"))
            })
            .unwrap_or(default)
    };
    let scale = num("IGN_OQ5_SCALE", 10);
    let iters = num("IGN_OQ5_ITERS", 10);
    let reads = num("IGN_OQ5_READS", 20);
    let connects = num("IGN_OQ5_CONNECT_SAMPLES", 20);
    let op_timeout = num("IGN_OQ5_OP_TIMEOUT_S", 1800) as u32;
    let started = Instant::now();

    let payload = match (
        std::env::var("IGN_OQ5_SOURCE_REPO"),
        std::env::var("IGN_OQ5_SOURCE_ENV"),
    ) {
        (Ok(repo), Ok(env)) => pulled_payload(Path::new(&repo), &env).await,
        _ => corpus_payload(scale),
    };
    println!(
        "OQ5 {}",
        serde_json::json!({"label": label, "payload": {
            "source": payload.source, "files": payload.files, "bytes": payload.bytes,
            "units": payload.units, "types": payload.types.len(),
        }, "iters": iters, "reads": reads, "connect_samples": connects,
        "op_timeout_s": op_timeout})
    );

    let repo = repo(&live, "oq5", &live.data_dir, Some(op_timeout));
    let (target_file, _, node) = ssh_of(repo.path(), "oq5");
    let shell = shell_of(repo.path(), "oq5");
    let session = Session::resolve(Some(&require_env("IGN_WFLWVM01_PROFILE")))
        .expect("the gateway profile resolves");
    let home = home_sftp(&shell).await;
    let home_native = native(&home);
    let prod_native = format!("{}\\config\\resources\\{MODE}", live.data_dir);
    let prod_sftp = format!(
        "/{}/config/resources/{MODE}",
        live.data_dir.replace('\\', "/")
    );
    let staging_native = format!("{home_native}\\ign-deploy\\{GATEWAY}");
    let local = tempfile::tempdir().expect("local temp");

    // the gateway state the numbers were taken against
    let pre_scan = session
        .api()
        .scan_status(ScanKind::Config)
        .await
        .expect("scan status");
    println!(
        "OQ5 {}",
        serde_json::json!({"label": label, "gateway_scan_state_before": {
            "scanActive": pre_scan.scan_active,
            "lastScanTimestamp": pre_scan.last_scan_timestamp,
            "lastScanDuration": pre_scan.last_scan_duration,
        }})
    );
    assert!(!pre_scan.scan_active, "a scan is running; wait and rerun");

    // T-22-59: the listing and hashes the host must come back to
    // IGN_OQ5_RESTORE_ONLY=<sha256 of config-mode.json from the recorded pre-run
    // listing>: an earlier run was cut off; take the expected state from that
    // recorded hash instead of from the (dirty) host, measure nothing, restore.
    let restore_only = std::env::var("IGN_OQ5_RESTORE_ONLY").ok();
    let before = match &restore_only {
        None => snapshot(&shell, &prod_sftp, &prod_native, local.path())
            .await
            .expect("pre-run listing"),
        Some(sha) => BTreeMap::from([("config-mode.json".to_string(), (false, 133, sha.clone()))]),
    };
    println!(
        "OQ5 {}",
        serde_json::json!({"label": label, "pre_run_listing": before.iter()
            .map(|(p, (d, s, h))| serde_json::json!({"path": p, "dir": d, "size": s, "sha256": h}))
            .collect::<Vec<_>>()})
    );
    let prefilled = before.iter().filter(|(_, (d, _, _))| !d).count();
    assert_eq!(
        prefilled, 1,
        "production must hold only config-mode.json before the run"
    );
    let (iters, reads, connects) = if restore_only.is_some() {
        (0, 0, 0)
    } else {
        (iters, reads, connects)
    };

    let delete_payload = DeleteSet {
        files: payload.tree.files.keys().cloned().collect(),
    };
    let ctx = Oq5 {
        repo: repo.path().to_path_buf(),
        target_file,
        node,
        session,
        shell,
        payload,
        delete_payload,
        prod_native: prod_native.clone(),
    };

    let mut rec = Recorder::new(&label);
    let measured = oq5_measure(&ctx, &mut rec, iters, reads, connects).await;

    // restore, whatever happened above
    let restore = async {
        let entries = enumerate(&ctx.shell, &prod_native)
            .await
            .ok_or("production is missing")?;
        let leftover: Vec<RelPath> = entries
            .iter()
            .filter(|(p, dir, _)| !dir && !before.contains_key(p))
            .map(|(p, _, _)| RelPath::new(p).map_err(err_s))
            .collect::<Res<_>>()?;
        println!(
            "OQ5 {}",
            serde_json::json!({"label": label, "restore": {"files_to_remove": leftover.len()}})
        );
        if !leftover.is_empty() {
            let (secs, dirs) = ctx
                .delete_files(
                    &format!("oq5-{label}-restore"),
                    &DeleteSet { files: leftover },
                )
                .await?;
            println!(
                "OQ5 {}",
                serde_json::json!({"label": label, "restore": {"commit_secs": secs, "removed_dirs": dirs}})
            );
            let (scan_s, _) = ctx.scan().await?;
            println!(
                "OQ5 {}",
                serde_json::json!({"label": label, "restore": {"scan_secs": scan_s}})
            );
        }
        // empty directories the gateway or the commit left behind
        let entries = enumerate(&ctx.shell, &prod_native)
            .await
            .ok_or("production is missing")?;
        let stray_roots: BTreeSet<String> = entries
            .iter()
            .filter(|(p, _, _)| !before.contains_key(p))
            .filter_map(|(p, _, _)| p.split('/').next().map(str::to_string))
            .collect();
        for root in &stray_roots {
            println!(
                "OQ5 {}",
                serde_json::json!({"label": label, "restore": {"removing_stray_dir": root}})
            );
            remove_tree_retry(&ctx.shell, &format!("{prod_native}\\{root}")).await?;
        }
        let a = remove_tree_retry(&ctx.shell, &staging_native).await?;
        ctx.shell
            .exec_powershell(&script(
                "remove_if_empty",
                REMOVE_IF_EMPTY,
                path_params(&format!("{home_native}\\ign-deploy")),
            ))
            .await
            .map_err(err_s)?;
        println!(
            "OQ5 {}",
            serde_json::json!({"label": label, "restore": {"staging_gone": a}})
        );
        let after = snapshot(&ctx.shell, &prod_sftp, &prod_native, local.path()).await?;
        Ok::<_, String>(after)
    }
    .await;

    let summary = rec.summary(&ctx.payload);
    println!(
        "OQ5 {}",
        serde_json::json!({"label": label, "summary": summary})
    );
    println!(
        "OQ5 {}",
        serde_json::json!({"label": label, "total_secs": started.elapsed().as_secs_f64()})
    );

    let after = restore.expect("the host was not restored");
    let equal = after == before;
    println!(
        "OQ5 {}",
        serde_json::json!({"label": label, "restore_assertion": {
            "production_listing_equals_pre_run": equal,
            "entries_before": before.len(), "entries_after": after.len(),
        }})
    );
    let ign_home = enumerate(&ctx.shell, &format!("{home_native}\\ign-deploy")).await;
    println!(
        "OQ5 {}",
        serde_json::json!({"label": label, "ign_deploy_in_home_exists": ign_home.is_some()})
    );
    measured.expect("the measurement loop failed; the host was restored");
    assert!(
        equal,
        "the production collection differs from the pre-run listing"
    );
    assert!(
        ign_home.is_none(),
        "ign-deploy was left in the account home"
    );
    eprintln!("PASS oq5_timings");
}

// ---- offline checks of the OQ5 helpers --------------------------------------

#[test]
fn oq5_corpus_payload_scales_named_units_only() {
    let one = corpus_payload(1);
    let ten = corpus_payload(10);
    assert_eq!(one.files, 152);
    assert_eq!(one.units, 70);
    assert_eq!(ten.files, 1520);
    assert_eq!(ten.units, 700);
    assert!(
        !one.tree
            .files
            .keys()
            .any(|p| p.as_str() == "config-mode.json"),
        "the gateway-owned mode file is never part of the payload"
    );
    assert!(
        one.tree
            .files
            .keys()
            .all(|p| p.as_str().split('/').count() == 4 && p.as_str().contains("-oq5-01/")),
        "only named units, each under a distinct suffixed name"
    );
}

#[test]
fn oq5_copies_get_distinct_deterministic_uuids() {
    let body = b"{\"attributes\": {\"uuid\": \"adf419da-8154-4233-a042-9c6286a8e644\", \"x\": 1}}";
    let a = retag_uuids(body, 1);
    let b = retag_uuids(body, 2);
    assert_ne!(a, b);
    assert_eq!(a, retag_uuids(body, 1));
    assert_ne!(a.as_slice(), body.as_slice());
    assert_eq!(a.len(), body.len(), "a uuid is replaced by a uuid");
}

#[test]
fn oq5_percentile_is_nearest_rank() {
    let mut ten: Vec<f64> = (1..=10).rev().map(f64::from).collect();
    assert_eq!(percentile(&mut ten, 50.0), 5.0);
    assert_eq!(percentile(&mut ten, 95.0), 10.0);
    let mut twenty: Vec<f64> = (1..=20).map(f64::from).collect();
    assert_eq!(percentile(&mut twenty, 95.0), 19.0);
}

// ---- 23-08: the Windows script host (C-script-run, then commit and restore) ----

/// Run the probes of C-script-run and report them as `CAPTURE` lines: the
/// execution policy, `-File` against a script block under each policy, and the
/// three ways to put a staged file over a target (`Move-Item -Force`,
/// `File.Replace`, `File.Move`) with the target free and held with each share
/// mode. Fixed `const` bodies, as everywhere else in this file.
const PROBE_POLICY: &str = r#"$pol=(Get-ExecutionPolicy -List|ForEach-Object{$_.Scope.ToString()+'='+$_.ExecutionPolicy.ToString()}) -join ',';E @{ok=1;policy=$pol;ps=$PSVersionTable.PSVersion.ToString();mode=$ExecutionContext.SessionState.LanguageMode.ToString()}"#;

const PROBE_RUN: &str = r#"$f=$p.f;$r=@();function C($l,$c){$o=(cmd /c ($c+' 2>&1')) -join ' ';if($o.Length -gt 150){$o=$o.Substring(0,150)};$script:r+=,@($l,[int]$LASTEXITCODE,$o)};foreach($e in 'default','AllSigned','Restricted','Bypass'){$x='';if($e -ne 'default'){$x=' -ExecutionPolicy '+$e};C ('-File policy='+$e) ('powershell -NoProfile -NonInteractive'+$x+' -File "'+$f+'" x y')};$cmd='& ([ScriptBlock]::Create([IO.File]::ReadAllText(''\\?\'+$f+'''))) ''x'' ''y''';$b=[Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($cmd));foreach($e in 'default','AllSigned','Restricted'){$x='';if($e -ne 'default'){$x=' -ExecutionPolicy '+$e};C ('ScriptBlock policy='+$e) ('powershell -NoProfile -NonInteractive'+$x+' -EncodedCommand '+$b)};E @{ok=1;r=$r}"#;

const PROBE_MOVE: &str = r#"$r=$p.x+$p.root;$out=@();function Fresh{if([IO.Directory]::Exists($r)){Remove-Item -LiteralPath $r -Recurse -Force};[void][IO.Directory]::CreateDirectory($r+'\coll');[void][IO.Directory]::CreateDirectory($r+'\stage');[IO.File]::WriteAllText($r+'\coll\c.json','OLD-C');[IO.File]::WriteAllText($r+'\stage\c.json','NEW-C')};foreach($how in 'Move-Item -Force','File.Replace','File.Move'){foreach($share in 'free','None','Read','ReadWrite','Delete','ReadWrite,Delete'){Fresh;$h=$null;if($share -ne 'free'){$h=[IO.File]::Open($r+'\coll\c.json','Open','Read',[IO.FileShare]$share)};$res='ok';try{switch($how){'Move-Item -Force'{Move-Item -LiteralPath ($r+'\stage\c.json') -Destination ($r+'\coll\c.json') -Force}'File.Replace'{[IO.File]::Replace($r+'\stage\c.json',$r+'\coll\c.json',[NullString]::Value)}'File.Move'{[IO.File]::Move($r+'\stage\c.json',$r+'\coll\c.json')}}}catch{$res='ERR '+$_.Exception.Message.Trim()};if($h){$h.Dispose()};$out+=,@($how,$share,$res,[IO.File]::ReadAllText($r+'\coll\c.json'),[int][IO.File]::Exists($r+'\stage\c.json'))}};Fresh;$res='ok';try{Move-Item -LiteralPath ($r+'\coll\c.json') -Destination ($r+'\coll\c.json') -Force}catch{$res='ERR '+$_.Exception.Message.Trim()};$out+=,@('Move-Item -Force onto itself','-',$res,[IO.File]::ReadAllText($r+'\coll\c.json'),0);Remove-Item -LiteralPath $r -Recurse -Force;E @{ok=1;r=$out}"#;

/// Every file below `$p.path` (native, prefix added by `$p.x`) as relative
/// path to SHA-256, and every directory.
const LIST_HASHES: &str = r#"$b=$p.x+$p.path;$o=@{};$d=@();if([IO.Directory]::Exists($b)){$h=[Security.Cryptography.SHA256]::Create();foreach($f in [IO.Directory]::EnumerateFiles($b,'*',[IO.SearchOption]::AllDirectories)){$k=$f.Substring($b.Length+1).Replace('\','/');try{$s=New-Object IO.FileStream($f,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]7);$o[$k]=[BitConverter]::ToString($h.ComputeHash($s)).Replace('-','').ToLower();$s.Dispose()}catch{$o[$k]='?'}};foreach($f in [IO.Directory]::EnumerateDirectories($b,'*',[IO.SearchOption]::AllDirectories)){$d+=$f.Substring($b.Length+1).Replace('\','/')}};E @{ok=1;files=$o;dirs=$d}"#;

/// `(files: path to SHA-256, directories)` below `path` (native).
async fn hashes_below(shell: &RemoteShell, path: &str) -> (BTreeMap<String, String>, Vec<String>) {
    let value = shell
        .exec_powershell(&script("list_hashes", LIST_HASHES, path_params(path)))
        .await
        .expect("list_hashes");
    let files = value["files"]
        .as_object()
        .map(|m| {
            m.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                .collect()
        })
        .unwrap_or_default();
    let mut dirs: Vec<String> = match &value["dirs"] {
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        serde_json::Value::String(one) => vec![one.clone()],
        _ => Vec::new(),
    };
    dirs.sort();
    (files, dirs)
}

fn utc_now() -> String {
    let out = std::process::Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .expect("date");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn win_ledger(run: &str, files: &[&str]) -> Vec<u8> {
    Ledger {
        schema_version: 1,
        gateway: GATEWAY.to_string(),
        mode: MODE.to_string(),
        render_sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            .to_string(),
        run_id: run.to_string(),
        files: files.iter().map(|f| (*f).to_string()).collect(),
    }
    .to_bytes()
}

/// A batch for `run` made against the listing `live` (path to SHA-256) and the
/// ledger hash `ledger_pre`: the same shape the engine builds.
fn win_batch(
    run: &str,
    writes: &[(&str, &[u8])],
    deletes: &[&str],
    ledger: Option<&[u8]>,
    live: &BTreeMap<String, String>,
    ledger_pre: Option<String>,
) -> (BatchSpec, RenderedTree) {
    let mut tree = RenderedTree::default();
    let mut write_ops = Vec::new();
    for (path, bytes) in writes {
        let r = rel(path);
        tree.files.insert(r.clone(), bytes.to_vec());
        write_ops.push(WriteOp {
            path: r,
            sha256: sha256_hex(bytes),
            pre: live.get(*path).cloned(),
        });
    }
    write_ops.sort_by(|a, b| a.path.as_str().as_bytes().cmp(b.path.as_str().as_bytes()));
    let mut delete_ops: Vec<DeleteOp> = deletes
        .iter()
        .map(|path| DeleteOp {
            path: rel(path),
            pre: live
                .get(*path)
                .cloned()
                .expect("a delete names a live file"),
        })
        .collect();
    delete_ops.sort_by(|a, b| a.path.as_str().as_bytes().cmp(b.path.as_str().as_bytes()));
    let mut touched: Vec<TouchedEntry> = write_ops
        .iter()
        .map(|w| TouchedEntry {
            path: w.path.as_str().to_string(),
            pre_sha256: w.pre.clone(),
            post_sha256: Some(w.sha256.clone()),
        })
        .chain(delete_ops.iter().map(|d| TouchedEntry {
            path: d.path.as_str().to_string(),
            pre_sha256: Some(d.pre.clone()),
            post_sha256: None,
        }))
        .collect();
    if let Some(bytes) = ledger {
        touched.push(TouchedEntry {
            path: LEDGER_KEY.to_string(),
            pre_sha256: ledger_pre.clone(),
            post_sha256: Some(sha256_hex(bytes)),
        });
    }
    touched.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
    let pre_map: BTreeMap<String, Option<String>> = touched
        .iter()
        .map(|e| (e.path.clone(), e.pre_sha256.clone()))
        .collect();
    let pre_fingerprint = fingerprint(&pre_map);
    let journal = Journal {
        schema_version: 1,
        run_id: run.to_string(),
        gateway: GATEWAY.to_string(),
        mode: MODE.to_string(),
        phase: JournalPhase::Snapshotting,
        pre_fingerprint: pre_fingerprint.clone(),
        touched: touched.iter().map(|e| e.path.clone()).collect(),
    };
    let hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string();
    let record = RunRecord {
        schema_version: 1,
        run_id: run.to_string(),
        gateway: GATEWAY.to_string(),
        mode: MODE.to_string(),
        render_sha256: hash.clone(),
        plan_sha256: hash,
        state: RecordState::Applying,
        rolled_back_by: None,
        touched,
        pre_fingerprint,
        post_fingerprint: None,
    };
    (
        BatchSpec {
            run_id: RunId::parse(run).expect("run id"),
            gateway: GATEWAY.to_string(),
            mode: MODE.to_string(),
            writes: write_ops,
            deletes: delete_ops,
            ledger_new: ledger.map(<[u8]>::to_vec),
            ledger_pre,
            journal,
            record,
        },
        tree,
    )
}

/// The scratch gateway the script-host tests work in: `ign-live-scratch\data`
/// below the account home (never the gateway's own data directory, T-22-58),
/// with a `production` collection that holds a gateway-owned `config-mode.json`
/// and three database connections.
struct WinScratch {
    shell: RemoteShell,
    home: String,
    home_native: String,
    root_native: String,
    prod_native: String,
    staging_native: String,
    repo: tempfile::TempDir,
}

impl WinScratch {
    async fn new(live: &Live) -> WinScratch {
        let probe_repo = repo(live, "probe", &live.data_dir, None);
        let shell = shell_of(probe_repo.path(), "probe");
        let home = home_sftp(&shell).await;
        let home_native = native(&home);
        let data_native = scratch_native(&home_native);
        assert!(
            data_native.contains(SCRATCH)
                && !data_native.to_ascii_lowercase().contains("program files"),
            "{data_native}"
        );
        let root_native = format!("{home_native}\\{SCRATCH}");
        let staging_native = format!("{home_native}\\ign-deploy\\{GATEWAY}");
        let (a, b) = cleanup_scratch(&shell, &root_native, &staging_native, &home_native).await;
        assert!(a && b, "leftovers removed before the run");
        let res = format!("{home}/{SCRATCH}/data/config/resources");
        let prod = format!("{res}/production");
        let local = tempfile::tempdir().expect("local temp");
        let put = |name: &str, bytes: &str| -> PathBuf {
            let path = local.path().join(name);
            std::fs::write(&path, bytes).expect("local file");
            path
        };
        let mut ops = vec![
            SftpOp::Mkdir {
                path: format!("{home}/{SCRATCH}"),
            },
            SftpOp::Mkdir {
                path: format!("{home}/{SCRATCH}/data"),
            },
            SftpOp::Mkdir {
                path: format!("{home}/{SCRATCH}/data/config"),
            },
            SftpOp::Mkdir { path: res.clone() },
            SftpOp::Mkdir { path: prod.clone() },
            SftpOp::Put {
                local: put("mode.json", "{\"mode\":\"production\",\"scratch\":true}"),
                remote: format!("{prod}/config-mode.json"),
            },
            SftpOp::Mkdir {
                path: format!("{prod}/ignition"),
            },
            SftpOp::Mkdir {
                path: format!("{prod}/{DC}"),
            },
        ];
        for (dir, body) in [
            ("Existing", "{\"pre\":\"existing\"}"),
            ("ReplaceMe", "{\"pre\":\"old\"}"),
            ("Doomed", "{\"pre\":\"doomed\"}"),
        ] {
            ops.push(SftpOp::Mkdir {
                path: format!("{prod}/{DC}/{dir}"),
            });
            ops.push(SftpOp::Put {
                local: put(&format!("{dir}.json"), body),
                remote: format!("{prod}/{DC}/{dir}/config.json"),
            });
        }
        sftp(&shell, &ops).await;
        let prod_native = format!("{data_native}\\config\\resources\\production");
        WinScratch {
            repo: repo(live, "scratch", &data_native, None),
            shell,
            home,
            home_native,
            root_native,
            prod_native,
            staging_native,
        }
    }

    /// The Windows ApplyHost over the scratch data directory. The factory arm
    /// for Windows is 23-20's, so the host is built directly.
    fn host(&self) -> SftpTransport {
        let (_, ssh, _) = ssh_of(self.repo.path(), "scratch");
        let shell = shell_of(self.repo.path(), "scratch");
        let data = windows_sftp_root(&ssh.data_dir).expect("scratch data dir");
        SftpTransport::new_windows(shell, data, "nodes.master")
            .with_apply(GATEWAY, MODE)
            .expect("apply half")
    }

    async fn listing(&self) -> BTreeMap<String, String> {
        hashes_below(&self.shell, &self.prod_native).await.0
    }

    /// The deploy directory (`ign-deploy\live-scratch\production`) in native form.
    fn deploy_native(&self) -> String {
        format!("{}\\{MODE}", self.staging_native)
    }

    async fn cleanup(&self) {
        let (a, b) = cleanup_scratch(
            &self.shell,
            &self.root_native,
            &self.staging_native,
            &self.home_native,
        )
        .await;
        eprintln!("cleanup: scratch gone {a}, staging gone {b}");
        assert!(a && b);
    }
}

#[tokio::test]
#[ignore = "live: needs the Windows capture host and IGNITION_LIVE_WFLWVM01=1"]
async fn wflwvm01_apply_script_run() {
    let Some(live) = live() else { return };
    let started = Instant::now();
    let w = WinScratch::new(&live).await;
    let run = "20261017T120000Z-aaaaaa";

    // ---- C-script-run: how an uploaded script can run on this host
    eprintln!("CAPTURE c-script-run @ {}", utc_now());
    let policy = w
        .shell
        .exec_powershell(&script(
            "probe_policy",
            PROBE_POLICY,
            serde_json::Value::Null,
        ))
        .await
        .expect("policy");
    eprintln!(
        "CAPTURE policy: powershell {} language mode {} execution policy [{}]",
        policy["ps"], policy["mode"], policy["policy"]
    );
    let probe_dir = format!("{}\\probe", w.root_native);
    let probe_local = tempfile::tempdir().expect("probe dir");
    let probe_file = probe_local.path().join("probe.ps1");
    std::fs::write(
        &probe_file,
        "param($a,$b)\r\n[Console]::Out.WriteLine('IGN-RESULT:{\"ok\":1,\"a\":\"'+$a+'\",\"b\":\"'+$b+'\"}')\r\n",
    )
    .expect("probe script");
    sftp(
        &w.shell,
        &[SftpOp::Mkdir {
            path: format!("{}/{SCRATCH}/probe", w.home),
        }],
    )
    .await;
    sftp(
        &w.shell,
        &[SftpOp::Put {
            local: probe_file,
            remote: format!("{}/{SCRATCH}/probe/probe.ps1", w.home),
        }],
    )
    .await;
    let ran = w
        .shell
        .exec_powershell(&script(
            "probe_run",
            PROBE_RUN,
            serde_json::json!({ "f": format!("{probe_dir}\\probe.ps1") }),
        ))
        .await
        .expect("probe_run");
    for row in ran["r"].as_array().expect("rows") {
        eprintln!("CAPTURE run: {} -> exit {} : {}", row[0], row[1], row[2]);
    }
    let moved = w
        .shell
        .exec_powershell(&script(
            "probe_move",
            PROBE_MOVE,
            serde_json::json!({ "x": LONG_PATH_PREFIX, "root": format!("{}\\mv", w.root_native) }),
        ))
        .await
        .expect("probe_move");
    for row in moved["r"].as_array().expect("rows") {
        eprintln!(
            "CAPTURE move: {} target {} -> {} ; target now {} ; staged copy still there {}",
            row[0], row[1], row[2], row[3], row[4]
        );
    }

    // ---- the host, the stub and its budget
    let host = w.host();
    let roots = StubRoots {
        staging: format!("{}\\staging\\{run}", w.deploy_native()),
        collection: w.prod_native.clone(),
        deploy: w.deploy_native(),
    };
    let stub = script_stub(&script_ps1_sha256(), &"0".repeat(64), "commit", &roots);
    let command = ignition_core::deploy::transport::winps::encoded_command(&stub, COMMAND_BUDGET)
        .expect("the stub fits the budget");
    eprintln!(
        "CAPTURE encoded stub: {} characters of the {} budget (real roots; staging {} chars)",
        command.as_str().chars().count(),
        COMMAND_BUDGET,
        roots.staging.len()
    );

    // ---- one batch: a new file, a replaced file, a delete that empties a directory, a ledger
    let before = w.listing().await;
    assert_eq!(before.len(), 4, "{before:?}");
    let ledger = win_ledger(run, &[&format!("{DC}/Caf\u{e9} DB,1/config.json")]);
    let (spec, tree) = win_batch(
        run,
        &[
            (&format!("{DC}/Caf\u{e9} DB,1/config.json"), b"{\"new\":1}"),
            (&format!("{DC}/ReplaceMe/config.json"), b"{\"pre\":\"new\"}"),
        ],
        &[&format!("{DC}/Doomed/config.json")],
        Some(&ledger),
        &before,
        None,
    );

    // a tampered work list and a tampered script both stop at the stub, and
    // nothing runs (no journal)
    let t = Instant::now();
    let prepared = host.prepare(&spec, &tree).await.expect("prepare");
    timed("prepare (upload + verify)", t, ());
    let staging_sftp = format!("{}/ign-deploy/{GATEWAY}/{MODE}/staging/{run}", w.home);
    let local = tempfile::tempdir().expect("local");
    for (what, name) in [("work list", "work.tsv"), ("script", "commit.ps1")] {
        let garbage = local.path().join(name);
        std::fs::write(&garbage, "tampered\n").expect("garbage");
        sftp(
            &w.shell,
            &[SftpOp::Put {
                local: garbage,
                remote: format!("{staging_sftp}/{name}"),
            }],
        )
        .await;
        let err = host
            .commit(&prepared)
            .await
            .expect_err("a tampered file must stop the stub");
        eprintln!("CAPTURE tampered {what}: {err}");
        assert_eq!(err.code(), "transport_failed", "{err}");
        assert!(err.to_string().contains("does not match its hash"), "{err}");
        let (files, _) = hashes_below(&w.shell, &w.deploy_native()).await;
        assert!(
            !files.contains_key("journal.json"),
            "nothing ran, so there is no journal ({what})"
        );
        assert_eq!(w.listing().await, before, "the collection is untouched");
        // stage again, as a retry would
        let _ = host.prepare(&spec, &tree).await.expect("prepare again");
    }
    let prepared = host.prepare(&spec, &tree).await.expect("prepare again");

    // the commit: ONE exec_powershell call
    let t = Instant::now();
    let report = host.commit(&prepared).await.expect("commit");
    timed("commit (one script call)", t, ());
    eprintln!("CAPTURE commit report: {report:?}");
    assert_eq!(report.phase, JournalPhase::Written, "{report:?}");
    assert!(!report.stale && report.failure.is_none(), "{report:?}");
    assert_eq!(report.moved, vec![0, 1]);
    assert_eq!(report.deleted, vec![0]);
    assert_eq!(report.removed_dirs, vec![format!("{DC}/Doomed")]);
    let after = w.listing().await;
    assert_eq!(
        after.get(&format!("{DC}/Caf\u{e9} DB,1/config.json")),
        Some(&sha256_hex(b"{\"new\":1}"))
    );
    assert_eq!(
        after.get(&format!("{DC}/ReplaceMe/config.json")),
        Some(&sha256_hex(b"{\"pre\":\"new\"}"))
    );
    assert!(!after.contains_key(&format!("{DC}/Doomed/config.json")));
    let (deploy_files, _) = hashes_below(&w.shell, &w.deploy_native()).await;
    assert!(deploy_files.contains_key("journal.json"));
    assert!(deploy_files.contains_key("ledger.json"));
    assert!(deploy_files.contains_key(&format!("snapshots/{run}/record.json")));
    assert!(
        deploy_files.contains_key(&format!("snapshots/{run}/files/{DC}/ReplaceMe/config.json"))
    );
    assert!(deploy_files.contains_key(&format!("snapshots/{run}/files/{DC}/Doomed/config.json")));

    // a second commit over a leftover journal is refused and writes nothing
    let (spec2, tree2) = win_batch(
        "20261017T120001Z-bbbbbb",
        &[(&format!("{DC}/Extra/config.json"), b"{}")],
        &[],
        None,
        &after,
        None,
    );
    let prepared2 = host.prepare(&spec2, &tree2).await.expect("prepare 2");
    let err = host
        .commit(&prepared2)
        .await
        .expect_err("a journal of another run refuses");
    eprintln!("CAPTURE second commit over a journal: {err}");
    assert_eq!(err.code(), "deploy_journal_present", "{err}");
    assert_eq!(w.listing().await, after);

    // the restore: one script call, back to exactly the pre-run listing
    let t = Instant::now();
    let restored = host.restore(&spec.record).await.expect("restore");
    timed("restore (one script call)", t, ());
    eprintln!("CAPTURE restore report: {restored:?}");
    assert!(restored.failure.is_none(), "{restored:?}");
    assert_eq!(restored.phase, JournalPhase::Restoring);
    let restored_listing = w.listing().await;
    assert_eq!(
        restored_listing, before,
        "restore returns the pre-run listing"
    );
    let (deploy_files, _) = hashes_below(&w.shell, &w.deploy_native()).await;
    assert!(
        !deploy_files.contains_key("ledger.json"),
        "the first ledger is removed"
    );
    assert!(
        deploy_files.contains_key(&format!("snapshots/{run}/record.json")),
        "the snapshot stays"
    );
    // a second restore changes nothing
    let again = host.restore(&spec.record).await.expect("restore twice");
    assert!(again.failure.is_none(), "{again:?}");
    assert_eq!(w.listing().await, before);

    // ---- cleanup, with evidence
    w.cleanup().await;
    assert!(enumerate(&w.shell, &w.root_native).await.is_none());
    assert!(enumerate(&w.shell, &w.staging_native).await.is_none());
    timed("wflwvm01_apply_script_run total", started, ());
    eprintln!("PASS wflwvm01_apply_script_run");
}

// ---- 23-08 Task 2: C7b on the real collection --------------------------------

/// The gateway directory name the C7b runs write below `ign-deploy` (the batch
/// builder names `GATEWAY`).
const C7B_GATEWAY: &str = GATEWAY;

/// ACLs of the given plain native paths: `[path, owner, rules protected,
/// "identity:rights:type:I|E:inheritance | ..."]`.
const ACLS: &str = r#"$o=@();foreach($e in $p.paths){if(-not (Test-Path -LiteralPath $e)){$o+=,@($e,'ABSENT','','');continue};$i=Get-Item -LiteralPath $e -Force;$a=$i.GetAccessControl();$o+=,@($e,[string]$a.Owner,[string]$a.AreAccessRulesProtected,(($a.Access|ForEach-Object{$_.IdentityReference.ToString()+':'+$_.FileSystemRights.ToString()+':'+$_.AccessControlType.ToString()+':'+$(if($_.IsInherited){'I'}else{'E'})+':'+$_.InheritanceFlags.ToString()}) -join ' | '))};E @{ok=1;r=$o}"#;

const SERVICE: &str = r#"$q=(cmd /c 'sc qc Ignition 2>&1') -join ' ';$m=[regex]::Match($q,'SERVICE_START_NAME\s*:\s*(\S+)');E @{ok=1;start=$m.Groups[1].Value;raw=$q.Substring(0,[Math]::Min(160,$q.Length))}"#;

const DEFENDER: &str = r#"try{$s=Get-MpComputerStatus;$m=Get-MpPreference;E @{ok=1;rtp=[string]$s.RealTimeProtectionEnabled;paths=@($m.ExclusionPath).Count;procs=@($m.ExclusionProcess).Count;exts=@($m.ExclusionExtension).Count}}catch{$t=[string]$_.Exception.Message;E @{ok=1;err=$t.Substring(0,[Math]::Min(100,$t.Length))}}"#;

/// Hold `$p.path` open for `$p.s` seconds, sharing `$p.share` with other openers.
const HOLD: &str = r#"$h=[IO.File]::Open($p.path,'Open','Read',[IO.FileShare]$p.share);Start-Sleep -Seconds $p.s;$h.Dispose();E @{ok=1}"#;

/// Make `$p.dir` (prefix `$p.x`) and write `f.json` into it.
const MAKE_FILE: &str = r#"$d=$p.x+$p.dir;[void][IO.Directory]::CreateDirectory($d);[IO.File]::WriteAllText($d+'\f.json','{}');E @{ok=1}"#;

/// Remove one file (prefix `$p.x`).
const REMOVE_FILE: &str =
    r#"$f=$p.x+$p.path;if([IO.File]::Exists($f)){[IO.File]::Delete($f)};E @{ok=1}"#;

/// Remove one empty directory (prefix `$p.x`).
const REMOVE_EMPTY_DIR: &str =
    r#"$d=$p.x+$p.path;if([IO.Directory]::Exists($d)){[IO.Directory]::Delete($d)};E @{ok=1}"#;

async fn acl_rows(shell: &RemoteShell, paths: &[String]) -> Vec<(String, String, String, String)> {
    let value = shell
        .exec_powershell(&script("acls", ACLS, serde_json::json!({ "paths": paths })))
        .await
        .expect("acls");
    value["r"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| {
            (
                row[0].as_str().unwrap_or_default().to_string(),
                row[1].as_str().unwrap_or_default().to_string(),
                row[2].as_str().unwrap_or_default().to_string(),
                row[3].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect()
}

async fn print_acls(shell: &RemoteShell, label: &str, paths: &[String]) {
    eprintln!("CAPTURE acl {label} @ {}", utc_now());
    for (path, owner, protected, rules) in acl_rows(shell, paths).await {
        eprintln!(
            "CAPTURE acl   {path}\n                 owner={owner} protected={protected}\n                 {rules}"
        );
    }
}

/// The directory (native) below `base` whose `\\f.json` file has a native path
/// of exactly `total` characters.
fn dir_for_total(base: &str, total: usize) -> String {
    let want = total - "\\f.json".len();
    assert!(want > base.len() + 2, "{total} is too short for {base}");
    let mut dir = base.to_string();
    while dir.len() < want {
        let left = want - dir.len() - 1;
        let take = if left > 80 && left - 80 < 2 {
            left - 2
        } else {
            left.min(80)
        };
        dir.push('\\');
        dir.push_str(&"d".repeat(take));
    }
    assert_eq!(dir.len(), want);
    dir
}

/// What `finalize` does to the journal (23-20 builds it for this host): the
/// operator's equivalent between two runs.
async fn drop_journal(shell: &RemoteShell, deploy_native: &str) {
    shell
        .exec_powershell(&script(
            "remove_file",
            REMOVE_FILE,
            serde_json::json!({ "x": LONG_PATH_PREFIX, "path": format!("{deploy_native}\\{MODE}\\journal.json") }),
        ))
        .await
        .expect("drop the journal");
}

fn first_db_unit(payload: &Payload) -> Vec<(String, Vec<u8>)> {
    let unit = payload
        .tree
        .files
        .keys()
        .map(|p| p.as_str().split('/').take(3).collect::<Vec<_>>().join("/"))
        .find(|u| u.starts_with("ignition/database-connection/"))
        .expect("the corpus holds a database connection");
    payload
        .tree
        .files
        .iter()
        .filter(|(p, _)| p.as_str().starts_with(&format!("{unit}/")))
        .map(|(p, b)| (p.as_str().to_string(), b.clone()))
        .collect()
}

/// Put production back to `(files, dirs)`: delete the files that were not
/// there, then the directories that were not there (deepest first).
async fn put_back(
    shell: &RemoteShell,
    prod_native: &str,
    files: &BTreeMap<String, String>,
    dirs: &[String],
) {
    let (now_files, now_dirs) = hashes_below(shell, prod_native).await;
    for rel_path in now_files.keys().filter(|p| !files.contains_key(*p)) {
        shell
            .exec_powershell(&script(
                "remove_file",
                REMOVE_FILE,
                serde_json::json!({ "x": LONG_PATH_PREFIX, "path": format!("{prod_native}\\{}", rel_path.replace('/', "\\")) }),
            ))
            .await
            .expect("remove stray file");
    }
    let mut stray: Vec<&String> = now_dirs.iter().filter(|d| !dirs.contains(d)).collect();
    stray.sort_by_key(|d| std::cmp::Reverse(d.matches('/').count()));
    for dir in stray {
        shell
            .exec_powershell(&script(
                "remove_dir",
                REMOVE_EMPTY_DIR,
                serde_json::json!({ "x": LONG_PATH_PREFIX, "path": format!("{prod_native}\\{}", dir.replace('/', "\\")) }),
            ))
            .await
            .expect("remove stray directory");
    }
}

/// The gateway, reached through the installed `ign` binary and the host's
/// profile (`ign --profile <p> --json api call`). The profile's credential is in
/// the OS keyring and only the installed binary may read it without a prompt,
/// so this test never opens the keyring itself.
struct GwCli {
    profile: String,
}

impl GwCli {
    /// `data.result.data` of one raw call; a non-2xx status is an error.
    async fn call(
        &self,
        method: &str,
        path: &str,
        query: &[(&str, String)],
    ) -> Res<serde_json::Value> {
        let mut cmd = tokio::process::Command::new("ign");
        cmd.args([
            "--profile",
            &self.profile,
            "--json",
            "api",
            "call",
            "--method",
            method,
            "--path",
            path,
        ]);
        for (k, v) in query {
            cmd.args(["--query", &format!("{k}={v}")]);
        }
        let out = cmd.output().await.map_err(err_s)?;
        let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| {
            format!(
                "{method} {path}: no JSON ({e}); exit {:?}",
                out.status.code()
            )
        })?;
        let status = envelope["data"]["result"]["status"].as_i64().unwrap_or(0);
        if !(200..300).contains(&status) {
            return Err(format!("{method} {path}: status {status}"));
        }
        Ok(envelope["data"]["result"]["data"].clone())
    }

    async fn scan_status(&self) -> Res<(bool, i64, i64)> {
        let v = self.call("GET", "/data/api/v1/scan/config", &[]).await?;
        Ok((
            v["scanActive"].as_bool().unwrap_or(true),
            v["lastScanTimestamp"].as_i64().unwrap_or(0),
            v["lastScanDuration"].as_i64().unwrap_or(0),
        ))
    }

    /// A config scan, triggered and waited on: wall seconds and the gateway's
    /// own duration in milliseconds.
    async fn scan(&self) -> Res<(f64, i64)> {
        let (active, before, _) = self.scan_status().await?;
        if active {
            return Err("a scan was already active before the trigger".to_string());
        }
        let started = Instant::now();
        self.call("POST", "/data/api/v1/scan/config", &[]).await?;
        loop {
            let (active, ts, duration) = self.scan_status().await?;
            if !active && ts > before {
                return Ok((started.elapsed().as_secs_f64(), duration));
            }
            if started.elapsed() > Duration::from_secs(600) {
                return Err("the scan did not complete in 600 s".to_string());
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
}

/// Everything the C7b body works with.
struct C7b {
    shell: RemoteShell,
    /// The temp repo the shells are built from (a second ssh session holds a
    /// target open).
    repo_path: PathBuf,
    host: SftpTransport,
    gw: GwCli,
    home: String,
    home_native: String,
    prod_native: String,
    prod_sftp: String,
    deploy_native: String,
    lenroot_native: String,
    before_files: BTreeMap<String, String>,
}

#[tokio::test]
#[ignore = "live: needs the Windows capture host and IGNITION_LIVE_WFLWVM01=1; writes the real production collection and restores it"]
async fn wflwvm01_c7b() {
    let Some(live) = live() else { return };
    let started = Instant::now();
    let profile = require_env("IGN_WFLWVM01_PROFILE");
    let repo = repo(&live, "c7b", &live.data_dir, Some(150));
    let shell = shell_of(repo.path(), "c7b");
    let gw = GwCli { profile };
    let home = home_sftp(&shell).await;
    let home_native = native(&home);
    let prod_native = format!("{}\\config\\resources\\{MODE}", live.data_dir);
    let prod_sftp = format!(
        "/{}/config/resources/{MODE}",
        live.data_dir.replace('\\', "/")
    );
    let deploy_native = format!("{home_native}\\ign-deploy\\{C7B_GATEWAY}");
    let lenroot_native = format!("{home_native}\\ign-live-c7b");
    let (_, ssh, _) = ssh_of(repo.path(), "c7b");
    let host = SftpTransport::new_windows(
        shell_of(repo.path(), "c7b"),
        windows_sftp_root(&ssh.data_dir).expect("data dir"),
        "nodes.master",
    )
    .with_apply(C7B_GATEWAY, MODE)
    .expect("apply half");

    // leftovers of a cut-off run
    remove_tree(&shell, &deploy_native).await;
    remove_tree(&shell, &lenroot_native).await;

    eprintln!("CAPTURE c7b @ {}", utc_now());
    let service = shell
        .exec_powershell(&script("service", SERVICE, serde_json::Value::Null))
        .await
        .expect("service");
    eprintln!(
        "CAPTURE gateway service (sc qc Ignition): runs as {} ; {}",
        service["start"], service["raw"]
    );
    let defender = shell
        .exec_powershell(&script("defender", DEFENDER, serde_json::Value::Null))
        .await
        .expect("defender");
    eprintln!("CAPTURE defender as found: {defender}");

    let (before_files, before_dirs) = hashes_below(&shell, &prod_native).await;
    eprintln!(
        "CAPTURE production before: {} files, {} directories: {:?}",
        before_files.len(),
        before_dirs.len(),
        before_files.keys().collect::<Vec<_>>()
    );
    let scan_before = gw.scan().await.expect("a first scan");
    eprintln!("CAPTURE scan before anything is written: {scan_before:?}");

    let ctx = C7b {
        repo_path: repo.path().to_path_buf(),
        shell,
        host,
        gw,
        home,
        home_native,
        prod_native: prod_native.clone(),
        prod_sftp,
        deploy_native: deploy_native.clone(),
        lenroot_native: lenroot_native.clone(),
        before_files: before_files.clone(),
    };
    // the body runs as its own task so that a panic in it still leaves the
    // restore below to run
    let ctx = Arc::new(ctx);
    let outcome = match tokio::spawn({
        let ctx = ctx.clone();
        async move { c7b_body(&ctx).await }
    })
    .await
    {
        Ok(result) => result,
        Err(e) => Err(format!("the C7b body panicked: {e}")),
    };

    // whatever happened above: production goes back to its pre-run listing
    put_back(&ctx.shell, &prod_native, &before_files, &before_dirs).await;
    let (after_files, after_dirs) = hashes_below(&ctx.shell, &prod_native).await;
    let equal = after_files == before_files && after_dirs == before_dirs;
    eprintln!("CAPTURE production listing equals the pre-run listing: {equal}");
    let scan_after = ctx.gw.scan().await.expect("the closing scan");
    eprintln!("CAPTURE closing scan: {scan_after:?}");
    let g = remove_tree(&ctx.shell, &deploy_native).await;
    let l = remove_tree(&ctx.shell, &lenroot_native).await;
    ctx.shell
        .exec_powershell(&script(
            "remove_if_empty",
            REMOVE_IF_EMPTY,
            path_params(&format!("{}\\ign-deploy", ctx.home_native)),
        ))
        .await
        .expect("remove ign-deploy if empty");
    eprintln!("CAPTURE cleanup: deploy dir gone {g}, length probe gone {l}");
    outcome.expect("the C7b body failed; production was put back");
    assert!(
        equal,
        "the production collection differs from its pre-run listing"
    );
    assert!(g && l);
    timed("wflwvm01_c7b total", started, ());
    eprintln!("PASS wflwvm01_c7b");
}

async fn c7b_body(c: &C7b) -> Result<(), String> {
    let shell = &c.shell;
    let prod_native = c.prod_native.as_str();
    let corpus = corpus_payload(1);
    let unit = first_db_unit(&corpus);
    let unit_dir = unit[0]
        .0
        .rsplit_once('/')
        .map(|(d, _)| d.to_string())
        .expect("unit dir");
    let unit_name = unit_dir.rsplit('/').next().expect("name").to_string();
    eprintln!("CAPTURE the probe unit: {unit_dir} ({} files)", unit.len());
    let writes: Vec<(&str, &[u8])> = unit
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_slice()))
        .collect();

    // ---- 1. one new resource unit, moved from the account home into production
    let run1 = "20261017T130000Z-c7b001";
    let (spec1, tree1) = win_batch(run1, &writes, &[], None, &c.before_files, None);
    let prepared = c.host.prepare(&spec1, &tree1).await.map_err(err_s)?;
    let staged_root = format!("{}\\{MODE}\\staging\\{run1}\\files", c.deploy_native);
    let staged: Vec<String> = unit
        .iter()
        .map(|(p, _)| format!("{staged_root}\\{}", p.replace('/', "\\")))
        .collect();
    print_acls(
        shell,
        "the staged copies in the account home, before the move",
        &staged,
    )
    .await;
    let t = Instant::now();
    let report = c.host.commit(&prepared).await.map_err(err_s)?;
    timed("c7b: script commit of the probe unit", t, ());
    eprintln!("CAPTURE commit report: {report:?}");
    if report.failure.is_some() || report.moved.len() != unit.len() {
        return Err(format!("the probe unit was not committed: {report:?}"));
    }
    let unit_native = format!("{prod_native}\\{}", unit_dir.replace('/', "\\"));
    let type_native = unit_native
        .rsplit_once('\\')
        .map(|(d, _)| d.to_string())
        .expect("type dir");
    let moved_files: Vec<String> = unit
        .iter()
        .map(|(p, _)| format!("{prod_native}\\{}", p.replace('/', "\\")))
        .collect();
    let mut acl_paths = vec![
        prod_native.to_string(),
        format!("{prod_native}\\config-mode.json"),
        type_native.clone(),
        unit_native.clone(),
    ];
    acl_paths.extend(moved_files.iter().cloned());
    print_acls(shell, "after the script moved the unit in", &acl_paths).await;

    // a file made in place, for comparison: what inheritance gives a new file here
    let control = format!("{unit_native}\\control.txt");
    let control_dir = tempfile::tempdir().map_err(err_s)?;
    let control_local = control_dir.path().join("control.txt");
    std::fs::write(&control_local, b"control").map_err(err_s)?;
    sftp(
        shell,
        &[SftpOp::Put {
            local: control_local,
            remote: format!("{}/{unit_dir}/control.txt", c.prod_sftp),
        }],
    )
    .await;
    print_acls(
        shell,
        "a control file put by sftp into the same directory",
        std::slice::from_ref(&control),
    )
    .await;
    shell
        .exec_powershell(&script(
            "remove_file",
            REMOVE_FILE,
            serde_json::json!({ "x": LONG_PATH_PREFIX, "path": control }),
        ))
        .await
        .map_err(err_s)?;

    // ---- 2. does the gateway load it after a scan
    let (scan_s, scan_ms) = c.gw.scan().await?;
    let (_, scan_ts, scan_duration) = c.gw.scan_status().await?;
    let window_start = scan_ts - scan_duration - 1_000;
    let listed =
        c.gw.call(
            "GET",
            "/data/api/v1/resources/list/ignition/database-connection",
            &[],
        )
        .await?;
    let item = listed["items"]
        .as_array()
        .and_then(|items| items.iter().find(|i| i["name"] == unit_name.as_str()));
    eprintln!(
        "CAPTURE scan after the move: {scan_s:.2}s (gateway {scan_ms} ms); the REST view (resources/list) lists {unit_name}: {}",
        item.is_some()
    );
    if let Some(item) = item {
        eprintln!(
            "CAPTURE   {unit_name}: collection {} enabled {} version {} health {}",
            item["collection"], item["enabled"], item["version"], item["healthchecks"]
        );
    }
    let logs =
        c.gw.call(
            "GET",
            "/data/api/v1/logs",
            &[
                ("startTime", window_start.to_string()),
                ("minLevel", "WARN".to_string()),
                ("limit", "200".to_string()),
            ],
        )
        .await?;
    let lines = logs["items"].as_array().cloned().unwrap_or_default();
    let relevant: Vec<&serde_json::Value> = lines
        .iter()
        .filter(|e| {
            let m = e["message"].as_str().unwrap_or_default();
            m.contains(&unit_name) || m.contains("will not be loaded")
        })
        .collect();
    eprintln!(
        "CAPTURE gateway log since the scan: {} WARN+ lines, {} mention {unit_name} or `will not be loaded`",
        lines.len(),
        relevant.len()
    );
    for entry in relevant.iter().take(5) {
        let message: String = entry["message"]
            .as_str()
            .unwrap_or_default()
            .chars()
            .take(160)
            .collect();
        eprintln!(
            "CAPTURE   log {} {} {message}",
            entry["level"], entry["loggerName"]
        );
    }
    print_acls(shell, "after the gateway scanned the unit", &moved_files).await;
    // `finalize` is 23-20's; the operator's equivalent is dropping the journal
    drop_journal(shell, &c.deploy_native).await;

    // ---- 3. a held target. A reader that lets others read (FileShare.Read, what
    // a JVM or a scanner usually does) leaves the file readable and blocks the
    // replacement; a holder that shares nothing blocks the read as well.
    let held_rel = unit[0].0.clone();
    let held_native = format!("{prod_native}\\{}", held_rel.replace('/', "\\"));
    for (case, share, run, expect_refusal) in [
        ("share Read", "Read", "20261017T130100Z-c7b002", false),
        ("share None", "None", "20261017T130110Z-c7b003", true),
    ] {
        let (live_now, _) = hashes_below(shell, prod_native).await;
        let held_new = format!("{{\"replaced\":\"{case}\"}}").into_bytes();
        let (spec2, tree2) = win_batch(
            run,
            &[(held_rel.as_str(), held_new.as_slice())],
            &[],
            None,
            &live_now,
            None,
        );
        let prepared2 = c.host.prepare(&spec2, &tree2).await.map_err(err_s)?;
        let hold_secs = 60u64;
        let holder = shell_of(&c.repo_path, "c7b");
        let hold_path = held_native.clone();
        let hold = tokio::spawn(async move {
            holder
                .exec_powershell(&script(
                    "hold",
                    HOLD,
                    serde_json::json!({ "path": hold_path, "s": hold_secs, "share": share }),
                ))
                .await
        });
        // let the holder connect and open the file before the commit starts
        tokio::time::sleep(Duration::from_secs(12)).await;
        let t = Instant::now();
        let held_report = c.host.commit(&prepared2).await;
        let held_secs = t.elapsed().as_secs_f64();
        eprintln!(
            "CAPTURE commit with the target held open ({case}) @ {}: {held_secs:.1}s -> {held_report:?}",
            utc_now()
        );
        let after_held = hashes_below(shell, prod_native).await.0;
        eprintln!(
            "CAPTURE held target ({case}) after the commit: production listing unchanged {}",
            after_held.get(&held_rel).map(String::as_str)
                == live_now.get(&held_rel).map(String::as_str)
                || after_held.get(&held_rel).map(String::as_str) == Some("?")
        );
        if expect_refusal {
            let err = held_report.expect_err("a file that cannot be read stops the commit");
            if !err.to_string().contains("holds it open") {
                return Err(format!("the held refusal reads wrong: {err}"));
            }
            let (files, _) = hashes_below(shell, &c.deploy_native).await;
            if files.contains_key(&format!("{MODE}/journal.json")) {
                return Err("a refused commit left a journal".to_string());
            }
        } else {
            let held_report = held_report.map_err(err_s)?;
            if held_report.failure.is_none() {
                return Err(format!(
                    "the held target did not stop the commit ({held_secs:.1}s): {held_report:?}"
                ));
            }
            // the failed run is restored from its snapshot while the file is still held
            let restored2 = c.host.restore(&spec2.record).await.map_err(err_s)?;
            eprintln!("CAPTURE restore of the held run ({case}): {restored2:?}");
            if restored2.failure.is_some() {
                return Err(format!("the held run did not restore: {restored2:?}"));
            }
            drop_journal(shell, &c.deploy_native).await;
        }
        let _ = hold.await;
        let back = hashes_below(shell, prod_native).await.0;
        if back.get(&held_rel) != live_now.get(&held_rel) {
            return Err(format!(
                "the held file ({case}) is not back to its pre-run bytes"
            ));
        }
    }

    // restore run 1 too: production back to its listing, then a scan
    let restored1 = c.host.restore(&spec1.record).await.map_err(err_s)?;
    eprintln!("CAPTURE restore of the probe unit: {restored1:?}");
    if restored1.failure.is_some() {
        return Err(format!("the probe unit did not restore: {restored1:?}"));
    }
    drop_journal(shell, &c.deploy_native).await;

    // ---- 4. the 152-file corpus through the script (Defender as found)
    let run3 = "20261017T130200Z-c7b003";
    let corpus_writes: Vec<(String, Vec<u8>)> = corpus
        .tree
        .files
        .iter()
        .map(|(p, b)| (p.as_str().to_string(), b.clone()))
        .collect();
    let writes3: Vec<(&str, &[u8])> = corpus_writes
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_slice()))
        .collect();
    let (live3, _) = hashes_below(shell, prod_native).await;
    let (spec3, tree3) = win_batch(run3, &writes3, &[], None, &live3, None);
    let t = Instant::now();
    let prepared3 = c.host.prepare(&spec3, &tree3).await.map_err(err_s)?;
    let prepare_secs = t.elapsed().as_secs_f64();
    let t = Instant::now();
    let report3 = c.host.commit(&prepared3).await.map_err(err_s)?;
    let commit_secs = t.elapsed().as_secs_f64();
    eprintln!(
        "CAPTURE corpus: {} files ({} bytes); prepare {prepare_secs:.1}s; script commit {commit_secs:.2}s; moved {} failure {:?}",
        corpus.files,
        corpus.bytes,
        report3.moved.len(),
        report3.failure
    );
    let t = Instant::now();
    let restored3 = c.host.restore(&spec3.record).await.map_err(err_s)?;
    eprintln!(
        "CAPTURE corpus restore: {:.2}s (including its uploads and journal read); failure {:?}",
        t.elapsed().as_secs_f64(),
        restored3.failure
    );
    drop_journal(shell, &c.deploy_native).await;
    if report3.failure.is_some() || restored3.failure.is_some() {
        return Err("the corpus commit or its restore failed".to_string());
    }

    // ---- 5. the longest path sftp can still reach below the account home
    let mut rows = Vec::new();
    for total in [200usize, 230, 240, 250, 255, 258, 259, 260, 261, 270] {
        let dir = dir_for_total(&c.lenroot_native, total);
        shell
            .exec_powershell(&script(
                "make_file",
                MAKE_FILE,
                serde_json::json!({ "x": LONG_PATH_PREFIX, "dir": dir }),
            ))
            .await
            .map_err(err_s)?;
        let file_native = format!("{dir}\\f.json");
        assert_eq!(file_native.len(), total);
        let sftp_dir = format!("/{}", dir.replace('\\', "/"));
        let sftp_file = format!("/{}", file_native.replace('\\', "/"));
        let local_dir = tempfile::tempdir().map_err(err_s)?;
        let local_copy = local_dir.path().join("got.json");
        let listing = shell
            .sftp_batch(&[SftpOp::LsLong {
                path: sftp_dir.clone(),
            }])
            .await
            .map_err(err_s)?;
        let listed = String::from_utf8_lossy(&listing.stdout).contains("f.json");
        let got = shell
            .sftp_batch(&[SftpOp::Get {
                remote: sftp_file,
                local: local_copy.clone(),
            }])
            .await
            .map_err(err_s)?;
        let got_ok = got.code == 0 && local_copy.is_file();
        eprintln!(
            "CAPTURE path length {total}: ls lists f.json {listed} (exit {}); get ok {got_ok} (exit {})",
            listing.code, got.code
        );
        rows.push((total, listed, got_ok));
    }
    let longest = rows
        .iter()
        .filter(|(_, listed, got)| *listed && *got)
        .map(|(n, _, _)| *n)
        .max();
    eprintln!("CAPTURE longest reachable file path (ls and get both work): {longest:?}");
    let _ = (&c.home, &c.home_native);
    Ok(())
}
