//! How a verb reads a gateway node's resource collections (Phase 22).
//!
//! A transport reaches the gateway HOST's `data/config/resources/<collection>/`
//! directories. This module defines the **read half** the verbs receive:
//! [`TreeReader`]. The write half (a mirror driven by an explicit delete set,
//! [`TreeMirror`]) is a separate trait that is never reachable from a verb,
//! so "plan, pull and verify never write to the host" is a compile-time
//! fact for `actions::deploy`, not a convention.
//!
//! Two properties every reader keeps:
//!
//! - **A failed read is never an empty read.** Only an absent collection
//!   directory reads as empty. A missing or unreadable `data_dir`, a symlink,
//!   a special file, a non-UTF-8 name, a file over [`READ_MAX_FILE_BYTES`],
//!   more than [`READ_MAX_ENTRIES`] entries or more than
//!   [`READ_MAX_TOTAL_BYTES`] is [`CoreError::TransportFailed`]. A plan that
//!   mistook an unreadable tree for deletions would be worse than no plan.
//! - **Scoped.** Only the directories the manifest allowlists are opened
//!   (see [`Scope`]); nothing outside them is listed or read.
//!
//! The write half is [`TreeMirror`]; [`mirror_for_node`] is the only way to
//! obtain one, and a source-scan test (`write_half_unreachable`) pins that no
//! verb or surface names it.
//!
//! [`for_node`] chooses the reader for a target node: `local: { data_dir }`
//! selects [`local::LocalFsTransport`] (D-02); `ssh` selects
//! [`sftp::SftpTransport`] over the hardened [`remote::RemoteShell`], for a
//! Linux host with a POSIX `data_dir` and for a Windows OpenSSH host with a
//! drive-letter `data_dir` ([`sftp::windows_sftp_root`], 22-13). A Windows read
//! is cross-checked against a PowerShell enumeration ([`winps`]) before it is
//! reported complete.

pub mod batch;
pub mod local;
mod local_fs;
pub mod process;
pub mod remote;
pub mod sftp;
pub mod winps;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use serde::Serialize;

use super::apply::host::ApplyHost;
use super::layout::{RelPath, TAG_TYPES};
use super::manifest::Manifest;
use super::rules::GATEWAY_OWNED_FILES;
use super::target::{NodeTarget, TargetOs};
use super::tree::RenderedTree;
use crate::error::CoreError;

/// The literal name of the base resource collection on a gateway.
pub const CORE_COLLECTION: &str = "core";

/// A single file larger than this is refused (64 MiB).
pub const READ_MAX_FILE_BYTES: u64 = 67_108_864;
/// All files read in one call together may not exceed this (1 GiB).
pub const READ_MAX_TOTAL_BYTES: u64 = 1_073_741_824;
/// More directory entries than this in one read is refused.
pub const READ_MAX_ENTRIES: usize = 200_000;

/// One live file: its bytes and their lowercase hex SHA-256.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileBlob {
    /// Raw file bytes.
    pub bytes: Vec<u8>,
    /// Lowercase hex SHA-256 of `bytes`.
    pub sha256: String,
}

/// What a read covers.
///
/// Every `types` entry is a `<module>/<type>` directory read whole. Every
/// `tag_providers` entry `P` adds the four [`TAG_TYPES`] directories under
/// `ignition`, each restricted to its `<P>/` subtree. Pull's provider
/// discovery relies on a plain `types` entry for `ignition/tag-provider`
/// being read whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    /// The target's deployment mode: the name of the mode collection.
    pub mode: String,
    /// `(module, type)` directories read whole.
    pub types: Vec<(String, String)>,
    /// Tag provider directory names.
    pub tag_providers: Vec<String>,
}

impl Scope {
    /// The scope of a manifest for `mode`: the manifest's non-tag types and
    /// its tag providers. A `types` entry naming one of the four tag types
    /// is covered by `tag_providers` (restricted to those providers), so it
    /// is not read whole.
    pub fn from_manifest(manifest: &Manifest, mode: &str) -> Scope {
        let types = manifest
            .types
            .iter()
            .filter_map(|t| t.split_once('/'))
            .filter(|(module, ty)| !(*module == "ignition" && TAG_TYPES.contains(ty)))
            .map(|(module, ty)| (module.to_string(), ty.to_string()))
            .collect();
        Scope {
            mode: mode.to_string(),
            types,
            tag_providers: manifest.tag_providers.clone(),
        }
    }
}

