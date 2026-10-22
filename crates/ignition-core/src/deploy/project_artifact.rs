//! The project artifact and its canonical digest (Phase 25, D-01, D-09).
//!
//! [`build_artifact`] zips a project repo's checkout into the zip an
//! overwrite import would upload. [`canonical_digest`] reduces any project
//! zip (the artifact, or the gateway's export of the live project) to one
//! sha256 that does not depend on zip entry order, directory entries or the
//! volatile fields the gateway rewrites on import, so two zips of the same
//! project compare equal and a real change does not.
//!
//! ## Why the artifact is narrower than "the repo"
//!
//! An overwrite import replaces the whole project tree and the gateway drops,
//! without saying so, anything it does not adopt (live captures C8a, C8d). So
//! the artifact holds exactly what the gateway keeps (contract K1 to K4):
//!
//! - the layout is the checkout layout, verbatim; the canonical key of a
//!   member is its zip path and no `resources` segment is ever stripped by
//!   name (`com.inductiveautomation.webdev/resources` is a real checkout
//!   directory);
//! - the root `project.json`, plus the top-level directories named
//!   `ignition` or shaped like a module id ([`is_module_dir_name`]); every
//!   other root entry is left out;
//! - below those, every directory holding a `resource.json` contributes that
//!   file and exactly the files its `files` array lists. A listed file that
//!   is missing, or a directory inside a resource folder, refuses
//!   `deploy_tree_invalid`: the gateway would drop the whole resource;
//! - `.git`, `.github`, `.ign`, `node_modules`, `__pycache__` and `*.pyc` are
//!   never part of the walk, at any depth ([`BUILTIN_DENY`]). `deploy` is
//!   denied at the repo root only, and the root allowlist already does that;
//! - an optional `.ignexclude` at the repo root removes more paths
//!   ([`IgnExclude`]);
//! - everything left out is reported in [`Artifact::excluded`], so a reviewer
//!   sees what stays out of the uploaded project;
//! - a symlink or special file in the walk refuses `deploy_tree_invalid`
//!   naming its repo-relative path, and is never followed;
//! - a name inside the walk that is not valid UTF-8, a walk past
//!   [`ARTIFACT_MAX_ENTRIES`] entries, a built zip over `IMPORT_MAX_BYTES`
//!   and an artifact holding no member besides `project.json` each refuse
//!   `deploy_tree_invalid` too, before any gateway contact.
//!
//! The build is deterministic: entries in byte order of their zip path, one
//! fixed timestamp and permission bits, Deflate. Two builds of one checkout
//! are byte-identical.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::Path;

use super::tree::{read_repo_file, sha256_hex};
use crate::actions::projects::IMPORT_MAX_BYTES;
use crate::client::resources::normalize_descriptor;
use crate::error::CoreError;

/// The folder descriptor every resource folder carries.
const RESOURCE_JSON: &str = "resource.json";

/// The project descriptor at the root.
const PROJECT_JSON: &str = "project.json";

/// The canonical form of a project zip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalDigest {
    /// Lowercase hex sha256 over the `key NUL member-sha256 LF` lines, in
    /// byte order of the key.
    pub sha256: String,
    /// Member key (the zip path) to the sha256 of its canonical bytes.
    pub members: BTreeMap<String, String>,
}

/// A built artifact.
#[derive(Debug, Clone)]
pub struct Artifact {
    /// The zip bytes an import would upload.
    pub zip: Vec<u8>,
    /// Its canonical digest.
    pub digest: CanonicalDigest,
    /// File members in the zip (directory entries are not counted).
    pub members: usize,
    /// Repo-relative paths left out of the artifact, byte-sorted. A directory
    /// is listed once with a trailing `/`, never its contents.
    pub excluded: Vec<String>,
}

/// The repo-root file listing extra paths to leave out of the artifact.
pub const IGNEXCLUDE_FILE: &str = ".ignexclude";

/// Names left out of the artifact at any depth (K3). A pattern may hold `*`,
/// which matches within one path segment. `deploy` is not here: it is denied
/// at the repo root only, which the root allowlist already does, so a script
/// package or view named `deploy` below a module directory is kept.
pub const BUILTIN_DENY: &[&str] = &[
    ".git",
    ".github",
    ".ign",
    "node_modules",
    "__pycache__",
    "*.pyc",
];

/// Most entries the walk may visit before the build refuses (T-25-17).
pub const ARTIFACT_MAX_ENTRIES: usize = 200_000;

/// Whether `name` is a module directory name: `^[a-z][a-z0-9_-]*(\.[a-z0-9_-]+)+$`
/// (`com.inductiveautomation.perspective`). `ignition` is the one top-level
/// directory that does not match and is allowed by name instead (K3).
pub fn is_module_dir_name(name: &str) -> bool {
    let mut parts = name.split('.');
    let Some(first) = parts.next() else {
        return false;
    };
    let tail_ok = |part: &str| {
        part.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
    };
    let mut chars = first.chars();
    let head_ok = chars.next().is_some_and(|c| c.is_ascii_lowercase()) && tail_ok(chars.as_str());
    let mut rest = 0usize;
    for part in parts {
        if part.is_empty() || !tail_ok(part) {
            return false;
        }
        rest += 1;
    }
    head_ok && rest >= 1
}

