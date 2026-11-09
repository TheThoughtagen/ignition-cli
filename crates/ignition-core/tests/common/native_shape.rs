//! A deterministic native tag export with a production provider's shape
//! (plan 26-14, TAGDEP-13).
//!
//! The project-owned tag deploy reads a provider from the git module's
//! individual-file form (`tags/<P>/`, one directory per folder, one
//! `<name>.json` per tag, UDT types below `_types_/`), plans it against the
//! gateway's REST export, batches the writes and rolls them back. This module
//! generates such a provider with the measured shape of the production one,
//! read-only from its repo export:
//!
//! | Measure | Production | Generated |
//! |---|---|---|
//! | Files below `tags/<P>/` | 3,267 (3,072 under one top folder, 194 types, 1 `.tag-groups.json`) | the same |
//! | Instance member stubs | 36,471 atomic, 728 instance, 586 folder | within 2% (see [`shape_of`]) |
//! | Type nodes | about 9,000 | within 10% |
//! | Definition nodes | about 43,000 | within 10% |
//! | Deepest node | 13 path components | 13 |
//! | `%3A` names | present | about one per 40 files |
//! | Memory AtomicTags | 56 with `value` and `defaultValue` (16 differ), 10 with `value` only | the same |
//!
//! Everything is built from fixed rules and the seed: no RNG, no clock, no
//! environment. The seed moves memory values only; the paths and the shape
//! never change. Every name below the provider is a word of [`WORDS`]
//! followed by digits ([`neutral_name`]); the provider is the plan's fixed
//! [`PROVIDER`]. Node files are written in the module's exact byte form
//! ([`canonical_file_bytes`]).
//!
//! [`live_export_from`] builds the REST export the fake gateway serves from
//! those files, in the gateway's form (C12: a value-only memory tag gains
//! `defaultValue` equal to its `value`; F-P1: children listed in another
//! order) and with the [`Knobs`] that make nodes added, changed, drift or
//! removed in a plan.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use ignition_core::deploy::native_tags::{NativeProvider, decode_fs_name, encode_fs_name};
use ignition_core::deploy::tag_canon::canonical_file_bytes;
use ignition_core::deploy::tag_record::TagRecord;
use serde_json::{Value, json};

use super::prod_shape::fnv;

/// The owned provider every generated export lives in.
pub const PROVIDER: &str = "PlantA";

/// The one top folder holding every definition.
pub const TOP_FOLDER: &str = "Area01";

/// The recorded production numbers the generator reproduces.
pub const PROVIDER_FILES: usize = 3_267;
pub const TOP_FOLDER_FILES: usize = 3_072;
pub const TYPE_FILES: usize = 194;
pub const MEMBER_STUBS: usize = 36_471;
pub const INSTANCE_STUBS: usize = 728;
pub const FOLDER_STUBS: usize = 586;
pub const TYPE_NODES: usize = 9_000;
pub const DEFINITION_NODES: usize = 43_000;
pub const MAX_DEPTH: usize = 13;
pub const MEMORY_WITH_DEFAULT: usize = 56;
pub const MEMORY_DEFAULT_DIFFERS: usize = 16;
pub const MEMORY_VALUE_ONLY: usize = 10;

/// The neutral vocabulary every generated name is built from.
pub const WORDS: [&str; 6] = ["Area", "Line", "Unit", "Motor", "Valve", "Sensor"];

/// Names that are not built from [`WORDS`]: the structural types folder and
/// Ignition's own default tag group.
pub const BUILT_IN_NAMES: [&str; 2] = ["_types_", "Default"];

/// The settings file of a materialized repo's owned provider.
pub const PROVIDER_SETTINGS: &str = r#"{"schema_version": 1, "enabled": true, "config": {}}"#;

/// The project a materialized repo deploys.
pub const PROJECT: &str = "ShapeApp";