/// The live files a read found, keyed by collection-relative path
/// (`<module>/<type>/...`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObservedFiles {
    /// Files of the `core` collection.
    pub core: BTreeMap<RelPath, FileBlob>,
    /// Files of the mode collection (`Scope::mode`).
    pub mode: BTreeMap<RelPath, FileBlob>,
    /// Unit keys present in the `local` collection for the same scope. The
    /// gateway lets `local` override the mode collection; plan warns about
    /// them. Names only, no bytes.
    pub local_units: Vec<String>,
}

/// The files plan proposes to delete from the mode collection: the
/// declared-names ledger's files minus the rendered files (D-05, 22-03).
/// Collection-relative paths, byte-sorted and unique. Serialized as
/// `{"files": [...]}`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct DeleteSet {
    /// The paths, byte-sorted.
    pub files: Vec<RelPath>,
}

/// The read half of a transport.
#[async_trait::async_trait]
pub trait TreeReader: Send + Sync {
    /// The `<module>/<type>` directory pairs of `collection`, byte-sorted;
    /// empty when the collection directory is absent.
    async fn list_types(&self, collection: &str) -> Result<Vec<(String, String)>, CoreError>;

    /// Read the `core` and mode collections within `scope`, and list the
    /// `local` units in scope. Fails closed on any error.
    async fn read(&self, scope: &Scope) -> Result<ObservedFiles, CoreError>;

    /// The declared-names ledger bytes of `gateway` and `mode` at
    /// [`ledger_location`](super::ledger::ledger_location) below the node's
    /// data directory. `None` means the ledger is absent (a first deploy). A
    /// ledger that exists but cannot be read (a symlink, a non-regular file,
    /// over [`LEDGER_MAX_BYTES`](super::ledger::LEDGER_MAX_BYTES), a read
    /// error) is [`CoreError::DeployLedgerInvalid`], never `None` (D-07). The
    /// bytes are not validated here; see
    /// [`Ledger::parse`](super::ledger::Ledger::parse).
    async fn read_ledger(&self, gateway: &str, mode: &str) -> Result<Option<Vec<u8>>, CoreError>;

    /// The apply journal bytes of `gateway` and `mode` at
    /// [`journal_location`](super::ledger::journal_location), under the same
    /// root as the ledger. `None` means there is no journal at any level of
    /// its path: no interrupted run. A journal that exists but cannot be read
    /// (a symlink, a non-regular file, over
    /// [`JOURNAL_MAX_BYTES`](super::apply::journal::JOURNAL_MAX_BYTES), a read
    /// error) is [`CoreError::DeployRunInvalid`] naming the location, never
    /// `None`: `plan`, `apply` and `rollback` must not proceed over a run
    /// they cannot see (DAPL-09 fails closed). The bytes are not validated
    /// here; see [`Journal::parse`](super::apply::journal::Journal::parse).
    async fn read_journal(&self, gateway: &str, mode: &str) -> Result<Option<Vec<u8>>, CoreError>;
}

/// How a node's data directory is reached, resolved once for both halves.
enum NodeAccess {
    /// A directory on the runner's own filesystem.
    Local(std::path::PathBuf),
    /// A host over the hardened ssh shell.
    Ssh {
        shell: Box<remote::RemoteShell>,
        /// The data directory as sftp spells it (`/opt/ignition/data`, or
        /// `/C:/Program Files/.../data` on Windows).
        data_dir: String,
        /// A Windows OpenSSH host.
        windows: bool,
    },
}

