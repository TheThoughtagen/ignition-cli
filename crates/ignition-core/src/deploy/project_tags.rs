//! Tags a project uses (quick task 261106-bt0): which `[provider]path`
//! references the code of a project artifact holds, and whether a gateway's
//! tag export holds them.
//!
//! `ign deploy projects` does not deploy tags, so a project can go live
//! pointing at tags the target gateway lacks. This module is the pure half of
//! the warning that says so: [`scan_artifact`] reads an artifact zip into
//! [`TagRefs`], and [`TagTree`] answers [`Lookup`] for a path against the
//! native export of one provider. No network, no clock, no regex.
//!
//! A reference is an anchored literal, never free text. A false "missing" is
//! worse than a miss, because the report exists to be read and believed.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read as _;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// What the output and the README say about the limits of the check. Printed
/// by the human renderer on every run and carried in the JSON result.
pub const MISSING_TAGS_LIMIT: &str = "paths built at runtime aren't checked; a tag project code \
creates at runtime can appear as a warning; text that looks like a path outside a comment or \
docstring can appear as a warning; binary resources are not read; members below a __tests__ \
folder are not scanned";

/// Most referencing files one entry carries.
pub const FILES_PER_ENTRY: usize = 3;

/// The most bytes read from one artifact member.
const MEMBER_READ_MAX: u64 = 16 * 1024 * 1024;

/// One tag path the project uses that the target lacks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MissingTag {
    /// `[provider]path`, as the project spells it.
    pub path: String,
    /// At most [`FILES_PER_ENTRY`] artifact members that name it, byte-sorted.
    pub files: Vec<String>,
}

/// A provider the project references that the target gateway does not list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnknownProvider {
    /// The provider name, as the project spells it.
    pub provider: String,
    /// How many distinct paths below it the project uses.
    pub path_count: usize,
    /// At most [`FILES_PER_ENTRY`] artifact members that name it, byte-sorted.
    pub files: Vec<String>,
}

/// A read that could not be made. The check goes on; the reason is fixed
/// wording plus an error slug, never a response body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TagsUnavailable {
    /// The provider whose export failed; `None` when the failure is not per
    /// provider.
    pub provider: Option<String>,
    /// Why nothing was learned.
    pub reason: String,
}

/// One referenced path, merged over every file that names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagRef {
    /// The provider, as the project spells it.
    pub provider: String,
    /// The path below the provider, as the project spells it.
    pub path: String,
    /// Up to [`FILES_PER_ENTRY`] member paths, byte-sorted.
    pub files: BTreeSet<String>,
}

/// Every tag reference a project artifact holds, keyed by the ASCII-folded
/// provider and the folded raw path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagRefs {
    refs: BTreeMap<(String, String), TagRef>,
}

impl TagRefs {
    /// Whether the artifact references no tag at all.
    pub fn is_empty(&self) -> bool {
        self.refs.is_empty()
    }

    /// Every reference, byte-sorted by folded provider and folded path.
    pub fn iter(&self) -> impl Iterator<Item = &TagRef> {
        self.refs.values()
    }

    fn add(&mut self, provider: &str, path: &str, file: &str) {
        let key = (provider.to_ascii_lowercase(), path.to_lowercase());
        let entry = self.refs.entry(key).or_insert_with(|| TagRef {
            provider: provider.to_string(),
            path: path.to_string(),
            files: BTreeSet::new(),
        });
        // Two spellings of one path report one entry, under the smaller.
        if (provider, path) < (entry.provider.as_str(), entry.path.as_str()) {
            entry.provider = provider.to_string();
            entry.path = path.to_string();
        }
        entry.files.insert(file.to_string());
        while entry.files.len() > FILES_PER_ENTRY {
            entry.files.pop_last();
        }
    }
}

/// `[provider]rest` as `(provider, rest)`, or `None` when the text is not an
/// anchored tag-path literal.
pub fn parse_literal(text: &str) -> Option<(&str, &str)> {
    let inner = text.strip_prefix('[')?;
    let close = inner.find(']')?;
    let (provider, rest) = (&inner[..close], &inner[close + 1..]);
    if provider.is_empty() || rest.is_empty() {
        return None;
    }
    Some((provider, rest))
}

/// Whether the member is scanned, by its zip path.
fn scanned_member(name: &str) -> bool {
    let base = name.rsplit('/').next().unwrap_or(name);
    if name.ends_with(".py") {
        return true;
    }
    name.ends_with(".json") && base != "resource.json" && name != "project.json"
}

