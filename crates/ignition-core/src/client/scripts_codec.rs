//! The Ignition Flint script codec (07-04, INTR-01) — decode/encode
//! of the scripts EMBEDDED inside JSON resource members (Perspective
//! `view.json` component scripts, tag event scripts, …) into editable
//! `.py` sidecars, with a byte-exact unedited round-trip.
//!
//! PURE — the `client/resources.rs` discipline: zero
//! [`crate::client::GatewayApi`] surface, unit-testable without a
//! gateway. `ignition/script-python` members are ALREADY plain `.py`
//! text in the export (live-proven, 07-RESEARCH) and never decode —
//! this module targets the escaped string values under
//! [`SCRIPT_KEYS`] only (scope honesty, README "Script decode/
//! encode").
//!
//! ## The codec (ignition-nvim's exact contract, dual-ported there)
//!
//! - [`flint_encode`] — the ordered multi-pass replacement table,
//!   BACKSLASH FIRST.
//! - [`flint_decode`] — a SINGLE-PASS state machine (multi-pass
//!   cannot distinguish `\\t` from `\t`); unknown `\uXXXX` escapes
//!   keep the backslash.
//! - The invariant `flint_encode(flint_decode(x)) == x` is SACRED
//!   (over strings in the table's image — what Ignition writes).
//! - [`dedent`]/[`reindent`] strip/restore the common leading-TAB
//!   prefix (only non-empty lines reindent; whitespace-only lines
//!   normalize to empty — scripts with such lines re-encode with
//!   that one normalization, an accepted ignition-nvim semantic).
//!
//! ## Addressing = counter-named sidecars + JSON-pointer manifest
//!
//! `--decode-scripts` writes the export's members PLUS
//! `<member>.<n>.py` sidecar siblings PLUS a `scripts-manifest.json`
//! at the tree root mapping each member's JSON-pointer addresses →
//! `{sidecar, indent_prefix}`. The exported JSON stays MARKER-FREE
//! (gateway-clean — the manifest-aside beats markers, 07-RESEARCH
//! anti-patterns).
//!
//! ## Round-trip = raw byte-span splicing (NO preserve_order)
//!
//! [`decode_member`]/[`encode_member`] walk the member's RAW bytes
//! with ONE shared position-tracking scanner: decode resolves each
//! script string to its byte span, encode RE-RESOLVES each manifest
//! pointer in the CURRENT bytes (hand-edits stay valid) and splices
//! the re-encoded replacement at that span. serde_json's
//! `preserve_order` feature is deliberately NOT enabled — feature
//! unification is workspace-wide and would flip every
//! `serde_json::Value` map to insertion order, churning key order in
//! the existing Value-re-serializing goldens (`tags export -o -`,
//! doctor/webdev passthroughs). serde_json is used READ-ONLY here
//! (manifest parse/serialize; parse-to-Value walks are
//! order-agnostic); NO code path re-serializes a member `Value`.
//! Acceptance = byte-equality of UNEDITED re-encoded members (the
//! sacred invariant at file level).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// The JSON keys whose string values carry embedded scripts — the
/// ignition-nvim list verbatim (nine keys; the plan sketch said ten,
/// the dual-ported source is the authority); kept in sync by comment
/// reference (lua/ignition/json_parser.lua SCRIPT_KEYS /
/// ignition_lsp/json_scanner.py SCRIPT_KEYS).
pub const SCRIPT_KEYS: [&str; 9] = [
    "script",
    "code",
    "eventScript",
    "transform",
    "onActionPerformed",
    "onChange",
    "onStartup",
    "onShutdown",
    "expression",
];

/// The manifest file a decoded export tree carries at its root —
/// consumed + stripped on re-encode (it never enters an uploaded zip).
pub const MANIFEST_NAME: &str = "scripts-manifest.json";

// ---- The codec -------------------------------------------------------------

/// Encode plain text into the Ignition Flint JSON-string form — the
/// EXACT ordered multi-pass table (backslash FIRST so later passes
/// cannot double-escape), cross-validated Lua + Python in
/// ignition-nvim.
pub fn flint_encode(s: &str) -> String {
    // Backslash first (must be!), then the remaining pairs in the
    // table's order. Each later pass's pattern contains no backslash,
    // so order after the first is stable.
    let out = s.replace('\\', "\\\\");
    let out = out.replace('"', "\\\"");
    let out = out.replace('\t', "\\t");
    let out = out.replace('\u{8}', "\\b");
    let out = out.replace('\n', "\\n");
    let out = out.replace('\r', "\\r");
    let out = out.replace('\u{c}', "\\f");
    let out = out.replace('<', "\\u003c");
    let out = out.replace('>', "\\u003e");
    let out = out.replace('&', "\\u0026");
    let out = out.replace('=', "\\u003d");
    out.replace('\'', "\\u0027")
}

/// The single-char escapes the decoder maps (everything after a
/// backslash except `u`).
fn escape_char(next: char) -> Option<char> {
    match next {
        '\\' => Some('\\'),
        '"' => Some('"'),
        't' => Some('\t'),
        'b' => Some('\u{8}'),
        'n' => Some('\n'),
        'r' => Some('\r'),
        'f' => Some('\u{c}'),
        _ => None,
    }
}

/// The `\uXXXX` escapes the decoder maps (the Flint table's HTML
/// five; anything else keeps its backslash).
fn unicode_escape(hex: &str) -> Option<char> {
    match hex {
        "003c" => Some('<'),
        "003e" => Some('>'),
        "0026" => Some('&'),
        "003d" => Some('='),
        "0027" => Some('\''),
        _ => None,
    }
}

