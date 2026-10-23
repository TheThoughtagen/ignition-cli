//! Phase 25 DPRJ-04 against real ssh hosts: `project_host_for_node` builds the
//! ssh `ProjectHost`, and its `.git` probe and release-record read are run as
//! the key-only deploy account `svc-ignition-deploy` on the Phase 23 capture
//! VMs. Every test is `#[ignore]`d and a quiet green skip while its variables
//! are unset.
//!
//! ```text
//! source <the 0600 env file that exports the IGN_LNXVM_* variables below>
//! export IGNITION_LIVE_LNXVM=1
//! cargo test -p ignition-core --test live_project_host -- --ignored --nocapture --test-threads=1
//! ```
//!
//! | var | meaning |
//! |---|---|
//! | `IGNITION_LIVE_LNXVM` | must equal `1`; anything else is a quiet green skip of the Linux gate |
//! | `IGN_LNXVM_HOST` | the host name or address |
//! | `IGN_LNXVM_USER` | the login (default `svc-ignition-deploy`) |
//! | `IGN_LNXVM_KEY_FILE` | the deploy account's private key FILE path handed to `ssh -i`; never read or printed |
//! | `IGN_LNXVM_KNOWN_HOSTS` | the pinned known_hosts file (required) |
//! | `IGN_LNXVM_PROFILE` | the `ign` profile named in the target block |
//! | `IGN_LNXVM_DATA_DIR` | the gateway data directory (default `/usr/local/bin/ignition/data`) |
//! | `IGN_LNXVM_ADMIN_KEY` | the `ignadmin` key FILE used only for test setup and cleanup (default `~/.cache/ign-lnxvm01/ignadmin_ed25519`) |
//!
//! The Windows gate (`live_project_host_winvm`) uses the Phase 22/23 Windows
//! capture host `ign-win-836`:
//!
//! ```text
//! source <the 0600 env file that exports the IGN_WINVM_* variables below>
//! export IGNITION_LIVE_WINVM=1
//! cargo test -p ignition-core --test live_project_host live_project_host_winvm -- --ignored --nocapture --test-threads=1
//! ```
//!
//! | var | meaning |
//! |---|---|
//! | `IGNITION_LIVE_WINVM` | must equal `1`; anything else is a quiet green skip of the Windows gate |
//! | `IGN_WINVM_HOST` | the host name or address |
//! | `IGN_WINVM_USER` | the login (default `svc-ignition-deploy`) |
//! | `IGN_WINVM_KEY` | the deploy account's private key FILE path handed to `ssh -i`; never read or printed |
//! | `IGN_WINVM_KNOWN_HOSTS` | the pinned known_hosts file (required) |
//! | `IGN_WINVM_PROFILE` | the `ign` profile named in the target block |
//! | `IGN_WINVM_DATA_DIR` | the gateway data directory (default `C:\Program Files\Inductive Automation\Ignition\data`) |
//! | `IGN_WINVM_RG`, `IGN_WINVM_VM` | the Azure resource group and VM of the admin route (defaults `RG-IGN-CLI-WIN-TEST`, `ign-win-836`) |
//!
//! ## The release record write (25-13)
//!
//! `live_record_write_lnxvm` and `live_record_write_winvm` write a `CAP25-REC`
//! release record for the gateway `cap25-gw` through `release_writer_for_node`
//! as the deploy account, read it back through `ProjectHost::read_release_record`,
//! overwrite it once and read the new bytes. They touch exactly
//! `~/ign-deploy/cap25-gw` in the deploy account's home (and `~/ign-deploy`
//! itself when the test had to make it), and remove it, also when an
//! assertion fails. Setup and cleanup run as the deploy account over ssh.
//!
//! ## What is written, and where (the `.git` probe tests)
//!
//! Setup runs as `ignadmin` with sudo on Linux and as SYSTEM through
//! `az vm run-command` on Windows (the admin routes recorded in
//! `23-LIVE-CAPTURES.md`), never as the deploy account, and touches exactly
//! one path: `<data_dir>/projects/CAP25-GITPROBE`, which this test creates and
//! removes (also when an assertion fails). The deploy account only reads. The
//! output carries shapes, kinds and wall times, never a key.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use ignition_core::actions::deploy::load_target;
use ignition_core::deploy::projects::ReleaseRecord;
use ignition_core::deploy::transport::projects::{
    GitEntry, ProjectHost, project_host_for_node, release_writer_for_node,
};
use ignition_core::deploy::tree::sha256_hex;
use ignition_core::error::CoreError;

