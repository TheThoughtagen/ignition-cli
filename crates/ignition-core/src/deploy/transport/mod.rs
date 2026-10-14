//! How a verb reads a gateway node's resource collections (Phase 22).
//!
//! A transport reaches the gateway HOST's `data/config/resources/<collection>/`
//! directories. This module defines the **read half** the verbs receive:
//! [`TreeReader`]. The write half (a mirror driven by an explicit delete set)
//! is a separate trait that lands later and is never reachable from a verb,
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
//! [`for_node`] chooses the reader for a target node: `local: { data_dir }`
//! selects [`local::LocalFsTransport`] (D-02).

pub mod local;

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

use super::layout::{RelPath, TAG_TYPES};
use super::manifest::Manifest;
use super::target::NodeTarget;
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
    match (&node.ssh, &node.local) {
        (Some(_), Some(_)) => Err(CoreError::deploy_tree_invalid(
            target_file,
            None,
            None,
            format!("{label}: set ssh or local, not both"),
        )),
        (None, Some(local)) => {
            let configured = Path::new(&local.data_dir);
            let data_dir = if configured.is_absolute() {
                configured.to_path_buf()
            } else {
                repo.join(configured)
            };
            Ok(Box::new(local::LocalFsTransport::new(data_dir, label)))
        }
        (Some(_), None) => Err(CoreError::deploy_tree_invalid(
            target_file,
            None,
            None,
            format!(
                "{label}.ssh: the ssh transport is not available in this build yet; use a local data_dir"
            ),
        )),
        (None, None) => Err(CoreError::deploy_tree_invalid(
            target_file,
            None,
            None,
            format!("{label}: set ssh or local; this verb needs host access"),
        )),
    }
}
