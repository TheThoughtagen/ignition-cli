//! The local-filesystem transport: the gateway data directory is on the
//! runner's own filesystem (D-02, `local: { data_dir }`).
//!
//! Reads `<data_dir>/config/resources/<collection>/...` for the collections
//! `core` and the target's mode (plus a names-only listing of `local`).
//! Every entry is inspected with `symlink_metadata` and never followed;
//! symlinks, special files and non-UTF-8 names anywhere under an opened
//! directory are refused, and the three caps in the parent module are
//! enforced while walking. Only an ABSENT collection directory reads as
//! empty; every other failure is `transport_failed` naming the node and the
//! path relative to `data_dir`, never file content.
//!
//! The write half ([`TreeMirror`]) is implemented below, bound to
//! `<data_dir>/config/resources/<mode>`. It never follows a symlink, creates
//! no temp or dot-named entry inside the collection, checks the whole
//! expected state before its first write and touches only the staged files
//! and the delete set (see the trait's contract).
//!
//! On unix every mutation (staging-root writes, collection directories, file
//! writes, renames, unlinks, rmdirs) is descriptor-relative: a directory
//! handle chain is opened with `O_DIRECTORY | O_NOFOLLOW` per component (see
//! `tree::dirfd`), so a directory swapped for a symlink after the preflight
//! fails the open and the commit stops. The preflight stays path-based and
//! advisory. Windows keeps the path-based mutations, a documented residual.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use super::{
    CORE_COLLECTION, DeleteSet, FileBlob, MirrorReport, ObservedFiles, READ_MAX_ENTRIES,
    READ_MAX_FILE_BYTES, READ_MAX_TOTAL_BYTES, Scope, Staged, StagingMode, TreeMirror, TreeReader,
    copy_to_temp, mirror_mode_problem, read_staged_copy, validate_mirror_inputs,
    validate_mirror_paths, validate_staging_root,
};
use crate::deploy::component_problem;
use crate::deploy::layout::{RelPath, TAG_TYPES};
use crate::deploy::ledger::{LEDGER_MAX_BYTES, ledger_location};
use crate::deploy::plan::unit_key;
#[cfg(unix)]
use crate::deploy::tree::dirfd::{self, FdError, Kind};
use crate::deploy::tree::{OpenError, RenderedTree, open_regular_file, sha256_hex, walk_capped};
use crate::error::CoreError;

/// The collection that overrides the mode collection on the gateway.
const LOCAL_COLLECTION: &str = "local";

/// Reads a gateway data directory on the runner's own filesystem.
#[derive(Debug, Clone)]
pub struct LocalFsTransport {
    data_dir: PathBuf,
    label: String,
    mirror: Option<MirrorSetup>,
}

/// What the write half needs: the mode collection and the staging mode.
#[derive(Debug, Clone)]
struct MirrorSetup {
    mode: String,
    staging: StagingMode,
}

/// What a commit's preflight established, handed to its apply phase.
struct Prepared {
    #[cfg_attr(unix, allow(dead_code))]
    collection: PathBuf,
    #[cfg_attr(unix, allow(dead_code))]
    collection_exists: bool,
    /// The directory the staged copies are read from (staging root, or the
    /// private temp dir for `InPlace`).
    source: PathBuf,
    /// The delete set, byte-sorted.
    deletes: Vec<RelPath>,
}

/// Running totals of one read, shared by every walk in it.
#[derive(Default)]
struct Budget {
    entries: usize,
    bytes: u64,
}

impl LocalFsTransport {
    /// A reader over `data_dir`; `label` (e.g. `nodes.master`) names the node
    /// in errors.
    pub fn new(data_dir: PathBuf, label: impl Into<String>) -> Self {
        LocalFsTransport {
            data_dir,
            label: label.into(),
            mirror: None,
        }
    }

    /// Bind the write half to the `mode` collection with `staging`. Fails
    /// when `mode` is not a writable collection name or the staging root is
    /// unusable, so a misconfigured mirror cannot be built.
    pub fn with_mirror(mut self, mode: &str, staging: StagingMode) -> Result<Self, String> {
        if let Some(why) = mirror_mode_problem(mode) {
            return Err(why);
        }
        validate_staging_root(&staging, &self.resources_display())?;
        self.mirror = Some(MirrorSetup {
            mode: mode.to_string(),
            staging,
        });
        Ok(self)
    }

    /// `<data_dir>/config/resources` spelled with `/`, for the staging-root
    /// containment check.
    fn resources_display(&self) -> String {
        let mut text = self
            .data_dir
            .join("config")
            .join("resources")
            .display()
            .to_string();
        if cfg!(windows) {
            text = text.replace('\\', "/");
        }
        text
    }

    fn fail(&self, detail: impl std::fmt::Display) -> CoreError {
        CoreError::TransportFailed {
            operation: "read".to_string(),
            node: self.label.clone(),
            detail: detail.to_string(),
        }
    }

    /// Require `data_dir` to be a real directory (not a symlink).
    fn require_data_dir(&self) -> Result<(), CoreError> {
        let meta = std::fs::symlink_metadata(&self.data_dir).map_err(|e| {
            self.fail(format!(
                "data_dir {} cannot be read ({})",
                self.data_dir.display(),
                e.kind()
            ))
        })?;
        if meta.file_type().is_symlink() || !meta.file_type().is_dir() {
            return Err(self.fail(format!(
                "data_dir {} is not a real directory (symlinks are refused)",
                self.data_dir.display()
            )));
        }
        Ok(())
    }

    /// `<data_dir>/config/resources`, after requiring `data_dir` and it to be
    /// real directories.
    fn resources_root(&self) -> Result<PathBuf, CoreError> {
        self.require_data_dir()?;
        let root = self.data_dir.join("config").join("resources");
        match std::fs::symlink_metadata(&root) {
            Ok(meta) if meta.file_type().is_dir() => Ok(root),
            Ok(_) => Err(self.fail("config/resources is not a real directory")),
            Err(e) => Err(self.fail(format!(
                "config/resources cannot be read ({}); is data_dir a gateway data directory?",
                e.kind()
            ))),
        }
    }

