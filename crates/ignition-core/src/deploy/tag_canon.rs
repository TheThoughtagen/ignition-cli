//! Canonical form of native tag JSON, for the plan diff and the verify of
//! project-owned tags (Phase 26, TAGDEP-08, research `## Revision 3`).
//!
//! The repo side is the git module's export ([`super::native_tags`]); the
//! live side is the gateway's REST export (`tags/export?provider=P`), cut into
//! the same per-file nodes by [`flatten_export`]. Neither is compared byte for
//! byte. [`canonicalize`] applies, in this order:
//!
//! - **R3-VK** (with [`CanonOpts::strip_values`]). `value`, `timestamp` and
//!   `quality` are runtime state of an AtomicTag ([`VALUE_KEYS`]).
//!   [`strip_runtime_values`] removes them as direct properties of an object
//!   whose `tagType` is `AtomicTag` (a top-level tag or an instance member)
//!   and nowhere else: never inside a `parameters` object, never another key
//!   of a member override, never below `_types_/`, where a member's `value`
//!   is the type's default and therefore configuration.
//! - **R3-KEY.** Object keys are sorted; the children of every `tags` array
//!   and of every `alarms` array are keyed by `name`, those of
//!   `eventScripts` by `eventid`, so the order a gateway lists them in never
//!   counts.
//! - **Stubs.** Inside a UDT instance (and the folders of one), a member that
//!   holds nothing but `name`, `tagType` and stub members is the module's
//!   export of "no override" and equals an absent member. A type's own
//!   members are its definition and are never stubs.
//! - **Nulls.** A `parameters` entry whose `value` is `null` equals the same
//!   entry without `value`, and so does an AtomicTag's `value: null` (the
//!   gateway exports no `value` for it).
//! - **Empties.** `parameters: {}` and `tags: []` equal an absent key: the
//!   gateway exports neither.
//! - **Numbers.** Numbers compare by value: `0` equals `0.0` and `2` equals
//!   `2.0` (the gateway writes a Float tag's `0` back as `0.0`). A string
//!   never equals a number.
//!
//! None of this touches [`canonical`], the form `tags_sha256` hashes.
//!
//! **R3-DV** needs both sides, so [`diff_properties`] applies it before
//! canonicalizing: the gateway adds `defaultValue` (equal to the sent
//! `value`) to an AtomicTag created without one (capture C12).
//! [`absorb_default_value`] is the one implementation; it accepts exactly
//! that difference and nothing else. Known limit KL-1: a repo that drops a
//! `defaultValue` key it used to carry is not reported, because the live
//! `defaultValue` is then indistinguishable from the gateway's own.
//!
//! Every result names property paths, never a value (T-26-14). Pure: values
//! in, values out.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde_json::{Map, Value};

use super::native_tags::TYPES_DIR;
use super::node::Node;
use crate::error::CoreError;

/// The runtime keys of an AtomicTag, removed in every comparison of a node
/// outside `_types_/` (R3-VK, D-03). Only direct properties of an object whose
/// `tagType` is `AtomicTag` are runtime keys.
pub const VALUE_KEYS: [&str; 3] = ["value", "timestamp", "quality"];

/// The property R3-DV is about.
const DEFAULT_VALUE: &str = "defaultValue";

/// How a node is canonicalized and compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CanonOpts {
    /// Remove the AtomicTag runtime keys (R3-VK, [`strip_runtime_values`]).
    /// Verify always passes `true`; the plan passes `false` only to find the
    /// nodes whose repo value differs from the live one.
    pub strip_values: bool,
    /// The run created the live node: R3-DV's seed check applies
    /// ([`absorb_default_value`]).
    pub created: bool,
    /// Both sides are gateway exports (R3-DV part 5: the run's agreement of
    /// two verify reads, a drift node against its pre-image): each holds the
    /// gateway's own `defaultValue`, so R3-DV is not applied and
    /// `defaultValue` is compared as configuration. Only R3-VK and R3-KEY
    /// apply.
    pub exports: bool,
}

/// Whether `v` is an object whose `tagType` is `AtomicTag`.
fn is_atomic(v: &Value) -> bool {
    tag_type(v) == Some("AtomicTag")
}

/// `v`'s `tagType`, when it is a string.
fn tag_type(v: &Value) -> Option<&str> {
    v.get("tagType").and_then(Value::as_str)
}

/// Whether a provider-relative path is below `_types_/`.
pub fn in_types(path: &str) -> bool {
    path.strip_prefix(TYPES_DIR)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// `v` with object keys sorted, recursively, and the children of every `tags`
/// array ordered by `name` (R3-KEY). Strips and normalizes nothing else: this
/// is the form `tags_sha256` hashes, so it must not change for the same input.
/// Comparisons use [`canonicalize`].
pub fn canonical(v: &Value) -> Value {
    match v {
        Value::Object(object) => {
            let mut keys: Vec<&String> = object.keys().collect();
            keys.sort();
            let mut out = Map::new();
            for key in keys {
                let child = canonical(&object[key.as_str()]);
                let child = match (key.as_str(), child) {
                    ("tags", Value::Array(mut items)) => {
                        items.sort_by(|a, b| child_key(a, "name").cmp(child_key(b, "name")));
                        Value::Array(items)
                    }
                    (_, child) => child,
                };
                out.insert(key.clone(), child);
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical).collect()),
        other => other.clone(),
    }
}

/// A child's string `field`, or empty when it has none.
fn child_key<'a>(v: &'a Value, field: &str) -> &'a [u8] {
    v.get(field)
        .and_then(Value::as_str)
        .unwrap_or("")
        .as_bytes()
}

/// The field that keys the children of the array at `key`, for the arrays
/// R3-KEY keys.
fn keyed_by(key: &str) -> Option<&'static str> {
    match key {
        "tags" | "alarms" => Some("name"),
        "eventScripts" => Some("eventid"),
        _ => None,
    }
}

/// Remove the AtomicTag runtime keys ([`VALUE_KEYS`]) from `node` and from
/// every AtomicTag member below it in `tags` (R3-VK), for the node at
/// provider-relative `path`. Never descends into `parameters` or any key
/// other than `tags`; does nothing below `_types_/`. The one stripping rule:
/// comparisons, import bodies, re-sends and rollback re-imports all use it.
pub fn strip_runtime_values(node: &mut Value, path: &str) {
    if !in_types(path) {
        strip_atomic(node);
    }
}

fn strip_atomic(node: &mut Value) {
    if is_atomic(node)
        && let Some(object) = node.as_object_mut()
    {
        for key in VALUE_KEYS {
            object.remove(key);
        }
    }
    if let Some(children) = node.get_mut("tags").and_then(Value::as_array_mut) {
        for child in children {
            strip_atomic(child);
        }
    }
}

