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

use crate::client::scripts_codec::SCRIPT_KEYS;
use crate::deploy::project_artifact::is_format_char;
use crate::error::CoreError;

/// What the output and the README say about the limits of the check. Printed
/// by the human renderer on every run and carried in the JSON result.
pub const MISSING_TAGS_LIMIT: &str = "paths built at runtime aren't checked; \
a path through a UDT type the tag export does not resolve isn't checked; \
triple-quoted Python strings are not scanned; a tag project code creates at runtime can appear as \
a warning; text that looks like a path outside a comment or triple-quoted string can appear as a \
warning; binary resources are not read; members below a __tests__ folder are not scanned";

/// Most referencing files one entry carries.
pub const FILES_PER_ENTRY: usize = 3;

/// The most bytes scanned from one artifact member. A file over it is scanned
/// in part and reported ([`TagRefs::cut_reason`]), never read as complete.
pub const MEMBER_READ_MAX: u64 = 16 * 1024 * 1024;

/// One tag path the project uses that the target lacks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MissingTag {
    /// `[provider]path`, as the project spells it.
    pub path: String,
    /// At most [`FILES_PER_ENTRY`] artifact members that name it, byte-sorted.
    /// A control or non-printing format character in a name is written out as
    /// an escape.
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
    /// A control or non-printing format character in a name is written out as
    /// an escape.
    pub files: Vec<String>,
}

/// A read that could not be made. The check goes on; the reason is fixed
/// wording plus an error slug, or the names of the files read only in part,
/// never a response body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TagsUnavailable {
    /// The provider whose export failed; `None` when the failure is not per
    /// provider.
    pub provider: Option<String>,
    /// Why nothing was learned: fixed wording plus an error slug, or the names
    /// of the files read only in part. Never a response body.
    pub reason: String,
}

/// One referenced path, merged over every file that names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagRef {
    /// The provider, as the project spells it.
    pub provider: String,
    /// The path below the provider, as the project spells it.
    pub path: String,
    /// Up to [`FILES_PER_ENTRY`] member paths, byte-sorted. A control or
    /// non-printing format character in a name is written out as an escape.
    pub files: BTreeSet<String>,
}

/// Every tag reference a project artifact holds, keyed by the ASCII-folded
/// provider and the folded raw path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagRefs {
    refs: BTreeMap<(String, String), TagRef>,
    /// Members over [`MEMBER_READ_MAX`] that were scanned only in part. A
    /// control or non-printing format character in a name is written out as an
    /// escape, so a name is always one printable line.
    cut: BTreeSet<String>,
}

impl TagRefs {
    /// Whether the artifact references no tag at all. A cut file does not
    /// change this: it means "no reference was read".
    pub fn is_empty(&self) -> bool {
        self.refs.is_empty()
    }

    /// Why the scan is incomplete, or `None` when every file was read whole.
    ///
    /// Names at most [`FILES_PER_ENTRY`] files, byte-sorted, and counts the
    /// rest, so an artifact with many large files still gives one short line.
    pub fn cut_reason(&self) -> Option<String> {
        if self.cut.is_empty() {
            return None;
        }
        let names: Vec<&str> = self
            .cut
            .iter()
            .take(FILES_PER_ENTRY)
            .map(String::as_str)
            .collect();
        let rest = self.cut.len() - names.len();
        let mut reason = format!(
            "the part past the {} MiB read limit of {} project file(s): {}",
            MEMBER_READ_MAX / (1024 * 1024),
            self.cut.len(),
            names.join(", ")
        );
        if rest > 0 {
            reason.push_str(&format!(" and {rest} more"));
        }
        Some(reason)
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

/// The longest path (the part after `[provider]`) a literal may hold.
const PATH_MAX_BYTES: usize = 512;

/// How many leading bytes of a member are checked for a NUL (binary data).
const BINARY_PROBE_BYTES: usize = 8 * 1024;

/// `[provider]rest` as `(provider, rest)`, or `None` when the text is not an
/// anchored tag-path literal.
///
/// Refused: an empty provider, or one holding `/`, `[`, `{`, a control
/// character or a non-printing format character; an empty `rest`, one over 512
/// bytes, or one that starts or ends with whitespace, `/` or `.`; `{ } % $ [ ]`,
/// a backslash, a double quote, a control character or a non-printing format
/// character anywhere in `rest`; an empty, `.` or `..` segment. A format
/// character is refused, not written out, because the path reaches the
/// confirmation prose and a terminal as it is, and a path that was changed
/// would not be found in the export.
/// Everything refused is text that is built at runtime or only looks like a
/// path.
pub fn parse_literal(text: &str) -> Option<(&str, &str)> {
    let inner = text.strip_prefix('[')?;
    let close = inner.find(']')?;
    let (provider, rest) = (&inner[..close], &inner[close + 1..]);
    if provider.is_empty()
        || provider.contains(['/', '[', '{'])
        || provider
            .chars()
            .any(|c| c.is_control() || is_format_char(c))
    {
        return None;
    }
    if rest.is_empty() || rest.len() > PATH_MAX_BYTES {
        return None;
    }
    let edge = |c: char| c.is_whitespace() || c == '/' || c == '.';
    if rest.starts_with(edge) || rest.ends_with(edge) {
        return None;
    }
    let refused = |c: char| {
        matches!(c, '{' | '}' | '%' | '$' | '[' | ']' | '\\' | '"')
            || c.is_control()
            || is_format_char(c)
    };
    if rest.chars().any(refused) {
        return None;
    }
    if rest
        .split('/')
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return None;
    }
    Some((provider, rest))
}

/// `name` with every control or non-printing format character written out as
/// an escape (`char::escape_default`), the form the human renderer uses for
/// control characters. Everything else, a backslash included, is copied.
///
/// A member name comes from a repo and reaches the confirmation prose, which
/// keeps newlines (so a name could add a line, such as a fake
/// `artifact_sha256` one), and terminal output, which keeps format characters
/// (a bidi override reorders text, a separator breaks a line). Encoding once,
/// where the name is read, covers every output. Two names that encode alike
/// count once in the cut list.
fn printable_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_control() || is_format_char(c) {
            out.extend(c.escape_default());
        } else {
            out.push(c);
        }
    }
    out
}