const DEFAULT_DATA_DIR: &str = "/usr/local/bin/ignition/data";
/// The only project name this file creates.
const PROBE: &str = "CAP25-GITPROBE";
/// The gateway name in the target; the record read is for this gateway.
const GATEWAY: &str = "live-project-host";
/// The record write tests: the gateway and project names they create.
const REC_GATEWAY: &str = "cap25-gw";
const REC_PROJECT: &str = "CAP25-REC";

fn skip(message: &str) {
    eprintln!("skipping: {message}");
}

fn require_env(name: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| panic!("{name} must be set to run the live project host gate"))
}

/// What the Linux gate needs once it is open.
struct Live {
    host: String,
    user: String,
    key_file: PathBuf,
    known_hosts: PathBuf,
    profile: String,
    data_dir: String,
    admin_key: PathBuf,
}

/// `None` is the quiet green skip.
fn live_linux() -> Option<Live> {
    if std::env::var("IGNITION_LIVE_LNXVM").as_deref() != Ok("1") {
        skip("IGNITION_LIVE_LNXVM is not 1");
        return None;
    }
    let host = require_env("IGN_LNXVM_HOST");
    let key_file = PathBuf::from(require_env("IGN_LNXVM_KEY_FILE"));
    let known_hosts = PathBuf::from(require_env("IGN_LNXVM_KNOWN_HOSTS"));
    let profile = require_env("IGN_LNXVM_PROFILE");
    let user = std::env::var("IGN_LNXVM_USER").unwrap_or_else(|_| "svc-ignition-deploy".into());
    let data_dir =
        std::env::var("IGN_LNXVM_DATA_DIR").unwrap_or_else(|_| DEFAULT_DATA_DIR.to_string());
    let admin_key = std::env::var("IGN_LNXVM_ADMIN_KEY")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(std::env::var("HOME").expect("HOME"))
                .join(".cache/ign-lnxvm01/ignadmin_ed25519")
        });
    assert!(
        known_hosts.is_file(),
        "the pinned known_hosts file named by IGN_LNXVM_KNOWN_HOSTS must exist before anything touches the network"
    );
    assert!(
        key_file.is_absolute() && key_file.is_file(),
        "IGN_LNXVM_KEY_FILE must name an existing private key file by an absolute path"
    );
    assert!(
        admin_key.is_file(),
        "the admin key file (IGN_LNXVM_ADMIN_KEY) must exist"
    );
    Some(Live {
        host,
        user,
        key_file,
        known_hosts,
        profile,
        data_dir,
        admin_key,
    })
}

/// A path that is safe to put inside single quotes in the setup script.
fn plain_path(path: &str) -> bool {
    !path.is_empty()
        && path.starts_with('/')
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-'))
}

/// The target file of a Linux ssh master node with a key FILE.
fn target_yaml(live: &Live) -> String {
    format!(
        "schema_version: 1\ngateway: {GATEWAY}\nmode: production\nnodes:\n  master:\n    profile: {profile}\n    ssh:\n      host: {host}\n      user: {user}\n      key_file: '{key}'\n      os: linux\n      data_dir: '{data_dir}'\n      known_hosts: known_hosts\n",
        profile = live.profile,
        host = live.host,
        user = live.user,
        key = live.key_file.display(),
        data_dir = live.data_dir,
    )
}

/// A temp targets root with the pinned `known_hosts` and `targets/<env>.yaml`.
fn targets_root(yaml: &str, known_hosts: &Path) -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("temp targets root");
    std::fs::create_dir_all(root.path().join("targets")).expect("targets dir");
    std::fs::copy(known_hosts, root.path().join("known_hosts")).expect("copy known_hosts");
    std::fs::write(root.path().join("targets/live.yaml"), yaml).expect("target file");
    root
}