/// R3-DV for one AtomicTag outside `_types_/` and, by name, for every member
/// below it in `tags`, in a comparison of the repo node `repo` with the live
/// node `live`.
///
/// When the repo node carries `defaultValue`, nothing is done: it is
/// configuration and is compared. When it carries none, a live `defaultValue`
/// is the gateway's own addition and is removed from `live`, except on a node
/// the run `created` whose repo node carries `value`: there the live
/// `defaultValue` must first equal that `value` canonically (the C12 seed).
/// A live member the repo node does not hold counts as a stub (no
/// `defaultValue`, no `value`). A seed that differs is `Err` naming the first
/// such property path (`defaultValue`, `tags.M1.defaultValue`) and stays in
/// `live`, so the comparison reports it. Never a value. KL-1: a removed repo
/// `defaultValue` is not reported.
pub fn absorb_default_value(repo: &Value, live: &mut Value, created: bool) -> Result<(), String> {
    let mut mismatch = None;
    absorb_at(repo, live, created, "", &mut mismatch);
    mismatch.map_or(Ok(()), Err)
}

fn absorb_at(
    repo: &Value,
    live: &mut Value,
    created: bool,
    at: &str,
    mismatch: &mut Option<String>,
) {
    if is_atomic(repo)
        && repo.get(DEFAULT_VALUE).is_none()
        && let Some(live_default) = live.get(DEFAULT_VALUE)
    {
        let seed_differs = created
            && repo
                .get("value")
                .is_some_and(|seed| normalize(live_default, false) != normalize(seed, false));
        if seed_differs {
            mismatch.get_or_insert_with(|| join_path(at, DEFAULT_VALUE));
        } else if let Some(object) = live.as_object_mut() {
            object.remove(DEFAULT_VALUE);
        }
    }
    let Some(members) = live.get_mut("tags").and_then(Value::as_array_mut) else {
        return;
    };
    let repo_members = repo.get("tags").and_then(Value::as_array);
    for member in members {
        let Some(name) = member
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            continue;
        };
        let stub;
        let repo_member = match repo_members.and_then(|items| {
            items
                .iter()
                .find(|m| m.get("name").and_then(Value::as_str) == Some(name.as_str()))
        }) {
            Some(found) => found,
            None => {
                stub = serde_json::json!({ "tagType": tag_type(member).unwrap_or("") });
                &stub
            }
        };
        let at = join_path(&join_path(at, "tags"), &name);
        absorb_at(repo_member, member, created, &at, mismatch);
    }
}

/// `prefix.key`, or `key` at the top.
fn join_path(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_string()
    } else {
        format!("{prefix}.{key}")
    }
}

/// The canonical form of the node at provider-relative `path`: R3-VK when
/// `opts.strip_values` (never below `_types_/`), then R3-KEY, member stubs
/// dropped, nulls, empties and numbers normalized (module doc). Two nodes are
/// equal configuration when their canonical forms are equal, R3-DV aside.
pub fn canonicalize(node: &Value, path: &str, opts: CanonOpts) -> Value {
    let mut node = node.clone();
    if opts.strip_values {
        strip_runtime_values(&mut node, path);
    }
    normalize(&node, false)
}

/// Whether the `tags` of `v` are instance members: `v` is a UDT instance, or
/// a folder inside one.
fn holds_members(v: &Value, in_instance: bool) -> bool {
    match tag_type(v) {
        Some("UdtInstance") => true,
        Some("Folder") => in_instance,
        _ => false,
    }
}

/// Whether a normalized instance member is a stub: nothing but `name` and
/// `tagType` (stub members were already dropped, and so was an emptied
/// `tags`).
fn is_stub(v: &Value) -> bool {
    v.as_object().is_some_and(|object| {
        matches!(tag_type(v), Some("AtomicTag" | "UdtInstance" | "Folder"))
            && object.keys().all(|key| key == "name" || key == "tagType")
    })
}

fn normalize(v: &Value, in_instance: bool) -> Value {
    match v {
        Value::Object(object) => {
            let members = holds_members(v, in_instance);
            let atomic = is_atomic(v);
            let mut out = Map::new();
            let mut keys: Vec<&String> = object.keys().collect();
            keys.sort();
            for key in keys {
                let child = &object[key.as_str()];
                // The gateway stores an AtomicTag `value: null` as no
                // `value`, and `parameters: {}` or `tags: []` as no key.
                let dropped = match (key.as_str(), child) {
                    ("value", Value::Null) => atomic,
                    ("parameters", Value::Object(params)) => params.is_empty(),
                    ("tags", Value::Array(items)) => items.is_empty(),
                    _ => false,
                };
                if dropped {
                    continue;
                }
                let child = match (key.as_str(), child) {
                    ("tags", Value::Array(items)) => {
                        let mut items: Vec<Value> =
                            items.iter().map(|item| normalize(item, members)).collect();
                        if members {
                            items.retain(|item| !is_stub(item));
                            if items.is_empty() {
                                continue;
                            }
                        }
                        items.sort_by(|a, b| child_key(a, "name").cmp(child_key(b, "name")));
                        Value::Array(items)
                    }
                    ("parameters", Value::Object(params)) => {
                        let mut params = params.clone();
                        for param in params.values_mut() {
                            if let Some(entry) = param.as_object_mut()
                                && entry.get("value") == Some(&Value::Null)
                            {
                                entry.remove("value");
                            }
                        }
                        normalize(&Value::Object(params), false)
                    }
                    (key, Value::Array(items)) if keyed_by(key).is_some() => {
                        let field = keyed_by(key).unwrap_or("name");
                        let mut items: Vec<Value> =
                            items.iter().map(|item| normalize(item, false)).collect();
                        items.sort_by(|a, b| child_key(a, field).cmp(child_key(b, field)));
                        Value::Array(items)
                    }
                    // A value is data (a Document tag holds any JSON): only
                    // its numbers compare by value, its keys are its own.
                    ("value", child) => normalize_data(child),
                    (_, child) => normalize(child, false),
                };
                out.insert(key.clone(), child);
            }
            Value::Object(out)
        }
        Value::Array(items) => {
            Value::Array(items.iter().map(|item| normalize(item, false)).collect())
        }
        Value::Number(n) => normalize_number(n),
        other => other.clone(),
    }
}

/// `v`, a tag's `value`, with only its numbers normalized
/// ([`normalize_number`]): no key is dropped and no array is reordered.
fn normalize_data(v: &Value) -> Value {
    match v {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(k, v)| (k.clone(), normalize_data(v)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(normalize_data).collect()),
        Value::Number(n) => normalize_number(n),
        other => other.clone(),
    }
}

/// 2^53: every integer of at most this magnitude is an exact `f64`.
const EXACT_INT_F64: f64 = 9_007_199_254_740_992.0;

/// A number in the form that compares by value: a float with no fraction
/// and a magnitude of at most 2^53 becomes that integer, so `0.0` equals `0`
/// and `2.0` equals `2` (the gateway writes `0.0` back for a Float tag sent
/// `0`). Anything else is kept as it is.
fn normalize_number(n: &serde_json::Number) -> Value {
    match n.as_f64() {
        Some(f) if n.is_f64() && f.fract() == 0.0 && f.abs() <= EXACT_INT_F64 => {
            // |f| <= 2^53: the float is an exact integer, so the cast is exact.
            Value::from(f as i64)
        }
        _ => Value::Number(n.clone()),
    }
}

