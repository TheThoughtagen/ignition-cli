//! The git module's native tag export, read from a project repo (Phase 26,
//! D-04, D-08, TAGDEP-01).
//!
//! A project that owns a tag provider `P` keeps it under `tags/<P>/` in the
//! individual-file layout `GitTagManager` writes: a folder is a directory,
//! every other node is a `<name>.json` file holding the node without its
//! `name`, UDT types live below `_types_/`, and file and directory names are
//! `%XX`-encoded where the tag name holds a character a filesystem cannot
//! carry ([`encode_fs_name`], [`decode_fs_name`]). Entries whose name starts
//! with `.` (`.tag-config.json`, `.tag-groups.json`, agent state) are not
//! nodes, as in the module's `readTagsFromDirectory`.
//!
//! The checkout is untrusted (T-26-11..13). [`read_provider`] never follows a
//! symlink ([`std::fs::symlink_metadata`] on every entry), refuses special
//! files, names that are unsafe on disk on Linux or Windows, decoded tag names
//! that are unsafe as one path segment (a `%2F`, `%5C` or `%2E%2E` form), two
//! entries giving one tag name, the legacy single-file layout, files over
//! [`NATIVE_FILE_MAX_BYTES`] and trees over [`NATIVE_FILES_MAX`] entries.
//! Every defect of one provider comes back in one `deploy_tree_invalid`,
//! labelled by repo file and never quoting file content.
//!
//! Pure and offline: the repo is read, nothing is contacted. Every file passes
//! the strict JSON reader ([`node::classify`]) before it becomes a
//! [`serde_json::Value`].

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use serde_json::Value;

use super::component_problem;
use super::node;
use super::tag_canon::{FileForm, GSON_HTML_ESCAPED, MODULE_FORM, in_types, write_file_form};
use super::transport::winps::windows_component_problem;
use super::tree::read_repo_file;
use crate::error::{CoreError, DeployFinding};

/// The repo directory that holds one directory per tag provider.
pub const TAGS_DIR: &str = "tags";

/// The folder of a provider that holds its UDT types.
pub const TYPES_DIR: &str = "_types_";

/// The module's settings file at `tags/` (included providers, policy).
pub const TAG_CONFIG_FILE: &str = ".tag-config.json";

/// The module's summary of a provider's tag groups, inside `tags/<P>/`.
pub const TAG_GROUPS_FILE: &str = ".tag-groups.json";

/// The largest native file read (16 MiB, as [`node::JSON_MAX_BYTES`]).
/// Checked on the file's metadata before a byte is read.
pub const NATIVE_FILE_MAX_BYTES: u64 = 16 * 1024 * 1024;

/// The most entries one provider's walk lists. A real provider is far below
/// it (about 3,500 files at the largest measured); the cap bounds a hostile
/// checkout.
pub const NATIVE_FILES_MAX: usize = 200_000;

/// The characters the module `%XX`-encodes besides `%` and control
/// characters (`GitTagManager.FS_RESERVED_CHARS`).
const FS_RESERVED_CHARS: &str = "<>:\"/\\|?*";

/// Encode a tag name as one file or directory name, as
/// `GitTagManager.encodeFsName` does: `%`, every character below U+0020 and
/// the Windows-reserved characters `< > : " / \ | ? *` become `%XX`
/// (uppercase hex); everything else, non-ASCII included, is kept.
pub fn encode_fs_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c == '%' || u32::from(c) < 0x20 || FS_RESERVED_CHARS.contains(c) {
            let _ = write!(out, "%{:02X}", u32::from(c));
        } else {
            out.push(c);
        }
    }
    out
}

/// Decode one `%XX`-encoded file or directory name, as
/// `GitTagManager.decodeFsName` does: each `%` followed by two hex digits
/// becomes the character with that code (U+0000 to U+00FF, as Java's
/// `(char) code` gives), and any other `%` is kept as it is. Java's
/// `Integer.parseInt` would also take a sign (`%+1`); that is not ported, so a
/// `%` followed by a sign stays literal.
pub fn decode_fs_name(fs_name: &str) -> String {
    if !fs_name.contains('%') {
        return fs_name.to_string();
    }
    let chars: Vec<char> = fs_name.chars().collect();
    let mut out = String::with_capacity(fs_name.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%'
            && i + 2 < chars.len()
            && let (Some(hi), Some(lo)) = (chars[i + 1].to_digit(16), chars[i + 2].to_digit(16))
        {
            out.push(char::from(u8::try_from(hi * 16 + lo).unwrap_or(0)));
            i += 3;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// `tags/.tag-config.json` as data (the module's `TagExportConfig`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagConfig {
    /// `includedProviders`. Empty (or no file) means every provider.
    pub included_providers: Vec<String>,
    /// `excludedTagPaths`, as written.
    pub excluded_tag_paths: Vec<String>,
    /// `collisionPolicy`, as written; `None` when the file does not state it.
    pub collision_policy: Option<String>,
}

impl TagConfig {
    /// Whether the module exports and imports `provider`, as
    /// `TagExportConfig.isProviderIncluded`: an empty list includes every
    /// provider. (`System` is never owned; `load_project` refuses it first.)
    pub fn includes(&self, provider: &str) -> bool {
        self.included_providers.is_empty() || self.included_providers.iter().any(|p| p == provider)
    }
}

/// Read `tags/.tag-config.json` below `repo`. No file is the module's
/// default ([`TagConfig::default`]). A file that is not a strict JSON object,
/// or whose known keys have the wrong type, refuses `deploy_tree_invalid`
/// naming it. Unknown keys are ignored.
pub fn read_tag_config(repo: &Path) -> Result<TagConfig, CoreError> {
    let rel = format!("{TAGS_DIR}/{TAG_CONFIG_FILE}");
    let bad = |why: &str| CoreError::deploy_tree_invalid(&rel, None, None, why);
    let Some(value) = read_optional_json(repo, &rel)? else {
        return Ok(TagConfig::default());
    };
    let Some(object) = value.as_object() else {
        return Err(bad("must be a JSON object (the git module's tag settings)"));
    };
    let strings = |key: &str| -> Result<Vec<String>, CoreError> {
        match object.get(key) {
            None | Some(Value::Null) => Ok(Vec::new()),
            Some(Value::Array(items)) => items
                .iter()
                .map(|item| {
                    item.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| bad(&format!("`{key}` must be an array of strings")))
                })
                .collect(),
            Some(_) => Err(bad(&format!("`{key}` must be an array of strings"))),
        }
    };
    let collision_policy = match object.get("collisionPolicy") {
        None | Some(Value::Null) => None,
        Some(Value::String(policy)) => Some(policy.clone()),
        Some(_) => return Err(bad("`collisionPolicy` must be a string")),
    };
    Ok(TagConfig {
        included_providers: strings("includedProviders")?,
        excluded_tag_paths: strings("excludedTagPaths")?,
        collision_policy,
    })
}

/// Read the strict JSON file `rel`, or `None` when nothing is at `rel`. A
/// symlink, a special file, a file over [`NATIVE_FILE_MAX_BYTES`] or invalid
/// JSON refuses `deploy_tree_invalid` naming `rel`.
fn read_optional_json(repo: &Path, rel: &str) -> Result<Option<Value>, CoreError> {
    match std::fs::symlink_metadata(repo.join(rel)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(CoreError::deploy_tree_invalid(
                rel,
                None,
                None,
                format!("could not be read ({:?})", e.kind()),
            ));
        }
        Ok(meta) if meta.len() > NATIVE_FILE_MAX_BYTES => {
            return Err(CoreError::deploy_tree_invalid(
                rel,
                None,
                None,
                format!("is over the {NATIVE_FILE_MAX_BYTES} byte file cap"),
            ));
        }
        Ok(_) => {}
    }
    let bytes = read_repo_file(repo, rel, None)?;
    parse_strict(&bytes)
        .map(Some)
        .map_err(|why| CoreError::deploy_tree_invalid(rel, None, None, why))
}