/// Whether `name` matches `pattern`, where `*` matches any run of characters.
/// Both are single path segments, so `*` never crosses a `/`.
fn segment_glob(pattern: &str, name: &str) -> bool {
    let (p, n) = (pattern.as_bytes(), name.as_bytes());
    let (mut pi, mut ni) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ni < n.len() {
        if pi < p.len() && p[pi] == b'*' {
            star = Some((pi, ni));
            pi += 1;
        } else if pi < p.len() && p[pi] == n[ni] {
            pi += 1;
            ni += 1;
        } else if let Some((star_p, star_n)) = star {
            pi = star_p + 1;
            ni = star_n + 1;
            star = Some((star_p, star_n + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|b| *b == b'*')
}

/// Whether a walked name is on the any-depth denylist (K3).
fn is_denied(name: &str) -> bool {
    BUILTIN_DENY
        .iter()
        .any(|pattern| segment_glob(pattern, name))
}

/// One `.ignexclude` line: a repo-relative path, one glob per segment.
#[derive(Debug, Clone, PartialEq, Eq)]
struct IgnRule {
    segments: Vec<String>,
    /// A trailing `/`: matches directories only.
    dir_only: bool,
}

/// The parsed `.ignexclude` (D-09, K3). One repo-relative path per line; a
/// trailing `/` names a directory subtree; `*` matches within one segment;
/// `#` starts a comment line; blank lines are ignored; spaces and tabs around
/// a line are trimmed; one trailing CR per line is tolerated. There is no
/// negation: `!`, `**`, an absolute path, a `.` or `..` component, a
/// backslash or a control character refuse `deploy_tree_invalid` naming
/// `.ignexclude` and the line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IgnExclude {
    rules: Vec<IgnRule>,
}

impl IgnExclude {
    /// Parse the bytes of a `.ignexclude`.
    pub fn parse(bytes: &[u8]) -> Result<Self, CoreError> {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| invalid(IGNEXCLUDE_FILE, "is not valid UTF-8"))?;
        let mut rules = Vec::new();
        for (index, raw) in text.split('\n').enumerate() {
            let at = |message: &str| {
                CoreError::deploy_tree_invalid(
                    IGNEXCLUDE_FILE,
                    Some(index + 1),
                    Some(1),
                    message.to_string(),
                )
            };
            let line = raw.strip_suffix('\r').unwrap_or(raw);
            let line = line.trim_matches(|c| c == ' ' || c == '\t');
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.chars().any(char::is_control) {
                return Err(at("holds a control character"));
            }
            if line.starts_with('!') {
                return Err(at("negation (`!`) is not supported"));
            }
            if line.contains("**") {
                return Err(at("`**` is not supported; `*` matches within one segment"));
            }
            if line.starts_with('/') {
                return Err(at(
                    "an absolute path is not supported; use a path relative to the repo root",
                ));
            }
            if line.contains('\\') {
                return Err(at(
                    "a backslash is not supported; separate segments with `/`",
                ));
            }
            let dir_only = line.ends_with('/');
            let segments: Vec<String> = line
                .trim_end_matches('/')
                .split('/')
                .map(str::to_string)
                .collect();
            if segments.iter().any(|segment| segment == "..") {
                return Err(at("`..` is not supported; paths stay inside the repo"));
            }
            if segments.iter().any(|s| s.is_empty() || s == ".") {
                return Err(at("holds an empty or `.` path segment"));
            }
            rules.push(IgnRule { segments, dir_only });
        }
        Ok(Self { rules })
    }

    /// Whether the repo-relative path `rel` is excluded. A match on a
    /// directory leaves out its whole subtree, because the walk never
    /// descends into it.
    pub fn matches(&self, rel: &str, is_dir: bool) -> bool {
        let parts: Vec<&str> = rel.split('/').collect();
        self.rules.iter().any(|rule| {
            (is_dir || !rule.dir_only)
                && rule.segments.len() == parts.len()
                && rule
                    .segments
                    .iter()
                    .zip(&parts)
                    .all(|(pattern, part)| segment_glob(pattern, part))
        })
    }
}

/// Read the repo's `.ignexclude`; absent is no rules.
fn load_ignexclude(repo: &Path) -> Result<IgnExclude, CoreError> {
    match std::fs::symlink_metadata(repo.join(IGNEXCLUDE_FILE)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(IgnExclude::default()),
        Err(e) => Err(invalid(
            IGNEXCLUDE_FILE,
            format!("cannot inspect ({})", e.kind()),
        )),
        Ok(meta) if meta.file_type().is_file() => {
            IgnExclude::parse(&read_repo_file(repo, IGNEXCLUDE_FILE, None)?)
        }
        Ok(_) => Err(invalid(
            IGNEXCLUDE_FILE,
            "is not a regular file; symlinks are refused",
        )),
    }
}

/// `rel` as an `excluded` entry: a directory carries a trailing `/`.
fn excluded_entry(rel: &str, is_dir: bool) -> String {
    if is_dir {
        format!("{rel}/")
    } else {
        rel.to_string()
    }
}

fn top_level_allowed(name: &str) -> bool {
    name == "ignition" || is_module_dir_name(name)
}

/// Sort the top-level `files` array of a `resource.json` document in place
/// (K4 row 1): the gateway re-sorts it on import, so order carries no
/// meaning.
fn sort_files_array(value: &mut serde_json::Value) {
    if let Some(files) = value
        .as_object_mut()
        .and_then(|object| object.get_mut("files"))
        .and_then(|files| files.as_array_mut())
        && files.iter().all(|f| f.is_string())
    {
        files.sort_by(|a, b| {
            a.as_str()
                .unwrap_or_default()
                .as_bytes()
                .cmp(b.as_str().unwrap_or_default().as_bytes())
        });
    }
}

/// Drop an `attributes` key whose value is an empty object. The gateway adds
/// `attributes` (holding only the two volatile stamps) to a descriptor that
/// had none; once the stamps are normalised away an empty object is all that
/// is left, and it must not make a hand-authored descriptor read as changed.
fn drop_empty_attributes(value: &mut serde_json::Value) {
    if let Some(object) = value.as_object_mut()
        && object
            .get("attributes")
            .and_then(serde_json::Value::as_object)
            .is_some_and(serde_json::Map::is_empty)
    {
        object.remove("attributes");
    }
}

/// K4 row 1: `normalize_descriptor`, then an empty `attributes` object
/// dropped and the `files` array sorted. Bytes that are not a JSON object hash
/// raw.
fn canonical_resource_json(bytes: &[u8]) -> Vec<u8> {
    let Some(normalized) = normalize_descriptor(bytes) else {
        return bytes.to_vec();
    };
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&normalized) else {
        return bytes.to_vec();
    };
    drop_empty_attributes(&mut value);
    sort_files_array(&mut value);
    // `serde_json::Map` is a BTreeMap here, so keys stay sorted.
    serde_json::to_vec(&value).unwrap_or_else(|_| bytes.to_vec())
}

/// K4 row 2: `project.json` parsed, an absent (or null) `description` or
/// `parent` read as the empty string, keys sorted. The gateway adds both keys
/// on first import. Bytes that are not a JSON object hash raw.
fn canonical_project_json(bytes: &[u8]) -> Vec<u8> {
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return bytes.to_vec();
    };
    let Some(object) = value.as_object_mut() else {
        return bytes.to_vec();
    };
    for key in ["description", "parent"] {
        if object.get(key).is_none_or(serde_json::Value::is_null) {
            object.insert(key.to_string(), serde_json::Value::String(String::new()));
        }
    }
    serde_json::to_vec(&value).unwrap_or_else(|_| bytes.to_vec())
}

/// The canonical bytes of one member, chosen by its basename and place.
fn canonical_member_bytes(key: &str, bytes: &[u8]) -> Vec<u8> {
    if key == PROJECT_JSON {
        canonical_project_json(bytes)
    } else if key.rsplit('/').next() == Some(RESOURCE_JSON) {
        canonical_resource_json(bytes)
    } else {
        bytes.to_vec()
    }
}

/// The digest over a member map: `key NUL hash LF` lines in key byte order.
fn digest_of(members: &BTreeMap<String, String>) -> String {
    let mut listing = Vec::new();
    for (key, hash) in members {
        listing.extend_from_slice(key.as_bytes());
        listing.push(0);
        listing.extend_from_slice(hash.as_bytes());
        listing.push(b'\n');
    }
    sha256_hex(&listing)
}

/// Reduce a project zip to its canonical digest (K4). Every file entry is
/// hashed under its zip path; directory entries and entry order are ignored.
/// An empty member set is `deploy_tree_invalid`: an import of nothing would
/// empty the project (C8a), so it must never compare equal to anything.
pub fn canonical_digest(zip_bytes: &[u8]) -> Result<CanonicalDigest, CoreError> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))
        .map_err(|err| CoreError::Internal(format!("project zip is not readable: {err}")))?;
    let mut members = BTreeMap::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| CoreError::Internal(format!("cannot walk project zip: {err}")))?;
        if entry.is_dir() {
            continue;
        }
        let key = entry.name().to_string();
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|err| CoreError::Internal(format!("cannot decompress a zip member: {err}")))?;
        members.insert(
            key.clone(),
            sha256_hex(&canonical_member_bytes(&key, &bytes)),
        );
    }
    if members.is_empty() {
        return Err(CoreError::deploy_tree_invalid(
            PROJECT_JSON,
            None,
            None,
            "the project holds no members; an import of nothing would empty the project",
        ));
    }
    Ok(CanonicalDigest {
        sha256: digest_of(&members),
        members,
    })
}

fn invalid(rel: &str, message: impl Into<String>) -> CoreError {
    CoreError::deploy_tree_invalid(rel, None, None, message)
}

/// One directory entry of the walk, with its repo-relative path.
struct Walked {
    name: String,
    rel: String,
    kind: Kind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Dir,
    File,
}

/// What the walk carries: the repo, the `.ignexclude` rules, the members
/// found so far, the paths left out and the entries visited.
struct Cx<'a> {
    repo: &'a Path,
    ignore: &'a IgnExclude,
    files: BTreeMap<String, Vec<u8>>,
    excluded: Vec<String>,
    visited: usize,
    max_entries: usize,
}