/// Resolve `node` to an access path.
fn node_access(
    repo: &Path,
    target_file: &str,
    label: &str,
    node: &NodeTarget,
) -> Result<NodeAccess, CoreError> {
    match (&node.ssh, &node.local) {
        (Some(_), Some(_)) => Err(CoreError::deploy_tree_invalid(
            target_file,
            None,
            None,
            format!("{label}: set ssh or local, not both"),
        )),
        (None, Some(local)) => {
            let configured = Path::new(&local.data_dir);
            // A relative data_dir is relative to the repo, and `--repo` is
            // `.` by default. The mirror validates its staging and resources
            // roots as absolute paths, so the join is absolutized here.
            let data_dir = if configured.is_absolute() {
                configured.to_path_buf()
            } else {
                std::path::absolute(repo.join(configured)).map_err(|e| {
                    CoreError::deploy_tree_invalid(
                        target_file,
                        None,
                        None,
                        format!(
                            "{label}.local.data_dir {:?}: cannot be made absolute ({:?})",
                            local.data_dir,
                            e.kind()
                        ),
                    )
                })?
            };
            Ok(NodeAccess::Local(data_dir))
        }
        (Some(ssh), None) => {
            let windows = ssh.os == TargetOs::Windows;
            let data_dir = if windows {
                sftp::windows_sftp_root(&ssh.data_dir)
            } else {
                sftp::posix_root(&ssh.data_dir)
            }
            .map_err(|why| {
                CoreError::deploy_tree_invalid(
                    target_file,
                    None,
                    None,
                    format!("{label}.ssh.data_dir: {why}"),
                )
            })?;
            let shell = remote::RemoteShell::from_ssh(
                repo,
                target_file,
                label,
                ssh,
                Arc::new(process::TokioProcessRunner),
            )?;
            Ok(NodeAccess::Ssh {
                shell: Box::new(shell),
                data_dir,
                windows,
            })
        }
        (None, None) => Err(CoreError::deploy_tree_invalid(
            target_file,
            None,
            None,
            format!("{label}: set ssh or local; this verb needs host access"),
        )),
    }
}

/// An sftp transport over `shell` for a Linux or a Windows host.
fn sftp_transport(
    shell: remote::RemoteShell,
    data_dir: String,
    label: &str,
    windows: bool,
) -> sftp::SftpTransport {
    if windows {
        sftp::SftpTransport::new_windows(shell, data_dir, label)
    } else {
        sftp::SftpTransport::new(shell, data_dir, label)
    }
}

/// The reader for `node`, from its target block.
///
/// `local` builds the local-filesystem transport (a relative `data_dir`
/// resolves against `repo`). A node with neither `ssh` nor `local` is
/// refused as `deploy_tree_invalid` naming `label`, because a verb that
/// reads the host needs host access (D-02).
pub fn for_node(
    repo: &Path,
    target_file: &str,
    label: &str,
    node: &NodeTarget,
) -> Result<Box<dyn TreeReader>, CoreError> {
    Ok(match node_access(repo, target_file, label, node)? {
        NodeAccess::Local(data_dir) => Box::new(local::LocalFsTransport::new(data_dir, label)),
        NodeAccess::Ssh {
            shell,
            data_dir,
            windows,
        } => Box::new(sftp_transport(*shell, data_dir, label, windows)),
    })
}

/// The write half of a transport: stage a rendered tree, then commit it with
/// an explicit delete set.
///
/// **No Phase 22 verb may import this trait.** `render`, `plan`, `pull` and
/// `verify` receive a [`TreeReader`] from [`for_node`] and cannot reach it;
/// the `write_half_unreachable` test scans the verb sources for
/// `TreeMirror`, `mirror_for_node`, `StagingMode`, `.stage(` and `.commit(`.
/// Phase 23 is the first caller: it calls [`stage`](TreeMirror::stage) before
/// the scan-lock and [`commit`](TreeMirror::commit) inside it.
///
/// What a mirror guarantees, on both transports:
///
/// - **Bounded writes.** [`commit`](TreeMirror::commit) writes exactly the
///   staged files and removes exactly the delete-set files. Nothing else in
///   the collection is touched (never `config-mode.json`, never an
///   undeclared resource, no mirror-style removal of extras). A directory
///   emptied by a delete is removed; the `<module>/<type>` directory and
///   everything above it never is.
/// - **Nothing foreign in the collection.** No temp, backup, staging or
///   dot-named entry is created below `config/resources` (the Designer
///   rejects such names). Staging happens in a private temp dir and, for
///   [`StagingMode::Separate`], in a root outside `config/resources`.
/// - **Fail closed.** Every path is validated before any I/O. The expected
///   state (delete targets are regular files, write targets are absent or
///   regular files, no component is a symlink, staged copies are intact) is
///   checked before the first write; any mismatch is `transport_failed` with
///   nothing written. Symlinks are never followed and never created.
/// - **Bounded size.** The staged tree obeys the read side's
///   [`READ_MAX_FILE_BYTES`], [`READ_MAX_TOTAL_BYTES`] and
///   [`READ_MAX_ENTRIES`].
/// - **Atomic where the platform allows.** `Separate` moves each staged file
///   into place with a rename (a replace on the same filesystem); `InPlace`
///   writes the target directly and is not atomic per file. A commit is not
///   atomic across files: a failure part-way leaves the files already moved
///   in place, which is why Phase 23 wraps it in a snapshot.
///
/// A [`Staged`] belongs to the mirror that made it and is consumed by its
/// commit in `Separate` mode (the staged files are moved); stage again to
/// commit again.
#[async_trait::async_trait]
pub trait TreeMirror: Send + Sync {
    /// Copy `tree` into staging. Writes nothing into the collection.
    async fn stage(&self, tree: &RenderedTree) -> Result<Staged, CoreError>;