/// Parse `bytes` through the strict reader, then as a value.
fn parse_strict(bytes: &[u8]) -> Result<Value, String> {
    node::classify(bytes).map_err(|why| format!("is not valid JSON: {why}"))?;
    serde_json::from_slice(bytes).map_err(|_| "is not valid JSON".to_string())
}

/// What a node of the native layout is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// A directory: a folder node.
    Folder,
    /// A `.json` file outside `_types_/`: a tag or a UDT instance.
    Tag,
    /// A `.json` file below `_types_/`: a UDT type.
    Type,
}

/// One node of an owned provider as the repo holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct NativeNode {
    /// Provider-relative path: the decoded names joined with `/`. Types sit
    /// below `_types_/`.
    pub path: String,
    /// Folder, tag or type.
    pub kind: NodeKind,
    /// The node's own JSON object, without `name` (the module writes none). A
    /// folder is `{"tagType":"Folder"}`.
    pub json: serde_json::Value,
    /// The repo-relative file (or directory) the node was read from.
    pub file: String,
}

/// An owned provider as the repo holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct NativeProvider {
    /// The provider's name, as `deploy/project.yaml` lists it.
    pub name: String,
    /// Every node, keyed by [`NativeNode::path`]. The `_types_` folder itself
    /// is structural (the gateway always has it) and is not a node.
    pub nodes: BTreeMap<String, NativeNode>,
    /// `tags/<P>/.tag-groups.json` as JSON, when present (the module's lossy
    /// summary of the provider's tag groups; plan 26-10 reads it).
    pub groups: Option<serde_json::Value>,
}

/// Read `tags/<provider>/` below `repo`.
///
/// A missing directory refuses `deploy_tree_invalid` naming it: an absent
/// provider is never read as an empty one. So does a provider that
/// `tags/.tag-config.json` does not include (D-04), and every refusal listed
/// in the module doc. All defects are collected into one
/// `deploy_tree_invalid`. Entries are visited in byte order of their names,
/// so the same tree gives the same model.
pub fn read_provider(repo: &Path, provider: &str) -> Result<NativeProvider, CoreError> {
    read_provider_capped(repo, provider, NATIVE_FILES_MAX).map(|(provider, _)| provider)
}

/// [`read_provider`], also returning each node file's bytes as read, keyed by
/// its repo-relative file, for [`check_canonical_form`].
pub fn read_provider_with_raw(
    repo: &Path,
    provider: &str,
) -> Result<(NativeProvider, BTreeMap<String, Vec<u8>>), CoreError> {
    read_provider_capped(repo, provider, NATIVE_FILES_MAX)
}

/// [`read_provider_with_raw`] with the entry cap as a parameter.
fn read_provider_capped(
    repo: &Path,
    provider: &str,
    max_entries: usize,
) -> Result<(NativeProvider, BTreeMap<String, Vec<u8>>), CoreError> {
    let root_rel = format!("{TAGS_DIR}/{provider}");
    let mut walk = Walk {
        repo,
        nodes: BTreeMap::new(),
        raw: BTreeMap::new(),
        findings: Vec::new(),
        listed: 0,
        max_entries,
        stopped: false,
    };
    match read_tag_config(repo) {
        Ok(config) if !config.includes(provider) => walk.finding(
            &format!("{TAGS_DIR}/{TAG_CONFIG_FILE}"),
            format!(
                "does not list `{provider}` under includedProviders: the git module neither \
                 exports nor imports that provider, so deploy/project.yaml cannot own it"
            ),
        ),
        Ok(_) => {}
        Err(err) => walk.absorb(err)?,
    }
    let legacy_rel = format!("{TAGS_DIR}/{}.json", encode_fs_name(provider));
    let legacy = std::fs::symlink_metadata(repo.join(&legacy_rel)).is_ok();
    if legacy {
        walk.finding(&legacy_rel, legacy_message());
    }
    let mut groups = None;
    match std::fs::symlink_metadata(repo.join(&root_rel)) {
        Ok(meta) if meta.file_type().is_dir() => {
            if !walk.legacy_dir(&root_rel, provider) {
                walk.dir(&root_rel, "");
            }
            match read_optional_json(repo, &format!("{root_rel}/{TAG_GROUPS_FILE}")) {
                Ok(value) => groups = value,
                Err(err) => walk.absorb(err)?,
            }
        }
        Ok(_) => walk.finding(
            &root_rel,
            "is a symlink or not a directory (an owned tag provider is the git module's \
             tags/<provider>/ directory; symlinks are never followed)",
        ),
        Err(_) if legacy => {}
        Err(_) => walk.finding(
            &root_rel,
            "does not exist: deploy/project.yaml lists this provider under tag_providers, \
             and an owned provider is never read as empty",
        ),
    }
    if !walk.findings.is_empty() {
        return Err(CoreError::deploy_findings(walk.findings));
    }
    Ok((
        NativeProvider {
            name: provider.to_string(),
            nodes: walk.nodes,
            groups,
        },
        walk.raw,
    ))
}