/// Runs `script` as root on the Linux host through `ignadmin` with sudo.
fn admin_sh(live: &Live, script: &str) -> String {
    let out = Command::new("ssh")
        .arg("-i")
        .arg(&live.admin_key)
        .arg("-o")
        .arg(format!("UserKnownHostsFile={}", live.known_hosts.display()))
        .args([
            "-o",
            "StrictHostKeyChecking=yes",
            "-o",
            "IdentitiesOnly=yes",
            "-o",
            "BatchMode=yes",
            "-o",
            "ForwardAgent=no",
            "-o",
            "ConnectTimeout=30",
        ])
        .arg(format!("ignadmin@{}", live.host))
        .arg(format!("sudo -n sh -c '{script}'"))
        .output()
        .expect("spawn ssh");
    assert!(
        out.status.success(),
        "admin setup failed (exit {:?}): {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
            .lines()
            .last()
            .unwrap_or("")
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Removes the probe project when dropped, so a failed assertion still cleans
/// up. The path was checked by [`plain_path`] and ends in the probe name.
struct Cleanup<'a> {
    live: &'a Live,
    dir: String,
}

impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        assert!(plain_path(&self.dir) && self.dir.ends_with(&format!("/projects/{PROBE}")));
        let script = format!(
            "rm -rf {dir}; ls -1 {parent}",
            dir = self.dir,
            parent = { self.dir.rsplit_once('/').expect("parent").0 }
        );
        let listing = admin_sh_quiet(self.live, &script);
        eprintln!(
            "cleanup: {PROBE} entries left in data/projects: {}",
            listing.lines().filter(|l| l.starts_with("CAP25-")).count()
        );
    }
}

/// [`admin_sh`] that never panics (cleanup must not mask the real failure).
fn admin_sh_quiet(live: &Live, script: &str) -> String {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| admin_sh(live, script)))
        .unwrap_or_default()
}

fn kind_name(result: &Result<GitEntry, CoreError>) -> String {
    match result {
        Ok(kind) => kind.as_str().to_string(),
        Err(CoreError::TransportFailed { .. }) => "transport_failed".to_string(),
        Err(other) => format!("unexpected error {other:?}"),
    }
}

async fn timed(host: &dyn ProjectHost, shape: &str) -> (Result<GitEntry, CoreError>, u128) {
    let started = Instant::now();
    let result = host.git_entry(PROBE).await;
    let ms = started.elapsed().as_millis();
    eprintln!("probe {shape}: {} ({ms} ms)", kind_name(&result));
    (result, ms)
}

