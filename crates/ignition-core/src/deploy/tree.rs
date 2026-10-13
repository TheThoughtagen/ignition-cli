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

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
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

/// File name of the marker `--out` writes at the root of a rendered tree.
pub const RENDER_MARKER: &str = "render.json";

/// Suffix of the temporary file a write goes through before its rename.
const TMP_SUFFIX: &str = ".ign-render-tmp";

/// `render.json`: ign's own record of a render written with `--out`. It is
/// not a gateway file and is never part of `render_sha256`. Phase 22 reads
/// it (`plan --base`), so its shape is versioned by `schema_version`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderMarker {
    /// Marker schema version (1).
    pub schema_version: u32,
    /// Gateway directory name the render came from.
    pub gateway: String,
    /// Environment the render was made for.
    pub env: String,
    /// Deployment mode from the target.
    pub mode: String,
    /// The render's `render_sha256` (equals the envelope's).
    pub render_sha256: String,
    /// Every written file, byte-sorted by path.
    pub files: Vec<MarkerFile>,
}

/// One file entry of [`RenderMarker`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkerFile {
    /// Collection path, relative to the `--out` directory.
    pub path: String,
    /// Lowercase hex SHA-256 of the file bytes.
    pub sha256: String,
    /// File size in bytes.
    pub bytes: u64,
}

impl RenderMarker {
    /// The marker for `tree` (files in byte-sorted path order).
    pub fn for_tree(gateway: &str, env: &str, mode: &str, tree: &RenderedTree) -> RenderMarker {
        RenderMarker {
            schema_version: 1,
            gateway: gateway.to_string(),
            env: env.to_string(),
            mode: mode.to_string(),
            render_sha256: tree.render_sha256(),
            files: tree
                .files
                .iter()
                .map(|(path, bytes)| MarkerFile {
                    path: path.as_str().to_string(),
                    sha256: sha256_hex(bytes),
                    bytes: bytes.len() as u64,
                })
                .collect(),
        }
    }

    /// The marker as written: pretty JSON plus one LF.
    pub fn to_bytes(&self) -> Result<Vec<u8>, CoreError> {
        let mut text = serde_json::to_string_pretty(self)
            .map_err(|e| CoreError::Internal(format!("cannot serialize {RENDER_MARKER}: {e}")))?;
        text.push('\n');
        Ok(text.into_bytes())
    }
}

fn refuse(out: &Path, reason: impl std::fmt::Display) -> CoreError {
    CoreError::InvalidInput {
        reason: format!(
            "--out {}: {reason}; remove the directory or choose an empty or absent one",
            out.display()
        ),
    }
}

/// What the check phase found.
enum OutPlan {
    /// The path is absent: create it.
    Create,
    /// An existing empty directory.
    Empty,
    /// A prior render of the same gateway and environment: its listed
    /// files (unchanged) may be replaced.
    Refresh(BTreeSet<String>),
}