/// The refusal of the legacy single-file layout.
fn legacy_message() -> &'static str {
    "is the git module's legacy single-file tag layout; an owned tag provider must use the \
     module's individual-file format (tags/<provider>/ holding one directory per folder and \
     one <name>.json file per tag), which the git module writes on export"
}

/// The state of one [`read_provider`] walk.
struct Walk<'a> {
    repo: &'a Path,
    nodes: BTreeMap<String, NativeNode>,
    raw: BTreeMap<String, Vec<u8>>,
    findings: Vec<DeployFinding>,
    /// Entries listed so far, hidden ones included.
    listed: usize,
    max_entries: usize,
    /// Set once the walk passed `max_entries`; nothing more is listed.
    stopped: bool,
}

impl Walk<'_> {
    fn finding(&mut self, file: &str, message: impl Into<String>) {
        self.findings.push(DeployFinding {
            file: file.to_string(),
            line: None,
            column: None,
            message: message.into(),
        });
    }

    /// Keep the findings of a `deploy_tree_invalid`; any other error is
    /// returned.
    fn absorb(&mut self, err: CoreError) -> Result<(), CoreError> {
        match err {
            CoreError::DeployTreeInvalid(details) => {
                self.findings.extend(details.findings);
                Ok(())
            }
            other => Err(other),
        }
    }

    /// Whether `tags/<P>/` is the legacy layout: no subdirectory and nothing
    /// but one file named after the provider. Records the finding.
    fn legacy_dir(&mut self, root_rel: &str, provider: &str) -> bool {
        let Ok(entries) = std::fs::read_dir(self.repo.join(root_rel)) else {
            return false;
        };
        let mut visible = Vec::new();
        for entry in entries.take(self.max_entries) {
            let Ok(entry) = entry else { return false };
            if !entry.file_name().to_string_lossy().starts_with('.') {
                visible.push(entry);
                if visible.len() > 1 {
                    return false;
                }
            }
        }
        let [only] = visible.as_slice() else {
            return false;
        };
        let file = format!("{}.json", encode_fs_name(provider));
        let is_file = std::fs::symlink_metadata(only.path()).is_ok_and(|m| m.file_type().is_file());
        if is_file && only.file_name().to_str() == Some(file.as_str()) {
            self.finding(&format!("{root_rel}/{file}"), legacy_message());
            return true;
        }
        false
    }

    /// List the directory `rel`, byte-sorted, counting every entry against
    /// the cap. `None` when the listing failed or the cap stopped the walk.
    fn list(&mut self, rel: &str) -> Option<Vec<String>> {
        let entries = match std::fs::read_dir(self.repo.join(rel)) {
            Ok(entries) => entries,
            Err(e) => {
                self.finding(rel, format!("could not be read ({:?})", e.kind()));
                return None;
            }
        };
        let mut names = Vec::new();
        for entry in entries {
            self.listed += 1;
            if self.listed > self.max_entries {
                self.stopped = true;
                let root = rel.split('/').take(2).collect::<Vec<_>>().join("/");
                self.finding(
                    &root,
                    format!(
                        "holds more than {} entries; the read stopped (a hostile or \
                         misplaced tree)",
                        self.max_entries
                    ),
                );
                return None;
            }
            match entry {
                Ok(entry) => match entry.file_name().into_string() {
                    Ok(name) if name.chars().any(char::is_control) => {
                        self.finding(rel, "holds an entry whose name has a control character");
                    }
                    Ok(name) => names.push(name),
                    Err(_) => self.finding(rel, "holds an entry whose name is not valid UTF-8"),
                },
                Err(e) => self.finding(rel, format!("could not be listed ({:?})", e.kind())),
            }
        }
        names.sort();
        Some(names)
    }

    /// Walk the directory `rel` (repo-relative), whose nodes sit at the
    /// provider-relative `prefix` (empty at the provider root).
    fn dir(&mut self, rel: &str, prefix: &str) {
        let Some(names) = self.list(rel) else {
            return;
        };
        let in_types = prefix == TYPES_DIR || prefix.starts_with(&format!("{TYPES_DIR}/"));
        // Decoded names seen in this folder, lowercased: tag names are
        // case-insensitive, so `A` and `a` (or `%41` and `A`) are one tag.
        let mut seen: BTreeMap<String, String> = BTreeMap::new();
        for fs_name in names {
            if self.stopped {
                return;
            }
            if fs_name.starts_with('.') {
                continue;
            }
            let entry_rel = format!("{rel}/{fs_name}");
            let meta = match std::fs::symlink_metadata(self.repo.join(&entry_rel)) {
                Ok(meta) => meta,
                Err(e) => {
                    self.finding(&entry_rel, format!("could not be read ({:?})", e.kind()));
                    continue;
                }
            };
            let kind = meta.file_type();
            if kind.is_symlink() {
                self.finding(
                    &entry_rel,
                    "is a symlink; symlinks are refused, never followed",
                );
                continue;
            }
            if !kind.is_dir() && !kind.is_file() {
                self.finding(&entry_rel, "is not a regular file or directory");
                continue;
            }
            if let Some(why) = component_problem(&fs_name)
                .map(str::to_string)
                .or_else(|| windows_component_problem(&fs_name))
            {
                self.finding(
                    &entry_rel,
                    format!("is not a safe file name on Linux and Windows: {why}"),
                );
                continue;
            }
            let stem = if kind.is_dir() {
                fs_name.as_str()
            } else if let Some(stem) = fs_name.strip_suffix(".json") {
                stem
            } else {
                self.finding(
                    &entry_rel,
                    "is not a .json file; an owned tag provider holds directories and .json \
                     files only (the git module's individual-file format)",
                );
                continue;
            };
            let name = decode_fs_name(stem);
            if let Some(why) = decoded_problem(&name) {
                self.finding(&entry_rel, why);
                continue;
            }
            if let Some(first) = seen.get(&name.to_lowercase()) {
                let first = first.clone();
                self.finding(
                    &entry_rel,
                    format!(
                        "gives the same tag name as {first} in this folder (tag names are \
                         case-insensitive)"
                    ),
                );
                continue;
            }
            seen.insert(name.to_lowercase(), entry_rel.clone());
            let path = join(prefix, &name);
            if kind.is_dir() {
                // The `_types_` folder at the provider root is structural.
                if !(prefix.is_empty() && name == TYPES_DIR) {
                    self.nodes.insert(
                        path.clone(),
                        NativeNode {
                            path: path.clone(),
                            kind: NodeKind::Folder,
                            json: serde_json::json!({ "tagType": "Folder" }),
                            file: entry_rel.clone(),
                        },
                    );
                }
                self.dir(&entry_rel, &path);
                continue;
            }
            if meta.len() > NATIVE_FILE_MAX_BYTES {
                self.finding(
                    &entry_rel,
                    format!("is over the {NATIVE_FILE_MAX_BYTES} byte file cap"),
                );
                continue;
            }
            let Some((json, bytes)) = self.read_node(&entry_rel, &name, in_types) else {
                continue;
            };
            self.raw.insert(entry_rel.clone(), bytes);
            self.nodes.insert(
                path.clone(),
                NativeNode {
                    path,
                    kind: if in_types {
                        NodeKind::Type
                    } else {
                        NodeKind::Tag
                    },
                    json,
                    file: entry_rel,
                },
            );
        }
    }

    /// Read one node file strictly: a JSON object with a string `tagType`
    /// that fits where it sits, and no `name` other than its file name (an
    /// equal `name` is dropped). `None` (with a finding) otherwise.
    fn read_node(&mut self, rel: &str, name: &str, in_types: bool) -> Option<(Value, Vec<u8>)> {
        let bytes = match read_repo_file(self.repo, rel, None) {
            Ok(bytes) => bytes,
            Err(err) => {
                self.finding(rel, format!("could not be read ({})", err.code()));
                return None;
            }
        };
        let mut value = match parse_strict(&bytes) {
            Ok(value) => value,
            Err(why) => {
                self.finding(rel, why);
                return None;
            }
        };
        let Some(object) = value.as_object_mut() else {
            self.finding(rel, "must be a JSON object (one tag node)");
            return None;
        };
        if let Some(own) = object.get("name") {
            if own.as_str() != Some(name) {
                self.finding(
                    rel,
                    "holds a `name` key that differs from its file name; the git module names \
                     a node by its file name",
                );
                return None;
            }
            object.remove("name");
        }
        let problem = match object.get("tagType").and_then(Value::as_str) {
            None => Some("has no string `tagType`"),
            Some("UdtType") if !in_types => {
                Some("holds a UdtType outside _types_/; the git module writes types below _types_/")
            }
            Some("Provider") => {
                Some("holds a Provider node; a provider is the tags/<provider>/ directory")
            }
            Some(other) if in_types && other != "UdtType" => {
                Some("is below _types_/ but is not a UdtType")
            }
            Some(_) => None,
        };
        if let Some(problem) = problem {
            self.finding(rel, problem);
            return None;
        }
        Some((value, bytes))
    }
}