// The generator's own numbers, tuned so the shape lands on the recorded one.
const LINES: usize = 12;
const UNITS: usize = 8;
const FILES_PER_UNIT: usize = 32;
const INSTANCES: usize = 800;
const MEMORY_TAGS: usize = MEMORY_WITH_DEFAULT + MEMORY_VALUE_ONLY;
const LEAF_TYPES: usize = 20;
const LEAF_MEMBERS: usize = 8;
const MAIN_TYPES: usize = TYPE_FILES - LEAF_TYPES;
const MEMBER_BASE: usize = 28;
const MEMBER_SPREAD: usize = 13;
const FOLDER_MEMBERS: usize = 6;
/// The spine: the first unit's files from this index on sit one folder
/// deeper per pair, down to [`MAX_DEPTH`].
const SPINE_FROM: usize = 14;

/// Whether item `index` of `of` is picked when `count` are spread evenly,
/// item 0 always included: exactly `count` are picked.
fn spread(index: usize, count: usize, of: usize) -> bool {
    let at = |i: usize| (i * count).div_ceil(of);
    at(index + 1) != at(index)
}

/// A fixed hash of `text` with every bit mixed: FNV-1a then the splitmix64
/// finalizer. FNV-1a alone barely moves its high bits for the last bytes,
/// so paths sharing a folder would sort together.
fn mixed(text: &str) -> u64 {
    let mut h = fnv(text);
    h = (h ^ (h >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^ (h >> 31)
}

/// Whether `name` is neutral: a word of [`WORDS`] followed by one or more
/// digits, optionally `:` and one or more digits; or one of
/// [`BUILT_IN_NAMES`].
pub fn neutral_name(name: &str) -> bool {
    if BUILT_IN_NAMES.contains(&name) {
        return true;
    }
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    WORDS.iter().any(|word| {
        name.strip_prefix(word)
            .is_some_and(|rest| match rest.split_once(':') {
                Some((number, suffix)) => digits(number) && digits(suffix),
                None => digits(rest),
            })
    })
}

/// One generated UDT type.
struct TypeDef {
    /// The `typeId`: the path below `_types_/`.
    id: String,
    /// Direct atomic members.
    members: usize,
    /// Holds the folder member `Unit01`.
    folder: bool,
    /// The leaf type of the nested instance member `Motor01`, if any.
    nested: Option<usize>,
}

fn leaf_types() -> Vec<TypeDef> {
    (1..=LEAF_TYPES)
        .map(|n| TypeDef {
            id: format!("Valve00/Valve{n:02}"),
            members: LEAF_MEMBERS,
            folder: false,
            nested: None,
        })
        .collect()
}

/// The kind of main type `t`: `(nested, folder)`. Most hold both, as most
/// production instances do.
fn main_kind(t: usize) -> (bool, bool) {
    match t {
        0..100 => (true, true),
        100..140 => (true, false),
        140..160 => (false, true),
        _ => (false, false),
    }
}

fn main_types() -> Vec<TypeDef> {
    (0..MAIN_TYPES)
        .map(|t| {
            let (nested, folder) = main_kind(t);
            let family = if t % 2 == 0 { "Motor" } else { "Sensor" };
            TypeDef {
                id: format!("{family}00/{family}{:03}", t + 1),
                members: MEMBER_BASE + (t * 7) % MEMBER_SPREAD,
                folder,
                nested: nested.then_some(t % LEAF_TYPES),
            }
        })
        .collect()
}

/// A type member: every third an OPC tag, the others memory tags whose
/// `value` is the type's default.
fn type_member(name: &str, k: usize) -> Value {
    if k % 3 == 2 {
        json!({
            "name": name, "tagType": "AtomicTag", "dataType": "Float8",
            "valueSource": "opc", "opcServer": "Ignition OPC UA Server",
            "opcItemPath": format!("ns=1;s=[Device]{{Line01}}/{name}")
        })
    } else {
        json!({
            "name": name, "tagType": "AtomicTag", "dataType": "Int4",
            "valueSource": "memory", "value": (k % 5) as i64
        })
    }
}

fn stub(name: &str) -> Value {
    json!({"name": name, "tagType": "AtomicTag"})
}

fn stubs(word: &str, n: usize) -> Vec<Value> {
    (1..=n).map(|k| stub(&format!("{word}{k:02}"))).collect()
}

fn type_json(t: &TypeDef, leafs: &[TypeDef]) -> Value {
    let mut members: Vec<Value> = (0..t.members)
        .map(|k| type_member(&format!("Sensor{:02}", k + 1), k))
        .collect();
    if t.folder {
        let inner: Vec<Value> = (0..FOLDER_MEMBERS)
            .map(|k| type_member(&format!("Valve{:02}", k + 1), k))
            .collect();
        members.push(json!({"name": "Unit01", "tagType": "Folder", "tags": inner}));
    }
    if let Some(leaf) = t.nested {
        members.push(json!({
            "name": "Motor01", "tagType": "UdtInstance", "typeId": leafs[leaf].id,
            "tags": stubs("Sensor", leafs[leaf].members)
        }));
    }
    if t.nested.is_none() && !t.folder && t.members == LEAF_MEMBERS {
        return json!({"tagType": "UdtType", "tags": members});
    }
    json!({
        "tagType": "UdtType",
        "parameters": {"Line01": {"dataType": "Integer"}},
        "tags": members
    })
}

/// An instance of `t`: one stub per direct member, a folder stub and a nested
/// instance stub mirroring the type, and a memory override on every tenth.
fn instance_json(t: &TypeDef, leafs: &[TypeDef], i: usize, line: usize, doc: &str) -> Value {
    let mut members = stubs("Sensor", t.members);
    if i % 10 == 3 {
        members[0]["value"] = json!(7);
    }
    if t.folder {
        members.push(json!({"name": "Unit01", "tagType": "Folder",
            "tags": stubs("Valve", FOLDER_MEMBERS)}));
    }
    if let Some(leaf) = t.nested {
        members.push(json!({"name": "Motor01", "tagType": "UdtInstance",
            "tags": stubs("Sensor", leafs[leaf].members)}));
    }
    json!({
        "tagType": "UdtInstance",
        "typeId": t.id,
        "parameters": {"Line01": {"dataType": "Integer", "value": line as i64}},
        "documentation": doc,
        "tags": members
    })
}

/// The provider-relative folder of file `j` of unit `(line, unit)`.
fn folder_of(line: usize, unit: usize, j: usize) -> String {
    let base = format!("{TOP_FOLDER}/Line{line:02}/Unit{unit:02}");
    if line == 1 && unit == 1 && j >= SPINE_FROM {
        let level = 2 + (j - SPINE_FROM) / 2;
        let spine: Vec<String> = (2..=level).map(|n| format!("Unit{n:02}")).collect();
        return format!("{base}/{}", spine.join("/"));
    }
    base
}

/// The repo-relative file of the provider-relative node `path`.
fn file_of(path: &str) -> String {
    let encoded: Vec<String> = path.split('/').map(encode_fs_name).collect();
    format!("tags/{PROVIDER}/{}.json", encoded.join("/"))
}

/// The generated native export: every file of a project repo's `tags/`
/// (`tags/.tag-config.json`, `tags/<P>/.tag-groups.json`, the types and the
/// definitions), as `(repo-relative path, bytes)`, byte-sorted by path.
pub fn native_export(seed: u64) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    files.push((
        "tags/.tag-config.json".to_string(),
        canonical_file_bytes(&json!({
            "collisionPolicy": "o", "excludedTagPaths": [], "includedProviders": [PROVIDER]
        })),
    ));
    files.push((
        format!("tags/{PROVIDER}/.tag-groups.json"),
        serde_json::to_vec_pretty(&json!({"tagGroups": [
            {"name": "Default", "rate": 1000, "mode": "Direct", "oneShot": false}
        ]}))
        .expect("group summary"),
    ));
    let leafs = leaf_types();
    let mains = main_types();
    for t in leafs.iter().chain(&mains) {
        files.push((
            file_of(&format!("_types_/{}", t.id)),
            canonical_file_bytes(&type_json(t, &leafs)),
        ));
    }
    // the main types by kind, for the instances to pick from
    let mut by_kind: BTreeMap<(bool, bool), Vec<usize>> = BTreeMap::new();
    for (t, def) in mains.iter().enumerate() {
        by_kind
            .entry((def.nested.is_some(), def.folder))
            .or_default()
            .push(t);
    }
    let atomics = TOP_FOLDER_FILES - INSTANCES;
    let (mut g, mut i, mut a, mut q) = (0usize, 0usize, 0usize, 0usize);
    for line in 1..=LINES {
        for unit in 1..=UNITS {
            for j in 0..FILES_PER_UNIT {
                let folder = folder_of(line, unit, j);
                let instance = spread(g, INSTANCES, TOP_FOLDER_FILES);
                let memory = !instance && spread((a * 1_009) % atomics, MEMORY_TAGS, atomics);
                let word = if instance {
                    "Motor"
                } else if memory {
                    "Sensor"
                } else {
                    "Valve"
                };
                let mut name = format!("{word}{:02}", j + 1);
                if g % 40 == 7 {
                    name.push_str(&format!(":{}", g % 9 + 1));
                }
                let path = format!("{folder}/{name}");
                let doc = format!("Line{line:02} Unit{unit:02} {word} {g:04}");
                let json = if instance {
                    let nested = spread(i, INSTANCE_STUBS, INSTANCES);
                    let has_folder = spread((i * 313) % INSTANCES, FOLDER_STUBS, INSTANCES);
                    let group = &by_kind[&(nested, has_folder)];
                    let t = &mains[group[(i * 37 + 11) % group.len()]];
                    i += 1;
                    instance_json(t, &leafs, i - 1, line, &doc)
                } else if memory {
                    let value = ((fnv(&path) ^ seed) % 50) as i64;
                    let mut tag = json!({
                        "tagType": "AtomicTag", "dataType": "Int4", "valueSource": "memory",
                        "value": value, "documentation": doc
                    });
                    if q >= MEMORY_VALUE_ONLY {
                        let differs = q < MEMORY_VALUE_ONLY + MEMORY_DEFAULT_DIFFERS;
                        tag["defaultValue"] = json!(if differs { value + 100 } else { value });
                    }
                    q += 1;
                    a += 1;
                    tag
                } else {
                    let tag = if a % 5 == 4 {
                        json!({
                            "tagType": "AtomicTag", "dataType": "Float8", "valueSource": "expr",
                            "expression": "{[.]Valve01} * 2", "documentation": doc
                        })
                    } else {
                        json!({
                            "tagType": "AtomicTag", "dataType": "Float8", "valueSource": "opc",
                            "opcServer": "Ignition OPC UA Server",
                            "opcItemPath": format!("ns=1;s=[Device]{path}"),
                            "engUnit": "degC", "engHigh": 100, "engLow": 0,
                            "historyEnabled": true, "documentation": doc
                        })
                    };
                    a += 1;
                    tag
                };
                files.push((file_of(&path), canonical_file_bytes(&json)));
                g += 1;
            }
        }
    }
    files.sort_by(|x, y| x.0.as_bytes().cmp(y.0.as_bytes()));
    files
}

/// Write `files` below `dir`, plus `deploy/project.yaml` owning [`PROVIDER`]
/// for project [`PROJECT`] in environment `dev`, and
/// `deploy/tag-providers/<P>.json` ([`PROVIDER_SETTINGS`]).
pub fn materialize_repo(dir: &Path, files: &[(String, Vec<u8>)]) {
    let put = |rel: &str, bytes: &[u8]| {
        let target = rel.split('/').fold(dir.to_path_buf(), |p, c| p.join(c));
        std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
        std::fs::write(target, bytes).expect("write");
    };
    for (rel, bytes) in files {
        put(rel, bytes);
    }
    put(
        "deploy/project.yaml",
        format!(
            "schema_version: 1\nproject: {PROJECT}\nenvironments: [dev]\ntag_providers: [{PROVIDER}]\n"
        )
        .as_bytes(),
    );
    put(
        &format!("deploy/tag-providers/{PROVIDER}.json"),
        PROVIDER_SETTINGS.as_bytes(),
    );
}

/// The nodes of `provider` the files hold, keyed by provider-relative path as
/// the native reader keys them: each node file's JSON, and a
/// `{"tagType":"Folder"}` for every folder (the structural `_types_` folder
/// excepted). Hidden entries are skipped.
pub fn nodes_from_files(files: &[(String, Vec<u8>)], provider: &str) -> BTreeMap<String, Value> {
    let prefix = format!("tags/{provider}/");
    let mut out = BTreeMap::new();
    for (rel, bytes) in files {
        let Some(rest) = rel.strip_prefix(&prefix) else {
            continue;
        };
        let segments: Vec<&str> = rest.split('/').collect();
        if segments.iter().any(|s| s.starts_with('.')) {
            continue;
        }
        let Some((last, dirs)) = segments.split_last() else {
            continue;
        };
        let Some(stem) = last.strip_suffix(".json") else {
            continue;
        };
        let mut path = String::new();
        for dir in dirs {
            if !path.is_empty() {
                path.push('/');
            }
            path.push_str(&decode_fs_name(dir));
            if path != "_types_" {
                out.entry(path.clone())
                    .or_insert_with(|| json!({"tagType": "Folder"}));
            }
        }
        if !path.is_empty() {
            path.push('/');
        }
        path.push_str(&decode_fs_name(stem));
        let mut json: Value = serde_json::from_slice(bytes).expect("a node file is JSON");
        if let Some(object) = json.as_object_mut() {
            object.remove("name");
        }
        out.insert(path, json);
    }
    out
}

fn in_types(path: &str) -> bool {
    path == "_types_" || path.starts_with("_types_/")
}

fn is_memory_atom(v: &Value) -> bool {
    v.get("tagType").and_then(Value::as_str) == Some("AtomicTag")
        && v.get("valueSource").and_then(Value::as_str) == Some("memory")
}

/// One folder of the export being built.
#[derive(Default)]
struct Level {
    own: Option<Value>,
    children: BTreeMap<String, Level>,
}

/// The gateway's REST export of `nodes` (provider-relative path to the node
/// without its name, as [`nodes_from_files`] gives them): a `Provider` root,
/// the `_types_` folder, every folder holding its children in reverse byte
/// order of their names (F-P1), and C12 applied: a memory AtomicTag outside
/// `_types_/` holding `value` and no `defaultValue` gains `defaultValue`
/// equal to its `value`.
pub fn gateway_export(nodes: &BTreeMap<String, Value>) -> Value {
    let mut root = Level::default();
    root.children.insert(
        "_types_".to_string(),
        Level {
            own: Some(json!({"tagType": "Folder"})),
            children: BTreeMap::new(),
        },
    );
    for (path, json) in nodes {
        let mut json = json.clone();
        if !in_types(path)
            && is_memory_atom(&json)
            && json.get("defaultValue").is_none()
            && let Some(value) = json.get("value").cloned()
        {
            json["defaultValue"] = value;
        }
        let mut level = &mut root;
        for segment in path.split('/') {
            level = level.children.entry(segment.to_string()).or_default();
        }
        level.own = Some(json);
    }
    fn render(name: &str, level: &Level) -> Value {
        let mut out = level
            .own
            .clone()
            .unwrap_or_else(|| json!({"tagType": "Folder"}));
        out["name"] = json!(name);
        let folder = out.get("tagType").and_then(Value::as_str) == Some("Folder");
        if folder {
            let kids: Vec<Value> = level
                .children
                .iter()
                .rev()
                .map(|(n, l)| render(n, l))
                .collect();
            out["tags"] = Value::Array(kids);
        }
        out
    }
    let kids: Vec<Value> = root
        .children
        .iter()
        .rev()
        .map(|(n, l)| render(n, l))
        .collect();
    json!({"name": "", "tagType": "Provider", "tags": kids})
}

/// What the live export differs in from the repo. Each count picks that
/// many nodes, disjoint, from the definitions of the first four lines
/// (`Area01/Line01` to `Line04`, 32 folders), in a fixed hash order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Knobs {
    /// Repo nodes the gateway lacks: the plan adds them.
    pub added: usize,
    /// Nodes whose `documentation` differs live: changed, `MergeOverwrite`.
    pub modified: usize,
    /// Nodes holding a `tooltip` live the repo lacks: changed, the repo drops
    /// a property (forward `Overwrite` with carry).
    pub dropping: usize,
    /// Nodes missing their `documentation` live: changed, the run adds a
    /// property (a rollback `Overwrite` with carry).
    pub adding: usize,
    /// Main types whose first member's default `value` differs live.
    pub types_changed: usize,
    /// Live-only memory tags no record lists: drift.
    pub drift: usize,
    /// Live-only memory tags the record lists: planned removals
    /// ([`tag_record`]).
    pub removed: usize,
}

