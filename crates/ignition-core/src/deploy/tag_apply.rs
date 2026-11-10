//! Import batches of project-owned tags (Phase 26, 26-08, TAGDEP-05).
//!
//! [`import_batches`] turns one owned provider's plan into the ordered
//! `tags/import` requests a run sends, in the git module's order (research
//! Q1): UDT types first, one per request in dependency order
//! ([`type_order`]), each wrapped in the `_types_` folder chain at the
//! provider root, then the other tags grouped by their deepest existing
//! parent path. Pure: the run sends the batches ([`ImportBatch`]), this module
//! never reaches a gateway.
//!
//! ## The write design (the 26-08 checkpoint, decided 2026-11-09)
//!
//! TAGDEP-00 verdict `rest-writes-no-delete` (26-LIVE-CAPTURES.md `## Write
//! design from TAGDEP-00`). The user chose the delete primitive
//! `webdev-replan` (the authenticated WebDev delete design, re-planned for
//! the local rigs only after this plan; nothing WebDev is built here) and the
//! removed-property policy `overwrite-carry`. So, per write kind:
//!
//! - **New node or instance:** `Abort`, the node as the repo export holds it,
//!   values included (D-10; C1, C7 seed, p26-create). Added children travel
//!   nested in their head.
//! - **Changed existing node:** `MergeOverwrite`, passed through
//!   [`strip_runtime_values`]: AtomicTag `value`, `timestamp` and `quality`
//!   only; `defaultValue`, instance `parameters` and member override
//!   configuration are sent (R3-POL; C2 and p26-change keep the live value,
//!   C7 I3 keeps instance parameters, member overrides and member values).
//! - **Changed node whose repo version drops a property the live node has:**
//!   `MergeOverwrite` cannot remove a property (C6 Q), so under
//!   [`REMOVED_PROPERTY_POLICY`] it goes `Overwrite` with `carry`: the run
//!   reads a fresh export right before the batch and [`carry_live_values`]
//!   puts each memory-sourced live value back into the body (R3-CARRY; C6 P,
//!   p26-rollback-shape V2 and I2). A runtime write between that export and
//!   the import is overwritten (R3-RACE, accepted at the checkpoint).
//! - **UDT type, new or changed:** `Abort` or `Overwrite` of the whole type,
//!   never stripped (below `_types_/` a member's `value` is the type's
//!   default). A changed type is not captured (Q5); 26-15 captures it.
//! - **Removed node:** none. No REST import deletes an omitted child (C4, C5),
//!   so a removal is refused before any write (`tag_removal`, 26-07, R3-DEL).
//!
//! No body [`import_batches`] builds exceeds [`IMPORT_BATCH_MAX_BYTES`]
//! (R3-BATCH; C9): a new folder over it goes alone, then its children below
//! it (C1d), and a single node over it is refused before any write. A body only ever names nodes the plan lists, plus the
//! folders on the way to a new type: a live-only node is never sent (D-01).
//!
//! ## The rollback (26-11)
//!
//! [`rollback_plan`] draws the undo of a partial or failed run from the
//! saved pre-image (R3-RB): changed types back whole, changed tags back by
//! `MergeOverwrite` of the stripped pre-image, or by a carried `Overwrite`
//! when the run added a property. A created node cannot be deleted (R3-DEL)
//! and is reported. [`carry_guard`] refuses any carried `Overwrite` that
//! would reset a live value whose `valueSource` does not resolve. A tag with
//! no `valueSource` key resolves to `memory`, Ignition's default (26-15 O1).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use super::native_tags::{NativeNode, NativeProvider, NodeKind, TYPES_DIR};
use super::tag_canon::{
    CanonOpts, added_properties, diff_properties, in_types, strip_runtime_values,
};
use super::tag_delete::{RouteHandshake, delete_chunks};
use super::tag_plan::{ProviderPlan, parent_of};
use crate::client::tags::CollisionPolicy;
use crate::error::{CoreError, DeployFinding};

/// The largest import body a forward batch holds (R3-BATCH): 512 KiB. C9 measured one `Abort` import of 271,001 bytes
/// (3,000 memory tags) answered in 9.7 s; 512 KiB itself is not measured
/// (about 19 s by linear scaling), and 26-15 times one batch at the cap on
/// the rig before the first production run.
pub const IMPORT_BATCH_MAX_BYTES: usize = 524_288;

/// How a changed existing node is written when `MergeOverwrite` cannot
/// express the change (the repo drops a property the live node has).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExistingNodePolicy {
    /// `MergeOverwrite`, stripped (R3-POL). A dropped property cannot be
    /// applied this way, so a plan holding one is refused before any write
    /// (the `merge-default` choice, which no capture supports).
    MergeStripped,
    /// `Overwrite` with the memory-sourced live values carried from a fresh
    /// export (R3-CARRY, choice `overwrite-carry`). 26-11's rollback (R3-RB)
    /// uses it too.
    OverwriteCarry,
}

impl ExistingNodePolicy {
    /// The collision policy the gateway is sent.
    pub fn collision_policy(self) -> CollisionPolicy {
        match self {
            ExistingNodePolicy::MergeStripped => CollisionPolicy::MergeOverwrite,
            ExistingNodePolicy::OverwriteCarry => CollisionPolicy::Overwrite,
        }
    }
}

/// The removed-property policy the user chose at the 26-08 checkpoint
/// (`overwrite-carry`).
pub const REMOVED_PROPERTY_POLICY: ExistingNodePolicy = ExistingNodePolicy::OverwriteCarry;

/// Whether one tagDelete call removes the nodes below the node it names, by
/// kind (26-19). [`rollback_plan`] takes it as a parameter so both modes are
/// tested; the run passes [`TAG_DELETE_DESCENDANTS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeleteDescendants {
    /// A folder delete removes the folder's children.
    pub folders: bool,
    /// A UDT instance delete removes the instance's members.
    pub instances: bool,
}

/// What 8.3.6 does, from `26-LIVE-CAPTURES.md` `## Tag delete route (8.3.6)`
/// (26-17, single-node loopback rig, the folder row `Fold` with children
/// `X`, `Y` and the instance row `Inst` with members `M1`, `M2`):
///
/// ```text
/// folder delete removes children: yes
/// instance delete removes members: yes
/// ```
pub const TAG_DELETE_DESCENDANTS: DeleteDescendants = DeleteDescendants {
    folders: true,
    instances: true,
};

/// One `tags/import` request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportBatch {
    /// The provider.
    pub provider: String,
    /// The provider-relative path the body's nodes go below (empty at the
    /// provider root).
    pub path: String,
    /// The collision policy.
    pub policy: CollisionPolicy,
    /// `{"tags":[...]}`, compact JSON. A `carry` body is stripped: the run
    /// fills the values at send time ([`carried`]).
    pub body: Vec<u8>,
    /// Every planned node path the body carries, byte-sorted.
    pub nodes: Vec<String>,
    /// Whether the run must read a fresh export right before the batch and
    /// carry the live values into it (R3-CARRY).
    pub carry: bool,
}

/// What [`carried`] made of a `carry` batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Carried {
    /// The body to send, values carried.
    Body(Vec<u8>),
    /// With the values carried the body is over the cap: send these smaller
    /// batches instead, each with a fresh export of its own.
    Split(Vec<ImportBatch>),
}

/// `{"tags":[` and `]}`.
const BODY_FRAME: usize = 11;

/// The deepest a typeId chain or a nested instance is followed.
const MAX_DEPTH: usize = 32;

fn tag_type(v: &Value) -> Option<&str> {
    v.get("tagType").and_then(Value::as_str)
}

fn name_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}/{name}")
    }
}

/// `json` with its `name` set to `name`.
fn named(json: &Value, name: &str) -> Value {
    let mut out = json.clone();
    if let Some(object) = out.as_object_mut() {
        object.insert("name".to_string(), Value::String(name.to_string()));
    }
    out
}

/// The child of `v` named `name` in its `tags` array.
fn child<'a>(v: &'a Value, name: &str) -> Option<&'a Value> {
    v.get("tags")?
        .as_array()?
        .iter()
        .find(|c| c.get("name").and_then(Value::as_str) == Some(name))
}

/// Every `typeId` in `v` and below it in `tags`.
fn type_ids<'a>(v: &'a Value, out: &mut BTreeSet<&'a str>) {
    if let Some(id) = v.get("typeId").and_then(Value::as_str) {
        out.insert(id);
    }
    if let Some(children) = v.get("tags").and_then(Value::as_array) {
        for c in children {
            type_ids(c, out);
        }
    }
}

/// The type paths (`_types_/<typeId>`) that `v` names, by its own `typeId`
/// and every `typeId` below it in `tags`: the one typeId resolution
/// ([`type_order`], [`delete_calls`], 26-20's
/// [`super::tag_plan::removed_types_in_use`]).
pub(crate) fn type_refs(v: &Value) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    type_ids(v, &mut ids);
    ids.into_iter()
        .map(|id| format!("{TYPES_DIR}/{id}"))
        .collect()
}

/// The first of `removed_types` that a node of `survivors` uses
/// ([`type_refs`]), as `(type path, referrer path)`, the survivors read in
/// the order given (26-20, T-26-82). A survivor that is itself one of the
/// removed types is not a referrer.
pub(crate) fn type_in_use<'a>(
    removed_types: &BTreeSet<&str>,
    survivors: impl IntoIterator<Item = (&'a str, &'a Value)>,
) -> Option<(String, String)> {
    if removed_types.is_empty() {
        return None;
    }
    for (path, node) in survivors {
        if removed_types.contains(path) {
            continue;
        }
        if let Some(used) = type_refs(node)
            .into_iter()
            .find(|t| removed_types.contains(t.as_str()))
        {
            return Some((used, path.to_string()));
        }
    }
    None
}

/// `types` (UDT type nodes, paths below `_types_/`) in import order: a type
/// whose own `typeId` (its parent type) or any `typeId` below it in `tags`
/// (an instance member) names another type of the set comes after that type.
/// Otherwise byte order of the path. A `typeId` is the type's path below
/// `_types_/`; one naming a type outside the set orders nothing.
///
/// The semantics of the git module's `UdtDependencyResolver.sortByDependencies`
/// (a depth-first topological sort), except that a cycle, which the module
/// logs and imports anyway, is `deploy_tree_invalid` naming the cycle's
/// paths.
pub fn type_order(types: &[&NativeNode]) -> Result<Vec<String>, CoreError> {
    let by_path: BTreeMap<&str, &NativeNode> = types
        .iter()
        .map(|node| (node.path.as_str(), *node))
        .collect();
    let mut deps: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for (path, node) in &by_path {
        let mut list: Vec<String> = type_refs(&node.json)
            .into_iter()
            .filter(|dep| by_path.contains_key(dep.as_str()))
            .collect();
        list.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        deps.insert(path, list);
    }

    fn visit(
        path: &str,
        deps: &BTreeMap<&str, Vec<String>>,
        done: &mut BTreeSet<String>,
        stack: &mut Vec<String>,
        out: &mut Vec<String>,
    ) -> Result<(), Vec<String>> {
        if done.contains(path) {
            return Ok(());
        }
        if let Some(at) = stack.iter().position(|p| p == path) {
            return Err(stack[at..].to_vec());
        }
        stack.push(path.to_string());
        for dep in deps.get(path).into_iter().flatten() {
            visit(dep, deps, done, stack, out)?;
        }
        stack.pop();
        done.insert(path.to_string());
        out.push(path.to_string());
        Ok(())
    }

    let mut done = BTreeSet::new();
    let mut out = Vec::new();
    for path in by_path.keys() {
        let mut stack = Vec::new();
        if let Err(cycle) = visit(path, &deps, &mut done, &mut stack, &mut out) {
            let file = by_path
                .get(cycle[0].as_str())
                .map_or_else(|| cycle[0].clone(), |node| node.file.clone());
            return Err(CoreError::deploy_tree_invalid(
                file,
                None,
                None,
                format!(
                    "the UDT types {} reference each other in a cycle (typeId), so no import \
                     order creates each type after the types it uses; nothing was written",
                    cycle.join(" -> ")
                ),
            ));
        }
    }
    Ok(out)
}

/// The type definition `id` names (`_types_/<id>` in `fresh`), then its
/// parent types through their own `typeId`.
fn type_chain<'a>(id: Option<&str>, fresh: &'a BTreeMap<String, Value>) -> Vec<&'a Value> {
    let mut chain = Vec::new();
    let mut next = id.map(str::to_string);
    while let Some(id) = next {
        if chain.len() >= MAX_DEPTH {
            break;
        }
        let Some(def) = fresh.get(&format!("{TYPES_DIR}/{id}")) else {
            break;
        };
        chain.push(def);
        next = def
            .get("typeId")
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    chain
}

/// Put the memory-sourced live values of the fresh export into `node`, the
/// node at provider-relative `path` about to be sent with `Overwrite`
/// (R3-CARRY; the one carry implementation, 26-11 and 26-12 use it too).
///
/// `fresh` is the provider's export read right before the batch, cut by
/// [`super::tag_canon::flatten_export`]. For every AtomicTag in `node` (the
/// node itself and every instance member, through member folders and nested
/// instances): `timestamp` and `quality` are removed; `value` is set to the
/// value the export holds at the same place when the effective `valueSource`
/// is `memory` (the tag's own, or for an instance member the type member's,
/// resolved from the export's `_types_/` and the type's parents, and
/// `memory` when none of them holds the key: [`effective_value_source`]),
/// and removed otherwise. No `value` is added where the export holds none.
/// KL-5: only memory values are proven to survive a carried `Overwrite`, so
/// any other value source, and one that is not a name, is sent stripped.
pub fn carry_live_values(node: &mut Value, path: &str, fresh: &BTreeMap<String, Value>) {
    carry_at(node, fresh.get(path), &[], fresh, 0);
}

/// Ignition's default value source, and the only one a carry writes back.
const MEMORY_SOURCE: &str = "memory";

