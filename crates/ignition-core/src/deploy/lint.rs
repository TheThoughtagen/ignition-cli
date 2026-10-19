//! Render stage 7: the fail-closed lints (Phase 21, plan 21-08, DRND-07 and
//! DRND-08).
//!
//! - **Secrets** ([`scan_embedded`], [`scan_secrets`]): an `Embedded` secret
//!   object anywhere in a `.json` member; a non-null, non-Referenced value at
//!   an OpenAPI secret-typed path (Referenced means the exact shape of
//!   [`is_referenced`], nothing looser) ([`secret_paths`]); and, in a module type
//!   the 8.3.6 OpenAPI does not describe, a non-empty string under a key whose
//!   name contains `password` or `secret`.
//! - **Windows safety** (`lint_tree`): illegal names, case-fold collisions
//!   and the path budget (D-04).
//! - **Line endings**: a CR byte in a text member (D-01).
//!
//! Every output names a location, never a value. [`SecretScan`] has no field
//! that can hold a JSON string, so a secret cannot reach an error, a warning,
//! the `--json` envelope or a log through this module (T-21-33).

use std::collections::{BTreeMap, BTreeSet};

use super::layout::{RelPath, ResourceKey};
use super::node::Node;
use super::rules::{openapi_describes, secret_paths};
use super::target::{Target, TargetOs};
use super::tree::RenderedTree;
use crate::error::DeployFinding;

/// What a secret scan of one parsed file found. Locations only.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SecretScan {
    /// `<origin>: <json path> (<kind>)` for every refused location, in
    /// document order.
    pub refused: Vec<String>,
    /// Warning texts naming a path, in document order.
    pub warnings: Vec<String>,
}

/// The root of a document, as a JSON path.
const ROOT_PATH: &str = "$";

/// Whether `node` is an `Embedded` secret object: `"type": "Embedded"` with an
/// object `data`.
fn is_embedded(node: &Node) -> bool {
    matches!(node.get("type"), Some(Node::Str(t)) if t == "Embedded")
        && matches!(node.get("data"), Some(Node::Obj(_)))
}

/// Whether `node` is a well-formed `Referenced` secret object (CD7): exactly
/// the members `type` (the string `Referenced`) and `data`, where `data` is
/// exactly the string members `providerName` and `secretName`. Member order
/// does not matter; the parser refuses duplicate keys, so the length checks
/// plus named lookups are an exact shape test. Any other shape (an extra,
/// missing or wrong-case member, a non-string name, a sibling beside
/// `type`/`data`) is not Referenced and is scanned as ordinary content.
fn is_referenced(node: &Node) -> bool {
    let Node::Obj(members) = node else {
        return false;
    };
    if members.len() != 2 || !matches!(node.get("type"), Some(Node::Str(t)) if t == "Referenced") {
        return false;
    }
    match node.get("data") {
        Some(data @ Node::Obj(inner)) => {
            inner.len() == 2
                && matches!(data.get("providerName"), Some(Node::Str(_)))
                && matches!(data.get("secretName"), Some(Node::Str(_)))
        }
        _ => false,
    }
}

/// Collect the `(providerName, secretName)` of every well-formed `Referenced`
/// object anywhere in `node` into `out` (D-13).
///
/// Names only: no value is read and none is returned (ENT-3115). The shape
/// is [`is_referenced`], the one the secret lint accepts, so a reference the
/// lint lets through is exactly one this collects, and a lookalike (an extra
/// member, `referenced` in lower case, a non-string name) is not collected.
/// The set is byte-ordered by provider then name and holds each pair once.
pub fn referenced_secret_names(node: &Node, out: &mut BTreeSet<(String, String)>) {
    if is_referenced(node) {
        if let Some(data) = node.get("data")
            && let (Some(Node::Str(provider)), Some(Node::Str(name))) =
                (data.get("providerName"), data.get("secretName"))
        {
            out.insert((provider.clone(), name.clone()));
        }
        return;
    }
    match node {
        Node::Obj(members) => {
            for (_, member) in members {
                referenced_secret_names(member, out);
            }
        }
        Node::Arr(items) => {
            for item in items {
                referenced_secret_names(item, out);
            }
        }
        Node::Str(_) | Node::Lit(_) => {}
    }
}

/// Whether a member named `key` looks like it holds a secret: its name
/// contains `password` or `secret` (ASCII case-insensitive).
fn key_looks_secret(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.contains("password") || lower.contains("secret")
}

/// Join a member name onto a JSON path.
fn member_path(parent: &str, key: &str) -> String {
    if parent.is_empty() {
        key.to_string()
    } else {
        format!("{parent}.{key}")
    }
}

/// What a scan applies besides the generic Embedded rule.
struct Rules<'a> {
    /// OpenAPI secret-typed paths of the resource type (config.json only).
    schema: &'a [&'static str],
    /// Apply the key-name rule (config.json of a resource type).
    key_names: bool,
    /// The 8.3.6 OpenAPI describes the type: key-name hits warn, not refuse.
    described: bool,
}

struct Walker<'a> {
    origin: &'a str,
    rules: Option<Rules<'a>>,
    scan: SecretScan,
}