/// The four shapes of the plan on the Linux capture host, plus the denied
/// case the K9 decision is about, all through `project_host_for_node`.
#[tokio::test]
#[ignore = "needs the Linux capture VM and IGNITION_LIVE_LNXVM=1"]
async fn live_project_host_lnxvm() {
    let Some(live) = live_linux() else { return };
    let dir = format!("{}/projects/{PROBE}", live.data_dir.trim_end_matches('/'));
    assert!(
        plain_path(&dir),
        "the data dir must be plain path text for the setup script"
    );
    let root = targets_root(&target_yaml(&live), &live.known_hosts);
    let (target_file, target) = load_target(root.path(), "live").expect("target");
    let node = target.nodes.master;
    let host =
        project_host_for_node(root.path(), &target_file, "nodes.master", &node).expect("host");

    let _cleanup = Cleanup {
        live: &live,
        dir: dir.clone(),
    };
    // start clean: nothing named CAP25-GITPROBE
    admin_sh(&live, &format!("rm -rf {dir}"));

    // absent: no project directory at all
    let (result, _) = timed(host.as_ref(), "absent (no project directory)").await;
    assert_eq!(result.expect("absent"), GitEntry::Absent);
    assert_eq!(
        host.read_release_record(GATEWAY, PROBE)
            .await
            .expect("record read"),
        None,
        "a missing release record is None"
    );

    // absent: a project directory without .git
    admin_sh(&live, &format!("mkdir -p {dir} && chmod 755 {dir}"));
    let (result, _) = timed(host.as_ref(), "absent (project directory, no .git)").await;
    assert_eq!(result.expect("absent"), GitEntry::Absent);

    // directory
    admin_sh(
        &live,
        &format!("mkdir {dir}/.git && echo ref: refs/heads/main > {dir}/.git/HEAD"),
    );
    let (result, _) = timed(host.as_ref(), "directory").await;
    assert_eq!(result.expect("directory"), GitEntry::Directory);

    // file
    admin_sh(
        &live,
        &format!("rm -rf {dir}/.git && echo gitdir: ../elsewhere > {dir}/.git"),
    );
    let (result, _) = timed(host.as_ref(), "file").await;
    assert_eq!(result.expect("file"), GitEntry::File);

    // symlink (dangling: stat is not asked to follow it)
    admin_sh(
        &live,
        &format!("rm -rf {dir}/.git && ln -s {dir}/nowhere {dir}/.git"),
    );
    let (result, _) = timed(host.as_ref(), "symlink (dangling)").await;
    assert_eq!(result.expect("symlink"), GitEntry::Symlink);

    // denied: the project directory is not traversable by the deploy account,
    // so the probe cannot know; it must say so, never "absent"
    admin_sh(
        &live,
        &format!(
            "rm -rf {dir}/.git && mkdir {dir}/.git && chmod 700 {dir} && chmod 700 {dir}/.git"
        ),
    );
    let (result, _) = timed(host.as_ref(), "denied (project directory mode 700)").await;
    let err = result.expect_err("denied is a failure, not absent");
    assert!(
        matches!(&err, CoreError::TransportFailed { detail, .. } if detail.contains("Permission denied")),
        "{err:?}"
    );
    eprintln!("denied detail: {err}");

    // put back, remove: absent again
    admin_sh(&live, &format!("chmod 755 {dir} && rm -rf {dir}"));
    let (result, _) = timed(host.as_ref(), "absent (after removal)").await;
    assert_eq!(result.expect("absent"), GitEntry::Absent);
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

const DEFAULT_WIN_DATA_DIR: &str = "C:\\Program Files\\Inductive Automation\\Ignition\\data";

/// What the Windows gate needs once it is open.
struct LiveWin {
    host: String,
    user: String,
    key_file: PathBuf,
    known_hosts: PathBuf,
    profile: String,
    data_dir: String,
    resource_group: String,
    vm: String,
}

/// `None` is the quiet green skip.
fn live_windows() -> Option<LiveWin> {
    if std::env::var("IGNITION_LIVE_WINVM").as_deref() != Ok("1") {
        skip("IGNITION_LIVE_WINVM is not 1");
        return None;
    }
    let host = require_env("IGN_WINVM_HOST");
    let key_file = PathBuf::from(require_env("IGN_WINVM_KEY"));
    let known_hosts = PathBuf::from(require_env("IGN_WINVM_KNOWN_HOSTS"));
    let profile = require_env("IGN_WINVM_PROFILE");
    let user = std::env::var("IGN_WINVM_USER").unwrap_or_else(|_| "svc-ignition-deploy".into());
    let data_dir =
        std::env::var("IGN_WINVM_DATA_DIR").unwrap_or_else(|_| DEFAULT_WIN_DATA_DIR.to_string());
    let resource_group =
        std::env::var("IGN_WINVM_RG").unwrap_or_else(|_| "RG-IGN-CLI-WIN-TEST".into());
    let vm = std::env::var("IGN_WINVM_VM").unwrap_or_else(|_| "ign-win-836".into());
    assert!(
        known_hosts.is_file(),
        "the pinned known_hosts file named by IGN_WINVM_KNOWN_HOSTS must exist before anything touches the network"
    );
    assert!(
        key_file.is_absolute() && key_file.is_file(),
        "IGN_WINVM_KEY must name an existing private key file by an absolute path"
    );
    Some(LiveWin {
        host,
        user,
        key_file,
        known_hosts,
        profile,
        data_dir,
        resource_group,
        vm,
    })
}

/// Text that is safe inside a single-quoted PowerShell string and a
/// `C:\...` path.
fn plain_windows_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() > 3
        && bytes[0].is_ascii_alphabetic()
        && &path[1..3] == ":\\"
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '\\' | ':' | '.' | '_' | '-' | ' '))
}

