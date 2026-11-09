//! Import batches of project-owned tags (Phase 26, 26-08, TAGDEP-05).

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
        assert!(text.contains("_types_/A") && text.contains("_types_/B"), "{text}");
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
            (batches[0].policy, batches[0].carry, batches[0].nodes.clone()),
            (CollisionPolicy::MergeOverwrite, false, vec!["Line/Q".to_string()])
        );
        assert_eq!(
            (batches[1].policy, batches[1].carry, batches[1].nodes.clone()),
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
        assert!(text.contains("Line/P") && text.contains("tooltip"), "{text}");
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
        let mut node = json!({"name": "N", "tagType": "AtomicTag", "valueSource": MEMORY, "value": 1});
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

    /// KL-5: only memory-sourced values are carried; an `opc` tag (and a tag
    /// with no `valueSource`) is sent stripped.
    #[test]
    fn carry_leaves_other_value_sources_stripped() {
        let mut node = json!({"tagType": "AtomicTag", "valueSource": "opc", "value": 1});
        carry_live_values(&mut node, "Line/O", &fresh_with_type());
        assert_eq!(node, json!({"tagType": "AtomicTag", "valueSource": "opc"}));
        let mut unsourced = json!({"tagType": "AtomicTag", "value": 1});
        carry_live_values(&mut unsourced, "Line/O", &fresh_with_type());
        assert_eq!(unsourced, json!({"tagType": "AtomicTag"}));
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
        let batches = batches(&repo, &plan_time, ExistingNodePolicy::OverwriteCarry).expect("plans");
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
        assert_eq!(ty["tags"][0]["value"], 2, "a type member's value is its default");
        assert_eq!(ty["tags"][0]["timestamp"], 3, "nothing below _types_/ is stripped");
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
        fn level(
            depth: usize,
            prefix: &str,
            repo_nodes: &mut Vec<(String, Value)>,
        ) -> Vec<Value> {
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
                live.push(json!({"name": format!("DriftDir{i}"), "tagType": "Folder", "tags": [
                    {"name": "X", "tagType": "AtomicTag"}
                ]}));
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
    fn no_body_exceeds_the_cap_unless_it_holds_one_node() {
        let mut nodes: Vec<(String, Value)> = vec![("Line".to_string(), json!({"tagType": "Folder"}))];
        for i in 0..40 {
            nodes.push((format!("Line/T{i:02}"), big(i, 40_000)));
        }
        nodes.push(("Line/Huge".to_string(), big(0, IMPORT_BATCH_MAX_BYTES + 10)));
        let refs: Vec<(&str, Value)> = nodes.iter().map(|(p, v)| (p.as_str(), v.clone())).collect();
        let repo = repo(&refs);
        let live = live(json!([{"name": "Line", "tagType": "Folder", "tags": []}]));
        let batches = batches(&repo, &live, ExistingNodePolicy::OverwriteCarry).expect("plans");
        assert!(batches.len() >= 4, "{}", batches.len());
        let mut carried_nodes = 0;
        for batch in &batches {
            let single = body(batch)["tags"].as_array().expect("tags").len() == 1;
            assert!(
                batch.body.len() <= IMPORT_BATCH_MAX_BYTES || single,
                "{} bytes in a batch of {} nodes",
                batch.body.len(),
                batch.nodes.len()
            );
            carried_nodes += batch.nodes.len();
        }
        assert_eq!(carried_nodes, 41);
        let huge = batches
            .iter()
            .find(|b| b.nodes == ["Line/Huge"])
            .expect("the huge node goes alone");
        assert!(huge.body.len() > IMPORT_BATCH_MAX_BYTES);
    }

    /// A carried batch is measured after the carry: values that only the
    /// live side holds still keep it under the cap, and a fresh export whose
    /// values grew splits the batch at send time.
    #[test]
    fn a_carried_batch_is_measured_after_the_carry() {
        let mut nodes: Vec<(String, Value)> = vec![("Line".to_string(), json!({"tagType": "Folder"}))];
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
        let batches = batches(&repo, &plan_time, ExistingNodePolicy::OverwriteCarry).expect("plans");
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
            .map(|i| json!({"name": format!("S{i}"), "tagType": "AtomicTag",
                "valueSource": "memory", "dataType": "String", "tooltip": "t",
                "value": "v".repeat(400_000)}))
            .collect();
        let fresh = live(json!([{"name": "Line", "tagType": "Folder", "tags": grown}]));
        let two = batches
            .iter()
            .find(|b| b.nodes.len() >= 2)
            .expect("a batch of two");
        match carried(two, &fresh).expect("carried") {
            Carried::Split(parts) => {
                assert!(parts.len() >= 2);
                assert!(parts.iter().all(|p| p.carry && p.policy == CollisionPolicy::Overwrite));
                let names: usize = parts.iter().map(|p| p.nodes.len()).sum();
                assert_eq!(names, two.nodes.len());
            }
            Carried::Body(_) => panic!("over the cap with two nodes"),
        }
    }
}