impl Walker<'_> {
    fn refuse(&mut self, path: &str, kind: &str) {
        let shown = if path.is_empty() { ROOT_PATH } else { path };
        self.scan
            .refused
            .push(format!("{}: {shown} ({kind})", self.origin));
    }

    /// Visit `node` at `path`. `key_hit` is true when `node` is a string (or
    /// an array of them) held by a secret-looking key. A well-formed
    /// Referenced object is skipped whole; no flag turns the key-name rule off
    /// for any subtree.
    fn walk(&mut self, node: &Node, path: &str, key_hit: bool) {
        match node {
            Node::Obj(members) => {
                if is_embedded(node) {
                    self.refuse(path, "embedded");
                    return;
                }
                if is_referenced(node) {
                    // names only: no value, no nested object that could be Embedded
                    return;
                }
                for (key, value) in members {
                    let child = member_path(path, key);
                    let at_schema = self
                        .rules
                        .as_ref()
                        .is_some_and(|r| r.schema.contains(&child.as_str()));
                    if at_schema {
                        self.schema_value(value, &child);
                        continue;
                    }
                    let hit =
                        self.rules.as_ref().is_some_and(|r| r.key_names) && key_looks_secret(key);
                    self.walk(value, &child, hit);
                }
            }
            Node::Arr(items) => {
                for (i, item) in items.iter().enumerate() {
                    self.walk(item, &format!("{path}[{i}]"), key_hit);
                }
            }
            Node::Str(s) => {
                if key_hit && !s.is_empty() {
                    self.key_name_hit(path);
                }
            }
            Node::Lit(_) => {}
        }
    }

    /// A value at an OpenAPI secret-typed path: null and a well-formed
    /// Referenced pass, an Embedded object is refused as embedded, anything
    /// else is plaintext.
    fn schema_value(&mut self, value: &Node, path: &str) {
        match value {
            Node::Lit(l) if l == "null" => {}
            Node::Obj(_) if is_embedded(value) => self.refuse(path, "embedded"),
            Node::Obj(_) if is_referenced(value) => {}
            _ => self.refuse(path, "plaintext"),
        }
    }

    /// A non-empty string under a secret-looking key outside the schema paths.
    fn key_name_hit(&mut self, path: &str) {
        let described = self.rules.as_ref().is_some_and(|r| r.described);
        if described {
            self.scan.warnings.push(format!(
                "possible plaintext secret at {}: {path}; this field is not secret-typed in the 8.3.6 OpenAPI, check it is not a secret",
                self.origin
            ));
        } else {
            self.refuse(path, "plaintext, by key name");
        }
    }
}

/// The generic Embedded scan: every object in `node` that is an Embedded
/// secret, as `<origin>: <json path> (embedded)`. Used for every `.json`
/// member, including tag files, which have no resource type.
pub fn scan_embedded(origin: &str, node: &Node) -> SecretScan {
    let mut walker = Walker {
        origin,
        rules: None,
        scan: SecretScan::default(),
    };
    walker.walk(node, "", false);
    walker.scan
}

/// Scan one parsed resource file.
///
/// Always the generic Embedded rule. When `file_name` (the file's path
/// relative to the resource root) is `config.json`, also:
///
/// - each [`secret_paths`] entry must be absent, `null` or Referenced; an
///   Embedded object there is reported once, as embedded; a string or any
///   other value is plaintext;
/// - a non-empty string under a key containing `password` or `secret`
///   (outside the schema paths and outside a well-formed Referenced object) is
///   refused as plaintext by key name when the 8.3.6 OpenAPI does not
///   describe the type, and only warned about when it does (the schema is
///   authoritative there).
///
/// `origin` is the source file label findings should carry.
pub fn scan_secrets(origin: &str, key: &ResourceKey, file_name: &str, node: &Node) -> SecretScan {
    if file_name != "config.json" {
        return scan_embedded(origin, node);
    }
    let mut walker = Walker {
        origin,
        rules: Some(Rules {
            schema: secret_paths(&key.module, &key.ty),
            key_names: true,
            described: openapi_describes(&key.module, &key.ty),
        }),
        scan: SecretScan::default(),
    };
    walker.walk(node, "", false);
    walker.scan
}

// ---- Windows safety, path budget and line endings (21-08 task 2) ----------

/// A full Windows path must be shorter than this many UTF-16 code units
/// (DRND-08): 260 (`MAX_PATH`) less 12, the room Windows keeps for an 8.3
/// file name when it creates a directory. A total of 247 passes, 248 is
/// refused.
pub const WINDOWS_MAX_PATH: usize = 248;

/// The longest a single Windows path component may be, in UTF-16 code units.
pub const WINDOWS_MAX_COMPONENT: usize = 255;

/// The default Ignition data directory on Windows (51 characters). A Windows
/// node's `ssh.data_dir` replaces it in the budget when longer.
pub const DEFAULT_WINDOWS_DATA_DIR: &str = r"C:\Program Files\Inductive Automation\Ignition\data";

/// What sits between the data directory and the mode directory on a gateway
/// host: `\config\resources\` (18 characters).
pub const COLLECTION_SEGMENT: &str = r"\config\resources\";

/// Device names Windows reserves, whatever the extension (compared against the
/// ASCII-uppercased stem before the first `.`).
pub const RESERVED_STEMS: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Extensions (ASCII case-insensitive) that are always text members for the CR
/// lint, whatever their bytes (D-01).
pub const TEXT_EXTENSIONS: [&str; 13] = [
    "json",
    "css",
    "yaml",
    "yml",
    "txt",
    "md",
    "xml",
    "py",
    "csv",
    "properties",
    "html",
    "js",
    "svg",
];

