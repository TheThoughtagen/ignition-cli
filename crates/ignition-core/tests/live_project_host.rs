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
//! ## What is written, and where
//!
//! Setup runs as `ignadmin` with sudo (the admin route recorded in
//! `23-LIVE-CAPTURES.md`), never as the deploy account, and touches exactly
//! one path: `<data_dir>/projects/CAP25-GITPROBE`, which this test creates and
//! removes (also when an assertion fails). The deploy account only reads. The
//! output carries shapes, kinds and wall times, never a key.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use ignition_core::actions::deploy::load_target;
use ignition_core::deploy::transport::projects::{GitEntry, ProjectHost, project_host_for_node};
use ignition_core::error::CoreError;

const DEFAULT_DATA_DIR: &str = "/usr/local/bin/ignition/data";
/// The only project name this file creates.
const PROBE: &str = "CAP25-GITPROBE";
/// The gateway name in the target; the record read is for this gateway.
const GATEWAY: &str = "live-project-host";

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
