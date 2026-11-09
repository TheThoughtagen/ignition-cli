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

    #[test]
    fn runtime_values_are_stripped_from_atomic_tags_only() {
        let mut node = json!({
            "tagType": "UdtInstance",
            "value": "kept: not an AtomicTag",
            "parameters": {"P1": {"dataType": "Integer", "value": 7}},
            "tags": [
                {"name": "M1", "tagType": "AtomicTag", "value": 1, "timestamp": 2, "quality": 3, "documentation": "d"},
                {"name": "S", "tagType": "UdtInstance", "tags": [{"name": "M", "tagType": "AtomicTag", "value": 4}]}
            ]
        });
        strip_runtime_values(&mut node);
        assert_eq!(node["value"], "kept: not an AtomicTag");
        assert_eq!(
            node["parameters"]["P1"]["value"], 7,
            "parameters are config"
        );
        assert_eq!(
            node["tags"][0],
            json!({"name":"M1","tagType":"AtomicTag","documentation":"d"})
        );
        assert_eq!(
            node["tags"][1]["tags"][0],
            json!({"name":"M","tagType":"AtomicTag"})
        );
    }

    #[test]
    fn default_value_rule_r3_dv() {
        let repo = json!({"tagType":"AtomicTag","value":3});
        // C12: the gateway adds defaultValue equal to the sent value.
        let mut live = json!({"tagType":"AtomicTag","value":3,"defaultValue":3});
        absorb_default_value(&repo, &mut live, true).expect("the seed matches");
        assert!(live.get("defaultValue").is_none());
        // On a created node a different seed is the property defaultValue.
        let mut live = json!({"tagType":"AtomicTag","value":3,"defaultValue":4});
        assert_eq!(
            absorb_default_value(&repo, &mut live, true),
            Err("defaultValue".to_string())
        );
        assert_eq!(live["defaultValue"], 4, "left for the comparison to name");
        // On an existing node the live defaultValue is the gateway's own.
        absorb_default_value(&repo, &mut live, false).expect("existing");
        assert!(live.get("defaultValue").is_none());
        // A repo defaultValue is configuration and is never removed.
        let repo = json!({"tagType":"AtomicTag","defaultValue":5});
        let mut live = json!({"tagType":"AtomicTag","defaultValue":6});
        absorb_default_value(&repo, &mut live, true).expect("compared, not absorbed");
        assert_eq!(live["defaultValue"], 6);
    }

    #[test]
    fn config_diff_names_properties_never_values() {
        let repo = json!({"tagType":"AtomicTag","value":3,"documentation":"a"});
        let live = json!({"tagType":"AtomicTag","value":99,"defaultValue":3,"documentation":"a"});
        assert!(
            config_diff("F/T", &repo, &live, true).is_empty(),
            "R3-VK and R3-DV"
        );
        let live = json!({"tagType":"AtomicTag","value":3,"defaultValue":8,"documentation":"b"});
        assert_eq!(
            config_diff("F/T", &repo, &live, true),
            ["defaultValue", "documentation"]
        );
        // Below _types_/ a member value is the type's default: configuration.
        let repo =
            json!({"tagType":"UdtType","tags":[{"name":"M","tagType":"AtomicTag","value":0}]});
        let live =
            json!({"tagType":"UdtType","tags":[{"name":"M","tagType":"AtomicTag","value":1}]});
        assert_eq!(config_diff("_types_/Ty", &repo, &live, false), ["tags"]);
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