/// A generated live export and the nodes each knob picked
/// (provider-relative paths, byte-sorted).
#[derive(Debug, Clone)]
pub struct LiveExport {
    /// The export (`GET tags/export?provider=<P>&type=json`).
    pub json: Value,
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub dropping: Vec<String>,
    pub adding: Vec<String>,
    pub types_changed: Vec<String>,
    pub drift: Vec<String>,
    pub removed: Vec<String>,
}

impl LiveExport {
    /// Every changed node: modified, dropping, adding and the changed types,
    /// byte-sorted.
    pub fn changed(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .modified
            .iter()
            .chain(&self.dropping)
            .chain(&self.adding)
            .chain(&self.types_changed)
            .cloned()
            .collect();
        out.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        out
    }
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    v
}

/// The live export of the repo `files` ([`PROVIDER`]) shaped by `knobs`, in
/// the gateway's form ([`gateway_export`]). Runtime values differ from the
/// repo on every third memory tag (the plan keeps them, D-03); a value-only
/// memory tag's `defaultValue` is still the repo's `value`, seeded at
/// creation.
pub fn live_export_from(files: &[(String, Vec<u8>)], knobs: &Knobs) -> LiveExport {
    let mut nodes = nodes_from_files(files, PROVIDER);
    let region = |p: &str| (1..=4).any(|l| p.starts_with(&format!("{TOP_FOLDER}/Line{l:02}/")));
    let mut candidates: Vec<String> = nodes
        .iter()
        .filter(|(p, v)| region(p) && v.get("tagType").and_then(Value::as_str) != Some("Folder"))
        .map(|(p, _)| p.clone())
        .collect();
    candidates.sort_by_key(|p| (mixed(p), p.clone()));
    let mut take = {
        let mut rest = candidates.into_iter();
        move |n: usize| -> Vec<String> { rest.by_ref().take(n).collect() }
    };
    let added = sorted(take(knobs.added));
    let modified = sorted(take(knobs.modified));
    let dropping = sorted(take(knobs.dropping));
    let adding = sorted(take(knobs.adding));
    assert_eq!(
        added.len() + modified.len() + dropping.len() + adding.len(),
        knobs.added + knobs.modified + knobs.dropping + knobs.adding,
        "the knobs ask for more nodes than the region holds"
    );
    let types: Vec<String> = nodes
        .keys()
        .filter(|p| p.starts_with("_types_/Motor00/") || p.starts_with("_types_/Sensor00/"))
        .take(knobs.types_changed)
        .cloned()
        .collect();

    // runtime values, before the knobs: a third of the memory tags
    for (path, node) in nodes.iter_mut() {
        if !in_types(path) && is_memory_atom(node) && fnv(path).is_multiple_of(3) {
            let Some(value) = node.get("value").and_then(Value::as_i64) else {
                continue;
            };
            if node.get("defaultValue").is_none() {
                node["defaultValue"] = json!(value);
            }
            node["value"] = json!(value + 1);
        }
    }
    for path in &added {
        nodes.remove(path);
    }
    for path in &modified {
        let doc = nodes[path]["documentation"]
            .as_str()
            .unwrap_or("")
            .to_string();
        nodes.get_mut(path).expect("picked")["documentation"] = json!(format!("{doc} live"));
    }
    for path in &dropping {
        nodes.get_mut(path).expect("picked")["tooltip"] = json!("Unit tooltip");
    }
    for path in &adding {
        if let Some(object) = nodes.get_mut(path).and_then(Value::as_object_mut) {
            object.remove("documentation");
        }
    }
    for path in &types {
        nodes.get_mut(path).expect("picked")["tags"][0]["value"] = json!(99);
    }
    let folders: Vec<String> = (1..=4)
        .flat_map(|l| (1..=UNITS).map(move |u| format!("{TOP_FOLDER}/Line{l:02}/Unit{u:02}")))
        .collect();
    let live_only = |word: &str, n: usize| -> Vec<String> {
        (0..n)
            .map(|k| format!("{}/{word}{}", folders[k % folders.len()], 900 + k))
            .collect()
    };
    let drift = sorted(live_only("Sensor", knobs.drift));
    let removed = sorted(live_only("Valve", knobs.removed));
    for path in drift.iter().chain(&removed) {
        nodes.insert(
            path.clone(),
            json!({"tagType": "AtomicTag", "dataType": "Int4", "valueSource": "memory",
                   "value": 1, "defaultValue": 1}),
        );
    }
    LiveExport {
        json: gateway_export(&nodes),
        added,
        modified,
        dropping,
        adding,
        types_changed: types,
        drift,
        removed,
    }
}