/// Inspect `out` without writing anything and decide whether the render may
/// be written there.
fn check_out(tree: &RenderedTree, out: &Path, marker: &RenderMarker) -> Result<OutPlan, CoreError> {
    if tree.files.keys().any(|p| p.as_str() == RENDER_MARKER) {
        return Err(refuse(
            out,
            format!("the render holds a file named {RENDER_MARKER}"),
        ));
    }
    let meta = match std::fs::symlink_metadata(out) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(OutPlan::Create),
        Err(e) => return Err(refuse(out, format!("cannot inspect it ({})", e.kind()))),
    };
    if meta.file_type().is_symlink() {
        return Err(refuse(out, "is a symlink; symlinks are refused"));
    }
    if !meta.file_type().is_dir() {
        return Err(refuse(out, "exists and is not a directory"));
    }

    let mut files: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut dirs: BTreeSet<String> = BTreeSet::new();
    walk_out(out, "", &mut files, &mut dirs).map_err(|why| refuse(out, why))?;
    if files.is_empty() && dirs.is_empty() {
        return Ok(OutPlan::Empty);
    }

    let marker_path = match files.get(RENDER_MARKER) {
        Some(path) => path,
        None => {
            let first = files
                .keys()
                .next()
                .cloned()
                .or_else(|| dirs.iter().next().cloned())
                .unwrap_or_default();
            return Err(refuse(
                out,
                format!("is not empty and holds no {RENDER_MARKER} (first entry: {first})"),
            ));
        }
    };
    let prior: RenderMarker = std::fs::read(marker_path)
        .map_err(|e| refuse(out, format!("cannot read {RENDER_MARKER} ({})", e.kind())))
        .and_then(|bytes| {
            serde_json::from_slice(&bytes)
                .map_err(|_| refuse(out, format!("{RENDER_MARKER} is not a valid render marker")))
        })?;
    if prior.schema_version != 1 {
        return Err(refuse(
            out,
            format!(
                "{RENDER_MARKER} has schema_version {}, expected 1",
                prior.schema_version
            ),
        ));
    }
    if prior.gateway != marker.gateway || prior.env != marker.env {
        return Err(refuse(
            out,
            format!(
                "{RENDER_MARKER} belongs to gateway {} env {}, not gateway {} env {}",
                prior.gateway, prior.env, marker.gateway, marker.env
            ),
        ));
    }

    let mut listed: BTreeSet<String> = BTreeSet::new();
    for entry in &prior.files {
        if !listed.insert(entry.path.clone()) {
            return Err(refuse(
                out,
                format!("{RENDER_MARKER} lists {} twice", entry.path),
            ));
        }
    }
    // every regular file other than the marker is listed with its sha256
    for (rel, path) in &files {
        if rel == RENDER_MARKER {
            continue;
        }
        let Some(entry) = prior.files.iter().find(|f| &f.path == rel) else {
            return Err(refuse(
                out,
                format!("{rel} is not listed in {RENDER_MARKER}"),
            ));
        };
        let bytes = std::fs::read(path)
            .map_err(|e| refuse(out, format!("cannot read {rel} ({})", e.kind())))?;
        if sha256_hex(&bytes) != entry.sha256 {
            return Err(refuse(
                out,
                format!("{rel} was modified since it was rendered"),
            ));
        }
    }
    // every listed file is present, and still produced by the new render
    for rel in &listed {
        if !files.contains_key(rel) {
            return Err(refuse(
                out,
                format!("{rel} is listed in {RENDER_MARKER} but missing"),
            ));
        }
        if !tree.files.keys().any(|p| p.as_str() == rel) {
            return Err(refuse(
                out,
                format!("{rel} was rendered before but this render no longer produces it"),
            ));
        }
    }
    // no directory that holds no listed file
    for dir in &dirs {
        let prefix = format!("{dir}/");
        if !listed.iter().any(|rel| rel.starts_with(&prefix)) {
            return Err(refuse(out, format!("{dir}/ holds no rendered file")));
        }
    }
    // a new path may not collide with an existing file or directory
    for path in tree.files.keys() {
        let rel = path.as_str();
        if listed.contains(rel) {
            continue;
        }
        if files.contains_key(rel) || dirs.contains(rel) {
            return Err(refuse(out, format!("{rel} already exists")));
        }
        let mut at = 0;
        while let Some(i) = rel[at..].find('/') {
            at += i;
            if files.contains_key(&rel[..at]) {
                return Err(refuse(
                    out,
                    format!("{} is a file, not a directory", &rel[..at]),
                ));
            }
            at += 1;
        }
    }
    Ok(OutPlan::Refresh(listed))
}

/// Walk `dir` with `symlink_metadata`, entries sorted by name bytes.
/// Symlinks, special files and non-UTF-8 names are errors naming the path.
fn walk_out(
    dir: &Path,
    rel: &str,
    files: &mut BTreeMap<String, PathBuf>,
    dirs: &mut BTreeSet<String>,
) -> Result<(), String> {
    let read = std::fs::read_dir(dir)
        .map_err(|e| format!("cannot read directory {} ({})", join(rel, ""), e.kind()))?;
    let mut entries: Vec<(String, PathBuf)> = Vec::new();
    for entry in read {
        let entry = entry.map_err(|e| format!("cannot read a directory entry ({})", e.kind()))?;
        let name = entry.file_name().into_string().map_err(|raw| {
            format!(
                "{} has a name that is not valid UTF-8",
                join(rel, &raw.to_string_lossy())
            )
        })?;
        entries.push((name, entry.path()));
    }
    entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    for (name, path) in entries {
        let child = join(rel, &name);
        let meta = std::fs::symlink_metadata(&path)
            .map_err(|e| format!("cannot inspect {child} ({})", e.kind()))?;
        let kind = meta.file_type();
        if kind.is_symlink() {
            return Err(format!("{child} is a symlink; symlinks are refused"));
        } else if kind.is_dir() {
            dirs.insert(child.clone());
            walk_out(&path, &child, files, dirs)?;
        } else if kind.is_file() {
            files.insert(child, path);
        } else {
            return Err(format!("{child} is not a regular file or directory"));
        }
    }
    Ok(())
}

