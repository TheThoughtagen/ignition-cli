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

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ignition_core::actions::deploy::{
    PlanOptions, deploy_plan, deploy_pull, load_declared, load_target,
};
use ignition_core::deploy::layout::RelPath;
use ignition_core::deploy::target::Ssh;
use ignition_core::deploy::transport::process::TokioProcessRunner;
use ignition_core::deploy::transport::remote::{RemoteShell, SftpOp};
use ignition_core::deploy::transport::winps::{self, LONG_PATH_PREFIX, WinScript};
use ignition_core::deploy::transport::{self, DeleteSet, Scope, StagingMode, TreeReader};
use ignition_core::deploy::tree::{RenderedTree, sha256_hex};
use ignition_core::error::CoreError;

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
