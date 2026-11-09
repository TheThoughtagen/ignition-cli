//! The git module's native tag export, read from a project repo (Phase 26,
//! D-04, D-08).
//!
//! A project that owns a tag provider `P` keeps it under `tags/<P>/` in the
//! layout `GitTagManager` writes: a folder is a directory, every other node is
//! a `<name>.json` file holding the node without its `name`, UDT types live
//! below `_types_/`, and file and directory names are `%XX`-encoded where the
//! tag name holds a character a filesystem cannot carry. Entries whose name
//! starts with `.` (`.tag-config.json`, `.tag-groups.json`, agent state) are
//! not nodes, as in the module's `readTagsFromDirectory`.
//!
//! Pure and offline: the repo is read, nothing is contacted. Every file passes
//! the strict JSON reader ([`node::classify`]) before it becomes a
//! [`serde_json::Value`]. Plan 26-04 adds the remaining refusals and caps
//! inside [`read_provider`]'s walk.

use std::collections::BTreeMap;
use std::path::Path;

use super::node;
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
}

/// Read `tags/<provider>/` below `repo`.
///
/// A missing directory refuses `deploy_tree_invalid` naming it: an absent
/// provider is never read as an empty one. Every defect found in the walk is
/// collected into one `deploy_tree_invalid`. Entries are visited in byte order
/// of their names, so the same tree gives the same model.
pub fn read_provider(repo: &Path, provider: &str) -> Result<NativeProvider, CoreError> {
    let root_rel = format!("{TAGS_DIR}/{provider}");
    match std::fs::symlink_metadata(repo.join(&root_rel)) {
        Ok(meta) if meta.file_type().is_dir() => {}
        Ok(_) => {
            return Err(CoreError::deploy_tree_invalid(
                &root_rel,
                None,
                None,
                "is not a directory (an owned tag provider is the git module's tags/<provider>/ export)",
            ));
        }
        Err(_) => {
            return Err(CoreError::deploy_tree_invalid(
                &root_rel,
                None,
                None,
                "does not exist: deploy/project.yaml lists this provider under tag_providers, \
                 and an owned provider is never read as empty",
            ));
        }
    }
    let mut walk = Walk {
        repo,
        nodes: BTreeMap::new(),
        findings: Vec::new(),
    };
    walk.dir(&root_rel, "");
    if !walk.findings.is_empty() {
        return Err(CoreError::deploy_findings(walk.findings));
    }
    Ok(NativeProvider {
        name: provider.to_string(),
        nodes: walk.nodes,
    })
}