/// One repo file the git module would rewrite on its next export, with a
/// fixed reason (TAGDEP-12). A warning only: the file is imported as read.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct NonCanonical {
    /// The repo-relative file.
    pub file: String,
    /// One of `trailing newline`, `CRLF line ending`, `indent`, `key order`,
    /// `escaping`, `value without defaultValue`, `other`. Never content.
    pub reason: &'static str,
}

/// Every node file of `provider` that is not byte-identical to the git
/// module's own form ([`super::tag_canon::canonical_file_bytes`]), from the
/// bytes [`read_provider_with_raw`] kept. One entry per reason a file has,
/// byte-sorted by file, reasons in the fixed order [`NonCanonical::reason`]
/// lists. `value without defaultValue` marks a memory AtomicTag outside
/// `_types_/` carrying `value` and no `defaultValue`: the module writes both
/// for memory tags, so such files are hand-made (the gateway then adds a
/// `defaultValue` the plan ignores, R3-DV and KL-1).
pub fn check_canonical_form(
    provider: &NativeProvider,
    raw: &BTreeMap<String, Vec<u8>>,
) -> Vec<NonCanonical> {
    let mut out = Vec::new();
    for node in provider.nodes.values() {
        if node.kind == NodeKind::Folder {
            continue;
        }
        let Some(bytes) = raw.get(&node.file) else {
            continue;
        };
        for reason in form_reasons(bytes, node) {
            out.push(NonCanonical {
                file: node.file.clone(),
                reason,
            });
        }
    }
    // Stable: a file's reasons keep their fixed order.
    out.sort_by(|a, b| a.file.as_bytes().cmp(b.file.as_bytes()));
    out
}

