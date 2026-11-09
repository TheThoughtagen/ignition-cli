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
//! The tag record read (26-06) follows the same rule at
//! [`tag_record_location`](super::super::ledger::tag_record_location), capped
//! at [`TAG_RECORD_MAX_BYTES`](super::super::tag_record::TAG_RECORD_MAX_BYTES).
//! It has no default body: a host that cannot answer must not read as "no
//! owner" (D-05).
//!
//! [`project_host_for_node`] is the only way to obtain a host: the `local` arm
//! reads the runner's own filesystem, the `ssh` arm (25-08) reads a Linux or
//! Windows gateway host through the same hardened shell `for_node` builds.

use std::path::Path;

use serde::Serialize;

use super::remote::safe_remote_path;
use super::winps::windows_component_problem;
use super::{NodeAccess, local, node_access, sftp_transport};
use crate::deploy::apply::run_store::RunId;
use crate::deploy::component_problem;
use crate::deploy::project_run::{PRE_IMAGE_MAX_BYTES, PreImageFile, PreImageRun};
use crate::deploy::target::{NodeTarget, TargetOs};
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
/// release-record writer is the separate [`ReleaseRecordWriter`], the
/// tag-record writer the separate [`TagRecordWriter`], and nothing in the
/// phase removes a `.git` (D-11).
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

    /// The tag-record bytes of `provider` on `gateway` at
    /// [`tag_record_location`](crate::deploy::ledger::tag_record_location)
    /// below the node's ledger root (26-06). `None` means no level of that
    /// path exists (no project has deployed the provider yet). A record that
    /// exists but cannot be read (a symlink, a non-regular file, over
    /// [`TAG_RECORD_MAX_BYTES`](crate::deploy::tag_record::TAG_RECORD_MAX_BYTES),
    /// a read error) or a name that cannot become that path is
    /// [`CoreError::DeployRunInvalid`] naming the location, and an unreadable
    /// host is [`CoreError::TransportFailed`]; neither is ever `None`. The
    /// bytes are not validated here: [`TagRecord::parse`] does that.
    ///
    /// [`TagRecord::parse`]: crate::deploy::tag_record::TagRecord::parse
    async fn read_tag_record(
        &self,
        gateway: &str,
        provider: &str,
    ) -> Result<Option<Vec<u8>>, CoreError>;

    /// The host journal of a tag-owning run of `project` on `gateway` at
    /// [`project_journal_location`](crate::deploy::ledger::project_journal_location)
    /// (26-12). `None` only when no level of that path exists; a symlink, a
    /// non-regular file, a file over
    /// [`PROJECT_JOURNAL_MAX_BYTES`](crate::deploy::project_run::PROJECT_JOURNAL_MAX_BYTES)
    /// or an unusable name is [`CoreError::DeployRunInvalid`], an unreadable
    /// host [`CoreError::TransportFailed`]. The bytes are not validated here.
    async fn read_journal(
        &self,
        gateway: &str,
        project: &str,
    ) -> Result<Option<Vec<u8>>, CoreError>;

    /// One file of the latest run's pre-image of `project` on `gateway`,
    /// `name` one of [`PreImageFile`]'s names (26-12), under the same rule as
    /// [`read_journal`](Self::read_journal) with the cap
    /// [`PRE_IMAGE_MAX_BYTES`](crate::deploy::project_run::PRE_IMAGE_MAX_BYTES).
    /// A name outside the fixed set is [`CoreError::DeployRunInvalid`].
    async fn read_pre_image(
        &self,
        gateway: &str,
        project: &str,
        name: &str,
    ) -> Result<Option<Vec<u8>>, CoreError>;
}

/// The host writes of a tag-owning `deploy projects` run besides its records
/// (26-12, TAGDEP-10): the journal of the run in flight and the latest run's
/// pre-image, beside the release record.
///
/// Separate from [`ProjectHost`] and from the record writers on purpose: no
/// read-only surface can reach it, and the source scan
/// `project_run_writer_is_confined` lets only `deploy/transport/` and
/// `actions/deploy_projects.rs` name it. An implementation writes only
/// `journal.json` and the fixed names of [`PreImageFile`] below
/// [`project_journal_rel`](crate::deploy::ledger::project_journal_rel)'s
/// directory, each as a private (0600 where the host has modes) temporary
/// sibling named for the run, renamed over the file in one atomic step. A
/// symlink on the path is never followed. It fails closed: the names pass
/// [`project_run_names_problem_on`] for the node's family, the journal bytes
/// must parse as this gateway's and project's [`ProjectJournal`], and a
/// `run.json` as this run's [`PreImageRun`].
///
/// [`ProjectJournal`]: crate::deploy::project_run::ProjectJournal
/// [`PreImageRun`]: crate::deploy::project_run::PreImageRun
#[async_trait::async_trait]
pub trait ProjectRunWriter: Send + Sync {
    /// Write (or replace) the journal `bytes` of `project` on `gateway`.
    async fn write_journal(
        &self,
        gateway: &str,
        project: &str,
        bytes: &[u8],
    ) -> Result<(), CoreError>;