impl Cx<'_> {
    /// Count one visited entry; refuse past the cap (T-25-17).
    fn visit(&mut self, rel: &str) -> Result<(), CoreError> {
        self.visited += 1;
        if self.visited > self.max_entries {
            return Err(invalid(
                rel,
                format!(
                    "the walk passed {} entries; the checkout is too large to deploy",
                    self.max_entries
                ),
            ));
        }
        Ok(())
    }
}

/// `name` as UTF-8, or a refusal naming its place below `rel` (lossily, so
/// the message is printable). Zip paths are UTF-8.
fn utf8_name(rel: &str, name: &std::ffi::OsStr) -> Result<String, CoreError> {
    name.to_str().map(str::to_string).ok_or_else(|| {
        invalid(
            &format!("{rel}/{}", name.to_string_lossy()),
            "the name is not valid UTF-8",
        )
    })
}

/// The sorted children of `rel` (a directory inside an allowed top-level
/// directory). Denylisted and `.ignexclude`d names are left out and recorded
/// in `cx.excluded`, never examined further. A symlink, a special file or a
/// name that is not a plain UTF-8 component refuses.
fn read_children(cx: &mut Cx<'_>, rel: &str) -> Result<Vec<Walked>, CoreError> {
    let unreadable =
        |e: std::io::Error| invalid(rel, format!("cannot read the directory ({})", e.kind()));
    let mut names = Vec::new();
    for entry in std::fs::read_dir(cx.repo.join(rel)).map_err(unreadable)? {
        let entry = entry.map_err(unreadable)?;
        let name = utf8_name(rel, &entry.file_name())?;
        names.push((name, entry));
    }
    names.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    let mut out = Vec::new();
    for (name, entry) in names {
        let child = format!("{rel}/{name}");
        cx.visit(&child)?;
        let listed_as_dir = entry.file_type().is_ok_and(|t| t.is_dir());
        if is_denied(&name) {
            cx.excluded.push(excluded_entry(&child, listed_as_dir));
            continue;
        }
        if name.contains('\\') || name.contains('\0') {
            return Err(invalid(
                &child,
                "the name holds a backslash or NUL; zip paths use `/` only",
            ));
        }
        if cx.ignore.matches(&child, listed_as_dir) {
            cx.excluded.push(excluded_entry(&child, listed_as_dir));
            continue;
        }
        let meta = std::fs::symlink_metadata(entry.path())
            .map_err(|e| invalid(&child, format!("cannot inspect ({})", e.kind())))?;
        let file_type = meta.file_type();
        if file_type.is_symlink() {
            return Err(invalid(&child, "is a symlink; symlinks are refused"));
        }
        let kind = if file_type.is_dir() {
            Kind::Dir
        } else if file_type.is_file() {
            Kind::File
        } else {
            return Err(invalid(&child, "is not a regular file or directory"));
        };
        out.push(Walked {
            name,
            rel: child,
            kind,
        });
    }
    Ok(out)
}

/// The `files` array of a `resource.json` as plain single-component names.
fn listed_files(rel: &str, bytes: &[u8]) -> Result<Vec<String>, CoreError> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| {
        invalid(
            rel,
            "is not valid JSON; the gateway would drop the resource",
        )
    })?;
    let Some(object) = value.as_object() else {
        return Err(invalid(rel, "must be a JSON object"));
    };
    let Some(files) = object.get("files") else {
        return Ok(Vec::new());
    };
    let Some(files) = files.as_array() else {
        return Err(invalid(rel, "`files` must be an array of file names"));
    };
    let mut out = Vec::new();
    for (i, item) in files.iter().enumerate() {
        let Some(name) = item.as_str() else {
            return Err(invalid(rel, format!("files[{i}] must be a string")));
        };
        if name.is_empty()
            || name == "."
            || name == ".."
            || name.contains('/')
            || name.contains('\\')
            || name.contains('\0')
        {
            return Err(invalid(
                rel,
                format!("files[{i}] must be a plain file name in the same folder"),
            ));
        }
        if name != RESOURCE_JSON && !out.iter().any(|n| n == name) {
            out.push(name.to_string());
        }
    }
    Ok(out)
}

/// Walk `rel` (a directory inside an allowed top-level directory), adding the
/// members of every resource folder to `cx.files` and everything else to
/// `cx.excluded`.
fn walk(cx: &mut Cx<'_>, rel: &str) -> Result<(), CoreError> {
    let children = read_children(cx, rel)?;
    let descriptor = children
        .iter()
        .find(|c| c.name == RESOURCE_JSON && c.kind == Kind::File);
    if let Some(descriptor) = descriptor {
        // A resource folder: exactly the descriptor and the files it lists.
        if let Some(dir) = children.iter().find(|c| c.kind == Kind::Dir) {
            return Err(invalid(
                &dir.rel,
                "is a directory inside a resource folder; the gateway would drop the whole resource",
            ));
        }
        let descriptor_bytes = read_repo_file(cx.repo, &descriptor.rel, None)?;
        let listed = listed_files(&descriptor.rel, &descriptor_bytes)?;
        for name in &listed {
            let Some(child) = children
                .iter()
                .find(|c| &c.name == name && c.kind == Kind::File)
            else {
                let why = if cx.ignore.matches(&format!("{rel}/{name}"), false) {
                    format!(
                        "lists `{name}`, which {IGNEXCLUDE_FILE} leaves out; the gateway would drop the whole resource"
                    )
                } else {
                    format!("lists `{name}`, which is not a file in the folder")
                };
                return Err(invalid(&descriptor.rel, why));
            };
            let bytes = read_repo_file(cx.repo, &child.rel, None)?;
            cx.files.insert(child.rel.clone(), bytes);
        }
        for child in &children {
            if child.name != RESOURCE_JSON && !listed.contains(&child.name) {
                cx.excluded.push(child.rel.clone());
            }
        }
        cx.files.insert(descriptor.rel.clone(), descriptor_bytes);
        return Ok(());
    }
    for child in &children {
        match child.kind {
            Kind::Dir => descend(cx, &child.rel)?,
            Kind::File => cx.excluded.push(child.rel.clone()),
        }
    }
    Ok(())
}

/// [`walk`] a directory; when nothing below it is a member, report the
/// directory once instead of what it holds.
fn descend(cx: &mut Cx<'_>, rel: &str) -> Result<(), CoreError> {
    let mark = cx.excluded.len();
    let before = cx.files.len();
    walk(cx, rel)?;
    if cx.files.len() == before {
        cx.excluded.truncate(mark);
        cx.excluded.push(excluded_entry(rel, true));
    }
    Ok(())
}