/// This project's tag record of `repo` from an earlier deploy, for the plan
/// against `live`: every repo node the gateway holds (the added ones are new)
/// and every planned removal.
pub fn tag_record(
    repo: &NativeProvider,
    live: &LiveExport,
    gateway: &str,
    project: &str,
) -> TagRecord {
    let added: BTreeSet<&String> = live.added.iter().collect();
    let nodes: Vec<String> = repo
        .nodes
        .keys()
        .filter(|p| !added.contains(p))
        .chain(&live.removed)
        .cloned()
        .collect();
    TagRecord {
        schema_version: 1,
        gateway: gateway.to_string(),
        provider: repo.name.clone(),
        project: project.to_string(),
        release: "v0.9.0".to_string(),
        run_id: "20261020T101500Z-ab12".to_string(),
        deployed_at: "2026-10-20T10:15:00Z".to_string(),
        tags_sha256: "ab".repeat(32),
        nodes: sorted(nodes),
        groups: vec!["Default".to_string()],
    }
}

/// The shape of a native export, measured from its files alone. Counts only,
/// never a name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Shape {
    /// Files below `tags/<P>/`, hidden ones included.
    pub provider_files: usize,
    /// Node files below the one top folder holding the most.
    pub top_folder_files: usize,
    /// Node files below `_types_/`.
    pub type_files: usize,
    /// AtomicTag members of instances (outside `_types_/`), at any depth.
    pub member_stubs: usize,
    /// UdtInstance members of instances.
    pub instance_stubs: usize,
    /// Folder members of instances.
    pub folder_stubs: usize,
    /// Every node in the type files, the types included.
    pub type_nodes: usize,
    /// Every node outside `_types_/`: folders, node files and their members.
    pub definition_nodes: usize,
    /// Components of the deepest node's provider-relative path.
    pub max_depth: usize,
    /// Path components holding `%3A`.
    pub colon_names: usize,
    /// Memory AtomicTag files outside `_types_/` with `value` and
    /// `defaultValue`.
    pub memory_with_default: usize,
    /// Of those, the ones whose two differ.
    pub memory_default_differs: usize,
    /// Memory AtomicTag files outside `_types_/` with `value` only.
    pub memory_value_only: usize,
    /// Bytes of the node files.
    pub bytes: usize,
}