/// Whether the member is scanned, by its zip path. Python anywhere and JSON
/// other than a `resource.json` descriptor and the root `project.json`; never
/// a member below a directory named exactly `__tests__`, and never a binary
/// resource.
fn scanned_member(name: &str) -> bool {
    let (dirs, base) = name.rsplit_once('/').unwrap_or(("", name));
    if dirs.split('/').any(|component| component == "__tests__") {
        return false;
    }
    base.ends_with(".py")
        || (base.ends_with(".json") && base != "resource.json" && name != "project.json")
}

/// Whether `bytes` from `at` starts an identifier-like run that is a Python
/// string prefix directly followed by a quote. Returns the prefix length.
fn string_prefix_at(bytes: &[u8], at: usize) -> Option<usize> {
    let mut len = 0;
    while len < 2 && bytes.get(at + len).is_some_and(u8::is_ascii_alphabetic) {
        len += 1;
    }
    if len == 0 || !matches!(bytes.get(at + len), Some(b'"' | b'\'')) {
        return None;
    }
    let prefix = bytes[at..at + len].to_ascii_lowercase();
    matches!(
        prefix.as_slice(),
        b"r" | b"u" | b"b" | b"f" | b"br" | b"rb" | b"fr" | b"rf"
    )
    .then_some(len)
}

/// The index of the next significant byte at or after `at`: blanks are
/// skipped, and when `multiline` also newlines and `#` comments (inside
/// brackets a statement goes on).
fn next_significant(bytes: &[u8], mut at: usize, multiline: bool) -> usize {
    while let Some(&b) = bytes.get(at) {
        match b {
            b' ' | b'\t' | b'\r' | 0x0c => at += 1,
            b'\\' if bytes.get(at + 1) == Some(&b'\n') => at += 2,
            b'\n' if multiline => at += 1,
            b'#' if multiline => {
                while bytes.get(at).is_some_and(|&c| c != b'\n') {
                    at += 1;
                }
            }
            _ => break,
        }
    }
    at
}

/// The string literals of Python text that stand for a tag path.
///
/// A linear lexer over bytes. It tracks bracket depth, whether anything but
/// whitespace came before on the line and the previous significant byte.
/// Comments and triple-quoted literals (docstrings, SQL) are skipped whole.
/// A one-line literal is dropped when it is built at runtime (`+` on either
/// side, `%` or `.format` after it, or a string on either side) or stands
/// alone as a statement,
/// including right after the colon of a `def` or `class` header on the same
/// line (a one-line docstring).
fn scan_python(text: &str, file: &str, refs: &mut TagRefs) {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut depth: i32 = 0;
    let mut line_started = false;
    let mut last_sig: u8 = 0;
    // A `def` or `class` header whose colon has not been seen yet, and the
    // header colon as the last token: the next string may be a docstring.
    let mut in_header = false;
    let mut after_header_colon = false;
    while i < len {
        let b = bytes[i];
        match b {
            b'\n' => {
                line_started = false;
                if depth == 0 {
                    last_sig = 0;
                    in_header = false;
                    after_header_colon = false;
                }
                i += 1;
            }
            b' ' | b'\t' | b'\r' | 0x0c => i += 1,
            b'#' => {
                while i < len && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'\\' if bytes.get(i + 1) == Some(&b'\n') => i += 2,
            b'(' | b'[' | b'{' => {
                depth += 1;
                line_started = true;
                last_sig = b;
                after_header_colon = false;
                i += 1;
            }
            b')' | b']' | b'}' => {
                depth = (depth - 1).max(0);
                line_started = true;
                last_sig = b;
                after_header_colon = false;
                i += 1;
            }
            b'"' | b'\'' | b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'0'..=b'9' => {
                // An identifier run is one token; a short one directly before
                // a quote is a string prefix.
                let mut prefix_len = 0;
                if b != b'"' && b != b'\'' {
                    let mut end = i;
                    while bytes
                        .get(end)
                        .is_some_and(|&c| c.is_ascii_alphanumeric() || c == b'_')
                    {
                        end += 1;
                    }
                    match string_prefix_at(bytes, i).filter(|&n| i + n == end) {
                        Some(n) => prefix_len = n,
                        None => {
                            if !line_started
                                && depth == 0
                                && matches!(&bytes[i..end], b"def" | b"class")
                            {
                                in_header = true;
                            }
                            line_started = true;
                            last_sig = b'a';
                            after_header_colon = false;
                            i = end;
                            continue;
                        }
                    }
                }
                let quote_at = i + prefix_len;
                let quote = bytes[quote_at];
                let prefix = &bytes[i..quote_at];
                let previous = last_sig;
                let started = line_started;
                let doc_slot = after_header_colon;
                line_started = true;
                last_sig = b'"';
                after_header_colon = false;
                if bytes.get(quote_at + 1) == Some(&quote)
                    && bytes.get(quote_at + 2) == Some(&quote)
                {
                    // Triple-quoted: a docstring or SQL. Skipped whole, so a path in one is not
                    // checked; the limit text says so.
                    let mut j = quote_at + 3;
                    while j < len {
                        if bytes[j] == b'\\' {
                            j += 2;
                        } else if bytes[j] == quote
                            && bytes.get(j + 1) == Some(&quote)
                            && bytes.get(j + 2) == Some(&quote)
                        {
                            j += 3;
                            break;
                        } else {
                            j += 1;
                        }
                    }
                    i = j.min(len);
                    continue;
                }
                let start = quote_at + 1;
                let mut j = start;
                let mut closed = false;
                while j < len {
                    match bytes[j] {
                        b'\\' => j += 2,
                        b'\n' => break,
                        c if c == quote => {
                            closed = true;
                            break;
                        }
                        _ => j += 1,
                    }
                }
                if !closed {
                    i = j.min(len);
                    continue;
                }
                i = j + 1;
                // Only an f-string is built at runtime. Jython 2.7 reads a
                // `b`, `br` or `rb` literal as a plain string.
                let formatted = prefix.iter().any(|c| c.eq_ignore_ascii_case(&b'f'));
                let after_blank = next_significant(bytes, i, false);
                let ends = matches!(bytes.get(after_blank), None | Some(b'\n' | b'#' | b';'));
                let alone = !started && depth == 0 && ends;
                // `def f(): "doc"` on one line: a docstring, not a reference.
                let docstring = doc_slot && depth == 0 && ends;
                let next = next_significant(bytes, i, depth > 0);
                let built = match bytes.get(next) {
                    Some(b'+' | b'%' | b'"' | b'\'') => true,
                    Some(b'.') => bytes[next + 1..].starts_with(b"format"),
                    Some(c) if c.is_ascii_alphabetic() => string_prefix_at(bytes, next).is_some(),
                    _ => false,
                };
                // A `+` or another string before it builds it at runtime too:
                // `"a" "[p]x"` is one value, so no piece is a reference.
                if formatted || alone || docstring || built || matches!(previous, b'+' | b'"') {
                    continue;
                }
                if let Some((provider, rest)) = text.get(start..j).and_then(parse_literal) {
                    refs.add(provider, rest, file);
                }
            }
            _ => {
                // The first colon at depth 0 ends a `def` or `class` header:
                // default-argument and dict colons are inside brackets.
                if b == b':' && depth == 0 && in_header {
                    after_header_colon = true;
                    in_header = false;
                } else {
                    after_header_colon = false;
                }
                line_started = true;
                last_sig = b;
                i += 1;
            }
        }
    }
}

/// Quoted one-line `'[p]path'` / `"[p]path"` literals of any text, for an
/// expression or JSON that did not parse. A literal followed by `+` is built
/// at runtime and is dropped.
fn scan_quoted(text: &str, file: &str, refs: &mut TagRefs) {
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
            let built = bytes.get(next_significant(bytes, j + 1, false)) == Some(&b'+');
            if !built && let Some((provider, rest)) = text.get(start..j).and_then(parse_literal) {
                refs.add(provider, rest, file);
            }
            i = j + 1;
        } else {
            i = j.max(start);
        }
    }
}