    /// Write the staged files into the mode collection and remove
    /// `delete_set`. Refuses, with nothing written, anything that is not a
    /// valid, in-bounds, expected-state operation.
    async fn commit(
        &self,
        staged: &Staged,
        delete_set: &DeleteSet,
    ) -> Result<MirrorReport, CoreError>;
}

/// Where staged files wait before they are moved into the collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StagingMode {
    /// Staged files are uploaded to `root`, a directory OUTSIDE
    /// `config/resources`, and moved in with a rename at commit.
    Separate {
        /// The staging root, as the transport spells paths (a local path, or
        /// an absolute POSIX path on the host).
        root: String,
    },
    /// Files are written straight into the collection at commit.
    InPlace,
}

/// A rendered tree held in staging, ready to commit.
#[derive(Debug)]
pub struct Staged {
    /// Each staged file with the lowercase hex SHA-256 of its bytes.
    pub files: Vec<(RelPath, String)>,
    /// A private temp dir holding a copy of every staged file. Commit
    /// re-reads and re-verifies it through
    /// [`open_regular_file`](super::tree::open_regular_file).
    pub(crate) local: tempfile::TempDir,
    /// The `Separate` staging root the files were copied to, `None` for
    /// `InPlace`.
    pub(crate) remote_root: Option<String>,
    /// Which mirror (node and mode) staged this.
    pub(crate) owner: String,
}

/// What a commit did, collection-relative and byte-sorted.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct MirrorReport {
    /// Files written (staged files).
    pub written: Vec<String>,
    /// Files removed (the delete set).
    pub deleted: Vec<String>,
    /// Directories removed because a delete emptied them.
    pub removed_dirs: Vec<String>,
}

/// The write half for `node`'s `mode` collection.
///
/// `local` builds the local-filesystem mirror; an `ssh` node (Linux or
/// Windows) builds the sftp mirror over the same hardened shell as
/// [`for_node`]. `mode` is the mode collection name; `core` and `local` are
/// never written.
pub fn mirror_for_node(
    repo: &Path,
    target_file: &str,
    label: &str,
    node: &NodeTarget,
    mode: &str,
    staging: StagingMode,
) -> Result<Box<dyn TreeMirror>, CoreError> {
    let invalid = |detail: String| CoreError::deploy_tree_invalid(target_file, None, None, detail);
    Ok(match node_access(repo, target_file, label, node)? {
        NodeAccess::Local(data_dir) => Box::new(
            local::LocalFsTransport::new(data_dir, label)
                .with_mirror(mode, staging)
                .map_err(|why| invalid(format!("{label}: {why}")))?,
        ),
        NodeAccess::Ssh {
            shell,
            data_dir,
            windows,
        } => Box::new(
            sftp_transport(*shell, data_dir, label, windows)
                .with_mirror(mode, staging)
                .map_err(|why| invalid(format!("{label}: {why}")))?,
        ),
    })
}

/// The apply half ([`ApplyHost`](super::apply::host::ApplyHost)) for `node`'s
/// `mode` collection, for the target `gateway`.
///
/// `local` builds the local-filesystem host. An `ssh` node is refused as
/// `deploy_tree_invalid` naming the node: apply over ssh is not supported by
/// this build yet (23-06 replaces the Linux arm and 23-20 the Windows arm; no
/// verb exposes apply before 23-13, so the refusal is never user-visible).
/// `gateway` and `mode` must be names a run may write under.
pub fn apply_host_for_node(
    repo: &Path,
    target_file: &str,
    label: &str,
    node: &NodeTarget,
    gateway: &str,
    mode: &str,
) -> Result<Box<dyn ApplyHost>, CoreError> {
    let invalid = |detail: String| CoreError::deploy_tree_invalid(target_file, None, None, detail);
    if let Some(why) = mirror_mode_problem(mode) {
        return Err(invalid(format!("{label}: {why}")));
    }
    Ok(match node_access(repo, target_file, label, node)? {
        NodeAccess::Local(data_dir) => Box::new(
            local::LocalFsTransport::new(data_dir, label)
                .with_apply(gateway, mode)
                .map_err(|why| invalid(format!("{label}: {why}")))?,
        ),
        NodeAccess::Ssh { .. } => {
            return Err(invalid(format!(
                "{label}: apply over ssh is not supported by this build"
            )));
        }
    })
}

