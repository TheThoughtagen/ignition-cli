//! Phase 23 FA-23-05 against a real Linux ssh host: the POSIX `ApplyHost`
//! (`commit.sh` over ssh and sftp) on a provisioned Azure VM, `ign-lnx-836`
//! (Ubuntu 22.04, Ignition 8.3.6, production mode, `dash` as `/bin/sh`), as
//! the key-only, non-root, no-sudo account `svc-ignition-deploy`.
//!
//! `commit.sh` was proven before only on macOS, through a local `sh` and the
//! local sftp-server. This file proves, on a real Linux gateway host:
//!
//! * the ssh path (the hash-checking stub through the account's bash login
//!   shell, home discovery by sftp `pwd`, the pinned host key);
//! * the file modes under a real session umask (0002 here);
//! * the WR-04 deadline stop over ssh;
//! * the shared conformance table under the host's dash, GNU coreutils and
//!   `/usr/lib/openssh/sftp-server`;
//! * the shipped `ign deploy apply`, `verify` and `rollback --run` against the
//!   real `production` collection.
//!
//! ```text
//! source <the 0600 env file that exports the IGN_LNXVM_* variables below>
//! export IGNITION_LIVE_LNXVM=1
//! cargo test -p ignition-core --test live_transport_lnxvm -- --ignored --nocapture --test-threads=1
//! ```
//!
//! | var | meaning |
//! |---|---|
//! | `IGNITION_LIVE_LNXVM` | must equal `1`; anything else is a quiet green skip |
//! | `IGN_LNXVM_HOST` | the host name or address |
//! | `IGN_LNXVM_USER` | the login (default `svc-ignition-deploy`) |
//! | `IGN_LNXVM_KEY_FILE` | a private key FILE path handed to `ssh -i`; never read or printed |
//! | `IGN_LNXVM_KNOWN_HOSTS` | the pinned known_hosts file (required) |
//! | `IGN_LNXVM_PROFILE` | the `ign` profile of the gateway (named in the target; the CLI gate calls the gateway with it) |
//! | `IGN_LNXVM_DATA_DIR` | the gateway data directory (default `/usr/local/bin/ignition/data`) |
//! | `IGN_LNXVM_CONFORMANCE_BIN` | a linux x86_64 build of the `deploy_host_conformance` test binary; unset = that gate skips |
//! | `IGNITION_LIVE_LNXVM_PROD` | must equal `1` for the gate that writes the real production collection |
//!
//! ## What is written, and where
//!
//! The scratch gates write only below the account home: `ign-live-scratch`
//! (the scratch gateway data directory), `ign-deploy/live-scratch` (the
//! separate-mode staging and ledger root) and, for the conformance gate,
//! `ign-live-conf`. Every removal is guarded by [`removable`]. The production
//! collection is written only by `lnxvm_cli_apply_rollback`, which needs the
//! extra opt-in `IGNITION_LIVE_LNXVM_PROD=1` and always puts production back.
//! Output carries counts, paths, modes and timings, never a key or a token.
//!
//! ## Building the conformance binary
//!
//! The conformance rows inspect results through the runner's own `std::fs`, so
//! they run ON the host as a cross-built binary (the 23-20 Windows precedent).
//! Build it from the committed tree in a throw-away amd64 container with its
//! own target directory (never the shared `CARGO_TARGET_DIR`):
//!
//! ```text
//! git archive --format=tar HEAD | tar -x -C "$B/src"
//! docker run --rm --platform linux/amd64 -v "$B/src:/src" -v "$B/out:/out" -w /src ubuntu:22.04 bash -c '
//!   apt-get update -qq && apt-get install -y -qq curl ca-certificates build-essential pkg-config cmake libdbus-1-dev
//!   curl --proto =https --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
//!   . $HOME/.cargo/env
//!   CARGO_TARGET_DIR=/out/target CARGO_PROFILE_DEV_DEBUG=0 cargo test -p ignition-core --test deploy_host_conformance --no-run --locked
//!   cp "$(ls -t /out/target/debug/deps/deploy_host_conformance-* | grep -v "\.d$" | head -1)" /out/deploy_host_conformance-linux-x86_64
//!   strip /out/deploy_host_conformance-linux-x86_64'
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine as _;
use ignition_core::actions::deploy::load_target;
use ignition_core::deploy::apply::fingerprint::{LEDGER_KEY, fingerprint};
use ignition_core::deploy::apply::hold::HOLD_S;
use ignition_core::deploy::apply::host::{ApplyHost, BatchSpec, DeleteOp, WriteOp};
use ignition_core::deploy::apply::journal::{Journal, JournalPhase};
use ignition_core::deploy::apply::record::{RecordState, RunRecord, TouchedEntry};
use ignition_core::deploy::apply::run_store::RunId;
use ignition_core::deploy::layout::RelPath;
use ignition_core::deploy::ledger::Ledger;
use ignition_core::deploy::target::{Ssh, TargetOs};
use ignition_core::deploy::transport::process::TokioProcessRunner;
use ignition_core::deploy::transport::remote::{RemoteCommand, RemoteShell, SftpOp};
use ignition_core::deploy::transport::sftp::{SftpTransport, posix_root};
use ignition_core::deploy::tree::{RenderedTree, sha256_hex};

const DEFAULT_DATA_DIR: &str = "/usr/local/bin/ignition/data";
/// The scratch tree below the account home.
const SCRATCH: &str = "ign-live-scratch";
/// The directory the conformance binary runs in, below the account home.
const CONF: &str = "ign-live-conf";
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
    key_file: PathBuf,
    known_hosts: PathBuf,
    profile: String,
    data_dir: String,
}

fn require_env(name: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| panic!("{name} must be set to run the live Linux transport gate"))
}

/// `None` is the quiet green skip.
fn live() -> Option<Live> {
    if std::env::var("IGNITION_LIVE_LNXVM").as_deref() != Ok("1") {
        skip("IGNITION_LIVE_LNXVM is not 1");
        return None;
    }
    let host = require_env("IGN_LNXVM_HOST");
    let key_file = PathBuf::from(require_env("IGN_LNXVM_KEY_FILE"));
    let known_hosts = PathBuf::from(require_env("IGN_LNXVM_KNOWN_HOSTS"));
    let profile = require_env("IGN_LNXVM_PROFILE");
    let user = std::env::var("IGN_LNXVM_USER").unwrap_or_else(|_| "svc-ignition-deploy".into());
    assert!(
        known_hosts.is_file(),
        "the pinned known_hosts file named by IGN_LNXVM_KNOWN_HOSTS must exist before anything touches the network"
    );
    assert!(
        key_file.is_absolute() && key_file.is_file(),
        "IGN_LNXVM_KEY_FILE must name an existing private key file by an absolute path"
    );
    let data_dir =
        std::env::var("IGN_LNXVM_DATA_DIR").unwrap_or_else(|_| DEFAULT_DATA_DIR.to_string());
    Some(Live {
        host,
        user,
        key_file,
        known_hosts,
        profile,
        data_dir,
    })
}