/// `{[provider]path}` expression references.
fn scan_braced(text: &str, file: &str, refs: &mut TagRefs) {
    let mut rest = text;
    while let Some(open) = rest.find("{[") {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else { break };
        if let Some((provider, path)) = parse_literal(&after[..close]) {
            refs.add(provider, path, file);
        }
        rest = &after[close + 1..];
    }
}

/// One JSON string value under `key`.
fn scan_json_string(key: Option<&str>, value: &str, file: &str, refs: &mut TagRefs) {
    // `expression` is the expression language, not Python.
    if let Some(key) = key
        && key != "expression"
        && SCRIPT_KEYS.contains(&key)
    {
        scan_python(value, file, refs);
    } else if let Some((provider, rest)) = parse_literal(value) {
        refs.add(provider, rest, file);
    } else {
        scan_braced(value, file, refs);
        scan_quoted(value, file, refs);
    }
}

fn walk_json(value: &serde_json::Value, key: Option<&str>, file: &str, refs: &mut TagRefs) {
    match value {
        serde_json::Value::String(text) => scan_json_string(key, text, file, refs),
        serde_json::Value::Array(items) => {
            for item in items {
                walk_json(item, key, file, refs);
            }
        }
        serde_json::Value::Object(map) => {
            for (name, item) in map {
                walk_json(item, Some(name), file, refs);
            }
        }
        _ => {}
    }
}

/// Every tag reference in the artifact `zip`.
///
/// Each member is read through a limit of `MEMBER_READ_MAX + 1` bytes and
/// judged by the bytes actually read, never by the size the zip declares. A
/// member that reads past [`MEMBER_READ_MAX`] is scanned in part and recorded
/// for [`TagRefs::cut_reason`]. A binary member is skipped before that
/// decision and is never reported as cut.
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
        // The raw name picked the member; every output gets the written-out one.
        let shown = printable_name(&name);
        let mut bytes = Vec::new();
        entry
            .take(MEMBER_READ_MAX + 1)
            .read_to_end(&mut bytes)
            .map_err(|err| CoreError::Internal(format!("cannot decompress a zip member: {err}")))?;
        if bytes.iter().take(BINARY_PROBE_BYTES).any(|&b| b == 0) {
            continue;
        }
        if bytes.len() as u64 > MEMBER_READ_MAX {
            refs.cut.insert(shown.clone());
            bytes.truncate(MEMBER_READ_MAX as usize);
            // The bytes after the last newline may belong to a statement that
            // continues past the cut, so a literal there is not a whole one.
            // A prefix with no newline (minified JSON) is kept whole.
            if let Some(last) = bytes.iter().rposition(|&b| b == b'\n') {
                bytes.truncate(last + 1);
            }
        }
        if name.ends_with(".py") {
            scan_python(&String::from_utf8_lossy(&bytes), &shown, &mut refs);
        } else {
            match serde_json::from_slice::<serde_json::Value>(&bytes) {
                Ok(value) => walk_json(&value, None, &shown, &mut refs),
                Err(_) => scan_quoted(&String::from_utf8_lossy(&bytes), &shown, &mut refs),
            }
        }
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

/// One node of an export, with only the three fields the lookup needs. The
/// rest of a tag's definition is skipped by serde, so a 48 MB export costs
/// three small fields per node and never a `serde_json::Value`.
#[derive(Debug, Deserialize)]
struct RawNode {
    #[serde(default)]
    name: String,
    #[serde(default, rename = "typeId")]
    type_id: Option<String>,
    #[serde(default)]
    tags: Vec<RawNode>,
}

/// The root of an export. Unlike a child it must say it is a `Provider` and
/// must hold a `tags` array: `tags` is `None` when the key is missing, so a
/// truncated or foreign document is told from an empty provider.
#[derive(Debug, Deserialize)]
struct RawRoot {
    #[serde(default, rename = "tagType")]
    tag_type: Option<String>,
    #[serde(default, rename = "typeId")]
    type_id: Option<String>,
    #[serde(default)]
    tags: Option<Vec<RawNode>>,
}

#[derive(Debug, Default)]
struct Node {
    type_id: Option<String>,
    children: BTreeMap<String, Node>,
}