/// Decode the Ignition Flint JSON-string form back to plain text —
/// SINGLE-PASS (multi-pass cannot distinguish `\\t` — literal
/// backslash + t — from `\t` — a tab). Unknown `\uXXXX` and unknown
/// single escapes KEEP the backslash (the ignition-nvim semantics:
/// the escape sequence rides through verbatim for the re-encode).
pub fn flint_decode(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c != '\\' {
            out.push(c);
            i += 1;
            continue;
        }
        // A backslash with nothing after it rides verbatim.
        let Some(next) = chars.get(i + 1).copied() else {
            out.push(c);
            i += 1;
            continue;
        };
        if next == 'u' {
            if i + 5 < chars.len() {
                let hex: String = chars[i + 2..=i + 5].iter().collect();
                if let Some(decoded) = unicode_escape(&hex) {
                    out.push(decoded);
                    i += 6;
                    continue;
                }
            }
            // Unknown/truncated unicode escape: keep the backslash
            // (the rest re-scans as plain chars).
            out.push('\\');
            i += 1;
        } else if let Some(decoded) = escape_char(next) {
            out.push(decoded);
            i += 2;
        } else {
            // Unknown escape: keep the backslash.
            out.push('\\');
            i += 1;
        }
    }
    out
}

/// Strip the common leading-TAB prefix Ignition stores scripts with —
/// the ignition-nvim semantics verbatim: the minimum leading-tab
/// count over non-empty lines decides the prefix; stray spaces in
/// the leading whitespace strip alongside the tabs. Returns
/// `(dedented_text, indent_prefix)` so [`reindent`] can restore it.
pub fn dedent(text: &str) -> (String, String) {
    if text.is_empty() {
        return (String::new(), String::new());
    }
    let lines: Vec<&str> = text.split('\n').collect();
    // The minimum leading-TAB count across non-empty lines (tabs at
    // any position in the leading whitespace count — mixed stray
    // spaces are the ignition-nvim tolerance).
    let mut min_tabs: Option<usize> = None;
    for line in &lines {
        if line.trim().is_empty() {
            continue;
        }
        let leading = line.len() - line.trim_start().len();
        let tab_count = line[..leading].matches('\t').count();
        min_tabs = Some(match min_tabs {
            None => tab_count,
            Some(min) => min.min(tab_count),
        });
    }
    let min_tabs = match min_tabs {
        None | Some(0) => return (text.to_string(), String::new()),
        Some(tabs) => tabs,
    };
    let stripped: Vec<String> = lines
        .iter()
        .map(|line| {
            if line.trim().is_empty() {
                return String::new();
            }
            let mut rest: &str = line;
            let mut tabs_removed = 0usize;
            while tabs_removed < min_tabs && !rest.is_empty() {
                if let Some(stripped) = rest.strip_prefix('\t') {
                    rest = stripped;
                    tabs_removed += 1;
                } else if let Some(stripped) = rest.strip_prefix(' ') {
                    rest = stripped; // stray spaces remove alongside
                } else {
                    break;
                }
            }
            rest.to_string()
        })
        .collect();
    (stripped.join("\n"), "\t".repeat(min_tabs))
}

/// Restore the prefix [`dedent`] stripped — ONLY non-empty lines
/// reindent (the ignition-nvim semantics verbatim).
pub fn reindent(text: &str, prefix: &str) -> String {
    if prefix.is_empty() {
        return text.to_string();
    }
    text.split('\n')
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                format!("{prefix}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The decode heuristic (07-RESEARCH Pitfall 5): a SCRIPT_KEYS value
/// decodes only when it LOOKS like a script — the raw text carries a
/// script-ish escape marker AND the decoded text is multi-line.
/// Single-line Ignition expressions (even ones carrying `\u003c`-class
/// escapes) pass through untouched.
fn looks_like_script(raw_inner: &str) -> bool {
    let has_marker = raw_inner.contains("\\n")
        || raw_inner.contains("\\t")
        || raw_inner.contains("\\\"")
        || raw_inner.contains("\\u00");
    has_marker && flint_decode(raw_inner).contains('\n')
}

// ---- The shared position-tracking scanner ----------------------------------

/// A byte span of one JSON string value's INNER content (the quotes
/// excluded) inside the raw member bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Span {
    start: usize,
    end: usize,
}

/// One script-candidate the walk found: the JSON-pointer address of
/// the string VALUE (ending in its script-key token) and the
/// inner-content span.
#[derive(Debug, Clone)]
struct Found {
    pointer: String,
    span: Span,
}

/// JSON-pointer token escaping: `~` → `~0`, `/` → `~1`.
fn escape_pointer_token(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

/// Standard JSON string unescape for OBJECT KEYS (pointer building
/// needs the decoded key text; keys are ASCII in these members but
/// correctness beats assumption).
fn json_unescape(raw: &str) -> Result<String, CoreError> {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('/') => out.push('/'),
            Some('b') => out.push('\u{8}'),
            Some('f') => out.push('\u{c}'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                let code = u32::from_str_radix(&hex, 16).map_err(|_| {
                    CoreError::Internal(format!("bad \\u{hex} escape in a JSON key"))
                })?;
                // Lone surrogates are not representable in Rust
                // strings — replace (keys, display-only context).
                out.push(char::from_u32(code).unwrap_or(char::REPLACEMENT_CHARACTER));
            }
            other => {
                return Err(CoreError::Internal(format!(
                    "bad escape in a JSON key: {other:?}"
                )));
            }
        }
    }
    Ok(out)
}