/// The property paths that differ between the repo node `repo` and the live
/// node `live` at provider-relative `path`, byte-sorted. R3-DV
/// ([`absorb_default_value`], with `opts.created`) is applied outside
/// `_types_/` unless `opts.exports` (both sides are exports), then both sides
/// are [`canonicalize`]d with `opts`.
///
/// A path is dotted: `documentation`, `parameters.P1.value`,
/// `alarms.HighAlarm.priority` (keyed children by their key),
/// `tags.Member1.value`. A key or keyed child on one side only is its own
/// path; an instance member on one side only is compared with a stub, so its
/// properties are named. Names only, never a value. Empty means equal.
pub fn diff_properties(repo: &Value, live: &Value, path: &str, opts: CanonOpts) -> Vec<String> {
    let mut live = live.clone();
    if !in_types(path) && !opts.exports {
        // A seed mismatch stays on the live side, so the walk names it.
        let _ = absorb_default_value(repo, &mut live, opts.created);
    }
    let a = canonicalize(repo, path, opts);
    let b = canonicalize(&live, path, opts);
    let mut out = Vec::new();
    diff_walk(&a, &b, "", false, &mut |p| out.push(p), Side::Both);
    out.sort_by(|x, y| x.as_bytes().cmp(y.as_bytes()));
    out.dedup();
    out
}

/// Whether two nodes are equal configuration: [`diff_properties`] is empty.
pub fn nodes_equal(repo: &Value, live: &Value, path: &str, opts: CanonOpts) -> bool {
    diff_properties(repo, live, path, opts).is_empty()
}

/// The property paths present in `current` and absent in `pre` (both nodes
/// at provider-relative `path`), byte-sorted. Both sides are stripped
/// (R3-VK) and go through R3-DV with `pre` as the reference, so neither a
/// runtime key nor the gateway's own `defaultValue` counts as an addition.
/// A removed or changed property is not listed. R3-RB uses it to choose
/// `MergeOverwrite` (nothing added) or `Overwrite`.
pub fn added_properties(pre: &Value, current: &Value, path: &str) -> Vec<String> {
    let mut current = current.clone();
    if !in_types(path) {
        let _ = absorb_default_value(pre, &mut current, false);
    }
    let opts = CanonOpts {
        strip_values: true,
        created: false,
        exports: false,
    };
    let a = canonicalize(pre, path, opts);
    let b = canonicalize(&current, path, opts);
    let mut out = Vec::new();
    diff_walk(&a, &b, "", false, &mut |p| out.push(p), Side::Right);
    out.sort_by(|x, y| x.as_bytes().cmp(y.as_bytes()));
    out.dedup();
    out
}

/// Which differences a walk reports.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    /// Every difference ([`diff_properties`]).
    Both,
    /// Only paths that exist on the right and not on the left
    /// ([`added_properties`]).
    Right,
}

/// Walk two canonical values, reporting the dotted paths that differ (or,
/// with [`Side::Right`], that exist on the right only).
fn diff_walk(
    a: &Value,
    b: &Value,
    at: &str,
    in_instance: bool,
    report: &mut dyn FnMut(String),
    side: Side,
) {
    let (Value::Object(left), Value::Object(right)) = (a, b) else {
        if a != b && side == Side::Both {
            report(at.to_string());
        }
        return;
    };
    let members = holds_members(a, in_instance) || holds_members(b, in_instance);
    let mut keys: Vec<&String> = left.keys().chain(right.keys()).collect();
    keys.sort();
    keys.dedup();
    let empty = Value::Array(Vec::new());
    for key in keys {
        let here = join_path(at, key);
        let (l, r) = (left.get(key.as_str()), right.get(key.as_str()));
        let field = keyed_by(key);
        // Instance members missing on one side are stubs: compare them.
        let (l, r) = match (l, r) {
            (None, Some(r)) if key == "tags" && members => (&empty, r),
            (Some(l), None) if key == "tags" && members => (l, &empty),
            (Some(l), Some(r)) => (l, r),
            (None, Some(_)) => {
                report(here);
                continue;
            }
            (Some(_), None) => {
                if side == Side::Both {
                    report(here);
                }
                continue;
            }
            (None, None) => continue,
        };
        if l == r {
            continue;
        }
        match (field, keyed(l, field), keyed(r, field)) {
            (Some(_), Some(lk), Some(rk)) => {
                let child_instance = key == "tags" && members;
                let mut names: Vec<&String> = lk.keys().chain(rk.keys()).copied().collect();
                names.sort_by(|x, y| x.as_bytes().cmp(y.as_bytes()));
                names.dedup();
                for name in names {
                    let child = join_path(&here, name);
                    match (lk.get(name), rk.get(name)) {
                        (Some(x), Some(y)) => diff_walk(x, y, &child, child_instance, report, side),
                        (Some(x), None) if child_instance => {
                            diff_walk(x, &stub_of(x), &child, true, report, side);
                        }
                        (None, Some(y)) if child_instance => {
                            diff_walk(&stub_of(y), y, &child, true, report, side);
                        }
                        (None, Some(_)) => report(child),
                        (Some(_), None) => {
                            if side == Side::Both {
                                report(child);
                            }
                        }
                        (None, None) => {}
                    }
                }
            }
            _ if l.is_object() && r.is_object() => diff_walk(l, r, &here, false, report, side),
            _ => {
                if side == Side::Both {
                    report(here);
                }
            }
        }
    }
}

/// The children of a keyed array by their key, or `None` when `v` is not an
/// array whose every child carries a distinct string key.
fn keyed<'a>(v: &'a Value, field: Option<&str>) -> Option<BTreeMap<&'a String, &'a Value>> {
    let field = field?;
    let mut out = BTreeMap::new();
    for item in v.as_array()? {
        let Some(Value::String(key)) = item.get(field) else {
            return None;
        };
        if out.insert(key, item).is_some() {
            return None;
        }
    }
    Some(out)
}

/// The stub of a member: its name and type, nothing else.
fn stub_of(member: &Value) -> Value {
    let mut stub = Map::new();
    for key in ["name", "tagType"] {
        if let Some(v) = member.get(key) {
            stub.insert(key.to_string(), v.clone());
        }
    }
    Value::Object(stub)
}

/// The byte form of a native file writer: what `--check` compares a repo
/// file against ([`canonical_file_bytes`] is the module's form).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FileForm<'a> {
    /// Write object keys in byte order (the module does); otherwise in
    /// source order.
    pub(crate) sort_keys: bool,
    /// One indent level.
    pub(crate) indent: &'a str,
}

/// The git module's form: `TAG_GSON.toJson(createDeterministicCopy(node))`.
pub(crate) const MODULE_FORM: FileForm<'static> = FileForm {
    sort_keys: true,
    indent: "  ",
};

/// The bytes the git module writes for `json` (TAGDEP-12): object keys sorted
/// recursively, a 2-space indent, `": "` between key and value, LF line
/// ends, no trailing newline, non-ASCII kept, Gson's HTML-safe escapes (`<`
/// `>` `&` `=` `'` and U+2028/U+2029 as `\uXXXX`), and arrays in their own
/// order (the module sorts keys only). Numbers are written as serde_json
/// formats them; [`write_file_form`] over a parsed file keeps the source
/// lexeme instead.
pub fn canonical_file_bytes(json: &Value) -> Vec<u8> {
    write_file_form(&to_node(json), MODULE_FORM).into_bytes()
}