/// The effective `valueSource` of the AtomicTag `node` (R3-CARRY, 26-15 O1):
/// the key of the first of `node` and then `defs` (its type members, the
/// nearest first) that holds one. When none holds the key the tag is a
/// memory tag, Ignition's default: 8.3.6 exports a memory tag created without
/// the key without one (26-LIVE-CAPTURES, `valueSource in exports`). `None`
/// when the key that applies is not a string (a binding, say): no source
/// resolves, and the carry guard fails closed on it.
fn effective_value_source<'a>(node: &'a Value, defs: &[&'a Value]) -> Option<&'a str> {
    std::iter::once(node)
        .chain(defs.iter().copied())
        .find_map(|d| d.get("valueSource"))
        .map_or(Some(MEMORY_SOURCE), Value::as_str)
}

fn carry_at(
    node: &mut Value,
    live: Option<&Value>,
    defs: &[&Value],
    fresh: &BTreeMap<String, Value>,
    depth: usize,
) {
    if depth > MAX_DEPTH {
        return;
    }
    match tag_type(node) {
        Some("AtomicTag") => {
            let memory = effective_value_source(node, defs) == Some(MEMORY_SOURCE);
            let carried = live
                .and_then(|l| l.get("value"))
                .filter(|_| memory)
                .cloned();
            if let Some(object) = node.as_object_mut() {
                for key in ["value", "timestamp", "quality"] {
                    object.remove(key);
                }
                if let Some(value) = carried {
                    object.insert("value".to_string(), value);
                }
            }
        }
        Some("UdtInstance") => {
            let id = node
                .get("typeId")
                .and_then(Value::as_str)
                .or_else(|| {
                    defs.iter()
                        .find_map(|d| d.get("typeId").and_then(Value::as_str))
                })
                .map(str::to_string);
            let mut scope: Vec<&Value> = defs.to_vec();
            scope.extend(type_chain(id.as_deref(), fresh));
            carry_children(node, live, &scope, fresh, depth);
        }
        Some("Folder") => carry_children(node, live, defs, fresh, depth),
        _ => {}
    }
}

fn carry_children(
    node: &mut Value,
    live: Option<&Value>,
    defs: &[&Value],
    fresh: &BTreeMap<String, Value>,
    depth: usize,
) {
    let Some(children) = node.get_mut("tags").and_then(Value::as_array_mut) else {
        return;
    };
    for c in children {
        let Some(name) = c.get("name").and_then(Value::as_str).map(str::to_string) else {
            continue;
        };
        let live_child = live.and_then(|l| child(l, &name));
        let child_defs: Vec<&Value> = defs.iter().filter_map(|d| child(d, &name)).collect();
        carry_at(c, live_child, &child_defs, fresh, depth + 1);
    }
}

/// One node of a body, with the size it counts for and the planned paths it
/// carries.
struct Item {
    value: Value,
    size: usize,
    paths: Vec<String>,
}

fn json_len(v: &Value) -> usize {
    serde_json::to_vec(v).map_or(0, |bytes| bytes.len())
}

fn body_bytes(values: Vec<Value>) -> Result<Vec<u8>, CoreError> {
    serde_json::to_vec(&json!({ "tags": values }))
        .map_err(|e| CoreError::Internal(format!("could not serialize a tag import: {e}")))
}

/// Pack `items` below `path` into batches no larger than the cap, a single
/// node excepted, in their order.
fn pack(
    provider: &str,
    path: &str,
    policy: CollisionPolicy,
    carry: bool,
    items: Vec<Item>,
    out: &mut Vec<ImportBatch>,
) -> Result<(), CoreError> {
    let mut current: Vec<Item> = Vec::new();
    let mut size = BODY_FRAME;
    let mut flush = |current: &mut Vec<Item>| -> Result<(), CoreError> {
        if current.is_empty() {
            return Ok(());
        }
        let mut nodes: Vec<String> = current.iter().flat_map(|i| i.paths.clone()).collect();
        nodes.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        let values = current.drain(..).map(|i| i.value).collect();
        out.push(ImportBatch {
            provider: provider.to_string(),
            path: path.to_string(),
            policy,
            body: body_bytes(values)?,
            nodes,
            carry,
        });
        Ok(())
    };
    for item in items {
        let add = item.size + usize::from(!current.is_empty());
        if !current.is_empty() && size + add > IMPORT_BATCH_MAX_BYTES {
            flush(&mut current)?;
            size = BODY_FRAME;
        }
        size += item.size + usize::from(!current.is_empty());
        current.push(item);
    }
    flush(&mut current)
}

/// The import batches of one owned provider's `plan`, drawn against the
/// repo's nodes `repo` and the live nodes `live` the plan was drawn from, in
/// send order: types (module doc), then the changed tags whose tagType
/// changes and that hold an added node below them, then the added tags
/// (`Abort`), then the other changed tags (`MergeOverwrite`, stripped), then
/// the changed tags that drop a property (`Overwrite`, `carry`), each group
/// by parent path in byte order. `policy` is what a dropped property becomes
/// ([`ExistingNodePolicy`]); under [`ExistingNodePolicy::MergeStripped`] a
/// plan holding one is `deploy_tree_invalid` naming each node and property,
/// before anything is built. A carried batch is packed by its size with the
/// plan-time live values carried. A new folder whose nested body is over
/// [`IMPORT_BATCH_MAX_BYTES`] goes alone at its parent path and its children
/// are batched below it, recursively; a tag, instance, changed node or type
/// that alone makes a body over the cap is `deploy_tree_invalid` naming each
/// with its size.
pub fn import_batches(
    plan: &ProviderPlan,
    repo: &NativeProvider,
    live: &BTreeMap<String, Value>,
    policy: ExistingNodePolicy,
) -> Result<Vec<ImportBatch>, CoreError> {
    let provider = plan.provider.as_str();
    let missing = |path: &str| {
        CoreError::Internal(format!(
            "the tag plan of provider {provider} names {path}, which the repo or the live \
             export does not hold"
        ))
    };
    let added: BTreeSet<&str> = plan.added.iter().map(String::as_str).collect();

    // A dropped property first: a refusal comes before anything is built.
    let mut carried_paths: BTreeSet<&str> = BTreeSet::new();
    let mut refused = Vec::new();
    for change in &plan.changed {
        let path = change.path.as_str();
        let node = repo.nodes.get(path).ok_or_else(|| missing(path))?;
        let live_node = live.get(path).ok_or_else(|| missing(path))?;
        if in_types(path) {
            // Only a type has an import below `_types_/` (ultrareview B2):
            // a changed folder there would go out in no batch.
            if node.kind != NodeKind::Type {
                refused.push(DeployFinding {
                    file: node.file.clone(),
                    line: None,
                    column: None,
                    message: format!(
                        "[{provider}]{path} is a {} in the repo and a {} on the gateway; this \
                         build cannot change a node below {TYPES_DIR}/ that is not a UDT type, \
                         so nothing was written",
                        tag_type(&node.json).unwrap_or("node"),
                        tag_type(live_node).unwrap_or("node"),
                    ),
                });
            }
            continue;
        }
        let dropped = added_properties(&node.json, live_node, path);
        if dropped.is_empty() {
            continue;
        }
        match policy {
            ExistingNodePolicy::OverwriteCarry => {
                carried_paths.insert(path);
            }
            ExistingNodePolicy::MergeStripped => refused.push(DeployFinding {
                file: node.file.clone(),
                line: None,
                column: None,
                message: format!(
                    "[{provider}]{path} drops propert(ies) the live tag has ({}); MergeOverwrite \
                     cannot remove a property and this build's removed-property policy refuses \
                     it, so nothing was written",
                    dropped.join(", ")
                ),
            }),
        }
    }
    if !refused.is_empty() {
        return Err(CoreError::deploy_findings(refused));
    }

    let mut out = Vec::new();
    // Nodes that alone make a body over the cap: refused once all are found.
    let mut oversized: Vec<DeployFinding> = Vec::new();

    // Types: one per batch, dependency order, wrapped in the `_types_` chain.
    let mut type_nodes: Vec<&NativeNode> = Vec::new();
    for path in plan
        .added
        .iter()
        .chain(plan.changed.iter().map(|c| &c.path))
    {
        let node = repo.nodes.get(path).ok_or_else(|| missing(path))?;
        if in_types(path) && node.kind == NodeKind::Type {
            type_nodes.push(node);
        }
    }
    let mut covered: BTreeSet<String> = BTreeSet::new();
    for path in type_order(&type_nodes)? {
        let node = &repo.nodes[&path];
        let rest = path
            .strip_prefix(&format!("{TYPES_DIR}/"))
            .ok_or_else(|| missing(&path))?;
        let segments: Vec<&str> = rest.split('/').collect();
        let mut nodes = vec![path.clone()];
        let mut inner = named(&node.json, name_of(&path));
        for i in (0..segments.len().saturating_sub(1)).rev() {
            let folder = format!("{TYPES_DIR}/{}", segments[..=i].join("/"));
            if added.contains(folder.as_str()) {
                nodes.push(folder.clone());
                covered.insert(folder);
            }
            inner = json!({"name": segments[i], "tagType": "Folder", "tags": [inner]});
        }
        let wrapper = json!({"name": TYPES_DIR, "tagType": "Folder", "tags": [inner]});
        nodes.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        let body = body_bytes(vec![wrapper])?;
        if body.len() > IMPORT_BATCH_MAX_BYTES {
            oversized.push(over_cap(provider, &path, &node.file, body.len()));
            continue;
        }
        out.push(ImportBatch {
            provider: provider.to_string(),
            path: String::new(),
            policy: if added.contains(path.as_str()) {
                CollisionPolicy::Abort
            } else {
                CollisionPolicy::Overwrite
            },
            body,
            nodes,
            carry: false,
        });
    }

    // A changed node whose tagType changes and that has an added node below
    // it goes before the added tags (ultrareview B3): an added child of a
    // tag that becomes a folder needs the folder there first.
    let (early, late): (Vec<&str>, Vec<&str>) = plan
        .changed
        .iter()
        .map(|c| c.path.as_str())
        .filter(|p| !in_types(p))
        .partition(|path| {
            repo.nodes.get(*path).map(|n| tag_type(&n.json)) != live.get(*path).map(tag_type)
                && added.iter().any(|a| is_below(a, path))
        });
    changed_batches(
        provider,
        &early,
        repo,
        live,
        &carried_paths,
        &mut oversized,
        &mut out,
    )?;

    // Added tags: heads at their parent path, added children nested. A new
    // folder below `_types_/` that no type batch created goes here too. A
    // new folder whose nested body is over the cap goes alone at its parent
    // path, and its children become heads below it (C1d), recursively; the
    // queue is drained in byte order, so a parent's batches come first.
    let tag_added: BTreeSet<&str> = plan
        .added
        .iter()
        .map(String::as_str)
        .filter(|path| {
            !in_types(path)
                || (repo
                    .nodes
                    .get(*path)
                    .is_some_and(|n| n.kind == NodeKind::Folder)
                    && !covered.contains(*path))
        })
        .collect();
    let mut kids: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut heads: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for path in &tag_added {
        match parent_of(path) {
            Some(parent) if tag_added.contains(parent) => {
                kids.entry(parent).or_default().push(path);
            }
            parent => heads.entry(parent.unwrap_or("")).or_default().push(path),
        }
    }
    fn nested(
        repo: &NativeProvider,
        path: &str,
        kids: &BTreeMap<&str, Vec<&str>>,
        paths: &mut Vec<String>,
    ) -> Value {
        let node = &repo.nodes[path];
        let mut value = named(&node.json, name_of(path));
        paths.push(path.to_string());
        if node.kind == NodeKind::Folder
            && let Some(children) = kids.get(path)
            && let Some(object) = value.as_object_mut()
        {
            let nested: Vec<Value> = children
                .iter()
                .map(|c| nested(repo, c, kids, paths))
                .collect();
            object.insert("tags".to_string(), Value::Array(nested));
        }
        value
    }
    let mut queue: BTreeMap<String, Vec<&str>> = heads
        .into_iter()
        .map(|(parent, list)| (parent.to_string(), list))
        .collect();
    while let Some((parent, list)) = queue.pop_first() {
        let mut items = Vec::new();
        for head in list {
            let node = &repo.nodes[head];
            let mut paths = Vec::new();
            let value = nested(repo, head, &kids, &mut paths);
            let size = json_len(&value);
            if size + BODY_FRAME <= IMPORT_BATCH_MAX_BYTES {
                items.push(Item { value, size, paths });
                continue;
            }
            let children = kids.get(head).filter(|_| node.kind == NodeKind::Folder);
            let Some(children) = children else {
                oversized.push(over_cap(provider, head, &node.file, size + BODY_FRAME));
                continue;
            };
            let alone = named(&node.json, name_of(head));
            let size = json_len(&alone);
            if size + BODY_FRAME > IMPORT_BATCH_MAX_BYTES {
                oversized.push(over_cap(provider, head, &node.file, size + BODY_FRAME));
            } else {
                items.push(Item {
                    value: alone,
                    size,
                    paths: vec![head.to_string()],
                });
            }
            queue.insert(head.to_string(), children.clone());
        }
        pack(
            provider,
            &parent,
            CollisionPolicy::Abort,
            false,
            items,
            &mut out,
        )?;
    }

    // Changed tags: merged (stripped), then carried.
    changed_batches(
        provider,
        &late,
        repo,
        live,
        &carried_paths,
        &mut oversized,
        &mut out,
    )?;
    if !oversized.is_empty() {
        return Err(CoreError::deploy_findings(oversized));
    }
    Ok(out)
}

/// The batches of the changed tags `paths` of [`import_batches`]: merged
/// (`MergeOverwrite`, stripped), then carried (`Overwrite`, `carry`) for
/// those in `carried_paths`, each group by parent path in byte order. A node
/// that alone makes a body over the cap goes to `oversized`.
fn changed_batches(
    provider: &str,
    paths: &[&str],
    repo: &NativeProvider,
    live: &BTreeMap<String, Value>,
    carried_paths: &BTreeSet<&str>,
    oversized: &mut Vec<DeployFinding>,
    out: &mut Vec<ImportBatch>,
) -> Result<(), CoreError> {
    for carry in [false, true] {
        let mut groups: BTreeMap<&str, Vec<Item>> = BTreeMap::new();
        let mut changed: Vec<&str> = paths
            .iter()
            .copied()
            .filter(|p| carried_paths.contains(p) == carry)
            .collect();
        changed.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        for path in changed {
            let node = repo.nodes.get(path).ok_or_else(|| {
                CoreError::Internal(format!(
                    "the tag plan of provider {provider} names {path}, which the repo does not \
                     hold"
                ))
            })?;
            let mut value = named(&node.json, name_of(path));
            strip_runtime_values(&mut value, path);
            let size = if carry {
                let mut measured = value.clone();
                carry_live_values(&mut measured, path, live);
                json_len(&measured)
            } else {
                json_len(&value)
            };
            if size + BODY_FRAME > IMPORT_BATCH_MAX_BYTES {
                oversized.push(over_cap(provider, path, &node.file, size + BODY_FRAME));
                continue;
            }
            groups
                .entry(parent_of(path).unwrap_or(""))
                .or_default()
                .push(Item {
                    value,
                    size,
                    paths: vec![path.to_string()],
                });
        }
        let policy = if carry {
            ExistingNodePolicy::OverwriteCarry
        } else {
            ExistingNodePolicy::MergeStripped
        }
        .collision_policy();
        for (parent, items) in groups {
            pack(provider, parent, policy, carry, items, out)?;
        }
    }
    Ok(())
}

/// The finding of a node at `path` that alone makes an import body of `size`
/// bytes, over the cap (R3-BATCH): a tag, an instance or a type cannot be
/// split across imports.
fn over_cap(provider: &str, path: &str, file: &str, size: usize) -> DeployFinding {
    DeployFinding {
        file: file.to_string(),
        line: None,
        column: None,
        message: format!(
            "[{provider}]{path} is {size} bytes as one import body, over the \
             {IMPORT_BATCH_MAX_BYTES}-byte cap (R3-BATCH); a single tag, UDT instance or type \
             cannot be split across imports, so nothing was written"
        ),
    }
}

/// The body of the `carry` batch `batch` with the live values of `fresh` (the
/// provider's export read right before it, flattened) carried into every node
/// ([`carry_live_values`]). Over the cap with more than one node, the batch is
/// split in two stripped halves instead, each to be sent with a fresh export
/// of its own. One node over the cap with its values carried is
/// `deploy_tree_invalid` naming it and its size (26-REVIEW WR-03): it is
/// never sent over the cap.
pub fn carried(batch: &ImportBatch, fresh: &BTreeMap<String, Value>) -> Result<Carried, CoreError> {
    let body: Value = serde_json::from_slice(&batch.body)
        .map_err(|e| CoreError::Internal(format!("a tag import body is not JSON: {e}")))?;
    let stripped: Vec<Value> = body
        .get("tags")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut values = stripped.clone();
    for value in &mut values {
        let name = value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        carry_live_values(value, &join(&batch.path, &name), fresh);
    }
    let bytes = body_bytes(values)?;
    if bytes.len() <= IMPORT_BATCH_MAX_BYTES {
        return Ok(Carried::Body(bytes));
    }
    if stripped.len() < 2 {
        let path = batch
            .nodes
            .first()
            .cloned()
            .unwrap_or_else(|| batch.path.clone());
        let full = format!("[{}]{path}", batch.provider);
        return Err(CoreError::deploy_findings(vec![over_cap(
            &batch.provider,
            &path,
            &full,
            bytes.len(),
        )]));
    }
    let (left, right) = stripped.split_at(stripped.len() / 2);
    let half = |part: &[Value]| -> Result<ImportBatch, CoreError> {
        let mut nodes: Vec<String> = part
            .iter()
            .filter_map(|v| v.get("name").and_then(Value::as_str))
            .map(|name| join(&batch.path, name))
            .collect();
        nodes.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        Ok(ImportBatch {
            body: body_bytes(part.to_vec())?,
            nodes,
            ..batch.clone()
        })
    };
    Ok(Carried::Split(vec![half(left)?, half(right)?]))
}

/// The nodes of `plan` whose rollback would send one import body over
/// [`IMPORT_BATCH_MAX_BYTES`] (26-REVIEW WR-03), measured on the pre-image
/// export `pre_image` (flattened) as [`rollback_plan`] builds them: a changed
/// type whole in its folder chain, a changed tag stripped (with its memory
/// values carried when its repo node in `repo` adds a property, so the
/// rollback would go `Overwrite`), a removed type in its chain, a removed tag
/// or instance with its subtree and values ([`recreated_node`]), a removed
/// folder alone. Each is a finding naming the node and its size; the run
/// refuses them before any write, since it could not undo them.
pub fn rollback_over_cap(
    plan: &ProviderPlan,
    repo: &NativeProvider,
    pre_image: &BTreeMap<String, Value>,
) -> Vec<DeployFinding> {
    let provider = plan.provider.as_str();
    let mut out = Vec::new();
    let finding = |path: &str, size: usize| DeployFinding {
        file: format!("[{provider}]{path}"),
        line: None,
        column: None,
        message: format!(
            "[{provider}]{path} is {size} bytes as the one import body its rollback would send \
             (its pre-image), over the {IMPORT_BATCH_MAX_BYTES}-byte cap (R3-BATCH), so the run \
             could not undo it; nothing was written"
        ),
    };
    let chain_size = |path: &str, node: &Value| {
        body_bytes(vec![in_types_chain(
            path,
            named(node, name_of(path)),
            pre_image,
        )])
        .map_or(0, |b| b.len())
    };
    let removed: BTreeSet<&str> = plan.removed.iter().map(String::as_str).collect();
    let changed = plan.changed.iter().map(|c| c.path.as_str());
    for path in changed.chain(removed.iter().copied()) {
        let Some(node) = pre_image.get(path) else {
            continue;
        };
        let size = match tag_type(node) {
            Some("UdtType") => chain_size(path, node),
            Some("Folder") => json_len(&named(node, name_of(path))) + BODY_FRAME,
            _ if in_types(path) => chain_size(path, node),
            _ if removed.contains(path) => {
                let mut paths = Vec::new();
                json_len(&recreated_node(path, pre_image, &mut paths, 0)) + BODY_FRAME
            }
            _ => {
                let mut value = named(node, name_of(path));
                strip_runtime_values(&mut value, path);
                let adds = repo
                    .nodes
                    .get(path)
                    .is_none_or(|sent| !added_properties(node, &sent.json, path).is_empty());
                if adds {
                    carry_live_values(&mut value, path, pre_image);
                }
                json_len(&value) + BODY_FRAME
            }
        };
        if size > IMPORT_BATCH_MAX_BYTES {
            out.push(finding(path, size));
        }
    }
    out
}

// -- carry guard (26-11, 26-08 open point 3, KL-5) ---------------------------

/// The AtomicTags of `node` (the node at provider-relative `path` that a
/// carried `Overwrite` would send) whose live counterpart in `live` (a
/// flattened export) holds a `value` that [`carry_live_values`] cannot carry
/// because no `valueSource` resolves for it: the key that applies (the tag's
/// in `node`, else for an instance member its type member's, resolved from
/// `live`'s `_types_`) is not a string. Such a value would be reset by the
/// `Overwrite`. The walk and the resolution are the carry's own, so this is
/// exactly the set of live values a carry would drop. Paths are `path` or
/// `path/<member>`, never a value. A tag whose source resolves to anything
/// (memory or not) passes: a memory value is carried, any other comes from
/// its source (KL-5). A tag where no level holds the key is memory (26-15
/// O1) and passes.
pub fn uncarriable(path: &str, node: &Value, live: &BTreeMap<String, Value>) -> Vec<String> {
    let mut out = Vec::new();
    guard_at(node, live.get(path), &[], live, path, 0, &mut out);
    out
}

/// Fail closed (T-26-91): `deploy_tree_invalid` naming every AtomicTag of
/// `node` whose live value a carried `Overwrite` would reset
/// ([`uncarriable`]). The forward run checks every node of every `carry`
/// batch before its first write and again against the fresh export at send
/// time; a rollback lists such nodes in [`RollbackPlan::uncarriable`].
pub fn carry_guard(
    path: &str,
    node: &Value,
    live: &BTreeMap<String, Value>,
) -> Result<(), CoreError> {
    let found = uncarriable(path, node, live);
    if found.is_empty() {
        return Ok(());
    }
    Err(CoreError::deploy_tree_invalid(
        path,
        None,
        None,
        format!(
            "{} hold(s) a live value and a valueSource that names no source (not a string), \
             so an Overwrite cannot carry the value and would reset it; the tag is not sent \
             that way (fail closed, KL-5)",
            found.join(", ")
        ),
    ))
}

/// [`uncarriable`] over every node of `batch`'s body: each node path whose
/// carry would reset a live value, with what [`uncarriable`] found.
pub fn uncarriable_in_batch(
    batch: &ImportBatch,
    live: &BTreeMap<String, Value>,
) -> Result<Vec<(String, Vec<String>)>, CoreError> {
    let body: Value = serde_json::from_slice(&batch.body)
        .map_err(|e| CoreError::Internal(format!("a tag import body is not JSON: {e}")))?;
    let mut out = Vec::new();
    for node in body
        .get("tags")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(name) = node.get("name").and_then(Value::as_str) else {
            continue;
        };
        let path = join(&batch.path, name);
        let found = uncarriable(&path, node, live);
        if !found.is_empty() {
            out.push((path, found));
        }
    }
    Ok(out)
}

fn guard_at(
    node: &Value,
    live: Option<&Value>,
    defs: &[&Value],
    fresh: &BTreeMap<String, Value>,
    at: &str,
    depth: usize,
    out: &mut Vec<String>,
) {
    if depth > MAX_DEPTH {
        return;
    }
    match tag_type(node) {
        Some("AtomicTag") => {
            let source = effective_value_source(node, defs);
            if source.is_none() && live.is_some_and(|l| l.get("value").is_some()) {
                out.push(at.to_string());
            }
        }
        Some("UdtInstance") => {
            let id = node
                .get("typeId")
                .and_then(Value::as_str)
                .or_else(|| {
                    defs.iter()
                        .find_map(|d| d.get("typeId").and_then(Value::as_str))
                })
                .map(str::to_string);
            let mut scope: Vec<&Value> = defs.to_vec();
            scope.extend(type_chain(id.as_deref(), fresh));
            guard_children(node, live, &scope, fresh, at, depth, out);
        }
        Some("Folder") => guard_children(node, live, defs, fresh, at, depth, out),
        _ => {}
    }
}

fn guard_children(
    node: &Value,
    live: Option<&Value>,
    defs: &[&Value],
    fresh: &BTreeMap<String, Value>,
    at: &str,
    depth: usize,
    out: &mut Vec<String>,
) {
    let Some(children) = node.get("tags").and_then(Value::as_array) else {
        return;
    };
    for c in children {
        let Some(name) = c.get("name").and_then(Value::as_str) else {
            continue;
        };
        let live_child = live.and_then(|l| child(l, name));
        let child_defs: Vec<&Value> = defs.iter().filter_map(|d| child(d, name)).collect();
        guard_at(
            c,
            live_child,
            &child_defs,
            fresh,
            &join(at, name),
            depth + 1,
            out,
        );
    }
}

// -- rollback planner (26-11, R3-RB, R3-CARRY, R3-DEL) -----------------------

/// What a run sent to one owned provider, for its rollback (26-11): every
/// planned node path a sent import batch named. A batch that answered with a
/// failure may still have applied part of its nodes, so every node of every
/// sent batch counts as possibly applied, and [`rollback_plan`] compares each
/// with the provider as it is now.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppliedSteps {
    /// The provider-relative paths, byte-sorted.
    pub nodes: BTreeSet<String>,
    /// Every provider-relative path a sent tagDelete call named, whatever it
    /// answered (26-19; the forward delete step fills it). A path the
    /// provider holds now is not re-created.
    pub deleted: Vec<String>,
    /// The node the run sent at each added path of a sent batch, flattened
    /// as the repo holds it (26-REVIEW CR-01). A created node is the run's
    /// to delete only when the live node is this one ([`rollback_plan`]).
    pub bodies: BTreeMap<String, Value>,
    /// The added paths whose import answered that the node already existed
    /// (an `Abort` collision): another client's node, never deleted.
    pub collided: BTreeSet<String>,
}

impl AppliedSteps {
    /// `batch` was sent, whatever came back.
    pub fn sent(&mut self, batch: &ImportBatch) {
        self.nodes.extend(batch.nodes.iter().cloned());
    }

    /// Whether no batch and no delete was sent.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty() && self.deleted.is_empty()
    }
}

/// The undo of the tag writes a run made to one owned provider (26-11),
/// drawn by [`rollback_plan`] from the pre-image export and the provider as
/// it is now. Paths are provider-relative and byte-sorted; nothing in it is
/// a value except the pre-image configuration in the import bodies.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RollbackPlan {
    /// The provider.
    pub provider: String,
    /// The restore imports, in send order: the UDT types a delete removed
    /// (`Abort` of the whole pre-image type, one per batch, dependency
    /// order), changed UDT types (`Overwrite` of the whole pre-image type,
    /// dependency order), the tag nodes a delete removed (`Abort` from the
    /// pre-image, parents first), the changed tags another client removed
    /// (26-12), then changed tags that gained no property (`MergeOverwrite`
    /// of the stripped pre-image), then changed tags that gained one
    /// (`Overwrite` with `carry`).
    pub imports: Vec<ImportBatch>,
    /// Nodes the run created in a provider that existed and that no delete
    /// removes: without a ready tag-delete route listing the provider
    /// (R3-DEL) every one of them, and with one a created folder that holds
    /// a live node the run did not create (26-19). They stay and the
    /// rollback ends `rollback_failed` naming them. Never approximated by
    /// another write.
    pub undeletable: Vec<String>,
    /// Nodes the run created in a provider the run created: they go with the
    /// provider when the run deletes it.
    pub provider_deletes: Vec<String>,
    /// The nodes of the carried restores.
    pub carried: Vec<String>,
    /// Property-adding changed nodes whose live value a carried `Overwrite`
    /// cannot carry ([`carry_guard`]): never sent, the rollback ends
    /// `rollback_failed` naming them.
    pub uncarriable: Vec<String>,
    /// Changed nodes absent now that `recreate` lists (26-12): re-created
    /// with `Abort` from their stripped pre-image.
    pub recreated: Vec<String>,
    /// The tag-delete route's deletes of the nodes the run created (26-19),
    /// sent after `imports`, in send order: created tag nodes, then created
    /// types (reverse dependency order) and their folders. With
    /// [`DeleteDescendants`] true for a node's kind and every live node
    /// below it created by the run, only the top-most node is named;
    /// otherwise every created path, deepest first.
    pub deletes: Vec<String>,
    /// The deleted paths re-created from the pre-image (26-19): each top-most
    /// one, with every pre-image node below it.
    pub recreated_deleted: Vec<String>,
    /// Nodes at added paths, absent from the pre-image and present now, that
    /// the run cannot show it wrote (26-REVIEW CR-01): never deleted, the
    /// rollback ends `rollback_failed` naming each with its reason.
    pub foreign: Vec<(String, Foreign)>,
    /// The nodes the run created that `deletes` and the kept folders in
    /// `undeletable` were drawn from, when the route lists the provider
    /// (26-REVIEW WR-04): the rollback draws its deletes again from an
    /// export read right before them ([`created_deletes_now`]).
    pub created: Vec<String>,
    /// Created types a node that stays uses, as `(path, user)`, also in
    /// `undeletable` (26-REVIEW WR-05).
    pub in_use: Vec<(String, String)>,
}

/// Why a node at a path the run added is not the run's to delete
/// (26-REVIEW CR-01).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Foreign {
    /// Its import answered that the node already existed (an `Abort`
    /// collision): another client created it first.
    Collided,
    /// The live node is not the node the run sent (values stripped, the
    /// R3-DV created-node rule).
    Differs,
    /// Nothing records what the run sent at the path.
    Unrecorded,
}

impl Foreign {
    /// The reason as a rollback names it.
    pub fn describe(self) -> &'static str {
        match self {
            Foreign::Collided => {
                "not removed: its import answered that it already existed, so it is another \
                 client's"
            }
            Foreign::Differs => {
                "not removed: the live node is not what this run sent, so it may be another \
                 client's"
            }
            Foreign::Unrecorded => {
                "not removed: the host copy does not record what this run sent at this path"
            }
        }
    }
}

/// How a created node is compared with what the run sent: R3-VK, R3-KEY and
/// R3-DV's created-node rule, as the forward verify compares it.
const CREATED_NODE: CanonOpts = CanonOpts {
    strip_values: true,
    created: true,
    exports: false,
};

/// Whether the node at `path` in `current` is the one the run wrote there
/// (26-REVIEW CR-01): its import did not collide and it equals the sent
/// node. `Err` says why not.
fn written_by_run(
    path: &str,
    current: &BTreeMap<String, Value>,
    applied: &AppliedSteps,
) -> Result<(), Foreign> {
    if applied.collided.contains(path) {
        return Err(Foreign::Collided);
    }
    let Some(sent) = applied.bodies.get(path) else {
        return Err(Foreign::Unrecorded);
    };
    match current.get(path) {
        Some(now) if diff_properties(sent, now, path, CREATED_NODE).is_empty() => Ok(()),
        _ => Err(Foreign::Differs),
    }
}