/// The staging mode Phase 23 uses by default (Decision B(i), D-05): a
/// [`StagingMode::Separate`] root at `ign-deploy/<gateway>/<mode>/staging/<run_id>`
/// below the ledger root of the node's transport, outside every collection:
///
/// - `ssh` nodes (Linux and Windows): below the service account's home, which
///   the server reports (`/C:/Users/svc-ignition-deploy/...` on the Windows
///   capture host). The account can write there and not under the data
///   directory (C6), and the commit is a rename from the home into the mode
///   collection on the same volume.
/// - `local` nodes: below `<data_dir>`.
///
/// `gateway` and `mode` are single path components and `run_id` is
/// `[A-Za-z0-9._-]{1,128}` not starting with a dot, or the call is refused
/// before any I/O. Only an `ssh` node touches the network (one `pwd`).
pub async fn default_staging_mode(
    repo: &Path,
    target_file: &str,
    label: &str,
    node: &NodeTarget,
    gateway: &str,
    mode: &str,
    run_id: &str,
) -> Result<StagingMode, CoreError> {
    let invalid = |detail: String| CoreError::deploy_tree_invalid(target_file, None, None, detail);
    for (what, value) in [("gateway", gateway), ("mode", mode)] {
        if let Some(why) = super::component_problem(value) {
            return Err(invalid(format!("{label}: the staging {what} {why}")));
        }
    }
    let run_ok = (1..=128).contains(&run_id.len())
        && !run_id.starts_with('.')
        && run_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    if !run_ok {
        return Err(invalid(format!(
            "{label}: the run id must be 1 to 128 characters of [A-Za-z0-9._-] and must not start with a dot"
        )));
    }
    let rel = super::ledger::staging_rel(gateway, mode, run_id);
    Ok(match node_access(repo, target_file, label, node)? {
        NodeAccess::Local(data_dir) => {
            let root = data_dir.join(&rel);
            let root = root.to_str().ok_or_else(|| {
                invalid(format!("{label}: the data directory is not valid UTF-8"))
            })?;
            StagingMode::Separate {
                root: root.to_string(),
            }
        }
        NodeAccess::Ssh {
            shell,
            data_dir,
            windows,
        } => {
            sftp_transport(*shell, data_dir, label, windows)
                .default_staging(gateway, mode, run_id)
                .await?
        }
    })
}

/// A mode name that may be written: a plain path component that is not one of
/// the collections a mirror must never touch.
pub(crate) fn mirror_mode_problem(mode: &str) -> Option<String> {
    if let Some(why) = super::component_problem(mode) {
        return Some(format!("the mode {why}"));
    }
    if mode == CORE_COLLECTION || mode == "local" {
        return Some(format!(
            "the mode {mode:?} names a collection a mirror never writes"
        ));
    }
    None
}

/// Why `path` may not be written or deleted by a mirror, or `None`.
///
/// A mirror path is `<module>/<type>/<name>...`: at least three components,
/// no dot-named component (the Designer rejects them), no gateway-owned file
/// name and no control character.
pub(crate) fn mirror_path_problem(path: &RelPath) -> Option<String> {
    let parts: Vec<&str> = path.components().collect();
    if parts.len() < MIRROR_MIN_COMPONENTS {
        return Some(format!(
            "{path} is not a collection path (<module>/<type>/<name>..., at least {MIRROR_MIN_COMPONENTS} components)"
        ));
    }
    for part in parts {
        if part.starts_with('.') {
            return Some(format!("{path} has a dot-named component {part:?}"));
        }
        if GATEWAY_OWNED_FILES.contains(&part) {
            return Some(format!("{path} names the gateway-owned file {part:?}"));
        }
        if part.chars().any(char::is_control) {
            return Some(format!("{path} has a control character in a name"));
        }
    }
    None
}