fn target_yaml_windows(live: &LiveWin) -> String {
    format!(
        "schema_version: 1\ngateway: {GATEWAY}\nmode: production\nnodes:\n  master:\n    profile: {profile}\n    ssh:\n      host: {host}\n      user: {user}\n      key_file: '{key}'\n      os: windows\n      data_dir: '{data_dir}'\n      known_hosts: known_hosts\n",
        profile = live.profile,
        host = live.host,
        user = live.user,
        key = live.key_file.display(),
        data_dir = live.data_dir,
    )
}

/// Runs the PowerShell `script` as SYSTEM through `az vm run-command` (the
/// admin route of `23-LIVE-CAPTURES.md`). `az` answers exit 0 for a script
/// that wrote to stderr, so the `[stderr]` section is checked as well.
fn admin_ps(live: &LiveWin, script: &str) -> String {
    let out = Command::new("az")
        .args(["vm", "run-command", "invoke", "-g"])
        .arg(&live.resource_group)
        .arg("-n")
        .arg(&live.vm)
        .args(["--command-id", "RunPowerShellScript", "--scripts"])
        .arg(script)
        .args(["--query", "value[0].message", "-o", "tsv"])
        .output()
        .expect("spawn az");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "admin setup failed (exit {:?}): {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
            .lines()
            .last()
            .unwrap_or("")
    );
    assert!(
        !stdout.contains("[stderr]"),
        "admin setup wrote to stderr: {}",
        stdout
            .lines()
            .skip_while(|l| !l.contains("[stderr]"))
            .collect::<Vec<_>>()
            .join(" | ")
    );
    stdout
}

fn admin_ps_quiet(live: &LiveWin, script: &str) -> String {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| admin_ps(live, script)))
        .unwrap_or_default()
}

/// The PowerShell that removes every `.git*` entry and, with `remove_dir`,
/// the project directory itself. Junctions and symlinks are removed with
/// `cmd /c rmdir` first so nothing behind them is recursed into.
fn ps_clean(dir: &str, remove_dir: bool) -> String {
    let mut s = format!(
        "$P='{dir}'\n\
         icacls $P /reset /T 2>$null | Out-Null\n\
         foreach ($n in '.git','.git-real') {{\n\
           $q = Join-Path $P $n\n\
           if (Get-Item -LiteralPath $q -Force -ErrorAction SilentlyContinue) {{\n\
             cmd /c rmdir \"$q\" 2>$null\n\
             if (Get-Item -LiteralPath $q -Force -ErrorAction SilentlyContinue) {{ Remove-Item -LiteralPath $q -Recurse -Force }}\n\
           }}\n\
         }}\n"
    );
    if remove_dir {
        s.push_str(
            "if (Test-Path -LiteralPath $P) { Remove-Item -LiteralPath $P -Recurse -Force }\n",
        );
    }
    s
}

/// Removes the probe project (also on a failed assertion) and prints how many
/// `CAP25-` entries remain in `data\projects`.
struct CleanupWin<'a> {
    live: &'a LiveWin,
    dir: String,
}

impl Drop for CleanupWin<'_> {
    fn drop(&mut self) {
        assert!(
            plain_windows_path(&self.dir) && self.dir.ends_with(&format!("\\projects\\{PROBE}"))
        );
        let parent = self.dir.rsplit_once('\\').expect("parent").0.to_string();
        let script = format!(
            "{}Get-ChildItem -LiteralPath '{parent}' -Force | ForEach-Object {{ $_.Name }}\n",
            ps_clean(&self.dir, true)
        );
        let listing = admin_ps_quiet(self.live, &script);
        eprintln!(
            "cleanup: CAP25- entries left in data\\projects: {}",
            listing
                .lines()
                .filter(|l| l.trim().starts_with("CAP25-"))
                .count()
        );
    }
}

