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
use std::time::Instant;

use base64::Engine as _;
use ignition_core::actions::deploy::load_target;
use ignition_core::deploy::apply::fingerprint::{LEDGER_KEY, fingerprint};
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