impl Node {
    fn from_raw(raw: RawNode) -> Node {
        let mut node = Node {
            type_id: raw.type_id,
            children: BTreeMap::new(),
        };
        for child in raw.tags {
            let key = child.name.to_lowercase();
            let child = Node::from_raw(child);
            node.children.entry(key).or_default().absorb(child);
        }
        node
    }

    /// Two nodes of one name merge: a tag the other one lacks is kept.
    fn absorb(&mut self, other: Node) {
        if self.type_id.is_none() {
            self.type_id = other.type_id;
        }
        for (key, child) in other.children {
            self.children.entry(key).or_default().absorb(child);
        }
    }
}

/// How many `typeId` links one node follows before the lookup gives up.
const TYPE_CHAIN_MAX: usize = 32;

/// The folder that holds a provider's UDT definitions.
const TYPES_FOLDER: &str = "_types_";

/// The tag tree of one provider, from its native export.
#[derive(Debug)]
pub struct TagTree {
    root: Node,
    /// The provider this tree belongs to, folded, to read `[provider]` in a
    /// `typeId`.
    provider: String,
}

impl TagTree {
    /// Parse the export of provider `provider`. Anything that is not a
    /// `Provider` root holding a `tags` array is refused.
    ///
    /// An 8.3.6 gateway writes an empty provider as a `Provider` root whose
    /// `tags` array holds only the structural `_types_` folder (captured from a
    /// live rig, quick task 261106-tel). A root with no `tags` key is not that
    /// shape, so it is refused rather than read as an empty tree: a truncated
    /// or foreign document would otherwise report every path as missing. Only
    /// the root is held to this. A child with no `tags` key is a leaf, as the
    /// `_types_` folder of that same export is.
    pub fn parse(bytes: &[u8], provider: &str) -> Result<TagTree, CoreError> {
        let raw: RawRoot = serde_json::from_slice(bytes).map_err(|err| {
            CoreError::Internal(format!("the tag export is not a tag tree: {err}"))
        })?;
        if raw.tag_type.as_deref() != Some("Provider") {
            return Err(CoreError::Internal(
                "the tag export is not a tag tree: the root is not a Provider".to_string(),
            ));
        }
        let Some(tags) = raw.tags else {
            return Err(CoreError::Internal(
                "the tag export is not a tag tree: the root holds no tags array".to_string(),
            ));
        };
        Ok(TagTree {
            root: Node::from_raw(RawNode {
                name: String::new(),
                type_id: raw.type_id,
                tags,
            }),
            provider: provider.to_ascii_lowercase(),
        })
    }

    /// The type definition a `typeId` names, or `None` when it cannot be
    /// resolved in this tree (another provider, an unknown type).
    fn type_node(&self, type_id: &str) -> Option<&Node> {
        let mut id = type_id.trim();
        if let Some(qualified) = id.strip_prefix('[') {
            let (provider, rest) = qualified.split_once(']')?;
            if !provider.eq_ignore_ascii_case(&self.provider) {
                return None;
            }
            id = rest;
        }
        let folder = format!("{TYPES_FOLDER}/");
        // `get` is boundary-safe: a multi-byte character across the offset is
        // not the prefix, and gateway data must never panic a warning.
        if let Some(head) = id.get(..folder.len())
            && head.eq_ignore_ascii_case(&folder)
        {
            id = &id[folder.len()..];
        }
        let mut node = self.root.children.get(TYPES_FOLDER)?;
        for segment in id.split('/') {
            if segment.is_empty() {
                return None;
            }
            node = node.children.get(&segment.to_lowercase())?;
        }
        Some(node)
    }

    /// `node` and every type definition its `typeId` chain reaches, in order.
    /// Sets `unknown` when a link cannot be resolved, when the chain repeats
    /// and when it is longer than [`TYPE_CHAIN_MAX`].
    fn expand<'a>(&'a self, node: &'a Node, unknown: &mut bool) -> Vec<&'a Node> {
        let mut chain = vec![node];
        let mut current = node;
        for _ in 0..TYPE_CHAIN_MAX {
            let Some(type_id) = current.type_id.as_deref() else {
                return chain;
            };
            let Some(next) = self.type_node(type_id) else {
                *unknown = true;
                return chain;
            };
            if chain.iter().any(|seen| std::ptr::eq(*seen, next)) {
                *unknown = true;
                return chain;
            }
            chain.push(next);
            current = next;
        }
        // A chain of exactly the guard whose last type names no further type
        // is complete; only a link past the guard is unknown.
        if current.type_id.is_some() {
            *unknown = true;
        }
        chain
    }

