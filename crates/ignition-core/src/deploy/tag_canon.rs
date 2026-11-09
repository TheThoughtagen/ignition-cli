//! Canonical form of native tag JSON, for the plan diff and the verify of
//! project-owned tags (Phase 26, research `## Revision 3`).
//!
//! The repo side is the git module's export ([`super::native_tags`]); the
//! live side is the gateway's REST export (`tags/export?provider=P`), cut into
//! the same per-file nodes by [`flatten_export`]. Neither is compared byte for
//! byte:
//!
//! - **R3-KEY.** Object keys are sorted and the children of every `tags` array
//!   are keyed by `name`, so the order a gateway lists children in never
//!   counts ([`canonical`]).
//! - **R3-VK.** `value`, `timestamp` and `quality` are runtime state of an
//!   AtomicTag ([`VALUE_KEYS`]). [`strip_runtime_values`] removes them as
//!   direct properties of an object whose `tagType` is `AtomicTag` (a
//!   top-level tag or an instance member) and nowhere else: never inside a
//!   `parameters` object, never another key of a member override, never below
//!   `_types_/`, where a member's `value` is the type's default and therefore
//!   configuration. Plan 26-04 completes the rule after capture C7.
//! - **R3-DV.** The gateway adds `defaultValue` (equal to the sent `value`)
//!   to an AtomicTag created without one (capture C12). [`absorb_default_value`]
//!   accepts exactly that difference and nothing else.
//!
//! Pure: values in, values out.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use super::native_tags::TYPES_DIR;
use crate::error::CoreError;

/// The runtime keys of an AtomicTag, removed in every comparison of a node
/// outside `_types_/` (R3-VK, D-03). Only direct properties of an object whose
/// `tagType` is `AtomicTag` are runtime keys.
pub const VALUE_KEYS: [&str; 3] = ["value", "timestamp", "quality"];

/// The property R3-DV is about.
const DEFAULT_VALUE: &str = "defaultValue";

/// Whether `v` is an object whose `tagType` is `AtomicTag`.
fn is_atomic(v: &Value) -> bool {
    v.get("tagType").and_then(Value::as_str) == Some("AtomicTag")
}