/// UTF-16 length: the unit Windows limits are counted in.
fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// The length of the Windows path before the relative path: the longest of
/// [`DEFAULT_WINDOWS_DATA_DIR`] and every Windows node's `ssh.data_dir`
/// (trailing separators trimmed), plus [`COLLECTION_SEGMENT`], plus
/// `target.mode`, in UTF-16 code units. The default always counts, even for a
/// Linux-only target, because `resources/` is shared between environments.
pub fn windows_prefix_len(target: &Target) -> usize {
    let nodes = std::iter::once(&target.nodes.master).chain(target.nodes.backup.as_ref());
    let data_dir = nodes
        .filter_map(|n| n.ssh.as_ref())
        .filter(|ssh| ssh.os == TargetOs::Windows)
        .map(|ssh| utf16_len(ssh.data_dir.trim_end_matches(['\\', '/'])))
        .fold(utf16_len(DEFAULT_WINDOWS_DATA_DIR), usize::max);
    data_dir + utf16_len(COLLECTION_SEGMENT) + utf16_len(&target.mode)
}

/// Whether the CR lint treats a member as text: its extension is one of
/// [`TEXT_EXTENSIONS`], or its bytes are valid UTF-8 without a NUL byte. A
/// jar, whose bytes are not text, is therefore exempt (D-01).
pub fn is_text_member(path: &str, bytes: &[u8]) -> bool {
    let ext = path.rsplit_once('.').map(|(_, e)| e);
    if let Some(ext) = ext
        && !ext.contains('/')
        && TEXT_EXTENSIONS.iter().any(|t| t.eq_ignore_ascii_case(ext))
    {
        return true;
    }
    !bytes.contains(&0) && std::str::from_utf8(bytes).is_ok()
}