/// Why `bytes` (the file of `node`) differ from the module's form. Each
/// defect is normalized away in turn (CRLF, trailing newlines, the indent
/// the file uses, its key order, raw HTML characters); whatever still differs
/// is `other`. A file whose indent unit is not trusted ([`indent_unit`]) is
/// `indent` and is not rebuilt, so `other` is not judged for it.
fn form_reasons(bytes: &[u8], node: &NativeNode) -> Vec<&'static str> {
    let (Ok(text), Ok((_, tree))) = (std::str::from_utf8(bytes), node::classify(bytes)) else {
        return vec!["other"];
    };
    let crlf = text.contains("\r\n");
    let text = text.replace("\r\n", "\n");
    let trailing = text.ends_with('\n');
    let text = text.trim_end_matches('\n');
    // An untrusted unit is reported as `indent` and the file is not rebuilt.
    let unit = indent_unit(text);
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        if GSON_HTML_ESCAPED.contains(c) {
            let _ = write!(escaped, "\\u{:04x}", u32::from(c));
        } else {
            escaped.push(c);
        }
    }
    // Written in the file's own key order and indent, so only what the
    // reasons above do not cover remains.
    let rebuilt = unit.map(|unit| {
        write_file_form(
            &tree,
            FileForm {
                sort_keys: false,
                indent: unit,
            },
        )
    });
    let json = &node.json;
    let memory_value_only = !in_types(&node.path)
        && json.get("tagType").and_then(Value::as_str) == Some("AtomicTag")
        && json.get("valueSource").and_then(Value::as_str) == Some("memory")
        && json.get("value").is_some()
        && json.get("defaultValue").is_none();
    [
        (trailing, "trailing newline"),
        (crlf, "CRLF line ending"),
        (unit != Some(MODULE_FORM.indent), "indent"),
        (!keys_sorted(&tree), "key order"),
        (escaped != text, "escaping"),
        (memory_value_only, "value without defaultValue"),
        (
            rebuilt.is_some_and(|rebuilt| rebuilt != escaped) || tree.get("name").is_some(),
            "other",
        ),
    ]
    .into_iter()
    .filter_map(|(found, reason)| found.then_some(reason))
    .collect()
}

/// The longest indent unit the form check rebuilds a file with.
const INDENT_UNIT_MAX: usize = 8;

/// The indent unit a file uses: the leading whitespace of its line 2, or the
/// module's own unit when line 2 has none (compact JSON, `{}`). `None` when
/// that whitespace is longer than [`INDENT_UNIT_MAX`] or mixes spaces and
/// tabs: such a unit is never trusted, because the rebuild repeats it at
/// every depth and a hostile file would multiply far past the read cap.
fn indent_unit(text: &str) -> Option<&str> {
    let Some(line) = text.split('\n').nth(1) else {
        return Some(MODULE_FORM.indent);
    };
    let unit = &line[..line.len() - line.trim_start_matches([' ', '\t']).len()];
    if unit.is_empty() {
        return Some(MODULE_FORM.indent);
    }
    let pure = unit.bytes().all(|b| b == b' ') || unit.bytes().all(|b| b == b'\t');
    (unit.len() <= INDENT_UNIT_MAX && pure).then_some(unit)
}

/// Whether every object in `tree` lists its keys in byte order.
fn keys_sorted(tree: &node::Node) -> bool {
    match tree {
        node::Node::Obj(members) => {
            members
                .windows(2)
                .all(|pair| pair[0].0.as_bytes() <= pair[1].0.as_bytes())
                && members.iter().all(|(_, value)| keys_sorted(value))
        }
        node::Node::Arr(items) => items.iter().all(keys_sorted),
        _ => true,
    }
}

/// Why a decoded tag name cannot be one path segment, or `None`. Never
/// quotes the name: it may hold a control character.
fn decoded_problem(name: &str) -> Option<String> {
    if name.chars().any(char::is_control) {
        return Some("decodes to a tag name holding a control character".to_string());
    }
    component_problem(name).map(|why| format!("decodes to an unsafe tag name (the name {why})"))
}