/// The target file of a Linux ssh master node with a key FILE.
fn target_yaml(live: &Live, data_dir: &str, op_timeout_s: Option<u32>) -> String {
    let timeout = op_timeout_s
        .map(|s| format!("      op_timeout_s: {s}\n"))
        .unwrap_or_default();
    format!(
        "schema_version: 1\ngateway: {GATEWAY}\nmode: {MODE}\nnodes:\n  master:\n    profile: {profile}\n    ssh:\n      host: {host}\n      user: {user}\n      key_file: '{key}'\n      os: linux\n      data_dir: '{data_dir}'\n      known_hosts: known_hosts\n{timeout}",
        profile = live.profile,
        host = live.host,
        user = live.user,
        key = live.key_file.display(),
    )
}

/// A temp repo with the pinned `known_hosts` and `targets/<env>.yaml`.
fn repo(live: &Live, env: &str, data_dir: &str, op_timeout_s: Option<u32>) -> tempfile::TempDir {
    let repo = tempfile::tempdir().expect("temp repo");
    std::fs::create_dir_all(repo.path().join("targets")).expect("targets dir");
    std::fs::create_dir_all(repo.path().join("gateways")).expect("gateways dir");
    std::fs::copy(&live.known_hosts, repo.path().join("known_hosts")).expect("copy known_hosts");
    std::fs::write(
        repo.path().join("targets").join(format!("{env}.yaml")),
        target_yaml(live, data_dir, op_timeout_s),
    )
    .expect("target file");
    repo
}

fn ssh_of(repo: &Path, env: &str) -> (String, Ssh) {
    let (target_file, target) = load_target(repo, env).expect("target");
    let ssh = target.nodes.master.ssh.clone().expect("ssh node");
    (target_file, ssh)
}

fn shell_of(repo: &Path, env: &str) -> RemoteShell {
    let (target_file, ssh) = ssh_of(repo, env);
    RemoteShell::from_ssh(
        repo,
        &target_file,
        "nodes.master",
        &ssh,
        Arc::new(TokioProcessRunner),
    )
    .expect("remote shell")
}

// ---- pure helpers, tested offline on every OS ------------------------------------

/// The scratch gateway data directory below `home`.
fn scratch_data(home: &str) -> String {
    format!("{home}/{SCRATCH}/data")
}