    /// Whether `path` (the part after `[provider]`) is in the tree.
    ///
    /// Each segment widens the frontier to the nodes themselves plus their
    /// type chains, then keeps every child of that name. A member an instance
    /// stub lacks (or carries without a `typeId`) is therefore still found in
    /// its type. [`Lookup::Unknown`] answers when the walk dead-ends after a
    /// link it could not follow.
    pub fn lookup(&self, path: &str) -> Lookup {
        let mut frontier: Vec<&Node> = vec![&self.root];
        let mut unknown = false;
        for segment in path.split('/') {
            let key = segment.to_lowercase();
            let mut next: Vec<&Node> = Vec::new();
            for node in &frontier {
                for expanded in self.expand(node, &mut unknown) {
                    if let Some(child) = expanded.children.get(&key)
                        && !next.iter().any(|seen| std::ptr::eq(*seen, child))
                    {
                        next.push(child);
                    }
                }
            }
            if next.is_empty() {
                return if unknown {
                    Lookup::Unknown
                } else {
                    Lookup::Missing
                };
            }
            frontier = next;
        }
        Lookup::Found
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::{parse_literal, scan_artifact};
    use crate::deploy::project_artifact::is_format_char;

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

    /// Jython 2.7 reads a `b"..."` literal as a plain string, so it is a
    /// reference; an f-string is built at runtime and is not.
    #[test]
    fn scan_python_bytes_literal_is_a_reference_and_an_f_string_is_not() {
        let code = "a = b\"[default]A/B\"\n\
                    b = br'[default]C/D'\n\
                    c = Rb\"[default]E/F\"\n\
                    d = f\"[default]G/H\"\n\
                    e = rf'[default]I/J'\n";
        assert_eq!(py(code), ["[default]A/B", "[default]C/D", "[default]E/F"]);
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

    /// A one-line string standing alone after the colon of a `def` or `class`
    /// header on the same line is a docstring. A string after `return`, after
    /// `lambda:` or in a statement after the header is still a reference.
    #[test]
    fn scan_python_one_line_docstring_after_a_header_is_not_a_reference() {
        let code = "def example(): \"[default]Area/Example\"\n\
                    class Holder(object): '[default]Area/Doc'\n\
                    def g(x=\":\", d={\"k\": 1}): \"[default]Area/Doc2\"\n\
                    def h(): \"[default]Area/Doc3\"  # c\n\
                    def f(): return \"[default]A/B\"\n\
                    k = lambda: \"[default]C/D\"\n\
                    class K: x = \"[default]E/F\"\n";
        assert_eq!(py(code), ["[default]A/B", "[default]C/D", "[default]E/F"]);
    }

    /// Adjacent string literals are one runtime value, so no piece of the
    /// concatenation is a reference, the last one included.
    #[test]
    fn scan_python_last_piece_of_adjacent_strings_is_not_a_reference() {
        let code = "x = (\"see \" \"[default]Area/Absent\")\n\
                    y = (\"see \"\n     \"[default]Area/Other\")\n\
                    z = \"a\" \"[default]Area/Third\"\n\
                    w = [\"a\", \"[default]C/D\"]\n";
        assert_eq!(py(code), ["[default]C/D"]);
    }

    /// A guard: a triple-quoted string is skipped whole and the limit text
    /// says so. A change to this needs a change to the limit text.
    #[test]
    fn scan_python_assigned_triple_quoted_string_is_not_scanned() {
        let code = "x = \"\"\"[default]A/B\"\"\"\n\
                    y = '''[default]C/D'''\n";
        assert_eq!(py(code), Vec::<String>::new());
        assert!(super::MISSING_TAGS_LIMIT.contains("triple-quoted Python strings are not scanned"));
    }

    /// A guard: a path through an unresolved UDT type is reported as not
    /// checked, and the limit text says so.
    #[test]
    fn missing_tags_limit_names_unresolved_udt_types() {
        assert!(
            super::MISSING_TAGS_LIMIT.contains(
                "a path through a UDT type the tag export does not resolve isn't checked"
            )
        );
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
            ("bidi override in the rest", "[default]A\u{202e}B"),
            ("line separator in the rest", "[default]A\u{2028}B"),
            ("paragraph separator in the rest", "[default]A\u{2029}B"),
            ("zero-width space in the rest", "[default]A\u{200b}B"),
            ("bidi override in the provider", "[de\u{202e}fault]A"),
            ("byte order mark in the provider", "[\u{feff}default]A"),
        ];
        for (why, text) in refused {
            assert_eq!(parse_literal(text), None, "{why}: {text:?}");
        }
        assert_eq!(
            parse_literal("[default]A/it's"),
            Some(("default", "A/it's"))
        );
    }

    // ---- the tree -----------------------------------------------------------

    use serde_json::{Value, json};

    use super::{Lookup, TagTree};

    fn atom(name: &str) -> Value {
        json!({"name": name, "tagType": "AtomicTag"})
    }

    fn folder(name: &str, tags: Vec<Value>) -> Value {
        json!({"name": name, "tagType": "Folder", "tags": tags})
    }

    fn udt_type(name: &str, type_id: Option<&str>, tags: Vec<Value>) -> Value {
        let mut node = json!({"name": name, "tagType": "UdtType", "tags": tags});
        if let Some(type_id) = type_id {
            node["typeId"] = json!(type_id);
        }
        node
    }

    fn instance(name: &str, type_id: Option<&str>, tags: Vec<Value>) -> Value {
        let mut node = json!({"name": name, "tagType": "UdtInstance", "tags": tags});
        if let Some(type_id) = type_id {
            node["typeId"] = json!(type_id);
        }
        node
    }

    fn tree_of(tags: Vec<Value>) -> TagTree {
        let root = json!({"name": "", "tagType": "Provider", "tags": tags});
        TagTree::parse(root.to_string().as_bytes(), "default").expect("a tag tree")
    }

    /// The captured 8.3.6 export (a Provider root, a `_types_` folder, an
    /// instance carrying one stub per member) plus the shapes the capture did
    /// not hold: instances with no stubs, nested instances, inheritance,
    /// qualified type ids, unknown types and cycles.
    fn captured() -> TagTree {
        tree_of(vec![
            json!({"name": "R1", "tagType": "AtomicTag", "dataType": "Int4", "value": 1}),
            folder("F", vec![atom("F1"), atom("v1.2")]),
            instance("RInst", Some("A/B/T"), vec![atom("M1")]),
            instance("Bare", Some("A/B/T"), vec![]),
            instance("O", Some("A/B/Outer"), vec![]),
            instance(
                "O2",
                Some("A/B/Outer"),
                vec![instance("Inner", None, vec![])],
            ),
            instance("C", Some("A/B/Child"), vec![]),
            instance("Q", Some("[default]_types_/A/B/T"), vec![]),
            instance("X", Some("[other]A/B/T"), vec![]),
            instance("Y", Some("No/Such"), vec![]),
            instance("Z", Some("CycA"), vec![]),
            instance("Deep", Some("Chain00"), vec![]),
            folder(
                "_types_",
                [
                    vec![
                        udt_type("T0", None, vec![atom("M0")]),
                        folder(
                            "A",
                            vec![folder(
                                "B",
                                vec![
                                    udt_type("T", None, vec![atom("M1")]),
                                    udt_type(
                                        "Outer",
                                        None,
                                        vec![instance("Inner", Some("A/B/T"), vec![])],
                                    ),
                                    udt_type("Child", Some("A/B/T"), vec![atom("M2")]),
                                ],
                            )],
                        ),
                        udt_type("CycA", Some("CycB"), vec![]),
                        udt_type("CycB", Some("CycA"), vec![]),
                    ],
                    (0..40)
                        .map(|n| {
                            let next = format!("Chain{:02}", n + 1);
                            let own = format!("Own{n:02}");
                            udt_type(
                                &format!("Chain{n:02}"),
                                (n < 39).then_some(next.as_str()),
                                vec![atom(&own)],
                            )
                        })
                        .collect(),
                ]
                .concat(),
            ),
        ])
    }

    #[test]
    fn tree_instance_with_stubs_resolves_its_members() {
        assert_eq!(captured().lookup("RInst/M1"), Lookup::Found);
    }

    #[test]
    fn tree_instance_with_no_stubs_resolves_through_its_type() {
        assert_eq!(captured().lookup("Bare/M1"), Lookup::Found);
    }

    #[test]
    fn tree_member_of_a_nested_instance_inside_a_type_resolves() {
        assert_eq!(captured().lookup("O/Inner/M1"), Lookup::Found);
    }

    #[test]
    fn tree_instance_stub_with_no_type_id_still_has_the_type_members() {
        assert_eq!(captured().lookup("O2/Inner/M1"), Lookup::Found);
    }

    #[test]
    fn tree_type_inherits_the_members_of_its_own_type_id() {
        let tree = captured();
        assert_eq!(tree.lookup("C/M1"), Lookup::Found);
        assert_eq!(tree.lookup("C/M2"), Lookup::Found);
    }

    #[test]
    fn tree_qualified_type_id_resolves_and_another_provider_is_unknown() {
        let tree = captured();
        assert_eq!(tree.lookup("Q/M1"), Lookup::Found);
        assert_eq!(tree.lookup("X/M1"), Lookup::Unknown);
        assert_eq!(tree.lookup("X/Nope"), Lookup::Unknown);
    }

    #[test]
    fn tree_unknown_type_and_type_cycle_are_unknown_and_return_at_once() {
        let tree = captured();
        assert_eq!(tree.lookup("Y/M1"), Lookup::Unknown);
        assert_eq!(tree.lookup("Z/Nope"), Lookup::Unknown);
    }

    #[test]
    fn tree_a_type_chain_past_the_guard_is_unknown_but_near_members_resolve() {
        let tree = captured();
        assert_eq!(tree.lookup("Deep/Own05"), Lookup::Found);
        assert_eq!(tree.lookup("Deep/Own39"), Lookup::Unknown);
    }

    /// `count` UDT types `{prefix}01`.. each naming the next by `typeId`, the
    /// last naming none, each holding atom `OwnNN`.
    fn type_chain(prefix: &str, count: usize) -> Vec<Value> {
        (1..=count)
            .map(|n| {
                let next = format!("{prefix}{:02}", n + 1);
                udt_type(
                    &format!("{prefix}{n:02}"),
                    (n < count).then_some(next.as_str()),
                    vec![atom(&format!("Own{n:02}"))],
                )
            })
            .collect()
    }

    /// The guard counts links: a chain of exactly 32 whose last type names no
    /// further type is complete, so an absent member is missing. One more
    /// link is past the guard.
    #[test]
    fn tree_a_type_chain_of_exactly_the_guard_is_complete() {
        let tree = tree_of(vec![
            instance("Exact", Some("E01"), vec![]),
            instance("Over", Some("F01"), vec![]),
            folder(
                "_types_",
                [type_chain("E", 32), type_chain("F", 33)].concat(),
            ),
        ]);
        assert_eq!(tree.lookup("Exact/Nope"), Lookup::Missing);
        assert_eq!(tree.lookup("Exact/Own32"), Lookup::Found);
        assert_eq!(tree.lookup("Over/Nope"), Lookup::Unknown);
        assert_eq!(tree.lookup("Over/Own01"), Lookup::Found);
    }

    #[test]
    fn tree_missing_means_the_tree_is_understood_and_lacks_the_tag() {
        let tree = captured();
        assert_eq!(tree.lookup("R1/leaf"), Lookup::Missing);
        assert_eq!(tree.lookup("Bare/Nope"), Lookup::Missing);
        assert_eq!(tree.lookup("F/Nope"), Lookup::Missing);
        assert_eq!(tree.lookup("Nope"), Lookup::Missing);
    }

    #[test]
    fn tree_letter_case_is_ignored() {
        let tree = captured();
        assert_eq!(tree.lookup("rinst/m1"), Lookup::Found);
        assert_eq!(tree.lookup("_TYPES_/a/b/t/m1"), Lookup::Found);
    }

    #[test]
    fn tree_lookup_finds_a_tag_named_with_a_dot() {
        assert_eq!(captured().lookup("F/v1.2"), Lookup::Found);
    }

    #[test]
    fn tree_refuses_what_is_not_a_provider_export() {
        for (why, bytes) in [
            ("empty object", b"{}".as_slice()),
            ("array", b"[]".as_slice()),
            (
                "folder root",
                br#"{"name":"x","tagType":"Folder"}"#.as_slice(),
            ),
            ("not json", b"<html>".as_slice()),
            (
                "tags not an array",
                br#"{"tagType":"Provider","tags":3}"#.as_slice(),
            ),
            (
                "provider root with no tags",
                br#"{"name":"","tagType":"Provider"}"#.as_slice(),
            ),
            (
                "provider root with tags null",
                br#"{"tagType":"Provider","tags":null}"#.as_slice(),
            ),
        ] {
            assert!(TagTree::parse(bytes, "default").is_err(), "{why}");
        }
    }

    /// What an 8.3.6 gateway answers for a provider that holds no tag (captured
    /// from a live rig, quick task 261106-tel): the root has a `tags` array
    /// whose only member is the structural `_types_` folder, and that folder
    /// has no `tags` key.
    const EMPTY_PROVIDER_EXPORT: &[u8] = br#"{
  "name": "",
  "tagType": "Provider",
  "tags": [
    {
      "name": "_types_",
      "tagType": "Folder"
    }
  ]
}"#;