/// `prefix/name`, or `name` at the provider root.
fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}/{name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(root: &Path, rel: &str, bytes: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, bytes).expect("write");
    }

    const ATOMIC: &str = r#"{"tagType":"AtomicTag"}"#;

    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/native-tags")
            .join(name)
    }

    /// The findings of a `deploy_tree_invalid`, as `(file, message)`.
    fn findings(err: &CoreError) -> Vec<(String, String)> {
        match err {
            CoreError::DeployTreeInvalid(details) => details
                .findings
                .iter()
                .map(|f| (f.file.clone(), f.message.clone()))
                .collect(),
            other => panic!("not deploy_tree_invalid: {other}"),
        }
    }

    /// The files named by the findings, sorted.
    fn files(err: &CoreError) -> Vec<String> {
        let mut files: Vec<String> = findings(err).into_iter().map(|(file, _)| file).collect();
        files.sort();
        files
    }

    #[test]
    fn decode_fs_name_matches_the_module() {
        assert_eq!(decode_fs_name("Plain"), "Plain");
        assert_eq!(decode_fs_name("Motor%3AA"), "Motor:A");
        assert_eq!(decode_fs_name("100%25"), "100%");
        assert_eq!(decode_fs_name("a%2fb"), "a/b", "hex is case-insensitive");
        // An invalid or short escape stays literal.
        assert_eq!(decode_fs_name("Bad%ZZ"), "Bad%ZZ");
        assert_eq!(decode_fs_name("50%"), "50%");
        assert_eq!(decode_fs_name("%4"), "%4");
        assert_eq!(decode_fs_name("%zz1"), "%zz1");
        assert_eq!(decode_fs_name("%+1x"), "%+1x", "a sign is not a hex digit");
        // Java's `(char) code`: one byte is one Latin-1 character.
        assert_eq!(decode_fs_name("%C3%A9"), "\u{c3}\u{a9}");
    }

    #[test]
    fn encode_fs_name_matches_the_module_and_round_trips() {
        assert_eq!(encode_fs_name("Plain name"), "Plain name");
        assert_eq!(encode_fs_name("Motor:A"), "Motor%3AA");
        assert_eq!(encode_fs_name("100%"), "100%25");
        assert_eq!(encode_fs_name("a/b\\c"), "a%2Fb%5Cc");
        assert_eq!(encode_fs_name("<>\"|?*"), "%3C%3E%22%7C%3F%2A");
        assert_eq!(encode_fs_name("tab\there\u{1}"), "tab%09here%01");
        assert_eq!(
            encode_fs_name("caf\u{e9}"),
            "caf\u{e9}",
            "non-ASCII is kept"
        );
        assert_eq!(
            encode_fs_name("del\u{7f}"),
            "del\u{7f}",
            "only < 0x20 is control"
        );
        // Every name the module writes decodes back, and encodes again to
        // the same file name.
        for name in [
            "Plain",
            "Motor:A",
            "100%",
            "a/b",
            "q?",
            "<x>",
            "pipe|star*",
            "quote\"back\\",
            "%41",
            "caf\u{e9}",
        ] {
            let fs = encode_fs_name(name);
            assert_eq!(decode_fs_name(&fs), name, "{fs}");
            assert_eq!(encode_fs_name(&decode_fs_name(&fs)), fs, "{fs}");
        }
    }

    #[test]
    fn read_provider_builds_the_node_model() {
        let provider = read_provider(&fixture("good"), "PlantA").expect("reads");
        let paths: Vec<&str> = provider.nodes.keys().map(String::as_str).collect();
        assert_eq!(
            paths,
            [
                "Line1",
                "Line1/Motor:A",
                "Line1/Speed",
                "_types_/Motor",
                "_types_/Valve"
            ]
        );
        assert_eq!(provider.nodes["Line1"].kind, NodeKind::Folder);
        assert_eq!(provider.nodes["Line1/Speed"].kind, NodeKind::Tag);
        assert_eq!(provider.nodes["_types_/Motor"].kind, NodeKind::Type);
        assert_eq!(
            provider.nodes["Line1/Motor:A"].file,
            "tags/PlantA/Line1/Motor%3AA.json"
        );
        assert_eq!(provider.nodes["Line1/Speed"].json["value"], 12.5);
        assert_eq!(
            provider.groups,
            Some(serde_json::json!({"Default": {"rate": 1000}})),
            ".tag-groups.json is kept as JSON"
        );
    }

    #[test]
    fn the_tag_config_is_read_as_data() {
        let config = read_tag_config(&fixture("good")).expect("config");
        assert_eq!(
            config,
            TagConfig {
                included_providers: vec!["PlantA".to_string()],
                excluded_tag_paths: Vec::new(),
                collision_policy: Some("o".to_string()),
            }
        );
        // No file is the module's default: every provider, no exclusions.
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            read_tag_config(dir.path()).expect("absent"),
            TagConfig::default()
        );
        // Unknown keys stay out of the struct; strict JSON still applies.
        put(
            dir.path(),
            "tags/.tag-config.json",
            r#"{"includedProviders":[],"futureKey":1}"#,
        );
        assert_eq!(
            read_tag_config(dir.path()).expect("unknown key"),
            TagConfig::default()
        );
        for bad in [
            r#"{"includedProviders":"PlantA"}"#,
            r#"{"includedProviders":[1]}"#,
            r#"{"collisionPolicy":3}"#,
            r#"{"a":1,"a":2}"#,
            "[]",
            "{",
        ] {
            put(dir.path(), "tags/.tag-config.json", bad);
            let err = read_tag_config(dir.path()).expect_err(bad);
            assert_eq!(files(&err), ["tags/.tag-config.json"], "{bad}");
        }
    }

    #[test]
    fn an_owned_provider_must_be_in_included_providers() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path();
        put(repo, "tags/PlantA/T.json", ATOMIC);
        put(
            repo,
            "tags/.tag-config.json",
            r#"{"includedProviders":["Other"]}"#,
        );
        let err = read_provider(repo, "PlantA").expect_err("not included");
        assert_eq!(files(&err), ["tags/.tag-config.json"]);
        assert!(err.to_string().contains("includedProviders"), "{err}");
        // An empty list includes every provider, as in the module.
        put(repo, "tags/.tag-config.json", r#"{"includedProviders":[]}"#);
        read_provider(repo, "PlantA").expect("empty list includes all");
    }

    #[test]
    fn a_missing_provider_directory_is_refused_by_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = read_provider(dir.path(), "PlantA").expect_err("no tags/PlantA");
        assert_eq!(err.code(), "deploy_tree_invalid");
        assert!(err.to_string().contains("tags/PlantA"), "{err}");
    }

    #[test]
    fn the_legacy_single_file_layout_is_refused() {
        for (case, file) in [
            ("legacy-file", "tags/PlantA.json"),
            ("legacy-dir", "tags/PlantA/PlantA.json"),
        ] {
            let err = read_provider(&fixture(case), "PlantA").expect_err(case);
            assert_eq!(files(&err), [file], "{case}");
            assert!(err.to_string().contains("individual"), "{case}: {err}");
        }
        // tags/<P>.json beside a tags/<P>/ directory is refused too.
        let dir = tempfile::tempdir().expect("tempdir");
        put(dir.path(), "tags/PlantA/F/T.json", ATOMIC);
        put(dir.path(), "tags/PlantA.json", "{}");
        let err = read_provider(dir.path(), "PlantA").expect_err("both layouts");
        assert_eq!(files(&err), ["tags/PlantA.json"]);
    }

    #[test]
    fn bad_node_files_are_collected_into_one_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path();
        put(repo, "tags/P/A.json", "[1]");
        put(
            repo,
            "tags/P/B.json",
            r#"{"name":"Other","tagType":"AtomicTag"}"#,
        );
        put(
            repo,
            "tags/P/C.json",
            r#"{"a":1,"a":2,"tagType":"AtomicTag"}"#,
        );
        put(repo, "tags/P/D.json", "{");
        put(repo, "tags/P/E.json", r#"{"documentation":"no type"}"#);
        put(repo, "tags/P/F.json", r#"{"tagType":"UdtType"}"#);
        put(repo, "tags/P/G.json", r#"{"tagType":"Provider"}"#);
        put(repo, "tags/P/_types_/H.json", ATOMIC);
        put(repo, "tags/P/notes.txt", "not a node");
        let err = read_provider(repo, "P").expect_err("nine bad files");
        assert_eq!(
            files(&err),
            [
                "tags/P/A.json",
                "tags/P/B.json",
                "tags/P/C.json",
                "tags/P/D.json",
                "tags/P/E.json",
                "tags/P/F.json",
                "tags/P/G.json",
                "tags/P/_types_/H.json",
                "tags/P/notes.txt",
            ]
        );
        // A finding names the file, never its content.
        assert!(!err.to_string().contains("no type"), "{err}");
    }

    #[test]
    fn a_name_key_equal_to_the_file_stem_is_dropped() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path();
        put(
            repo,
            "tags/P/Motor%3AA.json",
            r#"{"name":"Motor:A","tagType":"AtomicTag"}"#,
        );
        let provider = read_provider(repo, "P").expect("same name");
        assert_eq!(
            provider.nodes["Motor:A"].json,
            serde_json::json!({"tagType": "AtomicTag"})
        );
    }

    #[test]
    fn unsafe_decoded_and_on_disk_names_are_refused() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path();
        for file in [
            "a%2Fb.json",
            "a%5Cb.json",
            "%2E%2E.json",
            "%2E.json",
            "%2Ehidden.json",
            "nul%00.json",
            "esc%1B.json",
            "%2E%2E/X.json",
            "CON.json",
            "raw:colon.json",
            "Dir./X.json",
        ] {
            put(repo, &format!("tags/P/{file}"), ATOMIC);
        }
        put(repo, "tags/P/Fine/Motor%3AA.json", ATOMIC);
        let err = read_provider(repo, "P").expect_err("unsafe names");
        assert_eq!(
            files(&err),
            [
                "tags/P/%2E%2E",
                "tags/P/%2E%2E.json",
                "tags/P/%2E.json",
                "tags/P/%2Ehidden.json",
                "tags/P/CON.json",
                "tags/P/Dir.",
                "tags/P/a%2Fb.json",
                "tags/P/a%5Cb.json",
                "tags/P/esc%1B.json",
                "tags/P/nul%00.json",
                "tags/P/raw:colon.json",
            ]
        );
        // A decoded control character never reaches the message.
        assert!(!err.to_string().contains('\u{1b}'), "{err}");
    }

    #[test]
    fn two_entries_giving_one_tag_name_are_refused() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path();
        // `%41` decodes to `A`; `a` differs only in case (tag paths are
        // case-insensitive); a folder and a file may not share a name.
        put(repo, "tags/P/%41.json", ATOMIC);
        put(repo, "tags/P/a.json", ATOMIC);
        put(repo, "tags/P/F.json", ATOMIC);
        put(repo, "tags/P/F/T.json", ATOMIC);
        let err = read_provider(repo, "P").expect_err("collisions");
        assert_eq!(files(&err), ["tags/P/F.json", "tags/P/a.json"]);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_and_special_files_are_refused_never_followed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path();
        put(repo, "elsewhere/X.json", ATOMIC);
        std::fs::create_dir_all(repo.join("tags/P")).expect("mkdir");
        std::os::unix::fs::symlink(repo.join("elsewhere"), repo.join("tags/P/Link")).expect("link");
        std::os::unix::fs::symlink(repo.join("elsewhere/X.json"), repo.join("tags/P/L.json"))
            .expect("link");
        std::os::unix::fs::symlink(
            repo.join("elsewhere/X.json"),
            repo.join("tags/P/.tag-groups.json"),
        )
        .expect("link");
        let _socket =
            std::os::unix::net::UnixListener::bind(repo.join("tags/P/S.json")).expect("socket");
        let err = read_provider(repo, "P").expect_err("links");
        assert_eq!(
            files(&err),
            [
                "tags/P/.tag-groups.json",
                "tags/P/L.json",
                "tags/P/Link",
                "tags/P/S.json"
            ]
        );
        // A symlinked provider root is refused too.
        std::os::unix::fs::symlink(repo.join("elsewhere"), repo.join("tags/Q")).expect("link");
        let err = read_provider(repo, "Q").expect_err("root link");
        assert_eq!(files(&err), ["tags/Q"]);
    }

    #[test]
    fn the_file_and_tree_caps_refuse_before_reading() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path();
        std::fs::create_dir_all(repo.join("tags/P")).expect("mkdir");
        let big = std::fs::File::create(repo.join("tags/P/Big.json")).expect("create");
        big.set_len(NATIVE_FILE_MAX_BYTES + 1).expect("sparse");
        let err = read_provider(repo, "P").expect_err("over the file cap");
        assert_eq!(files(&err), ["tags/P/Big.json"]);
        assert!(err.to_string().contains("cap"), "{err}");

        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path();
        for i in 0..5 {
            put(repo, &format!("tags/P/F/T{i}.json"), ATOMIC);
        }
        // F plus five files is six entries.
        read_provider_capped(repo, "P", 6).expect("at the cap");
        let err = read_provider_capped(repo, "P", 5).expect_err("over the cap");
        assert!(err.to_string().contains("more than 5"), "{err}");
        assert_eq!(NATIVE_FILES_MAX, 200_000);
    }
}