/// THE shared scanner: a recursive-descent walk over the member's
/// RAW bytes recording the span of every string value whose key is in
/// [`SCRIPT_KEYS`], at any nesting depth (scripts nest arbitrarily
/// deep in view JSON — 07-RESEARCH "Don't Hand-Roll"). No
/// `serde_json::Value` materialization anywhere on this path: spans
/// address the raw bytes the splice writes back into.
struct Scanner<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Scanner<'a> {
    fn err(&self, why: &str) -> CoreError {
        CoreError::Internal(format!(
            "member JSON scan failed at byte {}: {why}",
            self.pos
        ))
    }

    fn skip_ws(&mut self) {
        while matches!(self.bytes.get(self.pos), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    /// Scan a JSON string (self.pos at the opening quote); returns
    /// the inner-content span, pos lands just past the closing quote.
    fn string(&mut self) -> Result<Span, CoreError> {
        self.pos += 1; // opening quote (caller checked)
        let start = self.pos;
        loop {
            match self.bytes.get(self.pos) {
                None => return Err(self.err("unterminated string")),
                Some(b'"') => {
                    let end = self.pos;
                    self.pos += 1;
                    return Ok(Span { start, end });
                }
                // An escape pair skips verbatim (the inner content is
                // opaque to the walk — the codec owns its meaning).
                Some(b'\\') => self.pos += 2,
                Some(_) => self.pos += 1,
            }
        }
    }

    /// Scan one literal token (true/false/null).
    fn literal(&mut self, token: &str) -> Result<(), CoreError> {
        if self.bytes[self.pos..].starts_with(token.as_bytes()) {
            self.pos += token.len();
            Ok(())
        } else {
            Err(self.err("unexpected token"))
        }
    }

    /// Scan a number token.
    fn number(&mut self) -> Result<(), CoreError> {
        while matches!(
            self.bytes.get(self.pos),
            Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
        ) {
            self.pos += 1;
        }
        Ok(())
    }

    /// Walk one value under `pointer`; a string whose key was a
    /// SCRIPT_KEY records its span into `found`.
    fn value(
        &mut self,
        pointer: &str,
        script_key: Option<&'static str>,
        found: &mut Vec<Found>,
    ) -> Result<(), CoreError> {
        self.skip_ws();
        match self.bytes.get(self.pos) {
            Some(b'{') => self.object(pointer, found),
            Some(b'[') => self.array(pointer, found),
            Some(b'"') => {
                let span = self.string()?;
                if script_key.is_some()
                    && std::str::from_utf8(&self.bytes[span.start..span.end]).is_ok()
                {
                    found.push(Found {
                        pointer: pointer.to_string(),
                        span,
                    });
                }
                Ok(())
            }
            Some(b't') => self.literal("true"),
            Some(b'f') => self.literal("false"),
            Some(b'n') => self.literal("null"),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(self.err("unexpected byte for a value")),
        }
    }

    fn object(&mut self, pointer: &str, found: &mut Vec<Found>) -> Result<(), CoreError> {
        self.pos += 1; // '{'
        self.skip_ws();
        if self.bytes.get(self.pos) == Some(&b'}') {
            self.pos += 1;
            return Ok(());
        }
        loop {
            self.skip_ws();
            if self.bytes.get(self.pos) != Some(&b'"') {
                return Err(self.err("expected an object key string"));
            }
            let key_span = self.string()?;
            let key = json_unescape(
                std::str::from_utf8(&self.bytes[key_span.start..key_span.end])
                    .map_err(|_| self.err("object key is not UTF-8"))?,
            )?;
            self.skip_ws();
            if self.bytes.get(self.pos) != Some(&b':') {
                return Err(self.err("expected ':' after an object key"));
            }
            self.pos += 1;
            let child_pointer = format!("{pointer}/{}", escape_pointer_token(&key));
            let script_key = SCRIPT_KEYS
                .iter()
                .copied()
                .find(|candidate| *candidate == key);
            self.value(&child_pointer, script_key, found)?;
            self.skip_ws();
            match self.bytes.get(self.pos) {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(());
                }
                _ => return Err(self.err("expected ',' or '}' in an object")),
            }
        }
    }

    fn array(&mut self, pointer: &str, found: &mut Vec<Found>) -> Result<(), CoreError> {
        self.pos += 1; // '['
        self.skip_ws();
        if self.bytes.get(self.pos) == Some(&b']') {
            self.pos += 1;
            return Ok(());
        }
        let mut index = 0usize;
        loop {
            let child_pointer = format!("{pointer}/{index}");
            self.value(&child_pointer, None, found)?;
            self.skip_ws();
            match self.bytes.get(self.pos) {
                Some(b',') => {
                    self.pos += 1;
                    index += 1;
                }
                Some(b']') => {
                    self.pos += 1;
                    return Ok(());
                }
                _ => return Err(self.err("expected ',' or ']' in an array")),
            }
        }
    }
}

/// Walk the raw member bytes for SCRIPT_KEYS string values — the
/// shared entry both decode and encode ride (span resolution is ONE
/// mechanism, never two).
fn scan_script_strings(json: &[u8]) -> Result<Vec<Found>, CoreError> {
    let mut scanner = Scanner {
        bytes: json,
        pos: 0,
    };
    let mut found = Vec::new();
    scanner.value("", None, &mut found)?;
    scanner.skip_ws();
    if scanner.pos != json.len() {
        return Err(scanner.err("trailing bytes after the JSON document"));
    }
    Ok(found)
}

// ---- Member-level decode/encode ---------------------------------------------

/// One manifest entry: the JSON-pointer address of a script string
/// value, its counter-named sidecar sibling (`<member>.<n>.py`), and
/// the indent prefix [`dedent`] stripped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEntry {
    /// JSON pointer to the string value (e.g.
    /// `/children/0/eventScripts/actionPerformed/script`).
    pub pointer: String,
    /// The sidecar file's basename, a sibling of the member file
    /// (e.g. `view.json.1.py`).
    pub sidecar: String,
    /// The common leading-tab prefix to restore on encode.
    pub indent_prefix: String,
}