/// The Windows shapes on the capture host, through `project_host_for_node`:
/// the Hidden `.git` directory (what `git init` and the Git module create on
/// Windows), a visible one, a Hidden file, a junction, a dangling symlink, and
/// the denied case.
#[tokio::test]
#[ignore = "needs the Windows capture VM, az login and IGNITION_LIVE_WINVM=1"]
async fn live_project_host_winvm() {
    let Some(live) = live_windows() else { return };
    let dir = format!(
        "{}\\projects\\{PROBE}",
        live.data_dir.trim_end_matches('\\')
    );
    assert!(
        plain_windows_path(&dir),
        "the data dir must be plain path text for the setup script"
    );
    let root = targets_root(&target_yaml_windows(&live), &live.known_hosts);
    let (target_file, target) = load_target(root.path(), "live").expect("target");
    let node = target.nodes.master;
    let host =
        project_host_for_node(root.path(), &target_file, "nodes.master", &node).expect("host");

    let _cleanup = CleanupWin {
        live: &live,
        dir: dir.clone(),
    };
    let clean = |remove_dir: bool| ps_clean(&dir, remove_dir);
    let make_dir =
        "if (!(Test-Path -LiteralPath $P)) { New-Item -ItemType Directory -Path $P | Out-Null }\n";
    let listing = "Get-ChildItem -LiteralPath $P -Force | ForEach-Object { $_.Name + ' attrs=' + [int]$_.Attributes + ' (' + $_.Attributes + ')' } | Out-String\n";

    // absent: no project directory at all
    admin_ps(&live, &clean(true));
    let (result, _) = timed(host.as_ref(), "absent (no project directory)").await;
    assert_eq!(result.expect("absent"), GitEntry::Absent);
    assert_eq!(
        host.read_release_record(GATEWAY, PROBE)
            .await
            .expect("record read"),
        None,
        "a missing release record is None"
    );

    // absent: a project directory without .git
    admin_ps(&live, &format!("{}{make_dir}{listing}", clean(false)));
    let (result, _) = timed(host.as_ref(), "absent (project directory, no .git)").await;
    assert_eq!(result.expect("absent"), GitEntry::Absent);

    // directory, Hidden (Attributes=18): the shape A4 and Pitfall 4 are about
    let hidden_dir = "New-Item -ItemType Directory -Path (Join-Path $P '.git') | Out-Null\n\
        Set-Content -LiteralPath (Join-Path $P '.git\\HEAD') -Value 'ref: refs/heads/main'\n\
        attrib +h (Join-Path $P '.git')\n";
    let out = admin_ps(
        &live,
        &format!("{}{make_dir}{hidden_dir}{listing}", clean(false)),
    );
    eprintln!(
        "setup: {}",
        out.lines()
            .find(|l| l.contains(".git"))
            .unwrap_or("")
            .trim()
    );
    assert!(
        out.contains("attrs=18"),
        "the .git must be Hidden, Directory: {out}"
    );
    let (result, _) = timed(host.as_ref(), "directory (Hidden, Attributes=18)").await;
    assert_eq!(result.expect("hidden directory"), GitEntry::Directory);

    // directory, visible
    let visible_dir = "New-Item -ItemType Directory -Path (Join-Path $P '.git') | Out-Null\n\
        Set-Content -LiteralPath (Join-Path $P '.git\\HEAD') -Value 'ref: refs/heads/main'\n";
    admin_ps(&live, &format!("{}{make_dir}{visible_dir}", clean(false)));
    let (result, _) = timed(host.as_ref(), "directory (visible)").await;
    assert_eq!(result.expect("visible directory"), GitEntry::Directory);

    // file, Hidden
    let hidden_file = "Set-Content -LiteralPath (Join-Path $P '.git') -Value 'gitdir: .\\.git-real'\n\
        attrib +h (Join-Path $P '.git')\n";
    admin_ps(&live, &format!("{}{make_dir}{hidden_file}", clean(false)));
    let (result, _) = timed(host.as_ref(), "file (Hidden)").await;
    assert_eq!(result.expect("hidden file"), GitEntry::File);

    // junction (a reparse point on a directory): present, not followed
    let junction = "New-Item -ItemType Directory -Path (Join-Path $P '.git-real') | Out-Null\n\
        cmd /c mklink /J (Join-Path $P '.git') (Join-Path $P '.git-real') | Out-Null\n";
    admin_ps(&live, &format!("{}{make_dir}{junction}", clean(false)));
    let (result, _) = timed(host.as_ref(), "junction").await;
    assert_eq!(result.expect("junction"), GitEntry::Symlink);

    // dangling file symlink: must be present, never absent
    let dangling = "cmd /c mklink (Join-Path $P '.git') 'C:\\cap25-nowhere\\target' | Out-Null\n";
    let out = admin_ps(
        &live,
        &format!("{}{make_dir}{dangling}{listing}", clean(false)),
    );
    eprintln!(
        "setup: {}",
        out.lines()
            .find(|l| l.contains(".git"))
            .unwrap_or("")
            .trim()
    );
    let (result, _) = timed(host.as_ref(), "symlink (dangling)").await;
    assert_eq!(result.expect("dangling symlink"), GitEntry::Symlink);

    // denied: the deploy account has no right on the project directory, so the
    // probe cannot know; it must say so, never "absent"
    let deny = "icacls $P /inheritance:d | Out-Null\n\
        icacls $P /remove:g 'BUILTIN\\Users' | Out-Null\n\
        icacls $P\n";
    let out = admin_ps(
        &live,
        &format!("{}{make_dir}{hidden_dir}{deny}", clean(false)),
    );
    assert!(
        !out.to_lowercase().contains("svc-ignition-deploy") && !out.contains("BUILTIN\\Users"),
        "the deploy account must have no right left on the project directory: {out}"
    );
    let (result, _) = timed(host.as_ref(), "denied (no right for the deploy account)").await;
    let err = result.expect_err("denied is a failure, not absent");
    assert!(matches!(&err, CoreError::TransportFailed { .. }), "{err:?}");
    eprintln!("denied detail: {err}");

    // put back, remove: absent again
    admin_ps(&live, &clean(true));
    let (result, _) = timed(host.as_ref(), "absent (after removal)").await;
    assert_eq!(result.expect("absent"), GitEntry::Absent);
}

