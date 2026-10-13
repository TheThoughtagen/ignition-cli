//! The in-memory source and rendered trees, and the render digest.
//!
//! [`SourceTree`] is what render reads: gateway-relative path to raw bytes.
//! [`RenderedTree`] is what render produces: collection path to raw bytes.
//! Both are `BTreeMap`s keyed by [`RelPath`], so iteration is byte-sorted
//! on every platform and never depends on directory order.
//!
//! ## `render_sha256` listing format (documented in the README)
//!
//! For every rendered file, in byte-sorted collection-path order, one entry:
//!
//! ```text
//! <collection path> NUL <lowercase hex sha256 of the file bytes> LF
//! ```
//!
//! The digest is the lowercase hex SHA-256 of the concatenated entries. An
//! empty tree hashes the empty listing ([`EMPTY_RENDER_SHA256`]).

use std::collections::BTreeMap;
use std::path::Path;

use sha2::{Digest, Sha256};

use super::layout::RelPath;
use crate::error::{CoreError, DeployFinding};

/// SHA-256 of the empty listing (the digest of a render that produced no
/// files). Equal to the SHA-256 of zero bytes.
pub const EMPTY_RENDER_SHA256: &str =
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Source files of one gateway directory, keyed by gateway-relative path
/// (`resources/<module>/<type>/...`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceTree {
    /// Gateway-relative path to raw file bytes.
    pub files: BTreeMap<RelPath, Vec<u8>>,
}

/// The rendered files, keyed by collection path (`<module>/<type>/...`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenderedTree {
    /// Collection path to raw file bytes.
    pub files: BTreeMap<RelPath, Vec<u8>>,
}

impl RenderedTree {
    /// Total size of every rendered file in bytes.
    pub fn bytes(&self) -> usize {
        self.files.values().map(Vec::len).sum()
    }

    /// The `render_sha256` digest (see the module doc for the listing).
    pub fn render_sha256(&self) -> String {
        let mut hasher = Sha256::new();
        for (path, bytes) in &self.files {
            hasher.update(path.as_str().as_bytes());
            hasher.update([0u8]);
            hasher.update(sha256_hex(bytes).as_bytes());
            hasher.update(b"\n");
        }
        format!("{:x}", hasher.finalize())
    }
}

impl SourceTree {
    /// Build a tree in memory (tests). Later entries replace earlier ones
    /// with the same path.
    pub fn from_files<I, P, B>(files: I) -> Result<SourceTree, String>
    where
        I: IntoIterator<Item = (P, B)>,
        P: AsRef<str>,
        B: Into<Vec<u8>>,
    {
        let mut map = BTreeMap::new();
        for (path, bytes) in files {
            map.insert(RelPath::new(path.as_ref())?, bytes.into());
        }
        Ok(SourceTree { files: map })
    }

    /// Read `resources/`, `tags/` and `modes/<env>/` of one gateway directory.
    ///
    /// Every entry is inspected with `symlink_metadata` and never followed;
    /// each directory's entries are sorted by name bytes before recursing.
    /// At the gateway root `manifest.yaml` (not collected, the action reads
    /// it), `resources/`, `tags/` (walked), `modes/` and `.gitkeep` (skipped)
    /// are accepted; any other root entry is a finding. Under `modes/` only
    /// the directory named exactly `env` is opened; other entries that are
    /// directories belong to other environments and are skipped unopened, so
    /// their content can never fail or change this render. Under
    /// `modes/<env>/` only `resources/` and `tags/` are accepted. Overlay
    /// files are keyed by their full gateway-relative path
    /// (`modes/<env>/resources/...`); render's overlay stage resolves them.
    /// Symlinks, non-regular files and unusable names are findings. All
    /// findings are raised together as one `deploy_tree_invalid`.
    pub fn read_gateway(gateway_dir: &Path, env: &str) -> Result<SourceTree, CoreError> {
        let gateway = gateway_dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut reader = Reader {
            prefix: format!("gateways/{gateway}"),
            env: env.to_string(),
            files: BTreeMap::new(),
            findings: Vec::new(),
        };
        reader.read_root(gateway_dir);
        if reader.findings.is_empty() {
            Ok(SourceTree {
                files: reader.files,
            })
        } else {
            Err(CoreError::deploy_findings(reader.findings))
        }
    }
}

struct Reader {
    /// `gateways/<name>`, the repo-relative prefix used in findings.
    prefix: String,
    /// The environment whose `modes/<env>/` overlay is read.
    env: String,
    files: BTreeMap<RelPath, Vec<u8>>,
    findings: Vec<DeployFinding>,
}

impl Reader {
    fn finding(&mut self, rel: &str, message: impl Into<String>) {
        let file = if rel.is_empty() {
            self.prefix.clone()
        } else {
            format!("{}/{rel}", self.prefix)
        };
        self.findings.push(DeployFinding {
            file,
            line: None,
            column: None,
            message: message.into(),
        });
    }