/// `json` as a strict-reader tree (keys in the map's order).
pub(crate) fn to_node(json: &Value) -> Node {
    match json {
        Value::Object(object) => Node::Obj(
            object
                .iter()
                .map(|(key, value)| (key.clone(), to_node(value)))
                .collect(),
        ),
        Value::Array(items) => Node::Arr(items.iter().map(to_node).collect()),
        Value::String(text) => Node::Str(text.clone()),
        Value::Number(number) => Node::Lit(number.to_string()),
        Value::Bool(flag) => Node::Lit(flag.to_string()),
        Value::Null => Node::Lit("null".to_string()),
    }
}

/// Write `tree` in `form`, with Gson's string escapes. Number, boolean and
/// null lexemes are written as they are.
pub(crate) fn write_file_form(tree: &Node, form: FileForm<'_>) -> String {
    let mut out = String::new();
    write_at(tree, form, 0, &mut out);
    out
}

fn write_at(tree: &Node, form: FileForm<'_>, depth: usize, out: &mut String) {
    let newline = |out: &mut String, depth: usize| {
        out.push('\n');
        for _ in 0..depth {
            out.push_str(form.indent);
        }
    };
    match tree {
        Node::Obj(members) if members.is_empty() => out.push_str("{}"),
        Node::Arr(items) if items.is_empty() => out.push_str("[]"),
        Node::Obj(members) => {
            let mut order: Vec<&(String, Node)> = members.iter().collect();
            if form.sort_keys {
                order.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
            }
            out.push('{');
            for (i, (key, value)) in order.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(out, depth + 1);
                write_string(key, out);
                out.push_str(": ");
                write_at(value, form, depth + 1, out);
            }
            newline(out, depth);
            out.push('}');
        }
        Node::Arr(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(out, depth + 1);
                write_at(item, form, depth + 1, out);
            }
            newline(out, depth);
            out.push(']');
        }
        Node::Str(text) => write_string(text, out),
        Node::Lit(lexeme) => out.push_str(lexeme),
    }
}

/// A JSON string as Gson's HTML-safe writer escapes it.
fn write_string(text: &str, out: &mut String) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\u{c}' => out.push_str("\\f"),
            c if u32::from(c) < 0x20 || GSON_HTML_ESCAPED.contains(c) => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// The characters Gson's HTML-safe writer escapes as `\uXXXX` besides the
/// control characters.
pub(crate) const GSON_HTML_ESCAPED: &str = "<>&='\u{2028}\u{2029}";

/// Cut a provider's REST export into the nodes of the native layout: each
/// node keyed by the same provider-relative path [`super::native_tags`] gives
/// it, without its `name`. A folder keeps its own properties and loses its
/// `tags` (its children are nodes of their own); any other node keeps its
/// whole subtree, as its file would. The root folder `_types_` is structural
/// and not a node; the types below it are.
///
/// Accepts what [`super::project_tags::TagTree::parse`] accepts: a `Provider`
/// root holding a `tags` array, objects below it, a string `name` and an
/// array `tags` where present. It also refuses a node without a `name` and two
/// nodes at one path, which cannot be compared node by node.
pub fn flatten_export(bytes: &[u8], provider: &str) -> Result<BTreeMap<String, Value>, CoreError> {
    let root: Value = serde_json::from_slice(bytes).map_err(|_| {
        CoreError::Internal(format!(
            "the tag export of provider {provider} is not a tag tree: not valid JSON"
        ))
    })?;
    flatten_tree(&root, provider)
}

/// [`flatten_export`] over an export already parsed.
pub fn flatten_tree(root: &Value, provider: &str) -> Result<BTreeMap<String, Value>, CoreError> {
    let bad = |why: &str| {
        CoreError::Internal(format!(
            "the tag export of provider {provider} is not a tag tree: {why}"
        ))
    };
    if root.get("tagType").and_then(Value::as_str) != Some("Provider") {
        return Err(bad("the root is not a Provider"));
    }
    let Some(tags) = root.get("tags").and_then(Value::as_array) else {
        return Err(bad("the root holds no tags array"));
    };
    let mut out = BTreeMap::new();
    flatten_level(tags, "", &mut out).map_err(|why| bad(&why))?;
    Ok(out)
}