    /// Descend `parts` below `base`, checking every component with
    /// `symlink_metadata`. `Ok(None)` when a component is absent.
    fn probe(
        &self,
        base: &Path,
        base_rel: &str,
        parts: &[&str],
    ) -> Result<Option<PathBuf>, CoreError> {
        let mut path = base.to_path_buf();
        let mut rel = base_rel.to_string();
        for part in parts {
            path.push(part);
            rel.push('/');
            rel.push_str(part);
            match std::fs::symlink_metadata(&path) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    return Err(self.fail(format!("{rel} is a symlink; symlinks are refused")));
                }
                Ok(meta) if !meta.file_type().is_dir() => {
                    return Err(self.fail(format!("{rel} is not a directory")));
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => {
                    return Err(self.fail(format!("cannot inspect {rel} ({})", e.kind())));
                }
            }
        }
        Ok(Some(path))
    }

    /// The directories of `scope`, as component lists below a collection.
    fn scope_dirs(scope: &Scope) -> Vec<Vec<&str>> {
        let mut dirs: Vec<Vec<&str>> = scope
            .types
            .iter()
            .map(|(module, ty)| vec![module.as_str(), ty.as_str()])
            .collect();
        for provider in &scope.tag_providers {
            for tag_type in TAG_TYPES {
                dirs.push(vec!["ignition", tag_type, provider.as_str()]);
            }
        }
        dirs
    }

    /// Every file of `scope` in `collection`: collection-relative path to
    /// filesystem path. Absent directories contribute nothing.
    fn collect_paths(
        &self,
        resources: &Path,
        collection: &str,
        scope: &Scope,
        budget: &mut Budget,
    ) -> Result<BTreeMap<RelPath, PathBuf>, CoreError> {
        let base_rel = "config/resources";
        let mut found: BTreeMap<RelPath, PathBuf> = BTreeMap::new();
        for parts in Self::scope_dirs(scope) {
            let mut chain: Vec<&str> = vec![collection];
            chain.extend(&parts);
            let Some(dir) = self.probe(resources, base_rel, &chain)? else {
                continue;
            };
            let dir_rel = format!("{base_rel}/{collection}/{}", parts.join("/"));
            let mut files: BTreeMap<String, PathBuf> = BTreeMap::new();
            let mut dirs: BTreeSet<String> = BTreeSet::new();
            let remaining = READ_MAX_ENTRIES.saturating_sub(budget.entries);
            walk_capped(&dir, &dir_rel, &mut files, &mut dirs, remaining)
                .map_err(|why| self.fail(why))?;
            budget.entries += files.len() + dirs.len();
            if budget.entries > READ_MAX_ENTRIES {
                return Err(self.fail(format!(
                    "more than {READ_MAX_ENTRIES} entries under config/resources"
                )));
            }
            let strip = format!("{base_rel}/{collection}/");
            for (full, path) in files {
                let key = full.strip_prefix(&strip).unwrap_or(&full);
                let key = RelPath::new(key)
                    .map_err(|why| self.fail(format!("{full} has an unusable name ({why})")))?;
                found.insert(key, path);
            }
        }
        Ok(found)
    }

    /// Read and hash the files of `paths`, enforcing the byte caps.
    fn read_blobs(
        &self,
        collection: &str,
        paths: BTreeMap<RelPath, PathBuf>,
        budget: &mut Budget,
    ) -> Result<BTreeMap<RelPath, FileBlob>, CoreError> {
        let mut blobs = BTreeMap::new();
        for (key, path) in paths {
            let shown = format!("config/resources/{collection}/{key}");
            let before = std::fs::symlink_metadata(&path)
                .map_err(|e| self.fail(format!("cannot inspect {shown} ({})", e.kind())))?;
            if !before.file_type().is_file() {
                return Err(self.fail(format!("{shown} is no longer a regular file")));
            }
            // Open first, then validate the handle: a file swapped for a
            // symlink after the check above is not read.
            let (file, held) = self.open_checked(&path, &before, &shown)?;
            let len = held.len();
            if len > READ_MAX_FILE_BYTES {
                return Err(self.fail(format!(
                    "{shown} is {len} bytes, over the {READ_MAX_FILE_BYTES} byte file cap"
                )));
            }
            budget.bytes += len;
            if budget.bytes > READ_MAX_TOTAL_BYTES {
                return Err(self.fail(format!(
                    "the files under config/resources exceed {READ_MAX_TOTAL_BYTES} bytes in total (at {shown})"
                )));
            }
            let mut bytes = Vec::with_capacity(len as usize);
            file.take(READ_MAX_FILE_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| self.fail(format!("cannot read {shown} ({})", e.kind())))?;
            if bytes.len() as u64 > READ_MAX_FILE_BYTES {
                return Err(self.fail(format!(
                    "{shown} grew past the {READ_MAX_FILE_BYTES} byte file cap while it was read"
                )));
            }
            let sha256 = sha256_hex(&bytes);
            blobs.insert(key, FileBlob { bytes, sha256 });
        }
        Ok(blobs)
    }

    /// Open `path` (a file below `data_dir`) descriptor-relative from
    /// `data_dir` and validate the handle against `before`: no component of
    /// the path is followed if it has become a symlink since the walk.
    fn open_checked(
        &self,
        path: &Path,
        before: &std::fs::Metadata,
        shown: &str,
    ) -> Result<(std::fs::File, std::fs::Metadata), CoreError> {
        let Ok(rel) = path.strip_prefix(&self.data_dir) else {
            return Err(self.fail(format!("{shown} is outside data_dir")));
        };
        open_regular_file(&self.data_dir, rel, before).map_err(|e| match e {
            OpenError::Changed => self.fail(format!("{shown} changed while it was read")),
            OpenError::Io(e) => self.fail(format!("cannot read {shown} ({})", e.kind())),
        })
    }

    fn read_blocking(&self, scope: &Scope) -> Result<ObservedFiles, CoreError> {
        let resources = self.resources_root()?;
        let mut budget = Budget::default();
        let core_paths = self.collect_paths(&resources, CORE_COLLECTION, scope, &mut budget)?;
        let mode_paths = self.collect_paths(&resources, &scope.mode, scope, &mut budget)?;
        let local_paths = self.collect_paths(&resources, LOCAL_COLLECTION, scope, &mut budget)?;
        let core = self.read_blobs(CORE_COLLECTION, core_paths, &mut budget)?;
        let mode = self.read_blobs(&scope.mode, mode_paths, &mut budget)?;

        let layer: BTreeMap<RelPath, ()> = local_paths.keys().map(|k| (k.clone(), ())).collect();
        let local_units: BTreeSet<String> = layer.keys().map(|k| unit_key(k, &layer)).collect();
        Ok(ObservedFiles {
            core,
            mode,
            local_units: local_units.into_iter().collect(),
        })
    }

    /// The ledger bytes, or `None` when any level of its location is absent.
    /// Every level is inspected with `symlink_metadata` and a symlink or a
    /// non-regular file is refused; the bytes are read through
    /// [`open_regular_file`], descriptor-relative from `data_dir`, at most
    /// [`LEDGER_MAX_BYTES`] of them.
    fn read_ledger_blocking(
        &self,
        gateway: &str,
        mode: &str,
    ) -> Result<Option<Vec<u8>>, CoreError> {
        let location = ledger_location(gateway, mode);
        let bad = |problem: String| CoreError::DeployLedgerInvalid {
            location: location.clone(),
            problem,
        };
        for (what, value) in [("gateway", gateway), ("mode", mode)] {
            if let Some(why) = component_problem(value) {
                return Err(bad(format!("the target {what} {why}")));
            }
        }
        // An unreadable host is not an absent ledger.
        self.require_data_dir()?;

        let parts: Vec<&str> = location.split('/').collect();
        let mut path = self.data_dir.clone();
        let mut meta = None;
        for (index, part) in parts.iter().enumerate() {
            path.push(part);
            let last = index + 1 == parts.len();
            match std::fs::symlink_metadata(&path) {
                Ok(m) if m.file_type().is_symlink() => {
                    return Err(bad(
                        "a path component is a symlink; symlinks are refused".to_string()
                    ));
                }
                Ok(m) if last && !m.file_type().is_file() => {
                    return Err(bad("the ledger is not a regular file".to_string()));
                }
                Ok(m) if !last && !m.file_type().is_dir() => {
                    return Err(bad("a path component is not a directory".to_string()));
                }
                Ok(m) => meta = Some(m),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(bad(format!("cannot be inspected ({})", e.kind()))),
            }
        }
        let Some(before) = meta else {
            return Ok(None);
        };
        let (file, held) = match open_regular_file(&self.data_dir, Path::new(&location), &before) {
            Ok(opened) => opened,
            Err(OpenError::Changed) => {
                return Err(bad(
                    "changed while it was read; symlinks are refused".to_string()
                ));
            }
            Err(OpenError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(OpenError::Io(e)) => return Err(bad(format!("cannot be read ({})", e.kind()))),
        };
        if held.len() > LEDGER_MAX_BYTES as u64 {
            return Err(bad(format!(
                "the file is {} bytes, over the {LEDGER_MAX_BYTES} byte cap",
                held.len()
            )));
        }
        let mut bytes = Vec::new();
        file.take(LEDGER_MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| bad(format!("cannot be read ({})", e.kind())))?;
        if bytes.len() > LEDGER_MAX_BYTES {
            return Err(bad(format!(
                "the file grew past the {LEDGER_MAX_BYTES} byte cap while it was read"
            )));
        }
        Ok(Some(bytes))
    }

    fn list_types_blocking(&self, collection: &str) -> Result<Vec<(String, String)>, CoreError> {
        let resources = self.resources_root()?;
        let base_rel = "config/resources";
        let Some(coll_dir) = self.probe(&resources, base_rel, &[collection])? else {
            return Ok(Vec::new());
        };
        let coll_rel = format!("{base_rel}/{collection}");
        let mut pairs = Vec::new();
        let mut seen = 0;
        for module in self.child_dirs(&coll_dir, &coll_rel, &mut seen, READ_MAX_ENTRIES)? {
            let module_dir = coll_dir.join(&module);
            let module_rel = format!("{coll_rel}/{module}");
            for ty in self.child_dirs(&module_dir, &module_rel, &mut seen, READ_MAX_ENTRIES)? {
                pairs.push((module.clone(), ty));
            }
        }
        pairs.sort_by(|a, b| {
            (a.0.as_bytes(), a.1.as_bytes()).cmp(&(b.0.as_bytes(), b.1.as_bytes()))
        });
        Ok(pairs)
    }

    /// Names of the sub-directories of `dir`; plain files are ignored, a
    /// symlink or a non-UTF-8 name is refused. Every entry read, files
    /// included, counts against `seen`, shared across one listing, and the
    /// listing stops as soon as `seen` passes `max_entries`.
    fn child_dirs(
        &self,
        dir: &Path,
        rel: &str,
        seen: &mut usize,
        max_entries: usize,
    ) -> Result<Vec<String>, CoreError> {
        let read = std::fs::read_dir(dir)
            .map_err(|e| self.fail(format!("cannot read directory {rel} ({})", e.kind())))?;
        let mut names = Vec::new();
        for entry in read {
            let entry = entry
                .map_err(|e| self.fail(format!("cannot read an entry of {rel} ({})", e.kind())))?;
            *seen += 1;
            if *seen > max_entries {
                return Err(self.fail(format!("more than {max_entries} entries (at {rel})")));
            }
            let name = entry.file_name().into_string().map_err(|raw| {
                self.fail(format!(
                    "{rel}/{} has a name that is not valid UTF-8",
                    raw.to_string_lossy()
                ))
            })?;
            let meta = std::fs::symlink_metadata(entry.path())
                .map_err(|e| self.fail(format!("cannot inspect {rel}/{name} ({})", e.kind())))?;
            if meta.file_type().is_symlink() {
                return Err(self.fail(format!("{rel}/{name} is a symlink; symlinks are refused")));
            }
            if meta.file_type().is_dir() {
                names.push(name);
            }
        }
        names.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        Ok(names)
    }
}