/// True when `path` is one of the three scratch roots below `home` or lies
/// below one of them, with no empty, `.` or `..` component and no trailing
/// slash (T-nr2-03). Every removal asserts it first.
fn removable(home: &str, path: &str) -> bool {
    let home = home.trim_end_matches('/');
    if home.is_empty() || !home.starts_with('/') || !path.starts_with('/') || path.ends_with('/') {
        return false;
    }
    if path
        .split('/')
        .skip(1)
        .any(|c| c.is_empty() || c == "." || c == "..")
    {
        return false;
    }
    [
        format!("{home}/{SCRATCH}"),
        format!("{home}/ign-deploy/{GATEWAY}"),
        format!("{home}/{CONF}"),
    ]
    .iter()
    .any(|root| {
        path == root
            || path
                .strip_prefix(root.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

/// `(rel path to sha256, directories)` from the text the LIST body prints:
/// `sha256sum` lines (`<hash>  ./<rel>`) and `D  <rel>` lines.
fn decode_listing(text: &str) -> (BTreeMap<String, String>, Vec<String>) {
    let mut files = BTreeMap::new();
    let mut dirs = Vec::new();
    for line in text.lines() {
        if let Some(dir) = line.strip_prefix("D  ") {
            dirs.push(dir.to_string());
        } else if line.len() > 66 && line.is_char_boundary(64) {
            let (hash, rest) = line.split_at(64);
            let path = rest.strip_prefix("  ").expect("two spaces after the hash");
            files.insert(
                path.strip_prefix("./").unwrap_or(path).to_string(),
                hash.to_string(),
            );
        }
    }
    dirs.sort();
    (files, dirs)
}

// ---- offline checks, run on every OS ----------------------------------------------

#[test]
fn lnxvm_target_yaml_loads_as_a_linux_ssh_node() {
    let live = Live {
        host: "203.0.113.9".to_string(),
        user: "svc-ignition-deploy".to_string(),
        key_file: PathBuf::from("/nonexistent/key"),
        known_hosts: PathBuf::from("/nonexistent"),
        profile: "azure-lnx".to_string(),
        data_dir: DEFAULT_DATA_DIR.to_string(),
    };
    let repo = tempfile::tempdir().expect("repo");
    std::fs::create_dir_all(repo.path().join("targets")).expect("targets");
    std::fs::write(
        repo.path().join("known_hosts"),
        "203.0.113.9 ssh-ed25519 AAAA\n",
    )
    .expect("known_hosts");
    std::fs::write(
        repo.path().join("targets/live.yaml"),
        target_yaml(&live, &live.data_dir, Some(7)),
    )
    .expect("target");
    let (_, target) = load_target(repo.path(), "live").expect("loads");
    let ssh = target.nodes.master.ssh.expect("ssh");
    assert_eq!(ssh.os, TargetOs::Linux);
    assert_eq!(ssh.key_file.as_deref(), Some("/nonexistent/key"));
    assert!(ssh.key_env.is_none());
    assert_eq!(ssh.op_timeout_s, Some(7));
    assert_eq!(
        posix_root(&ssh.data_dir).expect("posix root"),
        DEFAULT_DATA_DIR
    );
}

#[test]
fn the_scratch_data_dir_is_never_the_gateways() {
    let scratch = scratch_data("/home/svc-ignition-deploy");
    assert_eq!(scratch, "/home/svc-ignition-deploy/ign-live-scratch/data");
    assert!(!scratch.starts_with(DEFAULT_DATA_DIR));
    assert!(scratch.contains(SCRATCH));
}

#[test]
fn removable_allows_only_the_scratch_roots() {
    let home = "/home/svc-ignition-deploy";
    for ok in [
        "/home/svc-ignition-deploy/ign-live-scratch",
        "/home/svc-ignition-deploy/ign-live-scratch/data/config",
        "/home/svc-ignition-deploy/ign-deploy/live-scratch",
        "/home/svc-ignition-deploy/ign-deploy/live-scratch/production/staging",
        "/home/svc-ignition-deploy/ign-live-conf",
        "/home/svc-ignition-deploy/ign-live-conf/tmp",
    ] {
        assert!(removable(home, ok), "{ok}");
    }
    for bad in [
        "/usr/local/bin/ignition/data/config/resources/production",
        "/home/svc-ignition-deploy",
        "/home/svc-ignition-deploy/ign-deploy",
        "/home/svc-ignition-deploy/.ssh",
        "/",
        "",
        "ign-live-scratch",
        "/home/svc-ignition-deploy/ign-live-scratch/../.ssh",
        "/home/svc-ignition-deploy/ign-live-scratch/./x",
        "/home/svc-ignition-deploy/ign-live-scratch/",
        "/home/svc-ignition-deploy/ign-live-scratch//x",
        "/home/svc-ignition-deploy/ign-live-scratch-x",
        "/home/svc-ignition-deploy/ign-deploy/live-scratchy",
        "/home/svc-ignition-deploy/ign-live-conf-old",
        "/home/other/ign-live-scratch",
    ] {
        assert!(!removable(home, bad), "{bad}");
    }
    assert!(!removable("", "/ign-live-scratch"));
    assert!(!removable("relative", "relative/ign-live-scratch"));
}

#[test]
fn listing_decodes_files_and_dirs() {
    let a = "a".repeat(64);
    let b = "0123456789abcdef".repeat(4);
    let text = format!(
        "{a}  ./ignition/database-connection/Caf\u{e9} DB,1/config.json\nD  ignition\nD  ignition/database-connection\n{b}  ./config-mode.json\nD  ignition/database-connection/Caf\u{e9} DB,1\n"
    );
    let (files, dirs) = decode_listing(&text);
    assert_eq!(files.len(), 2);
    assert_eq!(
        files.get("ignition/database-connection/Caf\u{e9} DB,1/config.json"),
        Some(&a)
    );
    assert_eq!(files.get("config-mode.json"), Some(&b));
    assert_eq!(
        dirs,
        vec![
            "ignition".to_string(),
            "ignition/database-connection".to_string(),
            "ignition/database-connection/Caf\u{e9} DB,1".to_string(),
        ]
    );
    let (none, nothing) = decode_listing("");
    assert!(none.is_empty() && nothing.is_empty());
}

// ---- the posix helper layer -------------------------------------------------------
//
// Test-local sh bodies: fixed one-line `const`s run through the same typed
// `exec_posix` as the product's own. They read dynamic values only from the
// positional parameters ($1, $2, ...), exit 0 and print exactly one
// `IGN-RESULT:` JSON line. `RemoteCommand::posix` refuses control characters,
// so a body is ONE line.

/// `cd $1`, then every file as a `sha256sum` line and every directory as a
/// `D  <rel>` line, base64 as `b64`; `absent` when `$1` is not a directory.
const LIST: &str = r##"cd "$1" 2>/dev/null || { echo 'IGN-RESULT:{"ok":1,"absent":1}'; exit 0; }; b=$( { find . -type f -exec sha256sum -- {} + ; find . -mindepth 1 -type d | sed 's|^\./|D  |'; } | base64 -w0 ); echo "IGN-RESULT:{\"ok\":1,\"b64\":\"$b\"}""##;
/// `stat -c '%a %n'` of the paths `$2...` relative to `$1`, as base64 `b64`.
const MODES: &str = r##"cd "$1" || exit 3; shift; b=$(for p in "$@"; do stat -c '%a %n' -- "$p"; done | base64 -w0); echo "IGN-RESULT:{\"ok\":1,\"b64\":\"$b\"}""##;
/// `getfacl -p $1` as base64 `b64`.
const GETFACL: &str =
    r##"b=$(getfacl -p -- "$1" 2>&1 | base64 -w0); echo "IGN-RESULT:{\"ok\":1,\"b64\":\"$b\"}""##;
/// `chmod $1 $2`.
const CHMOD: &str = r##"chmod "$1" "$2" && echo 'IGN-RESULT:{"ok":1}'"##;
/// Remove the tree `$1`, first restoring owner access so a read-only
/// directory cannot block the removal; reports `gone`.
const REMOVE_TREE: &str = r##"chmod -R u+rwx -- "$1" 2>/dev/null; rm -rf -- "$1"; if [ -e "$1" ]; then echo 'IGN-RESULT:{"ok":1,"gone":0}'; else echo 'IGN-RESULT:{"ok":1,"gone":1}'; fi"##;
/// `rmdir $1`, quietly, when it is empty.
const RMDIR_IF_EMPTY: &str = r##"rmdir -- "$1" 2>/dev/null; echo 'IGN-RESULT:{"ok":1}'"##;
/// `rm -f $1`.
const REMOVE_FILE: &str = r##"rm -f -- "$1"; echo 'IGN-RESULT:{"ok":1}'"##;
/// The session umask and uid.
const UMASK: &str = r##"echo "IGN-RESULT:{\"ok\":1,\"umask\":\"$(umask)\",\"uid\":$(id -u)}""##;

/// Run `body` with `args` as `$1...` through `exec_posix`.
async fn sh_json(
    shell: &RemoteShell,
    op: &str,
    body: &str,
    args: &[&str],
) -> Result<serde_json::Value, ignition_core::error::CoreError> {
    let mut argv = vec!["sh", "-c", body, "sh"];
    argv.extend_from_slice(args);
    let command = RemoteCommand::posix(&argv).expect("a one-line body and clean arguments");
    shell.exec_posix(op, &command).await
}

fn b64_text(value: &serde_json::Value) -> String {
    let b64 = value["b64"].as_str().expect("a b64 field");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .expect("base64");
    String::from_utf8(bytes).expect("UTF-8 text")
}

/// `Remote working directory:` of the account, as sftp spells it (already the
/// native path on Linux).
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

async fn sftp(shell: &RemoteShell, ops: &[SftpOp]) -> String {
    let out = shell.sftp_batch(ops).await.expect("sftp spawn");
    let out = shell.require_success("live", out).expect("sftp batch");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn rel(text: &str) -> RelPath {
    RelPath::new(text).expect("rel path")
}

fn timed<T>(name: &str, started: Instant, value: T) -> T {
    eprintln!("TIMING {name}: {:.2}s", started.elapsed().as_secs_f64());
    value
}

fn utc_now() -> String {
    let out = std::process::Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .expect("date");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

type Listing = (BTreeMap<String, String>, Vec<String>);

/// Files (path to SHA-256) and directories below `path`; `None` when it is
/// not a directory.
async fn list_opt(shell: &RemoteShell, path: &str) -> Option<Listing> {
    let value = sh_json(shell, "list", LIST, &[path]).await.expect("list");
    if value["absent"] == 1 {
        return None;
    }
    Some(decode_listing(&b64_text(&value)))
}

/// [`list_opt`], empty when the directory is absent.
async fn list_below(shell: &RemoteShell, path: &str) -> Listing {
    list_opt(shell, path).await.unwrap_or_default()
}

/// The octal modes of `rels` below `base`.
async fn modes_of(shell: &RemoteShell, base: &str, rels: &[&str]) -> BTreeMap<String, String> {
    let mut args = vec![base];
    args.extend_from_slice(rels);
    let value = sh_json(shell, "modes", MODES, &args).await.expect("modes");
    b64_text(&value)
        .lines()
        .filter_map(|l| l.split_once(' '))
        .map(|(mode, name)| (name.to_string(), mode.to_string()))
        .collect()
}

/// Remove `path` through the guard; true when the host reports it gone.
async fn remove_tree(shell: &RemoteShell, home: &str, path: &str) -> bool {
    assert!(removable(home, path), "refusing to remove {path}");
    let value = sh_json(shell, "remove_tree", REMOVE_TREE, &[path])
        .await
        .expect("remove_tree");
    value["gone"] == 1
}

async fn remove_file(shell: &RemoteShell, home: &str, path: &str) {
    assert!(removable(home, path), "refusing to remove {path}");
    sh_json(shell, "remove_file", REMOVE_FILE, &[path])
        .await
        .expect("remove_file");
}

/// `rmdir` of `<home>/ign-deploy` when nothing else is in it.
async fn rmdir_deploy_root(shell: &RemoteShell, home: &str) {
    sh_json(
        shell,
        "rmdir",
        RMDIR_IF_EMPTY,
        &[&format!("{home}/ign-deploy")],
    )
    .await
    .expect("rmdir ign-deploy if empty");
}

async fn chmod(shell: &RemoteShell, home: &str, mode: &str, path: &str) {
    assert!(
        removable(home, path),
        "refusing to chmod outside the scratch: {path}"
    );
    sh_json(shell, "chmod", CHMOD, &[mode, path])
        .await
        .expect("chmod");
}

// ---- batch builders (ported from the Windows gate) --------------------------------

fn ledger_bytes(run: &str, files: &[&str]) -> Vec<u8> {
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
fn batch(
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
            commit_budget_s: None,
        },
        tree,
    )
}

// ---- the scratch gateway ----------------------------------------------------------

/// The scratch gateway the script-host gates work in: `ign-live-scratch/data`
/// below the account home (never the gateway's own data directory), with a
/// `production` collection that holds a `config-mode.json` and three database
/// connections.
struct LnxScratch {
    shell: RemoteShell,
    home: String,
    root: String,
    prod: String,
    staging: String,
    repo: tempfile::TempDir,
}

impl LnxScratch {
    async fn new(live: &Live) -> LnxScratch {
        let probe_repo = repo(live, "probe", &live.data_dir, None);
        let shell = shell_of(probe_repo.path(), "probe");
        let home = home_sftp(&shell).await;
        let data = scratch_data(&home);
        assert!(
            data.contains(SCRATCH)
                && !data.starts_with(DEFAULT_DATA_DIR)
                && !data.starts_with(live.data_dir.trim_end_matches('/')),
            "the scratch data dir is the gateway's"
        );
        let root = format!("{home}/{SCRATCH}");
        let staging = format!("{home}/ign-deploy/{GATEWAY}");
        let a = remove_tree(&shell, &home, &root).await;
        let b = remove_tree(&shell, &home, &staging).await;
        assert!(a && b, "leftovers removed before the run");
        let res = format!("{data}/config/resources");
        let prod = format!("{res}/{MODE}");
        let local = tempfile::tempdir().expect("local temp");
        let put = |name: &str, bytes: &str| -> PathBuf {
            let path = local.path().join(name);
            std::fs::write(&path, bytes).expect("local file");
            path
        };
        let mut ops = vec![
            SftpOp::Mkdir { path: root.clone() },
            SftpOp::Mkdir { path: data.clone() },
            SftpOp::Mkdir {
                path: format!("{data}/config"),
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
        LnxScratch {
            repo: repo(live, "scratch", &data, None),
            shell,
            home,
            root,
            prod,
            staging,
        }
    }

    /// The POSIX ApplyHost over the scratch data directory, built directly (the
    /// gates need the concrete type over a data directory that is not the
    /// gateway's). No `with_home`: the real server reports the home. The
    /// factory arm (`apply_host_for_node`) is proved by
    /// `lnxvm_cli_apply_rollback`, which goes through the installed binary.
    fn host(&self) -> SftpTransport {
        let (_, ssh) = ssh_of(self.repo.path(), "scratch");
        let shell = shell_of(self.repo.path(), "scratch");
        let data = posix_root(&ssh.data_dir).expect("scratch data dir");
        SftpTransport::new(shell, data, "nodes.master")
            .with_apply(GATEWAY, MODE)
            .expect("apply half")
    }

    async fn listing(&self) -> BTreeMap<String, String> {
        list_below(&self.shell, &self.prod).await.0
    }

    /// `<home>/ign-deploy/live-scratch/production`.
    fn deploy_dir(&self) -> String {
        format!("{}/{MODE}", self.staging)
    }

    /// `<deploy dir>/staging/<run>`.
    fn staging_run(&self, run: &str) -> String {
        format!("{}/staging/{run}", self.deploy_dir())
    }

    async fn cleanup(&self) {
        let a = remove_tree(&self.shell, &self.home, &self.root).await;
        let b = remove_tree(&self.shell, &self.home, &self.staging).await;
        rmdir_deploy_root(&self.shell, &self.home).await;
        eprintln!("cleanup: scratch gone {a}, staging gone {b}");
        assert!(a && b);
        assert!(list_opt(&self.shell, &self.root).await.is_none());
        assert!(list_opt(&self.shell, &self.staging).await.is_none());
    }
}

// ---- the gates --------------------------------------------------------------------

#[tokio::test]
#[ignore = "live: needs ign-lnx-836 and IGNITION_LIVE_LNXVM=1"]
async fn lnxvm_apply_script_run() {
    let Some(live) = live() else { return };
    let started = Instant::now();
    let w = LnxScratch::new(&live).await;
    let run = "20261018T120000Z-aaaaaa";

    // ---- the session: umask and uid of the deploy account
    eprintln!("CAPTURE lnxvm_apply_script_run @ {}", utc_now());
    let facts = sh_json(&w.shell, "umask", UMASK, &[]).await.expect("umask");
    eprintln!(
        "CAPTURE ssh session: umask {} uid {}",
        facts["umask"], facts["uid"]
    );
    assert_ne!(facts["uid"].as_i64(), Some(0), "the account is not root");

    // ---- the host and the batch
    let host = w.host();
    let before = w.listing().await;
    assert_eq!(before.len(), 4, "{before:?}");
    let new_file = format!("{DC}/Caf\u{e9} DB,1/config.json");
    let replaced = format!("{DC}/ReplaceMe/config.json");
    let doomed = format!("{DC}/Doomed/config.json");
    let new_dir = format!("{DC}/Caf\u{e9} DB,1");
    let ledger = ledger_bytes(run, &[&new_file]);
    let (spec, tree) = batch(
        run,
        &[
            (&new_file, b"{\"new\":1}"),
            (&replaced, b"{\"pre\":\"new\"}"),
        ],
        &[&doomed],
        Some(&ledger),
        &before,
        None,
    );

    // a tampered work list and a tampered script both stop at the stub, and
    // nothing runs (no journal)
    let t = Instant::now();
    let prepared = host.prepare(&spec, &tree).await.expect("prepare");
    timed("prepare (upload + verify)", t, ());
    let staging_run = w.staging_run(run);
    let local = tempfile::tempdir().expect("local");
    for (what, name) in [("work list", "work.tsv"), ("script", "commit.sh")] {
        let garbage = local.path().join(name);
        std::fs::write(&garbage, "tampered\n").expect("garbage");
        sftp(
            &w.shell,
            &[SftpOp::Put {
                local: garbage,
                remote: format!("{staging_run}/{name}"),
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
        let (files, _) = list_below(&w.shell, &w.deploy_dir()).await;
        assert!(
            !files.contains_key("journal.json"),
            "nothing ran, so there is no journal ({what})"
        );
        assert_eq!(w.listing().await, before, "the collection is untouched");
        // stage again, as a retry would
        let _ = host.prepare(&spec, &tree).await.expect("prepare again");
    }
    let prepared = host.prepare(&spec, &tree).await.expect("prepare again");

    // the staged copy is 0600; the moved file must still come out 0644
    chmod(
        &w.shell,
        &w.home,
        "600",
        &format!("{staging_run}/files/{new_file}"),
    )
    .await;

    // the commit: ONE exec_posix call
    let t = Instant::now();
    let report = host.commit(&prepared).await.expect("commit");
    timed("commit (one ssh call)", t, ());
    eprintln!("CAPTURE commit report: {report:?}");
    assert_eq!(report.phase, JournalPhase::Written, "{report:?}");
    assert!(!report.stale && report.failure.is_none(), "{report:?}");
    assert_eq!(report.moved, vec![0, 1]);
    assert_eq!(report.deleted, vec![0]);
    assert_eq!(report.removed_dirs, vec![format!("{DC}/Doomed")]);
    let after = w.listing().await;
    assert_eq!(after.get(&new_file), Some(&sha256_hex(b"{\"new\":1}")));
    assert_eq!(
        after.get(&replaced),
        Some(&sha256_hex(b"{\"pre\":\"new\"}"))
    );
    assert!(!after.contains_key(&doomed));
    let (deploy_files, _) = list_below(&w.shell, &w.deploy_dir()).await;
    assert!(deploy_files.contains_key("journal.json"));
    assert!(deploy_files.contains_key("ledger.json"));
    assert!(deploy_files.contains_key(&format!("snapshots/{run}/record.json")));
    assert!(deploy_files.contains_key(&format!("snapshots/{run}/files/{replaced}")));
    assert!(deploy_files.contains_key(&format!("snapshots/{run}/files/{doomed}")));

    // the modes: the script, not the session umask, decides
    let modes = modes_of(
        &w.shell,
        &w.prod,
        &[
            &new_file,
            &replaced,
            &new_dir,
            "ignition",
            DC,
            &format!("{DC}/Existing"),
        ],
    )
    .await;
    eprintln!("CAPTURE modes after the commit: {modes:?}");
    assert_eq!(
        modes.get(&new_file).map(String::as_str),
        Some("644"),
        "the moved new file (staged copy was 0600, session umask 0002)"
    );
    assert_eq!(
        modes.get(&replaced).map(String::as_str),
        Some("644"),
        "the moved replacement"
    );
    assert_eq!(
        modes.get(&new_dir).map(String::as_str),
        Some("755"),
        "the directory this commit created"
    );

    // a second batch over the journal is refused and writes nothing
    let (spec2, tree2) = batch(
        "20261018T120001Z-bbbbbb",
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

    // the restore: back to exactly the pre-run listing
    let t = Instant::now();
    let restored = host.restore(&spec.record).await.expect("restore");
    timed("restore (one ssh call)", t, ());
    eprintln!("CAPTURE restore report: {restored:?}");
    assert!(restored.failure.is_none(), "{restored:?}");
    assert_eq!(restored.phase, JournalPhase::Restoring);
    assert_eq!(
        w.listing().await,
        before,
        "restore returns the pre-run listing"
    );
    let (deploy_files, _) = list_below(&w.shell, &w.deploy_dir()).await;
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
    timed("lnxvm_apply_script_run total", started, ());
    eprintln!("PASS lnxvm_apply_script_run");
}

/// Delete `journal.json` of the scratch deploy directory (guarded).
async fn drop_journal(w: &LnxScratch) {
    remove_file(
        &w.shell,
        &w.home,
        &format!("{}/journal.json", w.deploy_dir()),
    )
    .await;
}

/// Review WR-04 on the real Linux host: `commit.sh` accepts the `B` (time
/// budget) record over ssh, a commit inside its budget reports no deadline,
/// and a commit with a one-second budget over a held (read-only) target
/// directory stops and says so (`deadline`, a value-free reason) instead of
/// retrying on. The run is then restored and the scratch tree is as it was.
#[tokio::test]
#[ignore = "live: needs ign-lnx-836 and IGNITION_LIVE_LNXVM=1"]
async fn lnxvm_sh_commit_time_budget() {
    let Some(live) = live() else { return };
    let started = Instant::now();
    let w = LnxScratch::new(&live).await;
    let host = w.host();
    let before = w.listing().await;

    // ---- inside the budget: the record parses and nothing is reported late
    let run = "20261018T180000Z-budget1";
    let fresh = format!("{DC}/BudgetFresh/config.json");
    let (mut spec, tree) = batch(run, &[(&fresh, b"{\"new\":1}")], &[], None, &before, None);
    spec.commit_budget_s = Some(174);
    let prepared = host.prepare(&spec, &tree).await.expect("prepare");
    let report = host.commit(&prepared).await.expect("commit");
    eprintln!("CAPTURE commit inside a 174 s budget: {report:?}");
    assert_eq!(report.phase, JournalPhase::Written, "{report:?}");
    assert!(report.failure.is_none() && !report.deadline, "{report:?}");
    let restored = host.restore(&spec.record).await.expect("restore");
    assert!(restored.failure.is_none(), "{restored:?}");
    assert_eq!(w.listing().await, before);
    drop_journal(&w).await;

    // ---- over the budget: a read-only target directory and a one-second budget
    let doomed = format!("{DC}/Doomed/config.json");
    let existing = format!("{DC}/Existing/config.json");
    let replace = format!("{DC}/ReplaceMe/config.json");
    let run2 = "20261018T180100Z-budget2";
    let (mut spec2, tree2) = batch(
        run2,
        &[
            (&doomed, b"{\"pre\":\"changed d\"}"),
            (&existing, b"{\"pre\":\"changed e\"}"),
            (&replace, b"{\"pre\":\"changed r\"}"),
        ],
        &[],
        None,
        &before,
        None,
    );
    spec2.commit_budget_s = Some(1);
    let prepared2 = host.prepare(&spec2, &tree2).await.expect("prepare 2");
    let locked = format!("{}/{DC}/Existing", w.prod);
    chmod(&w.shell, &w.home, "555", &locked).await;
    let t = Instant::now();
    let result = host.commit(&prepared2).await;
    let wall = t.elapsed();
    // unlock before anything else can fail
    chmod(&w.shell, &w.home, "755", &locked).await;
    timed("commit over a read-only directory with a 1 s budget", t, ());
    let report2 = result.expect("a held target is a report, not an error");
    eprintln!(
        "CAPTURE commit over a read-only directory with a 1 s budget ({:.2}s wall): {report2:?}",
        wall.as_secs_f64()
    );
    assert!(report2.deadline, "{report2:?}");
    assert_eq!(report2.phase, JournalPhase::Writing, "{report2:?}");
    assert!(report2.moved.len() <= 1, "{report2:?}");
    let failure = report2.failure.clone().expect("a failure");
    assert!(
        (failure.contains("write 0") || failure.contains("write 1"))
            && failure.contains("time budget"),
        "{failure}"
    );
    assert!(!failure.contains("config.json"), "no path: {failure}");
    assert_eq!(
        w.listing().await.get(&replace),
        before.get(&replace),
        "the third file is never reached"
    );
    let restored2 = host.restore(&spec2.record).await.expect("restore 2");
    assert!(restored2.failure.is_none(), "{restored2:?}");
    assert_eq!(
        w.listing().await,
        before,
        "restore returns the pre-run listing"
    );
    drop_journal(&w).await;

    w.cleanup().await;
    timed("lnxvm_sh_commit_time_budget total", started, ());
    eprintln!("PASS lnxvm_sh_commit_time_budget");
}

// ---- the shipped binary: apply, then rollback by run id --------------------------------

type Res<T> = Result<T, String>;

fn err_s(e: impl std::fmt::Debug) -> String {
    format!("{e:?}")
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

/// The tracer fixture's gateway directory: a `database-translator` and the
/// alarm settings, four files.
const TRACER_GATEWAY: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/deploy/repos/tracer/gateways/tracer-gw"
);

/// Epoch milliseconds of an `events.ndjson` timestamp, `YYYY-MM-DDTHH:MM:SS.mmmZ`.
fn event_ms(t: &str) -> i64 {
    let num = |range: std::ops::Range<usize>| -> i64 { t[range].parse().expect("a number") };
    let (y, m, d) = (num(0..4), num(5..7), num(8..10));
    let (hh, mm, ss) = (num(11..13), num(14..16), num(17..19));
    let millis = if t.len() > 20 && &t[19..20] == "." {
        num(20..23)
    } else {
        0
    };
    // Howard Hinnant's days-from-civil.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    ((days * 86_400 + hh * 3_600 + mm * 60 + ss) * 1_000) + millis
}

#[test]
fn event_ms_reads_the_run_store_timestamp() {
    assert_eq!(event_ms("1970-01-01T00:00:00.000Z"), 0);
    assert_eq!(event_ms("1970-01-02T00:00:01.250Z"), 86_401_250);
    assert_eq!(event_ms("2026-10-18T04:00:00.500Z"), 1_792_296_000_500);
}

/// `(t, event)` of every line of an `events.ndjson`.
fn events_of(run_dir: &Path) -> Vec<(i64, String)> {
    std::fs::read_to_string(run_dir.join("events.ndjson"))
        .expect("events.ndjson")
        .lines()
        .map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).expect("an event line");
            (
                event_ms(v["t"].as_str().expect("t")),
                v["event"].as_str().expect("event").to_string(),
            )
        })
        .collect()
}

/// Milliseconds from `lock_acquired` to `scan_done`: the scan request
/// releases the lock (D12), so this bounds the lock window from above.
fn lock_window_ms(events: &[(i64, String)]) -> i64 {
    let at = |name: &str| {
        events
            .iter()
            .find(|(_, e)| e == name)
            .unwrap_or_else(|| panic!("{name} in {events:?}"))
            .0
    };
    at("scan_done") - at("lock_acquired")
}

/// One call of the installed `ign` binary: its JSON envelope (stdout, or the
/// error envelope on stderr when stdout is empty), exit code and wall time.
/// The child is killed when `limit` passes.
async fn ign_call(
    profile: &str,
    cwd: &Path,
    args: &[&str],
    limit: Duration,
) -> Res<(i32, serde_json::Value, f64)> {
    let started = Instant::now();
    let mut cmd = tokio::process::Command::new("ign");
    cmd.args(["--profile", profile, "--json"])
        .args(args)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    let out = tokio::time::timeout(limit, cmd.output())
        .await
        .map_err(|_| format!("ign {} did not finish in {limit:?}", args.join(" ")))?
        .map_err(err_s)?;
    let wall = started.elapsed().as_secs_f64();
    let code = out.status.code().unwrap_or(-1);
    let raw = if out.stdout.iter().any(|b| !b.is_ascii_whitespace()) {
        &out.stdout
    } else {
        &out.stderr
    };
    let envelope = serde_json::from_slice(raw).map_err(|e| {
        format!(
            "ign {}: no JSON envelope ({e}); exit {code}; stderr {} bytes",
            args.join(" "),
            out.stderr.len()
        )
    })?;
    Ok((code, envelope, wall))
}

/// The run directory a verb reported, resolved against the repo.
fn run_dir_of(repo: &Path, data: &serde_json::Value) -> PathBuf {
    let dir = PathBuf::from(data["run_dir"].as_str().expect("data.run_dir"));
    if dir.is_absolute() {
        dir
    } else {
        repo.join(dir)
    }
}

/// Keep the directory entries below `<module>/<type>` only: the host ops
/// never remove `<module>/<type>` or above (the remove-emptied rule), so a
/// directory an apply created there stays after a rollback by design.
fn deep_dirs(dirs: &[String]) -> Vec<String> {
    dirs.iter()
        .filter(|d| d.split('/').count() >= 3)
        .cloned()
        .collect()
}

/// The gateway, reached through the installed `ign` binary and the host's
/// profile (`ign --profile <p> --json api call`). The token reaches the child
/// through the environment only; this test never opens the keyring.
struct GwCli {
    profile: String,
}

impl GwCli {
    /// `data.result.data` of one raw call; a non-2xx status is an error.
    async fn call(&self, method: &str, path: &str) -> Res<serde_json::Value> {
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
        ])
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
        let out = tokio::time::timeout(Duration::from_secs(120), cmd.output())
            .await
            .map_err(|_| format!("{method} {path}: did not finish in 120 s"))?
            .map_err(err_s)?;
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
        let v = self.call("GET", "/data/api/v1/scan/config").await?;
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
        self.call("POST", "/data/api/v1/scan/config").await?;
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

/// Put production back to `(files, dirs)`: delete the files that were not
/// there, then the directories that were not there (deepest first). Every
/// path is strictly below `prod`, has no `..` and was absent from the pre-run
/// listing (T-nr2-03).
async fn put_back(
    shell: &RemoteShell,
    prod: &str,
    files: &BTreeMap<String, String>,
    dirs: &[String],
) {
    let strictly_below = |rel: &str| {
        !rel.is_empty()
            && !rel.starts_with('/')
            && rel
                .split('/')
                .all(|c| !c.is_empty() && c != "." && c != "..")
    };
    let (now_files, now_dirs) = list_below(shell, prod).await;
    for rel_path in now_files.keys().filter(|p| !files.contains_key(*p)) {
        assert!(strictly_below(rel_path), "refusing to remove {rel_path}");
        sh_json(
            shell,
            "remove_file",
            REMOVE_FILE,
            &[&format!("{prod}/{rel_path}")],
        )
        .await
        .expect("remove stray file");
    }
    let mut stray: Vec<&String> = now_dirs.iter().filter(|d| !dirs.contains(d)).collect();
    stray.sort_by_key(|d| std::cmp::Reverse(d.matches('/').count()));
    for dir in stray {
        assert!(strictly_below(dir), "refusing to remove {dir}");
        sh_json(shell, "rmdir", RMDIR_IF_EMPTY, &[&format!("{prod}/{dir}")])
            .await
            .expect("remove stray directory");
    }
}

/// Everything the CLI body needs.
struct CliRun {
    profile: String,
    repo: PathBuf,
    before_files: BTreeMap<String, String>,
    before_dirs: Vec<String>,
    shell: RemoteShell,
    prod: String,
}

#[tokio::test]
#[ignore = "live: needs ign-lnx-836, IGNITION_LIVE_LNXVM=1, IGNITION_LIVE_LNXVM_PROD=1 and the installed ign; writes the real production collection and restores it"]
async fn lnxvm_cli_apply_rollback() {
    let Some(live) = live() else { return };
    if std::env::var("IGNITION_LIVE_LNXVM_PROD").as_deref() != Ok("1") {
        skip("IGNITION_LIVE_LNXVM_PROD is not 1");
        return;
    }
    let started = Instant::now();
    let repo = repo(&live, "cli", &live.data_dir, None);
    // The declared tree: the tracer's two units, a git repo that ignores `.ign/`.
    copy_dir(
        Path::new(TRACER_GATEWAY),
        &repo.path().join("gateways").join(GATEWAY),
    );
    std::fs::write(repo.path().join(".gitignore"), ".ign/\n").expect(".gitignore");
    let init = std::process::Command::new("git")
        .args(["init", "-q"])
        .arg(repo.path())
        .status()
        .expect("git init");
    assert!(init.success(), "git init");

    let shell = shell_of(repo.path(), "cli");
    let home = home_sftp(&shell).await;
    let prod = format!(
        "{}/config/resources/{MODE}",
        live.data_dir.trim_end_matches('/')
    );
    let deploy = format!("{home}/ign-deploy/{GATEWAY}");
    remove_tree(&shell, &home, &deploy).await;

    eprintln!("CAPTURE cli apply and rollback @ {}", utc_now());
    let (before_files, before_dirs) = list_below(&shell, &prod).await;
    eprintln!(
        "CAPTURE production before: {} files, {} directories: {:?}",
        before_files.len(),
        before_dirs.len(),
        before_files.keys().collect::<Vec<_>>()
    );
    let ctx = Arc::new(CliRun {
        profile: live.profile.clone(),
        repo: repo.path().to_path_buf(),
        before_files: before_files.clone(),
        before_dirs: before_dirs.clone(),
        shell: shell_of(repo.path(), "cli"),
        prod: prod.clone(),
    });
    // The body runs as its own task so a panic still leaves the restore below
    // to run.
    let outcome = match tokio::spawn({
        let ctx = ctx.clone();
        async move { cli_body(&ctx).await }
    })
    .await
    {
        Ok(result) => result,
        Err(e) => Err(format!("the CLI body panicked: {e}")),
    };

    // Whatever happened: production goes back to its pre-run listing, the
    // gateway is scanned, and the deploy directory below the home is removed.
    put_back(&shell, &prod, &before_files, &before_dirs).await;
    let (after_files, after_dirs) = list_below(&shell, &prod).await;
    let equal = after_files == before_files && after_dirs == before_dirs;
    eprintln!("CAPTURE production listing equals the pre-run listing: {equal}");
    let gw = GwCli {
        profile: ctx.profile.clone(),
    };
    let scan_after = gw.scan().await;
    eprintln!("CAPTURE closing scan: {scan_after:?}");
    let gone = remove_tree(&shell, &home, &deploy).await;
    rmdir_deploy_root(&shell, &home).await;
    eprintln!("CAPTURE cleanup: deploy dir gone {gone}");
    outcome.expect("the CLI body failed; production was put back");
    scan_after.expect("the closing scan");
    assert!(equal, "production differs from its pre-run listing");
    assert!(gone);
    timed("lnxvm_cli_apply_rollback total", started, ());
    eprintln!("PASS lnxvm_cli_apply_rollback");
}

async fn cli_body(c: &CliRun) -> Result<(), String> {
    let repo_arg = c.repo.to_string_lossy().into_owned();
    let short = Duration::from_secs(120);
    let long = Duration::from_secs(600);

    // plan
    let (code, plan, wall) = ign_call(
        &c.profile,
        &c.repo,
        &["deploy", "plan", "--env", "cli", "--repo", &repo_arg],
        short,
    )
    .await?;
    let hash = plan["data"]["plan_sha256"]
        .as_str()
        .ok_or_else(|| format!("plan: no plan_sha256 (exit {code}): {plan}"))?
        .to_string();
    eprintln!(
        "CAPTURE ign deploy plan: exit {code} in {wall:.1}s, summary {}, plan_sha256 {hash}",
        plan["data"]["summary"]
    );
    if code != 0 || plan["data"]["summary"]["has_changes"] != true {
        return Err(format!("plan: exit {code}, no changes: {plan}"));
    }

    // apply
    let (code, applied, apply_wall) = ign_call(
        &c.profile,
        &c.repo,
        &[
            "--yes",
            "deploy",
            "apply",
            "--env",
            "cli",
            "--repo",
            &repo_arg,
            "--plan-sha256",
            &hash,
        ],
        long,
    )
    .await?;
    let data = &applied["data"];
    eprintln!(
        "CAPTURE ign deploy apply: exit {code} in {apply_wall:.1}s, outcome {}, run {}, written {}, deleted {}, gwbk {}, warnings {}",
        data["outcome"],
        data["run_id"],
        data["written"].as_array().map_or(0, Vec::len),
        data["deleted"].as_array().map_or(0, Vec::len),
        data["gwbk"],
        data["warnings"]
    );
    if code != 0 || data["outcome"] != "applied" {
        return Err(format!("apply: exit {code}: {applied}"));
    }
    let run_id = data["run_id"]
        .as_str()
        .ok_or("apply: no run_id")?
        .to_string();
    let apply_dir = run_dir_of(&c.repo, data);
    let gwbk = apply_dir.join(data["gwbk"].as_str().ok_or("apply: no gwbk")?);
    let gwbk_bytes = std::fs::read(&gwbk).map_err(|e| format!("the .gwbk: {e}"))?;
    if !gwbk_bytes.starts_with(b"PK") {
        return Err("the .gwbk is not a ZIP".to_string());
    }
    if !apply_dir.join("snapshot").join("record.json").is_file() {
        return Err("the run directory has no snapshot/record.json".to_string());
    }
    let events = events_of(&apply_dir);
    let window_ms = lock_window_ms(&events);
    eprintln!(
        "CAPTURE apply run directory: .gwbk {} bytes, snapshot/record.json present, events {:?}",
        gwbk_bytes.len(),
        events.iter().map(|(_, e)| e.as_str()).collect::<Vec<_>>()
    );
    eprintln!(
        "CAPTURE apply lock window (lock_acquired to scan_done): {:.3}s against HOLD_S {HOLD_S}s",
        window_ms as f64 / 1000.0
    );
    if window_ms >= HOLD_S as i64 * 1000 {
        return Err(format!(
            "the apply lock window {window_ms} ms reached HOLD_S"
        ));
    }
    let (files_now, dirs_now) = list_below(&c.shell, &c.prod).await;
    if files_now == c.before_files {
        return Err("the apply left production unchanged".to_string());
    }
    eprintln!(
        "CAPTURE production after the apply: {} files (was {})",
        files_now.len(),
        c.before_files.len()
    );

    // the modes: every file the apply wrote is 0644 (a rename keeps the staged
    // inode's mode, and the script chmods it first); the directories it created
    // are recorded, not asserted, because production carries a default ACL
    let written: Vec<&str> = files_now
        .iter()
        .filter(|(p, h)| c.before_files.get(*p) != Some(*h))
        .map(|(p, _)| p.as_str())
        .collect();
    let file_modes = modes_of(&c.shell, &c.prod, &written).await;
    eprintln!(
        "CAPTURE modes of the {} files written: {file_modes:?}",
        written.len()
    );
    let wrong: Vec<&str> = written
        .iter()
        .copied()
        .filter(|p| file_modes.get(*p).map(String::as_str) != Some("644"))
        .collect();
    if !wrong.is_empty() || file_modes.len() != written.len() {
        return Err(format!("files written are not all 0644: {wrong:?}"));
    }
    let created: Vec<&str> = dirs_now
        .iter()
        .filter(|d| !c.before_dirs.contains(d))
        .map(String::as_str)
        .collect();
    let dir_modes = modes_of(&c.shell, &c.prod, &created).await;
    eprintln!(
        "CAPTURE modes of the {} directories created: {dir_modes:?}",
        created.len()
    );
    if let Some(first) = created.first() {
        let acl = sh_json(
            &c.shell,
            "getfacl",
            GETFACL,
            &[&format!("{}/{first}", c.prod)],
        )
        .await
        .map_err(err_s)?;
        eprintln!(
            "CAPTURE getfacl of {first}: {}",
            b64_text(&acl).replace('\n', " | ")
        );
    }

    // a fresh verify: the gateway loads what was deployed
    let (code, verified, wall) = ign_call(
        &c.profile,
        &c.repo,
        &["deploy", "verify", "--env", "cli", "--repo", &repo_arg],
        short,
    )
    .await?;
    let checks: Vec<String> = verified["data"]["checks"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|c| format!("{}={}", c["id"], c["status"]))
                .collect()
        })
        .unwrap_or_default();
    eprintln!(
        "CAPTURE ign deploy verify: exit {code} in {wall:.1}s, ok {}, resources_checked {}, checks {checks:?}",
        verified["data"]["ok"], verified["data"]["resources_checked"]
    );
    if code != 0 || verified["data"]["ok"] != true {
        return Err(format!("verify: exit {code}, checks {checks:?}"));
    }

    // rollback by run id
    let (code, rolled, roll_wall) = ign_call(
        &c.profile,
        &c.repo,
        &[
            "--yes", "deploy", "rollback", "--env", "cli", "--repo", &repo_arg, "--run", &run_id,
        ],
        long,
    )
    .await?;
    let data = &rolled["data"];
    eprintln!(
        "CAPTURE ign deploy rollback: exit {code} in {roll_wall:.1}s, outcome {}, run {}, restored {}, deleted {}, discarded {}",
        data["outcome"],
        data["run_id"],
        data["restored"].as_array().map_or(0, Vec::len),
        data["deleted"].as_array().map_or(0, Vec::len),
        data["discarded_live_changes"]
    );
    if code != 0 || data["outcome"] != "restored" {
        return Err(format!("rollback: exit {code}: {rolled}"));
    }
    let roll_dir = run_dir_of(&c.repo, data);
    let roll_events = events_of(&roll_dir);
    let roll_window_ms = lock_window_ms(&roll_events);
    eprintln!(
        "CAPTURE rollback lock window (lock_acquired to scan_done): {:.3}s against HOLD_S {HOLD_S}s; events {:?}",
        roll_window_ms as f64 / 1000.0,
        roll_events
            .iter()
            .map(|(_, e)| e.as_str())
            .collect::<Vec<_>>()
    );
    if roll_window_ms >= HOLD_S as i64 * 1000 {
        return Err(format!(
            "the rollback lock window {roll_window_ms} ms reached HOLD_S"
        ));
    }

    // production is back (files exactly; directories below <module>/<type>)
    let (files_back, dirs_back) = list_below(&c.shell, &c.prod).await;
    if files_back != c.before_files {
        return Err(format!(
            "production files differ after the rollback: {:?}",
            files_back.keys().collect::<Vec<_>>()
        ));
    }
    if !deep_dirs(&dirs_back).is_empty() {
        return Err(format!(
            "directories left below <module>/<type>: {:?}",
            deep_dirs(&dirs_back)
        ));
    }
    let (code, again, _) = ign_call(
        &c.profile,
        &c.repo,
        &["deploy", "plan", "--env", "cli", "--repo", &repo_arg],
        short,
    )
    .await?;
    let hash_again = again["data"]["plan_sha256"].as_str().unwrap_or_default();
    eprintln!("CAPTURE plan after the rollback: exit {code}, plan_sha256 {hash_again}");
    if hash_again != hash {
        return Err(format!(
            "the plan after the rollback differs: {hash_again} vs {hash}"
        ));
    }
    Ok(())
}