    /// A guard: the gateway's own empty export is an empty tree, so a path in
    /// it is missing, not unknown.
    #[test]
    fn tree_empty_provider_export_as_the_gateway_writes_it_is_an_empty_tree() {
        let tree = TagTree::parse(EMPTY_PROVIDER_EXPORT, "default").expect("a tag tree");
        assert_eq!(tree.lookup("A/B"), Lookup::Missing);
    }

    /// A guard: only the root needs a `tags` key. A child with none is a leaf,
    /// as the `_types_` folder of the gateway's own empty export is.
    #[test]
    fn tree_child_with_no_tags_key_is_a_leaf() {
        let tree = TagTree::parse(
            br#"{"tagType":"Provider","tags":[{"name":"A","tagType":"AtomicTag"}]}"#,
            "default",
        )
        .expect("a tag tree");
        assert_eq!(tree.lookup("A"), Lookup::Found);
        assert_eq!(tree.lookup("A/B"), Lookup::Missing);
    }

    // ---- multi-byte text from a repo or a gateway never panics ------------------

    #[test]
    fn tree_type_id_with_a_multibyte_character_across_the_types_prefix_is_unknown() {
        // `_types_/` is 8 bytes; the e-acute spans bytes 7 and 8.
        let tree = tree_of(vec![
            instance("A", Some("abcdefg\u{e9}x/T"), vec![]),
            instance("B", Some("[d\u{e9}fault]X"), vec![]),
            instance("C", Some("\u{65e5}\u{672c}\u{8a9e}/T"), vec![]),
            folder("_types_", vec![udt_type("T", None, vec![atom("M")])]),
        ]);
        assert_eq!(tree.lookup("A/M"), Lookup::Unknown);
        assert_eq!(tree.lookup("B/M"), Lookup::Unknown);
        assert_eq!(tree.lookup("C/M"), Lookup::Unknown);
        assert_eq!(tree.lookup("\u{e9}/\u{fc}/\u{65e5}"), Lookup::Missing);
    }

