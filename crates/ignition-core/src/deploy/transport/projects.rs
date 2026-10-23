//! How `deploy projects` reads a gateway project's host side (Phase 25).
//!
//! The gateway REST API cannot see `data/projects/<name>/.git` or the
//! per-project release record, so the host is read directly. [`ProjectHost`]
//! is the **read-only** surface the verb receives: it has no method that
//! writes, deletes or renames anything, so "the verb never removes a `.git`"
//! (D-11) is a fact about the type, not a convention. An administrator
//! removes `.git` per project; `ign` only reports it.
//!
//! Two properties every implementation keeps:
//!
//! - **A failed probe is never an absent one.** [`GitEntry::Absent`] is
//!   returned only when the host answered that there is no such entry (no
//!   project directory, or a project directory without `.git`). A missing
//!   `data/projects` directory, an unreadable project directory or a denied
//!   probe is [`CoreError::TransportFailed`]: a probe that silently turned
//!   "denied" into "no git" would let a deploy run over a project the git
//!   module still manages (research Pitfall 3, T-25-20). The 25-03 captures
//!   showed that `test -e`, an sftp listing, `Directory.Exists` and
//!   `Test-Path` each answer a false "absent" under denial, which is why the
//!   probe is a `stat` (POSIX) or `Get-Item -Force` (Windows) and why any
//!   `.git` entry of any kind counts (K9).
//! - **Names are checked before they become paths.** The project name passes
//!   [`component_problem`] before any path is built (T-25-21).
//!
//! The release record read mirrors `TreeReader::read_ledger`: `None` only when
//! no level of its path exists; a symlink, a non-regular file, a file over
//! [`RELEASE_RECORD_MAX_BYTES`](super::super::ledger::RELEASE_RECORD_MAX_BYTES)
//! or a read error is `deploy_run_invalid` naming the location.
//!
//! [`project_host_for_node`] is the only way to obtain a host: the `local` arm
//! reads the runner's own filesystem, the `ssh` arm (25-08) reads a Linux or
//! Windows gateway host through the same hardened shell `for_node` builds.

use std::path::Path;

use serde::Serialize;

use super::{NodeAccess, local, node_access, sftp_transport};
use crate::deploy::target::NodeTarget;
use crate::error::CoreError;

/// What `data/projects/<name>/.git` is, when it exists. Never followed: a
/// symlink is reported as a symlink.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GitEntry {
    /// The project directory or its `.git` does not exist.
    Absent,
    /// A directory: the git module's working state.
    Directory,
    /// A regular file: a worktree or submodule pointer.
    File,
    /// A symlink (not followed).
    Symlink,
    /// Anything else (a device, a socket, a pipe).
    Other,
}

impl GitEntry {
    /// True for every kind but [`GitEntry::Absent`]: any `.git` entry marks a
    /// project the git module may still manage (K9).
    pub fn is_present(self) -> bool {
        !matches!(self, GitEntry::Absent)
    }

    /// The kind as the refusal message and the JSON name it.
    pub fn as_str(self) -> &'static str {
        match self {
            GitEntry::Absent => "absent",
            GitEntry::Directory => "directory",
            GitEntry::File => "file",
            GitEntry::Symlink => "symlink",
            GitEntry::Other => "other",
        }
    }
}

/// The `.git` path of `project` as a refusal and a log name it: relative to
/// the node's data directory, `projects/<project>/.git`. The probe always
/// reads below the data directory, whatever root the release record uses.
pub fn git_entry_location(project: &str) -> String {
    format!("projects/{project}/.git")
}

/// The read-only host surface of `deploy projects`.
///
/// There is deliberately no method that writes, deletes or renames: the
/// release-record writer is a separate trait that arrives with 25-11, and
/// nothing in the phase removes a `.git` (D-11).
#[async_trait::async_trait]
pub trait ProjectHost: Send + Sync {
    /// What `<data_dir>/projects/<project>/.git` is. [`GitEntry::Absent`]
    /// only when the host says the project directory or `.git` does not
    /// exist; any other failure (a missing `projects` directory, a denied or
    /// unreadable path, a project that is not a directory) is
    /// [`CoreError::TransportFailed`], never `Absent`.
    async fn git_entry(&self, project: &str) -> Result<GitEntry, CoreError>;

    /// The release-record bytes of `project` on `gateway` at
    /// [`release_record_location`](crate::deploy::ledger::release_record_location)
    /// below the node's ledger root. `None` means no level of that path
    /// exists (no `ign` deploy yet). A record that exists but cannot be read
    /// (a symlink, a non-regular file, over
    /// [`RELEASE_RECORD_MAX_BYTES`](crate::deploy::ledger::RELEASE_RECORD_MAX_BYTES),
    /// a read error) is [`CoreError::DeployRunInvalid`] naming the location,
    /// never `None`. The bytes are not validated here.
    async fn read_release_record(
        &self,
        gateway: &str,
        project: &str,
    ) -> Result<Option<Vec<u8>>, CoreError>;
}