#[async_trait::async_trait]
impl TreeReader for LocalFsTransport {
    async fn list_types(&self, collection: &str) -> Result<Vec<(String, String)>, CoreError> {
        let this = self.clone();
        let collection = collection.to_string();
        tokio::task::spawn_blocking(move || this.list_types_blocking(&collection))
            .await
            .map_err(|e| CoreError::Internal(format!("local read task failed: {e}")))?
    }

    async fn read(&self, scope: &Scope) -> Result<ObservedFiles, CoreError> {
        let this = self.clone();
        let scope = scope.clone();
        tokio::task::spawn_blocking(move || this.read_blocking(&scope))
            .await
            .map_err(|e| CoreError::Internal(format!("local read task failed: {e}")))?
    }

    async fn read_ledger(&self, gateway: &str, mode: &str) -> Result<Option<Vec<u8>>, CoreError> {
        let this = self.clone();
        let (gateway, mode) = (gateway.to_string(), mode.to_string());
        tokio::task::spawn_blocking(move || this.read_ledger_blocking(&gateway, &mode))
            .await
            .map_err(|e| CoreError::Internal(format!("local read task failed: {e}")))?
    }
}

// ---------------------------------------------------------------------------
// The write half (22-07)
// ---------------------------------------------------------------------------

/// What `symlink_metadata` says a path is. A symlink is never followed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Node {
    Missing,
    Dir,
    File,
    Symlink,
    Other,
}

fn node_of(path: &Path) -> std::io::Result<Node> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            let kind = meta.file_type();
            Ok(if kind.is_symlink() {
                Node::Symlink
            } else if kind.is_dir() {
                Node::Dir
            } else if kind.is_file() {
                Node::File
            } else {
                Node::Other
            })
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Node::Missing),
        Err(e) => Err(e),
    }
}

/// The canonical form of `path`: its deepest existing ancestor resolved
/// (symlinks and all), the missing tail appended. Used only to compare where
/// two paths really are.
fn canonical_with_tail(path: &Path) -> PathBuf {
    let mut tail = Vec::new();
    let mut current = path.to_path_buf();
    loop {
        if let Ok(resolved) = std::fs::canonicalize(&current) {
            let mut out = resolved;
            for part in tail.iter().rev() {
                out.push(part);
            }
            return out;
        }
        match (current.parent(), current.file_name()) {
            (Some(parent), Some(name)) => {
                tail.push(name.to_owned());
                current = parent.to_path_buf();
            }
            _ => return path.to_path_buf(),
        }
    }
}

impl LocalFsTransport {
    fn mirror_fail(&self, op: &str, detail: impl std::fmt::Display) -> CoreError {
        CoreError::TransportFailed {
            operation: op.to_string(),
            node: self.label.clone(),
            detail: detail.to_string(),
        }
    }

    fn mirror_setup(&self, op: &str) -> Result<&MirrorSetup, CoreError> {
        self.mirror.as_ref().ok_or_else(|| {
            self.mirror_fail(
                op,
                "this transport has no write half; build it with mirror_for_node",
            )
        })
    }

    /// Which node and mode a [`Staged`] belongs to.
    fn mirror_owner(&self, setup: &MirrorSetup) -> String {
        format!("local:{}:{}", self.data_dir.display(), setup.mode)
    }

    /// `<data_dir>/config/resources`, after requiring `data_dir`, `config`
    /// and `resources` to be real directories (no symlinks).
    fn mirror_resources(&self, op: &str) -> Result<PathBuf, CoreError> {
        let mut path = self.data_dir.clone();
        for (shown, part) in [
            ("data_dir", None),
            ("config", Some("config")),
            ("config/resources", Some("resources")),
        ] {
            if let Some(part) = part {
                path.push(part);
            }
            match node_of(&path) {
                Ok(Node::Dir) => {}
                Ok(Node::Symlink) => {
                    return Err(
                        self.mirror_fail(op, format!("{shown} is a symlink; symlinks are refused"))
                    );
                }
                Ok(Node::Missing) => {
                    return Err(self.mirror_fail(
                        op,
                        format!("{shown} does not exist; is data_dir a gateway data directory?"),
                    ));
                }
                Ok(_) => {
                    return Err(self.mirror_fail(op, format!("{shown} is not a directory")));
                }
                Err(e) => {
                    return Err(
                        self.mirror_fail(op, format!("{shown} cannot be inspected ({})", e.kind()))
                    );
                }
            }
        }
        Ok(path)
    }

    /// Where the staging root REALLY is must not be inside or around
    /// `config/resources`, even through a symlinked ancestor.
    fn check_staging_real(&self, op: &str, root: &str, resources: &Path) -> Result<(), CoreError> {
        let real_root = canonical_with_tail(Path::new(root));
        let real_resources = canonical_with_tail(resources);
        if real_root.starts_with(&real_resources) || real_resources.starts_with(&real_root) {
            return Err(self.mirror_fail(
                op,
                format!(
                    "the staging root {root:?} resolves to {} which overlaps config/resources",
                    real_root.display()
                ),
            ));
        }
        Ok(())
    }

    /// Walk `parts` below `base`, creating each missing directory with
    /// `create_dir`; an existing component must be a real directory.
    #[cfg(not(unix))]
    fn ensure_dir_chain(
        &self,
        op: &str,
        base: &Path,
        parts: &[&str],
    ) -> Result<PathBuf, CoreError> {
        let mut path = base.to_path_buf();
        for part in parts {
            path.push(part);
            match node_of(&path) {
                Ok(Node::Dir) => {}
                Ok(Node::Missing) => std::fs::create_dir(&path).map_err(|e| {
                    self.mirror_fail(
                        op,
                        format!("cannot create {} ({})", self.shown(&path), e.kind()),
                    )
                })?,
                Ok(Node::Symlink) => {
                    return Err(self.mirror_fail(
                        op,
                        format!("{} is a symlink; symlinks are refused", self.shown(&path)),
                    ));
                }
                Ok(_) => {
                    return Err(
                        self.mirror_fail(op, format!("{} is not a directory", self.shown(&path)))
                    );
                }
                Err(e) => {
                    return Err(self.mirror_fail(
                        op,
                        format!("cannot inspect {} ({})", self.shown(&path), e.kind()),
                    ));
                }
            }
        }
        Ok(path)
    }

    /// `path` for a message: below `data_dir` when it is, else as given.
    fn shown(&self, path: &Path) -> String {
        path.strip_prefix(&self.data_dir)
            .unwrap_or(path)
            .display()
            .to_string()
    }

    /// What `rel` is below `base`: every component above the last must be a
    /// real directory (a symlink or file there is refused), an absent
    /// component makes the whole path `Missing`.
    fn inspect(&self, op: &str, base: &Path, rel: &RelPath) -> Result<Node, CoreError> {
        let parts: Vec<&str> = rel.components().collect();
        let mut path = base.to_path_buf();
        for (index, part) in parts.iter().enumerate() {
            path.push(part);
            let last = index + 1 == parts.len();
            match node_of(&path) {
                Ok(Node::Missing) => return Ok(Node::Missing),
                Ok(Node::Symlink) => {
                    return Err(self.mirror_fail(
                        op,
                        format!("{} is a symlink; symlinks are refused", self.shown(&path)),
                    ));
                }
                Ok(node) if last => return Ok(node),
                Ok(Node::Dir) => {}
                Ok(_) => {
                    return Err(
                        self.mirror_fail(op, format!("{} is not a directory", self.shown(&path)))
                    );
                }
                Err(e) => {
                    return Err(self.mirror_fail(
                        op,
                        format!("cannot inspect {} ({})", self.shown(&path), e.kind()),
                    ));
                }
            }
        }
        Ok(Node::Missing)
    }