fn flatten_level(
    tags: &[Value],
    prefix: &str,
    out: &mut BTreeMap<String, Value>,
) -> Result<(), String> {
    for node in tags {
        let Some(object) = node.as_object() else {
            return Err("a node is not an object".to_string());
        };
        let Some(name) = object.get("name").and_then(Value::as_str) else {
            return Err("a node has no string name".to_string());
        };
        let children = match object.get("tags") {
            None => None,
            Some(Value::Array(children)) => Some(children),
            Some(_) => return Err("a node's tags is not an array".to_string()),
        };
        let path = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}/{name}")
        };
        let folder = object.get("tagType").and_then(Value::as_str) == Some("Folder");
        let mut own = object.clone();
        own.remove("name");
        if folder {
            own.remove("tags");
        }
        if !(prefix.is_empty() && folder && name == TYPES_DIR)
            && out.insert(path.clone(), Value::Object(own)).is_some()
        {
            return Err("two nodes share one path".to_string());
        }
        if folder && let Some(children) = children {
            flatten_level(children, &path, out)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const STRIP: CanonOpts = CanonOpts {
        strip_values: true,
        created: false,
        exports: false,
    };
    const STRIP_CREATED: CanonOpts = CanonOpts {
        strip_values: true,
        created: true,
        exports: false,
    };
    const KEEP: CanonOpts = CanonOpts {
        strip_values: false,
        created: false,
        exports: false,
    };

    #[test]
    fn canonical_sorts_keys_and_keys_children_by_name() {
        let a = json!({"tagType":"UdtType","tags":[{"name":"B","x":1},{"name":"A","y":2}]});
        let b = json!({"tags":[{"y":2,"name":"A"},{"x":1,"name":"B"}],"tagType":"UdtType"});
        assert_eq!(canonical(&a), canonical(&b));
        assert_eq!(
            serde_json::to_string(&canonical(&a)).expect("json"),
            r#"{"tagType":"UdtType","tags":[{"name":"A","y":2},{"name":"B","x":1}]}"#
        );
    }

    /// R3-KEY: `tags` and `alarms` by `name`, `eventScripts` by `eventid`.
    #[test]
    fn key_and_child_order_never_count() {
        let repo = json!({
            "tagType": "AtomicTag",
            "documentation": "d",
            "alarms": [{"name": "Lo", "priority": "Low"}, {"name": "Hi", "priority": "High"}],
            "eventScripts": [{"eventid": "valueChanged", "script": "a"}, {"eventid": "qualityChanged", "script": "b"}]
        });
        let live = json!({
            "eventScripts": [{"script": "b", "eventid": "qualityChanged"}, {"script": "a", "eventid": "valueChanged"}],
            "alarms": [{"priority": "High", "name": "Hi"}, {"priority": "Low", "name": "Lo"}],
            "documentation": "d",
            "tagType": "AtomicTag"
        });
        assert!(nodes_equal(&repo, &live, "F/T", STRIP));
        assert_eq!(
            canonicalize(&repo, "F/T", KEEP),
            canonicalize(&live, "F/T", KEEP)
        );
        let repo = json!({"tagType":"UdtType","tags":[
            {"name":"B","tagType":"AtomicTag","value":1},{"name":"A","tagType":"AtomicTag","value":2}]});
        let live = json!({"tagType":"UdtType","tags":[
            {"name":"A","tagType":"AtomicTag","value":2},{"name":"B","tagType":"AtomicTag","value":1}]});
        assert!(nodes_equal(&repo, &live, "_types_/Ty", STRIP));
    }

    /// Member stubs (atomic, instance and folder forms) equal absent members,
    /// inside an instance only. A type's member is its definition.
    #[test]
    fn instance_member_stubs_equal_absent_members() {
        let stubs = json!({"tagType":"UdtInstance","typeId":"Motor","tags":[
            {"name":"M1","tagType":"AtomicTag"},
            {"name":"V","tagType":"UdtInstance","tags":[{"name":"Open","tagType":"AtomicTag"}]},
            {"name":"Grp","tagType":"Folder","tags":[{"name":"X","tagType":"AtomicTag"}]}
        ]});
        let bare = json!({"tagType":"UdtInstance","typeId":"Motor"});
        assert!(nodes_equal(&stubs, &bare, "Line1/Motor", STRIP));
        assert!(nodes_equal(&stubs, &bare, "Line1/Motor", KEEP));
        assert_eq!(
            canonicalize(&stubs, "Line1/Motor", KEEP),
            canonicalize(&bare, "Line1/Motor", KEEP)
        );
        // A member override is not a stub.
        let over = json!({"tagType":"UdtInstance","typeId":"Motor","tags":[
            {"name":"Grp","tagType":"Folder","tags":[{"name":"X","tagType":"AtomicTag","documentation":"o"}]}
        ]});
        assert_eq!(
            diff_properties(&over, &bare, "Line1/Motor", STRIP),
            ["tags.Grp.tags.X.documentation"]
        );
        // Under _types_/ a member with only a name and a type is a member.
        let ty = json!({"tagType":"UdtType","tags":[{"name":"M1","tagType":"AtomicTag"}]});
        let empty = json!({"tagType":"UdtType"});
        assert_eq!(diff_properties(&ty, &empty, "_types_/Ty", STRIP), ["tags"]);
    }

    #[test]
    fn an_absent_parameter_value_equals_null() {
        let repo = json!({"tagType":"UdtType","parameters":{"P1":{"dataType":"Integer"}}});
        let live =
            json!({"tagType":"UdtType","parameters":{"P1":{"dataType":"Integer","value":null}}});
        assert!(nodes_equal(&repo, &live, "_types_/Ty", STRIP));
        let live =
            json!({"tagType":"UdtType","parameters":{"P1":{"dataType":"Integer","value":7}}});
        assert_eq!(
            diff_properties(&repo, &live, "_types_/Ty", STRIP),
            ["parameters.P1.value"]
        );
    }

    /// R3-VK: the value keys go only as direct properties of AtomicTag
    /// objects, top level or instance member, outside `_types_/`.
    #[test]
    fn strip_removes_atomic_runtime_keys_only() {
        let node = json!({
            "tagType": "UdtInstance",
            "value": "kept: not an AtomicTag",
            "parameters": {"P1": {"dataType": "Integer", "value": 7}},
            "tags": [
                {"name": "M1", "tagType": "AtomicTag", "value": 1, "timestamp": 2, "quality": 3, "documentation": "d"},
                {"name": "S", "tagType": "UdtInstance", "tags": [{"name": "M", "tagType": "AtomicTag", "value": 4, "tooltip": "t"}]}
            ]
        });
        let mut stripped = node.clone();
        strip_runtime_values(&mut stripped, "Line1/I");
        assert_eq!(stripped["value"], "kept: not an AtomicTag");
        assert_eq!(
            stripped["parameters"]["P1"]["value"], 7,
            "parameters are config"
        );
        assert_eq!(
            stripped["tags"][0],
            json!({"name":"M1","tagType":"AtomicTag","documentation":"d"})
        );
        assert_eq!(
            stripped["tags"][1]["tags"][0],
            json!({"name":"M","tagType":"AtomicTag","tooltip":"t"}),
            "other keys of a member override stay"
        );
        // canonicalize applies exactly the same rule.
        assert_eq!(
            canonicalize(&node, "Line1/I", STRIP),
            canonicalize(&stripped, "Line1/I", KEEP)
        );
        // strip_values false keeps the value keys.
        assert_eq!(canonicalize(&node, "Line1/I", KEEP)["tags"][0]["value"], 1);
        // A top-level AtomicTag.
        let mut tag =
            json!({"tagType":"AtomicTag","value":1,"timestamp":2,"quality":3,"defaultValue":4});
        strip_runtime_values(&mut tag, "F/T");
        assert_eq!(tag, json!({"tagType":"AtomicTag","defaultValue":4}));
        // Nothing below _types_/: a member value is the type's default.
        let ty = json!({"tagType":"UdtType","tags":[{"name":"M","tagType":"AtomicTag","value":0}]});
        let mut kept = ty.clone();
        strip_runtime_values(&mut kept, "_types_/Ty");
        assert_eq!(kept, ty);
        assert_eq!(
            canonicalize(&ty, "_types_/Ty", STRIP)["tags"][0]["value"],
            0
        );
    }

    /// Dotted property paths, byte-sorted, never a value.
    #[test]
    fn diff_properties_names_paths_never_values() {
        let repo = json!({"tagType":"UdtInstance","documentation":"secret-sentinel-a",
            "alarms":[{"name":"HighAlarm","priority":"High"}],
            "tags":[{"name":"Member1","tagType":"AtomicTag","value":"secret-sentinel-c"}]});
        let live = json!({"tagType":"UdtInstance","documentation":"secret-sentinel-b",
            "alarms":[{"name":"HighAlarm","priority":"Low"}],
            "tags":[{"name":"Member1","tagType":"AtomicTag","value":"secret-sentinel-d"}]});
        let diff = diff_properties(&repo, &live, "Line1/I", KEEP);
        assert_eq!(
            diff,
            [
                "alarms.HighAlarm.priority",
                "documentation",
                "tags.Member1.value"
            ]
        );
        assert!(!diff.join(" ").contains("sentinel"));
        // With the runtime keys stripped the member value no longer counts.
        assert_eq!(
            diff_properties(&repo, &live, "Line1/I", STRIP),
            ["alarms.HighAlarm.priority", "documentation"]
        );
        // A whole alarm on one side is the alarm's path.
        let live = json!({"tagType":"UdtInstance","documentation":"secret-sentinel-a",
            "alarms":[{"name":"HighAlarm","priority":"High"},{"name":"LowAlarm","priority":"Low"}]});
        assert_eq!(
            diff_properties(&repo, &live, "Line1/I", STRIP),
            ["alarms.LowAlarm"]
        );
    }

    #[test]
    fn default_value_rule_r3_dv() {
        let repo = json!({"tagType":"AtomicTag","value":3});
        // (a) C12: the gateway adds defaultValue equal to the sent value.
        let live = json!({"tagType":"AtomicTag","value":3,"defaultValue":3});
        assert!(nodes_equal(&repo, &live, "F/T", STRIP));
        assert!(nodes_equal(&repo, &live, "F/T", STRIP_CREATED));
        // (b) A different live defaultValue: the gateway's own on an
        // existing node (KL-1 side), a seed mismatch on a created one.
        let live = json!({"tagType":"AtomicTag","defaultValue":4});
        assert!(nodes_equal(&repo, &live, "F/T", STRIP));
        assert_eq!(
            diff_properties(&repo, &live, "F/T", STRIP_CREATED),
            ["defaultValue"]
        );
        // (c) A repo defaultValue is configuration on any node.
        let repo_dv = json!({"tagType":"AtomicTag","value":3,"defaultValue":1});
        let live = json!({"tagType":"AtomicTag","defaultValue":2});
        for opts in [STRIP, STRIP_CREATED] {
            assert_eq!(
                diff_properties(&repo_dv, &live, "F/T", opts),
                ["defaultValue"]
            );
        }
        // (d) An instance member follows the same rule.
        let repo = json!({"tagType":"UdtInstance","typeId":"Ty","tags":[{"name":"M1","tagType":"AtomicTag","value":5}]});
        let live = json!({"tagType":"UdtInstance","typeId":"Ty","tags":[{"name":"M1","tagType":"AtomicTag","value":5,"defaultValue":5}]});
        assert!(nodes_equal(&repo, &live, "F/I", STRIP));
        assert!(nodes_equal(&repo, &live, "F/I", STRIP_CREATED));
        let live = json!({"tagType":"UdtInstance","typeId":"Ty","tags":[{"name":"M1","tagType":"AtomicTag","defaultValue":4}]});
        assert!(nodes_equal(&repo, &live, "F/I", STRIP));
        assert_eq!(
            diff_properties(&repo, &live, "F/I", STRIP_CREATED),
            ["tags.M1.defaultValue"]
        );
        // (e) Under _types_/ there is no rule.
        let repo =
            json!({"tagType":"UdtType","tags":[{"name":"M1","tagType":"AtomicTag","value":0}]});
        let live = json!({"tagType":"UdtType","tags":[{"name":"M1","tagType":"AtomicTag","value":0,"defaultValue":0}]});
        assert_eq!(
            diff_properties(&repo, &live, "_types_/Ty", STRIP),
            ["tags.M1.defaultValue"]
        );
    }

    #[test]
    fn absorb_default_value_is_the_one_r3_dv_implementation() {
        let repo = json!({"tagType":"AtomicTag","value":3});
        let mut live = json!({"tagType":"AtomicTag","value":3,"defaultValue":3});
        absorb_default_value(&repo, &mut live, true).expect("the seed matches");
        assert!(live.get("defaultValue").is_none());
        let mut live = json!({"tagType":"AtomicTag","value":3,"defaultValue":4});
        assert_eq!(
            absorb_default_value(&repo, &mut live, true),
            Err("defaultValue".to_string())
        );
        assert_eq!(live["defaultValue"], 4, "left for the comparison to name");
        absorb_default_value(&repo, &mut live, false).expect("existing");
        assert!(live.get("defaultValue").is_none());
        let repo = json!({"tagType":"AtomicTag","defaultValue":5});
        let mut live = json!({"tagType":"AtomicTag","defaultValue":6});
        absorb_default_value(&repo, &mut live, true).expect("compared, not absorbed");
        assert_eq!(live["defaultValue"], 6);
        // Members: matched by name; a member the repo lacks is a stub.
        let repo =
            json!({"tagType":"UdtInstance","tags":[{"name":"M1","tagType":"AtomicTag","value":5}]});
        let mut live = json!({"tagType":"UdtInstance","tags":[
            {"name":"M1","tagType":"AtomicTag","defaultValue":9},
            {"name":"M2","tagType":"AtomicTag","defaultValue":1}]});
        assert_eq!(
            absorb_default_value(&repo, &mut live, true),
            Err("tags.M1.defaultValue".to_string())
        );
        assert_eq!(live["tags"][0]["defaultValue"], 9);
        assert!(live["tags"][1].get("defaultValue").is_none());
    }

    /// The C12 bodies (26-LIVE-CAPTURES.md): the import requests against
    /// the export straight after. R3-DV absorbs the one recorded difference,
    /// `F/T.defaultValue`, and nothing else differs.
    #[test]
    fn c12_bodies_compare_equal_under_r3_dv() {
        let sent = json!({"name":"","tagType":"Provider","tags":[
            {"name":"_types_","tagType":"Folder","tags":[
                {"name":"Ty","parameters":{"P1":{"dataType":"Integer"}},"tagType":"UdtType","tags":[
                    {"dataType":"Int4","name":"M1","tagType":"AtomicTag","value":0,"valueSource":"memory"},
                    {"dataType":"String","documentation":"m2","name":"M2","tagType":"AtomicTag","valueSource":"memory"}]}]},
            {"name":"F","tagType":"Folder","tags":[
                {"dataType":"Int4","documentation":"t","name":"T","tagType":"AtomicTag","value":3,"valueSource":"memory"},
                {"name":"I","parameters":{"P1":{"dataType":"Integer","value":7}},"tagType":"UdtInstance","tags":[
                    {"name":"M1","tagType":"AtomicTag"},{"name":"M2","tagType":"AtomicTag"}],"typeId":"Ty"}]}]});
        let exported = json!({"name":"","tagType":"Provider","tags":[
            {"name":"_types_","tagType":"Folder","tags":[
                {"name":"Ty","parameters":{"P1":{"dataType":"Integer"}},"tagType":"UdtType","tags":[
                    {"dataType":"Int4","name":"M1","tagType":"AtomicTag","value":0,"valueSource":"memory"},
                    {"dataType":"String","documentation":"m2","name":"M2","tagType":"AtomicTag","valueSource":"memory"}]}]},
            {"name":"F","tagType":"Folder","tags":[
                {"dataType":"Int4","defaultValue":3,"documentation":"t","name":"T","tagType":"AtomicTag","value":3,"valueSource":"memory"},
                {"name":"I","parameters":{"P1":{"dataType":"Integer","value":7}},"tagType":"UdtInstance","tags":[
                    {"name":"M1","tagType":"AtomicTag"},{"name":"M2","tagType":"AtomicTag"}],"typeId":"Ty"}]}]});
        let flat = |v: &Value| {
            flatten_export(&serde_json::to_vec(v).expect("json"), "P26C12").expect("flat")
        };
        let (repo, live) = (flat(&sent), flat(&exported));
        assert_eq!(
            repo.keys().collect::<Vec<_>>(),
            live.keys().collect::<Vec<_>>()
        );
        for (path, node) in &repo {
            for opts in [
                STRIP_CREATED,
                CanonOpts {
                    strip_values: false,
                    created: true,
                    exports: false,
                },
            ] {
                assert!(
                    diff_properties(node, &live[path], path, opts).is_empty(),
                    "{path}"
                );
            }
            // Without R3-DV, F/T.defaultValue is the only difference.
            let raw: Vec<String> = {
                let (a, b) = (canonical(node), canonical(&live[path]));
                let (a, b) = (a.as_object().expect("obj"), b.as_object().expect("obj"));
                let mut keys: Vec<String> = a
                    .keys()
                    .chain(b.keys())
                    .filter(|k| a.get(*k) != b.get(*k))
                    .cloned()
                    .collect();
                keys.dedup();
                keys
            };
            let want: &[&str] = if path == "F/T" {
                &["defaultValue"]
            } else {
                &[]
            };
            assert_eq!(raw, want, "{path}");
        }
        // The type member M1 kept `value` 0 and gained no defaultValue.
        assert!(live["_types_/Ty"]["tags"][0].get("defaultValue").is_none());
    }

    /// R3-RB input: what the run added relative to the pre-image.
    #[test]
    fn added_properties_lists_paths_new_in_current() {
        let pre = json!({"tagType":"AtomicTag","documentation":"d","value":1,
            "alarms":[{"name":"Hi","priority":"High"}]});
        let current = json!({"tagType":"AtomicTag","documentation":"e","tooltip":"t","value":2,
            "timestamp":5,"defaultValue":2,
            "alarms":[{"name":"Hi","priority":"High","notes":"n"},{"name":"Lo","priority":"Low"}]});
        assert_eq!(
            added_properties(&pre, &current, "F/T"),
            ["alarms.Hi.notes", "alarms.Lo", "tooltip"],
            "value keys (R3-VK) and the gateway's defaultValue (R3-DV) are not additions"
        );
        // A removed property is not an addition.
        assert!(
            added_properties(&current, &pre, "F/T").is_empty(),
            "removals only"
        );
        // A member override where the pre-image held a stub.
        let pre = json!({"tagType":"UdtInstance","tags":[{"name":"M1","tagType":"AtomicTag"}]});
        let current = json!({"tagType":"UdtInstance","tags":[{"name":"M1","tagType":"AtomicTag","documentation":"o","value":4}]});
        assert_eq!(
            added_properties(&pre, &current, "F/I"),
            ["tags.M1.documentation"]
        );
    }

    #[test]
    fn flatten_export_uses_the_native_paths() {
        let export = json!({"name":"","tagType":"Provider","tags":[
            {"name":"_types_","tagType":"Folder","tags":[
                {"name":"Grp","tagType":"Folder","tags":[{"name":"Ty","tagType":"UdtType","tags":[{"name":"M","tagType":"AtomicTag"}]}]}
            ]},
            {"name":"F","tagType":"Folder","tags":[{"name":"T","tagType":"AtomicTag","value":3}]}
        ]});
        let nodes = flatten_export(&serde_json::to_vec(&export).expect("json"), "P").expect("flat");
        let paths: Vec<&str> = nodes.keys().map(String::as_str).collect();
        assert_eq!(paths, ["F", "F/T", "_types_/Grp", "_types_/Grp/Ty"]);
        assert_eq!(nodes["F"], json!({"tagType":"Folder"}));
        assert_eq!(nodes["F/T"], json!({"tagType":"AtomicTag","value":3}));
        assert_eq!(nodes["_types_/Grp/Ty"]["tags"][0]["name"], "M");
    }

    #[test]
    fn flatten_export_refuses_what_is_not_a_tag_tree() {
        for bad in [
            br#"{"tagType":"Provider","tags":3}"#.as_slice(),
            br#"{"name":"","tagType":"Provider"}"#,
            br#"{"tagType":"Folder","tags":[]}"#,
            br#"not json"#,
            br#"{"tagType":"Provider","tags":[{"tagType":"AtomicTag"}]}"#,
            br#"{"tagType":"Provider","tags":[{"name":"A"},{"name":"A"}]}"#,
            br#"{"tagType":"Provider","tags":[{"name":"F","tagType":"Folder","tags":{}}]}"#,
        ] {
            assert!(
                flatten_export(bad, "P").is_err(),
                "{}",
                String::from_utf8_lossy(bad)
            );
        }
        // The empty 8.3.6 provider: only the structural _types_ folder.
        let empty =
            br#"{"name":"","tagType":"Provider","tags":[{"name":"_types_","tagType":"Folder"}]}"#;
        assert!(flatten_export(empty, "P").expect("empty").is_empty());
    }

    /// The git module's byte form: keys sorted, 2-space indent, `": "`, LF,
    /// no trailing newline, non-ASCII kept, Gson HTML escapes, arrays kept
    /// in order.
    #[test]
    fn canonical_file_bytes_is_the_module_form() {
        let node = json!({
            "tagType": "AtomicTag",
            "documentation": "<a> & 'b' = c \"q\" back\\ caf\u{e9} tab\t ctl\u{1} ls\u{2028}",
            "alarms": [{"name": "Z", "priority": 1}, {"name": "A", "on": true, "setpoint": null}],
            "parameters": {},
            "tags": [],
            "value": 12.5
        });
        let want = concat!(
            "{\n",
            "  \"alarms\": [\n",
            "    {\n",
            "      \"name\": \"Z\",\n",
            "      \"priority\": 1\n",
            "    },\n",
            "    {\n",
            "      \"name\": \"A\",\n",
            "      \"on\": true,\n",
            "      \"setpoint\": null\n",
            "    }\n",
            "  ],\n",
            "  \"documentation\": \"\\u003ca\\u003e \\u0026 \\u0027b\\u0027 \\u003d c \\\"q\\\" back\\\\ caf\u{e9} tab\\t ctl\\u0001 ls\\u2028\",\n",
            "  \"parameters\": {},\n",
            "  \"tagType\": \"AtomicTag\",\n",
            "  \"tags\": [],\n",
            "  \"value\": 12.5\n",
            "}"
        );
        assert_eq!(
            String::from_utf8(canonical_file_bytes(&node)).expect("utf8"),
            want
        );
        // The neutral fixture was written in the module form by the measured
        // reproduction (canon.py): every node file is byte-identical.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/native-tags/good/tags/PlantA");
        for rel in [
            "Line1/Speed.json",
            "Line1/Motor%3AA.json",
            "_types_/Motor.json",
            "_types_/Valve.json",
        ] {
            let bytes = std::fs::read(root.join(rel)).expect("fixture");
            let value: Value = serde_json::from_slice(&bytes).expect("json");
            assert_eq!(canonical_file_bytes(&value), bytes, "{rel}");
        }
    }

    /// R3-DV part 5: with `exports` both sides are gateway exports, so a
    /// `defaultValue` on one side only is a difference, not the gateway's own
    /// addition; R3-VK and R3-KEY still apply.
    #[test]
    fn export_against_export_compares_default_value() {
        let pre = json!({"tagType": "AtomicTag", "valueSource": "memory", "value": 1});
        let after =
            json!({"tagType": "AtomicTag", "valueSource": "memory", "value": 2, "defaultValue": 1});
        assert!(
            nodes_equal(&pre, &after, "T", STRIP),
            "repo against export absorbs it"
        );
        let exports = CanonOpts {
            exports: true,
            ..STRIP
        };
        assert_eq!(
            diff_properties(&pre, &after, "T", exports),
            ["defaultValue"]
        );
        let same =
            json!({"tagType": "AtomicTag", "valueSource": "memory", "value": 7, "defaultValue": 1});
        assert!(
            nodes_equal(&after, &same, "T", exports),
            "values never count"
        );
    }

    /// What the gateway stores for what was written (8.3.6): an AtomicTag
    /// `value: null` comes back without `value`, in a type member too.
    #[test]
    fn an_atomic_null_value_equals_an_absent_value() {
        let repo = json!({"tagType":"AtomicTag","dataType":"Int4","value":null});
        let live = json!({"tagType":"AtomicTag","dataType":"Int4"});
        for opts in [STRIP, KEEP, STRIP_CREATED] {
            assert!(nodes_equal(&repo, &live, "F/T", opts), "{opts:?}");
        }
        let ty =
            json!({"tagType":"UdtType","tags":[{"name":"M","tagType":"AtomicTag","value":null}]});
        let live = json!({"tagType":"UdtType","tags":[{"name":"M","tagType":"AtomicTag"}]});
        assert!(nodes_equal(&ty, &live, "_types_/Scanner", STRIP));
        // Below `_types_/` a real default still counts.
        let live =
            json!({"tagType":"UdtType","tags":[{"name":"M","tagType":"AtomicTag","value":0}]});
        assert_eq!(
            diff_properties(&ty, &live, "_types_/Scanner", STRIP),
            ["tags.M.value"]
        );
        // Only an AtomicTag's `value`: a null elsewhere is still a value.
        let repo = json!({"tagType":"Folder","documentation":null});
        let live = json!({"tagType":"Folder"});
        assert_eq!(diff_properties(&repo, &live, "F", KEEP), ["documentation"]);
    }

    /// `parameters: {}` and an empty `tags: []` come back absent.
    #[test]
    fn empty_parameters_and_tags_equal_absent_ones() {
        let repo = json!({"tagType":"UdtType","parameters":{},"tags":[]});
        let live = json!({"tagType":"UdtType"});
        assert!(nodes_equal(&repo, &live, "_types_/Scanner", STRIP));
        let repo = json!({"tagType":"UdtInstance","typeId":"Scanner","parameters":{}});
        let live = json!({"tagType":"UdtInstance","typeId":"Scanner"});
        assert!(nodes_equal(&repo, &live, "F/I", KEEP));
        let repo = json!({"tagType":"Folder","tags":[]});
        assert!(nodes_equal(&repo, &json!({"tagType":"Folder"}), "F", KEEP));
        // A parameter that is there still counts.
        let repo = json!({"tagType":"UdtType","parameters":{"Enabled":{"dataType":"Integer"}}});
        assert_eq!(
            diff_properties(
                &repo,
                &json!({"tagType":"UdtType"}),
                "_types_/Scanner",
                STRIP
            ),
            ["parameters"]
        );
    }

    /// Numbers compare by value (`0` and `0.0`, `2` and `2.0`), in
    /// parameters too; a string never equals a number.
    #[test]
    fn numerically_equal_numbers_are_equal() {
        let repo = json!({"tagType":"AtomicTag","dataType":"Float4","value":0,"deadband":2});
        let live = json!({"tagType":"AtomicTag","dataType":"Float4","value":0.0,"deadband":2.0});
        for opts in [STRIP, KEEP] {
            assert!(nodes_equal(&repo, &live, "F/T", opts), "{opts:?}");
        }
        let repo = json!({"tagType":"UdtInstance","typeId":"Scanner",
            "parameters":{"Rate":{"dataType":"Float","value":2}}});
        let live = json!({"tagType":"UdtInstance","typeId":"Scanner",
            "parameters":{"Rate":{"dataType":"Float","value":2.0}}});
        assert!(nodes_equal(&repo, &live, "F/I", STRIP));
        let neg = json!({"tagType":"AtomicTag","dataType":"Float8","value":-3});
        let neg_live = json!({"tagType":"AtomicTag","dataType":"Float8","value":-3.0});
        assert!(nodes_equal(&neg, &neg_live, "F/T", KEEP));
        // Different numbers and number-like strings still differ.
        let live = json!({"tagType":"UdtInstance","typeId":"Scanner",
            "parameters":{"Rate":{"dataType":"Float","value":2.5}}});
        assert_eq!(
            diff_properties(&repo, &live, "F/I", STRIP),
            ["parameters.Rate.value"]
        );
        let live = json!({"tagType":"UdtInstance","typeId":"Scanner",
            "parameters":{"Rate":{"dataType":"Float","value":"2"}}});
        assert_eq!(
            diff_properties(&repo, &live, "F/I", STRIP),
            ["parameters.Rate.value"]
        );
        // A created node's seed: `value` 0 against the gateway's
        // `defaultValue` 0.0 is the C12 seed, not a mismatch.
        let repo = json!({"tagType":"AtomicTag","dataType":"Float4","value":0});
        let live =
            json!({"tagType":"AtomicTag","dataType":"Float4","value":0.0,"defaultValue":0.0});
        assert!(nodes_equal(&repo, &live, "F/T", STRIP_CREATED));
        // The hash form is untouched: `canonical` keeps the repo's bytes.
        assert_ne!(
            canonical(&json!({"value":0})),
            canonical(&json!({"value":0.0}))
        );
        // Up to 2^53 inclusive a whole float is its integer (CodeRabbit #51).
        let big =
            json!({"tagType":"AtomicTag","dataType":"Int8","value":9_000_000_000_000_000_i64});
        let big_live =
            json!({"tagType":"AtomicTag","dataType":"Int8","value":9_000_000_000_000_000.0});
        assert!(nodes_equal(&big, &big_live, "F/T", KEEP));
        let edge =
            json!({"tagType":"AtomicTag","dataType":"Int8","value":9_007_199_254_740_992_i64});
        let edge_live =
            json!({"tagType":"AtomicTag","dataType":"Int8","value":9_007_199_254_740_992.0});
        assert!(nodes_equal(&edge, &edge_live, "F/T", KEEP));
    }

    /// A Document value is data: an empty `tags` or `parameters` inside it
    /// is a key of the document, never dropped; its numbers still compare by
    /// value (CodeRabbit #51).
    #[test]
    fn a_document_value_keeps_its_empty_keys() {
        let doc = |value: Value| json!({"tagType":"AtomicTag","dataType":"Document","value":value});
        for (a, b) in [
            (json!({"tags": []}), json!({})),
            (json!({"parameters": {}}), json!({})),
            (json!([3, 1]), json!([1, 3])),
        ] {
            assert!(
                !nodes_equal(&doc(a.clone()), &doc(b.clone()), "_types_/T/D", KEEP),
                "{a} vs {b}"
            );
        }
        assert!(nodes_equal(
            &doc(json!({"n": 2})),
            &doc(json!({"n": 2.0})),
            "_types_/T/D",
            KEEP
        ));
    }
}