/// The state of one [`read_provider`] walk.
struct Walk<'a> {
    repo: &'a Path,
    nodes: BTreeMap<String, NativeNode>,
    findings: Vec<DeployFinding>,
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

    /// Walk the directory `rel` (repo-relative), whose nodes sit at the
    /// provider-relative `prefix` (empty at the provider root).
    fn dir(&mut self, rel: &str, prefix: &str) {
        let entries = match std::fs::read_dir(self.repo.join(rel)) {
            Ok(entries) => entries,
            Err(e) => {
                self.finding(rel, format!("could not be read ({:?})", e.kind()));
                return;
            }
        };
        let mut names = Vec::new();
        for entry in entries {
            match entry {
                Ok(entry) => match entry.file_name().into_string() {
                    Ok(name) => names.push(name),
                    Err(_) => self.finding(rel, "holds an entry whose name is not valid UTF-8"),
                },
                Err(e) => self.finding(rel, format!("could not be listed ({:?})", e.kind())),
            }
        }
        names.sort();
        for fs_name in names {
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
            let in_types = prefix == TYPES_DIR || prefix.starts_with(&format!("{TYPES_DIR}/"));
            if meta.file_type().is_dir() {
                let name = decode_fs_name(&fs_name);
                let path = join(prefix, &name);
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
            } else if meta.file_type().is_file() {
                let Some(stem) = fs_name.strip_suffix(".json") else {
                    // The module reads `.json` files only.
                    continue;
                };
                let name = decode_fs_name(stem);
                let path = join(prefix, &name);
                let Some(json) = self.read_node(&entry_rel) else {
                    continue;
                };
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
            } else {
                self.finding(
                    &entry_rel,
                    "is a symlink or special file; an owned tag provider holds directories and .json files only",
                );
            }
        }
    }

    /// Read one node file strictly; `None` (with a finding) when it is not a
    /// JSON object without a `name` key.
    fn read_node(&mut self, rel: &str) -> Option<serde_json::Value> {
        let bytes = match read_repo_file(self.repo, rel, None) {
            Ok(bytes) => bytes,
            Err(err) => {
                self.finding(rel, format!("could not be read ({})", err.code()));
                return None;
            }
        };
        if let Err(why) = node::classify(&bytes) {
            self.finding(rel, format!("is not valid JSON: {why}"));
            return None;
        }
        let value: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(_) => {
                self.finding(rel, "is not valid JSON");
                return None;
            }
        };
        match value.as_object() {
            None => {
                self.finding(rel, "must be a JSON object (one tag node)");
                None
            }
            Some(object) if object.contains_key("name") => {
                self.finding(
                    rel,
                    "holds a `name` key; the git module names a node by its file name",
                );
                None
            }
            Some(_) => Some(value),
        }
    }
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

    #[test]
    fn decode_fs_name_matches_the_module() {
        assert_eq!(decode_fs_name("Plain"), "Plain");
        assert_eq!(decode_fs_name("Motor%3AA"), "Motor:A");
        assert_eq!(decode_fs_name("100%25"), "100%");
        assert_eq!(decode_fs_name("a%2fb"), "a/b", "hex is case-insensitive");
        // An invalid or short escape stays literal.
        assert_eq!(decode_fs_name("50%"), "50%");
        assert_eq!(decode_fs_name("%4"), "%4");
        assert_eq!(decode_fs_name("%zz1"), "%zz1");
        assert_eq!(decode_fs_name("%+1x"), "%+1x", "a sign is not a hex digit");
        // Java's `(char) code`: one byte is one Latin-1 character.
        assert_eq!(decode_fs_name("%C3%A9"), "\u{c3}\u{a9}");
    }

    #[test]
    fn read_provider_builds_the_node_model() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path();
        put(repo, "tags/.tag-config.json", r#"{"collisionPolicy":"o"}"#);
        put(repo, "tags/PlantA/.tag-groups.json", "{}");
        put(
            repo,
            "tags/PlantA/Line1/Speed.json",
            r#"{"tagType":"AtomicTag","value":3,"valueSource":"memory"}"#,
        );
        put(
            repo,
            "tags/PlantA/Line1/Motor%3AA.json",
            r#"{"tagType":"UdtInstance","typeId":"Motor"}"#,
        );
        put(repo, "tags/PlantA/Line1/notes.txt", "not a node");
        put(
            repo,
            "tags/PlantA/_types_/Motor.json",
            r#"{"tagType":"UdtType","tags":[]}"#,
        );
        let provider = read_provider(repo, "PlantA").expect("reads");
        let paths: Vec<&str> = provider.nodes.keys().map(String::as_str).collect();
        assert_eq!(
            paths,
            ["Line1", "Line1/Motor:A", "Line1/Speed", "_types_/Motor"]
        );
        assert_eq!(provider.nodes["Line1"].kind, NodeKind::Folder);
        assert_eq!(provider.nodes["Line1/Speed"].kind, NodeKind::Tag);
        assert_eq!(provider.nodes["_types_/Motor"].kind, NodeKind::Type);
        assert_eq!(
            provider.nodes["Line1/Motor:A"].file,
            "tags/PlantA/Line1/Motor%3AA.json"
        );
        assert_eq!(provider.nodes["Line1/Speed"].json["value"], 3);
    }

    #[test]
    fn a_missing_provider_directory_is_refused_by_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = read_provider(dir.path(), "PlantA").expect_err("no tags/PlantA");
        assert_eq!(err.code(), "deploy_tree_invalid");
        assert!(err.to_string().contains("tags/PlantA"), "{err}");
    }

    #[test]
    fn bad_node_files_are_collected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path();
        put(repo, "tags/P/A.json", "[1]");
        put(
            repo,
            "tags/P/B.json",
            r#"{"name":"B","tagType":"AtomicTag"}"#,
        );
        put(repo, "tags/P/C.json", r#"{"a":1,"a":2}"#);
        put(repo, "tags/P/D.json", "{");
        let err = read_provider(repo, "P").expect_err("four bad files");
        let text = err.to_string();
        for file in [
            "tags/P/A.json",
            "tags/P/B.json",
            "tags/P/C.json",
            "tags/P/D.json",
        ] {
            assert!(text.contains(file), "{file}: {text}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_in_the_walk_is_refused() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path();
        put(repo, "elsewhere/X.json", r#"{"tagType":"AtomicTag"}"#);
        std::fs::create_dir_all(repo.join("tags/P")).expect("mkdir");
        std::os::unix::fs::symlink(repo.join("elsewhere"), repo.join("tags/P/Link")).expect("link");
        let err = read_provider(repo, "P").expect_err("symlink");
        assert!(err.to_string().contains("tags/P/Link"), "{err}");
    }
}