// ---------------------------------------------------------------------------
// The release record write (25-13)
// ---------------------------------------------------------------------------

/// A release record for [`REC_GATEWAY`] / [`REC_PROJECT`].
fn record_bytes(release: &str, run_id: &str) -> Vec<u8> {
    ReleaseRecord {
        schema_version: 1,
        gateway: REC_GATEWAY.to_string(),
        project: REC_PROJECT.to_string(),
        release: release.to_string(),
        artifact_sha256: "ab".repeat(32),
        run_id: run_id.to_string(),
        deployed_at: "2026-10-23T00:00:00Z".to_string(),
    }
    .to_bytes()
}

/// Runs `script` on the Linux host as the deploy account (the account the
/// record is written as), over the same pinned-host-key ssh the tests use for
/// setup. The key file is handed to `ssh -i` and never read.
fn deploy_sh(live: &Live, script: &str) -> String {
    let out = Command::new("ssh")
        .arg("-i")
        .arg(&live.key_file)
        .arg("-o")
        .arg(format!("UserKnownHostsFile={}", live.known_hosts.display()))
        .args([
            "-o",
            "StrictHostKeyChecking=yes",
            "-o",
            "IdentitiesOnly=yes",
            "-o",
            "BatchMode=yes",
            "-o",
            "ForwardAgent=no",
            "-o",
            "ConnectTimeout=30",
        ])
        .arg(format!("{}@{}", live.user, live.host))
        .arg(script)
        .output()
        .expect("spawn ssh");
    assert!(
        out.status.success(),
        "deploy-account command failed (exit {:?}): {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
            .lines()
            .last()
            .unwrap_or("")
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn deploy_sh_quiet(live: &Live, script: &str) -> String {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| deploy_sh(live, script)))
        .unwrap_or_default()
}

/// Removes `~/ign-deploy/cap25-gw` when dropped (and `~/ign-deploy` when the
/// test made it), so a failed assertion still cleans up, and prints how many
/// entries of the test's names remain.
struct RecordCleanup<'a> {
    live: &'a Live,
    /// `~/ign-deploy` existed before the test.
    base_existed: bool,
}