    /// Remove `journal.json` of `project` on `gateway` and nothing else; a
    /// journal that is already absent is a success. A symlink or a
    /// non-regular file there is refused and left.
    async fn clear_journal(&self, gateway: &str, project: &str) -> Result<(), CoreError>;

    /// Write (or replace) the pre-image file `name` of run `run_id`.
    async fn write_pre_image(
        &self,
        gateway: &str,
        project: &str,
        run_id: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<(), CoreError>;
}

/// Why `gateway`, `project` or the pre-image file `name` cannot be used for
/// the project run files on a host family (`None` for a local data
/// directory, `Some(windows)` for an ssh host), or `None` (26-12). Both names
/// pass the tag-record name rules of that family; `name` must be one of
/// [`PreImageFile`]'s names, and its provider must pass them too.
pub(crate) fn project_run_names_problem_on(
    gateway: &str,
    project: &str,
    name: Option<&str>,
    ssh_windows: Option<bool>,
) -> Option<String> {
    if let Some(why) = tag_record_names_problem_on(gateway, project, ssh_windows) {
        return Some(why.replacen("the provider name", "the project name", 1));
    }
    let name = name?;
    let Some(file) = PreImageFile::parse(name) else {
        return Some(
            "the pre-image file name is not one of run.json, project.zip, tags-<P>.json, \
             provider-<P>.json or record-<P>.json"
                .to_string(),
        );
    };
    let provider = file.provider()?;
    tag_record_names_problem_on(gateway, provider, ssh_windows)
        .map(|why| format!("the pre-image file name: {why}"))
}

/// A `deploy_run_invalid` naming `location`.
fn run_invalid(location: &str, problem: impl std::fmt::Display) -> CoreError {
    CoreError::DeployRunInvalid {
        run_id: "unknown".to_string(),
        problem: format!("{location}: {problem}"),
    }
}

/// The checks every pre-image write applies, on any host family, before a
/// byte is written: the names, the run id, the size, and for `run.json` the
/// set itself (this gateway's, project's and run's).
pub(crate) fn check_pre_image_write(
    location: &str,
    gateway: &str,
    project: &str,
    run_id: &str,
    name: &str,
    bytes: &[u8],
    ssh_windows: Option<bool>,
) -> Result<(), CoreError> {
    if let Some(why) = project_run_names_problem_on(gateway, project, Some(name), ssh_windows) {
        return Err(run_invalid(location, why));
    }
    if RunId::parse(run_id).is_err() {
        return Err(run_invalid(
            location,
            "the run id is not 1 to 128 characters of [A-Za-z0-9._-]",
        ));
    }
    if bytes.len() > PRE_IMAGE_MAX_BYTES {
        return Err(run_invalid(
            location,
            format!(
                "the file is {} bytes, over the {PRE_IMAGE_MAX_BYTES} byte cap",
                bytes.len()
            ),
        ));
    }
    if PreImageFile::parse(name) == Some(PreImageFile::Run) {
        let run = PreImageRun::parse(bytes, location, gateway, project)?;
        if run.run_id != run_id {
            return Err(run_invalid(location, "run.json names another run"));
        }
    }
    Ok(())
}

/// The project run writer for `node`, built as [`tag_record_writer_for_node`]
/// builds the tag-record writer. No connection is opened until a write is
/// called.
pub fn project_run_writer_for_node(
    base: &Path,
    target_file: &str,
    label: &str,
    node: &NodeTarget,
) -> Result<Box<dyn ProjectRunWriter>, CoreError> {
    match node_access(base, target_file, label, node)? {
        NodeAccess::Local(data_dir) => Ok(Box::new(local::LocalFsTransport::new(data_dir, label))),
        NodeAccess::Ssh {
            shell,
            data_dir,
            windows,
        } => Ok(Box::new(sftp_transport(*shell, data_dir, label, windows))),
    }
}

/// The one write `deploy projects` makes to a gateway host: the release
/// record (V5, D-03).
///
/// It is a separate trait from [`ProjectHost`] on purpose: no read-only
/// surface (`--check`, the `.git` probe) can reach it, and the source scan
/// `release_writer_is_confined` lets only `deploy/transport/` and
/// `actions/deploy_projects.rs` name it. An implementation writes exactly one
/// file, [`release_record_rel`](crate::deploy::ledger::release_record_rel)
/// below the node's ledger root, and nothing else (no `.git`, no project file,
/// no other record). It lands as a private temporary sibling that is then
/// renamed over the record in one atomic step (Linux `mv -f`, Windows
/// `MoveFileEx` with `MOVEFILE_REPLACE_EXISTING`, 25-20), so a concurrent
/// reader sees the old record or the new one, never a missing or half-written
/// one.
///
/// That guarantee is the atomic rename of NTFS on Windows and of `rename(2)` on
/// Linux: the replace is atomic for readers and it is not a durable write. A
/// replace that is still blocked after its bounded retries (a reader that
/// holds the record open without delete sharing) fails with `transport_failed`
/// naming the cause, and the previous record stays. A symlink on the path is
/// never followed.
///
/// It fails closed. `gateway` and `project` pass the same name check the
/// record read applies, `bytes` must parse as a [`ReleaseRecord`] naming that
/// gateway and project (so the writer cannot be used to put anything else on
/// the host), and a path that cannot be used is an error, never a silent skip.
///
/// [`ReleaseRecord`]: crate::deploy::projects::ReleaseRecord
#[async_trait::async_trait]
pub trait ReleaseRecordWriter: Send + Sync {
    /// Write the release record `bytes` of `project` on `gateway`.
    async fn write_release_record(
        &self,
        gateway: &str,
        project: &str,
        bytes: &[u8],
    ) -> Result<(), CoreError>;
}

/// The one tag-ownership write `deploy projects` makes to a gateway host: the
/// tag record of one owned provider (26-06, TAGDEP-09).
///
/// Separate from [`ProjectHost`] and from [`ReleaseRecordWriter`] on purpose:
/// no read-only surface can reach it, and the source scan
/// `tag_record_writer_is_confined` lets only `deploy/transport/` and
/// `actions/deploy_projects.rs` name it. An implementation writes exactly one
/// file, [`tag_record_rel`](crate::deploy::ledger::tag_record_rel) below the
/// node's ledger root, and nothing else. Like the release record it lands as
/// a private temporary sibling named for the run and is renamed over the old
/// record in one atomic step (Linux `mv -f`, Windows `MoveFileEx` with
/// `MOVEFILE_REPLACE_EXISTING`); a reader sees the old record or the new one.
/// A symlink on the path is never followed.
///
/// It fails closed. `gateway` and `provider` pass
/// [`tag_record_names_problem`] for the node's family, `bytes` must parse as a
/// [`TagRecord`] naming that gateway and provider exactly (so the writer cannot
/// put anything else on the host), and a path that cannot be used is an
/// error, never a silent skip.
///
/// [`TagRecord`]: crate::deploy::tag_record::TagRecord
#[async_trait::async_trait]
pub trait TagRecordWriter: Send + Sync {
    /// Write the tag record `bytes` of `provider` on `gateway`.
    async fn write_tag_record(
        &self,
        gateway: &str,
        provider: &str,
        bytes: &[u8],
    ) -> Result<(), CoreError>;
}

/// Why `gateway` or `provider` cannot name a tag record on `node`, or `None`
/// (26-06, T-26-21).
///
/// The writers apply exactly this check, so a caller that runs it offline
/// (before any contact) refuses every name a writer would refuse after the
/// import. On every node a name must be one safe path component without a
/// control character. On an `ssh` node the record path reaches sftp on a
/// batch line, and sftp globs, so a name with `[ ] { } * ?`, a quote or a
/// backslash is refused too; on a Windows `ssh` node the Windows name rules
/// (device names, a trailing dot or space, `:` and the reserved characters)
/// apply as well.
pub fn tag_record_names_problem(
    gateway: &str,
    provider: &str,
    node: &NodeTarget,
) -> Option<String> {
    tag_record_names_problem_on(
        gateway,
        provider,
        node.ssh.as_ref().map(|ssh| ssh.os == TargetOs::Windows),
    )
}

/// [`tag_record_names_problem`] for a host family: `None` for a local data
/// directory, `Some(windows)` for an ssh host.
pub(crate) fn tag_record_names_problem_on(
    gateway: &str,
    provider: &str,
    ssh_windows: Option<bool>,
) -> Option<String> {
    for (kind, value) in [("gateway", gateway), ("provider", provider)] {
        if let Some(why) = component_problem(value) {
            return Some(format!("the {kind} name {why}"));
        }
        if value.chars().any(char::is_control) {
            return Some(format!("the {kind} name holds a control character"));
        }
        let Some(windows) = ssh_windows else {
            continue;
        };
        if windows && let Some(why) = windows_component_problem(value) {
            return Some(format!("the {kind} name: {why}"));
        }
        if let Err(why) = safe_remote_path(value) {
            return Some(format!(
                "the {kind} name cannot be used for the tag record on an ssh node ({why})"
            ));
        }
    }
    None
}

/// The tag-record writer for `node`, from its target block, built exactly as
/// [`release_writer_for_node`] builds the release-record writer: the `local`
/// arm writes below the runner's own data directory, the `ssh` arm in the
/// deploy account's home. No connection is opened until the write is called.
pub fn tag_record_writer_for_node(
    base: &Path,
    target_file: &str,
    label: &str,
    node: &NodeTarget,
) -> Result<Box<dyn TagRecordWriter>, CoreError> {
    match node_access(base, target_file, label, node)? {
        NodeAccess::Local(data_dir) => Ok(Box::new(local::LocalFsTransport::new(data_dir, label))),
        NodeAccess::Ssh {
            shell,
            data_dir,
            windows,
        } => Ok(Box::new(sftp_transport(*shell, data_dir, label, windows))),
    }
}

/// The release-record writer for `node`, from its target block.
///
/// The `local` arm writes below the runner's own data directory. The `ssh` arm
/// (25-13) builds the sftp transport exactly as [`project_host_for_node`] does
/// and writes in the deploy account's home, the place the record read looks
/// (Decision B(i)): nothing is written under the gateway's data directory. No
/// connection is opened until the write is called.
pub fn release_writer_for_node(
    base: &Path,
    target_file: &str,
    label: &str,
    node: &NodeTarget,
) -> Result<Box<dyn ReleaseRecordWriter>, CoreError> {
    match node_access(base, target_file, label, node)? {
        NodeAccess::Local(data_dir) => Ok(Box::new(local::LocalFsTransport::new(data_dir, label))),
        NodeAccess::Ssh {
            shell,
            data_dir,
            windows,
        } => Ok(Box::new(sftp_transport(*shell, data_dir, label, windows))),
    }
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

    /// A local node and an ssh node each get a writer; the ssh one is built
    /// without a connection, from the pinned `known_hosts` under the targets
    /// root, and an ssh node whose `known_hosts` is missing is refused before
    /// anything is written.
    #[test]
    fn release_writer_for_node_builds_local_and_ssh() {
        let dir = tempfile::tempdir().expect("tempdir");
        let local_yaml = format!(
            "schema_version: 1\ngateway: g\nmode: m\nnodes:\n  master:\n    profile: p\n    local:\n      data_dir: '{}'\n",
            dir.path().display()
        );
        let local = crate::deploy::target::Target::parse("targets/dev.yaml", local_yaml.as_bytes())
            .expect("local target")
            .nodes
            .master;
        release_writer_for_node(dir.path(), "targets/dev.yaml", "nodes.master", &local)
            .map(|_| ())
            .expect("a local node has a writer");

        let ssh = ssh_target("hosts/lnx.known_hosts").expect("target parses");
        // no known_hosts under this base: refused naming the file, no host built
        let err = release_writer_for_node(dir.path(), "targets/live.yaml", "nodes.master", &ssh)
            .map(|_| ())
            .expect_err("no known_hosts under this base");
        let CoreError::DeployTreeInvalid(detail) = err else {
            panic!("expected deploy_tree_invalid, got {err:?}");
        };
        assert!(
            format!("{detail:?}").contains("hosts/lnx.known_hosts"),
            "{detail:?}"
        );

        std::fs::create_dir_all(dir.path().join("hosts")).expect("hosts dir");
        std::fs::write(
            dir.path().join("hosts/lnx.known_hosts"),
            "lnx.invalid ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFakeHostKeyForTheUnitTestOnly0000000\n",
        )
        .expect("known_hosts");
        release_writer_for_node(dir.path(), "targets/live.yaml", "nodes.master", &ssh)
            .map(|_| ())
            .expect("an ssh node has a writer");
    }

    /// 26-06: the tag record writer is built for a local and an ssh node the
    /// same way as the release writer, without a connection, and an ssh node
    /// whose `known_hosts` is missing is refused before anything is written.
    #[test]
    fn tag_record_writer_for_node_builds_local_and_ssh() {
        let dir = tempfile::tempdir().expect("tempdir");
        let local_yaml = format!(
            "schema_version: 1\ngateway: g\nmode: m\nnodes:\n  master:\n    profile: p\n    local:\n      data_dir: '{}'\n",
            dir.path().display()
        );
        let local = crate::deploy::target::Target::parse("targets/dev.yaml", local_yaml.as_bytes())
            .expect("local target")
            .nodes
            .master;
        tag_record_writer_for_node(dir.path(), "targets/dev.yaml", "nodes.master", &local)
            .map(|_| ())
            .expect("a local node has a writer");

        let ssh = ssh_target("hosts/lnx.known_hosts").expect("target parses");
        let err = tag_record_writer_for_node(dir.path(), "targets/live.yaml", "nodes.master", &ssh)
            .map(|_| ())
            .expect_err("no known_hosts under this base");
        assert_eq!(err.code(), "deploy_tree_invalid", "{err}");

        std::fs::create_dir_all(dir.path().join("hosts")).expect("hosts dir");
        std::fs::write(
            dir.path().join("hosts/lnx.known_hosts"),
            "lnx.invalid ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFakeHostKeyForTheUnitTestOnly0000000\n",
        )
        .expect("known_hosts");
        tag_record_writer_for_node(dir.path(), "targets/live.yaml", "nodes.master", &ssh)
            .map(|_| ())
            .expect("an ssh node has a writer");
    }

    /// T-26-21: a provider (or gateway) name the ssh tag record writer would
    /// refuse is found offline. A local node takes any name that passed the
    /// component checks; an ssh node also refuses the sftp batch-line
    /// characters, and a Windows ssh node the Windows name rules.
    #[test]
    fn tag_record_names_problem_matches_the_writers() {
        let dir = tempfile::tempdir().expect("tempdir");
        let local_yaml = format!(
            "schema_version: 1\ngateway: g\nmode: m\nnodes:\n  master:\n    profile: p\n    local:\n      data_dir: '{}'\n",
            dir.path().display()
        );
        let local = crate::deploy::target::Target::parse("targets/dev.yaml", local_yaml.as_bytes())
            .expect("local target")
            .nodes
            .master;
        let ssh = ssh_target("hosts/lnx.known_hosts").expect("ssh target");
        let win_yaml = "schema_version: 1\ngateway: g\nmode: m\nnodes:\n  master:\n    profile: p\n    ssh:\n      host: win.invalid\n      user: svc\n      os: windows\n      data_dir: 'C:\\Program Files\\Inductive Automation\\Ignition\\data'\n      known_hosts: hosts/win.known_hosts\n";
        let win = crate::deploy::target::Target::parse("targets/win.yaml", win_yaml.as_bytes())
            .expect("windows target")
            .nodes
            .master;

        for node in [&local, &ssh, &win] {
            assert_eq!(tag_record_names_problem("gw", "Plant A", node), None);
            assert_eq!(tag_record_names_problem("gw", "MQTT Engine", node), None);
            for bad in ["", "..", "a/b", ".x"] {
                assert!(
                    tag_record_names_problem("gw", bad, node).is_some(),
                    "{bad:?}"
                );
            }
        }
        for glob in ["Plant[1]", "Plant*", "Plant?", "Plant{A}", "Plant\"A"] {
            assert_eq!(tag_record_names_problem("gw", glob, &local), None, "{glob}");
            for node in [&ssh, &win] {
                let why = tag_record_names_problem("gw", glob, node).expect(glob);
                assert!(why.contains("provider"), "{why}");
            }
            let why = tag_record_names_problem(glob, "PlantA", &ssh).expect(glob);
            assert!(why.contains("gateway"), "{why}");
        }
        for windows_only in ["NUL", "PlantA.", "a:b"] {
            assert_eq!(
                tag_record_names_problem("gw", windows_only, &ssh),
                None,
                "{windows_only}"
            );
            assert!(
                tag_record_names_problem("gw", windows_only, &win).is_some(),
                "{windows_only}"
            );
        }
    }

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