fn io_failure(what: &str, path: &Path, e: &std::io::Error) -> CoreError {
    if e.kind() == std::io::ErrorKind::AlreadyExists {
        return CoreError::InvalidInput {
            reason: format!(
                "--out: {} already exists (another writer won the race); nothing was replaced",
                path.display()
            ),
        };
    }
    CoreError::Internal(format!("cannot {what} {}: {e}", path.display()))
}

/// Write `bytes` to `<path>.ign-render-tmp` (created new) and rename it
/// over `path`.
fn write_replacing(path: &Path, bytes: &[u8]) -> Result<(), CoreError> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(TMP_SUFFIX);
    let tmp = PathBuf::from(tmp);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| io_failure("create", &tmp, &e))?;
    file.write_all(bytes)
        .map_err(|e| io_failure("write", &tmp, &e))?;
    drop(file);
    std::fs::rename(&tmp, path).map_err(|e| io_failure("replace", path, &e))
}

/// Write `tree` and its `marker` under `out`, in two phases.
///
/// Check phase (no writes): `out` must be absent, an empty directory, or a
/// directory that is exactly a previous render of the same gateway and
/// environment (see the README `--out` section); anything else is
/// `invalid_input` naming the reason and the first offending path. Symlinks
/// and special files anywhere under `out` are refused.
///
/// Write phase: new paths are opened with `create_new`, listed paths are
/// replaced through a temporary file and a rename, and `render.json` goes
/// last the same way. Nothing is ever removed.
pub fn write_out(tree: &RenderedTree, out: &Path, marker: &RenderMarker) -> Result<(), CoreError> {
    let plan = check_out(tree, out, marker)?;
    let marker_bytes = marker.to_bytes()?;

    match &plan {
        OutPlan::Create => {
            if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent).map_err(|e| io_failure("create", parent, &e))?;
            }
            std::fs::create_dir(out).map_err(|e| io_failure("create", out, &e))?;
        }
        OutPlan::Empty | OutPlan::Refresh(_) => {}
    }
    let listed = match &plan {
        OutPlan::Refresh(listed) => Some(listed),
        _ => None,
    };

    for (path, bytes) in &tree.files {
        let rel = path.as_str();
        let dest = rel.split('/').fold(out.to_path_buf(), |acc, c| acc.join(c));
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| io_failure("create", parent, &e))?;
        }
        if listed.is_some_and(|l| l.contains(rel)) {
            write_replacing(&dest, bytes)?;
        } else {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&dest)
                .map_err(|e| io_failure("create", &dest, &e))?;
            file.write_all(bytes)
                .map_err(|e| io_failure("write", &dest, &e))?;
        }
    }
    write_replacing(&out.join(RENDER_MARKER), &marker_bytes)
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
            if self.dot_entry(&path, &name, &name) {
                continue;
            }
            match name.as_str() {
                "manifest.yaml" => {
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
            if name == ".gitkeep" {
                self.dot_entry(&path, &child, &name);
                continue;
            }
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
            if self.dot_entry(&path, &child, &name) {
                continue;
            }
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

    /// Handle a dot-named entry: a regular file named `.gitkeep` is skipped
    /// at any depth, everything else that starts with `.` is a finding and is
    /// neither read nor walked. Only the first character decides, so
    /// `com.inductiveautomation.perspective` is not a dot-name. Returns
    /// whether `name` was a dot-name (and so is handled).
    fn dot_entry(&mut self, path: &Path, rel: &str, name: &str) -> bool {
        if !name.starts_with('.') {
            return false;
        }
        match std::fs::symlink_metadata(path) {
            Ok(meta) if name == ".gitkeep" && meta.file_type().is_file() => {}
            Ok(meta) if meta.file_type().is_symlink() => {
                self.finding(rel, "is a symlink; symlinks are refused");
            }
            _ => self.finding(
                rel,
                "name starts with `.`; dot-named entries are refused (only a `.gitkeep` file is skipped)",
            ),
        }
        true
    }

    /// Walk directory `dir` (repo-gateway-relative `rel`), collecting files.
    fn walk(&mut self, dir: &Path, rel: &str) {
        if !self.require_dir(dir, rel) {
            return;
        }
        for (name, path) in self.sorted_entries(dir, rel) {
            let child = join(rel, &name);
            if self.dot_entry(&path, &child, &name) {
                continue;
            }
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
