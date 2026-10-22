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
//!   never part of the walk, at any depth;
//! - a symlink or special file in the walk refuses `deploy_tree_invalid`
//!   naming its repo-relative path, and is never followed.
//!
//! The build is deterministic: entries in byte order of their zip path, one
//! fixed timestamp and permission bits, Deflate. Two builds of one checkout
//! are byte-identical.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::Path;

use super::tree::{read_repo_file, sha256_hex};
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
}

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

/// Whether a walked name is on the any-depth denylist (K3).
fn is_denied(name: &str) -> bool {
    matches!(
        name,
        ".git" | ".github" | ".ign" | "node_modules" | "__pycache__"
    ) || name.ends_with(".pyc")
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

/// K4 row 1: `normalize_descriptor`, then the `files` array sorted. Bytes
/// that are not a JSON object hash raw.
fn canonical_resource_json(bytes: &[u8]) -> Vec<u8> {
    let Some(normalized) = normalize_descriptor(bytes) else {
        return bytes.to_vec();
    };
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&normalized) else {
        return bytes.to_vec();
    };
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

/// The sorted children of `rel` (a directory inside an allowed top-level
/// directory), denylisted names left out. A symlink, a special file or a name
/// that is not a plain UTF-8 component refuses.
fn read_children(repo: &Path, rel: &str) -> Result<Vec<Walked>, CoreError> {
    let unreadable =
        |e: std::io::Error| invalid(rel, format!("cannot read the directory ({})", e.kind()));
    let mut names = Vec::new();
    for entry in std::fs::read_dir(repo.join(rel)).map_err(unreadable)? {
        let entry = entry.map_err(unreadable)?;
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            return Err(invalid(
                &format!("{rel}/{}", entry.file_name().to_string_lossy()),
                "the name is not valid UTF-8",
            ));
        };
        names.push((name, entry));
    }
    names.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    let mut out = Vec::new();
    for (name, entry) in names {
        if is_denied(&name) {
            continue;
        }
        let child = format!("{rel}/{name}");
        if name.contains('\\') || name.contains('\0') {
            return Err(invalid(
                &child,
                "the name holds a backslash or NUL; zip paths use `/` only",
            ));
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
/// members of every resource folder to `files`.
fn walk(repo: &Path, rel: &str, files: &mut BTreeMap<String, Vec<u8>>) -> Result<(), CoreError> {
    let children = read_children(repo, rel)?;
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
        let descriptor_bytes = read_repo_file(repo, &descriptor.rel, None)?;
        let listed = listed_files(&descriptor.rel, &descriptor_bytes)?;
        for name in &listed {
            let Some(child) = children
                .iter()
                .find(|c| &c.name == name && c.kind == Kind::File)
            else {
                return Err(invalid(
                    &descriptor.rel,
                    format!("lists `{name}`, which is not a file in the folder"),
                ));
            };
            files.insert(child.rel.clone(), read_repo_file(repo, &child.rel, None)?);
        }
        files.insert(descriptor.rel.clone(), descriptor_bytes);
        return Ok(());
    }
    for child in children.iter().filter(|c| c.kind == Kind::Dir) {
        walk(repo, &child.rel, files)?;
    }
    Ok(())
}

/// The allowed top-level directories of the repo (K3), byte-sorted. Root
/// entries outside the allowlist are not part of the walk, so what they are
/// (a symlink, a file, a name that is not UTF-8) is not examined. An allowed
/// name that is a symlink refuses; one that is a file is skipped.
fn allowed_top_level_dirs(repo: &Path) -> Result<Vec<String>, CoreError> {
    let read = std::fs::read_dir(repo)
        .map_err(|e| invalid(".", format!("cannot read the directory ({})", e.kind())))?;
    let mut names = Vec::new();
    for entry in read {
        let entry =
            entry.map_err(|e| invalid(".", format!("cannot read the directory ({})", e.kind())))?;
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if !top_level_allowed(&name) || is_denied(&name) {
            continue;
        }
        let meta = std::fs::symlink_metadata(entry.path())
            .map_err(|e| invalid(&name, format!("cannot inspect ({})", e.kind())))?;
        if meta.file_type().is_symlink() {
            return Err(invalid(&name, "is a symlink; symlinks are refused"));
        }
        // `project.json` and `tags.json` match the module-id shape: a file is
        // simply not a module directory.
        if meta.file_type().is_dir() {
            names.push(name);
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

/// Build the artifact from the checkout at `repo` (D-01, D-09, K1 to K3).
pub fn build_artifact(repo: &Path) -> Result<Artifact, CoreError> {
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
    let mut files = BTreeMap::new();
    files.insert(
        PROJECT_JSON.to_string(),
        read_repo_file(repo, PROJECT_JSON, None)?,
    );
    for name in allowed_top_level_dirs(repo)? {
        walk(repo, &name, &mut files)?;
    }
    let zip = write_zip(&files)?;
    let digest = canonical_digest(&zip)?;
    Ok(Artifact {
        members: files.len(),
        zip,
        digest,
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
}