/// The decoded-export manifest carried at the tree root — consumed +
/// stripped by [`encode_export_tree`] (it never rides an upload).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// Manifest format version (1).
    pub version: u32,
    /// Member path → its entries (pointer/sidecar/indent triples).
    pub members: BTreeMap<String, Vec<ManifestEntry>>,
}

/// One decoded member: the manifest entries plus each sidecar's
/// text (decoded + dedented) ready to write.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedMember {
    /// The zip member path the entries belong to.
    pub member_path: String,
    /// The entries, document order.
    pub entries: Vec<DecodedEntry>,
}

/// One entry with its sidecar content.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedEntry {
    /// The manifest record (pointer/sidecar/indent).
    pub entry: ManifestEntry,
    /// The sidecar text: decoded + dedented.
    pub text: String,
}

/// Decode one member's embedded scripts: walk the RAW bytes for
/// SCRIPT_KEYS string values that [`looks_like_script`] accepts,
/// producing sidecar texts (decoded + dedented) and manifest entries
/// with counter-named sidecars (`<member-basename>.<n>.py`). `None`
/// when the member holds no decodable scripts (or does not scan —
/// the caller leaves such members untouched, byte-verbatim).
pub fn decode_member(member_json: &[u8], member_path: &str) -> Option<DecodedMember> {
    let found = scan_script_strings(member_json).ok()?;
    let basename = member_path.rsplit('/').next().unwrap_or(member_path);
    let mut entries = Vec::new();
    for f in found {
        let Ok(raw_inner) = std::str::from_utf8(&member_json[f.span.start..f.span.end]) else {
            continue; // non-UTF-8 script string — leave verbatim
        };
        if !looks_like_script(raw_inner) {
            continue; // expression-shaped / single-line — pass through
        }
        let (text, indent_prefix) = dedent(&flint_decode(raw_inner));
        entries.push(DecodedEntry {
            entry: ManifestEntry {
                pointer: f.pointer,
                sidecar: format!("{basename}.{}.py", entries.len() + 1),
                indent_prefix,
            },
            text,
        });
    }
    if entries.is_empty() {
        return None;
    }
    Some(DecodedMember {
        member_path: member_path.to_string(),
        entries,
    })
}

/// Encode one member's scripts back: re-resolve each manifest
/// pointer to its raw byte span in the CURRENT member bytes (the
/// same shared scanner — re-scanning at encode time keeps splices
/// valid even when the user hand-edited the member JSON), then
/// splice the re-encoded replacement (reindent + [`flint_encode`])
/// at that span. Rules:
///
/// - a manifest entry whose sidecar is ABSENT from `sidecar_texts`
///   keeps the JSON's current value (never silently drop edits);
/// - a pointer that no longer resolves keeps the current value;
/// - unedited members re-encode BYTE-IDENTICAL (the sacred
///   invariant at file level — untouched spans copy verbatim).
pub fn encode_member(
    member_json: &[u8],
    entries: &[ManifestEntry],
    sidecar_texts: &HashMap<String, String>,
) -> Result<Vec<u8>, CoreError> {
    let found = scan_script_strings(member_json).map_err(|err| CoreError::InvalidInput {
        reason: format!(
            "the edited member no longer parses as JSON — cannot splice its \
             scripts back ({err})"
        ),
    })?;
    // Resolve spans for the entries that have sidecar text.
    let mut splices: Vec<(Span, Vec<u8>)> = Vec::new();
    for entry in entries {
        let Some(text) = sidecar_texts.get(&entry.sidecar) else {
            continue; // missing sidecar — keep the current value
        };
        let Some(f) = found.iter().find(|f| f.pointer == entry.pointer) else {
            continue; // value gone from the member — keep the current bytes
        };
        let encoded = flint_encode(&reindent(text, &entry.indent_prefix));
        splices.push((f.span, encoded.into_bytes()));
    }
    splices.sort_by_key(|(span, _)| span.start);
    let mut out = Vec::with_capacity(member_json.len());
    let mut cursor = 0usize;
    for (span, replacement) in &splices {
        if span.start < cursor {
            return Err(CoreError::Internal(
                "overlapping script spans — refusing to splice".to_string(),
            ));
        }
        out.extend_from_slice(&member_json[cursor..span.start]);
        out.extend_from_slice(replacement);
        cursor = span.end;
    }
    out.extend_from_slice(&member_json[cursor..]);
    Ok(out)
}

// ---- Tree-level wrappers -----------------------------------------------------

/// Count a zip's FILE members (directory entries excluded) — the
/// export-decode result's member count.
pub fn count_file_members(zip_bytes: &[u8]) -> Result<usize, CoreError> {
    let mut archive = open_archive(zip_bytes)?;
    let mut count = 0usize;
    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|err| CoreError::Internal(format!("cannot walk export zip: {err}")))?;
        if !file.is_dir() {
            count += 1;
        }
    }
    Ok(count)
}

/// Open an export zip for reading — the resources.rs classification
/// (a non-zip export is a gateway-contract violation, exit 1).
fn open_archive(zip_bytes: &[u8]) -> Result<zip::ZipArchive<std::io::Cursor<&[u8]>>, CoreError> {
    zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))
        .map_err(|err| CoreError::Internal(format!("project export is not a readable zip: {err}")))
}

/// The deterministic options every re-encoded member rides (the
/// resources.rs rewrite convention).
fn rewrite_options() -> zip::write::SimpleFileOptions {
    zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated)
}