/// The undo of one owned provider's tag writes (R3-RB): for every node a
/// sent batch named (`applied`), against the pre-image export `pre_image`
/// and the provider as it is now, `current` (both flattened):
///
/// - a node the plan added that is there now is `undeletable`, or, in a
///   provider the run created (`provider_created`), one of
///   `provider_deletes`; one that is not there needs nothing;
/// - a changed node already equal to its pre-image (export against export:
///   R3-VK, R3-KEY, `defaultValue` compared) needs nothing, and one that is
///   gone is left to the rollback verify;
/// - a changed type goes back whole (`Overwrite`, the pre-image's own
///   folder chain), in dependency order ([`type_order`]);
/// - a changed tag goes back by `MergeOverwrite` of its stripped pre-image
///   when [`added_properties`] finds nothing the run added, so modified and
///   removed properties return and no value is written; otherwise by
///   `Overwrite` of the stripped pre-image with `carry` (R3-CARRY: the run
///   fills the values from an export read right before the batch), unless
///   [`uncarriable`] finds a value the carry would reset, which makes it
///   `uncarriable`.
///
/// - a changed tag that is gone now is left to the rollback verify, unless
///   `recreate` (the provider's tag record from before the run, 26-12)
///   lists it: then it is re-created by `Abort` of its stripped pre-image,
///   so no pre-run value is written (no live value exists to keep). The
///   automatic rollback passes an empty set.
///
/// - a path a sent tagDelete call named (`applied.deleted`) that the
///   provider lacks now is re-created by `Abort` from the pre-image, with
///   every pre-image node below it ([`recreate_deleted`]); a path below a
///   re-created one is not sent again, and one the plan does not remove is
///   `Internal` (the applied steps are inconsistent);
/// - a node the run created (in the plan's added nodes, absent from the
///   pre-image, present now) goes to `deletes` instead of `undeletable`
///   when `route` (the tag-delete route's handshake, when it is ready) lists
///   the provider (26-19, D-13 item 9), by [`created_deletes`] under
///   `descendants`;
/// - such a node counts as created only when the run wrote it
///   (26-REVIEW CR-01): its import did not answer that it already existed
///   (`applied.collided`) and the live node equals the node the run sent
///   (`applied.bodies`, values stripped, R3-DV's created-node rule).
///   Anything else is `foreign` with its reason and is never deleted.
///
/// Nodes outside the plan are never named (drift, untouched siblings), and
/// neither is a node the pre-image held.
#[allow(clippy::too_many_arguments)]
pub fn rollback_plan(
    plan: &ProviderPlan,
    pre_image: &BTreeMap<String, Value>,
    current: &BTreeMap<String, Value>,
    applied: &AppliedSteps,
    provider_created: bool,
    recreate: &BTreeSet<String>,
    route: Option<&RouteHandshake>,
    descendants: DeleteDescendants,
) -> Result<RollbackPlan, CoreError> {
    let provider = plan.provider.as_str();
    let mut out = RollbackPlan {
        provider: provider.to_string(),
        ..RollbackPlan::default()
    };
    let removed: BTreeSet<&str> = plan.removed.iter().map(String::as_str).collect();
    if let Some(stray) = applied
        .deleted
        .iter()
        .find(|path| !removed.contains(path.as_str()))
    {
        return Err(CoreError::Internal(format!(
            "the rollback of provider {provider} found {stray} among the deleted paths, and the \
             plan does not remove it: the run's applied steps are inconsistent"
        )));
    }
    let (deleted_types, deleted_tags, recreated_deleted) =
        recreate_deleted(provider, &applied.deleted, pre_image, current)?;
    out.imports.extend(deleted_types);
    out.recreated_deleted = recreated_deleted;
    let added: BTreeSet<&str> = plan.added.iter().map(String::as_str).collect();
    let changed: BTreeSet<&str> = plan.changed.iter().map(|c| c.path.as_str()).collect();
    let exports = CanonOpts {
        strip_values: true,
        created: false,
        exports: true,
    };
    let mut types: Vec<&str> = Vec::new();
    let mut merged: Vec<&str> = Vec::new();
    let mut carry: Vec<&str> = Vec::new();
    let mut recreated: Vec<&str> = Vec::new();
    // Nodes the run created in this existing provider: added, absent from
    // the pre-image, present now. Only these may ever be deleted.
    let mut created: Vec<&str> = Vec::new();
    for path in &applied.nodes {
        let path = path.as_str();
        let pre = pre_image.get(path).filter(|_| !added.contains(path));
        let Some(pre) = pre else {
            if current.contains_key(path) {
                if provider_created {
                    out.provider_deletes.push(path.to_string());
                } else if added.contains(path) && !pre_image.contains_key(path) {
                    match written_by_run(path, current, applied) {
                        Ok(()) => created.push(path),
                        Err(why) => out.foreign.push((path.to_string(), why)),
                    }
                } else {
                    out.undeletable.push(path.to_string());
                }
            }
            continue;
        };
        if !changed.contains(path) {
            continue;
        }
        let Some(now) = current.get(path) else {
            if recreate.contains(path) && !in_types(path) {
                recreated.push(path);
            }
            continue;
        };
        if diff_properties(pre, now, path, exports).is_empty() {
            continue;
        }
        if in_types(path) {
            if tag_type(pre) != Some("Folder") {
                types.push(path);
            }
            continue;
        }
        if added_properties(pre, now, path).is_empty() {
            merged.push(path);
            continue;
        }
        let mut value = named(pre, name_of(path));
        strip_runtime_values(&mut value, path);
        let found = uncarriable(path, &value, current);
        if found.is_empty() {
            carry.push(path);
        } else {
            out.uncarriable.push(path.to_string());
        }
    }

    // Types first, whole, in dependency order, in their pre-image chain.
    let type_nodes: Vec<NativeNode> = types
        .iter()
        .map(|path| NativeNode {
            path: (*path).to_string(),
            kind: NodeKind::Type,
            json: pre_image[*path].clone(),
            file: format!("[{provider}]{path}"),
        })
        .collect();
    let refs: Vec<&NativeNode> = type_nodes.iter().collect();
    for path in type_order(&refs)? {
        let wrapper = in_types_chain(&path, named(&pre_image[&path], name_of(&path)), pre_image);
        out.imports.push(ImportBatch {
            provider: provider.to_string(),
            path: String::new(),
            policy: CollisionPolicy::Overwrite,
            body: body_bytes(vec![wrapper])?,
            nodes: vec![path],
            carry: false,
        });
    }

    // Then the tag nodes a delete removed, parents first (26-19).
    out.imports.extend(deleted_tags);

    // Then the changed tags another client removed, re-created stripped
    // (26-12), each by parent path.
    let mut gone: BTreeMap<&str, Vec<Item>> = BTreeMap::new();
    for path in &recreated {
        let mut value = named(&pre_image[*path], name_of(path));
        strip_runtime_values(&mut value, path);
        let size = json_len(&value);
        gone.entry(parent_of(path).unwrap_or(""))
            .or_default()
            .push(Item {
                value,
                size,
                paths: vec![(*path).to_string()],
            });
    }
    for (parent, items) in gone {
        pack(
            provider,
            parent,
            CollisionPolicy::Abort,
            false,
            items,
            &mut out.imports,
        )?;
    }
    out.recreated = recreated.iter().map(|p| (*p).to_string()).collect();

    // Then the tags: merged (stripped), then carried, each by parent path.
    for (carrying, paths) in [(false, &merged), (true, &carry)] {
        let mut groups: BTreeMap<&str, Vec<Item>> = BTreeMap::new();
        for path in paths {
            let mut value = named(&pre_image[*path], name_of(path));
            strip_runtime_values(&mut value, path);
            let size = if carrying {
                let mut measured = value.clone();
                carry_live_values(&mut measured, path, current);
                json_len(&measured)
            } else {
                json_len(&value)
            };
            groups
                .entry(parent_of(path).unwrap_or(""))
                .or_default()
                .push(Item {
                    value,
                    size,
                    paths: vec![(*path).to_string()],
                });
        }
        let policy = if carrying {
            ExistingNodePolicy::OverwriteCarry
        } else {
            ExistingNodePolicy::MergeStripped
        }
        .collision_policy();
        for (parent, items) in groups {
            pack(provider, parent, policy, carrying, items, &mut out.imports)?;
        }
    }
    out.carried = carry.iter().map(|p| (*p).to_string()).collect();

    // Then the deletes of what the run created, through the route when it
    // is ready and lists the provider (26-19); otherwise 26-11's ending.
    let allowed = route.is_some_and(|h| h.providers.iter().any(|p| p == provider));
    if allowed {
        let drawn = created_deletes(provider, &created, current, descendants)?;
        out.deletes = drawn.deletes;
        out.undeletable.extend(drawn.kept);
        out.undeletable
            .extend(drawn.in_use.iter().map(|(path, _)| path.clone()));
        out.in_use = drawn.in_use;
        out.created = created.iter().map(|p| (*p).to_string()).collect();
    } else {
        out.undeletable
            .extend(created.iter().map(|p| (*p).to_string()));
    }
    out.undeletable
        .sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    Ok(out)
}

/// `inner`, the node at `path` below `_types_/`, wrapped in its folder chain
/// up to the `_types_` folder at the provider root, each folder as the
/// pre-image holds it (a bare folder where it holds none).
fn in_types_chain(path: &str, inner: Value, pre_image: &BTreeMap<String, Value>) -> Value {
    let rest = path.strip_prefix(&format!("{TYPES_DIR}/")).unwrap_or(path);
    let segments: Vec<&str> = rest.split('/').collect();
    let mut inner = inner;
    for i in (0..segments.len().saturating_sub(1)).rev() {
        let folder = format!("{TYPES_DIR}/{}", segments[..=i].join("/"));
        let mut wrapper = pre_image
            .get(&folder)
            .cloned()
            .unwrap_or_else(|| json!({"tagType": "Folder"}));
        if let Some(object) = wrapper.as_object_mut() {
            object.insert("name".to_string(), Value::String(segments[i].to_string()));
            object.insert("tags".to_string(), Value::Array(vec![inner]));
        }
        inner = wrapper;
    }
    json!({"name": TYPES_DIR, "tagType": "Folder", "tags": [inner]})
}

/// The direct children of `path` in the flattened `nodes`, in byte order.
fn direct_children<'a>(path: &str, nodes: &'a BTreeMap<String, Value>) -> Vec<&'a str> {
    let prefix = format!("{path}/");
    live_below(path, nodes)
        .into_iter()
        .filter(|below| !below[prefix.len()..].contains('/'))
        .collect()
}

/// The node at `path` as the pre-image holds it, to be re-created (26-19):
/// a folder with every pre-image node below it nested; any other node with
/// its own subtree, and with the values R3-CARRY carries put back from the
/// pre-image ([`carry_live_values`] against the pre-image): a memory-sourced
/// AtomicTag (no `valueSource` key is memory) keeps its pre-image `value`,
/// since no live value exists to keep; any other value source is sent
/// without it; `timestamp` and `quality` are dropped; `defaultValue`,
/// parameters and member overrides stay as the pre-image holds them. Below
/// `_types_/` the node is sent whole and never stripped. Every path it
/// holds goes to `paths`.
fn recreated_node(
    path: &str,
    pre_image: &BTreeMap<String, Value>,
    paths: &mut Vec<String>,
    depth: usize,
) -> Value {
    let mut value = named(&pre_image[path], name_of(path));
    paths.push(path.to_string());
    if tag_type(&value) == Some("Folder") {
        if depth <= MAX_DEPTH
            && let Some(object) = value.as_object_mut()
        {
            let kids: Vec<Value> = direct_children(path, pre_image)
                .into_iter()
                .map(|child| recreated_node(child, pre_image, paths, depth + 1))
                .collect();
            object.insert("tags".to_string(), Value::Array(kids));
        }
    } else if !in_types(path) {
        carry_live_values(&mut value, path, pre_image);
    }
    value
}

/// The re-create of every path in `deleted` the provider lacks now and the
/// pre-image holds (26-19, R3-RB): `(type batches, tag batches, top-most
/// paths re-created)`. A path below another re-created one comes back with
/// it and is not sent again, whichever delete mode named it.
///
/// Below `_types_/`: each type of the re-created subtrees goes alone in its
/// pre-image folder chain, in [`type_order`], and a re-created folder that
/// holds no type goes alone the same way. Other nodes: [`recreated_node`],
/// by `Abort` at their parent, parents first; a node whose parent the
/// provider lacks now (and that is not itself re-created) is left to the
/// rollback verify. A folder whose nested body is over
/// [`IMPORT_BATCH_MAX_BYTES`] goes alone and its children below it,
/// recursively (26-14); only a single node over the cap makes a body over
/// it.
#[allow(clippy::type_complexity)]
fn recreate_deleted(
    provider: &str,
    deleted: &[String],
    pre_image: &BTreeMap<String, Value>,
    current: &BTreeMap<String, Value>,
) -> Result<(Vec<ImportBatch>, Vec<ImportBatch>, Vec<String>), CoreError> {
    let depth = |p: &str| p.bytes().filter(|b| *b == b'/').count();
    let mut order: Vec<&str> = deleted
        .iter()
        .map(String::as_str)
        .filter(|path| !current.contains_key(*path) && pre_image.contains_key(*path))
        .collect();
    order.sort_by(|a, b| depth(a).cmp(&depth(b)).then(a.as_bytes().cmp(b.as_bytes())));
    order.dedup();
    let mut heads: Vec<&str> = Vec::new();
    for path in order {
        if heads.iter().any(|head| is_below(path, head)) {
            continue;
        }
        if !in_types(path)
            && let Some(parent) = parent_of(path)
            && !current.contains_key(parent)
        {
            continue;
        }
        heads.push(path);
    }
    heads.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));

    // Types: every pre-image node of the re-created subtrees below `_types_`.
    let mut type_set: BTreeSet<&str> = BTreeSet::new();
    for head in heads.iter().filter(|h| in_types(h)) {
        type_set.insert(head);
        type_set.extend(live_below(head, pre_image));
    }
    let types: Vec<NativeNode> = type_set
        .iter()
        .filter(|path| tag_type(&pre_image[**path]) == Some("UdtType"))
        .map(|path| NativeNode {
            path: (*path).to_string(),
            kind: NodeKind::Type,
            json: pre_image[*path].clone(),
            file: format!("[{provider}]{path}"),
        })
        .collect();
    let refs: Vec<&NativeNode> = types.iter().collect();
    let mut type_batches = Vec::new();
    let mut covered: BTreeSet<String> = BTreeSet::new();
    let chain_nodes = |path: &str, covered: &mut BTreeSet<String>| {
        let mut nodes = vec![path.to_string()];
        let mut at = parent_of(path);
        while let Some(folder) = at.filter(|f| in_types(f)) {
            if type_set.contains(folder) && covered.insert(folder.to_string()) {
                nodes.push(folder.to_string());
            }
            at = parent_of(folder);
        }
        nodes.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        nodes
    };
    for path in type_order(&refs)? {
        let nodes = chain_nodes(&path, &mut covered);
        let wrapper = in_types_chain(&path, named(&pre_image[&path], name_of(&path)), pre_image);
        type_batches.push(ImportBatch {
            provider: provider.to_string(),
            path: String::new(),
            policy: CollisionPolicy::Abort,
            body: body_bytes(vec![wrapper])?,
            nodes,
            carry: false,
        });
    }
    for folder in type_set
        .iter()
        .filter(|path| tag_type(&pre_image[**path]) == Some("Folder"))
    {
        if covered.contains(*folder)
            || type_set
                .iter()
                .any(|p| is_below(p, folder) && tag_type(&pre_image[*p]) == Some("UdtType"))
        {
            continue;
        }
        let nodes = chain_nodes(folder, &mut covered);
        let alone = named(&pre_image[*folder], name_of(folder));
        let wrapper = in_types_chain(folder, alone, pre_image);
        type_batches.push(ImportBatch {
            provider: provider.to_string(),
            path: String::new(),
            policy: CollisionPolicy::Abort,
            body: body_bytes(vec![wrapper])?,
            nodes,
            carry: false,
        });
    }

    // Tag nodes: by parent, parents first, a folder over the cap split.
    let mut queue: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for head in heads.iter().filter(|h| !in_types(h)) {
        queue
            .entry(parent_of(head).unwrap_or("").to_string())
            .or_default()
            .push((*head).to_string());
    }
    let mut tag_batches = Vec::new();
    while let Some((parent, list)) = queue.pop_first() {
        let mut items = Vec::new();
        for head in list {
            let mut paths = Vec::new();
            let value = recreated_node(&head, pre_image, &mut paths, 0);
            let size = json_len(&value);
            let children = direct_children(&head, pre_image);
            if size + BODY_FRAME <= IMPORT_BATCH_MAX_BYTES
                || tag_type(&value) != Some("Folder")
                || children.is_empty()
            {
                items.push(Item { value, size, paths });
                continue;
            }
            // The flattened pre-image folder holds its own keys only.
            let alone = named(&pre_image[&head], name_of(&head));
            let size = json_len(&alone);
            items.push(Item {
                value: alone,
                size,
                paths: vec![head.clone()],
            });
            queue.insert(
                head.clone(),
                children.into_iter().map(str::to_string).collect(),
            );
        }
        pack(
            provider,
            &parent,
            CollisionPolicy::Abort,
            false,
            items,
            &mut tag_batches,
        )?;
    }
    Ok((
        type_batches,
        tag_batches,
        heads.into_iter().map(str::to_string).collect(),
    ))
}

/// Whether `path` is strictly below `ancestor`.
fn is_below(path: &str, ancestor: &str) -> bool {
    path.len() > ancestor.len() + 1
        && path.starts_with(ancestor)
        && path.as_bytes()[ancestor.len()] == b'/'
}

/// The live nodes strictly below `path` in `current` (flattened: a folder's
/// children are nodes of their own, an instance or a type holds its
/// members).
fn live_below<'a>(path: &str, current: &'a BTreeMap<String, Value>) -> Vec<&'a str> {
    let prefix = format!("{path}/");
    current
        .range::<str, _>((
            std::ops::Bound::Included(prefix.as_str()),
            std::ops::Bound::Unbounded,
        ))
        .map(|(key, _)| key.as_str())
        .take_while(|key| key.starts_with(&prefix))
        .collect()
}

/// The member paths of the live node at `path` (an instance), through member
/// folders and nested instances.
fn member_paths(path: &str, node: &Value, out: &mut Vec<String>, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    for member in node
        .get("tags")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(name) = member.get("name").and_then(Value::as_str) {
            let below = join(path, name);
            member_paths(&below, member, out, depth + 1);
            out.push(below);
        }
    }
}

/// What [`created_deletes`] draws: the deletes in send order and the created
/// nodes that must stay, each list provider-relative.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CreatedDeletes {
    /// The paths to send, in send order.
    pub deletes: Vec<String>,
    /// Created folders that hold a live node the run did not create.
    pub kept: Vec<String>,
    /// Created types (or the created folder that would remove one) that a
    /// node that stays uses, as `(path, the user's path)` (26-REVIEW WR-05).
    pub in_use: Vec<(String, String)>,
    /// Created paths that are no longer the node the run wrote
    /// ([`created_deletes_now`] only, 26-REVIEW CR-01).
    pub foreign: Vec<(String, Foreign)>,
}

/// The deletes of the nodes the run `created` in `provider` (26-19,
/// T-26-77), and the created nodes that must stay.
///
/// A created node with a live node below it in `current` that the run did
/// not create stays (a created folder holding another client's node), and
/// is returned in `kept`; its created descendants are still sent. A created
/// type that a node that stays uses (a survivor of `current`, after the
/// deletes) is never sent, nor a created folder whose delete would remove
/// it, and so on for the types those use; each is in `in_use` with its user
/// (26-REVIEW WR-05).
/// Otherwise, when `descendants` says a delete of the node's kind removes
/// what is below it, the node goes alone and its created descendants are
/// not named; when it does not, every created path is sent (an instance
/// with its members). Order: created tag nodes deepest first (children
/// before their folder or instance, byte order within a depth), then below
/// `_types_/` the created types in reverse [`type_order`], then their
/// created folders deepest first.
pub fn created_deletes(
    provider: &str,
    created: &[&str],
    current: &BTreeMap<String, Value>,
    descendants: DeleteDescendants,
) -> Result<CreatedDeletes, CoreError> {
    let set: BTreeSet<&str> = created.iter().copied().collect();
    let mut collapsed: Vec<&str> = Vec::new();
    let mut kept: Vec<String> = Vec::new();
    let mut send: Vec<String> = Vec::new();
    for path in &set {
        let path = *path;
        if collapsed.iter().any(|top| is_below(path, top)) {
            continue;
        }
        if live_below(path, current)
            .iter()
            .any(|below| !set.contains(below))
        {
            kept.push(path.to_string());
            continue;
        }
        let node = current.get(path);
        let removes_below = match node.and_then(tag_type) {
            Some("Folder") => descendants.folders,
            Some("UdtInstance") => descendants.instances,
            _ => true,
        };
        if removes_below {
            collapsed.push(path);
        } else if let Some(node) = node.filter(|n| tag_type(n) == Some("UdtInstance")) {
            member_paths(path, node, &mut send, 0);
        }
        send.push(path.to_string());
    }
    // A created type a node that stays uses stays too (26-REVIEW WR-05),
    // with the collapsed folder that would remove it; repeated, since a
    // type that stays is a user of the types it names.
    let mut in_use: Vec<(String, String)> = Vec::new();
    loop {
        let sent: BTreeSet<&str> = send.iter().map(String::as_str).collect();
        let gone =
            |path: &str| sent.contains(path) || collapsed.iter().any(|top| is_below(path, top));
        let removed_types: BTreeSet<&str> = current
            .iter()
            .filter(|(path, node)| tag_type(node) == Some("UdtType") && gone(path))
            .map(|(path, _)| path.as_str())
            .collect();
        let survivors = current
            .iter()
            .filter(|(path, _)| !gone(path))
            .map(|(path, node)| (path.as_str(), node));
        let Some((type_path, user)) = type_in_use(&removed_types, survivors) else {
            break;
        };
        let top = collapsed
            .iter()
            .find(|top| is_below(&type_path, top))
            .map(|top| (*top).to_string());
        let stays = top.unwrap_or_else(|| type_path.clone());
        collapsed.retain(|c| *c != stays.as_str());
        send.retain(|p| *p != stays);
        if stays != type_path {
            in_use.push((stays, user.clone()));
        }
        in_use.push((type_path, user));
    }
    let depth = |p: &str| p.bytes().filter(|b| *b == b'/').count();
    let deepest_first =
        |a: &String, b: &String| depth(b).cmp(&depth(a)).then(a.as_bytes().cmp(b.as_bytes()));
    let (types, mut tags): (Vec<String>, Vec<String>) = send.into_iter().partition(|p| in_types(p));
    tags.sort_by(deepest_first);
    let (type_paths, mut type_folders): (Vec<String>, Vec<String>) = types
        .into_iter()
        .partition(|p| current.get(p).and_then(tag_type) == Some("UdtType"));
    let type_nodes: Vec<NativeNode> = type_paths
        .iter()
        .map(|path| NativeNode {
            path: path.clone(),
            kind: NodeKind::Type,
            json: current[path].clone(),
            file: format!("[{provider}]{path}"),
        })
        .collect();
    let refs: Vec<&NativeNode> = type_nodes.iter().collect();
    let mut ordered = type_order(&refs)?;
    ordered.reverse();
    type_folders.sort_by(deepest_first);
    tags.extend(ordered);
    tags.extend(type_folders);
    Ok(CreatedDeletes {
        deletes: tags,
        kept,
        in_use,
        foreign: Vec::new(),
    })
}