/// The quoted one-line literals of `.py` text.
fn scan_python(text: &str, file: &str, refs: &mut TagRefs) {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let quote = bytes[i];
        if quote != b'"' && quote != b'\'' {
            i += 1;
            continue;
        }
        let start = i + 1;
        let mut j = start;
        while j < bytes.len() && bytes[j] != quote && bytes[j] != b'\n' {
            if bytes[j] == b'\\' {
                j += 1;
            }
            j += 1;
        }
        if j < bytes.len() && bytes[j] == quote {
            if let Some((provider, rest)) = text.get(start..j).and_then(parse_literal) {
                refs.add(provider, rest, file);
            }
            i = j + 1;
        } else {
            i = j.max(start);
        }
    }
}

/// Every tag reference in the artifact `zip`.
pub fn scan_artifact(zip: &[u8]) -> Result<TagRefs, CoreError> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip))
        .map_err(|err| CoreError::Internal(format!("project zip is not readable: {err}")))?;
    let mut refs = TagRefs::default();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|err| CoreError::Internal(format!("cannot walk project zip: {err}")))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        if !scanned_member(&name) {
            continue;
        }
        let mut bytes = Vec::new();
        entry
            .take(MEMBER_READ_MAX)
            .read_to_end(&mut bytes)
            .map_err(|err| CoreError::Internal(format!("cannot decompress a zip member: {err}")))?;
        scan_python(&String::from_utf8_lossy(&bytes), &name, &mut refs);
    }
    Ok(refs)
}

/// What a tree says about a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lookup {
    /// The tag exists.
    Found,
    /// The tree is fully understood below the path and has no such tag.
    Missing,
    /// The tree cannot answer (never reported as missing).
    Unknown,
}

/// One node of an export, with only the four fields the lookup needs.
#[derive(Debug, Deserialize)]
struct RawNode {
    #[serde(default)]
    name: String,
    #[serde(default, rename = "tagType")]
    tag_type: Option<String>,
    #[serde(default)]
    tags: Vec<RawNode>,
}

#[derive(Debug, Default)]
struct Node {
    children: BTreeMap<String, Node>,
}

impl Node {
    fn from_raw(raw: RawNode) -> Node {
        let mut node = Node::default();
        for child in raw.tags {
            let key = child.name.to_lowercase();
            let child = Node::from_raw(child);
            node.children.entry(key).or_default().absorb(child);
        }
        node
    }

    fn absorb(&mut self, other: Node) {
        for (key, child) in other.children {
            self.children.entry(key).or_default().absorb(child);
        }
    }
}

/// The tag tree of one provider, from its native export.
#[derive(Debug)]
pub struct TagTree {
    root: Node,
}

impl TagTree {
    /// Parse the export of provider `provider`. Anything that is not a
    /// `Provider` root is refused.
    pub fn parse(bytes: &[u8], _provider: &str) -> Result<TagTree, CoreError> {
        let raw: RawNode = serde_json::from_slice(bytes).map_err(|err| {
            CoreError::Internal(format!("the tag export is not a tag tree: {err}"))
        })?;
        if raw.tag_type.as_deref() != Some("Provider") {
            return Err(CoreError::Internal(
                "the tag export is not a tag tree: the root is not a Provider".to_string(),
            ));
        }
        Ok(TagTree {
            root: Node::from_raw(raw),
        })
    }