    #[test]
    fn tree_type_id_with_a_multibyte_prefix_still_resolves_a_real_type() {
        let tree = tree_of(vec![
            instance("A", Some("_TYPES_/T\u{e9}"), vec![]),
            folder("_types_", vec![udt_type("T\u{e9}", None, vec![atom("M")])]),
        ]);
        assert_eq!(tree.lookup("A/M"), Lookup::Found);
    }

    #[test]
    fn scan_multibyte_text_around_literals_does_not_panic_and_keeps_the_path() {
        let code = "x = [\"\u{e9}[default]A/B\", \"[default]\u{dc}n\u{ef}/\u{c7}\", \"\u{65e5}\u{672c}[x]\"]\n\
                    y = '[d\u{e9}fault]\u{e9}/\u{fc}' # \u{65e5}\n";
        assert_eq!(
            py(code),
            [
                "[default]\u{dc}n\u{ef}/\u{c7}",
                "[d\u{e9}fault]\u{e9}/\u{fc}"
            ]
        );
        assert_eq!(
            json("{\"expression\":\"\u{e9}{[d\u{e9}f]\u{e9}/\u{fc}} + '\u{65e5}[x]\u{e9}'\"}"),
            ["[d\u{e9}f]\u{e9}/\u{fc}"]
        );
        assert_eq!(parse_literal("[\u{e9}]\u{e9}"), Some(("\u{e9}", "\u{e9}")));
        assert_eq!(parse_literal("[\u{e9}\u{e9}"), None);
    }

    // ---- members over the read limit (quick task 261106-nj6) ----

    const LIMIT: usize = super::MEMBER_READ_MAX as usize;