/// The shallowest path a mirror touches: `<module>/<type>/<name>`.
pub(crate) const MIRROR_MIN_COMPONENTS: usize = 3;

/// Refuse a staged tree and delete set before any I/O: every path valid, no
/// path both staged and deleted, no path a file and a directory of another,
/// bytes and entries within the read side's caps, and a `Separate` staging
/// root that is not inside (or around) `resources_root`.
pub(crate) fn validate_mirror_inputs(
    tree: &RenderedTree,
    delete_set: &DeleteSet,
    staging: &StagingMode,
    resources_root: &str,
) -> Result<(), String> {
    let mut total: u64 = 0;
    for (path, bytes) in &tree.files {
        let len = bytes.len() as u64;
        if len > READ_MAX_FILE_BYTES {
            return Err(format!(
                "{path} is {len} bytes, over the {READ_MAX_FILE_BYTES} byte file cap"
            ));
        }
        total = total.saturating_add(len);
        if total > READ_MAX_TOTAL_BYTES {
            return Err(format!(
                "the staged tree exceeds {READ_MAX_TOTAL_BYTES} bytes in total (at {path})"
            ));
        }
    }
    let staged: Vec<&RelPath> = tree.files.keys().collect();
    validate_mirror_paths(&staged, delete_set)?;
    validate_staging_root(staging, resources_root)
}

/// The path half of [`validate_mirror_inputs`], also run by commit on the
/// paths of a [`Staged`].
pub(crate) fn validate_mirror_paths(
    staged: &[&RelPath],
    delete_set: &DeleteSet,
) -> Result<(), String> {
    for path in staged {
        if let Some(why) = mirror_path_problem(path) {
            return Err(format!("staged path refused: {why}"));
        }
    }
    let mut deleting: BTreeSet<&str> = BTreeSet::new();
    for path in &delete_set.files {
        if let Some(why) = mirror_path_problem(path) {
            return Err(format!("delete-set path refused: {why}"));
        }
        if !deleting.insert(path.as_str()) {
            return Err(format!("delete-set path {path} is listed twice"));
        }
    }
    let staging: BTreeSet<&str> = staged.iter().map(|p| p.as_str()).collect();
    if staging.len() != staged.len() {
        return Err("a path is staged twice".to_string());
    }
    if let Some(both) = deleting.iter().find(|p| staging.contains(**p)) {
        return Err(format!("{both} is both staged and in the delete set"));
    }
    // A path may not be a file and the directory of another path.
    let mut dirs: BTreeSet<&str> = BTreeSet::new();
    for path in staging.iter().chain(deleting.iter()) {
        for (at, _) in path.match_indices('/') {
            dirs.insert(&path[..at]);
        }
    }
    if let Some(clash) = staging
        .iter()
        .chain(deleting.iter())
        .find(|p| dirs.contains(**p))
    {
        return Err(format!(
            "{clash} is a file and also the directory of another path"
        ));
    }
    if staging.len() + deleting.len() + dirs.len() > READ_MAX_ENTRIES {
        return Err(format!(
            "more than {READ_MAX_ENTRIES} entries (files, deletes and directories together)"
        ));
    }
    Ok(())
}

/// The components of an absolute path spelled with `/` or `\`, or why it is
/// not usable as a staging root.
pub(crate) fn absolute_components(text: &str) -> Result<Vec<String>, String> {
    if text.is_empty() || text.chars().any(char::is_control) {
        return Err("must be a non-empty path without control characters".to_string());
    }
    let normal = text.replace('\\', "/");
    let bytes = normal.as_bytes();
    let absolute = normal.starts_with('/')
        || (bytes.len() >= 3
            && bytes[1] == b':'
            && bytes[2] == b'/'
            && bytes[0].is_ascii_alphabetic());
    if !absolute {
        return Err("must be an absolute path".to_string());
    }
    let mut parts = Vec::new();
    for part in normal.split('/') {
        match part {
            "" => {}
            "." | ".." => return Err("must not contain `.` or `..` components".to_string()),
            other => parts.push(other.to_string()),
        }
    }
    Ok(parts)
}

/// Refuse a `Separate` staging root that is relative, too shallow, inside
/// `resources_root` (`config/resources`) or around it. `InPlace` has no root.
pub(crate) fn validate_staging_root(
    staging: &StagingMode,
    resources_root: &str,
) -> Result<(), String> {
    validate_staging_root_for(staging, resources_root, cfg!(windows))
}