/// Count every node below `tags` of `v` into `(atomic, instance, folder,
/// all)`.
fn count_members(v: &Value, out: &mut (usize, usize, usize, usize)) {
    for child in v
        .get("tags")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        out.3 += 1;
        match child.get("tagType").and_then(Value::as_str) {
            Some("AtomicTag") => out.0 += 1,
            Some("UdtInstance") => out.1 += 1,
            Some("Folder") => out.2 += 1,
            _ => {}
        }
        count_members(child, out);
    }
}

/// The [`Shape`] of `provider` in `files` (repo-relative paths and bytes).
pub fn shape_of(files: &[(String, Vec<u8>)], provider: &str) -> Shape {
    let prefix = format!("tags/{provider}/");
    let mut shape = Shape::default();
    let mut per_top: BTreeMap<String, usize> = BTreeMap::new();
    let mut folders: BTreeSet<String> = BTreeSet::new();
    for (rel, bytes) in files {
        let Some(rest) = rel.strip_prefix(&prefix) else {
            continue;
        };
        shape.provider_files += 1;
        let segments: Vec<&str> = rest.split('/').collect();
        if segments.iter().any(|s| s.starts_with('.')) {
            continue;
        }
        shape.colon_names += segments.iter().filter(|s| s.contains("%3A")).count();
        shape.max_depth = shape.max_depth.max(segments.len());
        shape.bytes += bytes.len();
        let Ok(json) = serde_json::from_slice::<Value>(bytes) else {
            continue;
        };
        if segments[0] == "_types_" {
            shape.type_files += 1;
            let mut members = (0, 0, 0, 0);
            count_members(&json, &mut members);
            shape.type_nodes += 1 + members.3;
            continue;
        }
        if segments.len() > 1 {
            *per_top.entry(segments[0].to_string()).or_default() += 1;
        }
        for i in 1..segments.len() {
            folders.insert(segments[..i].join("/"));
        }
        let mut members = (0, 0, 0, 0);
        if json.get("tagType").and_then(Value::as_str) == Some("UdtInstance") {
            count_members(&json, &mut members);
            shape.member_stubs += members.0;
            shape.instance_stubs += members.1;
            shape.folder_stubs += members.2;
        }
        shape.definition_nodes += 1 + members.3;
        if is_memory_atom(&json) && json.get("value").is_some() {
            match json.get("defaultValue") {
                Some(default) => {
                    shape.memory_with_default += 1;
                    if Some(default) != json.get("value") {
                        shape.memory_default_differs += 1;
                    }
                }
                None => shape.memory_value_only += 1,
            }
        }
    }
    shape.definition_nodes += folders.len();
    shape.top_folder_files = per_top.values().copied().max().unwrap_or(0);
    shape
}