/// The host for `node`, from its target block.
///
/// `local` builds the local-filesystem host (a relative `data_dir` resolves
/// against `base`, the targets root). A node with neither `ssh` nor `local` is
/// refused as `deploy_tree_invalid` ("set ssh or local; this verb needs host
/// access", research F3) naming `label`. `ssh` builds the sftp host exactly as
/// [`for_node`](super::for_node) does (`data_dir` normalised to the sftp
/// spelling, the pinned `known_hosts` resolved against `base`); no connection
/// is opened until a method is called.
pub fn project_host_for_node(
    base: &Path,
    target_file: &str,
    label: &str,
    node: &NodeTarget,
) -> Result<Box<dyn ProjectHost>, CoreError> {
    match node_access(base, target_file, label, node)? {
        NodeAccess::Local(data_dir) => Ok(Box::new(local::LocalFsTransport::new(data_dir, label))),
        NodeAccess::Ssh {
            shell,
            data_dir,
            windows,
        } => Ok(Box::new(sftp_transport(*shell, data_dir, label, windows))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_entry_kinds_name_themselves() {
        assert!(!GitEntry::Absent.is_present());
        for kind in [
            GitEntry::Directory,
            GitEntry::File,
            GitEntry::Symlink,
            GitEntry::Other,
        ] {
            assert!(kind.is_present(), "{kind:?}");
        }
        assert_eq!(GitEntry::Directory.as_str(), "directory");
        assert_eq!(GitEntry::File.as_str(), "file");
        assert_eq!(GitEntry::Symlink.as_str(), "symlink");
        assert_eq!(git_entry_location("CapApp"), "projects/CapApp/.git");
    }

    /// An ssh target whose `known_hosts` is the given repo-relative path.
    fn ssh_target(known_hosts: &str) -> Result<NodeTarget, CoreError> {
        let yaml = format!(
            "schema_version: 1\ngateway: g\nmode: m\nnodes:\n  master:\n    profile: p\n    ssh:\n      host: lnx.invalid\n      user: svc\n      os: linux\n      data_dir: /usr/local/bin/ignition/data\n      known_hosts: {known_hosts}\n"
        );
        crate::deploy::target::Target::parse("targets/live.yaml", yaml.as_bytes())
            .map(|target| target.nodes.master)
    }

    /// D-07, D-14, T-25-26: with `--targets DIR`, `base` is that directory, so
    /// the pinned `known_hosts` is read from the targets checkout and from
    /// nowhere else; a path that escapes the checkout never gets as far as a
    /// read.
    #[test]
    fn project_host_known_hosts_from_targets_root() {
        let entry = "lnx.invalid ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFakeHostKeyForTheUnitTestOnly0000000\n";
        let node = ssh_target("hosts/lnx.known_hosts").expect("target parses");

        // the file under the targets root is found: the host is built and no
        // connection is made (the host name does not resolve)
        let targets_root = tempfile::tempdir().expect("targets root");
        std::fs::create_dir_all(targets_root.path().join("hosts")).expect("hosts dir");
        std::fs::write(targets_root.path().join("hosts/lnx.known_hosts"), entry)
            .expect("known_hosts");
        project_host_for_node(
            targets_root.path(),
            "targets/live.yaml",
            "nodes.master",
            &node,
        )
        .map(|_| ())
        .expect("the ssh host is built from the targets root");

        // another base (the repo, the working directory) does not hold the
        // file, so the same node is refused naming the repo-relative path
        let elsewhere = tempfile::tempdir().expect("elsewhere");
        let err =
            project_host_for_node(elsewhere.path(), "targets/live.yaml", "nodes.master", &node)
                .map(|_| ())
                .expect_err("no known_hosts under this base");
        let CoreError::DeployTreeInvalid(detail) = err else {
            panic!("expected deploy_tree_invalid, got {err:?}");
        };
        let shown = format!("{detail:?}");
        assert!(shown.contains("hosts/lnx.known_hosts"), "{shown}");

        // a known_hosts path that leaves the targets root is refused by the
        // target parser, before any host is built
        for escaping in ["../x", "hosts/../../x", "/etc/ssh/ssh_known_hosts"] {
            let err = ssh_target(escaping).expect_err(escaping);
            let CoreError::DeployTreeInvalid(detail) = err else {
                panic!("expected deploy_tree_invalid, got {err:?}");
            };
            assert!(
                detail
                    .findings
                    .iter()
                    .any(|f| f.message.starts_with("nodes.master.ssh.known_hosts:")),
                "{escaping}: {:?}",
                detail.findings
            );
        }
    }
}