/// The deletes of the nodes the run `created` in `provider` drawn again from
/// `fresh`, the provider's export read right before them (26-REVIEW WR-04).
/// A created path `fresh` lacks is gone and
/// needs nothing; one that is no longer the node the run wrote is `foreign`
/// ([`rollback_plan`]'s rule); the others go through [`created_deletes`]
/// against `fresh`, so a node another client added below a created folder
/// during the restores keeps the folder.
pub fn created_deletes_now(
    provider: &str,
    created: &[String],
    fresh: &BTreeMap<String, Value>,
    applied: &AppliedSteps,
    descendants: DeleteDescendants,
) -> Result<CreatedDeletes, CoreError> {
    let mut ours: Vec<&str> = Vec::new();
    let mut foreign = Vec::new();
    for path in created.iter().filter(|path| fresh.contains_key(*path)) {
        match written_by_run(path, fresh, applied) {
            Ok(()) => ours.push(path),
            Err(why) => foreign.push((path.clone(), why)),
        }
    }
    let mut drawn = created_deletes(provider, &ours, fresh, descendants)?;
    drawn.foreign = foreign;
    Ok(drawn)
}

// -- the forward delete set (26-20, D-02) ------------------------------------

/// One tagDelete call of a run's delete step (26-20): provider-relative
/// paths of one provider, at most the route's `maxPaths`, never empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagDeleteCall {
    /// The provider.
    pub provider: String,
    /// The paths, in send order.
    pub paths: Vec<String>,
}

/// Why [`delete_calls`] sends nothing of a provider (26-20). Paths are
/// provider-relative.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeleteSetError {
    /// A removed folder or instance now holds `node`, a live node outside
    /// the removed set (D-01: drift is never deleted, T-26-84).
    Changed {
        /// The removed node.
        path: String,
        /// The live node below it that no removal covers.
        node: String,
    },
    /// A removed type that the surviving node `referrer` uses (T-26-82).
    TypeInUse {
        /// The removed type.
        type_path: String,
        /// The surviving node that names it by `typeId`.
        referrer: String,
    },
    /// A path longer than the route's `maxPathChars`.
    TooLong {
        /// The path.
        path: String,
        /// Its length in characters.
        chars: usize,
        /// The route's limit.
        max: usize,
    },
}

impl DeleteSetError {
    /// The refusal as the run reports it, with full tag paths of
    /// `provider`: paths and names only.
    pub fn describe(&self, provider: &str) -> String {
        let full = |path: &str| format!("[{provider}]{path}");
        match self {
            Self::Changed { path, node } => format!(
                "the removed node {} now holds {}, which no removal covers, so nothing of \
                 provider {provider} was deleted (live-only tags are never deleted)",
                full(path),
                full(node)
            ),
            Self::TypeInUse {
                type_path,
                referrer,
            } => format!(
                "the removed type {} is used by {}, which stays, so nothing of provider \
                 {provider} was deleted",
                full(type_path),
                full(referrer)
            ),
            Self::TooLong { path, chars, max } => format!(
                "the removed path {} is {chars} characters and the tag-delete route accepts at \
                 most {max}, so nothing of provider {provider} was deleted",
                full(path)
            ),
        }
    }
}