/// Why `name` is not a name Windows accepts for one path component. The rules
/// are an approximation of NTFS made fail-closed on purpose (D-04): real NTFS
/// behaviour is captured in Phase 22 (assumption A4), and the Linux encoding
/// of hostile names is decision CD6.
fn windows_problems(name: &str) -> Vec<String> {
    let mut problems = Vec::new();
    if let Some(c) = name
        .chars()
        .find(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') || c.is_control())
    {
        let shown = if c.is_control() {
            format!("the control character U+{:04X}", c as u32)
        } else {
            format!("`{c}`")
        };
        problems.push(format!(
            "contains {shown}, which Windows does not allow in a name"
        ));
    }
    if name.ends_with('.') || name.ends_with(' ') {
        problems.push("ends in a `.` or a space, which Windows strips".to_string());
    }
    let units = utf16_len(name);
    if units > WINDOWS_MAX_COMPONENT {
        problems.push(format!(
            "is {units} UTF-16 code units long; a Windows name component may be at most {WINDOWS_MAX_COMPONENT}"
        ));
    }
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    if RESERVED_STEMS.contains(&stem.as_str()) {
        problems.push(format!(
            "is the reserved Windows device name `{stem}` (reserved with any extension)"
        ));
    }
    problems
}

/// The 1-based (line, column) of the first CR byte in `bytes`, if any. The
/// column counts bytes from the start of its line.
fn first_cr(bytes: &[u8]) -> Option<(usize, usize)> {
    let at = bytes.iter().position(|&b| b == b'\r')?;
    let before = &bytes[..at];
    let line = before.iter().filter(|&&b| b == b'\n').count() + 1;
    let line_start = before
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(0, |i| i + 1);
    Some((line, at - line_start + 1))
}

/// Lint a rendered tree (see [`lint_files`]).
pub fn lint_tree(
    tree: &RenderedTree,
    target: &Target,
    env: &str,
    label: &dyn Fn(&RelPath) -> String,
) -> Vec<DeployFinding> {
    lint_files(&tree.files, target, env, label)
}

/// The fail-closed tree lints, all collected in one pass (DRND-08, D-01,
/// D-04). They apply to every environment, because `resources/` is shared.
///
/// - per path component (once per distinct component path): characters
///   Windows rejects, a trailing `.` or space, more than
///   [`WINDOWS_MAX_COMPONENT`] UTF-16 units, a reserved device stem;
/// - per parent directory: sibling names (files and directories) that differ
///   only by case under `str::to_lowercase`, naming every colliding sibling;
/// - per file: [`windows_prefix_len`] + 1 + the UTF-16 length of the
///   collection path must be below [`WINDOWS_MAX_PATH`];
/// - per text member ([`is_text_member`]): the first CR byte, with its line
///   and column (D-01; binary members such as jars are exempt);
/// - `target.mode`, held to the component rules (finding on
///   `targets/<env>.yaml`).
///
/// `label` names the source file a finding should carry for a collection path.
/// The Windows rules approximate NTFS and are fail-closed; the real NTFS
/// capture is Phase 22 (A4, D-04). CD6 records how an 8.3.6 gateway on Linux
/// encodes hostile names.
pub fn lint_files(
    files: &BTreeMap<RelPath, Vec<u8>>,
    target: &Target,
    env: &str,
    label: &dyn Fn(&RelPath) -> String,
) -> Vec<DeployFinding> {
    let mut findings = Vec::new();
    let finding = |file: String, line: Option<(usize, usize)>, message: String| DeployFinding {
        file,
        line: line.map(|l| l.0),
        column: line.map(|l| l.1),
        message,
    };

    for problem in windows_problems(&target.mode) {
        findings.push(finding(
            format!("targets/{env}.yaml"),
            None,
            format!("mode `{}` {problem}", target.mode),
        ));
    }

    let prefix = windows_prefix_len(target);
    // distinct component paths already checked, and the sibling groups
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    // parent -> lowercased name -> name -> label of the first file under it
    let mut siblings: BTreeMap<&str, BTreeMap<String, BTreeMap<&str, String>>> = BTreeMap::new();

    for (path, bytes) in files {
        let full = path.as_str();
        let source = label(path);

        let mut end = 0;
        for name in path.components() {
            let start = end;
            end += name.len();
            let upto = &full[..end];
            if end < full.len() {
                end += 1; // the `/` after this component
            }
            if !seen.insert(upto) {
                continue;
            }
            let parent = if start == 0 { "" } else { &full[..start - 1] };
            siblings
                .entry(parent)
                .or_default()
                .entry(name.to_lowercase())
                .or_default()
                .entry(name)
                .or_insert_with(|| source.clone());
            for problem in windows_problems(name) {
                findings.push(finding(
                    source.clone(),
                    None,
                    format!("path component `{name}` {problem}"),
                ));
            }
        }

        let total = prefix + 1 + utf16_len(full);
        if total >= WINDOWS_MAX_PATH {
            findings.push(finding(
                source.clone(),
                None,
                format!(
                    "the path would be {total} UTF-16 code units long on Windows (data directory, `{}`, the mode and `{full}`); it must be shorter than {WINDOWS_MAX_PATH}",
                    COLLECTION_SEGMENT.trim_matches('\\')
                ),
            ));
        }

        if is_text_member(full, bytes)
            && let Some((line, column)) = first_cr(bytes)
        {
            findings.push(finding(
                source,
                Some((line, column)),
                format!(
                    "carriage return (0x0D) at line {line} of a text file; render refuses CR so no tool can convert line endings (use LF)"
                ),
            ));
        }
    }

    for (parent, groups) in &siblings {
        for names in groups.values().filter(|names| names.len() > 1) {
            let shown: Vec<String> = names
                .keys()
                .map(|n| {
                    if parent.is_empty() {
                        format!("`{n}`")
                    } else {
                        format!("`{parent}/{n}`")
                    }
                })
                .collect();
            let first = names.values().next().cloned().unwrap_or_default();
            findings.push(finding(
                first,
                None,
                format!(
                    "sibling names collide when case is folded ({}); a case-insensitive filesystem keeps only one",
                    shown.join(", ")
                ),
            ));
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deploy::node;

    fn collect(json: &str) -> Vec<(String, String)> {
        let mut out = BTreeSet::new();
        referenced_secret_names(&node::parse(json).unwrap(), &mut out);
        out.into_iter().collect()
    }

    fn pair(provider: &str, name: &str) -> (String, String) {
        (provider.to_string(), name.to_string())
    }

    const REF_A: &str = r#"{"type":"Referenced","data":{"providerName":"p","secretName":"a"}}"#;

    #[test]
    fn collects_every_referenced_pair() {
        let doc = format!(
            r#"{{"top":{REF_A},"nested":{{"deep":{{"type":"Referenced","data":{{"providerName":"p","secretName":"b"}}}}}},"list":[1,{{"type":"Referenced","data":{{"providerName":"q","secretName":"c"}}}}]}}"#
        );
        assert_eq!(
            collect(&doc),
            [pair("p", "a"), pair("p", "b"), pair("q", "c")]
        );
        // The root itself may be a reference.
        assert_eq!(collect(REF_A), [pair("p", "a")]);
    }

    #[test]
    fn duplicate_reference_collects_once() {
        let doc = format!(r#"{{"x":{REF_A},"y":[{REF_A}]}}"#);
        assert_eq!(collect(&doc), [pair("p", "a")]);
    }

    #[test]
    fn same_name_two_providers_collects_twice() {
        let doc = r#"[{"type":"Referenced","data":{"providerName":"p2","secretName":"a"}},{"type":"Referenced","data":{"providerName":"p1","secretName":"a"}}]"#;
        // Byte order of provider, then name.
        assert_eq!(collect(doc), [pair("p1", "a"), pair("p2", "a")]);
    }

    #[test]
    fn no_reference_collects_nothing() {
        assert!(collect(r#"{"a":1,"b":["x",{"c":null}]}"#).is_empty());
        assert!(collect(r#"{}"#).is_empty());
        assert!(collect(r#""just a string""#).is_empty());
    }

    #[test]
    fn lookalikes_are_ignored() {
        for doc in [
            // an extra member beside type/data
            r#"{"type":"Referenced","data":{"providerName":"p","secretName":"a"},"x":1}"#,
            // lower case type
            r#"{"type":"referenced","data":{"providerName":"p","secretName":"a"}}"#,
            // a non-string secret name
            r#"{"type":"Referenced","data":{"providerName":"p","secretName":7}}"#,
            // a third member in data
            r#"{"type":"Referenced","data":{"providerName":"p","secretName":"a","extra":"v"}}"#,
            // a missing provider
            r#"{"type":"Referenced","data":{"secretName":"a"}}"#,
            // an Embedded object is not a reference
            r#"{"type":"Embedded","data":{"ciphertext":"X"}}"#,
        ] {
            assert!(collect(doc).is_empty(), "{doc}");
        }
    }

    #[test]
    fn a_lookalike_does_not_hide_a_reference_inside_it() {
        let doc = format!(r#"{{"type":"Referenced","extra":{REF_A}}}"#);
        assert_eq!(collect(&doc), [pair("p", "a")]);
    }

    fn key(module: &str, ty: &str) -> ResourceKey {
        ResourceKey {
            module: module.to_string(),
            ty: ty.to_string(),
            name: Some("x".to_string()),
        }
    }

    fn scan(module: &str, ty: &str, json: &str) -> SecretScan {
        scan_secrets(
            "f/config.json",
            &key(module, ty),
            "config.json",
            &node::parse(json).unwrap(),
        )
    }

    #[test]
    fn embedded_nested_in_arrays_is_found_with_index_path() {
        let s = scan(
            "m",
            "t",
            r#"{"a":[{"b":{"type":"Embedded","data":{"ciphertext":"X"}}}]}"#,
        );
        assert_eq!(s.refused, ["f/config.json: a[0].b (embedded)"]);
    }

    #[test]
    fn embedded_without_object_data_is_not_embedded() {
        let s = scan("m", "t", r#"{"a":{"type":"Embedded","data":"x"}}"#);
        assert!(s.refused.is_empty());
    }

    #[test]
    fn embedded_at_a_root_is_reported_as_root() {
        let s = scan_embedded(
            "f.json",
            &node::parse(r#"{"type":"Embedded","data":{}}"#).unwrap(),
        );
        assert_eq!(s.refused, ["f.json: $ (embedded)"]);
    }

    #[test]
    fn schema_path_values() {
        let ok = scan("ignition", "database-connection", r#"{"password":null}"#);
        assert_eq!(ok, SecretScan::default());
        let ok = scan(
            "ignition",
            "database-connection",
            r#"{"password":{"type":"Referenced","data":{"providerName":"p","secretName":"s"}}}"#,
        );
        assert_eq!(ok, SecretScan::default());
        for bad in [r#""x""#, r#""""#, "1", "true", "[]", "{}"] {
            let s = scan(
                "ignition",
                "database-connection",
                &format!(r#"{{"password":{bad}}}"#),
            );
            assert_eq!(
                s.refused,
                ["f/config.json: password (plaintext)"],
                "value {bad}"
            );
        }
    }

    #[test]
    fn non_config_files_get_only_the_embedded_rule() {
        let node = node::parse(r#"{"password":"x"}"#).unwrap();
        let s = scan_secrets(
            "f/other.json",
            &key("com.cirruslink.mqtt.engine.gateway", "server"),
            "other.json",
            &node,
        );
        assert_eq!(s, SecretScan::default());
    }

    #[test]
    fn key_name_in_a_described_type_warns_by_path() {
        let s = scan(
            "ignition",
            "database-connection",
            r#"{"settings":{"mqttPassword":"CANARY"}}"#,
        );
        assert!(s.refused.is_empty());
        assert_eq!(s.warnings.len(), 1);
        assert!(s.warnings[0].contains("settings.mqttPassword"));
        assert!(!s.warnings[0].contains("CANARY"));
    }

    #[test]
    fn referenced_data_secret_name_is_a_name() {
        let s = scan(
            "com.cirruslink.mqtt.engine.gateway",
            "server",
            r#"{"password":{"type":"Referenced","data":{"providerName":"p","secretName":"s"}}}"#,
        );
        assert_eq!(s, SecretScan::default());
    }

    #[test]
    fn referenced_with_extra_data_member_at_schema_path_is_plaintext() {
        let s = scan(
            "ignition",
            "database-connection",
            r#"{"password":{"type":"Referenced","data":{"providerName":"p","secretName":"s","password":"hunter2"}}}"#,
        );
        assert_eq!(s.refused, ["f/config.json: password (plaintext)"]);
        assert!(s.warnings.is_empty(), "{:?}", s.warnings);
    }

    #[test]
    fn key_name_rule_applies_inside_malformed_referenced() {
        let s = scan(
            "com.cirruslink.mqtt.engine.gateway",
            "server",
            r#"{"settings":{"cred":{"type":"Referenced","data":{"providerName":"p","secretName":"s","password":"hunter2"}}}}"#,
        );
        assert_eq!(
            s.refused,
            [
                "f/config.json: settings.cred.data.secretName (plaintext, by key name)",
                "f/config.json: settings.cred.data.password (plaintext, by key name)",
            ]
        );
    }

    /// A Referenced-shaped object at the database-connection `password` path,
    /// each deviating from the CD7 shape in one way.
    const MALFORMED_REFERENCED: [&str; 14] = [
        r#"{"type":"Referenced","data":{"providerName":"p","secretName":"s","password":"x"}}"#,
        r#"{"type":"Referenced","data":{"providerName":"p"}}"#,
        r#"{"type":"Referenced","data":{"secretName":"s"}}"#,
        r#"{"type":"Referenced","data":{}}"#,
        r#"{"type":"Referenced","data":{"providerName":1,"secretName":"s"}}"#,
        r#"{"type":"Referenced","data":{"providerName":"p","secretName":null}}"#,
        r#"{"type":"Referenced","data":{"providerName":"p","secretName":{"v":"x"}}}"#,
        r#"{"type":"Referenced","data":"x"}"#,
        r#"{"type":"Referenced","data":[]}"#,
        r#"{"type":"Referenced","data":{"providerName":"p","secretName":"s"},"value":"x"}"#,
        r#"{"type":"referenced","data":{"providerName":"p","secretName":"s"}}"#,
        r#"{"type":1,"data":{"providerName":"p","secretName":"s"}}"#,
        r#"{"data":{"providerName":"p","secretName":"s"}}"#,
        r#"{"type":"Referenced","data":{"providername":"p","secretName":"s"}}"#,
    ];

    #[test]
    fn malformed_referenced_variants_are_plaintext_at_schema_path() {
        for (i, variant) in MALFORMED_REFERENCED.iter().enumerate() {
            let s = scan(
                "ignition",
                "database-connection",
                &format!(r#"{{"password":{variant}}}"#),
            );
            assert_eq!(
                s.refused,
                ["f/config.json: password (plaintext)"],
                "variant {i}"
            );
            assert!(s.warnings.is_empty(), "variant {i}: {:?}", s.warnings);
        }
    }

    #[test]
    fn well_formed_referenced_passes_in_either_key_order() {
        for referenced in [
            r#"{"data":{"providerName":"p","secretName":"s"},"type":"Referenced"}"#,
            r#"{"type":"Referenced","data":{"secretName":"s","providerName":"p"}}"#,
        ] {
            let s = scan(
                "ignition",
                "database-connection",
                &format!(r#"{{"password":{referenced}}}"#),
            );
            assert_eq!(s, SecretScan::default(), "{referenced}");
            let s = scan(
                "com.cirruslink.mqtt.engine.gateway",
                "server",
                &format!(
                    r#"{{"password":{referenced},"settings":{{"clientSecret":{referenced}}}}}"#
                ),
            );
            assert_eq!(s, SecretScan::default(), "{referenced}");
        }
    }

    #[test]
    fn malformed_referenced_outside_schema_path_warns_in_described_type() {
        let s = scan(
            "ignition",
            "database-connection",
            r#"{"settings":{"cred":{"type":"Referenced","data":{"providerName":"p","secretName":"s","password":"hunter2"}}}}"#,
        );
        assert!(s.refused.is_empty(), "{:?}", s.refused);
        assert_eq!(s.warnings.len(), 2, "{:?}", s.warnings);
        assert!(
            s.warnings
                .iter()
                .any(|w| w.contains("settings.cred.data.secretName;"))
        );
        assert!(
            s.warnings
                .iter()
                .any(|w| w.contains("settings.cred.data.password;"))
        );
        assert!(s.warnings.iter().all(|w| !w.contains("hunter2")));
    }

    // ---- Windows safety, budget and CR ----------------------------------

    use proptest::prelude::*;

    fn target(mode: &str, extra_nodes: &str) -> Target {
        let yaml = format!(
            "schema_version: 1\ngateway: g\nmode: {mode}\nnodes:\n  master:\n    profile: p\n{extra_nodes}"
        );
        Target::parse("t.yaml", yaml.as_bytes()).unwrap()
    }

    fn files(entries: &[(&str, &[u8])]) -> BTreeMap<RelPath, Vec<u8>> {
        entries
            .iter()
            .map(|(p, b)| (RelPath::new(p).unwrap(), b.to_vec()))
            .collect()
    }

    fn lint(entries: &[(&str, &[u8])], t: &Target) -> Vec<DeployFinding> {
        lint_files(&files(entries), t, "dev", &|p| format!("src/{p}"))
    }

    fn texts(findings: &[DeployFinding]) -> String {
        findings
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" | ")
    }

    #[test]
    fn case_fold_collision_names_both() {
        let t = target("dev", "");
        let found = lint(
            &[
                ("ignition/schedule/Pump/config.json", b"{}"),
                ("ignition/schedule/pump/config.json", b"{}"),
                ("ignition/schedule/Other/config.json", b"{}"),
            ],
            &t,
        );
        assert_eq!(found.len(), 1, "{}", texts(&found));
        let text = texts(&found);
        assert!(text.contains("`ignition/schedule/Pump`"), "{text}");
        assert!(text.contains("`ignition/schedule/pump`"), "{text}");
        assert!(!text.contains("Other"), "{text}");

        // files collide as well as directories, and three-way groups name all
        let found = lint(&[("a/b/X", b""), ("a/b/x", b""), ("a/b/X.", b"")], &t);
        let text = texts(&found);
        assert!(
            text.contains("`a/b/X`") && text.contains("`a/b/x`"),
            "{text}"
        );
        // whole names fold, not a prefix
        assert!(lint(&[("a/b/x", b""), ("a/b/xy", b"")], &t).is_empty());
        // non-ASCII folds with to_lowercase
        assert_eq!(lint(&[("a/b/É", b""), ("a/b/é", b"")], &t).len(), 1);
    }

    #[test]
    fn illegal_characters_are_refused() {
        let t = target("dev", "");
        for bad in [
            "a<b", "a>b", "a:b", "a\"b", "a|b", "a?b", "a*b", "a\u{1}b", "a\u{1f}b", "a\u{7f}b",
        ] {
            let found = lint(&[(&format!("m/t/{bad}/config.json"), b"{}")], &t);
            assert_eq!(found.len(), 1, "{bad:?}: {}", texts(&found));
            assert!(
                found[0].message.contains("does not allow in a name"),
                "{bad:?}: {}",
                texts(&found)
            );
        }
        // each distinct component is reported once, however many files sit below
        let found = lint(&[("m/t/a:b/x.json", b"{}"), ("m/t/a:b/y.json", b"{}")], &t);
        assert_eq!(found.len(), 1, "{}", texts(&found));
        // legal punctuation passes
        assert!(lint(&[("m/t/a b-c_d.e%3ab/x.json", b"{}")], &t).is_empty());
    }

    #[test]
    fn trailing_dot_and_space_are_refused() {
        let t = target("dev", "");
        for bad in ["trail.", "trail ", "a b ", "dots.."] {
            let found = lint(&[(&format!("m/t/{bad}/x.json"), b"{}")], &t);
            assert_eq!(found.len(), 1, "{bad:?}: {}", texts(&found));
            assert!(found[0].message.contains("ends in"), "{}", texts(&found));
        }
        assert!(lint(&[("m/t/inner .dot/x.json", b"{}")], &t).is_empty());
    }

    #[test]
    fn reserved_names_are_refused() {
        let t = target("dev", "");
        for stem in RESERVED_STEMS {
            for name in [stem.to_string(), stem.to_lowercase(), format!("{stem}.txt")] {
                let found = lint(&[(&format!("m/t/{name}/x.json"), b"{}")], &t);
                assert_eq!(found.len(), 1, "{name}: {}", texts(&found));
                assert!(found[0].message.contains("reserved"), "{}", texts(&found));
            }
        }
        // the stem is what precedes the first dot, compared whole
        for ok in [
            "CON1", "COM0", "COM10", "xcon", "CONSOLE", "LPT", "a.CON", "COMM",
        ] {
            assert!(
                lint(&[(&format!("m/t/{ok}/x.json"), b"{}")], &t).is_empty(),
                "{ok}"
            );
        }
        // a file name is a component too
        assert_eq!(lint(&[("m/t/n/nul.json", b"{}")], &t).len(), 1);
    }

    #[test]
    fn component_length_boundary() {
        let t = target("d", "");
        // only the component rule is under test here, not the path budget
        let component = |path: &str| texts(&lint(&[(path, b"")], &t)).contains("name component");
        let long = |n: usize| format!("m/t/{}/x", "a".repeat(n));
        assert!(!component(&long(255)));
        assert!(component(&long(256)));
        assert!(texts(&lint(&[(&long(256), b"")], &t)).contains("256 UTF-16 code units"));
        // a non-BMP character is two UTF-16 units
        assert!(!component(&format!("m/t/{}a/x", "\u{1F600}".repeat(127))));
        assert!(component(&format!("m/t/{}/x", "\u{1F600}".repeat(128))));
        // 255 UTF-16 units but 510 bytes: bytes do not decide
        assert!(!component(&format!("m/t/{}/x", "\u{e9}".repeat(255))));
    }

    /// A relative path of exactly `total` UTF-16 units made of short parts.
    fn path_of(total: usize) -> String {
        let mut path = String::from("m/t/n/");
        path.push_str(&"a".repeat(total - path.len()));
        path
    }

    #[test]
    fn path_budget_boundary() {
        // 51 (default data dir) + 18 (collection segment) + 10 (mode) = 79,
        // plus 1 separator before the relative path
        let t = target("production", "");
        assert_eq!(windows_prefix_len(&t), 79);
        let ok = path_of(167);
        assert!(lint(&[(&ok, b"")], &t).is_empty(), "247 passes");
        let bad = path_of(168);
        let found = lint(&[(&bad, b"")], &t);
        assert_eq!(found.len(), 1, "{}", texts(&found));
        assert!(
            found[0].message.contains("248 UTF-16 code units"),
            "{}",
            texts(&found)
        );

        // a longer Windows data_dir (63 characters, 12 more) lowers the
        // allowed relative length by 12
        let dir = format!("D:\\{}", "d".repeat(60));
        let windows = format!(
            "    ssh:\n      host: h\n      user: u\n      os: windows\n      data_dir: '{dir}'\n      known_hosts: kh\n"
        );
        let t2 = target("production", &windows);
        assert_eq!(windows_prefix_len(&t2), 79 + 12);
        assert!(lint(&[(&path_of(167 - 12), b"")], &t2).is_empty());
        assert_eq!(lint(&[(&path_of(168 - 12), b"")], &t2).len(), 1);

        // a trailing separator is trimmed and the backup node counts
        let backup = "  backup:\n    profile: b\n    ssh:\n      host: h\n      user: u\n      os: windows\n      data_dir: 'C:\\Program Files\\Inductive Automation\\Ignition\\data\\ab\\'\n      known_hosts: kh\n";
        assert_eq!(windows_prefix_len(&target("production", backup)), 79 + 3);

        // a Linux node never lowers the budget
        let linux = format!(
            "    ssh:\n      host: h\n      user: u\n      os: linux\n      data_dir: /{}\n      known_hosts: kh\n",
            "l".repeat(100)
        );
        assert_eq!(windows_prefix_len(&target("production", &linux)), 79);
    }

    #[test]
    fn mode_is_held_to_the_component_rules() {
        let t = target("prod.", "");
        let found = lint(&[], &t);
        assert_eq!(found.len(), 1, "{}", texts(&found));
        assert_eq!(found[0].file, "targets/dev.yaml");
        assert!(found[0].message.contains("mode `prod.`"));
        assert_eq!(lint(&[], &target("CON", "")).len(), 1);
    }

    #[test]
    fn cr_in_text_member_is_refused_with_line() {
        let t = target("dev", "");
        let found = lint(
            &[("m/t/n/config.json", b"{\n  \"a\": 1,\r\n  \"b\": 2\n}")],
            &t,
        );
        assert_eq!(found.len(), 1, "{}", texts(&found));
        assert_eq!((found[0].line, found[0].column), (Some(2), Some(10)));
        assert_eq!(found[0].file, "src/m/t/n/config.json");
        assert!(
            found[0]
                .to_string()
                .starts_with("src/m/t/n/config.json:2:10: "),
            "{}",
            found[0]
        );
        // the first CR wins; a lone CR counts as well
        let found = lint(&[("m/t/n/a.txt", b"x\ry\r\n")], &t);
        assert_eq!(found[0].line, Some(1));
    }

    #[test]
    fn cr_in_jar_is_not_refused() {
        let t = target("dev", "");
        // NUL bytes: not text
        assert!(lint(&[("m/t/n/driver.jar", b"PK\r\n\x00\x01")], &t).is_empty());
        // invalid UTF-8: not text
        assert!(lint(&[("m/t/n/driver.bin", b"\xff\xfe\r\n")], &t).is_empty());
    }

    #[test]
    fn cr_in_unknown_text_extension_is_refused() {
        let t = target("dev", "");
        for ext in [
            "json",
            "css",
            "yaml",
            "yml",
            "txt",
            "md",
            "xml",
            "py",
            "csv",
            "properties",
            "html",
            "js",
            "svg",
            "JSON",
            "Xml",
            "weird",
            "noext",
        ] {
            let name = if ext == "noext" {
                "m/t/n/LICENSE".to_string()
            } else {
                format!("m/t/n/f.{ext}")
            };
            let found = lint(&[(&name, b"a\r\nb")], &t);
            assert_eq!(found.len(), 1, "{name}: {}", texts(&found));
        }
        // a known text extension is text whatever its bytes
        assert!(is_text_member("a.json", b"\x00\xff"));
        assert!(!is_text_member("a.jar", b"\x00PK"));
        assert!(is_text_member("a.jar", b"plain"));
        // a dot in a directory name is not an extension
        assert!(!is_text_member("dir.json/blob", b"\x00"));
    }

    #[test]
    fn empty_inputs_pass() {
        let t = target("dev", "");
        assert!(lint(&[], &t).is_empty());
        assert!(lint(&[("m/t/n/empty.txt", b""), ("m/t/n/empty.bin", b"")], &t).is_empty());
    }

    fn reserved(name: &str) -> bool {
        RESERVED_STEMS.contains(
            &name
                .split('.')
                .next()
                .unwrap()
                .to_ascii_uppercase()
                .as_str(),
        )
    }

    proptest! {
        #[test]
        fn prop_extra_data_member_is_refused_without_value(
            key in "[A-Za-z][A-Za-z0-9]{0,12}",
            value in "CANARY-[A-Za-z0-9]{6,12}",
        ) {
            prop_assume!(key != "providerName" && key != "secretName");
            let json = format!(
                r#"{{"password":{{"type":"Referenced","data":{{"providerName":"p","secretName":"s","{key}":"{value}"}}}}}}"#
            );
            let s = scan("ignition", "database-connection", &json);
            prop_assert_eq!(&s.refused, &["f/config.json: password (plaintext)".to_string()]);
            for text in s.refused.iter().chain(s.warnings.iter()) {
                prop_assert!(!text.contains(&value), "value leaked: {text}");
            }
        }

        #[test]
        fn prop_illegal_char_names_refused(
            base in "[A-Za-z0-9_-]{1,20}",
            at in 0usize..=20,
            bad in prop::sample::select(vec!['<', '>', ':', '"', '|', '?', '*', '\u{1}', '\u{8}', '\u{1f}']),
        ) {
            let mut name = base.clone();
            name.insert(at.min(base.len()), bad);
            let found = lint(
                &[(&format!("m/t/{name}/x.json"), b"{}")],
                &target("dev", ""),
            );
            prop_assert!(!found.is_empty(), "{name:?} passed");
        }

        #[test]
        fn prop_legal_names_pass(name in "[A-Za-z0-9_-]([A-Za-z0-9_ .-]{0,30}[A-Za-z0-9_-])?") {
            prop_assume!(!reserved(&name));
            let found = lint(
                &[(&format!("m/t/{name}/x.json"), b"{}")],
                &target("dev", ""),
            );
            prop_assert!(found.is_empty(), "{name:?}: {}", texts(&found));
        }
    }
}