/// Whether a provider-relative path is below `_types_/`.
pub fn in_types(path: &str) -> bool {
    path.strip_prefix(TYPES_DIR)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// `v` with object keys sorted, recursively, and the children of every `tags`
/// array ordered by `name` (R3-KEY). Two nodes are equal configuration when
/// their canonical forms are equal.
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
                        items.sort_by(|a, b| child_name(a).cmp(child_name(b)));
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

/// A child's `name`, or empty when it has none.
fn child_name(v: &Value) -> &[u8] {
    v.get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .as_bytes()
}

/// Remove the AtomicTag runtime keys ([`VALUE_KEYS`]) from `node` and from
/// every AtomicTag member below it in `tags` (R3-VK). Never descends into
/// `parameters` or any other key. The caller never passes a node below
/// `_types_/`.
pub fn strip_runtime_values(node: &mut Value) {
    if is_atomic(node)
        && let Some(object) = node.as_object_mut()
    {
        for key in VALUE_KEYS {
            object.remove(key);
        }
    }
    if let Some(children) = node.get_mut("tags").and_then(Value::as_array_mut) {
        for child in children {
            strip_runtime_values(child);
        }
    }
}

/// R3-DV for one AtomicTag outside `_types_/`, in a comparison of the repo
/// node `repo` with the live node `live`.
///
/// When the repo node carries `defaultValue`, nothing is done: it is
/// configuration and is compared. When it carries none, a live `defaultValue`
/// is the gateway's own addition and is removed from `live`, except on a node
/// the run `created` whose repo node carries `value`: there the live
/// `defaultValue` must first equal that `value` canonically (the C12 seed).
/// A seed that differs is `Err` naming the property `defaultValue` and leaves
/// `live` as it was, so the comparison reports it. Never a value.
pub fn absorb_default_value(repo: &Value, live: &mut Value, created: bool) -> Result<(), String> {
    if !is_atomic(repo) || repo.get(DEFAULT_VALUE).is_some() {
        return Ok(());
    }
    let Some(live_default) = live.get(DEFAULT_VALUE) else {
        return Ok(());
    };
    if created
        && let Some(seed) = repo.get("value")
        && canonical(live_default) != canonical(seed)
    {
        return Err(DEFAULT_VALUE.to_string());
    }
    if let Some(object) = live.as_object_mut() {
        object.remove(DEFAULT_VALUE);
    }
    Ok(())
}

/// The top-level property names whose canonical values differ between `a`
/// and `b` (a key on one side only counts), byte-sorted. Names only.
pub fn diff_properties(a: &Value, b: &Value) -> Vec<String> {
    let empty = Map::new();
    let left = a.as_object().unwrap_or(&empty);
    let right = b.as_object().unwrap_or(&empty);
    let mut keys: Vec<&String> = left.keys().chain(right.keys()).collect();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .filter(|key| {
            left.get(key.as_str()).map(canonical) != right.get(key.as_str()).map(canonical)
        })
        .cloned()
        .collect()
}

/// The configuration difference of one node at provider-relative `path`
/// between the repo and the gateway: the property names that differ once
/// R3-DV ([`absorb_default_value`], with `created`) and R3-VK
/// ([`strip_runtime_values`], both sides) are applied. Below `_types_/`
/// neither applies. Empty means equal.
pub fn config_diff(path: &str, repo: &Value, live: &Value, created: bool) -> Vec<String> {
    if in_types(path) {
        return diff_properties(repo, live);
    }
    let mut repo = repo.clone();
    let mut live = live.clone();
    // A seed mismatch leaves `defaultValue` on the live side only, so the
    // property comparison below names it.
    let _ = absorb_default_value(&repo, &mut live, created);
    strip_runtime_values(&mut repo);
    strip_runtime_values(&mut live);
    diff_properties(&repo, &live)
}

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
    let bad = |why: &str| {
        CoreError::Internal(format!(
            "the tag export of provider {provider} is not a tag tree: {why}"
        ))
    };
    let root: Value = serde_json::from_slice(bytes).map_err(|_| bad("not valid JSON"))?;
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
    };
    const STRIP_CREATED: CanonOpts = CanonOpts {
        strip_values: true,
        created: true,
    };
    const KEEP: CanonOpts = CanonOpts {
        strip_values: false,
        created: false,
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
        assert_eq!(canonicalize(&repo, "F/T", KEEP), canonicalize(&live, "F/T", KEEP));
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
        let live = json!({"tagType":"UdtType","parameters":{"P1":{"dataType":"Integer","value":null}}});
        assert!(nodes_equal(&repo, &live, "_types_/Ty", STRIP));
        let live = json!({"tagType":"UdtType","parameters":{"P1":{"dataType":"Integer","value":7}}});
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
        let mut tag = json!({"tagType":"AtomicTag","value":1,"timestamp":2,"quality":3,"defaultValue":4});
        strip_runtime_values(&mut tag, "F/T");
        assert_eq!(tag, json!({"tagType":"AtomicTag","defaultValue":4}));
        // Nothing below _types_/: a member value is the type's default.
        let ty = json!({"tagType":"UdtType","tags":[{"name":"M","tagType":"AtomicTag","value":0}]});
        let mut kept = ty.clone();
        strip_runtime_values(&mut kept, "_types_/Ty");
        assert_eq!(kept, ty);
        assert_eq!(canonicalize(&ty, "_types_/Ty", STRIP)["tags"][0]["value"], 0);
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
            ["alarms.HighAlarm.priority", "documentation", "tags.Member1.value"]
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
        let repo = json!({"tagType":"UdtType","tags":[{"name":"M1","tagType":"AtomicTag","value":0}]});
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
        let repo = json!({"tagType":"UdtInstance","tags":[{"name":"M1","tagType":"AtomicTag","value":5}]});
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
        let flat = |v: &Value| flatten_export(&serde_json::to_vec(v).expect("json"), "P26C12").expect("flat");
        let (repo, live) = (flat(&sent), flat(&exported));
        assert_eq!(repo.keys().collect::<Vec<_>>(), live.keys().collect::<Vec<_>>());
        for (path, node) in &repo {
            for opts in [STRIP_CREATED, CanonOpts { strip_values: false, created: true }] {
                assert!(
                    diff_properties(node, &live[path], path, opts).is_empty(),
                    "{path}"
                );
            }
            // Without R3-DV, F/T.defaultValue is the only difference.
            let raw: Vec<String> = {
                let (a, b) = (canonical(node), canonical(&live[path]));
                let (a, b) = (a.as_object().expect("obj"), b.as_object().expect("obj"));
                let mut keys: Vec<String> = a.keys().chain(b.keys()).filter(|k| a.get(*k) != b.get(*k)).cloned().collect();
                keys.dedup();
                keys
            };
            let want: &[&str] = if path == "F/T" { &["defaultValue"] } else { &[] };
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
}