/// Sort out the repo root (K3): the allowed top-level directories are walked
/// (byte-sorted), every other root entry is recorded in `cx.excluded`. Root
/// entries outside the allowlist are not examined, so what they are (a
/// symlink, a file, a name that is not UTF-8) cannot refuse. An allowed name
/// that is a symlink refuses; one that is a file is left out.
fn root_dirs(cx: &mut Cx<'_>) -> Result<Vec<String>, CoreError> {
    let read = std::fs::read_dir(cx.repo)
        .map_err(|e| invalid(".", format!("cannot read the directory ({})", e.kind())))?;
    let mut names = Vec::new();
    for entry in read {
        let entry =
            entry.map_err(|e| invalid(".", format!("cannot read the directory ({})", e.kind())))?;
        let lossy = entry.file_name().to_string_lossy().into_owned();
        if lossy == PROJECT_JSON {
            continue;
        }
        cx.visit(&lossy)?;
        let meta = std::fs::symlink_metadata(entry.path());
        let is_dir = meta.as_ref().is_ok_and(|m| m.file_type().is_dir());
        let name = match entry.file_name().to_str() {
            Some(name) if top_level_allowed(name) && !is_denied(name) => name.to_string(),
            _ => {
                cx.excluded.push(excluded_entry(&lossy, is_dir));
                continue;
            }
        };
        if cx.ignore.matches(&name, is_dir) {
            cx.excluded.push(excluded_entry(&name, is_dir));
            continue;
        }
        let meta = meta.map_err(|e| invalid(&name, format!("cannot inspect ({})", e.kind())))?;
        if meta.file_type().is_symlink() {
            return Err(invalid(&name, "is a symlink; symlinks are refused"));
        }
        // `tags.json` matches the module-id shape: a file is simply not a
        // module directory.
        if is_dir {
            names.push(name);
        } else {
            cx.excluded.push(name);
        }
    }
    names.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    Ok(names)
}

/// Fixed entry options: Deflate, the zip epoch, fixed permission bits.
fn entry_options(directory: bool) -> zip::write::SimpleFileOptions {
    zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default())
        .unix_permissions(if directory { 0o755 } else { 0o644 })
}

/// Zip `files` (zip path to bytes) with a directory entry for every ancestor
/// folder, all entries in byte order of their zip path.
fn write_zip(files: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, CoreError> {
    let mut dirs = BTreeSet::new();
    for path in files.keys() {
        let mut end = 0;
        while let Some(at) = path[end..].find('/') {
            end += at + 1;
            dirs.insert(path[..end].to_string());
        }
    }
    let mut entries: Vec<(String, Option<&Vec<u8>>)> = Vec::new();
    entries.extend(dirs.into_iter().map(|d| (d, None)));
    entries.extend(files.iter().map(|(p, b)| (p.clone(), Some(b))));
    entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));

    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let zip_err = |err: &dyn std::fmt::Display| {
        CoreError::Internal(format!("cannot write the project zip: {err}"))
    };
    for (path, bytes) in entries {
        match bytes {
            None => writer
                .add_directory(path, entry_options(true))
                .map_err(|e| zip_err(&e))?,
            Some(bytes) => {
                writer
                    .start_file(path, entry_options(false))
                    .map_err(|e| zip_err(&e))?;
                writer.write_all(bytes).map_err(|e| zip_err(&e))?;
            }
        }
    }
    Ok(writer.finish().map_err(|e| zip_err(&e))?.into_inner())
}

/// The limits a build enforces. The real ones are [`ARTIFACT_MAX_ENTRIES`] and
/// `IMPORT_MAX_BYTES`; a test passes small ones so it can reach the refusals
/// without a half-gigabyte checkout.
#[derive(Debug, Clone, Copy)]
struct Limits {
    max_entries: usize,
    max_zip_bytes: usize,
}

impl Limits {
    const REAL: Limits = Limits {
        max_entries: ARTIFACT_MAX_ENTRIES,
        max_zip_bytes: IMPORT_MAX_BYTES,
    };
}

/// Build the artifact from the checkout at `repo` (D-09, K1 to K3).
pub fn build_artifact(repo: &Path) -> Result<Artifact, CoreError> {
    build_artifact_limited(repo, Limits::REAL)
}