/// [`validate_staging_root`] with the comparison's case rule chosen by the
/// HOST the paths name, not by the runner: a Windows OpenSSH host is
/// case-insensitive, so `/c:/program files/.../staging` is inside
/// `/C:/Program Files/.../resources` whatever OS `ign` runs on.
pub(crate) fn validate_staging_root_for(
    staging: &StagingMode,
    resources_root: &str,
    case_insensitive: bool,
) -> Result<(), String> {
    let StagingMode::Separate { root } = staging else {
        return Ok(());
    };
    let staging_parts =
        absolute_components(root).map_err(|why| format!("the staging root {root:?} {why}"))?;
    if staging_parts.len() < 2 {
        return Err(format!(
            "the staging root {root:?} must be at least two directories deep"
        ));
    }
    let resources_parts = absolute_components(resources_root)
        .map_err(|why| format!("the resources root {resources_root:?} {why}"))?;
    let same = |a: &String, b: &String| {
        if case_insensitive {
            a.eq_ignore_ascii_case(b)
        } else {
            a == b
        }
    };
    let starts_with = |long: &[String], short: &[String]| {
        long.len() >= short.len() && long.iter().zip(short).all(|(a, b)| same(a, b))
    };
    if starts_with(&staging_parts, &resources_parts) {
        return Err(format!(
            "the staging root {root:?} is inside config/resources; staging entries there would be scanned by the gateway"
        ));
    }
    if starts_with(&resources_parts, &staging_parts) {
        return Err(format!(
            "the staging root {root:?} contains config/resources"
        ));
    }
    Ok(())
}

/// Copy every file of `tree` into a fresh private temp dir and hash it. The
/// dir is new and ours, so a plain `create_new` write is safe.
pub(crate) fn copy_to_temp(
    tree: &RenderedTree,
) -> Result<(tempfile::TempDir, Vec<(RelPath, String)>), String> {
    use std::io::Write;
    let temp = tempfile::Builder::new()
        .prefix("ign-stage-")
        .tempdir()
        .map_err(|e| format!("cannot create a staging dir ({})", e.kind()))?;
    let mut files = Vec::with_capacity(tree.files.len());
    for (path, bytes) in &tree.files {
        let target = temp.path().join(path.as_str());
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create the staging dir for {path} ({})", e.kind()))?;
        }
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(|e| format!("cannot stage {path} ({})", e.kind()))?;
        file.write_all(bytes)
            .map_err(|e| format!("cannot stage {path} ({})", e.kind()))?;
        files.push((path.clone(), super::tree::sha256_hex(bytes)));
    }
    Ok((temp, files))
}

/// The bytes of the staged copy `rel` below `base`, read through
/// [`open_regular_file`](super::tree::open_regular_file) (descriptor-relative,
/// no symlink followed) and required to be a regular file within the file cap
/// whose SHA-256 is `expect_sha`.
pub(crate) fn read_staged_copy(
    base: &Path,
    rel: &RelPath,
    expect_sha: &str,
) -> Result<Vec<u8>, String> {
    use super::tree::{OpenError, open_regular_file, sha256_hex};
    use std::io::Read;
    let path = base.join(rel.as_str());
    let before = std::fs::symlink_metadata(&path)
        .map_err(|e| format!("the staged copy of {rel} is gone ({})", e.kind()))?;
    if !before.file_type().is_file() {
        return Err(format!("the staged copy of {rel} is not a regular file"));
    }
    let (file, held) =
        open_regular_file(base, Path::new(rel.as_str()), &before).map_err(|e| match e {
            OpenError::Changed => format!("the staged copy of {rel} changed while it was read"),
            OpenError::Io(e) => format!("cannot read the staged copy of {rel} ({})", e.kind()),
        })?;
    if held.len() > READ_MAX_FILE_BYTES {
        return Err(format!(
            "the staged copy of {rel} is over the {READ_MAX_FILE_BYTES} byte file cap"
        ));
    }
    let mut bytes = Vec::with_capacity(held.len() as usize);
    file.take(READ_MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("cannot read the staged copy of {rel} ({})", e.kind()))?;
    if bytes.len() as u64 > READ_MAX_FILE_BYTES || sha256_hex(&bytes) != expect_sha {
        return Err(format!(
            "the staged copy of {rel} changed after it was staged"
        ));
    }
    Ok(bytes)
}