/// Every name below `provider` in `files`: decoded path segments (without
/// `.json`), every `name` in a node file, every parameter name, every
/// `typeId` segment and every group name of `.tag-groups.json`. For the
/// neutral-name allowlist test only.
pub fn names_of(files: &[(String, Vec<u8>)], provider: &str) -> BTreeSet<String> {
    fn walk(v: &Value, out: &mut BTreeSet<String>) {
        match v {
            Value::Object(object) => {
                for (key, child) in object {
                    match (key.as_str(), child) {
                        ("name", Value::String(name)) => {
                            out.insert(name.clone());
                        }
                        ("typeId", Value::String(id)) => {
                            out.extend(id.split('/').map(str::to_string));
                        }
                        ("parameters", Value::Object(params)) => {
                            out.extend(params.keys().cloned());
                        }
                        _ => {}
                    }
                    walk(child, out);
                }
            }
            Value::Array(items) => items.iter().for_each(|i| walk(i, out)),
            _ => {}
        }
    }
    let prefix = format!("tags/{provider}/");
    let mut out = BTreeSet::new();
    for (rel, bytes) in files {
        let Some(rest) = rel.strip_prefix(&prefix) else {
            continue;
        };
        let segments: Vec<&str> = rest.split('/').collect();
        let hidden = segments.iter().any(|s| s.starts_with('.'));
        if !hidden {
            for segment in &segments {
                let segment = segment.strip_suffix(".json").unwrap_or(segment);
                out.insert(decode_fs_name(segment));
            }
        }
        if let Ok(json) = serde_json::from_slice::<Value>(bytes) {
            walk(&json, &mut out);
        }
    }
    out
}