    /// Entries of `dir` sorted by name bytes; read errors become findings.
    fn sorted_entries(&mut self, dir: &Path, rel: &str) -> Vec<(String, std::path::PathBuf)> {
        let read = match std::fs::read_dir(dir) {
            Ok(read) => read,
            Err(e) => {
                self.finding(rel, format!("cannot read directory ({})", e.kind()));
                return Vec::new();
            }
        };
        let mut entries: Vec<(String, std::path::PathBuf)> = Vec::new();
        for entry in read {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    self.finding(rel, format!("cannot read directory entry ({})", e.kind()));
                    continue;
                }
            };
            match entry.file_name().into_string() {
                Ok(name) => entries.push((name, entry.path())),
                Err(raw) => {
                    let lossy = raw.to_string_lossy();
                    let at = join(rel, &lossy);
                    self.finding(&at, "name is not valid UTF-8");
                }
            }
        }
        entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
        entries
    }

    fn read_root(&mut self, dir: &Path) {
        for (name, path) in self.sorted_entries(dir, "") {
            match name.as_str() {
                "manifest.yaml" | ".gitkeep" => {
                    self.check_regular(&path, &name);
                }
                "resources" | "tags" => self.walk(&path, &name),
                "modes" => self.read_modes(&path, &name),
                _ => self.finding(
                    &name,
                    "unexpected entry at the gateway root (expected: manifest.yaml, resources/, tags/, modes/)",
                ),
            }
        }
    }

    /// `modes/`: open only the directory named for this render's
    /// environment.
    fn read_modes(&mut self, dir: &Path, rel: &str) {
        if !self.require_dir(dir, rel) {
            return;
        }
        for (name, path) in self.sorted_entries(dir, rel) {
            let child = join(rel, &name);
            if name == self.env {
                self.read_env(&path, &child);
                continue;
            }
            match std::fs::symlink_metadata(&path) {
                // another environment's tree: skipped without being opened
                Ok(meta) if meta.file_type().is_dir() => {}
                Ok(_) => self.finding(
                    &child,
                    "unexpected entry under modes/ (expected one directory per environment)",
                ),
                Err(e) => self.finding(&child, format!("cannot inspect ({})", e.kind())),
            }
        }
    }

    /// `modes/<env>/`: `resources/` and `tags/` only.
    fn read_env(&mut self, dir: &Path, rel: &str) {
        if !self.require_dir(dir, rel) {
            return;
        }
        for (name, path) in self.sorted_entries(dir, rel) {
            let child = join(rel, &name);
            match name.as_str() {
                "resources" | "tags" => self.walk(&path, &child),
                _ => self.finding(
                    &child,
                    "unexpected entry under modes/<env>/ (expected: resources/, tags/)",
                ),
            }
        }
    }

    /// Whether `dir` is a real directory (not a symlink); findings otherwise.
    fn require_dir(&mut self, dir: &Path, rel: &str) -> bool {
        match std::fs::symlink_metadata(dir) {
            Ok(meta) if meta.file_type().is_symlink() => {
                self.finding(rel, "is a symlink; symlinks are refused");
                false
            }
            Ok(meta) if meta.file_type().is_dir() => true,
            Ok(_) => {
                self.finding(rel, "must be a directory");
                false
            }
            Err(e) => {
                self.finding(rel, format!("cannot inspect ({})", e.kind()));
                false
            }
        }
    }

    /// Reject a non-regular entry; returns whether `path` is a regular file.
    fn check_regular(&mut self, path: &Path, rel: &str) -> bool {
        match std::fs::symlink_metadata(path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                self.finding(rel, "is a symlink; symlinks are refused");
                false
            }
            Ok(meta) if meta.file_type().is_file() => true,
            Ok(_) => {
                self.finding(rel, "is not a regular file or directory");
                false
            }
            Err(e) => {
                self.finding(rel, format!("cannot inspect ({})", e.kind()));
                false
            }
        }
    }

    /// Walk directory `dir` (repo-gateway-relative `rel`), collecting files.
    fn walk(&mut self, dir: &Path, rel: &str) {
        if !self.require_dir(dir, rel) {
            return;
        }
        for (name, path) in self.sorted_entries(dir, rel) {
            let child = join(rel, &name);
            let meta = match std::fs::symlink_metadata(&path) {
                Ok(meta) => meta,
                Err(e) => {
                    self.finding(&child, format!("cannot inspect ({})", e.kind()));
                    continue;
                }
            };
            let kind = meta.file_type();
            if kind.is_symlink() {
                self.finding(&child, "is a symlink; symlinks are refused");
            } else if kind.is_dir() {
                self.walk(&path, &child);
            } else if kind.is_file() {
                match RelPath::new(&child) {
                    Err(why) => self.finding(&child, format!("unusable path: {why}")),
                    Ok(key) => match std::fs::read(&path) {
                        Ok(bytes) => {
                            self.files.insert(key, bytes);
                        }
                        Err(e) => self.finding(&child, format!("cannot read file ({})", e.kind())),
                    },
                }
            } else {
                self.finding(&child, "is not a regular file or directory");
            }
        }
    }
}

fn join(rel: &str, name: &str) -> String {
    if rel.is_empty() {
        name.to_string()
    } else {
        format!("{rel}/{name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_listing_digest_is_sha256_of_nothing() {
        assert_eq!(RenderedTree::default().render_sha256(), EMPTY_RENDER_SHA256);
        assert_eq!(sha256_hex(b""), EMPTY_RENDER_SHA256);
    }

    #[test]
    fn digest_follows_the_documented_listing() {
        let mut tree = RenderedTree::default();
        tree.files
            .insert(RelPath::new("b/x").unwrap(), b"two".to_vec());
        tree.files
            .insert(RelPath::new("a/x").unwrap(), b"one".to_vec());
        let mut listing = Vec::new();
        for (p, c) in [("a/x", &b"one"[..]), ("b/x", &b"two"[..])] {
            listing.extend_from_slice(p.as_bytes());
            listing.push(0);
            listing.extend_from_slice(sha256_hex(c).as_bytes());
            listing.push(b'\n');
        }
        assert_eq!(tree.render_sha256(), sha256_hex(&listing));
        assert_eq!(tree.bytes(), 6);
    }

    #[test]
    fn from_files_rejects_hostile_paths() {
        assert!(SourceTree::from_files([("../x", "a")]).is_err());
        assert!(SourceTree::from_files([("resources/a/b/c", "a")]).is_ok());
    }
}