fn build_artifact_limited(repo: &Path, limits: Limits) -> Result<Artifact, CoreError> {
    match std::fs::symlink_metadata(repo.join(PROJECT_JSON)) {
        Ok(meta) if meta.file_type().is_file() => {}
        Ok(meta) if meta.file_type().is_symlink() => {
            return Err(invalid(PROJECT_JSON, "is a symlink; symlinks are refused"));
        }
        _ => {
            return Err(invalid(
                PROJECT_JSON,
                "does not exist (a project repo holds project.json at its root)",
            ));
        }
    }
    let ignore = load_ignexclude(repo)?;
    let mut cx = Cx {
        repo,
        ignore: &ignore,
        files: BTreeMap::new(),
        excluded: Vec::new(),
        visited: 0,
        max_entries: limits.max_entries,
    };
    cx.files.insert(
        PROJECT_JSON.to_string(),
        read_repo_file(repo, PROJECT_JSON, None)?,
    );
    for name in root_dirs(&mut cx)? {
        descend(&mut cx, &name)?;
    }
    let Cx {
        files,
        mut excluded,
        ..
    } = cx;
    if files.len() <= 1 {
        // An overwrite import of project.json alone would empty the project
        // (C8a): nothing below the module directories is adoptable.
        return Err(invalid(
            PROJECT_JSON,
            "the checkout holds no adoptable member besides project.json; an import would empty the project",
        ));
    }
    excluded.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    excluded.dedup();
    let zip = write_zip(&files)?;
    if zip.len() > limits.max_zip_bytes {
        return Err(invalid(
            PROJECT_JSON,
            format!(
                "the project zip is {} bytes, over the {} byte import limit",
                zip.len(),
                limits.max_zip_bytes
            ),
        ));
    }
    let digest = canonical_digest(&zip)?;
    Ok(Artifact {
        members: files.len(),
        zip,
        digest,
        excluded,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(root: &Path, rel: &str, bytes: &[u8]) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, bytes).expect("write");
    }

    /// A checkout: root project.json, one script folder, one view folder, and
    /// noise the artifact must leave out.
    fn checkout() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let r = dir.path();
        put(r, "project.json", br#"{"title":"T","enabled":true}"#);
        put(
            r,
            "ignition/script-python/cap/resource.json",
            br#"{"scope":"A","files":["code.py"],"attributes":{"lastModification":{"actor":"x"}}}"#,
        );
        put(
            r,
            "ignition/script-python/cap/code.py",
            b"def f():\n  pass\n",
        );
        put(
            r,
            "com.inductiveautomation.perspective/views/V/resource.json",
            br#"{"files":["view.json"]}"#,
        );
        put(
            r,
            "com.inductiveautomation.perspective/views/V/view.json",
            b"{}",
        );
        // Noise.
        put(r, "ignition/script-python/cap/notes.txt", b"unlisted");
        put(r, "docs/readme.md", b"root entry outside the allowlist");
        put(r, ".github/workflows/x.yml", b"ci");
        put(r, "deploy/project.yaml", b"schema_version: 1\n");
        put(
            r,
            "ignition/script-python/loose/code.py",
            b"no resource.json",
        );
        dir
    }

    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .expect("start");
            writer.write_all(bytes).expect("write");
        }
        writer.finish().expect("finish").into_inner()
    }

    #[test]
    fn module_dir_names() {
        for ok in [
            "com.inductiveautomation.perspective",
            "com.inductiveautomation.alarm-notification",
            "a.b",
            "a1_b-c.d2",
        ] {
            assert!(is_module_dir_name(ok), "{ok}");
        }
        for bad in [
            "ignition", "", ".", "a.", ".a", "a..b", "A.b", "1a.b", "a.B", "deploy", "docs",
            "a b.c",
        ] {
            assert!(!is_module_dir_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn digest_counts_checkout_layout_members() {
        let zip = zip_of(&[
            ("project.json", br#"{"title":"T"}"#),
            (
                "ignition/script-python/a/resource.json",
                br#"{"files":["code.py"]}"#,
            ),
            ("ignition/script-python/a/code.py", b"x"),
        ]);
        let digest = canonical_digest(&zip).expect("digest");
        assert_eq!(digest.members.len(), 3, "never an empty set");
        assert_eq!(digest.sha256.len(), 64);
        assert!(
            digest
                .members
                .contains_key("ignition/script-python/a/code.py")
        );
    }

    #[test]
    fn digest_ignores_entry_order_directories_and_volatile_fields() {
        let a = zip_of(&[
            ("project.json", br#"{"title":"T"}"#),
            (
                "ignition/s/resource.json",
                br#"{"files":["b.py","a.py"],"attributes":{"lastModification":{"t":1},"lastModificationSignature":"s"}}"#,
            ),
            ("ignition/s/a.py", b"1"),
            ("ignition/s/b.py", b"2"),
        ]);
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opt = zip::write::SimpleFileOptions::default();
        writer.add_directory("ignition/", opt).expect("dir");
        for (name, bytes) in [
            ("ignition/s/b.py", b"2".as_slice()),
            ("ignition/s/a.py", b"1".as_slice()),
            (
                "ignition/s/resource.json",
                br#"{"attributes":{"lastModification":{"t":99}},"files":["a.py","b.py"]}"#
                    .as_slice(),
            ),
            (
                "project.json",
                br#"{"parent":"","description":"","title":"T"}"#.as_slice(),
            ),
        ] {
            writer.start_file(name, opt).expect("start");
            writer.write_all(bytes).expect("write");
        }
        let b = writer.finish().expect("finish").into_inner();
        assert_eq!(
            canonical_digest(&a).expect("a"),
            canonical_digest(&b).expect("b")
        );
    }

    #[test]
    fn an_empty_attributes_object_does_not_make_a_descriptor_changed() {
        // Hand-authored: no `attributes`. The gateway's export of it carries
        // `attributes` holding only the two stamps.
        let authored = zip_of(&[
            ("project.json", b"{}"),
            ("ignition/s/resource.json", br#"{"files":["a.py"]}"#),
            ("ignition/s/a.py", b"1"),
        ]);
        let exported = zip_of(&[
            ("project.json", b"{}"),
            (
                "ignition/s/resource.json",
                br#"{"files":["a.py"],"attributes":{"lastModification":{"t":1},"lastModificationSignature":"s"}}"#,
            ),
            ("ignition/s/a.py", b"1"),
        ]);
        assert_eq!(
            canonical_digest(&authored).expect("authored"),
            canonical_digest(&exported).expect("exported")
        );
    }

    #[test]
    fn a_non_empty_attributes_object_still_counts() {
        let plain = zip_of(&[
            ("project.json", b"{}"),
            ("ignition/s/resource.json", br#"{"files":[]}"#),
        ]);
        let flagged = zip_of(&[
            ("project.json", b"{}"),
            (
                "ignition/s/resource.json",
                br#"{"files":[],"attributes":{"enabled":true}}"#,
            ),
        ]);
        assert_ne!(
            canonical_digest(&plain).expect("plain").sha256,
            canonical_digest(&flagged).expect("flagged").sha256
        );
    }

    #[test]
    fn digest_sees_a_real_change() {
        let one = zip_of(&[("project.json", b"{}"), ("ignition/a/code.py", b"1")]);
        let two = zip_of(&[("project.json", b"{}"), ("ignition/a/code.py", b"2")]);
        assert_ne!(
            canonical_digest(&one).expect("one").sha256,
            canonical_digest(&two).expect("two").sha256
        );
    }

    #[test]
    fn empty_member_set_is_refused() {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        writer
            .add_directory("ignition/", zip::write::SimpleFileOptions::default())
            .expect("dir");
        let only_dirs = writer.finish().expect("finish").into_inner();
        let err = canonical_digest(&only_dirs).expect_err("empty");
        assert_eq!(err.code(), "deploy_tree_invalid");
    }

    #[test]
    fn build_is_byte_identical_twice() {
        let dir = checkout();
        let first = build_artifact(dir.path()).expect("first");
        let second = build_artifact(dir.path()).expect("second");
        assert_eq!(first.zip, second.zip);
        assert_eq!(first.digest, second.digest);
    }

    #[test]
    fn non_module_root_dir_is_left_out() {
        let dir = checkout();
        let artifact = build_artifact(dir.path()).expect("artifact");
        let keys: Vec<&str> = artifact.digest.members.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            [
                "com.inductiveautomation.perspective/views/V/resource.json",
                "com.inductiveautomation.perspective/views/V/view.json",
                "ignition/script-python/cap/code.py",
                "ignition/script-python/cap/resource.json",
                "project.json",
            ]
        );
        assert_eq!(artifact.members, 5);
    }

    #[test]
    fn build_digest_equals_the_digest_of_an_equivalent_export() {
        let dir = checkout();
        let artifact = build_artifact(dir.path()).expect("artifact");
        let export = zip_of(&[
            (
                "project.json",
                br#"{"title":"T","enabled":true,"description":"","parent":""}"#,
            ),
            (
                "ignition/script-python/cap/resource.json",
                br#"{"scope":"A","files":["code.py"],"attributes":{"lastModification":{"actor":"y"},"lastModificationSignature":"z"}}"#,
            ),
            ("ignition/script-python/cap/code.py", b"def f():\n  pass\n"),
            (
                "com.inductiveautomation.perspective/views/V/resource.json",
                br#"{"files":["view.json"]}"#,
            ),
            ("com.inductiveautomation.perspective/views/V/view.json", b"{}"),
        ]);
        assert_eq!(
            artifact.digest,
            canonical_digest(&export).expect("export digest")
        );
    }

    fn excluded_of(dir: &tempfile::TempDir) -> Vec<String> {
        build_artifact(dir.path()).expect("artifact").excluded
    }

    #[test]
    fn segment_globs() {
        assert!(segment_glob("*.pyc", "code.cpython-311.pyc"));
        assert!(segment_glob("*.pyc", ".pyc"));
        assert!(!segment_glob("*.pyc", "code.pyc.txt"));
        assert!(segment_glob("a*b*c", "aXXbYYc"));
        assert!(segment_glob("a*b*c", "abc"));
        assert!(!segment_glob("a*b*c", "aXXbYY"));
        assert!(segment_glob("*", ""));
        assert!(segment_glob("exact", "exact"));
        assert!(!segment_glob("exact", "exacts"));
    }

    #[test]
    fn ignexclude_grammar() {
        let rules = IgnExclude::parse(
            b"# a comment\n\nfoo/\r\n  a/*.txt  \nignition/script-python/scratch\nb/*/c.py\n",
        )
        .expect("parse");
        // `foo/` is a directory subtree.
        assert!(rules.matches("foo", true));
        assert!(
            !rules.matches("foo", false),
            "a trailing / names a directory"
        );
        assert!(!rules.matches("foo/x", true), "the walk skips the subtree");
        // `*` stays within one segment.
        assert!(rules.matches("a/one.txt", false));
        assert!(!rules.matches("a/b/one.txt", false));
        assert!(!rules.matches("a/one.py", false));
        // A path without a trailing slash names a file or a directory.
        assert!(rules.matches("ignition/script-python/scratch", true));
        assert!(rules.matches("ignition/script-python/scratch", false));
        // A star in the middle segment.
        assert!(rules.matches("b/x/c.py", false));
        assert!(!rules.matches("b/c.py", false));
        // Comments and blank lines add nothing; CRLF parses like LF.
        assert_eq!(
            IgnExclude::parse(b"foo/\r\nbar\r\n").expect("crlf"),
            IgnExclude::parse(b"foo/\nbar\n").expect("lf")
        );
        assert!(
            !IgnExclude::parse(b"# only\n\n\n")
                .expect("empty")
                .matches("only", false)
        );
    }

    #[test]
    fn ignexclude_unsupported_constructs_refuse_with_the_line() {
        for (text, line) in [
            ("ok\n!x\n", 2),
            ("**/x\n", 1),
            ("a/**\n", 1),
            ("\n\n/abs\n", 3),
            ("../x\n", 1),
            ("a/../b\n", 1),
            ("a//b\n", 1),
            ("./a\n", 1),
            ("a\\b\n", 1),
            ("a\u{7}b\n", 1),
            ("a\tb\n", 1),
        ] {
            let err = IgnExclude::parse(text.as_bytes()).expect_err(text);
            assert_eq!(err.code(), "deploy_tree_invalid", "{text:?}");
            let shown = err.to_string();
            assert!(
                shown.contains(&format!(" {IGNEXCLUDE_FILE}:{line}:1:")),
                "{text:?}: {shown}"
            );
        }
        let err = IgnExclude::parse(&[0xff, 0xfe]).expect_err("not utf-8");
        assert!(err.to_string().contains(IGNEXCLUDE_FILE), "{err}");
    }

    #[test]
    fn builtin_deny_any_depth() {
        let dir = checkout();
        let r = dir.path();
        for rel in [
            "ignition/.git/HEAD",
            "ignition/.github/x.yml",
            "ignition/script-python/.ign/deploy/gw/run/export/x.zip",
            "ignition/script-python/cap/__pycache__/code.cpython-311.pyc",
            "ignition/script-python/cap/stale.pyc",
            "ignition/node_modules/m/index.js",
            "com.inductiveautomation.perspective/.ign/run.json",
            "com.inductiveautomation.perspective/views/__pycache__/x.pyc",
        ] {
            put(r, rel, b"x");
        }
        let artifact = build_artifact(r).expect("artifact");
        for key in artifact.digest.members.keys() {
            for part in key.split('/') {
                assert!(!is_denied(part), "{key} holds the denied name {part}");
            }
        }
        for want in [
            "ignition/.git/",
            "ignition/.github/",
            "ignition/script-python/.ign/",
            "ignition/script-python/cap/__pycache__/",
            "ignition/script-python/cap/stale.pyc",
            "ignition/node_modules/",
            "com.inductiveautomation.perspective/.ign/",
            "com.inductiveautomation.perspective/views/__pycache__/",
        ] {
            assert!(
                artifact.excluded.iter().any(|e| e == want),
                "{want} missing from {:?}",
                artifact.excluded
            );
        }
        // The same five members as without the noise.
        assert_eq!(artifact.members, 5);
    }

    #[test]
    fn a_deploy_directory_below_a_module_directory_is_kept() {
        let dir = checkout();
        put(
            dir.path(),
            "ignition/script-python/deploy/resource.json",
            br#"{"files":["code.py"]}"#,
        );
        put(dir.path(), "ignition/script-python/deploy/code.py", b"pass");
        let artifact = build_artifact(dir.path()).expect("artifact");
        assert!(
            artifact
                .digest
                .members
                .contains_key("ignition/script-python/deploy/code.py")
        );
    }

    #[test]
    fn excluded_lists_everything_left_out_byte_sorted() {
        let dir = checkout();
        let r = dir.path();
        put(r, "tags/t.json", b"{}");
        put(r, "images/i.png", b"x");
        put(r, ".ign/deploy/gw/run/export/x.zip", b"z");
        put(r, "README.md", b"root file");
        put(r, "tags.json", b"{}");
        assert_eq!(
            excluded_of(&dir),
            [
                ".github/",
                ".ign/",
                "README.md",
                "deploy/",
                "docs/",
                "ignition/script-python/cap/notes.txt",
                "ignition/script-python/loose/",
                "images/",
                "tags.json",
                "tags/",
            ]
        );
    }

    #[test]
    fn a_folder_without_a_descriptor_is_listed_once_not_by_contents() {
        let dir = checkout();
        put(
            dir.path(),
            "ignition/script-python/loose/deep/more/a.py",
            b"x",
        );
        let excluded = excluded_of(&dir);
        assert!(excluded.contains(&"ignition/script-python/loose/".to_string()));
        assert!(excluded.iter().all(|e| !e.contains("loose/deep")));
    }

    #[test]
    fn ignexclude_removes_paths_and_changes_the_digest() {
        let dir = checkout();
        let r = dir.path();
        put(
            r,
            "ignition/script-python/scratch/resource.json",
            br#"{"files":["s.py"]}"#,
        );
        put(r, "ignition/script-python/scratch/s.py", b"pass");
        let with_scratch = build_artifact(r).expect("without .ignexclude");
        assert!(
            with_scratch
                .digest
                .members
                .contains_key("ignition/script-python/scratch/s.py")
        );

        put(
            r,
            ".ignexclude",
            b"# scratch\nignition/script-python/scratch/\n",
        );
        let without = build_artifact(r).expect("with .ignexclude");
        assert!(
            without
                .digest
                .members
                .keys()
                .all(|k| !k.contains("scratch"))
        );
        assert!(
            without
                .excluded
                .contains(&"ignition/script-python/scratch/".to_string())
        );
        assert!(without.excluded.contains(&".ignexclude".to_string()));
        assert_ne!(with_scratch.digest.sha256, without.digest.sha256);
    }

    #[test]
    fn ignexclude_reaches_module_directories_and_single_files() {
        let dir = checkout();
        let r = dir.path();
        put(r, ".ignexclude", b"com.inductiveautomation.perspective/\n");
        let artifact = build_artifact(r).expect("artifact");
        assert!(
            artifact
                .digest
                .members
                .keys()
                .all(|k| !k.starts_with("com.inductiveautomation.perspective"))
        );
        assert!(
            artifact
                .excluded
                .contains(&"com.inductiveautomation.perspective/".to_string())
        );
    }

    #[test]
    fn a_bad_ignexclude_refuses_naming_the_file_and_line() {
        let dir = checkout();
        put(dir.path(), ".ignexclude", b"ok/\n!no\n");
        let err = build_artifact(dir.path()).expect_err("negation");
        assert_eq!(err.code(), "deploy_tree_invalid");
        assert!(err.to_string().contains(" .ignexclude:2:1:"), "{err}");
    }

    #[test]
    fn ignexclude_leaving_out_a_listed_file_refuses() {
        let dir = checkout();
        put(
            dir.path(),
            ".ignexclude",
            b"ignition/script-python/cap/code.py\n",
        );
        let err = build_artifact(dir.path()).expect_err("listed file removed");
        assert_eq!(err.code(), "deploy_tree_invalid");
        let text = err.to_string();
        assert!(text.contains("`code.py`"), "{text}");
        assert!(text.contains(IGNEXCLUDE_FILE), "{text}");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_ignexclude_is_refused() {
        let dir = checkout();
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join("x"), b"a\n").expect("write");
        std::os::unix::fs::symlink(outside.path().join("x"), dir.path().join(".ignexclude"))
            .expect("symlink");
        let err = build_artifact(dir.path()).expect_err("symlink");
        assert!(err.to_string().contains(IGNEXCLUDE_FILE), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn an_ignexcluded_symlink_is_left_out_not_refused() {
        let dir = checkout();
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join("secret"), b"s").expect("write");
        std::os::unix::fs::symlink(
            outside.path().join("secret"),
            dir.path().join("ignition/script-python/cap/link.py"),
        )
        .expect("symlink");
        put(
            dir.path(),
            ".ignexclude",
            b"ignition/script-python/cap/link.py\n",
        );
        let artifact = build_artifact(dir.path()).expect("artifact");
        assert!(
            artifact
                .excluded
                .contains(&"ignition/script-python/cap/link.py".to_string())
        );
    }

    #[test]
    fn entry_cap_refuses() {
        let dir = checkout();
        let tight = Limits {
            max_entries: 3,
            ..Limits::REAL
        };
        let err = build_artifact_limited(dir.path(), tight).expect_err("cap");
        assert_eq!(err.code(), "deploy_tree_invalid");
        assert!(err.to_string().contains("too large"), "{err}");
        build_artifact(dir.path()).expect("under the real cap");
    }

    #[test]
    fn size_cap_refuses() {
        let dir = checkout();
        let built = build_artifact(dir.path()).expect("artifact");
        let at_limit = Limits {
            max_zip_bytes: built.zip.len(),
            ..Limits::REAL
        };
        build_artifact_limited(dir.path(), at_limit).expect("exactly at the limit passes");
        let one_under = Limits {
            max_zip_bytes: built.zip.len() - 1,
            ..Limits::REAL
        };
        let err = build_artifact_limited(dir.path(), one_under).expect_err("size cap");
        assert_eq!(err.code(), "deploy_tree_invalid");
        assert!(err.to_string().contains("import limit"), "{err}");
    }

    #[test]
    fn the_real_limits_are_the_documented_ones() {
        assert_eq!(Limits::REAL.max_entries, 200_000);
        assert_eq!(Limits::REAL.max_zip_bytes, 512 * 1024 * 1024);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_is_refused() {
        let dir = checkout();
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join("secret"), b"s").expect("write");
        std::os::unix::fs::symlink(
            outside.path().join("secret"),
            dir.path().join("ignition/script-python/cap/link.py"),
        )
        .expect("symlink");
        let err = build_artifact(dir.path()).expect_err("symlink");
        assert_eq!(err.code(), "deploy_tree_invalid");
        let text = err.to_string();
        assert!(
            text.contains("ignition/script-python/cap/link.py"),
            "{text}"
        );
        assert!(text.contains("symlink"), "{text}");
    }

    #[cfg(unix)]
    #[test]
    fn symlink_among_excluded_root_entries_is_left_out() {
        let dir = checkout();
        let outside = tempfile::tempdir().expect("outside");
        std::os::unix::fs::symlink(outside.path(), dir.path().join("docs-link")).expect("symlink");
        let artifact = build_artifact(dir.path()).expect("an excluded root symlink is not walked");
        assert_eq!(artifact.members, 5);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_allowed_top_level_directory_is_refused() {
        let dir = checkout();
        let outside = tempfile::tempdir().expect("outside");
        std::os::unix::fs::symlink(outside.path(), dir.path().join("com.example.mod"))
            .expect("symlink");
        let err = build_artifact(dir.path()).expect_err("symlinked module dir");
        assert!(err.to_string().contains("com.example.mod"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_project_json_is_refused() {
        let dir = checkout();
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join("p.json"), b"{}").expect("write");
        std::fs::remove_file(dir.path().join("project.json")).expect("rm");
        std::os::unix::fs::symlink(
            outside.path().join("p.json"),
            dir.path().join("project.json"),
        )
        .expect("symlink");
        assert!(build_artifact(dir.path()).is_err());
    }

    #[test]
    fn missing_project_json_refuses() {
        let dir = checkout();
        std::fs::remove_file(dir.path().join("project.json")).expect("rm");
        let err = build_artifact(dir.path()).expect_err("no project.json");
        assert_eq!(err.code(), "deploy_tree_invalid");
        assert!(
            err.to_string()
                .contains("a project repo holds project.json at its root"),
            "{err}"
        );
    }

    #[test]
    fn directory_inside_a_resource_folder_refuses() {
        let dir = checkout();
        put(dir.path(), "ignition/script-python/cap/sub/x.txt", b"x");
        let err = build_artifact(dir.path()).expect_err("nested dir");
        let text = err.to_string();
        assert!(text.contains("ignition/script-python/cap/sub"), "{text}");
        assert!(text.contains("inside a resource folder"), "{text}");
    }

    #[test]
    fn denylisted_directory_inside_a_resource_folder_is_left_out() {
        let dir = checkout();
        put(
            dir.path(),
            "ignition/script-python/cap/__pycache__/code.cpython-311.pyc",
            b"x",
        );
        let artifact = build_artifact(dir.path()).expect("artifact");
        assert!(
            artifact
                .digest
                .members
                .keys()
                .all(|k| !k.contains("__pycache__"))
        );
    }

    #[test]
    fn listed_but_missing_file_refuses() {
        let dir = checkout();
        std::fs::remove_file(dir.path().join("ignition/script-python/cap/code.py")).expect("rm");
        let err = build_artifact(dir.path()).expect_err("listed missing");
        assert_eq!(err.code(), "deploy_tree_invalid");
        assert!(err.to_string().contains("`code.py`"), "{err}");
    }

    #[test]
    fn listed_missing_refuses() {
        let dir = checkout();
        put(
            dir.path(),
            "ignition/script-python/cap/resource.json",
            br#"{"scope":"A","files":["code.py","extra.py"]}"#,
        );
        let err = build_artifact(dir.path()).expect_err("extra.py is absent");
        assert_eq!(err.code(), "deploy_tree_invalid");
        let text = err.to_string();
        assert!(text.contains("`extra.py`"), "{text}");
        assert!(
            text.contains("ignition/script-python/cap/resource.json"),
            "the location is repo-relative: {text}"
        );
        assert!(
            !text.contains("def f()"),
            "no file content in the refusal: {text}"
        );
    }

    #[test]
    fn only_project_json_refuses() {
        let dir = tempfile::tempdir().expect("tempdir");
        put(dir.path(), "project.json", br#"{"title":"T"}"#);
        let err = build_artifact(dir.path()).expect_err("project.json alone");
        assert_eq!(err.code(), "deploy_tree_invalid");
        assert!(err.to_string().contains("besides project.json"), "{err}");

        // Module directories that hold nothing adoptable are the same case.
        put(dir.path(), "ignition/script-python/loose/code.py", b"x");
        put(dir.path(), "docs/readme.md", b"x");
        let err = build_artifact(dir.path()).expect_err("nothing adoptable");
        assert_eq!(err.code(), "deploy_tree_invalid");

        // One adopted resource is enough.
        put(
            dir.path(),
            "ignition/script-python/ok/resource.json",
            br#"{"files":[]}"#,
        );
        assert_eq!(build_artifact(dir.path()).expect("one member").members, 2);
    }

    #[test]
    fn an_ignexclude_that_removes_every_member_refuses() {
        let dir = checkout();
        put(dir.path(), ".ignexclude", b"ignition/\ncom.*/\n");
        let err = build_artifact(dir.path()).expect_err("nothing left");
        assert_eq!(err.code(), "deploy_tree_invalid");
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_name_refuses() {
        use std::os::unix::ffi::OsStrExt;
        let bad = std::ffi::OsStr::from_bytes(b"bad-\xff.py");
        let err = utf8_name("ignition/script-python/cap", bad).expect_err("non-UTF-8");
        assert_eq!(err.code(), "deploy_tree_invalid");
        let text = err.to_string();
        assert!(text.contains("not valid UTF-8"), "{text}");
        assert!(text.contains("ignition/script-python/cap/bad-"), "{text}");
        assert_eq!(
            utf8_name("a", std::ffi::OsStr::new("ok.py")).expect("utf-8"),
            "ok.py"
        );

        // Through the walk, on a filesystem that lets the name exist.
        let dir = checkout();
        let path = dir.path().join("ignition/script-python/cap").join(bad);
        if std::fs::write(&path, b"x").is_ok() {
            let err = build_artifact(dir.path()).expect_err("walk refuses");
            assert_eq!(err.code(), "deploy_tree_invalid");
            assert!(err.to_string().contains("not valid UTF-8"), "{err}");
        } else {
            // APFS refuses to create the name (EILSEQ), so it cannot be in a
            // checkout there; the pure check above is the coverage.
            assert!(!path.exists());
        }
    }

    #[test]
    fn unlisted_and_dot_files_are_left_out_but_listed_dot_files_ride() {
        let dir = checkout();
        put(dir.path(), "ignition/script-python/cap/.gitkeep", b"");
        put(
            dir.path(),
            "ignition/script-python/cap/resource.json",
            br#"{"files":["code.py",".gitkeep"]}"#,
        );
        let artifact = build_artifact(dir.path()).expect("artifact");
        let keys: Vec<&String> = artifact.digest.members.keys().collect();
        assert!(keys.iter().any(|k| k.ends_with("cap/.gitkeep")));
        assert!(keys.iter().all(|k| !k.ends_with("notes.txt")));
    }

    #[test]
    fn a_webdev_resources_directory_keeps_its_name() {
        let dir = checkout();
        put(
            dir.path(),
            "com.inductiveautomation.webdev/resources/route/resource.json",
            br#"{"files":["doGet.py"]}"#,
        );
        put(
            dir.path(),
            "com.inductiveautomation.webdev/resources/route/doGet.py",
            b"pass",
        );
        let artifact = build_artifact(dir.path()).expect("artifact");
        assert!(
            artifact
                .digest
                .members
                .contains_key("com.inductiveautomation.webdev/resources/route/doGet.py")
        );
    }

    #[test]
    fn zip_holds_a_directory_entry_for_every_ancestor_in_byte_order() {
        let dir = checkout();
        let artifact = build_artifact(dir.path()).expect("artifact");
        let mut archive =
            zip::ZipArchive::new(std::io::Cursor::new(artifact.zip.as_slice())).expect("zip");
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).expect("entry").name().to_string())
            .collect();
        assert!(names.contains(&"ignition/".to_string()));
        assert!(names.contains(&"ignition/script-python/cap/".to_string()));
        let mut sorted = names.clone();
        sorted.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        assert_eq!(names, sorted);
    }

    // -- digest_invariants -------------------------------------------------

    use proptest::prelude::*;

    /// One generated project: file names with contents, and the knobs a
    /// second runner or a gateway round trip may turn.
    #[derive(Debug, Clone)]
    struct Shape {
        files: Vec<(String, Vec<u8>)>,
        order: Vec<u64>,
        files_order: Vec<u64>,
        key_order: Vec<u64>,
        deflate: bool,
        with_attrs: bool,
        stamp: (u32, String, String),
    }

    fn shape() -> impl Strategy<Value = Shape> {
        (
            proptest::collection::btree_map(
                "[a-z]{1,6}\\.py",
                proptest::collection::vec(any::<u8>(), 1..24),
                1..6,
            ),
            proptest::collection::vec(any::<u64>(), 16),
            proptest::collection::vec(any::<u64>(), 16),
            proptest::collection::vec(any::<u64>(), 3),
            any::<bool>(),
            any::<bool>(),
            (any::<u32>(), "[a-zA-Z0-9]{0,12}", "[a-zA-Z0-9+/=]{0,24}"),
        )
            .prop_map(
                |(files, order, files_order, key_order, deflate, with_attrs, stamp)| Shape {
                    files: files.into_iter().collect(),
                    order,
                    files_order,
                    key_order,
                    deflate,
                    with_attrs,
                    stamp,
                },
            )
    }

    /// `items` sorted by the random keys, cycling when there are fewer keys.
    fn shuffled<T: Clone>(items: &[T], keys: &[u64]) -> Vec<T> {
        let mut keyed: Vec<(u64, usize)> = (0..items.len())
            .map(|i| (keys[i % keys.len()].rotate_left(i as u32 % 64), i))
            .collect();
        keyed.sort();
        keyed.into_iter().map(|(_, i)| items[i].clone()).collect()
    }

    /// The `resource.json` text: top-level keys in a shuffled order, the
    /// `files` array in a shuffled order, volatile stamps when asked.
    fn descriptor(shape: &Shape, names: &[String]) -> Vec<u8> {
        let listed = shuffled(names, &shape.files_order);
        let files = serde_json::to_string(&listed).expect("files");
        let (millis, actor, signature) = &shape.stamp;
        let mut parts = vec![format!("\"files\":{files}"), "\"scope\":\"A\"".to_string()];
        if shape.with_attrs {
            parts.push(format!(
                "\"attributes\":{{\"lastModificationSignature\":{sig},\"lastModification\":{{\"actor\":{act},\"timestamp\":\"{millis}\"}}}}",
                sig = serde_json::to_string(signature).expect("sig"),
                act = serde_json::to_string(actor).expect("actor"),
            ));
        }
        let parts = shuffled(&parts, &shape.key_order);
        format!("{{{}}}", parts.join(",")).into_bytes()
    }

    /// The project as a zip. `rename` swaps one member's file name; `flip`
    /// changes one byte of one file's content.
    fn project_zip(shape: &Shape, rename: Option<usize>, flip: Option<usize>) -> Vec<u8> {
        let mut files = shape.files.clone();
        if let Some(i) = rename {
            let i = i % files.len();
            files[i].0 = format!("{}_r.py", files[i].0.trim_end_matches(".py"));
        }
        if let Some(i) = flip {
            let i = i % files.len();
            let at = i % files[i].1.len();
            files[i].1[at] ^= 0x5a;
        }
        let names: Vec<String> = files.iter().map(|(n, _)| n.clone()).collect();
        let mut entries: Vec<(String, Vec<u8>)> = vec![
            ("project.json".to_string(), br#"{"title":"T"}"#.to_vec()),
            (
                "ignition/script-python/p/resource.json".to_string(),
                descriptor(shape, &names),
            ),
        ];
        for (name, bytes) in files {
            entries.push((format!("ignition/script-python/p/{name}"), bytes));
        }
        let entries = shuffled(&entries, &shape.order);
        let method = if shape.deflate {
            zip::CompressionMethod::Deflated
        } else {
            zip::CompressionMethod::Stored
        };
        let options = zip::write::SimpleFileOptions::default().compression_method(method);
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            writer.start_file(name, options).expect("start");
            writer.write_all(&bytes).expect("write");
        }
        writer.finish().expect("finish").into_inner()
    }

    /// The reference form: sorted everything, stored, no stamps, no
    /// attributes.
    fn reference(shape: &Shape) -> Shape {
        Shape {
            order: vec![0],
            files_order: vec![0],
            key_order: vec![0],
            deflate: false,
            with_attrs: false,
            stamp: (0, String::new(), String::new()),
            ..shape.clone()
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// DPRJ-03: the same members give the same digest in any entry
        /// order, with either compression method, with any
        /// `lastModification` and signature values (or none), with any key
        /// order and `files` order inside `resource.json`.
        #[test]
        fn digest_invariants(shape in shape()) {
            let base = canonical_digest(&project_zip(&reference(&shape), None, None)).expect("base");
            let variant = canonical_digest(&project_zip(&shape, None, None)).expect("variant");
            prop_assert_eq!(&base.sha256, &variant.sha256);
            prop_assert_eq!(&base.members, &variant.members);
        }

        /// A one-byte content change and a member rename each change the digest.
        #[test]
        fn digest_sees_a_byte_change_and_a_rename(shape in shape(), pick in 0usize..64) {
            let base = canonical_digest(&project_zip(&shape, None, None)).expect("base");
            let flipped = canonical_digest(&project_zip(&shape, None, Some(pick))).expect("flipped");
            prop_assert_ne!(&base.sha256, &flipped.sha256);
            let renamed = canonical_digest(&project_zip(&shape, Some(pick), None)).expect("renamed");
            prop_assert_ne!(&base.sha256, &renamed.sha256);
        }
    }
}