    /// A newly created regular file: never replaces, never follows a link.
    #[cfg(not(unix))]
    fn write_new(&self, op: &str, path: &Path, bytes: &[u8]) -> Result<(), CoreError> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| {
                self.mirror_fail(
                    op,
                    format!("cannot create {} ({})", self.shown(path), e.kind()),
                )
            })?;
        file.write_all(bytes).map_err(|e| {
            self.mirror_fail(
                op,
                format!("cannot write {} ({})", self.shown(path), e.kind()),
            )
        })
    }

    /// Overwrite (or create) `path` in place without following a symlink at
    /// the final component.
    #[cfg(not(unix))]
    fn write_in_place(&self, op: &str, path: &Path, bytes: &[u8]) -> Result<(), CoreError> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
        }
        let mut file = options.open(path).map_err(|e| {
            self.mirror_fail(
                op,
                format!(
                    "cannot open {} for writing ({})",
                    self.shown(path),
                    e.kind()
                ),
            )
        })?;
        file.write_all(bytes).map_err(|e| {
            self.mirror_fail(
                op,
                format!("cannot write {} ({})", self.shown(path), e.kind()),
            )
        })
    }

    /// The bytes of the staged copy `rel` below `base`, verified against
    /// `expect_sha` (see [`read_staged_copy`]).
    fn read_staged(
        &self,
        op: &str,
        base: &Path,
        rel: &RelPath,
        expect_sha: &str,
    ) -> Result<Vec<u8>, CoreError> {
        read_staged_copy(base, rel, expect_sha).map_err(|why| self.mirror_fail(op, why))
    }

    /// Map a descriptor-relative refusal to a `transport_failed`.
    #[cfg(unix)]
    fn fd_fail(&self, op: &str, what: &str, e: FdError) -> CoreError {
        let detail = match e {
            FdError::Changed => format!(
                "{what} changed while it was written (a directory on the way is, or became, a symlink or is not a directory); refused, nothing outside it was touched"
            ),
            FdError::Missing => {
                format!("{what} no longer exists; the tree changed while it was written")
            }
            FdError::Exists => format!("{what} already exists"),
            FdError::NotEmpty => format!("{what} is not empty"),
            FdError::Io(e) => format!("cannot access {what} ({})", e.kind()),
        };
        self.mirror_fail(op, detail)
    }

    /// Copy the rendered tree into the staging root, every create and unlink
    /// descriptor-relative from the root's own handle: a directory swapped for
    /// a symlink after the check cannot redirect a write out of the root.
    #[cfg(unix)]
    fn fill_staging_root(
        &self,
        op: &str,
        root_path: &Path,
        tree: &RenderedTree,
    ) -> Result<(), CoreError> {
        let root =
            dirfd::open_root(root_path).map_err(|e| self.fd_fail(op, "the staging root", e))?;
        for (path, bytes) in &tree.files {
            let parts: Vec<&str> = path.components().collect();
            let Some((name, parents)) = parts.split_last() else {
                continue;
            };
            let what = format!("{path} in the staging root");
            let dir = dirfd::ensure_walk(&root, parents).map_err(|e| self.fd_fail(op, &what, e))?;
            match dirfd::kind_at(&dir, name).map_err(|e| self.fd_fail(op, &what, e))? {
                Kind::Missing => {}
                Kind::File => dirfd::unlink_at(&dir, name)
                    .map_err(|e| self.fd_fail(op, &format!("a stale staged file {path}"), e))?,
                _ => {
                    return Err(self.mirror_fail(
                        op,
                        format!("{path} exists in the staging root and is not a regular file"),
                    ));
                }
            }
            let mut file = dirfd::create_new(&dir, name).map_err(|e| self.fd_fail(op, &what, e))?;
            file.write_all(bytes)
                .map_err(|e| self.mirror_fail(op, format!("cannot write {what} ({})", e.kind())))?;
        }
        Ok(())
    }

    /// Copy the rendered tree into the staging root (path-based; windows has no
    /// descriptor-relative primitives in std, a documented residual).
    #[cfg(not(unix))]
    fn fill_staging_root(
        &self,
        op: &str,
        root_path: &Path,
        tree: &RenderedTree,
    ) -> Result<(), CoreError> {
        for (path, bytes) in &tree.files {
            let parts: Vec<&str> = path.components().collect();
            let Some((name, parents)) = parts.split_last() else {
                continue;
            };
            let dir = self.ensure_dir_chain(op, root_path, parents)?;
            let target = dir.join(name);
            match node_of(&target) {
                Ok(Node::Missing) => {}
                Ok(Node::File) => std::fs::remove_file(&target).map_err(|e| {
                    self.mirror_fail(
                        op,
                        format!("cannot replace a stale staged file ({})", e.kind()),
                    )
                })?,
                Ok(_) => {
                    return Err(self.mirror_fail(
                        op,
                        format!(
                            "{} exists in the staging root and is not a regular file",
                            path
                        ),
                    ));
                }
                Err(e) => {
                    return Err(self.mirror_fail(
                        op,
                        format!("cannot inspect {path} in the staging root ({})", e.kind()),
                    ));
                }
            }
            self.write_new(op, &target, bytes)?;
        }
        Ok(())
    }

    fn stage_blocking(&self, tree: &RenderedTree) -> Result<Staged, CoreError> {
        const OP: &str = "stage";
        let setup = self.mirror_setup(OP)?;
        validate_mirror_inputs(
            tree,
            &DeleteSet::default(),
            &setup.staging,
            &self.resources_display(),
        )
        .map_err(|why| self.mirror_fail(OP, why))?;
        let resources = self.mirror_resources(OP)?;
        let remote_root = match &setup.staging {
            StagingMode::Separate { root } => {
                self.check_staging_real(OP, root, &resources)?;
                Some(root.clone())
            }
            StagingMode::InPlace => None,
        };

        let (local, files) = copy_to_temp(tree).map_err(|why| self.mirror_fail(OP, why))?;

        if let Some(root) = &remote_root {
            let root_path = PathBuf::from(root);
            match node_of(&root_path) {
                Ok(Node::Dir) => {}
                Ok(Node::Missing) => std::fs::create_dir_all(&root_path).map_err(|e| {
                    self.mirror_fail(
                        OP,
                        format!("cannot create the staging root {root:?} ({})", e.kind()),
                    )
                })?,
                Ok(_) => {
                    return Err(self.mirror_fail(
                        OP,
                        format!("the staging root {root:?} is not a real directory"),
                    ));
                }
                Err(e) => {
                    return Err(self.mirror_fail(
                        OP,
                        format!(
                            "the staging root {root:?} cannot be inspected ({})",
                            e.kind()
                        ),
                    ));
                }
            }
            if node_of(&root_path).ok() != Some(Node::Dir) {
                return Err(self.mirror_fail(
                    OP,
                    format!("the staging root {root:?} is not a real directory"),
                ));
            }
            self.fill_staging_root(OP, &root_path, tree)?;
        }
        Ok(Staged {
            files,
            local,
            remote_root,
            owner: self.mirror_owner(setup),
        })
    }

    fn commit_blocking(
        &self,
        local: &Path,
        files: &[(RelPath, String)],
        remote_root: Option<&str>,
        owner: &str,
        delete_set: &DeleteSet,
    ) -> Result<MirrorReport, CoreError> {
        let prepared = self.commit_preflight(local, files, remote_root, owner, delete_set)?;
        self.commit_apply(&prepared, files, remote_root)
    }

    /// Everything that can be refused before the first write: ownership,
    /// path validation, and the whole expected state of the collection and of
    /// the staged sources. Advisory for the apply phase, which re-checks every
    /// component as it goes (a stale answer here is exactly the TOCTOU window
    /// the descriptor-relative apply closes).
    fn commit_preflight(
        &self,
        local: &Path,
        files: &[(RelPath, String)],
        remote_root: Option<&str>,
        owner: &str,
        delete_set: &DeleteSet,
    ) -> Result<Prepared, CoreError> {
        const OP: &str = "commit";
        let setup = self.mirror_setup(OP)?;
        if owner != self.mirror_owner(setup) {
            return Err(self.mirror_fail(
                OP,
                "these files were staged by a different mirror (another node or mode)",
            ));
        }
        let expected_root = match &setup.staging {
            StagingMode::Separate { root } => Some(root.as_str()),
            StagingMode::InPlace => None,
        };
        if remote_root != expected_root {
            return Err(self.mirror_fail(OP, "these files were staged in a different staging mode"));
        }
        let staged_paths: Vec<&RelPath> = files.iter().map(|(path, _)| path).collect();
        validate_mirror_paths(&staged_paths, delete_set)
            .and_then(|()| validate_staging_root(&setup.staging, &self.resources_display()))
            .map_err(|why| self.mirror_fail(OP, why))?;

        // ---- expected state: everything is checked before the first write.
        let resources = self.mirror_resources(OP)?;
        if let Some(root) = remote_root {
            self.check_staging_real(OP, root, &resources)?;
        }
        let collection = resources.join(&setup.mode);
        let collection_exists = match node_of(&collection) {
            Ok(Node::Dir) => true,
            Ok(Node::Missing) => false,
            Ok(Node::Symlink) => {
                return Err(self.mirror_fail(
                    OP,
                    format!(
                        "the {} collection is a symlink; symlinks are refused",
                        setup.mode
                    ),
                ));
            }
            Ok(_) => {
                return Err(self.mirror_fail(
                    OP,
                    format!("the {} collection is not a directory", setup.mode),
                ));
            }
            Err(e) => {
                return Err(self.mirror_fail(
                    OP,
                    format!(
                        "the {} collection cannot be inspected ({})",
                        setup.mode,
                        e.kind()
                    ),
                ));
            }
        };

        let mut deletes: Vec<&RelPath> = delete_set.files.iter().collect();
        deletes.sort();
        for path in &deletes {
            let node = if collection_exists {
                self.inspect(OP, &collection, path)?
            } else {
                Node::Missing
            };
            if node != Node::File {
                return Err(self.mirror_fail(
                    OP,
                    format!("{path} is not a regular file in the {} collection ({node:?}); nothing was changed", setup.mode),
                ));
            }
        }
        for (path, _) in files {
            let node = if collection_exists {
                self.inspect(OP, &collection, path)?
            } else {
                Node::Missing
            };
            if !matches!(node, Node::Missing | Node::File) {
                return Err(self.mirror_fail(
                    OP,
                    format!("{path} exists in the {} collection and is not a regular file ({node:?}); nothing was changed", setup.mode),
                ));
            }
        }
        let source = remote_root.map_or_else(|| local.to_path_buf(), PathBuf::from);
        for (path, sha) in files {
            self.read_staged(OP, &source, path, sha)?;
        }
        Ok(Prepared {
            collection,
            collection_exists,
            source,
            deletes: deletes.into_iter().cloned().collect(),
        })
    }

    /// The mutations, all descriptor-relative (unix): a directory handle chain
    /// is opened from `data_dir` (`config`, `resources`, the mode collection),
    /// and every create, rename, unlink and rmdir is `*at` on a handle opened
    /// with `O_DIRECTORY | O_NOFOLLOW` per component. A component swapped for a
    /// symlink after the preflight fails the open and the commit stops; nothing
    /// outside the opened directories is ever named. The staged file is
    /// re-read and re-hashed through its handle immediately before `renameat`.
    #[cfg(unix)]
    fn commit_apply(
        &self,
        prepared: &Prepared,
        files: &[(RelPath, String)],
        remote_root: Option<&str>,
    ) -> Result<MirrorReport, CoreError> {
        const OP: &str = "commit";
        let setup = self.mirror_setup(OP)?;
        let Prepared {
            source, deletes, ..
        } = prepared;

        let root = dirfd::open_root(&self.data_dir).map_err(|e| self.fd_fail(OP, "data_dir", e))?;
        let config = dirfd::open_dir(&root, "config").map_err(|e| self.fd_fail(OP, "config", e))?;
        let resources = dirfd::open_dir(&config, "resources")
            .map_err(|e| self.fd_fail(OP, "config/resources", e))?;
        let collection_name = format!("the {} collection", setup.mode);
        let collection = dirfd::ensure_dir(&resources, &setup.mode)
            .map_err(|e| self.fd_fail(OP, &collection_name, e))?;
        let stage_root = match remote_root {
            Some(root) => Some(
                dirfd::open_root(Path::new(root))
                    .map_err(|e| self.fd_fail(OP, "the staging root", e))?,
            ),
            None => None,
        };

        // ---- writes
        let mut written = Vec::new();
        for (path, sha) in files {
            let parts: Vec<&str> = path.components().collect();
            let Some((name, parents)) = parts.split_last() else {
                continue;
            };
            let dir = dirfd::ensure_walk(&collection, parents)
                .map_err(|e| self.fd_fail(OP, &path.to_string(), e))?;
            match dirfd::kind_at(&dir, name) {
                Ok(Kind::Missing | Kind::File) => {}
                Ok(_) => {
                    return Err(self.mirror_fail(
                        OP,
                        format!("{path} changed during the commit and is not a regular file"),
                    ));
                }
                Err(e) => return Err(self.fd_fail(OP, &path.to_string(), e)),
            }
            if let Some(stage) = &stage_root {
                let what = format!("the staged copy of {path}");
                let staged_dir =
                    dirfd::walk(stage, parents).map_err(|e| self.fd_fail(OP, &what, e))?;
                // verify the very handle we are about to move
                let (ident, bytes) = dirfd::read_file(&staged_dir, name, READ_MAX_FILE_BYTES)
                    .map_err(|e| self.fd_fail(OP, &what, e))?;
                if bytes.len() as u64 > READ_MAX_FILE_BYTES || sha256_hex(&bytes) != *sha {
                    return Err(self.mirror_fail(OP, format!("{what} changed after it was staged")));
                }
                match dirfd::ident_at(&staged_dir, name) {
                    Ok(Some(now)) if now == ident => {}
                    Ok(_) => {
                        return Err(
                            self.mirror_fail(OP, format!("{what} changed while it was written"))
                        );
                    }
                    Err(e) => return Err(self.fd_fail(OP, &what, e)),
                }
                dirfd::rename_at(&staged_dir, name, &dir, name).map_err(|e| match e {
                    FdError::Io(io) => self.mirror_fail(
                        OP,
                        format!(
                            "cannot move {path} into place ({}); the staging root must be on the same filesystem as the collection",
                            io.kind()
                        ),
                    ),
                    other => self.fd_fail(OP, &path.to_string(), other),
                })?;
                match dirfd::ident_at(&dir, name) {
                    Ok(Some(now)) if now == ident => {}
                    Ok(_) => {
                        return Err(self.mirror_fail(
                            OP,
                            format!("{path} was replaced while it was moved into place"),
                        ));
                    }
                    Err(e) => return Err(self.fd_fail(OP, &path.to_string(), e)),
                }
            } else {
                let bytes = self.read_staged(OP, source, path, sha)?;
                let mut file = dirfd::open_overwrite(&dir, name)
                    .map_err(|e| self.fd_fail(OP, &path.to_string(), e))?;
                file.write_all(&bytes).map_err(|e| {
                    self.mirror_fail(OP, format!("cannot write {path} ({})", e.kind()))
                })?;
            }
            written.push(path.to_string());
        }

        // ---- deletes
        let stopped = |path: &RelPath| {
            self.mirror_fail(
                OP,
                format!("{path} is no longer a regular file; stopped before deleting it"),
            )
        };
        let mut deleted = Vec::new();
        for path in deletes {
            let parts: Vec<&str> = path.components().collect();
            let Some((name, parents)) = parts.split_last() else {
                continue;
            };
            let dir = match dirfd::walk(&collection, parents) {
                Ok(dir) => dir,
                Err(FdError::Missing) => return Err(stopped(path)),
                Err(e) => return Err(self.fd_fail(OP, &path.to_string(), e)),
            };
            match dirfd::kind_at(&dir, name) {
                Ok(Kind::File) => {}
                Ok(_) => return Err(stopped(path)),
                Err(e) => return Err(self.fd_fail(OP, &path.to_string(), e)),
            }
            dirfd::unlink_at(&dir, name).map_err(|e| self.fd_fail(OP, &path.to_string(), e))?;
            deleted.push(path.to_string());
        }

        // ---- directories a delete emptied, never `<module>/<type>` or above
        let mut removed: BTreeSet<String> = BTreeSet::new();
        for path in deletes {
            let parts: Vec<&str> = path.components().collect();
            for depth in (super::MIRROR_MIN_COMPONENTS..parts.len()).rev() {
                let rel = parts[..depth].join("/");
                if removed.contains(&rel) {
                    continue;
                }
                let Some((name, parents)) = parts[..depth].split_last() else {
                    continue;
                };
                let parent = match dirfd::walk(&collection, parents) {
                    Ok(parent) => parent,
                    Err(FdError::Missing) => continue,
                    Err(e) => return Err(self.fd_fail(OP, &rel, e)),
                };
                match dirfd::kind_at(&parent, name) {
                    Ok(Kind::Dir) => {}
                    Ok(Kind::Missing) => continue,
                    Ok(_) => {
                        return Err(self.mirror_fail(
                            OP,
                            format!("{rel} is not a real directory; stopped before removing it"),
                        ));
                    }
                    Err(e) => return Err(self.fd_fail(OP, &rel, e)),
                }
                match dirfd::rmdir_at(&parent, name) {
                    Ok(()) => {
                        removed.insert(rel);
                    }
                    // an occupied directory keeps its ancestors
                    Err(FdError::NotEmpty | FdError::Exists) => break,
                    Err(e) => return Err(self.fd_fail(OP, &rel, e)),
                }
            }
        }
        Ok(MirrorReport {
            written,
            deleted,
            removed_dirs: removed.into_iter().collect(),
        })
    }

    /// The mutations, path-based (windows has no descriptor-relative
    /// primitives in std; the unix build uses the descriptor-relative apply
    /// below). Residual: an intermediate directory swapped for a link between
    /// the preflight and a mutation is not defended against here.
    #[cfg(not(unix))]
    fn commit_apply(
        &self,
        prepared: &Prepared,
        files: &[(RelPath, String)],
        remote_root: Option<&str>,
    ) -> Result<MirrorReport, CoreError> {
        const OP: &str = "commit";
        let setup = self.mirror_setup(OP)?;
        let Prepared {
            collection,
            collection_exists,
            source,
            deletes,
        } = prepared;
        let (collection, collection_exists) = (collection.as_path(), *collection_exists);
        let deletes: Vec<&RelPath> = deletes.iter().collect();

        // ---- writes
        if !collection_exists {
            std::fs::create_dir(&collection).map_err(|e| {
                self.mirror_fail(
                    OP,
                    format!("cannot create the {} collection ({})", setup.mode, e.kind()),
                )
            })?;
        }
        let mut written = Vec::new();
        for (path, sha) in files {
            let parts: Vec<&str> = path.components().collect();
            let Some((name, parents)) = parts.split_last() else {
                continue;
            };
            let dir = self.ensure_dir_chain(OP, &collection, parents)?;
            let target = dir.join(name);
            match node_of(&target) {
                Ok(Node::Missing | Node::File) => {}
                _ => {
                    return Err(self.mirror_fail(
                        OP,
                        format!("{path} changed during the commit and is not a regular file"),
                    ));
                }
            }
            if remote_root.is_some() {
                std::fs::rename(source.join(path.as_str()), &target).map_err(|e| {
                    self.mirror_fail(
                        OP,
                        format!(
                            "cannot move {path} into place ({}); the staging root must be on the same filesystem as the collection",
                            e.kind()
                        ),
                    )
                })?;
            } else {
                let bytes = self.read_staged(OP, &source, path, sha)?;
                self.write_in_place(OP, &target, &bytes)?;
            }
            written.push(path.to_string());
        }

        // ---- deletes
        let mut deleted = Vec::new();
        for path in &deletes {
            let target = collection.join(path.as_str());
            if !matches!(node_of(&target), Ok(Node::File)) {
                return Err(self.mirror_fail(
                    OP,
                    format!("{path} is no longer a regular file; stopped before deleting it"),
                ));
            }
            std::fs::remove_file(&target).map_err(|e| {
                self.mirror_fail(OP, format!("cannot delete {path} ({})", e.kind()))
            })?;
            deleted.push(path.to_string());
        }

        // ---- directories a delete emptied, never `<module>/<type>` or above
        let mut removed: BTreeSet<String> = BTreeSet::new();
        for path in &deletes {
            let parts: Vec<&str> = path.components().collect();
            for depth in (super::MIRROR_MIN_COMPONENTS..parts.len()).rev() {
                let rel = parts[..depth].join("/");
                if removed.contains(&rel) {
                    continue;
                }
                let dir = collection.join(&rel);
                match node_of(&dir) {
                    Ok(Node::Dir) => {}
                    Ok(Node::Missing) => continue,
                    _ => {
                        return Err(self.mirror_fail(
                            OP,
                            format!("{rel} is not a real directory; stopped before removing it"),
                        ));
                    }
                }
                match std::fs::remove_dir(&dir) {
                    Ok(()) => {
                        removed.insert(rel);
                    }
                    Err(e) => {
                        let occupied = std::fs::read_dir(&dir)
                            .map(|mut entries| entries.next().is_some())
                            .unwrap_or(false);
                        if occupied {
                            break;
                        }
                        return Err(self.mirror_fail(
                            OP,
                            format!("cannot remove the emptied directory {rel} ({})", e.kind()),
                        ));
                    }
                }
            }
        }
        Ok(MirrorReport {
            written,
            deleted,
            removed_dirs: removed.into_iter().collect(),
        })
    }
}

