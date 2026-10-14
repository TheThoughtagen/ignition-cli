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

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use super::{
    CORE_COLLECTION, FileBlob, ObservedFiles, READ_MAX_ENTRIES, READ_MAX_FILE_BYTES,
    READ_MAX_TOTAL_BYTES, Scope, TreeReader,
};
use crate::deploy::layout::{RelPath, TAG_TYPES};
use crate::deploy::plan::unit_key;
use crate::deploy::tree::{OpenError, open_regular_file, sha256_hex, walk_capped};
use crate::error::CoreError;

/// The collection that overrides the mode collection on the gateway.
const LOCAL_COLLECTION: &str = "local";

/// Reads a gateway data directory on the runner's own filesystem.
#[derive(Debug, Clone)]
pub struct LocalFsTransport {
    data_dir: PathBuf,
    label: String,
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
        }
    }

    fn fail(&self, detail: impl std::fmt::Display) -> CoreError {
        CoreError::TransportFailed {
            operation: "read".to_string(),
            node: self.label.clone(),
            detail: detail.to_string(),
        }
    }

    /// `<data_dir>/config/resources`, after requiring `data_dir` and it to be
    /// real directories.
    fn resources_root(&self) -> Result<PathBuf, CoreError> {
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

    /// Open `path` and validate the handle against `before`.
    fn open_checked(
        &self,
        path: &Path,
        before: &std::fs::Metadata,
        shown: &str,
    ) -> Result<(std::fs::File, std::fs::Metadata), CoreError> {
        open_regular_file(path, before).map_err(|e| match e {
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
}