/// The tagDelete calls of one provider's removals (26-20, D-02, T-26-81),
/// against `fresh`, the provider's export read right before them:
///
/// - only paths of `plan.removed` are ever named; one `fresh` lacks is
///   skipped (already absent), so the gateway's state can shrink the set
///   and never widen it;
/// - a removed path with a live node below it in `fresh` that is not
///   removed is [`DeleteSetError::Changed`] (another client's node made
///   after the plan; D-01);
/// - a removed type a surviving node of `fresh` uses is
///   [`DeleteSetError::TypeInUse`] ([`type_refs`]);
/// - with `descendants` true for a removed folder's or instance's kind its
///   removed descendants are not named (the delete removes them); otherwise
///   every removed path goes, deepest first. An instance's members are never
///   named: they are not removals;
/// - order: tag nodes deepest first (byte order within a depth), then below
///   `_types_/` the types in reverse [`type_order`] (a type before the types
///   it uses), then their folders deepest first;
/// - calls of at most the route's `maxPaths`, each path within its
///   `maxPathChars` ([`delete_chunks`]); no empty call.
pub fn delete_calls(
    plan: &ProviderPlan,
    fresh: &BTreeMap<String, Value>,
    handshake: &RouteHandshake,
    descendants: DeleteDescendants,
) -> Result<Vec<TagDeleteCall>, DeleteSetError> {
    let provider = plan.provider.as_str();
    let removed: BTreeSet<&str> = plan.removed.iter().map(String::as_str).collect();
    let present: Vec<&str> = removed
        .iter()
        .copied()
        .filter(|path| fresh.contains_key(*path))
        .collect();
    for path in &present {
        if let Some(node) = live_below(path, fresh)
            .into_iter()
            .find(|below| !removed.contains(below))
        {
            return Err(DeleteSetError::Changed {
                path: (*path).to_string(),
                node: node.to_string(),
            });
        }
    }
    let removed_types: BTreeSet<&str> = present
        .iter()
        .copied()
        .filter(|path| in_types(path) && fresh.get(*path).and_then(tag_type) == Some("UdtType"))
        .collect();
    let survivors = fresh
        .iter()
        .filter(|(path, _)| !removed.contains(path.as_str()))
        .map(|(path, node)| (path.as_str(), node));
    if let Some((type_path, referrer)) = type_in_use(&removed_types, survivors) {
        return Err(DeleteSetError::TypeInUse {
            type_path,
            referrer,
        });
    }
    let mut collapsed: Vec<&str> = Vec::new();
    let mut send: Vec<String> = Vec::new();
    for path in &present {
        if collapsed.iter().any(|top| is_below(path, top)) {
            continue;
        }
        let removes_below = match fresh.get(*path).and_then(tag_type) {
            Some("Folder") => descendants.folders,
            Some("UdtInstance") => descendants.instances,
            _ => false,
        };
        if removes_below {
            collapsed.push(path);
        }
        send.push((*path).to_string());
    }
    let depth = |p: &str| p.bytes().filter(|b| *b == b'/').count();
    let deepest_first =
        |a: &String, b: &String| depth(b).cmp(&depth(a)).then(a.as_bytes().cmp(b.as_bytes()));
    let (types, mut tags): (Vec<String>, Vec<String>) = send.into_iter().partition(|p| in_types(p));
    tags.sort_by(deepest_first);
    let (type_paths, mut type_folders): (Vec<String>, Vec<String>) = types
        .into_iter()
        .partition(|p| fresh.get(p).and_then(tag_type) == Some("UdtType"));
    let type_nodes: Vec<NativeNode> = type_paths
        .iter()
        .map(|path| NativeNode {
            path: path.clone(),
            kind: NodeKind::Type,
            json: fresh[path].clone(),
            file: format!("[{provider}]{path}"),
        })
        .collect();
    let refs: Vec<&NativeNode> = type_nodes.iter().collect();
    // A cycle among live types orders nothing better than byte order; the
    // types go anyway, every one of them removed.
    let mut ordered = type_order(&refs).unwrap_or_else(|_| type_paths.clone());
    ordered.reverse();
    type_folders.sort_by(deepest_first);
    tags.extend(ordered);
    tags.extend(type_folders);
    if let Some(path) = tags
        .iter()
        .find(|p| p.chars().count() > handshake.max_path_chars)
    {
        return Err(DeleteSetError::TooLong {
            path: path.clone(),
            chars: path.chars().count(),
            max: handshake.max_path_chars,
        });
    }
    let chunks = delete_chunks(&tags, handshake).map_err(|_| DeleteSetError::TooLong {
        path: String::new(),
        chars: 0,
        max: handshake.max_path_chars,
    })?;
    Ok(chunks
        .into_iter()
        .filter(|chunk| !chunk.is_empty())
        .map(|paths| TagDeleteCall {
            provider: provider.to_string(),
            paths,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::tags::CollisionPolicy;
    use crate::deploy::native_tags::{NativeNode, NativeProvider, NodeKind};
    use crate::deploy::tag_canon::flatten_export;
    use crate::deploy::tag_plan::plan_provider;
    use serde_json::{Value, json};
    use std::collections::BTreeMap;

    /// A repo provider from `(path, json)` pairs; the kind follows the path
    /// and `tagType`.
    fn repo(nodes: &[(&str, Value)]) -> NativeProvider {
        NativeProvider {
            name: "P".to_string(),
            nodes: nodes
                .iter()
                .map(|(path, json)| {
                    let kind = if json["tagType"] == "Folder" {
                        NodeKind::Folder
                    } else if path.starts_with("_types_/") {
                        NodeKind::Type
                    } else {
                        NodeKind::Tag
                    };
                    (
                        (*path).to_string(),
                        NativeNode {
                            path: (*path).to_string(),
                            kind,
                            json: json.clone(),
                            file: format!("tags/P/{path}.json"),
                        },
                    )
                })
                .collect(),
            groups: None,
        }
    }

    /// A provider export holding `tags`, flattened.
    fn live(tags: Value) -> BTreeMap<String, Value> {
        let root = json!({"name": "", "tagType": "Provider", "tags": tags});
        flatten_export(&serde_json::to_vec(&root).expect("json"), "P").expect("a tag tree")
    }

    fn batches(
        repo: &NativeProvider,
        live: &BTreeMap<String, Value>,
        policy: ExistingNodePolicy,
    ) -> Result<Vec<ImportBatch>, crate::error::CoreError> {
        let plan = plan_provider(repo, live, None);
        import_batches(&plan, repo, live, policy)
    }

    fn body(batch: &ImportBatch) -> Value {
        serde_json::from_slice(&batch.body).expect("a JSON body")
    }

    /// The node paths a body names: every node of the `tags` arrays below
    /// `batch.path`, descending through folders only (as an export is cut).
    fn body_paths(batch: &ImportBatch) -> Vec<String> {
        fn walk(tags: &[Value], prefix: &str, out: &mut Vec<String>) {
            for node in tags {
                let name = node["name"].as_str().expect("a named node");
                let path = if prefix.is_empty() {
                    name.to_string()
                } else {
                    format!("{prefix}/{name}")
                };
                out.push(path.clone());
                if node["tagType"] == "Folder"
                    && let Some(kids) = node["tags"].as_array()
                {
                    walk(kids, &path, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(
            body(batch)["tags"].as_array().expect("tags"),
            &batch.path,
            &mut out,
        );
        out
    }

    fn type_node(path: &str, json: Value) -> NativeNode {
        NativeNode {
            path: path.to_string(),
            kind: NodeKind::Type,
            json,
            file: format!("tags/P/{path}.json"),
        }
    }

    const MEMORY: &str = "memory";

    #[test]
    fn the_cap_is_512_kib() {
        assert_eq!(IMPORT_BATCH_MAX_BYTES, 524_288);
    }

    #[test]
    fn policies_map_to_the_wire() {
        assert_eq!(
            ExistingNodePolicy::MergeStripped.collision_policy(),
            CollisionPolicy::MergeOverwrite
        );
        assert_eq!(
            ExistingNodePolicy::OverwriteCarry.collision_policy(),
            CollisionPolicy::Overwrite
        );
        assert_eq!(REMOVED_PROPERTY_POLICY, ExistingNodePolicy::OverwriteCarry);
    }

    // -- type_order --------------------------------------------------------

    #[test]
    fn a_type_comes_after_the_types_it_references() {
        let base = type_node("_types_/Z/Base", json!({"tagType": "UdtType"}));
        let child = type_node(
            "_types_/A/Child",
            json!({"tagType": "UdtType", "typeId": "Z/Base"}),
        );
        let holder = type_node(
            "_types_/B",
            json!({"tagType": "UdtType", "tags": [
                {"name": "F", "tagType": "Folder", "tags": [
                    {"name": "I", "tagType": "UdtInstance", "typeId": "A/Child"}
                ]}
            ]}),
        );
        let order = type_order(&[&holder, &child, &base]).expect("no cycle");
        assert_eq!(order, ["_types_/Z/Base", "_types_/A/Child", "_types_/B"]);
    }

    #[test]
    fn independent_types_keep_byte_order() {
        let c = type_node("_types_/c", json!({"tagType": "UdtType"}));
        let a = type_node("_types_/a", json!({"tagType": "UdtType"}));
        let b = type_node(
            "_types_/b",
            json!({"tagType": "UdtType", "typeId": "Elsewhere"}),
        );
        let order = type_order(&[&c, &a, &b]).expect("no cycle");
        assert_eq!(order, ["_types_/a", "_types_/b", "_types_/c"]);
    }

    #[test]
    fn a_type_cycle_is_deploy_tree_invalid_naming_the_cycle() {
        let a = type_node("_types_/A", json!({"tagType": "UdtType", "typeId": "B"}));
        let b = type_node(
            "_types_/B",
            json!({"tagType": "UdtType", "tags": [
                {"name": "I", "tagType": "UdtInstance", "typeId": "A"}
            ]}),
        );
        let free = type_node("_types_/C", json!({"tagType": "UdtType"}));
        let err = type_order(&[&a, &b, &free]).expect_err("a cycle");
        assert_eq!(err.code(), "deploy_tree_invalid");
        let text = err.to_string();
        assert!(
            text.contains("_types_/A") && text.contains("_types_/B"),
            "{text}"
        );
        assert!(!text.contains("_types_/C"), "{text}");
    }

    // -- import_batches: order, policies, bodies ---------------------------

    #[test]
    fn types_go_first_one_per_batch_in_dependency_order_wrapped_in_types() {
        let repo = repo(&[
            ("_types_/Sub", json!({"tagType": "Folder"})),
            (
                "_types_/Sub/Pump",
                json!({"tagType": "UdtType", "tags": [
                    {"name": "M", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 4}
                ], "typeId": "Base"}),
            ),
            (
                "_types_/Base",
                json!({"tagType": "UdtType", "documentation": "new", "tags": [
                    {"name": "B", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 1}
                ]}),
            ),
            ("Line", json!({"tagType": "Folder"})),
            (
                "Line/T",
                json!({"tagType": "AtomicTag", "valueSource": MEMORY, "value": 3}),
            ),
        ]);
        let live = live(json!([
            {"name": "_types_", "tagType": "Folder", "tags": [
                {"name": "Base", "tagType": "UdtType", "documentation": "old", "tags": [
                    {"name": "B", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 1}
                ]}
            ]},
            {"name": "Line", "tagType": "Folder", "tags": []}
        ]));
        let batches = batches(&repo, &live, ExistingNodePolicy::OverwriteCarry).expect("plans");
        assert_eq!(batches.len(), 3, "{batches:?}");

        // Base is changed: Overwrite, whole, before the type that uses it.
        let base = &batches[0];
        assert_eq!(
            (base.path.as_str(), base.policy, base.carry),
            ("", CollisionPolicy::Overwrite, false)
        );
        assert_eq!(base.nodes, ["_types_/Base"]);
        assert_eq!(
            body(base),
            json!({"tags": [{"name": "_types_", "tagType": "Folder", "tags": [
                {"name": "Base", "tagType": "UdtType", "documentation": "new", "tags": [
                    {"name": "B", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 1}
                ]}
            ]}]})
        );

        // Pump is new, below a new folder: Abort, the folder chain wraps it,
        // and the type member's value (its default) is sent.
        let pump = &batches[1];
        assert_eq!(
            (pump.path.as_str(), pump.policy),
            ("", CollisionPolicy::Abort)
        );
        assert_eq!(pump.nodes, ["_types_/Sub", "_types_/Sub/Pump"]);
        assert_eq!(
            body(pump),
            json!({"tags": [{"name": "_types_", "tagType": "Folder", "tags": [
                {"name": "Sub", "tagType": "Folder", "tags": [
                    {"name": "Pump", "tagType": "UdtType", "typeId": "Base", "tags": [
                        {"name": "M", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 4}
                    ]}
                ]}
            ]}]})
        );

        // Then the tags: the new tag as exported, values included (D-10).
        let tag = &batches[2];
        assert_eq!(
            (tag.path.as_str(), tag.policy),
            ("Line", CollisionPolicy::Abort)
        );
        assert_eq!(
            body(tag),
            json!({"tags": [{"name": "T", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 3}]})
        );
    }

    #[test]
    fn added_heads_nest_their_added_children_and_share_a_parent_batch() {
        let repo = repo(&[
            ("Line", json!({"tagType": "Folder"})),
            ("Line/New", json!({"tagType": "Folder"})),
            (
                "Line/New/A",
                json!({"tagType": "AtomicTag", "valueSource": MEMORY, "value": 1, "defaultValue": 1}),
            ),
            (
                "Line/B",
                json!({"tagType": "AtomicTag", "valueSource": MEMORY, "value": 2}),
            ),
        ]);
        let live = live(json!([{"name": "Line", "tagType": "Folder", "tags": []}]));
        let batches = batches(&repo, &live, ExistingNodePolicy::OverwriteCarry).expect("plans");
        assert_eq!(batches.len(), 1, "{batches:?}");
        let batch = &batches[0];
        assert_eq!(
            (batch.path.as_str(), batch.policy),
            ("Line", CollisionPolicy::Abort)
        );
        assert_eq!(batch.nodes, ["Line/B", "Line/New", "Line/New/A"]);
        assert_eq!(
            body(batch),
            json!({"tags": [
                {"name": "B", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 2},
                {"name": "New", "tagType": "Folder", "tags": [
                    {"name": "A", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 1, "defaultValue": 1}
                ]}
            ]})
        );
    }

    /// R3-POL: a changed existing node is `MergeOverwrite`, stripped of the
    /// AtomicTag runtime keys only; `defaultValue`, instance `parameters` and
    /// member override configuration are sent. Siblings share one batch.
    #[test]
    fn changed_nodes_merge_stripped_and_keep_configuration() {
        let repo = repo(&[
            ("Line", json!({"tagType": "Folder"})),
            (
                "Line/T",
                json!({"tagType": "AtomicTag", "valueSource": MEMORY, "value": 3,
                       "defaultValue": 3, "documentation": "new", "timestamp": 1, "quality": "Good"}),
            ),
            (
                "Line/I",
                json!({"tagType": "UdtInstance", "typeId": "Ty",
                "parameters": {"P1": {"dataType": "Integer", "value": 43}},
                "tags": [
                    {"name": "M1", "tagType": "AtomicTag", "value": 1},
                    {"name": "M2", "tagType": "AtomicTag", "documentation": "override"}
                ]}),
            ),
        ]);
        let live = live(json!([{"name": "Line", "tagType": "Folder", "tags": [
            {"name": "T", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 9,
             "defaultValue": 3, "documentation": "old"},
            {"name": "I", "tagType": "UdtInstance", "typeId": "Ty",
             "parameters": {"P1": {"dataType": "Integer", "value": 42}},
             "tags": [{"name": "M1", "tagType": "AtomicTag", "value": 5}]}
        ]}]));
        let batches = batches(&repo, &live, ExistingNodePolicy::MergeStripped).expect("plans");
        assert_eq!(batches.len(), 1, "{batches:?}");
        let batch = &batches[0];
        assert_eq!(
            (batch.path.as_str(), batch.policy, batch.carry),
            ("Line", CollisionPolicy::MergeOverwrite, false)
        );
        assert_eq!(batch.nodes, ["Line/I", "Line/T"]);
        assert_eq!(
            body(batch),
            json!({"tags": [
                {"name": "I", "tagType": "UdtInstance", "typeId": "Ty",
                 "parameters": {"P1": {"dataType": "Integer", "value": 43}},
                 "tags": [
                     {"name": "M1", "tagType": "AtomicTag"},
                     {"name": "M2", "tagType": "AtomicTag", "documentation": "override"}
                 ]},
                {"name": "T", "tagType": "AtomicTag", "valueSource": MEMORY,
                 "defaultValue": 3, "documentation": "new"}
            ]})
        );
    }

    /// Choice (c) `overwrite-carry`: a changed node whose repo version drops a
    /// property the live node has goes `Overwrite` with `carry`, apart from
    /// the merged ones.
    #[test]
    fn a_dropped_property_goes_overwrite_carry() {
        let repo = repo(&[
            ("Line", json!({"tagType": "Folder"})),
            (
                "Line/P",
                json!({"tagType": "AtomicTag", "valueSource": MEMORY, "value": 1}),
            ),
            (
                "Line/Q",
                json!({"tagType": "AtomicTag", "valueSource": MEMORY, "documentation": "d2"}),
            ),
        ]);
        let live = live(json!([{"name": "Line", "tagType": "Folder", "tags": [
            {"name": "P", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 9, "tooltip": "tip"},
            {"name": "Q", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 9, "documentation": "d1"}
        ]}]));
        let batches = batches(&repo, &live, ExistingNodePolicy::OverwriteCarry).expect("plans");
        assert_eq!(batches.len(), 2, "{batches:?}");
        assert_eq!(
            (
                batches[0].policy,
                batches[0].carry,
                batches[0].nodes.clone()
            ),
            (
                CollisionPolicy::MergeOverwrite,
                false,
                vec!["Line/Q".to_string()]
            )
        );
        assert_eq!(
            (
                batches[1].policy,
                batches[1].carry,
                batches[1].nodes.clone()
            ),
            (CollisionPolicy::Overwrite, true, vec!["Line/P".to_string()])
        );
        // The body is stripped; the value is filled at send time.
        assert_eq!(
            body(&batches[1]),
            json!({"tags": [{"name": "P", "tagType": "AtomicTag", "valueSource": MEMORY}]})
        );
    }

    /// Choice (c) `merge-default`: the same plan is refused before any write,
    /// naming the node and the property, never a value.
    #[test]
    fn a_dropped_property_is_refused_under_merge_default() {
        let repo = repo(&[
            ("Line", json!({"tagType": "Folder"})),
            (
                "Line/P",
                json!({"tagType": "AtomicTag", "valueSource": MEMORY, "value": 1}),
            ),
        ]);
        let live = live(json!([{"name": "Line", "tagType": "Folder", "tags": [
            {"name": "P", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 9,
             "tooltip": "tip-sentinel"}
        ]}]));
        let err = batches(&repo, &live, ExistingNodePolicy::MergeStripped).expect_err("refused");
        assert_eq!(err.code(), "deploy_tree_invalid");
        let text = err.to_string();
        assert!(
            text.contains("Line/P") && text.contains("tooltip"),
            "{text}"
        );
        assert!(!text.contains("sentinel"), "{text}");
    }

    // -- carry_live_values ---------------------------------------------------

    fn fresh_with_type() -> BTreeMap<String, Value> {
        live(json!([
            {"name": "_types_", "tagType": "Folder", "tags": [
                {"name": "Ty", "tagType": "UdtType", "tags": [
                    {"name": "M1", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 0},
                    {"name": "M2", "tagType": "AtomicTag", "valueSource": "opc"}
                ]}
            ]},
            {"name": "Line", "tagType": "Folder", "tags": [
                {"name": "V", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 9,
                 "timestamp": 5, "quality": "Good"},
                {"name": "N", "tagType": "AtomicTag", "valueSource": MEMORY},
                {"name": "O", "tagType": "AtomicTag", "valueSource": "opc", "value": 7},
                {"name": "I", "tagType": "UdtInstance", "typeId": "Ty", "tags": [
                    {"name": "M1", "tagType": "AtomicTag", "value": 5},
                    {"name": "M2", "tagType": "AtomicTag", "value": 6}
                ]}
            ]}
        ]))
    }

    #[test]
    fn carry_sets_a_memory_tags_live_value_and_drops_timestamp_and_quality() {
        let mut node = json!({"name": "V", "tagType": "AtomicTag", "valueSource": MEMORY,
                              "value": 1, "timestamp": 2, "quality": "Bad"});
        carry_live_values(&mut node, "Line/V", &fresh_with_type());
        assert_eq!(
            node,
            json!({"name": "V", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 9})
        );
    }

    #[test]
    fn carry_adds_no_value_where_the_export_has_none() {
        let mut node =
            json!({"name": "N", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 1});
        carry_live_values(&mut node, "Line/N", &fresh_with_type());
        assert_eq!(
            node,
            json!({"name": "N", "tagType": "AtomicTag", "valueSource": MEMORY})
        );
        // A node the export lacks altogether.
        let mut gone = json!({"tagType": "AtomicTag", "valueSource": MEMORY, "value": 1});
        carry_live_values(&mut gone, "Line/Absent", &fresh_with_type());
        assert_eq!(gone, json!({"tagType": "AtomicTag", "valueSource": MEMORY}));
    }

    /// KL-5: only memory-sourced values are carried; an `opc` tag, any other
    /// named source and a source that is not a name are sent stripped.
    #[test]
    fn carry_leaves_other_value_sources_stripped() {
        let mut node = json!({"tagType": "AtomicTag", "valueSource": "opc", "value": 1});
        carry_live_values(&mut node, "Line/O", &fresh_with_type());
        assert_eq!(node, json!({"tagType": "AtomicTag", "valueSource": "opc"}));
        for source in ["expr", "db", "query", "reference", "derived", "someday"] {
            let mut other = json!({"tagType": "AtomicTag", "valueSource": source, "value": 1});
            carry_live_values(&mut other, "Line/V", &fresh_with_type());
            assert!(other.get("value").is_none(), "{other}");
        }
        let bound = json!({"bindType": "parameter", "binding": "{src}"});
        let mut unresolved = json!({"tagType": "AtomicTag", "valueSource": bound, "value": 1});
        carry_live_values(&mut unresolved, "Line/V", &fresh_with_type());
        assert!(unresolved.get("value").is_none(), "{unresolved}");
    }

    /// 26-15 O1: a tag with no `valueSource` key is a memory tag (Ignition's
    /// default), so its live value is carried.
    #[test]
    fn carry_takes_a_tag_without_value_source_as_memory() {
        let mut unsourced = json!({"tagType": "AtomicTag", "value": 1, "quality": "Bad"});
        carry_live_values(&mut unsourced, "Line/V", &fresh_with_type());
        assert_eq!(unsourced, json!({"tagType": "AtomicTag", "value": 9}));
    }

    /// 26-15 O1: an instance member override that names no `valueSource`,
    /// of a type member that names none either, is a memory member: its live
    /// value is carried, and the guard passes it.
    #[test]
    fn carry_takes_a_member_override_without_value_source_as_memory() {
        let fresh = live(json!([
            {"name": "_types_", "tagType": "Folder", "tags": [
                {"name": "Ty", "tagType": "UdtType", "tags": [
                    {"name": "M", "tagType": "AtomicTag", "value": 0}
                ]}
            ]},
            {"name": "Line", "tagType": "Folder", "tags": [
                {"name": "D", "tagType": "UdtInstance", "typeId": "Ty", "tags": [
                    {"name": "M", "tagType": "AtomicTag", "value": 5, "documentation": "d"}
                ]}
            ]}
        ]));
        let mut node = json!({"tagType": "UdtInstance", "typeId": "Ty", "tags": [
            {"name": "M", "tagType": "AtomicTag", "documentation": "d"}
        ]});
        carry_guard("Line/D", &node, &fresh).expect("a memory member");
        carry_live_values(&mut node, "Line/D", &fresh);
        assert_eq!(node["tags"][0]["value"], 5, "{node}");
    }

    /// An instance member takes its value source from the type member,
    /// resolved from the export's `_types_`: M1 (memory) carries its live 5,
    /// M2 (opc) is stripped, and a member override of `valueSource` wins.
    #[test]
    fn carry_resolves_instance_members_through_the_type() {
        let mut node = json!({"tagType": "UdtInstance", "typeId": "Ty",
        "parameters": {"P1": {"dataType": "Integer", "value": 43}},
        "tags": [
            {"name": "M1", "tagType": "AtomicTag", "value": 1, "quality": "Good"},
            {"name": "M2", "tagType": "AtomicTag", "value": 2},
            {"name": "M3", "tagType": "AtomicTag", "valueSource": MEMORY}
        ]});
        carry_live_values(&mut node, "Line/I", &fresh_with_type());
        assert_eq!(
            node,
            json!({"tagType": "UdtInstance", "typeId": "Ty",
            "parameters": {"P1": {"dataType": "Integer", "value": 43}},
            "tags": [
                {"name": "M1", "tagType": "AtomicTag", "value": 5},
                {"name": "M2", "tagType": "AtomicTag"},
                {"name": "M3", "tagType": "AtomicTag", "valueSource": MEMORY}
            ]})
        );
        // A member that overrides the type's opc source with memory carries.
        let mut over = json!({"tagType": "UdtInstance", "typeId": "Ty", "tags": [
            {"name": "M2", "tagType": "AtomicTag", "valueSource": MEMORY}
        ]});
        carry_live_values(&mut over, "Line/I", &fresh_with_type());
        assert_eq!(over["tags"][0]["value"], 6);
    }

    #[test]
    fn a_carried_batch_carries_from_the_fresh_export() {
        let repo = repo(&[
            ("Line", json!({"tagType": "Folder"})),
            (
                "Line/V",
                json!({"tagType": "AtomicTag", "valueSource": MEMORY, "value": 1}),
            ),
        ]);
        let plan_time = live(json!([{"name": "Line", "tagType": "Folder", "tags": [
            {"name": "V", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 9, "tooltip": "t"}
        ]}]));
        let batches =
            batches(&repo, &plan_time, ExistingNodePolicy::OverwriteCarry).expect("plans");
        let fresh = live(json!([{"name": "Line", "tagType": "Folder", "tags": [
            {"name": "V", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 11, "tooltip": "t"}
        ]}]));
        let Carried::Body(bytes) = carried(&batches[0], &fresh).expect("carried") else {
            panic!("one node is never split");
        };
        let sent: Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(
            sent,
            json!({"tags": [{"name": "V", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 11}]})
        );
    }

    // -- stripping is strip_runtime_values only ------------------------------

    /// Nothing under `_types_/`, no `defaultValue`, no `parameters` value and
    /// no member override configuration is removed from any body.
    #[test]
    fn stripping_never_removes_configuration() {
        let repo = repo(&[
            (
                "_types_/Ty",
                json!({"tagType": "UdtType",
                       "parameters": {"P": {"dataType": "Integer", "value": 1}},
                       "tags": [{"name": "M", "tagType": "AtomicTag", "valueSource": MEMORY,
                                 "value": 2, "timestamp": 3, "quality": "Good"}]}),
            ),
            ("Line", json!({"tagType": "Folder"})),
            (
                "Line/T",
                json!({"tagType": "AtomicTag", "valueSource": MEMORY, "value": 1,
                       "defaultValue": {"k": "v"}, "documentation": "new"}),
            ),
            (
                "Line/I",
                json!({"tagType": "UdtInstance", "typeId": "Ty",
                       "parameters": {"P": {"dataType": "Integer", "value": 5}},
                       "tags": [{"name": "M", "tagType": "AtomicTag", "value": 3,
                                 "documentation": "over", "defaultValue": 4}]}),
            ),
        ]);
        let live = live(json!([
            {"name": "_types_", "tagType": "Folder", "tags": [
                {"name": "Ty", "tagType": "UdtType",
                 "parameters": {"P": {"dataType": "Integer", "value": 0}},
                 "tags": [{"name": "M", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 2}]}
            ]},
            {"name": "Line", "tagType": "Folder", "tags": [
                {"name": "T", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 8,
                 "defaultValue": {"k": "v"}, "documentation": "old"},
                {"name": "I", "tagType": "UdtInstance", "typeId": "Ty",
                 "parameters": {"P": {"dataType": "Integer", "value": 6}},
                 "tags": [{"name": "M", "tagType": "AtomicTag", "value": 3,
                           "documentation": "over", "defaultValue": 4}]}
            ]}
        ]));
        let batches = batches(&repo, &live, ExistingNodePolicy::MergeStripped).expect("plans");
        assert_eq!(batches.len(), 2, "{batches:?}");
        let ty = body(&batches[0]);
        let ty = &ty["tags"][0]["tags"][0];
        assert_eq!(
            ty["tags"][0]["value"], 2,
            "a type member's value is its default"
        );
        assert_eq!(
            ty["tags"][0]["timestamp"], 3,
            "nothing below _types_/ is stripped"
        );
        assert_eq!(ty["parameters"]["P"]["value"], 1);
        let tags = body(&batches[1]);
        let instance = &tags["tags"][0];
        assert_eq!(instance["parameters"]["P"]["value"], 5);
        assert_eq!(instance["tags"][0]["documentation"], "over");
        assert_eq!(instance["tags"][0]["defaultValue"], 4);
        assert!(instance["tags"][0].get("value").is_none());
        let tag = &tags["tags"][1];
        assert_eq!(tag["defaultValue"], json!({"k": "v"}));
        assert!(tag.get("value").is_none());
    }

    // -- D-01: live-only nodes never travel ----------------------------------

    /// A generated tree: every folder holds repo nodes (added, changed, same)
    /// and live-only siblings, nested three deep. No body names a drift path,
    /// and every body path is a planned node or a folder on the way to one.
    #[test]
    fn no_body_ever_names_a_drift_path() {
        let mut repo_nodes: Vec<(String, Value)> = Vec::new();
        fn level(depth: usize, prefix: &str, repo_nodes: &mut Vec<(String, Value)>) -> Vec<Value> {
            let mut live = Vec::new();
            for i in 0..3 {
                let join = |name: &str| {
                    if prefix.is_empty() {
                        name.to_string()
                    } else {
                        format!("{prefix}/{name}")
                    }
                };
                // Added in the repo only.
                repo_nodes.push((
                    join(&format!("Add{i}")),
                    json!({"tagType": "AtomicTag", "valueSource": "memory", "value": i}),
                ));
                // Changed: documentation differs.
                repo_nodes.push((
                    join(&format!("Chg{i}")),
                    json!({"tagType": "AtomicTag", "valueSource": "memory", "documentation": "new"}),
                ));
                live.push(json!({"name": format!("Chg{i}"), "tagType": "AtomicTag",
                                 "valueSource": "memory", "value": 7, "documentation": "old"}));
                // Same on both sides.
                repo_nodes.push((
                    join(&format!("Same{i}")),
                    json!({"tagType": "AtomicTag", "valueSource": "memory"}),
                ));
                live.push(json!({"name": format!("Same{i}"), "tagType": "AtomicTag",
                                 "valueSource": "memory"}));
                // Live only: drift.
                live.push(json!({"name": format!("Drift{i}"), "tagType": "AtomicTag",
                                 "valueSource": "memory", "value": 1}));
                live.push(
                    json!({"name": format!("DriftDir{i}"), "tagType": "Folder", "tags": [
                        {"name": "X", "tagType": "AtomicTag"}
                    ]}),
                );
            }
            if depth < 3 {
                let name = format!("D{depth}");
                let path = if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{prefix}/{name}")
                };
                repo_nodes.push((path.clone(), json!({"tagType": "Folder"})));
                let kids = level(depth + 1, &path, repo_nodes);
                live.push(json!({"name": name, "tagType": "Folder", "tags": kids}));
            }
            live
        }
        let live_tags = level(0, "", &mut repo_nodes);
        let refs: Vec<(&str, Value)> = repo_nodes
            .iter()
            .map(|(p, v)| (p.as_str(), v.clone()))
            .collect();
        let repo = repo(&refs);
        let live = live(Value::Array(live_tags));
        let plan = plan_provider(&repo, &live, None);
        assert!(plan.drift.len() >= 24, "{:?}", plan.drift);
        let batches =
            import_batches(&plan, &repo, &live, ExistingNodePolicy::OverwriteCarry).expect("plans");
        let mut named = 0;
        for batch in &batches {
            for path in body_paths(batch) {
                named += 1;
                assert!(
                    !plan.drift.contains(&path),
                    "drift path {path} in a body: {batch:?}"
                );
                assert!(
                    repo.nodes.contains_key(&path),
                    "{path} is not a repo node: {batch:?}"
                );
            }
            for path in &batch.nodes {
                assert!(
                    plan.added.contains(path) || plan.changed.iter().any(|c| &c.path == path),
                    "{path} is not planned"
                );
            }
        }
        assert_eq!(named, plan.added.len() + plan.changed.len());
    }

    // -- R3-BATCH: the size cap ---------------------------------------------

    fn big(i: usize, pad: usize) -> Value {
        json!({"tagType": "AtomicTag", "valueSource": "memory", "value": i,
               "documentation": "x".repeat(pad)})
    }

    #[test]
    fn no_body_exceeds_the_cap() {
        let mut nodes: Vec<(String, Value)> =
            vec![("Line".to_string(), json!({"tagType": "Folder"}))];
        for i in 0..40 {
            nodes.push((format!("Line/T{i:02}"), big(i, 40_000)));
        }
        let refs: Vec<(&str, Value)> = nodes.iter().map(|(p, v)| (p.as_str(), v.clone())).collect();
        let repo = repo(&refs);
        let live = live(json!([{"name": "Line", "tagType": "Folder", "tags": []}]));
        let batches = batches(&repo, &live, ExistingNodePolicy::OverwriteCarry).expect("plans");
        assert!(batches.len() >= 4, "{}", batches.len());
        let mut carried_nodes = 0;
        for batch in &batches {
            assert!(
                batch.body.len() <= IMPORT_BATCH_MAX_BYTES,
                "{} bytes in a batch of {} nodes",
                batch.body.len(),
                batch.nodes.len()
            );
            carried_nodes += batch.nodes.len();
        }
        assert_eq!(carried_nodes, 40);
    }

    /// Every body is under the cap, every node of `added` is named by
    /// exactly one batch, and every batch goes below a path that is live, the
    /// root, or a folder an earlier batch created (parents before children).
    fn assert_split(batches: &[ImportBatch], live: &BTreeMap<String, Value>, added: &[&str]) {
        let mut created: BTreeSet<String> = BTreeSet::new();
        let mut named: Vec<String> = Vec::new();
        for batch in batches {
            assert!(
                batch.body.len() <= IMPORT_BATCH_MAX_BYTES,
                "{} bytes at {} ({} nodes)",
                batch.body.len(),
                batch.path,
                batch.nodes.len()
            );
            assert!(
                batch.path.is_empty()
                    || live.contains_key(&batch.path)
                    || created.contains(&batch.path),
                "{} is sent before its parent exists",
                batch.path
            );
            let mut paths = body_paths(batch);
            paths.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            assert_eq!(paths, batch.nodes, "a batch names what its body holds");
            created.extend(batch.nodes.iter().cloned());
            named.extend(batch.nodes.iter().cloned());
        }
        named.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        let mut want: Vec<String> = added.iter().map(|p| (*p).to_string()).collect();
        want.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        assert_eq!(named, want, "each added node exactly once");
    }

    /// A chain of new folders over the cap: each goes alone at its parent
    /// path, then its children below it (C1d: under `Abort` an existing
    /// folder takes new children), down to the level that fits.
    fn deep_repo() -> (NativeProvider, Vec<String>) {
        let mut nodes: Vec<(String, Value)> = vec![
            ("Line".to_string(), json!({"tagType": "Folder"})),
            ("Line/New".to_string(), json!({"tagType": "Folder"})),
            ("Line/New/Sub".to_string(), json!({"tagType": "Folder"})),
            (
                "Line/New/Sub/Deep".to_string(),
                json!({"tagType": "Folder"}),
            ),
            (
                "Line/New/Side".to_string(),
                json!({"tagType": "AtomicTag", "valueSource": MEMORY, "value": 1}),
            ),
        ];
        for i in 0..20 {
            nodes.push((format!("Line/New/Sub/Deep/T{i:02}"), big(i, 40_000)));
        }
        let refs: Vec<(&str, Value)> = nodes.iter().map(|(p, v)| (p.as_str(), v.clone())).collect();
        let added = nodes
            .iter()
            .map(|(p, _)| p.clone())
            .filter(|p| p != "Line")
            .collect();
        (repo(&refs), added)
    }

    #[test]
    fn a_folder_over_the_cap_goes_alone_then_its_children_below_it() {
        let (repo, added) = deep_repo();
        let live = live(json!([{"name": "Line", "tagType": "Folder", "tags": []}]));
        let batches = batches(&repo, &live, ExistingNodePolicy::OverwriteCarry).expect("plans");
        let added: Vec<&str> = added.iter().map(String::as_str).collect();
        assert_split(&batches, &live, &added);
        assert!(batches.iter().all(|b| b.policy == CollisionPolicy::Abort));
        assert_eq!(
            (batches[0].path.as_str(), batches[0].nodes.as_slice()),
            ("Line", ["Line/New".to_string()].as_slice())
        );
        assert_eq!(
            body(&batches[0]),
            json!({"tags": [{"name": "New", "tagType": "Folder"}]}),
            "the folder alone, no children"
        );
        // Below it: the side tag beside Sub, which goes alone again.
        assert_eq!(batches[1].path, "Line/New");
        assert_eq!(batches[1].nodes, ["Line/New/Side", "Line/New/Sub"]);
        assert_eq!(
            body(&batches[1]),
            json!({"tags": [
                {"name": "Side", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 1},
                {"name": "Sub", "tagType": "Folder"}
            ]})
        );
        assert_eq!(batches[2].path, "Line/New/Sub");
        assert_eq!(batches[2].nodes, ["Line/New/Sub/Deep"]);
        let leaves: Vec<&ImportBatch> = batches[3..].iter().collect();
        assert!(leaves.len() >= 2, "{}", leaves.len());
        assert!(leaves.iter().all(|b| b.path == "Line/New/Sub/Deep"));
    }

    /// The same chain adopted into an empty provider: the top folder goes
    /// alone at the root.
    #[test]
    fn an_adoption_into_an_empty_provider_splits_from_the_root() {
        let (repo, mut added) = deep_repo();
        added.push("Line".to_string());
        let live = live(json!([{"name": "_types_", "tagType": "Folder", "tags": []}]));
        let batches = batches(&repo, &live, ExistingNodePolicy::OverwriteCarry).expect("plans");
        let added: Vec<&str> = added.iter().map(String::as_str).collect();
        assert_split(&batches, &live, &added);
        assert_eq!(
            (batches[0].path.as_str(), body(&batches[0])),
            ("", json!({"tags": [{"name": "Line", "tagType": "Folder"}]}))
        );
    }

    fn refused_text(err: &CoreError) -> String {
        assert_eq!(err.code(), "deploy_tree_invalid", "{err}");
        match err {
            CoreError::DeployTreeInvalid(details) => details
                .findings
                .iter()
                .map(|f| format!("{}: {}", f.file, f.message))
                .collect::<Vec<_>>()
                .join("\n"),
            other => panic!("not deploy_tree_invalid: {other}"),
        }
    }

    /// A new folder whose one child alone is over the cap: the child cannot
    /// be split, so the plan is refused naming it and its size, and nothing
    /// is built.
    #[test]
    fn a_folder_whose_single_child_is_over_the_cap_is_refused_naming_the_child() {
        let huge = big(0, IMPORT_BATCH_MAX_BYTES + 10);
        let size = json_len(&named(&huge, "Huge")) + BODY_FRAME;
        let repo = repo(&[
            ("Line", json!({"tagType": "Folder"})),
            ("Line/New", json!({"tagType": "Folder"})),
            ("Line/New/Huge", huge),
        ]);
        let live = live(json!([{"name": "Line", "tagType": "Folder", "tags": []}]));
        let err = batches(&repo, &live, ExistingNodePolicy::OverwriteCarry)
            .expect_err("an unsplittable node over the cap");
        let text = refused_text(&err);
        assert!(text.contains("tags/P/Line/New/Huge.json"), "{text}");
        assert!(text.contains("[P]Line/New/Huge"), "{text}");
        assert!(text.contains(&size.to_string()), "{text}");
        assert!(text.contains("524288"), "{text}");
        assert!(!text.contains("[P]Line/New "), "the folder splits: {text}");
    }

    /// A single tag, UDT instance, changed tag or type over the cap is
    /// refused before any write, every one named.
    #[test]
    fn an_unsplittable_node_over_the_cap_is_refused() {
        let pad = "x".repeat(IMPORT_BATCH_MAX_BYTES);
        let repo = repo(&[
            ("Line", json!({"tagType": "Folder"})),
            ("Line/Huge", big(0, IMPORT_BATCH_MAX_BYTES + 10)),
            (
                "Line/Inst",
                json!({"tagType": "UdtInstance", "typeId": "M", "documentation": pad}),
            ),
            (
                "Line/Old",
                json!({"tagType": "AtomicTag", "valueSource": MEMORY, "documentation": pad}),
            ),
            (
                "_types_/Big",
                json!({"tagType": "UdtType", "documentation": pad}),
            ),
        ]);
        let live = live(json!([
            {"name": "_types_", "tagType": "Folder", "tags": []},
            {"name": "Line", "tagType": "Folder", "tags": [
                {"name": "Old", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 1}
            ]}
        ]));
        let err = batches(&repo, &live, ExistingNodePolicy::OverwriteCarry)
            .expect_err("nodes over the cap");
        let text = refused_text(&err);
        for path in ["Line/Huge", "Line/Inst", "Line/Old", "_types_/Big"] {
            assert!(text.contains(&format!("[P]{path} ")), "{path}: {text}");
        }
        assert!(text.contains("nothing was written"), "{text}");
    }

    /// A carried batch is measured after the carry: values that only the
    /// live side holds still keep it under the cap, and a fresh export whose
    /// values grew splits the batch at send time.
    #[test]
    fn a_carried_batch_is_measured_after_the_carry() {
        let mut nodes: Vec<(String, Value)> =
            vec![("Line".to_string(), json!({"tagType": "Folder"}))];
        let mut live_kids = Vec::new();
        for i in 0..4 {
            nodes.push((
                format!("Line/S{i}"),
                json!({"tagType": "AtomicTag", "valueSource": "memory", "dataType": "String"}),
            ));
            live_kids.push(json!({"name": format!("S{i}"), "tagType": "AtomicTag",
                "valueSource": "memory", "dataType": "String", "tooltip": "t",
                "value": "v".repeat(200_000)}));
        }
        let refs: Vec<(&str, Value)> = nodes.iter().map(|(p, v)| (p.as_str(), v.clone())).collect();
        let repo = repo(&refs);
        let plan_time = live(json!([{"name": "Line", "tagType": "Folder", "tags": live_kids}]));
        let batches =
            batches(&repo, &plan_time, ExistingNodePolicy::OverwriteCarry).expect("plans");
        assert!(batches.iter().all(|b| b.carry));
        assert!(batches.len() >= 2, "carried values count: {batches:?}");
        for batch in &batches {
            let Carried::Body(bytes) = carried(batch, &plan_time).expect("carried") else {
                panic!("fits at plan time");
            };
            assert!(bytes.len() <= IMPORT_BATCH_MAX_BYTES || batch.nodes.len() == 1);
        }
        // Values grown between plan and send: the two-node batch splits.
        let grown: Vec<Value> = (0..4)
            .map(|i| {
                json!({"name": format!("S{i}"), "tagType": "AtomicTag",
                "valueSource": "memory", "dataType": "String", "tooltip": "t",
                "value": "v".repeat(400_000)})
            })
            .collect();
        let fresh = live(json!([{"name": "Line", "tagType": "Folder", "tags": grown}]));
        let two = batches
            .iter()
            .find(|b| b.nodes.len() >= 2)
            .expect("a batch of two");
        match carried(two, &fresh).expect("carried") {
            Carried::Split(parts) => {
                assert!(parts.len() >= 2);
                assert!(
                    parts
                        .iter()
                        .all(|p| p.carry && p.policy == CollisionPolicy::Overwrite)
                );
                let names: usize = parts.iter().map(|p| p.nodes.len()).sum();
                assert_eq!(names, two.nodes.len());
            }
            Carried::Body(_) => panic!("over the cap with two nodes"),
        }
        // 26-REVIEW WR-03: a one-node batch over the cap once its value is
        // carried is refused, never sent over the cap.
        let huge: Vec<Value> = (0..4)
            .map(|i| {
                json!({"name": format!("S{i}"), "tagType": "AtomicTag",
                "valueSource": "memory", "dataType": "String", "tooltip": "t",
                "value": "v".repeat(600_000)})
            })
            .collect();
        let fresh = live(json!([{"name": "Line", "tagType": "Folder", "tags": huge}]));
        let one = ImportBatch {
            nodes: vec!["Line/S0".to_string()],
            body: body_bytes(vec![json!({"name": "S0", "tagType": "AtomicTag",
                "valueSource": "memory", "dataType": "String"})])
            .expect("body"),
            ..two.clone()
        };
        let err = carried(&one, &fresh).expect_err("over the cap");
        assert_eq!(err.code(), "deploy_tree_invalid", "{err}");
        assert!(err.to_string().contains("[P]Line/S0"), "{err}");
    }

    // -- rollback_plan (26-11, R3-RB, R3-DEL) --------------------------------

    /// What a run that sent every batch of `repo` against `pre` names as
    /// possibly applied.
    fn all_sent(repo: &NativeProvider, pre: &BTreeMap<String, Value>) -> AppliedSteps {
        let mut applied = AppliedSteps::default();
        for batch in batches(repo, pre, ExistingNodePolicy::OverwriteCarry).expect("plans") {
            sent_from(&mut applied, &batch, repo);
        }
        applied
    }

    /// `batch` was sent, with the repo's node at each of its paths as the
    /// node the run sent there (what the run records, 26-REVIEW CR-01).
    fn sent_from(applied: &mut AppliedSteps, batch: &ImportBatch, repo: &NativeProvider) {
        applied.sent(batch);
        for path in &batch.nodes {
            if let Some(node) = repo.nodes.get(path) {
                applied.bodies.insert(path.clone(), node.json.clone());
            }
        }
    }

    fn rollback(
        repo: &NativeProvider,
        pre: &BTreeMap<String, Value>,
        current: &BTreeMap<String, Value>,
        applied: &AppliedSteps,
        provider_created: bool,
    ) -> RollbackPlan {
        let plan = plan_provider(repo, pre, None);
        rollback_plan(
            &plan,
            pre,
            current,
            applied,
            provider_created,
            &BTreeSet::new(),
            None,
            TAG_DELETE_DESCENDANTS,
        )
        .expect("a rollback plan")
    }

    /// A folder created empty and then filled: whatever prefix of the split
    /// batches was sent, the rollback names exactly the nodes those batches
    /// created and that are there now, undeletable in an existing provider
    /// and with the provider in a created one. It sends nothing.
    #[test]
    fn rollback_of_a_split_folder_names_exactly_what_the_run_created() {
        let (repo, _) = deep_repo();
        let pre = live(json!([{"name": "Line", "tagType": "Folder", "tags": []}]));
        let split = batches(&repo, &pre, ExistingNodePolicy::OverwriteCarry).expect("plans");
        assert!(split.len() >= 5, "{}", split.len());
        for sent in 1..=split.len() {
            let mut applied = AppliedSteps::default();
            let mut current = pre.clone();
            let mut want: Vec<String> = Vec::new();
            for batch in &split[..sent] {
                sent_from(&mut applied, batch, &repo);
                // The last batch sent answered a failure and applied nothing.
                if sent == split.len() || !std::ptr::eq(batch, &split[sent - 1]) {
                    for path in &batch.nodes {
                        current.insert(path.clone(), repo.nodes[path].json.clone());
                        want.push(path.clone());
                    }
                }
            }
            want.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            let undo = rollback(&repo, &pre, &current, &applied, false);
            assert_eq!(undo.undeletable, want, "{sent} batch(es) sent");
            assert!(undo.imports.is_empty() && undo.provider_deletes.is_empty());
            let undo = rollback(&repo, &pre, &current, &applied, true);
            assert_eq!(undo.provider_deletes, want, "{sent} batch(es) sent");
            assert!(undo.imports.is_empty() && undo.undeletable.is_empty());
        }
        // Only the folder batch sent and applied: the empty folder alone.
        let mut applied = AppliedSteps::default();
        sent_from(&mut applied, &split[0], &repo);
        let mut current = pre.clone();
        current.insert("Line/New".to_string(), json!({"tagType": "Folder"}));
        let undo = rollback(&repo, &pre, &current, &applied, false);
        assert_eq!(undo.undeletable, ["Line/New"]);
    }

    /// R3-DEL: a node the run created cannot be deleted; it is named in
    /// `undeletable` and never sent.
    #[test]
    fn rollback_lists_a_created_node_undeletable_and_sends_nothing() {
        let repo = repo(&[
            ("Line", json!({"tagType": "Folder"})),
            (
                "Line/New",
                json!({"tagType": "AtomicTag", "valueSource": MEMORY, "value": 1}),
            ),
        ]);
        let pre = live(json!([{"name": "Line", "tagType": "Folder", "tags": []}]));
        let current = live(json!([{"name": "Line", "tagType": "Folder", "tags": [
            {"name": "New", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 1, "defaultValue": 1}
        ]}]));
        let applied = all_sent(&repo, &pre);
        let plan = rollback(&repo, &pre, &current, &applied, false);
        assert_eq!(plan.provider, "P");
        assert!(plan.imports.is_empty(), "{:?}", plan.imports);
        assert_eq!(plan.undeletable, ["Line/New"]);
        assert!(plan.provider_deletes.is_empty());
        assert!(plan.carried.is_empty() && plan.uncarriable.is_empty());
    }

    /// A created node of a batch that never applied (absent now) needs no
    /// undo.
    #[test]
    fn rollback_ignores_a_created_node_that_is_not_there() {
        let repo = repo(&[
            ("Line", json!({"tagType": "Folder"})),
            (
                "Line/New",
                json!({"tagType": "AtomicTag", "valueSource": MEMORY, "value": 1}),
            ),
        ]);
        let pre = live(json!([{"name": "Line", "tagType": "Folder", "tags": []}]));
        let applied = all_sent(&repo, &pre);
        let plan = rollback(&repo, &pre, &pre, &applied, false);
        assert!(plan.imports.is_empty() && plan.undeletable.is_empty());
    }

    /// In a provider the run created, every created node goes with the
    /// provider (`provider_deletes`), not `undeletable`.
    #[test]
    fn rollback_of_a_created_provider_lists_provider_deletes() {
        let repo = repo(&[
            ("Line", json!({"tagType": "Folder"})),
            (
                "Line/New",
                json!({"tagType": "AtomicTag", "valueSource": MEMORY, "value": 1}),
            ),
        ]);
        let pre = BTreeMap::new();
        let current = live(json!([{"name": "Line", "tagType": "Folder", "tags": [
            {"name": "New", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 1}
        ]}]));
        let applied = all_sent(&repo, &pre);
        let plan = rollback(&repo, &pre, &current, &applied, true);
        assert!(plan.imports.is_empty());
        assert!(plan.undeletable.is_empty(), "{:?}", plan.undeletable);
        assert_eq!(plan.provider_deletes, ["Line", "Line/New"]);
    }

    /// R3-RB: a changed node that gained no property is restored by
    /// `MergeOverwrite` of its stripped pre-image: the modified property and
    /// the removed one come back, `defaultValue` is sent, no value is.
    #[test]
    fn rollback_merges_the_stripped_pre_image_when_nothing_was_added() {
        let repo = repo(&[
            ("Line", json!({"tagType": "Folder"})),
            (
                "Line/T",
                json!({"tagType": "AtomicTag", "valueSource": MEMORY, "value": 3,
                       "defaultValue": 3, "documentation": "new"}),
            ),
        ]);
        let pre = live(json!([{"name": "Line", "tagType": "Folder", "tags": [
            {"name": "T", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 9,
             "defaultValue": 3, "documentation": "old", "tooltip": "tip"}
        ]}]));
        let current = live(json!([{"name": "Line", "tagType": "Folder", "tags": [
            {"name": "T", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 12,
             "defaultValue": 3, "documentation": "new"}
        ]}]));
        let applied = all_sent(&repo, &pre);
        let plan = rollback(&repo, &pre, &current, &applied, false);
        assert_eq!(plan.imports.len(), 1, "{:?}", plan.imports);
        let batch = &plan.imports[0];
        assert_eq!(
            (batch.path.as_str(), batch.policy, batch.carry),
            ("Line", CollisionPolicy::MergeOverwrite, false)
        );
        assert_eq!(batch.nodes, ["Line/T"]);
        assert_eq!(
            body(batch),
            json!({"tags": [{"name": "T", "tagType": "AtomicTag", "valueSource": MEMORY,
                             "defaultValue": 3, "documentation": "old", "tooltip": "tip"}]})
        );
        assert!(body(batch)["tags"][0].get("value").is_none(), "no value");
        assert!(plan.carried.is_empty() && plan.undeletable.is_empty());
    }

    /// 26-12: a changed tag that is gone now is re-created by `Abort` of its
    /// stripped pre-image only when the pre-run tag record lists it; without
    /// that it is left to the verify, as in the automatic rollback.
    #[test]
    fn rollback_recreates_a_removed_node_only_when_the_record_lists_it() {
        let repo = repo(&[
            ("Line", json!({"tagType": "Folder"})),
            (
                "Line/T",
                json!({"tagType": "AtomicTag", "valueSource": MEMORY, "value": 3,
                       "defaultValue": 3, "documentation": "new"}),
            ),
        ]);
        let pre = live(json!([{"name": "Line", "tagType": "Folder", "tags": [
            {"name": "T", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 9,
             "defaultValue": 3, "documentation": "old"}
        ]}]));
        let current = live(json!([{"name": "Line", "tagType": "Folder", "tags": []}]));
        let applied = all_sent(&repo, &pre);
        let plan = plan_provider(&repo, &pre, None);

        let left = rollback_plan(
            &plan,
            &pre,
            &current,
            &applied,
            false,
            &BTreeSet::new(),
            None,
            TAG_DELETE_DESCENDANTS,
        )
        .expect("a rollback plan");
        assert!(left.imports.is_empty() && left.recreated.is_empty());

        let listed = BTreeSet::from(["Line/T".to_string()]);
        let undo = rollback_plan(
            &plan,
            &pre,
            &current,
            &applied,
            false,
            &listed,
            None,
            TAG_DELETE_DESCENDANTS,
        )
        .expect("a plan");
        assert_eq!(undo.recreated, ["Line/T"]);
        assert_eq!(undo.imports.len(), 1, "{:?}", undo.imports);
        let batch = &undo.imports[0];
        assert_eq!(
            (batch.path.as_str(), batch.policy, batch.carry),
            ("Line", CollisionPolicy::Abort, false)
        );
        assert_eq!(
            body(batch),
            json!({"tags": [{"name": "T", "tagType": "AtomicTag", "valueSource": MEMORY,
                             "defaultValue": 3, "documentation": "old"}]}),
            "stripped: no pre-run value is written"
        );
    }

    /// R3-RB, R3-CARRY, F-P4: a changed node that gained a property goes back
    /// by `Overwrite` of the pre-image with `carry`. Sent stripped, the tag
    /// would come back holding its `defaultValue` (5) and the member would
    /// lose its value; carried, both hold the fresh export's values (never
    /// the pre-image's).
    #[test]
    fn rollback_carries_fresh_values_when_the_run_added_a_property() {
        let ty = json!({"name": "_types_", "tagType": "Folder", "tags": [
            {"name": "Ty", "tagType": "UdtType", "tags": [
                {"name": "M1", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 0}
            ]}
        ]});
        let repo = repo(&[
            ("Line", json!({"tagType": "Folder"})),
            (
                "Line/V",
                json!({"tagType": "AtomicTag", "valueSource": MEMORY, "value": 1,
                       "defaultValue": 5, "tooltip": "tip"}),
            ),
            (
                "Line/I",
                json!({"tagType": "UdtInstance", "typeId": "Ty",
                       "parameters": {"P1": {"dataType": "Integer", "value": 1}}}),
            ),
        ]);
        let pre = live(
            json!([ty.clone(), {"name": "Line", "tagType": "Folder", "tags": [
                {"name": "V", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 9, "defaultValue": 5},
                {"name": "I", "tagType": "UdtInstance", "typeId": "Ty",
                 "tags": [{"name": "M1", "tagType": "AtomicTag", "value": 4}]}
            ]}]),
        );
        let current = live(
            json!([ty.clone(), {"name": "Line", "tagType": "Folder", "tags": [
                {"name": "V", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 11,
                 "defaultValue": 5, "tooltip": "tip"},
                {"name": "I", "tagType": "UdtInstance", "typeId": "Ty",
                 "parameters": {"P1": {"dataType": "Integer", "value": 1}},
                 "tags": [{"name": "M1", "tagType": "AtomicTag", "value": 7}]}
            ]}]),
        );
        let applied = all_sent(&repo, &pre);
        let plan = rollback(&repo, &pre, &current, &applied, false);
        assert_eq!(plan.imports.len(), 1, "{:?}", plan.imports);
        let batch = &plan.imports[0];
        assert_eq!(
            (batch.path.as_str(), batch.policy, batch.carry),
            ("Line", CollisionPolicy::Overwrite, true)
        );
        assert_eq!(batch.nodes, ["Line/I", "Line/V"]);
        assert_eq!(plan.carried, ["Line/I", "Line/V"]);

        // The stripped form: no top-level value (the gateway would reset V to
        // its defaultValue 5) and no member value (M1 would lose it).
        let stripped = body(batch);
        let node = |b: &Value, name: &str| {
            b["tags"]
                .as_array()
                .expect("tags")
                .iter()
                .find(|n| n["name"] == name)
                .cloned()
                .expect("a node")
        };
        assert!(node(&stripped, "V").get("value").is_none());
        assert_eq!(node(&stripped, "V")["defaultValue"], 5);
        assert!(node(&stripped, "I")["tags"][0].get("value").is_none());
        assert!(
            node(&stripped, "V").get("tooltip").is_none(),
            "pre-image form"
        );
        assert!(
            node(&stripped, "I").get("parameters").is_none(),
            "pre-image form"
        );

        // Carried from an export read just before the batch: its values.
        let fresh = live(json!([ty, {"name": "Line", "tagType": "Folder", "tags": [
            {"name": "V", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 12,
             "defaultValue": 5, "tooltip": "tip"},
            {"name": "I", "tagType": "UdtInstance", "typeId": "Ty",
             "parameters": {"P1": {"dataType": "Integer", "value": 1}},
             "tags": [{"name": "M1", "tagType": "AtomicTag", "value": 8}]}
        ]}]));
        let Carried::Body(bytes) = carried(batch, &fresh).expect("carried") else {
            panic!("small batches are not split");
        };
        let sent: Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(
            node(&sent, "V")["value"],
            12,
            "fresh, not the pre-image's 9"
        );
        assert_eq!(node(&sent, "I")["tags"][0]["value"], 8, "fresh, not 4");
    }

    /// A changed type is restored whole from the pre-image (`Overwrite`,
    /// nothing carried, member values kept) in dependency order, before the
    /// tag restores.
    #[test]
    fn rollback_restores_changed_types_whole_first_in_dependency_order() {
        let repo = repo(&[
            (
                "_types_/Holder",
                json!({"tagType": "UdtType", "documentation": "new", "tags": [
                    {"name": "B", "tagType": "UdtInstance", "typeId": "Base"}
                ]}),
            ),
            (
                "_types_/Base",
                json!({"tagType": "UdtType", "documentation": "new", "tags": [
                    {"name": "M", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 2}
                ]}),
            ),
            ("Line", json!({"tagType": "Folder"})),
            (
                "Line/T",
                json!({"tagType": "AtomicTag", "valueSource": MEMORY, "documentation": "new"}),
            ),
        ]);
        let types = |doc: &str| {
            json!({"name": "_types_", "tagType": "Folder", "tags": [
                {"name": "Base", "tagType": "UdtType", "documentation": doc, "tags": [
                    {"name": "M", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 2}
                ]},
                {"name": "Holder", "tagType": "UdtType", "documentation": doc, "tags": [
                    {"name": "B", "tagType": "UdtInstance", "typeId": "Base"}
                ]}
            ]})
        };
        let line = |doc: &str| {
            json!({"name": "Line", "tagType": "Folder", "tags": [
                {"name": "T", "tagType": "AtomicTag", "valueSource": MEMORY, "documentation": doc}
            ]})
        };
        let pre = live(json!([types("old"), line("old")]));
        let current = live(json!([types("new"), line("new")]));
        let applied = all_sent(&repo, &pre);
        let plan = rollback(&repo, &pre, &current, &applied, false);
        let order: Vec<(&str, CollisionPolicy, &[String])> = plan
            .imports
            .iter()
            .map(|b| (b.path.as_str(), b.policy, b.nodes.as_slice()))
            .collect();
        assert_eq!(order.len(), 3, "{order:?}");
        assert_eq!(order[0].0, "");
        assert_eq!(order[0].2, ["_types_/Base"]);
        assert_eq!(order[1].2, ["_types_/Holder"]);
        assert!(
            plan.imports[..2]
                .iter()
                .all(|b| b.policy == CollisionPolicy::Overwrite && !b.carry)
        );
        assert_eq!(order[2].2, ["Line/T"]);
        assert_eq!(
            body(&plan.imports[0]),
            json!({"tags": [{"name": "_types_", "tagType": "Folder", "tags": [
                {"name": "Base", "tagType": "UdtType", "documentation": "old", "tags": [
                    {"name": "M", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 2}
                ]}
            ]}]})
        );
    }

    /// Only the steps that ran are undone: a changed node no sent batch named
    /// is never in a body, even when it differs now.
    #[test]
    fn rollback_undoes_only_the_batches_that_were_sent() {
        let repo = repo(&[
            ("Line", json!({"tagType": "Folder"})),
            (
                "Line/A",
                json!({"tagType": "AtomicTag", "valueSource": MEMORY, "documentation": "new"}),
            ),
            ("Other", json!({"tagType": "Folder"})),
            (
                "Other/B",
                json!({"tagType": "AtomicTag", "valueSource": MEMORY, "documentation": "new"}),
            ),
        ]);
        let tree = |a: &str, b: &str| {
            live(json!([
                {"name": "Line", "tagType": "Folder", "tags": [
                    {"name": "A", "tagType": "AtomicTag", "valueSource": MEMORY, "documentation": a}
                ]},
                {"name": "Other", "tagType": "Folder", "tags": [
                    {"name": "B", "tagType": "AtomicTag", "valueSource": MEMORY, "documentation": b}
                ]}
            ]))
        };
        let pre = tree("old", "old");
        let current = tree("new", "new");
        let mut applied = AppliedSteps::default();
        let sent = batches(&repo, &pre, ExistingNodePolicy::OverwriteCarry).expect("plans");
        let first = sent
            .iter()
            .find(|b| b.nodes == ["Line/A"])
            .expect("A's batch");
        applied.sent(first);
        let plan = rollback(&repo, &pre, &current, &applied, false);
        assert_eq!(plan.imports.len(), 1, "{:?}", plan.imports);
        assert_eq!(plan.imports[0].nodes, ["Line/A"]);
        assert!(!body_paths(&plan.imports[0]).contains(&"Other/B".to_string()));
    }

    /// A node a sent batch named that is already as the pre-image held it
    /// (its batch failed before it applied) is not sent again.
    #[test]
    fn rollback_skips_a_node_already_equal_to_its_pre_image() {
        let repo = repo(&[
            ("Line", json!({"tagType": "Folder"})),
            (
                "Line/A",
                json!({"tagType": "AtomicTag", "valueSource": MEMORY, "documentation": "new"}),
            ),
        ]);
        let pre = live(json!([{"name": "Line", "tagType": "Folder", "tags": [
            {"name": "A", "tagType": "AtomicTag", "valueSource": MEMORY, "documentation": "old",
             "value": 1}
        ]}]));
        let current = live(json!([{"name": "Line", "tagType": "Folder", "tags": [
            {"name": "A", "tagType": "AtomicTag", "valueSource": MEMORY, "documentation": "old",
             "value": 5}
        ]}]));
        let applied = all_sent(&repo, &pre);
        let plan = rollback(&repo, &pre, &current, &applied, false);
        assert!(plan.imports.is_empty(), "{:?}", plan.imports);
    }

    /// D-01, T-26-40: drift and untouched siblings never appear in a rollback
    /// body, even when they changed during the run.
    #[test]
    fn rollback_never_names_drift_or_untouched_nodes() {
        let repo = repo(&[
            ("Line", json!({"tagType": "Folder"})),
            (
                "Line/A",
                json!({"tagType": "AtomicTag", "valueSource": MEMORY, "documentation": "new"}),
            ),
            (
                "Line/Same",
                json!({"tagType": "AtomicTag", "valueSource": MEMORY}),
            ),
        ]);
        let tree = |a: &str, stray: &str| {
            live(json!([{"name": "Line", "tagType": "Folder", "tags": [
                {"name": "A", "tagType": "AtomicTag", "valueSource": MEMORY, "documentation": a},
                {"name": "Same", "tagType": "AtomicTag", "valueSource": MEMORY},
                {"name": "Stray", "tagType": "AtomicTag", "valueSource": MEMORY, "documentation": stray}
            ]}]))
        };
        let pre = tree("old", "x");
        let current = tree("new", "y");
        let applied = all_sent(&repo, &pre);
        let plan = rollback(&repo, &pre, &current, &applied, false);
        assert_eq!(plan.imports.len(), 1);
        let names = body_paths(&plan.imports[0]);
        assert_eq!(names, ["Line/A"], "{names:?}");
    }

    // -- carry_guard (26-08 open point 3, KL-5) ------------------------------

    fn guard_live(node: Value) -> BTreeMap<String, Value> {
        live(json!([{"name": "Line", "tagType": "Folder", "tags": [node]}]))
    }

    /// 26-15 O1: no `valueSource` key is memory, which carries.
    #[test]
    fn carry_guard_passes_a_tag_without_a_value_source() {
        let node = json!({"name": "V", "tagType": "AtomicTag", "tooltip": "t"});
        let live = guard_live(json!({"name": "V", "tagType": "AtomicTag", "value": 9}));
        carry_guard("Line/V", &node, &live).expect("a memory tag");
        assert!(uncarriable("Line/V", &node, &live).is_empty());
    }

    /// Fail closed: a `valueSource` that is present but not a name (a
    /// binding object) resolves to no source, so a live value refuses.
    #[test]
    fn carry_guard_refuses_a_live_value_with_an_unresolved_value_source() {
        let node = json!({"name": "V", "tagType": "AtomicTag", "tooltip": "t",
                          "valueSource": {"bindType": "parameter", "binding": "{src}"}});
        let live = guard_live(json!({"name": "V", "tagType": "AtomicTag", "value": 9}));
        let err = carry_guard("Line/V", &node, &live).expect_err("fail closed");
        assert_eq!(err.code(), "deploy_tree_invalid");
        let text = err.to_string();
        assert!(
            text.contains("Line/V") && text.contains("valueSource"),
            "{text}"
        );
        assert!(!text.contains('9'), "never a value: {text}");
        assert_eq!(uncarriable("Line/V", &node, &live), ["Line/V"]);
    }

    #[test]
    fn carry_guard_passes_a_memory_source() {
        let node = json!({"name": "V", "tagType": "AtomicTag", "valueSource": MEMORY});
        let live = guard_live(json!({"name": "V", "tagType": "AtomicTag", "value": 9}));
        carry_guard("Line/V", &node, &live).expect("carried");
    }

    /// KL-5: another source sends the tag stripped; its value comes from the
    /// source, so nothing is reset by the carry.
    #[test]
    fn carry_guard_passes_another_value_source() {
        let node = json!({"name": "V", "tagType": "AtomicTag", "valueSource": "opc"});
        let live = guard_live(
            json!({"name": "V", "tagType": "AtomicTag", "valueSource": "opc", "value": 9}),
        );
        carry_guard("Line/V", &node, &live).expect("not a memory value");
    }

    #[test]
    fn carry_guard_passes_a_live_tag_without_a_value() {
        let node = json!({"name": "V", "tagType": "AtomicTag"});
        let live = guard_live(json!({"name": "V", "tagType": "AtomicTag", "tooltip": "t"}));
        carry_guard("Line/V", &node, &live).expect("no value to lose");
        let absent = guard_live(json!({"name": "Other", "tagType": "AtomicTag"}));
        carry_guard("Line/V", &node, &absent).expect("no live node");
    }

    /// An instance member resolves its source through the type, as the carry
    /// does: a member whose type names none is memory (26-15 O1) and passes,
    /// and a member whose type's source is not a name refuses, naming it.
    #[test]
    fn carry_guard_resolves_instance_members_through_the_type() {
        let live_tree = |member_source: Value| {
            let mut member = json!({"name": "M1", "tagType": "AtomicTag", "value": 0});
            if !member_source.is_null() {
                member["valueSource"] = member_source;
            }
            live(json!([
                {"name": "_types_", "tagType": "Folder", "tags": [
                    {"name": "Ty", "tagType": "UdtType", "tags": [member]}
                ]},
                {"name": "Line", "tagType": "Folder", "tags": [
                    {"name": "I", "tagType": "UdtInstance", "typeId": "Ty",
                     "tags": [{"name": "M1", "tagType": "AtomicTag", "value": 4}]}
                ]}
            ]))
        };
        let node = json!({"name": "I", "tagType": "UdtInstance", "typeId": "Ty",
                          "tags": [{"name": "M1", "tagType": "AtomicTag"}]});
        carry_guard("Line/I", &node, &live_tree(json!(MEMORY))).expect("through the type");
        carry_guard("Line/I", &node, &live_tree(Value::Null)).expect("memory by default");
        let bound = json!({"bindType": "parameter", "binding": "{src}"});
        let err = carry_guard("Line/I", &node, &live_tree(bound)).expect_err("no source");
        assert!(err.to_string().contains("Line/I/M1"), "{err}");
    }

    /// A property-adding changed node whose live tag cannot be carried goes
    /// to `uncarriable`, never into a body (no Overwrite that would reset its
    /// value).
    #[test]
    fn rollback_lists_an_uncarriable_node_and_sends_no_body_for_it() {
        let bound = json!({"bindType": "parameter", "binding": "{src}"});
        let repo = repo(&[
            ("Line", json!({"tagType": "Folder"})),
            (
                "Line/V",
                json!({"tagType": "AtomicTag", "value": 1, "tooltip": "tip",
                       "valueSource": bound}),
            ),
        ]);
        let pre = live(json!([{"name": "Line", "tagType": "Folder", "tags": [
            {"name": "V", "tagType": "AtomicTag", "value": 9, "valueSource": bound}
        ]}]));
        let current = live(json!([{"name": "Line", "tagType": "Folder", "tags": [
            {"name": "V", "tagType": "AtomicTag", "value": 9, "tooltip": "tip",
             "valueSource": bound}
        ]}]));
        let applied = all_sent(&repo, &pre);
        let plan = rollback(&repo, &pre, &current, &applied, false);
        assert!(plan.imports.is_empty(), "{:?}", plan.imports);
        assert_eq!(plan.uncarriable, ["Line/V"]);
        assert!(plan.carried.is_empty());
    }

    // -- rollback deletes through the tag-delete route (26-19) ---------------

    /// The route's handshake, listing `providers`.
    fn handshake(providers: &[&str]) -> RouteHandshake {
        RouteHandshake {
            route_version: "1".to_string(),
            min_cli: "0".to_string(),
            providers: providers.iter().map(|p| (*p).to_string()).collect(),
            max_paths: 500,
            max_path_chars: 1000,
        }
    }

    fn rollback_routed(
        repo: &NativeProvider,
        pre: &BTreeMap<String, Value>,
        current: &BTreeMap<String, Value>,
        applied: &AppliedSteps,
        route: Option<&RouteHandshake>,
        descendants: DeleteDescendants,
    ) -> RollbackPlan {
        let plan = plan_provider(repo, pre, None);
        rollback_plan(
            &plan,
            pre,
            current,
            applied,
            false,
            &BTreeSet::new(),
            route,
            descendants,
        )
        .expect("a rollback plan")
    }

    const BOTH: DeleteDescendants = DeleteDescendants {
        folders: true,
        instances: true,
    };
    const NEITHER: DeleteDescendants = DeleteDescendants {
        folders: false,
        instances: false,
    };

    /// A run that created, in the existing folder `Line`: the folder
    /// `Line/New` holding `A` and `B`, the instance `Line/Inst` (member `M1`)
    /// of the created type `_types_/Ty`, which derives from the created type
    /// `_types_/Base`, and the tag `Line/T`. `Line/Old` was there before and
    /// stays. Returns the repo, the pre-image and the provider now.
    fn created_tree() -> (
        NativeProvider,
        BTreeMap<String, Value>,
        BTreeMap<String, Value>,
    ) {
        let atomic = json!({"tagType": "AtomicTag", "valueSource": MEMORY, "value": 1});
        let member = json!({"name": "M1", "tagType": "AtomicTag", "valueSource": MEMORY});
        let repo = repo(&[
            ("Line", json!({"tagType": "Folder"})),
            ("Line/Old", atomic.clone()),
            ("Line/New", json!({"tagType": "Folder"})),
            ("Line/New/A", atomic.clone()),
            ("Line/New/B", atomic.clone()),
            (
                "Line/Inst",
                json!({"tagType": "UdtInstance", "typeId": "Ty", "tags": [{"name": "M1"}]}),
            ),
            ("Line/T", atomic.clone()),
            (
                "_types_/Base",
                json!({"tagType": "UdtType", "tags": [member.clone()]}),
            ),
            (
                "_types_/Ty",
                json!({"tagType": "UdtType", "typeId": "Base", "tags": [member.clone()]}),
            ),
        ]);
        let old = json!({"name": "Old", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 1});
        let pre = live(json!([
            {"name": "_types_", "tagType": "Folder", "tags": []},
            {"name": "Line", "tagType": "Folder", "tags": [old.clone()]}
        ]));
        let leaf = |name: &str| json!({"name": name, "tagType": "AtomicTag", "valueSource": MEMORY, "value": 1});
        let current = live(json!([
            {"name": "_types_", "tagType": "Folder", "tags": [
                {"name": "Base", "tagType": "UdtType", "tags": [member.clone()]},
                {"name": "Ty", "tagType": "UdtType", "typeId": "Base", "tags": [member]}
            ]},
            {"name": "Line", "tagType": "Folder", "tags": [
                old,
                {"name": "New", "tagType": "Folder", "tags": [leaf("A"), leaf("B")]},
                // As the run sent it (26-REVIEW CR-01: a created node is
                // deleted only when it is the sent one).
                {"name": "Inst", "tagType": "UdtInstance", "typeId": "Ty",
                 "tags": [{"name": "M1"}]},
                leaf("T")
            ]}
        ]));
        (repo, pre, current)
    }

    /// 26-17's record (`folder delete removes children: yes`, `instance
    /// delete removes members: yes`): a created folder whose live nodes were
    /// all created by the run goes as its top-most node alone. Tags go
    /// before types, and types in reverse dependency order. A node that was
    /// there before is never named.
    #[test]
    fn rollback_deletes_collapse_when_delete_removes_descendants() {
        let (repo, pre, current) = created_tree();
        let applied = all_sent(&repo, &pre);
        let route = handshake(&["P"]);
        let plan = rollback_routed(&repo, &pre, &current, &applied, Some(&route), BOTH);
        assert_eq!(
            plan.deletes,
            [
                "Line/Inst",
                "Line/New",
                "Line/T",
                "_types_/Ty",
                "_types_/Base"
            ]
        );
        assert!(plan.undeletable.is_empty(), "{:?}", plan.undeletable);
        assert!(plan.imports.is_empty(), "{:?}", plan.imports);
        assert!(!plan.deletes.iter().any(|p| p == "Line/Old" || p == "Line"));
        assert_eq!(TAG_DELETE_DESCENDANTS, BOTH, "the 26-17 live record");

        // No route, or a route that does not list the provider: 26-11.
        for route in [None, Some(handshake(&["Other"]))] {
            let plan = rollback_routed(&repo, &pre, &current, &applied, route.as_ref(), BOTH);
            assert!(plan.deletes.is_empty(), "{:?}", plan.deletes);
            assert_eq!(
                plan.undeletable,
                [
                    "Line/Inst",
                    "Line/New",
                    "Line/New/A",
                    "Line/New/B",
                    "Line/T",
                    "_types_/Base",
                    "_types_/Ty"
                ]
            );
        }
    }

    /// When a delete does not remove what is below the node it names, every
    /// created path is sent, deepest first: children (and instance members)
    /// before their folder or instance, tags before types.
    #[test]
    fn rollback_deletes_every_path_deepest_first_otherwise() {
        let (repo, pre, current) = created_tree();
        let applied = all_sent(&repo, &pre);
        let route = handshake(&["P"]);
        let plan = rollback_routed(&repo, &pre, &current, &applied, Some(&route), NEITHER);
        assert_eq!(
            plan.deletes,
            [
                "Line/Inst/M1",
                "Line/New/A",
                "Line/New/B",
                "Line/Inst",
                "Line/New",
                "Line/T",
                "_types_/Ty",
                "_types_/Base"
            ]
        );
        assert!(plan.undeletable.is_empty(), "{:?}", plan.undeletable);
    }

    /// 26-REVIEW WR-03: a changed tag, a changed type and a removed tag whose
    /// rollback body is over the cap are named with their sizes; small ones
    /// are not.
    #[test]
    fn rollback_over_cap_names_each_node_its_undo_could_not_send() {
        let big = "d".repeat(IMPORT_BATCH_MAX_BYTES);
        let pre = live(json!([
            {"name": "_types_", "tagType": "Folder", "tags": [
                {"name": "Ty", "tagType": "UdtType", "documentation": big.clone()}
            ]},
            {"name": "Line", "tagType": "Folder", "tags": [
                {"name": "A", "tagType": "AtomicTag", "valueSource": MEMORY, "documentation": big.clone()},
                {"name": "B", "tagType": "AtomicTag", "valueSource": MEMORY, "value": big.clone()},
                {"name": "C", "tagType": "AtomicTag", "valueSource": MEMORY, "documentation": big},
                {"name": "S", "tagType": "AtomicTag", "valueSource": MEMORY, "documentation": "s"}
            ]}
        ]));
        let plan = ProviderPlan {
            provider: "P".to_string(),
            changed: ["Line/A", "Line/B", "Line/S", "_types_/Ty"]
                .map(|p| crate::deploy::tag_plan::NodeChange {
                    path: p.to_string(),
                    properties: Vec::new(),
                    restart_likely: false,
                })
                .to_vec(),
            removed: vec!["Line/C".to_string()],
            ..ProviderPlan::default()
        };
        // B's repo node adds a property, so its undo carries the value.
        let with_tooltip = repo(&[(
            "Line/B",
            json!({"tagType": "AtomicTag", "valueSource": MEMORY, "tooltip": "t"}),
        )]);
        let found = rollback_over_cap(&plan, &with_tooltip, &pre);
        let named: Vec<&str> = found.iter().map(|f| f.file.as_str()).collect();
        assert_eq!(
            named,
            ["[P]Line/A", "[P]Line/B", "[P]_types_/Ty", "[P]Line/C"],
            "{found:?}"
        );
        assert!(found.iter().all(|f| f.message.contains("rollback")));
        // Without an added property the undo goes MergeOverwrite stripped:
        // a big value is never sent back.
        let plain = repo(&[(
            "Line/B",
            json!({"tagType": "AtomicTag", "valueSource": MEMORY}),
        )]);
        let found = rollback_over_cap(&plan, &plain, &pre);
        assert!(!found.iter().any(|f| f.file == "[P]Line/B"), "{found:?}");
    }

    /// 26-REVIEW WR-05: a created type a surviving node uses is never sent,
    /// nor a type it uses in turn; the run's own instance still goes.
    #[test]
    fn rollback_keeps_created_types_a_survivor_uses() {
        let (repo, pre, mut current) = created_tree();
        current.insert(
            "Line/Old".to_string(),
            json!({"tagType": "UdtInstance", "typeId": "Ty"}),
        );
        let applied = all_sent(&repo, &pre);
        let route = handshake(&["P"]);
        for mode in [BOTH, NEITHER] {
            let plan = rollback_routed(&repo, &pre, &current, &applied, Some(&route), mode);
            for path in ["_types_/Ty", "_types_/Base"] {
                assert!(
                    !plan.deletes.iter().any(|p| p == path),
                    "{mode:?}: {:?}",
                    plan.deletes
                );
            }
            assert!(plan.deletes.iter().any(|p| p == "Line/Inst"), "{mode:?}");
        }
    }

    /// 26-REVIEW WR-04: drawn again from a later export, a created folder
    /// that gained another client's node stays while the run's own nodes in
    /// it go; a created node now gone needs nothing; one changed since is
    /// `foreign`.
    #[test]
    fn created_deletes_now_follow_the_later_export() {
        let (repo, pre, current) = created_tree();
        let applied = all_sent(&repo, &pre);
        let route = handshake(&["P"]);
        let plan = rollback_routed(&repo, &pre, &current, &applied, Some(&route), BOTH);
        assert!(
            plan.created.iter().any(|p| p == "Line/New"),
            "{:?}",
            plan.created
        );
        let mut later = current.clone();
        later.insert(
            "Line/New/Other".to_string(),
            json!({"tagType": "AtomicTag", "valueSource": MEMORY}),
        );
        later.remove("Line/T");
        later
            .get_mut("Line/New/B")
            .and_then(Value::as_object_mut)
            .expect("B")
            .insert("tooltip".to_string(), json!("theirs"));
        let CreatedDeletes {
            deletes,
            kept,
            foreign,
            ..
        } = created_deletes_now("P", &plan.created, &later, &applied, BOTH).expect("drawn");
        assert_eq!(kept, ["Line/New"]);
        assert_eq!(foreign, [("Line/New/B".to_string(), Foreign::Differs)]);
        assert!(deletes.iter().any(|p| p == "Line/New/A"), "{deletes:?}");
        for path in ["Line/New", "Line/New/B", "Line/New/Other", "Line/T"] {
            assert!(!deletes.iter().any(|p| p == path), "{path}: {deletes:?}");
        }
    }

    /// 26-REVIEW CR-01: a node at an added path is deleted only when the run
    /// wrote it. One whose import collided, one unlike the node the run sent,
    /// and one with no sent node recorded are `foreign` and never sent; a
    /// created folder holding one stays.
    #[test]
    fn rollback_deletes_only_what_the_run_wrote() {
        let (repo, pre, mut current) = created_tree();
        let mut applied = all_sent(&repo, &pre);
        applied.collided.insert("Line/T".to_string());
        applied.bodies.remove("Line/New/B");
        current
            .get_mut("Line/New/A")
            .and_then(Value::as_object_mut)
            .expect("A")
            .insert("documentation".to_string(), json!("theirs"));
        let route = handshake(&["P"]);
        for mode in [BOTH, NEITHER] {
            let plan = rollback_routed(&repo, &pre, &current, &applied, Some(&route), mode);
            assert_eq!(
                plan.foreign,
                [
                    ("Line/New/A".to_string(), Foreign::Differs),
                    ("Line/New/B".to_string(), Foreign::Unrecorded),
                    ("Line/T".to_string(), Foreign::Collided),
                ],
                "{mode:?}"
            );
            assert_eq!(plan.undeletable, ["Line/New"], "{mode:?}");
            for path in ["Line/New", "Line/New/A", "Line/New/B", "Line/T"] {
                assert!(!plan.deletes.iter().any(|p| p == path), "{path}: {mode:?}");
            }
            assert!(plan.deletes.iter().any(|p| p == "Line/Inst"), "{mode:?}");
        }
        // Without a route a node the run did not write keeps its own reason.
        let plan = rollback_routed(&repo, &pre, &current, &applied, None, BOTH);
        assert_eq!(plan.foreign.len(), 3);
        assert!(!plan.undeletable.iter().any(|p| p == "Line/T"));
    }

    /// T-26-77: a created folder that holds a live node the run did not
    /// create stays (`undeletable`); its created children are still sent,
    /// and the other node is never named, in either mode.
    #[test]
    fn rollback_keeps_a_created_folder_holding_a_node_the_run_did_not_create() {
        let (repo, pre, mut current) = created_tree();
        current.insert(
            "Line/New/Drift".to_string(),
            json!({"tagType": "AtomicTag", "valueSource": MEMORY}),
        );
        let applied = all_sent(&repo, &pre);
        let route = handshake(&["P"]);
        for mode in [BOTH, NEITHER] {
            let plan = rollback_routed(&repo, &pre, &current, &applied, Some(&route), mode);
            assert_eq!(plan.undeletable, ["Line/New"], "{mode:?}");
            assert!(
                plan.deletes.iter().any(|p| p == "Line/New/A")
                    && plan.deletes.iter().any(|p| p == "Line/New/B"),
                "{:?}",
                plan.deletes
            );
            assert!(
                !plan
                    .deletes
                    .iter()
                    .any(|p| p == "Line/New" || p == "Line/New/Drift" || p == "Line/Old"),
                "{:?}",
                plan.deletes
            );
        }
    }

    /// A node absent now, or one the pre-image already held, is never sent
    /// to the route, so a rollback with nothing created sends no delete.
    #[test]
    fn rollback_deletes_nothing_the_run_did_not_create() {
        let (repo, pre, _) = created_tree();
        let applied = all_sent(&repo, &pre);
        let route = handshake(&["P"]);
        let plan = rollback_routed(&repo, &pre, &pre, &applied, Some(&route), BOTH);
        assert!(plan.deletes.is_empty(), "{:?}", plan.deletes);
        assert!(plan.undeletable.is_empty(), "{:?}", plan.undeletable);
    }

    // -- re-create batches for deleted nodes (26-19, R3-RB) -----------------

    /// The pre-image of a provider whose run deleted nodes: the types
    /// `_types_/Base` and `_types_/Ty` (derived from `Base`); the folder
    /// `Line/F` holding the memory tag `A` (value 7, `defaultValue` 3,
    /// a timestamp and a quality) and the instance `Inst` of `Ty` (a
    /// parameter, member `M1` overriding its documentation, value 4); the
    /// OPC tag `Line/O`; and `Line/Keep`, which stays.
    fn deleted_pre() -> BTreeMap<String, Value> {
        live(json!([
            {"name": "_types_", "tagType": "Folder", "tags": [
                {"name": "Base", "tagType": "UdtType", "tags": [
                    {"name": "M1", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 0}
                ]},
                {"name": "Ty", "tagType": "UdtType", "typeId": "Base", "tags": [
                    {"name": "M1", "tagType": "AtomicTag", "valueSource": MEMORY}
                ]}
            ]},
            {"name": "Line", "tagType": "Folder", "tags": [
                {"name": "F", "tagType": "Folder", "tooltip": "f", "tags": [
                    {"name": "A", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 7,
                     "defaultValue": 3, "timestamp": 1_700_000_000_000_i64, "quality": "Good"},
                    {"name": "Inst", "tagType": "UdtInstance", "typeId": "Ty",
                     "parameters": {"p": {"dataType": "Int4", "value": 1}},
                     "tags": [{"name": "M1", "tagType": "AtomicTag", "value": 4,
                               "documentation": "override", "timestamp": 1,
                               "quality": "Good"}]}
                ]},
                {"name": "O", "tagType": "AtomicTag", "valueSource": "opc", "value": 5,
                 "opcItemPath": "ns=1;s=O", "quality": "Good"},
                {"name": "Keep", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 1}
            ]}
        ]))
    }

    /// The provider after the deletes: `Line/Keep` only.
    fn deleted_now() -> BTreeMap<String, Value> {
        live(json!([
            {"name": "_types_", "tagType": "Folder", "tags": []},
            {"name": "Line", "tagType": "Folder", "tags": [
                {"name": "Keep", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 1}
            ]}
        ]))
    }

    /// The plan of a repo holding `Line/Keep` that removes every other node
    /// of [`deleted_pre`].
    fn removing_plan(pre: &BTreeMap<String, Value>) -> ProviderPlan {
        let repo = repo(&[
            ("Line", json!({"tagType": "Folder"})),
            (
                "Line/Keep",
                json!({"tagType": "AtomicTag", "valueSource": MEMORY, "value": 1}),
            ),
        ]);
        let mut plan = plan_provider(&repo, pre, None);
        plan.removed = [
            "Line/F",
            "Line/F/A",
            "Line/F/Inst",
            "Line/O",
            "_types_/Base",
            "_types_/Ty",
        ]
        .map(String::from)
        .to_vec();
        plan
    }

    fn deleted_steps(paths: &[&str]) -> AppliedSteps {
        AppliedSteps {
            deleted: paths.iter().map(|p| (*p).to_string()).collect(),
            ..AppliedSteps::default()
        }
    }

    fn undo_deleted(
        plan: &ProviderPlan,
        pre: &BTreeMap<String, Value>,
        current: &BTreeMap<String, Value>,
        applied: &AppliedSteps,
    ) -> Result<RollbackPlan, crate::error::CoreError> {
        rollback_plan(
            plan,
            pre,
            current,
            applied,
            false,
            &BTreeSet::new(),
            None,
            TAG_DELETE_DESCENDANTS,
        )
    }

    /// The tag-side re-create of [`deleted_pre`]: one `Abort` at `Line`
    /// holding `F` whole (`A` and `Inst` nested) and `O`.
    fn assert_line_recreated(batch: &ImportBatch) {
        assert_eq!(
            (batch.path.as_str(), batch.policy, batch.carry),
            ("Line", CollisionPolicy::Abort, false)
        );
        assert_eq!(batch.nodes, ["Line/F", "Line/F/A", "Line/F/Inst", "Line/O"]);
        let tags = body(batch)["tags"].clone();
        let names: Vec<&str> = tags
            .as_array()
            .expect("tags")
            .iter()
            .map(|t| t["name"].as_str().expect("name"))
            .collect();
        assert_eq!(names, ["F", "O"]);
        assert_eq!(tags[0]["tooltip"], "f");
        let kids: Vec<&str> = tags[0]["tags"]
            .as_array()
            .expect("F comes back whole")
            .iter()
            .map(|t| t["name"].as_str().expect("name"))
            .collect();
        assert_eq!(kids, ["A", "Inst"]);
    }

    /// A deleted path the provider holds now (another client put it back,
    /// or the delete did not land) is not re-created.
    #[test]
    fn a_deleted_path_present_now_is_not_recreated() {
        let pre = deleted_pre();
        let plan = removing_plan(&pre);
        let undo =
            undo_deleted(&plan, &pre, &pre, &deleted_steps(&["Line/O", "Line/F"])).expect("a plan");
        assert!(undo.imports.is_empty(), "{:?}", undo.imports);
        assert!(undo.recreated_deleted.is_empty());
    }

    /// A deleted folder comes back whole with every pre-image node below
    /// it, a deleted instance with its members, by `Abort` at its parent;
    /// a deleted path below a re-created one is not sent again. Both delete
    /// modes name the same nodes.
    #[test]
    fn a_deleted_folder_comes_back_whole_in_both_delete_modes() {
        let pre = deleted_pre();
        let now = deleted_now();
        let plan = removing_plan(&pre);
        for deleted in [
            vec!["Line/F", "Line/O"],
            vec!["Line/F/A", "Line/F/Inst", "Line/F", "Line/O"],
        ] {
            let undo = undo_deleted(&plan, &pre, &now, &deleted_steps(&deleted)).expect("a plan");
            assert_eq!(undo.imports.len(), 1, "{deleted:?}: {:?}", undo.imports);
            assert_line_recreated(&undo.imports[0]);
            assert_eq!(undo.recreated_deleted, ["Line/F", "Line/O"]);
        }
    }

    /// Deleted types come back first, one per batch in `type_order`, inside
    /// their `_types_` chain; then the deleted tag nodes (a folder holding
    /// an instance of a deleted type), parents first, grouped by their
    /// deepest existing parent.
    #[test]
    fn deleted_types_come_back_first_in_dependency_order() {
        let pre = deleted_pre();
        let now = deleted_now();
        let plan = removing_plan(&pre);
        let undo = undo_deleted(
            &plan,
            &pre,
            &now,
            &deleted_steps(&["Line/F", "Line/O", "_types_/Ty", "_types_/Base"]),
        )
        .expect("a plan");
        let heads: Vec<(&str, &[String])> = undo
            .imports
            .iter()
            .map(|b| (b.path.as_str(), b.nodes.as_slice()))
            .collect();
        assert_eq!(heads.len(), 3, "{heads:?}");
        assert_eq!(undo.imports[0].nodes, ["_types_/Base"]);
        assert_eq!(undo.imports[1].nodes, ["_types_/Ty"]);
        for batch in &undo.imports[..2] {
            assert_eq!(
                (batch.path.as_str(), batch.policy),
                ("", CollisionPolicy::Abort)
            );
            assert_eq!(body(batch)["tags"][0]["name"], "_types_");
        }
        assert_eq!(
            body(&undo.imports[1])["tags"][0]["tags"][0],
            json!({"name": "Ty", "tagType": "UdtType", "typeId": "Base", "tags": [
                {"name": "M1", "tagType": "AtomicTag", "valueSource": MEMORY}
            ]}),
            "a type comes back whole, as the pre-image holds it"
        );
        assert_line_recreated(&undo.imports[2]);
    }

    /// A deleted folder over the cap goes alone, then its children below it,
    /// parents first; no body is over the cap unless it holds one node.
    #[test]
    fn a_deleted_folder_over_the_cap_is_split() {
        let pad = IMPORT_BATCH_MAX_BYTES / 6;
        let children: Vec<Value> = (0..12)
            .map(|i| {
                let mut node = big(i, pad);
                node["name"] = json!(format!("T{i:02}"));
                node
            })
            .collect();
        let pre = live(json!([{"name": "Line", "tagType": "Folder", "tags": [
            {"name": "Big", "tagType": "Folder", "tags": children}
        ]}]));
        let now = live(json!([{"name": "Line", "tagType": "Folder", "tags": []}]));
        let repo = repo(&[("Line", json!({"tagType": "Folder"}))]);
        let mut plan = plan_provider(&repo, &pre, None);
        plan.removed = vec!["Line/Big".to_string()];
        let undo = undo_deleted(&plan, &pre, &now, &deleted_steps(&["Line/Big"])).expect("a plan");
        assert!(undo.imports.len() >= 3, "{}", undo.imports.len());
        assert_eq!(undo.imports[0].path, "Line");
        assert_eq!(undo.imports[0].nodes, ["Line/Big"]);
        let mut named = Vec::new();
        for batch in &undo.imports {
            assert_eq!(batch.policy, CollisionPolicy::Abort);
            assert!(
                batch.body.len() <= IMPORT_BATCH_MAX_BYTES || batch.nodes.len() == 1,
                "{} bytes, {} nodes",
                batch.body.len(),
                batch.nodes.len()
            );
            named.extend(batch.nodes.iter().cloned());
        }
        for batch in &undo.imports[1..] {
            assert_eq!(batch.path, "Line/Big");
        }
        named.sort();
        assert_eq!(named.len(), 13, "{named:?}");
    }

    /// The costly-to-reverse rule (26-19): a re-created memory tag carries
    /// its pre-image `value` (no live value exists to keep) and no
    /// `timestamp` or `quality`; `defaultValue`, parameters and member
    /// overrides are sent as the pre-image holds them; a tag of another
    /// value source is sent without its value (R3-CARRY, KL-5).
    #[test]
    fn a_recreated_memory_tag_carries_its_pre_image_value() {
        let pre = deleted_pre();
        let now = deleted_now();
        let plan = removing_plan(&pre);
        let undo =
            undo_deleted(&plan, &pre, &now, &deleted_steps(&["Line/F", "Line/O"])).expect("a plan");
        let tags = body(&undo.imports[0])["tags"].clone();
        let a = &tags[0]["tags"][0];
        assert_eq!(
            a,
            &json!({"name": "A", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 7,
                    "defaultValue": 3})
        );
        let inst = &tags[0]["tags"][1];
        assert_eq!(
            inst["parameters"],
            json!({"p": {"dataType": "Int4", "value": 1}})
        );
        assert_eq!(
            inst["tags"][0],
            json!({"name": "M1", "tagType": "AtomicTag", "value": 4,
                   "documentation": "override"}),
            "a member's value through its type's valueSource"
        );
        let o = &tags[1];
        assert!(o.get("value").is_none(), "{o}");
        assert!(o.get("quality").is_none(), "{o}");
        assert_eq!(o["opcItemPath"], "ns=1;s=O");
    }

    /// The full rollback order: re-created types, restored changed types,
    /// re-created tag nodes, restored changed tag nodes; then the deletes
    /// of what the run created.
    #[test]
    fn rollback_imports_follow_the_full_order() {
        let mut pre = deleted_pre();
        pre.insert(
            "_types_/Changed".to_string(),
            json!({"tagType": "UdtType", "tags": [
                {"name": "M1", "tagType": "AtomicTag", "documentation": "old"}
            ]}),
        );
        pre.insert(
            "Line/Keep".to_string(),
            json!({"tagType": "AtomicTag", "valueSource": MEMORY, "value": 1,
                   "documentation": "old"}),
        );
        let changed_type = json!({"tagType": "UdtType", "tags": [
            {"name": "M1", "tagType": "AtomicTag", "documentation": "new"}
        ]});
        let keep = json!({"tagType": "AtomicTag", "valueSource": MEMORY, "value": 1,
                          "documentation": "new"});
        let new = json!({"tagType": "AtomicTag", "valueSource": MEMORY, "value": 1});
        let repo = repo(&[
            ("Line", json!({"tagType": "Folder"})),
            ("Line/Keep", keep.clone()),
            ("Line/New", new.clone()),
            ("_types_/Changed", changed_type.clone()),
        ]);
        let mut plan = plan_provider(&repo, &pre, None);
        plan.removed = removing_plan(&deleted_pre()).removed;
        let mut now = deleted_now();
        now.insert("_types_/Changed".to_string(), changed_type);
        now.insert("Line/Keep".to_string(), keep);
        now.insert("Line/New".to_string(), new);
        let mut applied = all_sent(&repo, &pre);
        applied.deleted = ["Line/F", "Line/O", "_types_/Ty", "_types_/Base"]
            .map(String::from)
            .to_vec();
        let route = handshake(&["P"]);
        let undo = rollback_plan(
            &plan,
            &pre,
            &now,
            &applied,
            false,
            &BTreeSet::new(),
            Some(&route),
            TAG_DELETE_DESCENDANTS,
        )
        .expect("a plan");
        let order: Vec<(&str, CollisionPolicy, Vec<String>)> = undo
            .imports
            .iter()
            .map(|b| (b.path.as_str(), b.policy, b.nodes.clone()))
            .collect();
        let s = |v: &[&str]| v.iter().map(|p| (*p).to_string()).collect::<Vec<_>>();
        assert_eq!(
            order,
            vec![
                ("", CollisionPolicy::Abort, s(&["_types_/Base"])),
                ("", CollisionPolicy::Abort, s(&["_types_/Ty"])),
                ("", CollisionPolicy::Overwrite, s(&["_types_/Changed"])),
                (
                    "Line",
                    CollisionPolicy::Abort,
                    s(&["Line/F", "Line/F/A", "Line/F/Inst", "Line/O"])
                ),
                ("Line", CollisionPolicy::MergeOverwrite, s(&["Line/Keep"])),
            ]
        );
        assert_eq!(undo.deletes, ["Line/New"]);
    }

    /// A deleted path the plan does not remove means the applied steps are
    /// inconsistent: `Internal`, naming it, and nothing is planned.
    #[test]
    fn a_deleted_path_outside_the_removed_set_is_internal() {
        let pre = deleted_pre();
        let plan = removing_plan(&pre);
        let err = undo_deleted(
            &plan,
            &pre,
            &deleted_now(),
            &deleted_steps(&["Line/O", "Line/Keep"]),
        )
        .expect_err("Line/Keep is not removed by the plan");
        assert_eq!(err.code(), "internal", "{err}");
        assert!(err.to_string().contains("Line/Keep"), "{err}");
    }

    // -- the forward delete set (26-20, D-02, T-26-81, T-26-82, T-26-84) -----

    fn calls(
        plan: &ProviderPlan,
        fresh: &BTreeMap<String, Value>,
        route: &RouteHandshake,
        descendants: DeleteDescendants,
    ) -> Result<Vec<TagDeleteCall>, DeleteSetError> {
        delete_calls(plan, fresh, route, descendants)
    }

    fn paths_of(calls: &[TagDeleteCall]) -> Vec<String> {
        calls.iter().flat_map(|c| c.paths.clone()).collect()
    }

    /// No call ever names a node outside the plan's removals (T-26-81), no
    /// call is empty, and every call is for the plan's own provider.
    fn only_removed(plan: &ProviderPlan, calls: &[TagDeleteCall]) {
        for call in calls {
            assert_eq!(call.provider, plan.provider);
            assert!(!call.paths.is_empty(), "an empty call");
            for path in &call.paths {
                assert!(plan.removed.contains(path), "{path} is not removed");
            }
        }
    }

    /// 26-17's record (`yes` for both kinds): a removed folder goes alone and
    /// the removed paths below it are not sent; tag nodes before types, and
    /// types in reverse dependency order (the type that uses another first).
    #[test]
    fn delete_calls_collapse_when_delete_removes_descendants() {
        let fresh = deleted_pre();
        let plan = removing_plan(&fresh);
        let out = calls(&plan, &fresh, &handshake(&["P"]), BOTH).expect("a delete set");
        only_removed(&plan, &out);
        assert_eq!(out.len(), 1);
        assert_eq!(
            paths_of(&out),
            ["Line/F", "Line/O", "_types_/Ty", "_types_/Base"]
        );
        assert_eq!(
            out,
            calls(&plan, &fresh, &handshake(&["P"]), TAG_DELETE_DESCENDANTS).expect("the same")
        );
    }

    /// Without that record every removed path is sent, deepest first
    /// (children before their folder), byte order within a depth; an
    /// instance's members are never named (they are not removals).
    #[test]
    fn delete_calls_every_path_deepest_first_otherwise() {
        let fresh = deleted_pre();
        let plan = removing_plan(&fresh);
        let out = calls(&plan, &fresh, &handshake(&["P"]), NEITHER).expect("a delete set");
        only_removed(&plan, &out);
        assert_eq!(
            paths_of(&out),
            [
                "Line/F/A",
                "Line/F/Inst",
                "Line/F",
                "Line/O",
                "_types_/Ty",
                "_types_/Base"
            ]
        );
    }

    /// A removed path the fresh export lacks is not sent (it is already
    /// absent); with nothing left there is no call at all.
    #[test]
    fn delete_calls_skip_what_the_fresh_export_lacks() {
        let mut fresh = deleted_pre();
        fresh.remove("Line/O");
        let plan = removing_plan(&deleted_pre());
        let out = calls(&plan, &fresh, &handshake(&["P"]), BOTH).expect("a delete set");
        assert_eq!(paths_of(&out), ["Line/F", "_types_/Ty", "_types_/Base"]);
        let empty = deleted_now();
        let out = calls(&plan, &empty, &handshake(&["P"]), BOTH).expect("a delete set");
        assert!(out.is_empty(), "{out:?}");
    }

    /// D-01: a removed folder whose fresh subtree holds a node outside the
    /// removed set (another client's node made after the plan) stops the
    /// provider's deletes: nothing of it is sent.
    #[test]
    fn delete_calls_stop_on_a_live_node_outside_the_removed_set() {
        let mut fresh = deleted_pre();
        fresh.insert(
            "Line/F/New".to_string(),
            json!({"name": "New", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 1}),
        );
        let plan = removing_plan(&deleted_pre());
        for mode in [BOTH, NEITHER] {
            let err = calls(&plan, &fresh, &handshake(&["P"]), mode).expect_err("changed");
            assert_eq!(
                err,
                DeleteSetError::Changed {
                    path: "Line/F".to_string(),
                    node: "Line/F/New".to_string(),
                }
            );
        }
    }

    /// T-26-82 at send time: a removed type a surviving node uses is
    /// refused, whether the referrer is an instance (a drift node made after
    /// the plan), a surviving type deriving from it, or a member of a
    /// surviving type.
    #[test]
    fn delete_calls_refuse_a_removed_type_in_use() {
        let plan = removing_plan(&deleted_pre());
        let cases = [
            (
                "Line/Drift",
                json!({"name": "Drift", "tagType": "UdtInstance", "typeId": "Ty"}),
                "_types_/Ty",
            ),
            (
                "_types_/Child",
                json!({"name": "Child", "tagType": "UdtType", "typeId": "Base"}),
                "_types_/Base",
            ),
            (
                "_types_/Holder",
                json!({"name": "Holder", "tagType": "UdtType", "tags": [
                    {"name": "Sub", "tagType": "UdtInstance", "typeId": "Ty"}
                ]}),
                "_types_/Ty",
            ),
        ];
        for (path, node, used) in cases {
            let mut fresh = deleted_pre();
            fresh.insert(path.to_string(), node);
            let err = calls(&plan, &fresh, &handshake(&["P"]), BOTH).expect_err("in use");
            assert_eq!(
                err,
                DeleteSetError::TypeInUse {
                    type_path: used.to_string(),
                    referrer: path.to_string(),
                },
                "{path}"
            );
        }
    }

    /// Calls follow the handshake's `maxPaths`, and a path over its
    /// `maxPathChars` is refused before any call.
    #[test]
    fn delete_calls_follow_the_route_limits() {
        let fresh = deleted_pre();
        let plan = removing_plan(&fresh);
        let mut route = handshake(&["P"]);
        route.max_paths = 2;
        let out = calls(&plan, &fresh, &route, NEITHER).expect("a delete set");
        only_removed(&plan, &out);
        let sizes: Vec<usize> = out.iter().map(|c| c.paths.len()).collect();
        assert_eq!(sizes, [2, 2, 2]);
        route.max_path_chars = 6;
        let err = calls(&plan, &fresh, &route, NEITHER).expect_err("too long");
        assert!(
            matches!(&err, DeleteSetError::TooLong { max: 6, .. }),
            "{err:?}"
        );
    }

    /// Ultrareview B2: a changed node below `_types_/` that is not a type
    /// (a folder in the repo where the live provider holds a UDT type) has
    /// no import, so the plan is refused naming it before anything is
    /// built.
    #[test]
    fn a_changed_non_type_below_types_is_refused() {
        let repo = repo(&[
            ("_types_/Grp", json!({"tagType": "Folder"})),
            (
                "_types_/Grp/Ty",
                json!({"tagType": "UdtType", "tags": [{"name": "M", "tagType": "AtomicTag"}]}),
            ),
        ]);
        let live = live(json!([
            {"name": "_types_", "tagType": "Folder", "tags": [
                {"name": "Grp", "tagType": "UdtType", "tags": []}
            ]}
        ]));
        let plan = plan_provider(&repo, &live, None);
        assert!(
            plan.changed.iter().any(|c| c.path == "_types_/Grp"),
            "{plan:?}"
        );
        let err = import_batches(&plan, &repo, &live, ExistingNodePolicy::OverwriteCarry)
            .expect_err("no import for the changed folder");
        let text = refused_text(&err);
        assert!(text.contains("[P]_types_/Grp "), "{text}");
        assert!(text.contains("nothing was written"), "{text}");
    }

    /// Ultrareview B3: a changed node whose tagType becomes `Folder` is
    /// converted before the added children below it go out.
    #[test]
    fn a_node_converted_to_a_folder_goes_before_its_added_children() {
        let repo = repo(&[
            ("Line", json!({"tagType": "Folder"})),
            ("Line/Conv", json!({"tagType": "Folder"})),
            (
                "Line/Conv/A",
                json!({"tagType": "AtomicTag", "valueSource": MEMORY}),
            ),
        ]);
        let live = live(json!([{"name": "Line", "tagType": "Folder", "tags": [
            {"name": "Conv", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 1}
        ]}]));
        let batches = batches(&repo, &live, ExistingNodePolicy::OverwriteCarry).expect("plans");
        let at = |path: &str| {
            batches
                .iter()
                .position(|b| b.nodes.iter().any(|n| n == path))
                .unwrap_or_else(|| panic!("{path} is in no batch: {batches:?}"))
        };
        assert!(at("Line/Conv") < at("Line/Conv/A"), "{batches:?}");
    }

    /// Ultrareview B1: a created folder below `_types_/` that stays because a
    /// survivor uses one of its created types still has its other created
    /// descendants deleted.
    #[test]
    fn a_created_type_folder_kept_for_an_in_use_type_still_deletes_the_rest() {
        let current = live(json!([
            {"name": "_types_", "tagType": "Folder", "tags": [
                {"name": "Grp", "tagType": "Folder", "tags": [
                    {"name": "Used", "tagType": "UdtType", "tags": []},
                    {"name": "Spare", "tagType": "UdtType", "tags": []}
                ]}
            ]},
            {"name": "Old", "tagType": "UdtInstance", "typeId": "Grp/Used"}
        ]));
        let created = ["_types_/Grp", "_types_/Grp/Spare", "_types_/Grp/Used"];
        for mode in [BOTH, NEITHER] {
            let drawn = created_deletes("P", &created, &current, mode).expect("deletes");
            assert_eq!(drawn.deletes, ["_types_/Grp/Spare"], "{mode:?}: {drawn:?}");
            let held: Vec<&str> = drawn.in_use.iter().map(|(p, _)| p.as_str()).collect();
            assert!(held.contains(&"_types_/Grp/Used"), "{mode:?}: {held:?}");
        }
    }
}