/// Decode an export zip into a DIRECTORY: every member written at
/// its path, `<member>.<n>.py` sidecars beside the `.json` members
/// that carry embedded scripts, and [`MANIFEST_NAME`] at the tree
/// root. Returns the sidecar count. The exported JSON stays
/// MARKER-FREE (gateway-clean).
pub fn decode_export_tree(zip_bytes: &[u8], out_dir: &Path) -> Result<usize, CoreError> {
    let mut archive = open_archive(zip_bytes)?;
    let names: BTreeSet<String> = archive.file_names().map(str::to_string).collect();
    if names.contains(MANIFEST_NAME) {
        return Err(CoreError::Internal(format!(
            "the export already carries a {MANIFEST_NAME} member — refusing to \
             shadow it with the decode manifest"
        )));
    }
    std::fs::create_dir_all(out_dir).map_err(|err| {
        CoreError::Internal(format!(
            "cannot create decode directory {}: {err}",
            out_dir.display()
        ))
    })?;
    let mut manifest = Manifest {
        version: 1,
        members: BTreeMap::new(),
    };
    let mut scripts = 0usize;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|err| CoreError::Internal(format!("cannot walk export zip: {err}")))?;
        let name = file.name().to_string();
        if file.is_dir() {
            std::fs::create_dir_all(out_dir.join(&name))
                .map_err(|err| CoreError::Internal(format!("cannot create {}: {err}", name)))?;
            continue;
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(|err| {
            CoreError::Internal(format!("cannot decompress zip member {name:?}: {err}"))
        })?;
        let dest = out_dir.join(&name);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                CoreError::Internal(format!("cannot create {}: {err}", parent.display()))
            })?;
        }
        std::fs::write(&dest, &bytes).map_err(|err| {
            CoreError::Internal(format!("cannot write {}: {err}", dest.display()))
        })?;
        // The decode pass: `.json` members only, sidecars as
        // siblings (counter-named), entries recorded in the manifest.
        if name.ends_with(".json")
            && let Some(decoded) = decode_member(&bytes, &name)
        {
            for e in &decoded.entries {
                let sidecar_member_path = match name.rsplit_once('/') {
                    Some((parent, _)) => format!("{parent}/{}", e.entry.sidecar),
                    None => e.entry.sidecar.clone(),
                };
                if names.contains(&sidecar_member_path) {
                    return Err(CoreError::Internal(format!(
                        "sidecar {sidecar_member_path:?} collides with a real export \
                         member — refusing to shadow it"
                    )));
                }
                let sidecar_path = dest.with_file_name(&e.entry.sidecar);
                std::fs::write(&sidecar_path, &e.text).map_err(|err| {
                    CoreError::Internal(format!("cannot write {}: {err}", sidecar_path.display()))
                })?;
                scripts += 1;
            }
            manifest.members.insert(
                name.clone(),
                decoded.entries.iter().map(|e| e.entry.clone()).collect(),
            );
        }
    }
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|err| {
        CoreError::Internal(format!("cannot serialize the decode manifest: {err}"))
    })?;
    let manifest_path = out_dir.join(MANIFEST_NAME);
    std::fs::write(
        &manifest_path,
        format!("{}\n", String::from_utf8_lossy(&manifest_bytes)),
    )
    .map_err(|err| {
        CoreError::Internal(format!("cannot write {}: {err}", manifest_path.display()))
    })?;
    Ok(scripts)
}

/// Re-zip a decoded export DIRECTORY back into importable zip bytes:
/// the manifest is consumed + stripped, every sidecar referenced by
/// it is stripped, members with manifest entries ride
/// [`encode_member`] (span-level splice), everything else copies
/// verbatim. Missing sidecars keep the member's current value (the
/// decode rule). A directory without [`MANIFEST_NAME`] is not a
/// decoded export tree (usage-class refusal).
pub fn encode_export_tree(dir: &Path) -> Result<Vec<u8>, CoreError> {
    let manifest_path = dir.join(MANIFEST_NAME);
    let manifest: Manifest = std::fs::read(&manifest_path)
        .map_err(|err| CoreError::InvalidInput {
            reason: format!(
                "{} is not a decoded export directory (cannot read {MANIFEST_NAME}: {err})",
                dir.display()
            ),
        })
        .and_then(|bytes| {
            serde_json::from_slice(&bytes).map_err(|err| CoreError::InvalidInput {
                reason: format!("{MANIFEST_NAME} is not valid JSON: {err}"),
            })
        })?;
    // The sidecar set to strip: (member's parent dir, sidecar name).
    let mut sidecar_set: BTreeSet<(String, String)> = BTreeSet::new();
    for (member, entries) in &manifest.members {
        let parent = member
            .rsplit_once('/')
            .map_or(String::new(), |(p, _)| p.to_string());
        for entry in entries {
            sidecar_set.insert((parent.clone(), entry.sidecar.clone()));
        }
    }
    // Deterministic walk: entries sorted by name per directory.
    let mut files: Vec<PathBuf> = Vec::new();
    walk_files(dir, Path::new(""), &mut files)?;
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = rewrite_options();
    for rel in files {
        let rel_str = rel.to_string_lossy().into_owned();
        if rel_str == MANIFEST_NAME {
            continue; // consumed + stripped
        }
        let (parent, basename) = rel_str
            .rsplit_once('/')
            .map_or(("", rel_str.as_str()), |(p, b)| (p, b));
        if sidecar_set.contains(&(parent.to_string(), basename.to_string())) {
            continue; // stripped — their text rides via the splice
        }
        let bytes = std::fs::read(dir.join(&rel)).map_err(|err| CoreError::InvalidInput {
            reason: format!("cannot read {}: {err}", rel.display()),
        })?;
        let content = if let Some(entries) = manifest.members.get(&rel_str) {
            let mut texts: HashMap<String, String> = HashMap::new();
            let member_abs = dir.join(&rel);
            for entry in entries {
                if let Ok(text) = std::fs::read_to_string(member_abs.with_file_name(&entry.sidecar))
                {
                    texts.insert(entry.sidecar.clone(), text);
                }
            }
            encode_member(&bytes, entries, &texts)?
        } else {
            bytes
        };
        writer
            .start_file(rel_str.clone(), options)
            .map_err(|err| CoreError::Internal(format!("cannot re-zip {rel_str:?}: {err}")))?;
        writer
            .write_all(&content)
            .map_err(|err| CoreError::Internal(format!("cannot re-zip {rel_str:?}: {err}")))?;
    }
    writer
        .finish()
        .map_err(|err| CoreError::Internal(format!("cannot finalize re-encoded zip: {err}")))
        .map(|cursor| cursor.into_inner())
}