#[async_trait::async_trait]
impl TreeMirror for LocalFsTransport {
    async fn stage(&self, tree: &RenderedTree) -> Result<Staged, CoreError> {
        let this = self.clone();
        let tree = tree.clone();
        tokio::task::spawn_blocking(move || this.stage_blocking(&tree))
            .await
            .map_err(|e| CoreError::Internal(format!("local stage task failed: {e}")))?
    }

    async fn commit(
        &self,
        staged: &Staged,
        delete_set: &DeleteSet,
    ) -> Result<MirrorReport, CoreError> {
        let this = self.clone();
        let local = staged.local.path().to_path_buf();
        let files = staged.files.clone();
        let remote_root = staged.remote_root.clone();
        let owner = staged.owner.clone();
        let delete_set = delete_set.clone();
        tokio::task::spawn_blocking(move || {
            this.commit_blocking(&local, &files, remote_root.as_deref(), &owner, &delete_set)
        })
        .await
        .map_err(|e| CoreError::Internal(format!("local commit task failed: {e}")))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_dirs_stops_at_the_entry_cap() {
        let dir = tempfile::tempdir().expect("tempdir");
        for i in 0..6 {
            std::fs::write(dir.path().join(format!("f{i}")), b"x").expect("write");
        }
        let transport = LocalFsTransport::new(dir.path().to_path_buf(), "nodes.master");
        let mut seen = 0;
        let err = transport
            .child_dirs(dir.path(), "config/resources/core", &mut seen, 3)
            .expect_err("six entries exceed a cap of three");
        let text = err.to_string();
        assert!(text.contains("more than 3 entries"), "{text}");
        assert!(text.contains("config/resources/core"), "{text}");

        // The counter is shared: a second directory continues from it.
        let mut seen = 0;
        assert!(
            transport
                .child_dirs(dir.path(), "r", &mut seen, 6)
                .expect("six fit under six")
                .is_empty()
        );
        assert_eq!(seen, 6);
        assert!(transport.child_dirs(dir.path(), "r", &mut seen, 6).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn read_blobs_refuses_a_file_swapped_for_a_symlink_after_the_check() {
        let dir = tempfile::tempdir().expect("tempdir");
        let outside = tempfile::tempdir().expect("tempdir");
        let secret = outside.path().join("secret");
        std::fs::write(&secret, b"outside").expect("write");
        let file = dir.path().join("f");
        std::fs::write(&file, b"inside").expect("write");
        let transport = LocalFsTransport::new(dir.path().to_path_buf(), "nodes.master");

        let mut paths = BTreeMap::new();
        paths.insert(RelPath::new("f").expect("rel"), file.clone());
        let blobs = transport
            .read_blobs("core", paths.clone(), &mut Budget::default())
            .expect("a plain file reads");
        assert_eq!(blobs.values().next().expect("one").bytes, b"inside");

        // The swap itself is a race between check and open; the handle check
        // that closes it is exercised directly with the stale metadata.
        let before = std::fs::symlink_metadata(&file).expect("meta");
        std::fs::remove_file(&file).expect("remove");
        std::os::unix::fs::symlink(&secret, &file).expect("symlink");
        let err = transport
            .open_checked(&file, &before, "config/resources/core/f")
            .expect_err("a swapped inode must be refused");
        assert!(
            err.to_string()
                .contains("config/resources/core/f changed while it was read"),
            "{err}"
        );
        // And a symlink present at check time is still refused up front.
        let err = transport
            .read_blobs("core", paths, &mut Budget::default())
            .expect_err("a symlink is not a regular file");
        assert!(
            err.to_string().contains("no longer a regular file"),
            "{err}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn read_blobs_refuses_a_parent_directory_swapped_for_a_symlink() {
        let dir = tempfile::tempdir().expect("tempdir");
        let outside = tempfile::tempdir().expect("tempdir");
        std::fs::write(outside.path().join("f"), b"outside-secret").expect("write");
        std::fs::create_dir(dir.path().join("a")).expect("mkdir");
        std::fs::write(dir.path().join("a/f"), b"inside").expect("write");
        let transport = LocalFsTransport::new(dir.path().to_path_buf(), "nodes.master");

        // The walk already ran: this is its (now stale) path list.
        let mut paths = BTreeMap::new();
        paths.insert(RelPath::new("a/f").expect("rel"), dir.path().join("a/f"));
        let blobs = transport
            .read_blobs("core", paths.clone(), &mut Budget::default())
            .expect("an unswapped tree reads");
        assert_eq!(blobs.values().next().expect("one").bytes, b"inside");

        // Swap the parent directory for a symlink to an outside directory
        // holding a same-named regular file. lstat and open of the full path
        // both follow it to that file, so only a descriptor-relative open
        // refuses it.
        std::fs::remove_dir_all(dir.path().join("a")).expect("remove");
        std::os::unix::fs::symlink(outside.path(), dir.path().join("a")).expect("symlink");
        let err = transport
            .read_blobs("core", paths, &mut Budget::default())
            .expect_err("a swapped parent directory must be refused");
        let text = err.to_string();
        assert!(
            text.contains("config/resources/core/a/f changed while it was read"),
            "{text}"
        );
        assert!(!text.contains("outside-secret"), "{text}");
    }

    fn ledger_dir(root: &Path) -> PathBuf {
        root.join("ign-deploy").join("gw").join("dev")
    }

    #[test]
    fn read_ledger_local() {
        let dir = tempfile::tempdir().expect("tempdir");
        let transport = LocalFsTransport::new(dir.path().to_path_buf(), "nodes.master");
        let read = || transport.read_ledger_blocking("gw", "dev");

        // absent at every level
        assert_eq!(read().expect("no ign-deploy dir"), None);
        std::fs::create_dir(dir.path().join("ign-deploy")).expect("mkdir");
        assert_eq!(read().expect("no gateway dir"), None);
        std::fs::create_dir_all(ledger_dir(dir.path())).expect("mkdir");
        assert_eq!(read().expect("no ledger file"), None);

        // present: the bytes, unvalidated
        std::fs::write(ledger_dir(dir.path()).join("ledger.json"), b"anything").expect("write");
        assert_eq!(read().expect("present"), Some(b"anything".to_vec()));

        // an empty file is returned as empty bytes (the parser refuses it)
        std::fs::write(ledger_dir(dir.path()).join("ledger.json"), b"").expect("write");
        assert_eq!(read().expect("empty is present"), Some(Vec::new()));
    }

    #[test]
    fn read_ledger_refuses_a_directory_at_the_ledger_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(ledger_dir(dir.path()).join("ledger.json")).expect("mkdir");
        let transport = LocalFsTransport::new(dir.path().to_path_buf(), "nodes.master");
        let err = transport
            .read_ledger_blocking("gw", "dev")
            .expect_err("a directory is not a ledger");
        assert_eq!(err.code(), "deploy_ledger_invalid");
        assert!(
            err.to_string().contains("ign-deploy/gw/dev/ledger.json"),
            "{err}"
        );
    }

    #[test]
    fn read_ledger_refuses_an_oversized_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(ledger_dir(dir.path())).expect("mkdir");
        let file = std::fs::File::create(ledger_dir(dir.path()).join("ledger.json")).expect("file");
        file.set_len(LEDGER_MAX_BYTES as u64 + 1).expect("sparse");
        let transport = LocalFsTransport::new(dir.path().to_path_buf(), "nodes.master");
        let err = transport
            .read_ledger_blocking("gw", "dev")
            .expect_err("over the cap");
        assert_eq!(err.code(), "deploy_ledger_invalid");
        assert!(err.to_string().contains("8388608"), "{err}");
    }

    #[test]
    fn read_ledger_with_an_unreadable_host_is_transport_failed_not_absent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let transport = LocalFsTransport::new(dir.path().join("missing"), "nodes.master");
        let err = transport
            .read_ledger_blocking("gw", "dev")
            .expect_err("a missing data_dir is not an absent ledger");
        assert_eq!(err.code(), "transport_failed");
    }

    #[test]
    fn read_ledger_refuses_unsafe_target_names() {
        let dir = tempfile::tempdir().expect("tempdir");
        let transport = LocalFsTransport::new(dir.path().to_path_buf(), "nodes.master");
        for (gateway, mode) in [("..", "dev"), ("gw", "a/b"), ("", "dev"), (".x", "dev")] {
            let err = transport
                .read_ledger_blocking(gateway, mode)
                .expect_err("a name that is not one safe component");
            assert_eq!(err.code(), "deploy_ledger_invalid", "{gateway:?} {mode:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn read_ledger_refuses_symlinks() {
        let dir = tempfile::tempdir().expect("tempdir");
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join("ledger.json"), b"outside-secret").expect("write");
        let transport = LocalFsTransport::new(dir.path().to_path_buf(), "nodes.master");

        // a symlinked ledger file
        std::fs::create_dir_all(ledger_dir(dir.path())).expect("mkdir");
        std::os::unix::fs::symlink(
            outside.path().join("ledger.json"),
            ledger_dir(dir.path()).join("ledger.json"),
        )
        .expect("symlink");
        let err = transport
            .read_ledger_blocking("gw", "dev")
            .expect_err("a symlinked ledger");
        assert_eq!(err.code(), "deploy_ledger_invalid");
        assert!(!err.to_string().contains("outside-secret"), "{err}");

        // a symlinked directory on the way
        std::fs::remove_dir_all(dir.path().join("ign-deploy")).expect("remove");
        std::os::unix::fs::symlink(outside.path(), dir.path().join("ign-deploy")).expect("symlink");
        let err = transport
            .read_ledger_blocking("gw", "dev")
            .expect_err("a symlinked directory");
        assert_eq!(err.code(), "deploy_ledger_invalid");
    }

    #[tokio::test]
    async fn read_ledger_through_the_trait() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(ledger_dir(dir.path())).expect("mkdir");
        std::fs::write(ledger_dir(dir.path()).join("ledger.json"), b"x").expect("write");
        let transport = LocalFsTransport::new(dir.path().to_path_buf(), "nodes.master");
        let reader: &dyn TreeReader = &transport;
        assert_eq!(
            reader.read_ledger("gw", "dev").await.expect("read"),
            Some(b"x".to_vec())
        );
        assert_eq!(reader.read_ledger("gw", "prod").await.expect("read"), None);
    }

    // ---- F7: the commit's mutations are descriptor-relative (unix) --------

    #[cfg(unix)]
    mod toctou {
        use super::*;
        use std::collections::BTreeMap;

        const DT: &str = "ignition/database-translator";

        type Snap = BTreeMap<String, Option<Vec<u8>>>;

        fn snap(dir: &Path) -> Snap {
            fn walk(dir: &Path, rel: &str, out: &mut Snap) {
                let mut entries: Vec<_> = std::fs::read_dir(dir)
                    .expect("read_dir")
                    .map(|e| e.expect("entry"))
                    .collect();
                entries.sort_by_key(|e| e.file_name());
                for entry in entries {
                    let name = entry.file_name().into_string().expect("utf8");
                    let path = format!("{rel}{name}");
                    let meta = std::fs::symlink_metadata(entry.path()).expect("meta");
                    if meta.file_type().is_symlink() {
                        let target = std::fs::read_link(entry.path()).expect("link");
                        out.insert(path, Some(format!("-> {}", target.display()).into_bytes()));
                    } else if meta.is_dir() {
                        out.insert(path.clone(), None);
                        walk(&entry.path(), &format!("{path}/"), out);
                    } else {
                        out.insert(path, Some(std::fs::read(entry.path()).expect("read")));
                    }
                }
            }
            let mut out = Snap::new();
            walk(dir, "", &mut out);
            out
        }

        fn put(path: &Path, bytes: &[u8]) {
            std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
            std::fs::write(path, bytes).expect("write");
        }

        struct Rig {
            root: tempfile::TempDir,
            data: PathBuf,
            stage_root: PathBuf,
            outside: PathBuf,
        }

        impl Rig {
            fn new() -> Rig {
                let root = tempfile::tempdir().expect("tempdir");
                let base = root.path().canonicalize().expect("canonical");
                let data = base.join("data");
                let dev = data.join("config/resources/dev");
                put(&dev.join(format!("{DT}/KEEP/config.json")), b"{\"keep\":1}");
                put(&dev.join(format!("{DT}/OLD/config.json")), b"{\"old\":1}");
                put(&dev.join(format!("{DT}/UPD/config.json")), b"{\"v\":1}");
                let outside = base.join("outside");
                // an outside tree shaped like the collection, so a followed
                // symlink would find every name it looks for
                for rel in ["KEEP", "OLD", "UPD"] {
                    put(
                        &outside.join(format!("{DT}/{rel}/config.json")),
                        b"{\"secret\":1}",
                    );
                }
                put(
                    &outside.join(format!("{DT}/OLD/resource.json")),
                    b"{\"secret\":2}",
                );
                put(&outside.join("config.json"), b"{\"secret\":3}");
                Rig {
                    stage_root: base.join("stage"),
                    root,
                    data,
                    outside,
                }
            }

            fn mirror(&self, separate: bool) -> LocalFsTransport {
                let staging = if separate {
                    StagingMode::Separate {
                        root: self.stage_root.display().to_string(),
                    }
                } else {
                    StagingMode::InPlace
                };
                LocalFsTransport::new(self.data.clone(), "nodes.master")
                    .with_mirror("dev", staging)
                    .expect("mirror")
            }

            fn tree(&self) -> RenderedTree {
                let mut tree = RenderedTree::default();
                for (path, bytes) in [
                    (format!("{DT}/UPD/config.json"), &b"{\"v\":2}"[..]),
                    (format!("{DT}/NEW/config.json"), &b"{\"new\":1}"[..]),
                ] {
                    tree.files
                        .insert(RelPath::new(&path).expect("rel"), bytes.to_vec());
                }
                tree
            }

            fn deletes(&self) -> DeleteSet {
                DeleteSet {
                    files: [format!("{DT}/OLD/config.json")]
                        .iter()
                        .map(|p| RelPath::new(p).expect("rel"))
                        .collect(),
                }
            }

            /// Replace `path` (below the data dir) with a symlink to `target`
            /// after moving the real directory aside.
            fn swap(&self, below_data: &str, target: &Path) {
                let path = self.data.join(below_data);
                let aside = self
                    .root
                    .path()
                    .join("aside")
                    .join(below_data.replace('/', "_"));
                std::fs::create_dir_all(aside.parent().expect("parent")).expect("aside");
                std::fs::rename(&path, &aside).expect("move aside");
                std::os::unix::fs::symlink(target, &path).expect("symlink");
            }
        }

        /// A stale plan: the preflight passed, THEN a directory on the way was
        /// swapped for a symlink to an outside tree. The apply phase must refuse
        /// and leave the outside tree byte-identical.
        #[test]
        fn a_directory_swapped_for_a_symlink_after_the_preflight_is_refused() {
            let cases: [(&str, &str); 6] = [
                (
                    "the unit directory",
                    "config/resources/dev/ignition/database-translator/UPD",
                ),
                (
                    "the delete target's directory",
                    "config/resources/dev/ignition/database-translator/OLD",
                ),
                (
                    "the type directory",
                    "config/resources/dev/ignition/database-translator",
                ),
                ("the module directory", "config/resources/dev/ignition"),
                ("the mode collection", "config/resources/dev"),
                ("config", "config"),
            ];
            for separate in [false, true] {
                for (name, swapped) in cases {
                    let rig = Rig::new();
                    let mirror = rig.mirror(separate);
                    let staged = mirror.stage_blocking(&rig.tree()).expect("stage");
                    let deletes = rig.deletes();
                    let prepared = mirror
                        .commit_preflight(
                            staged.local.path(),
                            &staged.files,
                            staged.remote_root.as_deref(),
                            &staged.owner,
                            &deletes,
                        )
                        .expect("preflight passes on the honest tree");
                    let outside_before = snap(&rig.outside);
                    rig.swap(swapped, &rig.outside);

                    let err = mirror
                        .commit_apply(&prepared, &staged.files, staged.remote_root.as_deref())
                        .expect_err(&format!("{name} (separate={separate}) must be refused"));
                    assert_eq!(err.code(), "transport_failed", "{name}: {err}");
                    assert!(
                        err.to_string().contains("changed while it was written"),
                        "{name}: {err}"
                    );
                    assert_eq!(
                        snap(&rig.outside),
                        outside_before,
                        "{name} (separate={separate}): the outside tree was touched"
                    );
                }
            }
        }

        /// The same swap, but the symlink target is the staging root's twin: the
        /// Separate commit must not rename a staged file out of a swapped
        /// staging directory either.
        #[test]
        fn a_staging_directory_swapped_for_a_symlink_after_the_preflight_is_refused() {
            let rig = Rig::new();
            let mirror = rig.mirror(true);
            let staged = mirror.stage_blocking(&rig.tree()).expect("stage");
            let deletes = DeleteSet::default();
            let prepared = mirror
                .commit_preflight(
                    staged.local.path(),
                    &staged.files,
                    staged.remote_root.as_deref(),
                    &staged.owner,
                    &deletes,
                )
                .expect("preflight");
            // the staged unit directory becomes a link to an outside tree that
            // holds a file of the same name with other bytes
            let decoy = rig.root.path().join("decoy");
            put(&decoy.join("config.json"), b"{\"decoy\":1}");
            let staged_unit = rig.stage_root.join(format!("{DT}/NEW"));
            std::fs::rename(&staged_unit, rig.root.path().join("aside-new")).expect("aside");
            std::os::unix::fs::symlink(&decoy, &staged_unit).expect("symlink");
            let collection_before = snap(&rig.data);

            let err = mirror
                .commit_apply(&prepared, &staged.files, staged.remote_root.as_deref())
                .expect_err("a swapped staging directory");
            assert!(
                err.to_string().contains("changed while it was written"),
                "{err}"
            );
            assert_eq!(snap(&decoy).len(), 1);
            assert!(
                !snap(&rig.data)
                    .keys()
                    .any(|k| k.ends_with("NEW/config.json") && !collection_before.contains_key(k)),
                "the decoy was moved into the collection"
            );
        }

        /// Staging into a root whose subdirectory is a symlink writes nothing
        /// through it.
        #[test]
        fn staging_never_writes_through_a_symlink_in_the_staging_root() {
            let rig = Rig::new();
            std::fs::create_dir_all(rig.stage_root.join("ignition")).expect("dirs");
            std::os::unix::fs::symlink(&rig.outside, rig.stage_root.join(DT)).expect("symlink");
            let before = snap(&rig.outside);
            let mirror = rig.mirror(true);
            let err = mirror.stage_blocking(&rig.tree()).expect_err("symlink");
            assert_eq!(err.code(), "transport_failed", "{err}");
            assert_eq!(snap(&rig.outside), before);
        }

        /// An honest apply still does exactly the plan, with the file mode a
        /// plain `File::create` would give.
        #[test]
        fn the_descriptor_relative_apply_matches_the_path_based_result() {
            use std::os::unix::fs::PermissionsExt;
            for separate in [false, true] {
                let rig = Rig::new();
                let mirror = rig.mirror(separate);
                let staged = mirror.stage_blocking(&rig.tree()).expect("stage");
                let report = mirror
                    .commit_blocking(
                        staged.local.path(),
                        &staged.files,
                        staged.remote_root.as_deref(),
                        &staged.owner,
                        &rig.deletes(),
                    )
                    .expect("commit");
                assert_eq!(report.deleted, vec![format!("{DT}/OLD/config.json")]);
                assert_eq!(report.removed_dirs, vec![format!("{DT}/OLD")]);
                let dev = rig.data.join("config/resources/dev");
                assert_eq!(
                    std::fs::read(dev.join(format!("{DT}/NEW/config.json"))).unwrap(),
                    b"{\"new\":1}"
                );
                assert_eq!(
                    std::fs::read(dev.join(format!("{DT}/UPD/config.json"))).unwrap(),
                    b"{\"v\":2}"
                );
                assert!(!dev.join(format!("{DT}/OLD")).exists());
                assert!(dev.join(format!("{DT}/KEEP/config.json")).is_file());
                if !separate {
                    let mode = std::fs::metadata(dev.join(format!("{DT}/NEW/config.json")))
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o777;
                    assert_eq!(mode & 0o600, 0o600, "owner can read and write: {mode:o}");
                }
            }
        }
    }
}