/// `--check`'s canonical-form report (TAGDEP-12).
#[cfg(test)]
mod canonical {
    use super::*;

    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/native-tags")
            .join(name)
    }

    fn report(repo: &Path, provider: &str) -> Vec<(String, &'static str)> {
        let (provider, raw) = read_provider_with_raw(repo, provider).expect("reads");
        check_canonical_form(&provider, &raw)
            .into_iter()
            .map(|n| (n.file, n.reason))
            .collect()
    }

    #[test]
    fn the_module_form_passes() {
        assert!(report(&fixture("good"), "PlantA").is_empty());
    }

    /// One fixture per defect, each the same node: one reason each.
    #[test]
    fn each_defect_has_one_fixed_reason() {
        let got = report(&fixture("noncanonical"), "PlantA");
        let file = |name: &str| format!("tags/PlantA/{name}.json");
        assert_eq!(
            got,
            [
                (file("Crlf"), "CRLF line ending"),
                (file("Escaping"), "escaping"),
                (file("Indent"), "indent"),
                (file("KeyOrder"), "key order"),
                (file("TrailingNewline"), "trailing newline"),
                (file("ValueOnly"), "value without defaultValue"),
            ]
        );
    }

    #[test]
    fn other_forms_are_other_and_reasons_never_quote_content() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path();
        let put = |rel: &str, bytes: &str| {
            let path = repo.join(rel);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
            std::fs::write(path, bytes).expect("write");
        };
        // Compact JSON, a `é` escape the module would not write, and a
        // `name` key the module drops.
        put("tags/P/Compact.json", r#"{"tagType":"AtomicTag"}"#);
        put(
            "tags/P/Ascii.json",
            "{\n  \"documentation\": \"caf\\u00e9 sentinel\",\n  \"tagType\": \"AtomicTag\"\n}",
        );
        put(
            "tags/P/Named.json",
            "{\n  \"name\": \"Named\",\n  \"tagType\": \"AtomicTag\"\n}",
        );
        // Two defects in one file: two reasons, in the fixed order.
        put(
            "tags/P/Both.json",
            "{\n  \"tagType\": \"AtomicTag\",\n  \"documentation\": \"a=b\"\n}\n",
        );
        // A memory tag's value is configuration below _types_/: no reason.
        put(
            "tags/P/_types_/Ty.json",
            "{\n  \"tagType\": \"UdtType\",\n  \"tags\": [\n    {\n      \"name\": \"M\",\n      \"tagType\": \"AtomicTag\",\n      \"value\": 0,\n      \"valueSource\": \"memory\"\n    }\n  ]\n}",
        );
        let got = report(repo, "P");
        assert_eq!(
            got,
            [
                ("tags/P/Ascii.json".to_string(), "other"),
                ("tags/P/Both.json".to_string(), "trailing newline"),
                ("tags/P/Both.json".to_string(), "key order"),
                ("tags/P/Both.json".to_string(), "escaping"),
                ("tags/P/Compact.json".to_string(), "other"),
                ("tags/P/Named.json".to_string(), "other"),
            ]
        );
    }

    /// The indent unit read from line 2 is trusted only when it is short
    /// and pure: a hostile unit would be repeated at every depth of the
    /// rebuild and multiply the file far past the read cap.
    #[test]
    fn indent_unit_accepts_short_pure_units_only() {
        let line2 = |ws: &str| format!("{{\n{ws}\"tagType\": \"AtomicTag\"\n}}");
        for ok in ["  ", "    ", "\t", "\t\t", "        "] {
            assert_eq!(indent_unit(&line2(ok)), Some(ok), "{ok:?}");
        }
        for bad in [
            " ".repeat(9),
            " ".repeat(5000),
            " \t".to_string(),
            "\t ".to_string(),
        ] {
            assert_eq!(indent_unit(&line2(&bad)), None, "{} chars", bad.len());
        }
        // No indented line 2 (compact JSON, `{}`): the module's own unit.
        assert_eq!(indent_unit(r#"{"tagType":"AtomicTag"}"#), Some("  "));
        assert_eq!(indent_unit("{\n\"tagType\": \"AtomicTag\"\n}"), Some("  "));
    }

    #[test]
    fn a_hostile_indent_is_reported_without_an_amplified_rebuild() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path();
        let path = repo.join("tags/P/Wide.json");
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        let text = format!(
            "{{\n{}\"alarms\": [\n    {{\n      \"name\": \"A\"\n    }}\n  ],\n  \"tagType\": \"AtomicTag\"\n}}",
            " ".repeat(5000)
        );
        std::fs::write(&path, text).expect("write");
        assert_eq!(
            report(repo, "P"),
            [("tags/P/Wide.json".to_string(), "indent")]
        );
    }

    /// The measured WHK shape (canon.py: recursive key sort plus Gson HTML
    /// escapes reproduce most files; the rest are trailing newlines, unsorted
    /// keys and raw escape characters), on neutral synthetic files.
    #[test]
    fn the_measured_shape_reproduces_on_synthetic_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path();
        let write = |rel: &str, bytes: &[u8]| {
            let path = repo.join(rel);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
            std::fs::write(path, bytes).expect("write");
        };
        for i in 0..40 {
            let node = serde_json::json!({
                "tagType": "AtomicTag",
                "valueSource": "opc",
                "documentation": format!("pump {i} <run> & 'stop' = {i} caf\u{e9}"),
                "alarms": [{"name": "Z", "priority": "High"}, {"name": "A", "priority": "Low"}],
                "eventScripts": [{"script": "x = 1", "eventid": "valueChanged"}],
            });
            let bytes = crate::deploy::tag_canon::canonical_file_bytes(&node);
            let rel = format!("tags/P/F{}/T{i}.json", i % 4);
            match i {
                3 | 17 => write(&rel, &[bytes.as_slice(), b"\n"].concat()),
                5 => write(
                    &rel,
                    &String::from_utf8(bytes)
                        .expect("utf8")
                        .replacen("\"alarms\"", "\"zzz\": 1,\n  \"alarms\"", 1)
                        .into_bytes(),
                ),
                9 => write(
                    &rel,
                    &String::from_utf8(bytes)
                        .expect("utf8")
                        .replace("\\u003d", "=")
                        .into_bytes(),
                ),
                _ => write(&rel, &bytes),
            }
        }
        let got = report(repo, "P");
        assert_eq!(
            got,
            [
                ("tags/P/F1/T17.json".to_string(), "trailing newline"),
                ("tags/P/F1/T5.json".to_string(), "key order"),
                ("tags/P/F1/T9.json".to_string(), "escaping"),
                ("tags/P/F3/T3.json".to_string(), "trailing newline"),
            ]
        );
    }
}