impl Drop for RecordCleanup<'_> {
    fn drop(&mut self) {
        let mut script = format!("rm -rf \"$HOME/ign-deploy/{REC_GATEWAY}\"; ");
        if !self.base_existed {
            script.push_str("rmdir \"$HOME/ign-deploy\" 2>/dev/null; ");
        }
        script.push_str(&format!(
            "ls -1A \"$HOME/ign-deploy\" 2>/dev/null | grep -c '^{REC_GATEWAY}$'; \
             test -d \"$HOME/ign-deploy\" && echo base-present || echo base-absent"
        ));
        let out = deploy_sh_quiet(self.live, &script);
        eprintln!(
            "cleanup: {REC_GATEWAY} entries left in ~/ign-deploy: {}; {}",
            out.lines().next().unwrap_or("?").trim(),
            out.lines().nth(1).unwrap_or("?").trim()
        );
    }
}

/// The Linux record write through `release_writer_for_node`, read back through
/// `project_host_for_node`, overwritten once, then removed.
#[tokio::test]
#[ignore = "needs the Linux capture VM and IGNITION_LIVE_LNXVM=1"]
async fn live_record_write_lnxvm() {
    let Some(live) = live_linux() else { return };
    let data_dir = live.data_dir.trim_end_matches('/').to_string();
    assert!(
        plain_path(&data_dir),
        "the data dir must be plain path text for the check script"
    );
    let root = targets_root(&target_yaml(&live), &live.known_hosts);
    let (target_file, target) = load_target(root.path(), "live").expect("target");
    let node = target.nodes.master;
    let writer = release_writer_for_node(root.path(), &target_file, "nodes.master", &node)
        .expect("ssh writer");
    let host =
        project_host_for_node(root.path(), &target_file, "nodes.master", &node).expect("host");

    let base_existed =
        deploy_sh(&live, "test -d \"$HOME/ign-deploy\" && echo yes || echo no").trim() == "yes";
    let _cleanup = RecordCleanup {
        live: &live,
        base_existed,
    };
    // start clean: nothing named cap25-gw
    deploy_sh(&live, &format!("rm -rf \"$HOME/ign-deploy/{REC_GATEWAY}\""));
    assert_eq!(
        host.read_release_record(REC_GATEWAY, REC_PROJECT)
            .await
            .expect("read"),
        None,
        "no record yet"
    );

    // write, read back
    let first = record_bytes("v1.0.0", "CAP25-RUN-1");
    let started = Instant::now();
    writer
        .write_release_record(REC_GATEWAY, REC_PROJECT, &first)
        .await
        .expect("first write");
    eprintln!("write 1: {} ms", started.elapsed().as_millis());
    let back = host
        .read_release_record(REC_GATEWAY, REC_PROJECT)
        .await
        .expect("read")
        .expect("the record exists");
    eprintln!(
        "record 1: sha256 written {} read back {}",
        sha256_hex(&first),
        sha256_hex(&back)
    );
    assert_eq!(back, first);

    // overwrite, read the new bytes
    let second = record_bytes("v1.1.0", "CAP25-RUN-2");
    let started = Instant::now();
    writer
        .write_release_record(REC_GATEWAY, REC_PROJECT, &second)
        .await
        .expect("overwrite");
    eprintln!("write 2: {} ms", started.elapsed().as_millis());
    let back = host
        .read_release_record(REC_GATEWAY, REC_PROJECT)
        .await
        .expect("read")
        .expect("the record exists");
    eprintln!(
        "record 2: sha256 written {} read back {}",
        sha256_hex(&second),
        sha256_hex(&back)
    );
    assert_eq!(back, second);
    assert_ne!(sha256_hex(&first), sha256_hex(&second));

    // the directory holds the record and nothing else, and the data directory
    // was never written
    let listing = deploy_sh(
        &live,
        &format!(
            "ls -1A \"$HOME/ign-deploy/{REC_GATEWAY}/projects/{REC_PROJECT}\"; \
             stat -c 'mode=%a size=%s' \"$HOME/ign-deploy/{REC_GATEWAY}/projects/{REC_PROJECT}/release.json\"; \
             test -e '{data_dir}/ign-deploy/{REC_GATEWAY}' && echo data-dir-touched || echo data-dir-untouched"
        ),
    );
    eprintln!("after the overwrite: {}", listing.replace('\n', " | "));
    let lines: Vec<&str> = listing.lines().collect();
    assert_eq!(
        lines[0], "release.json",
        "no temporary file is left: {listing}"
    );
    assert_eq!(lines[2], "data-dir-untouched", "{listing}");
}