    /// Whether `path` (the part after `[provider]`) is in the tree.
    pub fn lookup(&self, path: &str) -> Lookup {
        let mut node = &self.root;
        for segment in path.split('/') {
            match node.children.get(&segment.to_lowercase()) {
                Some(child) => node = child,
                None => return Lookup::Missing,
            }
        }
        Lookup::Found
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::{parse_literal, scan_artifact};

    fn zip_of(members: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        for (name, bytes) in members {
            writer.start_file(*name, options).expect("start");
            writer.write_all(bytes).expect("write");
        }
        writer.finish().expect("finish").into_inner()
    }

    /// Every reference the members hold, as `[provider]path`, sorted.
    fn found(members: &[(&str, &[u8])]) -> Vec<String> {
        let refs = scan_artifact(&zip_of(members)).expect("scan");
        let mut all: Vec<String> = refs
            .iter()
            .map(|r| format!("[{}]{}", r.provider, r.path))
            .collect();
        all.sort();
        all
    }

    const PY: &str = "ignition/script-python/pkg/code.py";
    const VIEW: &str = "com.inductiveautomation.perspective/views/V/view.json";

    fn py(code: &str) -> Vec<String> {
        found(&[(PY, code.as_bytes())])
    }

    fn json(text: &str) -> Vec<String> {
        found(&[(VIEW, text.as_bytes())])
    }

    #[test]
    fn scan_python_finds_string_literal_forms() {
        let code = "x = system.tag.readBlocking([\"[default]A/B\", '[default]G/H'])\n\
                    y = r\"[default]C/D\"\n\
                    z = u\"[default]E/F\"\n";
        assert_eq!(
            py(code),
            [
                "[default]A/B",
                "[default]C/D",
                "[default]E/F",
                "[default]G/H"
            ]
        );
    }

    #[test]
    fn scan_python_comment_is_not_a_reference_but_a_hash_in_a_string_is_not_a_comment() {
        let code = "# [default]A/B\n\
                    t = [\"#\", \"[default]C/D\"]  # [default]E/F\n";
        assert_eq!(py(code), ["[default]C/D"]);
    }

    /// The brief's false positive: docstrings and statement strings that
    /// carry an example path are not references.
    #[test]
    fn scan_python_docstring_example_is_not_a_reference() {
        let code = "def f():\n\
                    \x20   \"\"\"\n\
                    \x20   Reads [provider]Area/MES/SomeData\n\
                    \x20   \"\"\"\n\
                    \x20   '''[default]Area/MES/SomeData'''\n\
                    \x20   \"[default]Area/Statement\"\n\
                    \x20   return 1\n";
        assert_eq!(py(code), Vec::<String>::new());
    }

    #[test]
    fn scan_python_last_list_element_without_a_comma_is_found() {
        let code = "tags = [\n    \"[default]A/B\",\n    \"[default]C/D\"\n]\n";
        assert_eq!(py(code), ["[default]A/B", "[default]C/D"]);
    }

    #[test]
    fn scan_python_runtime_built_paths_are_not_references() {
        let code = "a = \"[default]A/%s\" % x\n\
                    b = \"[default]A/%d\"\n\
                    c = \"[default]A/{}\".format(x)\n\
                    d = f\"[default]{a}/B\"\n\
                    e = \"[default]A/\" + x\n\
                    f = \"[default]A/B\" + x\n\
                    g = x + \"[default]A/B\"\n\
                    h = \"[default]A/B\" \"C\"\n\
                    i = (\"[default]A/B\"\n\
                    \x20    # a comment between the pieces\n\
                    \x20    \"C\")\n\
                    j = \"[default]\"\n\
                    k = \"[default] text\"\n\
                    l = \"[default]...A/B\"\n\
                    m = \"[default]A//B\"\n\
                    n = \"[default]A/../B\"\n\
                    o = \"[default]A/B[0]\"\n\
                    p = \"[default]A/B\\n\"\n\
                    q = \"[default]A/B\u{1}C\"\n";
        assert_eq!(py(code), Vec::<String>::new());
    }

    #[test]
    fn scan_python_unterminated_literal_and_bad_bytes_do_not_panic() {
        assert_eq!(py("x = \"[default]A/B\ny = 1\n"), Vec::<String>::new());
        assert_eq!(py("x = '[default]A/B"), Vec::<String>::new());
        let bad: &[u8] = b"# \xff\xfe\nx = ['[default]A/B']\n";
        assert_eq!(found(&[(PY, bad)]), ["[default]A/B"]);
    }

    #[test]
    fn scan_member_holding_nul_in_its_first_8k_is_binary_and_skipped() {
        let mut bytes = b"\0\0\0 x = ['[default]A/B']\n".to_vec();
        assert_eq!(found(&[(PY, &bytes)]), Vec::<String>::new());
        // A NUL after the first 8 KiB does not make it binary.
        bytes = b"x = ['[default]A/B']\n".to_vec();
        bytes.extend(std::iter::repeat_n(b' ', 9000));
        bytes.push(0);
        assert_eq!(found(&[(PY, &bytes)]), ["[default]A/B"]);
    }

    #[test]
    fn scan_json_finds_a_tag_path_value_with_the_member_as_its_file() {
        let refs = scan_artifact(&zip_of(&[(
            VIEW,
            br#"{"props":{"tagPath":"[default]A/B"}}"#.as_slice(),
        )]))
        .expect("scan");
        let all: Vec<_> = refs.iter().collect();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].path, "A/B");
        assert_eq!(all[0].files.iter().collect::<Vec<_>>(), [&VIEW.to_string()]);
    }

    #[test]
    fn scan_json_indirect_path_is_not_a_reference() {
        assert_eq!(
            json(r#"{"tagPath":"[default]A/{1}/B"}"#),
            Vec::<String>::new()
        );
    }

    #[test]
    fn scan_json_expression_forms() {
        assert_eq!(
            json(r#"{"expression":"{[default]A/B} + 1"}"#),
            ["[default]A/B"]
        );
        assert_eq!(
            json(r#"{"expression":"tag('[default]C/D')"}"#),
            ["[default]C/D"]
        );
        assert_eq!(
            json(r#"{"expression":"tag('[default]A/'+{x})"}"#),
            Vec::<String>::new()
        );
        assert_eq!(
            json(r#"{"expression":"tag('[default]A/B'+{x})"}"#),
            Vec::<String>::new()
        );
    }

    #[test]
    fn scan_json_script_value_is_python_scanned_after_decoding() {
        let text = r#"{"script":"def f():\n    # [default]A/B\n    return system.tag.readBlocking(['[default]C/D'])\n"}"#;
        assert_eq!(json(text), ["[default]C/D"]);
    }

    #[test]
    fn scan_json_label_with_a_path_inside_is_not_a_reference() {
        assert_eq!(json(r#"{"text":"see [default]A/B"}"#), Vec::<String>::new());
    }

    #[test]
    fn scan_json_unparseable_text_falls_back_to_quoted_literals() {
        assert_eq!(json(r#"{"tagPath": "[default]A/B", "#), ["[default]A/B"]);
    }

    #[test]
    fn scan_members_by_kind() {
        let body = br#"{"x":"[default]A/B"}"#.as_slice();
        let code = b"x = ['[default]C/D']\n".as_slice();
        assert_eq!(
            found(&[
                ("ignition/script-python/pkg/resource.json", body),
                ("project.json", body),
                ("ignition/event-scripts/data.bin", code),
                ("ignition/images/logo.png", code),
            ]),
            Vec::<String>::new()
        );
        assert_eq!(
            found(&[
                ("ignition/timer/Poll/handleTimerEvent.py", code),
                ("ignition/tag-change/T/onTagChange.py", code),
                ("ignition/update/onUpdate.py", code),
            ]),
            ["[default]C/D"]
        );
    }

    #[test]
    fn scan_members_below_a_tests_folder_are_skipped_by_path() {
        let code = b"x = ['[TEST]Fake/A', '[default]A/Absent']\n".as_slice();
        assert_eq!(
            found(&[
                ("ignition/script-python/pkg/__tests__/case/code.py", code),
                ("ignition/script-python/__tests__/code.py", code),
            ]),
            Vec::<String>::new()
        );
        assert_eq!(
            found(&[
                ("ignition/script-python/pkg/__tests__x/code.py", code),
                ("ignition/script-python/my__tests__/code.py", code),
            ]),
            ["[TEST]Fake/A", "[default]A/Absent"]
        );
    }

    #[test]
    fn scan_parse_literal_accepts_providers_and_paths_with_spaces_and_parens() {
        assert_eq!(
            parse_literal("[MQTT Engine]Edge Nodes/A"),
            Some(("MQTT Engine", "Edge Nodes/A"))
        );
        assert_eq!(
            parse_literal("[default]Pump (A)/Run"),
            Some(("default", "Pump (A)/Run"))
        );
    }

    #[test]
    fn scan_parse_literal_refusals() {
        let long = format!("[default]{}", "a".repeat(513));
        let refused = [
            ("no bracket", "default]A/B"),
            ("no close", "[default"),
            ("empty provider", "[]A/B"),
            ("slash in provider", "[a/b]A"),
            ("open bracket in provider", "[a[b]A"),
            ("brace in provider", "[a{b]A"),
            ("empty rest", "[default]"),
            ("too long", long.as_str()),
            ("leading space", "[default] A/B"),
            ("trailing space", "[default]A/B "),
            ("leading slash", "[default]/A"),
            ("trailing slash", "[default]A/"),
            ("leading dot", "[default].A"),
            ("trailing dot", "[default]A."),
            ("open brace", "[default]A/{x}"),
            ("close brace", "[default]A}"),
            ("percent", "[default]A/%s"),
            ("dollar", "[default]A/$x"),
            ("open square", "[default]A/B[0]"),
            ("close square", "[default]A]"),
            ("backslash", "[default]A\\B"),
            ("double quote", "[default]A\"B"),
            ("control character", "[default]A\u{7}B"),
            ("escape in provider", "[de\u{1b}fault]A"),
            ("empty segment", "[default]A//B"),
            ("dot segment", "[default]A/./B"),
            ("dot dot segment", "[default]A/../B"),
        ];
        for (why, text) in refused {
            assert_eq!(parse_literal(text), None, "{why}: {text:?}");
        }
        assert_eq!(
            parse_literal("[default]A/it's"),
            Some(("default", "A/it's"))
        );
    }
}