    /// Like [`zip_of`] but Stored: no compression work for a 16 MiB member.
    fn stored_zip_of(members: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, bytes) in members {
            writer.start_file(*name, options).expect("start");
            writer.write_all(bytes).expect("write");
        }
        writer.finish().expect("finish").into_inner()
    }

    /// Writes `size` as the declared uncompressed size of a one-member zip:
    /// the last central directory record, offset 24.
    fn declare_size(zip: &mut [u8], size: u32) {
        let at = zip
            .windows(4)
            .rposition(|w| w == b"PK\x01\x02")
            .expect("a central directory record");
        zip[at + 24..at + 28].copy_from_slice(&size.to_le_bytes());
    }

    /// `before`, then one `#` comment line that brings the length to exactly
    /// the limit. No NUL.
    fn pad_to_limit(before: &str) -> String {
        let filler = LIMIT - before.len();
        format!("{before}{}\n", "#".repeat(filler - 1))
    }

    /// A comment line, then `tail`, so that the whole is exactly the limit.
    fn tail_at_limit(tail: &str) -> String {
        format!("{}\n{tail}", "#".repeat(LIMIT - tail.len() - 1))
    }

    fn names_in(refs: &super::TagRefs) -> Vec<String> {
        let mut all: Vec<String> = refs
            .iter()
            .map(|r| format!("[{}]{}", r.provider, r.path))
            .collect();
        all.sort();
        all
    }

    fn cut_of(refs: &super::TagRefs) -> Vec<&str> {
        refs.cut.iter().map(String::as_str).collect()
    }

    #[test]
    fn scan_member_over_the_limit_is_named_and_a_reference_past_it_is_not_seen() {
        let code = pad_to_limit("x = [\"[default]Early/A\"]\n") + "y = [\"[default]Past/Limit\"]\n";
        let refs = scan_artifact(&zip_of(&[(PY, code.as_bytes())])).expect("scan");
        assert_eq!(names_in(&refs), ["[default]Early/A"]);
        assert_eq!(cut_of(&refs), [PY]);
    }

    #[test]
    fn scan_member_exactly_at_the_limit_is_a_full_scan() {
        let code = tail_at_limit("x = [\"[default]Last/Line\"]\n");
        assert_eq!(code.len(), LIMIT);
        let refs = scan_artifact(&stored_zip_of(&[(PY, code.as_bytes())])).expect("scan");
        assert_eq!(names_in(&refs), ["[default]Last/Line"]);
        assert!(refs.cut.is_empty(), "{:?}", refs.cut);
        assert_eq!(refs.cut_reason(), None);
    }

    #[test]
    fn scan_member_one_byte_over_the_limit_is_cut() {
        let code = tail_at_limit("x = [\"[default]Last/Line\"]\n") + "\n";
        assert_eq!(code.len(), LIMIT + 1);
        let refs = scan_artifact(&stored_zip_of(&[(PY, code.as_bytes())])).expect("scan");
        assert_eq!(names_in(&refs), ["[default]Last/Line"]);
        assert_eq!(cut_of(&refs), [PY]);
    }

    #[test]
    fn scan_member_whose_declared_size_lies_is_judged_by_what_it_holds() {
        // Understated: the member holds more than the limit, the zip says 10.
        let code = pad_to_limit("x = [\"[default]Early/A\"]\n") + "y = [\"[default]Past/Limit\"]\n";
        let mut zip = stored_zip_of(&[(PY, code.as_bytes())]);
        declare_size(&mut zip, 10);
        let refs = scan_artifact(&zip).expect("scan");
        assert_eq!(names_in(&refs), ["[default]Early/A"]);
        assert_eq!(cut_of(&refs), [PY]);

        // Overstated: a small member the zip says is 100 MiB is not cut.
        let mut zip = stored_zip_of(&[(PY, b"x = [\"[default]A/B\"]\n")]);
        declare_size(&mut zip, 100 * 1024 * 1024);
        let refs = scan_artifact(&zip).expect("scan");
        assert_eq!(names_in(&refs), ["[default]A/B"]);
        assert!(refs.cut.is_empty(), "{:?}", refs.cut);
    }

    #[test]
    fn scan_oversized_member_the_scan_does_not_read_is_not_reported() {
        let big = vec![b'a'; LIMIT + 10];
        let mut binary = vec![b'a'; LIMIT + 10];
        binary[100] = 0;
        let refs = scan_artifact(&stored_zip_of(&[
            (PY, &binary),
            ("ignition/images/big.png", &big),
            ("ignition/script-python/__tests__/t/code.py", &big),
            ("ignition/script-python/pkg/resource.json", &big),
        ]))
        .expect("scan");
        assert!(refs.cut.is_empty(), "{:?}", refs.cut);
        assert_eq!(refs.cut_reason(), None);
    }

    #[test]
    fn scan_json_over_the_limit_reports_plain_values_before_the_cut() {
        let head = "{\"tagPath\":\"[default]Early/A\",\"pad\":\"";
        let tail = "\",\"late\":\"[default]Past/Limit\"}";
        let text = format!("{head}{}{tail}", "a".repeat(LIMIT - head.len()));
        assert_eq!(text.len(), LIMIT + tail.len());
        let refs = scan_artifact(&stored_zip_of(&[(VIEW, text.as_bytes())])).expect("scan");
        assert_eq!(names_in(&refs), ["[default]Early/A"]);
        assert_eq!(cut_of(&refs), [VIEW]);
    }

    /// The last statement of the part read may go on past the cut, so a literal
    /// there is not a whole literal: a false "missing" is worse than a miss.
    #[test]
    fn scan_member_cut_keeps_whole_lines_so_a_built_literal_is_not_a_reference() {
        let literal = "x = \"[default]Built/Path\"";
        let code = format!("{} + y\n", tail_at_limit(literal));
        assert_eq!(code.len(), LIMIT + " + y\n".len());
        assert_eq!(code.as_bytes()[LIMIT - 1], b'"');
        let refs = scan_artifact(&stored_zip_of(&[(PY, code.as_bytes())])).expect("scan");
        assert_eq!(names_in(&refs), Vec::<String>::new());
        assert_eq!(cut_of(&refs), [PY]);
    }

    #[test]
    fn cut_reason_is_none_when_nothing_was_cut_and_names_three_files_and_counts_the_rest() {
        let mut refs = super::TagRefs::default();
        assert_eq!(refs.cut_reason(), None);
        refs.cut.insert("b/code.py".to_string());
        assert_eq!(
            refs.cut_reason().as_deref(),
            Some("the part past the 16 MiB read limit of 1 project file(s): b/code.py")
        );
        for name in ["e/code.py", "a/code.py", "d/code.py", "c/code.py"] {
            refs.cut.insert(name.to_string());
        }
        assert_eq!(
            refs.cut_reason().as_deref(),
            Some(
                "the part past the 16 MiB read limit of 5 project file(s): \
                 a/code.py, b/code.py, c/code.py and 2 more"
            )
        );
    }

    // ---- member names reach confirmation prose and terminals (261106-tel) ----

    /// A member name that would fake a line of the prose and rewrite a terminal.
    const FAKING_DIR: &str = "a\nartifact_sha256 fake\u{1b}[2K\u{202e}z";

    #[test]
    fn scan_member_name_with_a_newline_an_escape_and_an_override_is_one_line_in_the_reason() {
        let name = format!("ignition/script-python/{FAKING_DIR}/code.py");
        let code = pad_to_limit("x = 1\n") + "y = 2\n";
        let refs =
            scan_artifact(&stored_zip_of(&[(name.as_str(), code.as_bytes())])).expect("scan");
        let reason = refs.cut_reason().expect("the member was cut");
        assert_eq!(
            reason,
            "the part past the 16 MiB read limit of 1 project file(s): \
             ignition/script-python/a\\nartifact_sha256 fake\\u{1b}[2K\\u{202e}z/code.py"
        );
        assert!(
            !reason.chars().any(|c| c.is_control() || is_format_char(c)),
            "{reason:?}"
        );
    }

    #[test]
    fn scan_member_name_is_written_out_in_the_files_of_a_reference() {
        for c in [
            '\u{2028}', '\u{2029}', '\u{200b}', '\u{feff}', '\u{7f}', '\u{9b}',
        ] {
            let name = format!("ignition/script-python/x{c}y/code.py");
            let refs = scan_artifact(&zip_of(&[(name.as_str(), b"t = ['[default]A/B']\n")]))
                .expect("scan");
            let files: Vec<&String> = refs.iter().flat_map(|r| r.files.iter()).collect();
            let want = format!("ignition/script-python/x{}y/code.py", c.escape_default());
            assert_eq!(files, [&want], "{c:?}");
        }
    }

    /// A guard: text a terminal shows as it is stays as it is.
    #[test]
    fn scan_member_name_keeps_plain_text_and_a_backslash_as_they_are() {
        let name = "ignition/script-python/caf\u{e9} \u{4e2d}\\x/code.py";
        let refs = scan_artifact(&zip_of(&[(name, b"t = ['[default]A/B']\n")])).expect("scan");
        let files: Vec<&String> = refs.iter().flat_map(|r| r.files.iter()).collect();
        assert_eq!(files, [&name.to_string()]);
    }
}