/// Recursive, name-sorted file listing under `dir`/`prefix`.
fn walk_files(dir: &Path, prefix: &Path, out: &mut Vec<PathBuf>) -> Result<(), CoreError> {
    let mut entries: Vec<_> = std::fs::read_dir(dir.join(prefix))
        .map_err(|err| CoreError::InvalidInput {
            reason: format!("cannot walk {}: {err}", dir.join(prefix).display()),
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| CoreError::InvalidInput {
            reason: format!("cannot walk {}: {err}", dir.join(prefix).display()),
        })?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let rel = prefix.join(entry.file_name());
        if entry.path().is_dir() {
            walk_files(dir, &rel, out)?;
        } else {
            out.push(rel);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- The sacred codec invariant (ported ignition-nvim vectors) ----

    /// THE invariant corpus: every string Ignition's writer can
    /// produce round-trips `flint_encode(flint_decode(x)) == x` —
    /// all 12 escapes, the `\\t` ambiguity, quotes, HTML five,
    /// unicode text, and real script shapes.
    #[test]
    fn encode_decode_round_trip_is_sacred() {
        let corpus = [
            // all twelve table entries
            "back\\\\slash",
            "quote \" inside",
            "tab\there",
            "backspace\u{8}",
            "new\nline",
            "carriage\rreturn",
            "form\u{c}feed",
            "less < than",
            "greater > than",
            "amp & ersand",
            "equals = sign",
            "apostrophe ' here",
            // the ambiguity: literal backslash + t vs the tab escape
            "\\\\t is not a tab",
            "\ttab is a tab",
            // mixed real-world script shapes
            "if x < 3 && y > 2:\n\tprint 'it\\'s <>&='\n\treturn {value}",
            "print(\"quoted \\\"inside\\\"\")",
            // multi-line with all markers
            "\tfor i in range(10):\n\t\tif i & 1 == 0:\n\t\t\tprint i, '<', '=', '>'",
            // unicode rides verbatim (the table never escapes it)
            "café ☕ naïve",
            "",
            "no escapes at all",
        ];
        for text in corpus {
            let encoded = flint_encode(text);
            assert_eq!(flint_decode(&encoded), text, "decode(encode({text:?}))");
            // the sacred direction the plan pins:
            assert_eq!(
                flint_encode(&flint_decode(&encoded)),
                encoded,
                "encode(decode(x)) == x for {encoded:?}"
            );
        }
    }

    /// `\\t` (literal backslash + t) decodes differently from `\t`
    /// (tab) — the multi-pass impossibility, single-pass proof.
    #[test]
    fn decode_distinguishes_escaped_backslash_t_from_tab() {
        assert_eq!(flint_decode(r"\\t"), r"\t");
        assert_eq!(flint_decode(r"\t"), "\t");
        // backslash, backslash, tab-escape → backslash + tab.
        assert_eq!(flint_decode(r"\\\t"), "\\\t");
    }

    /// Unknown `\uXXXX` escapes KEEP the backslash (and the escape
    /// rides verbatim through re-encode-as-text); the HTML five map.
    #[test]
    fn decode_maps_the_html_five_and_keeps_unknown_unicode_escapes() {
        assert_eq!(flint_decode(r"\u003c"), "<");
        assert_eq!(flint_decode(r"\u003e"), ">");
        assert_eq!(flint_decode(r"\u0026"), "&");
        assert_eq!(flint_decode(r"\u003d"), "=");
        assert_eq!(flint_decode(r"\u0027"), "'");
        // Unknown — backslash kept, sequence verbatim:
        assert_eq!(flint_decode(r"\u0041"), r"\u0041");
        assert_eq!(flint_decode(r"\u00zz"), r"\u00zz");
        // Truncated — backslash kept:
        assert_eq!(flint_decode(r"\u00"), r"\u00");
        // Unknown single escapes keep the backslash too:
        assert_eq!(flint_decode(r"\/"), r"\/");
    }

    /// dedent/reindent: the common leading-TAB prefix strips and
    /// restores (only non-empty lines reindent; no-indent text is a
    /// no-op with an empty prefix).
    #[test]
    fn dedent_reindent_inverse_on_tab_indented_scripts() {
        let script = "\t\tfor i in range(3):\n\t\t\tprint i\n\t\tprint 'end'";
        let (dedented, prefix) = dedent(script);
        assert_eq!(dedented, "for i in range(3):\n\tprint i\nprint 'end'");
        assert_eq!(prefix, "\t\t");
        assert_eq!(reindent(&dedented, &prefix), script);

        // No common indent: unchanged, empty prefix.
        let flat = "print('x')\nprint('y')";
        let (same, empty) = dedent(flat);
        assert_eq!((same.as_str(), empty.as_str()), (flat, ""));
        assert_eq!(reindent(&same, &empty), flat);

        // Empty-string edge.
        assert_eq!(dedent(""), (String::new(), String::new()));

        // Trailing newline: the empty last line stays empty.
        let trailing = "\tdo()\n";
        let (dedented, prefix) = dedent(trailing);
        assert_eq!((dedented.as_str(), prefix.as_str()), ("do()\n", "\t"));
        assert_eq!(reindent(&dedented, &prefix), trailing);
    }

    /// SCRIPT_KEYS is the ignition-nvim list, all ten, in order.
    #[test]
    fn script_keys_match_ignition_nvim() {
        assert_eq!(
            SCRIPT_KEYS,
            [
                "script",
                "code",
                "eventScript",
                "transform",
                "onActionPerformed",
                "onChange",
                "onStartup",
                "onShutdown",
                "expression",
            ]
        );
    }

    // ---- decode_member / encode_member ---------------------------------

    /// A live-shaped view member: two embedded scripts at different
    /// depths, an expression value under a SCRIPT_KEY that must PASS
    /// THROUGH, and plain text fields.
    const VIEW_JSON: &str = r#"{
  "scope": "G",
  "children": [
    {
      "type": "ia.display.label",
      "meta": {
        "name": "lbl"
      },
      "props": {
        "text": "plain <>&=' text"
      },
      "eventScripts": {
        "actionPerformed": {
          "config": {
            "script": "\tprint \u0027clicked\u0027\n\tprint \u0027done \u003c\u003e\u0026\u003d\u0027"
          }
        }
      }
    },
    {
      "type": "ia.chart",
      "transform": {
        "script": "\t\tfor i in range(3):\n\t\t\tprint i\n\t\tprint \u0027end\u0027"
      },
      "props": {
        "expression": "toStr({view.args.x} * 2)"
      }
    }
  ]
}"#;

    #[test]
    fn decode_member_finds_nested_scripts_and_passes_expressions_through() {
        let decoded = decode_member(VIEW_JSON.as_bytes(), "c/views/Dashboard/view.json")
            .expect("two scripts decode");
        assert_eq!(decoded.entries.len(), 2, "the expression does not decode");
        assert_eq!(
            decoded.entries[0].entry.pointer,
            "/children/0/eventScripts/actionPerformed/config/script"
        );
        assert_eq!(decoded.entries[0].entry.sidecar, "view.json.1.py");
        assert_eq!(decoded.entries[0].entry.indent_prefix, "\t");
        assert_eq!(
            decoded.entries[0].text,
            "print 'clicked'\nprint 'done <>&='"
        );
        assert_eq!(
            decoded.entries[1].entry.pointer,
            "/children/1/transform/script"
        );
        assert_eq!(decoded.entries[1].entry.sidecar, "view.json.2.py");
        assert_eq!(decoded.entries[1].entry.indent_prefix, "\t\t");

        // A member without scripts decodes to None.
        assert!(decode_member(br#"{"title":"T"}"#, "project.json").is_none());
        // A member that does not scan decodes to None (rides verbatim).
        assert!(decode_member(b"<<<not json>>>", "broken.json").is_none());
        // A script-python member (plain text, not JSON) is None.
        assert!(decode_member(b"print('plain')\n", "ignition/x/scratch").is_none());
    }

    /// THE file-level sacred invariant, member edition: unedited
    /// sidecars re-encode BYTE-IDENTICAL; editing one sidecar changes
    /// only that span; a missing sidecar keeps the current value.
    #[test]
    fn encode_member_round_trips_bytes_and_splices_edits() {
        let decoded =
            decode_member(VIEW_JSON.as_bytes(), "c/views/Dashboard/view.json").expect("decodes");
        let entries: Vec<ManifestEntry> = decoded.entries.iter().map(|e| e.entry.clone()).collect();
        let texts: HashMap<String, String> = decoded
            .entries
            .iter()
            .map(|e| (e.entry.sidecar.clone(), e.text.clone()))
            .collect();

        // Unedited: byte-identical.
        let out = encode_member(VIEW_JSON.as_bytes(), &entries, &texts).expect("encodes");
        assert_eq!(out, VIEW_JSON.as_bytes(), "unedited round-trip is exact");

        // Edit sidecar 1 only: the re-encoded member differs, parses,
        // and carries the new script text at the SAME pointer.
        let mut edited = texts.clone();
        edited.insert(
            "view.json.1.py".to_string(),
            "print 'edited'\nprint 'twice'".to_string(),
        );
        let out = encode_member(VIEW_JSON.as_bytes(), &entries, &edited).expect("encodes");
        let parsed: serde_json::Value = serde_json::from_slice(&out).expect("still JSON");
        assert_eq!(
            parsed["children"][0]["eventScripts"]["actionPerformed"]["config"]["script"],
            "\tprint 'edited'\n\tprint 'twice'",
            "the edited text re-dents under the recorded prefix"
        );
        assert_eq!(
            parsed["children"][1]["transform"]["script"],
            "\t\tfor i in range(3):\n\t\t\tprint i\n\t\tprint 'end'",
            "the unedited sibling rides byte-equal content"
        );

        // Missing sidecar: the value is preserved (never dropped).
        let mut missing = texts.clone();
        missing.remove("view.json.2.py");
        let out = encode_member(VIEW_JSON.as_bytes(), &entries, &missing).expect("encodes");
        assert_eq!(
            out,
            VIEW_JSON.as_bytes(),
            "a missing sidecar keeps the value"
        );
    }

    /// A member whose manifest pointer no longer resolves (the user
    /// deleted the value) keeps its current bytes; a member that no
    /// longer parses refuses usage-class.
    #[test]
    fn encode_member_handles_unresolvable_pointers_and_broken_json() {
        let entries = vec![ManifestEntry {
            pointer: "/gone/script".to_string(),
            sidecar: "view.json.1.py".to_string(),
            indent_prefix: String::new(),
        }];
        let mut texts = HashMap::new();
        texts.insert("view.json.1.py".to_string(), "x".to_string());
        let out = encode_member(VIEW_JSON.as_bytes(), &entries, &texts).expect("encodes");
        assert_eq!(
            out,
            VIEW_JSON.as_bytes(),
            "an unresolvable pointer is a no-op"
        );

        let err = encode_member(b"<<<broken>>>", &entries, &texts).expect_err("must refuse");
        assert!(matches!(err, CoreError::InvalidInput { .. }), "{err}");
        assert_eq!(err.exit_code(), 2);
    }

    // ---- Tree wrappers ---------------------------------------------------

    /// Build an in-test export zip (the resources.rs fixture style).
    fn fixture_zip(members: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in members {
            writer.start_file(*name, options).expect("member starts");
            writer.write_all(bytes).expect("member writes");
        }
        writer.finish().expect("zip finalizes").into_inner()
    }

    /// The full tree round-trip: decode → encode with no edits →
    /// every file member byte-identical (the contract's core, here
    /// at unit weight; the dedicated contract file carries the full
    /// fixture matrix).
    #[test]
    fn decode_encode_tree_round_trips_unedited_members() {
        let zip = fixture_zip(&[
            ("project.json", br#"{"title":"T"}"#.as_slice()),
            (
                "c/resources/views/Dashboard/view.json",
                VIEW_JSON.as_bytes(),
            ),
            (
                "c/resources/views/Dashboard/resource.json",
                br#"{"scope":"G","version":1,"files":["view.json"]}"#.as_slice(),
            ),
            ("ignition/resources/scratch", b"print('plain')".as_slice()),
        ]);
        let dir = tempfile::tempdir().expect("tempdir");
        let scripts = decode_export_tree(&zip, dir.path()).expect("decodes");
        assert_eq!(scripts, 2);
        assert!(dir.path().join(MANIFEST_NAME).is_file());
        assert!(
            dir.path()
                .join("c/resources/views/Dashboard/view.json.1.py")
                .is_file()
        );

        let re_zipped = encode_export_tree(dir.path()).expect("re-encodes");
        let mut original = open_archive(&zip).expect("reopen");
        let mut re = open_archive(&re_zipped).expect("open re-zip");
        assert_eq!(re.len(), original.len(), "same member count");
        for index in 0..original.len() {
            let name = original.by_index(index).expect("orig").name().to_string();
            let mut orig_bytes = Vec::new();
            original
                .by_index(index)
                .expect("orig")
                .read_to_end(&mut orig_bytes)
                .expect("read");
            let mut re_file = re.by_name(&name).expect("member present");
            let mut re_bytes = Vec::new();
            re_file.read_to_end(&mut re_bytes).expect("read");
            assert_eq!(re_bytes, orig_bytes, "{name} byte-identical unedited");
        }
        // The manifest never rides the re-zip.
        assert!(re.by_name(MANIFEST_NAME).is_err());
        assert_eq!(count_file_members(&re_zipped).expect("counts"), 4);
    }

    /// A tree edit flows through: editing one sidecar changes only
    /// that member; the others stay byte-identical.
    #[test]
    fn tree_edit_splices_only_the_edited_member() {
        let zip = fixture_zip(&[
            ("project.json", br#"{"title":"T"}"#.as_slice()),
            (
                "c/resources/views/Dashboard/view.json",
                VIEW_JSON.as_bytes(),
            ),
            ("ignition/resources/scratch", b"print('plain')".as_slice()),
        ]);
        let dir = tempfile::tempdir().expect("tempdir");
        decode_export_tree(&zip, dir.path()).expect("decodes");
        let sidecar = dir
            .path()
            .join("c/resources/views/Dashboard/view.json.1.py");
        std::fs::write(&sidecar, "print 'edited alone'").expect("edit sidecar");
        let re_zipped = encode_export_tree(dir.path()).expect("re-encodes");

        let mut re = open_archive(&re_zipped).expect("open");
        let mut view = Vec::new();
        re.by_name("c/resources/views/Dashboard/view.json")
            .expect("view")
            .read_to_end(&mut view)
            .expect("read");
        let parsed: serde_json::Value = serde_json::from_slice(&view).expect("json");
        assert_eq!(
            parsed["children"][0]["eventScripts"]["actionPerformed"]["config"]["script"],
            "\tprint 'edited alone'"
        );
        let mut scratch = Vec::new();
        re.by_name("ignition/resources/scratch")
            .expect("scratch")
            .read_to_end(&mut scratch)
            .expect("read");
        assert_eq!(scratch, b"print('plain')");
    }

    /// encode_export_tree refuses a non-decoded directory
    /// usage-class; decode refuses an export already carrying a
    /// manifest member.
    #[test]
    fn tree_wrapper_error_shapes() {
        let plain = tempfile::tempdir().expect("tempdir");
        let err = encode_export_tree(plain.path()).expect_err("no manifest refuses");
        assert!(matches!(err, CoreError::InvalidInput { .. }), "{err}");
        assert_eq!(err.exit_code(), 2);
        assert!(err.to_string().contains("not a decoded export directory"));

        let zip = fixture_zip(&[(MANIFEST_NAME, b"{}".as_slice())]);
        let dir = tempfile::tempdir().expect("tempdir");
        let err = decode_export_tree(&zip, dir.path()).expect_err("shadow refuses");
        assert!(matches!(err, CoreError::Internal(_)), "{err}");
    }
}
