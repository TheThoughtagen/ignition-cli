//! Tag groups of a project's owned providers (Phase 26, 26-10, D-09, D-12).

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use serde_json::{Value, json};

    use super::*;
    use crate::deploy::native_tags::{NativeNode, NodeKind};
    use crate::deploy::transport::{FileBlob, ObservedFiles};
    use crate::deploy::tree::sha256_hex;

    const FILE: &str = "tags/PlantA/.tag-groups.json";

    fn summary(groups: Value) -> BTreeMap<String, GroupSummary> {
        parse_group_summary(
            FILE,
            &serde_json::to_vec(&json!({ "tagGroups": groups })).unwrap(),
        )
        .expect("a valid summary")
    }

    fn blob(bytes: &[u8]) -> FileBlob {
        FileBlob {
            bytes: bytes.to_vec(),
            sha256: sha256_hex(bytes),
        }
    }

    fn unit_config(config: Value) -> Vec<u8> {
        serde_json::to_vec_pretty(&json!({ "config": config })).unwrap()
    }

    const RESOURCE: &[u8] =
        br#"{"scope":"A","version":1,"restricted":false,"overridable":true,"files":["config.json"],"attributes":{"uuid":"u-1"}}"#;

    /// The observed files holding `core` and `mode` units of `PlantA`, each a
    /// `(group, config)` pair with the resource file beside it.
    fn observed(core: &[(&str, Value)], mode: &[(&str, Value)]) -> ObservedFiles {
        let files = |units: &[(&str, Value)]| {
            let mut out = BTreeMap::new();
            for (group, config) in units {
                for (file, bytes) in [
                    ("config.json", unit_config(config.clone())),
                    ("resource.json", RESOURCE.to_vec()),
                ] {
                    out.insert(
                        RelPath::new(&format!("ignition/tag-group/PlantA/{group}/{file}")).unwrap(),
                        blob(&bytes),
                    );
                }
            }
            out
        };
        ObservedFiles {
            core: files(core),
            mode: files(mode),
            local_units: Vec::new(),
            unreadable: Vec::new(),
        }
    }

    fn findings(err: &crate::error::CoreError) -> String {
        err.to_string()
    }

    // ---- parse_group_summary ----------------------------------------------

    #[test]
    fn parse_reads_the_module_properties() {
        let groups = summary(json!([
            {"name": "Fast", "rate": 250, "mode": "Direct", "oneShot": false},
            {"name": "Slow", "rate": 10000, "leasedRate": 1000,
             "drivingExpression": "{[.]Run}", "drivingComparison": "Equal",
             "drivingComparisonValue": 1.0}
        ]));
        assert_eq!(groups.keys().collect::<Vec<_>>(), ["Fast", "Slow"]);
        assert_eq!(groups["Fast"].properties["rate"], json!(250));
        assert_eq!(groups["Slow"].properties.len(), 6);
        assert!(!groups["Fast"].properties.contains_key("name"));
    }

    #[test]
    fn parse_refuses_an_unknown_property_naming_the_file() {
        let bytes = br#"{"tagGroups":[{"name":"Fast","rate":250,"opcDataMode":"Polled"}]}"#;
        let err = parse_group_summary(FILE, bytes).expect_err("unknown property");
        assert_eq!(err.code(), "deploy_tree_invalid");
        let text = findings(&err);
        assert!(text.contains(FILE), "{text}");
        assert!(text.contains("opcDataMode"), "{text}");
    }

    #[test]
    fn parse_refuses_bad_shapes() {
        for bad in [
            &br#"[]"#[..],
            br#"{"groups":[]}"#,
            br#"{"tagGroups":[{"rate":250}]}"#,
            br#"{"tagGroups":[{"name":"Fast"},{"name":"Fast"}]}"#,
            br#"{"tagGroups":[{"name":"Fast","rate":"250"}]}"#,
            br#"{"tagGroups":[{"name":"Fast","oneShot":1}]}"#,
            br#"{"tagGroups":[{"name":"a/b"}]}"#,
            br#"{"tagGroups":[{"name":".."}]}"#,
            br#"{"tagGroups":[{"name":"CON"}]}"#,
            br#"{"tagGroups":[1]}"#,
            br#"{"tagGroups":[{"name":"Fast"}], "tagGroups":[]}"#,
        ] {
            let err = parse_group_summary(FILE, bad).expect_err(std::str::from_utf8(bad).unwrap());
            assert_eq!(
                err.code(),
                "deploy_tree_invalid",
                "{}",
                String::from_utf8_lossy(bad)
            );
            assert!(findings(&err).contains(FILE));
        }
    }

    // ---- render_group_units -----------------------------------------------

    #[test]
    fn render_keeps_live_fields_and_overrides_the_summary_ones() {
        let groups = summary(json!([{"name": "Fast", "rate": 250, "mode": "Direct"}]));
        let live = LiveGroups::from_observed(
            "PlantA",
            &observed(
                &[(
                    "Fast",
                    json!({"mode": "Leased", "rate": 1000, "opcDataMode": "Polled",
                           "optWriteTimeout": 45000}),
                )],
                &[],
            ),
        );
        let units =
            render_group_units("PlantA", &groups["Fast"], live.effective("Fast")).expect("renders");
        let paths: Vec<&str> = units.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(
            paths,
            [
                "ignition/tag-group/PlantA/Fast/config.json",
                "ignition/tag-group/PlantA/Fast/resource.json"
            ]
        );
        let config: Value = serde_json::from_slice(&units[0].1).unwrap();
        assert_eq!(
            config,
            json!({"config": {"mode": "Direct", "rate": 250, "opcDataMode": "Polled",
                              "optWriteTimeout": 45000}})
        );
        assert_eq!(units[1].1, RESOURCE, "the live resource file is kept");
    }

    #[test]
    fn render_gives_a_new_group_the_captured_defaults() {
        let groups = summary(json!([{"name": "Slow", "rate": 5000}]));
        let units = render_group_units("PlantA", &groups["Slow"], None).expect("renders");
        let config: Value = serde_json::from_slice(&units[0].1).unwrap();
        assert_eq!(
            config,
            json!({"config": {"mode": "Direct", "oneShot": false, "opcDataMode": "Subscribed",
                              "optWriteTimeout": 60000, "optWrites": false, "rate": 5000,
                              "readAfterWrite": false}})
        );
        let resource: Value = serde_json::from_slice(&units[1].1).unwrap();
        assert_eq!(
            resource,
            json!({"scope": "A", "version": 1, "restricted": false, "overridable": true,
                   "files": ["config.json"], "attributes": {}})
        );
        // The config file is in the gateway's own form (C14): two-space
        // indent, sorted keys, no trailing newline.
        let text = std::str::from_utf8(&units[0].1).unwrap();
        assert!(
            text.starts_with("{\n  \"config\": {\n    \"mode\""),
            "{text}"
        );
        assert!(!text.ends_with('\n'));
    }

    // ---- plan_groups ------------------------------------------------------

    #[test]
    fn plan_classifies_added_changed_removed_same_and_drift() {
        let groups = summary(json!([
            {"name": "Default", "rate": 1000},
            {"name": "Fast", "rate": 250},
            {"name": "New", "rate": 100}
        ]));
        let live = LiveGroups::from_observed(
            "PlantA",
            &observed(
                &[
                    ("Default", json!({"rate": 1000, "mode": "Direct"})),
                    ("Other", json!({"rate": 1000})),
                ],
                &[
                    ("Fast", json!({"rate": 500, "mode": "Direct"})),
                    ("Gone", json!({"rate": 1000})),
                ],
            ),
        );
        let plan = plan_groups(
            "PlantA",
            &groups,
            &live,
            &["Gone".to_string(), "Fast".to_string()],
            &BTreeSet::new(),
        )
        .expect("plans");
        assert_eq!(plan.provider, "PlantA");
        assert_eq!(plan.added, ["New"]);
        assert_eq!(plan.changed.len(), 1);
        assert_eq!(plan.changed[0].path, "Fast");
        assert_eq!(plan.changed[0].properties, ["rate"]);
        assert_eq!(plan.removed, ["Gone"]);
        assert_eq!(plan.drift, ["Other"], "live, not recorded, not in the repo");
        assert_eq!(plan.same, 1);
        assert!(plan.would_change());

        let writes: Vec<(&str, bool)> = plan
            .writes
            .iter()
            .map(|w| (w.path.as_str(), w.pre.is_some()))
            .collect();
        assert_eq!(
            writes,
            [
                ("ignition/tag-group/PlantA/Fast/config.json", true),
                ("ignition/tag-group/PlantA/New/config.json", false),
                ("ignition/tag-group/PlantA/New/resource.json", false),
            ],
            "a changed mode unit rewrites only its config; a new one gets both files"
        );
        let deletes: Vec<&str> = plan.deletes.iter().map(|d| d.path.as_str()).collect();
        assert_eq!(
            deletes,
            [
                "ignition/tag-group/PlantA/Gone/config.json",
                "ignition/tag-group/PlantA/Gone/resource.json"
            ]
        );
    }

    #[test]
    fn plan_reads_the_mode_copy_over_core() {
        let groups = summary(json!([{"name": "Fast", "rate": 250}]));
        let live = LiveGroups::from_observed(
            "PlantA",
            &observed(
                &[("Fast", json!({"rate": 1000}))],
                &[("Fast", json!({"rate": 250}))],
            ),
        );
        let plan = plan_groups("PlantA", &groups, &live, &[], &BTreeSet::new()).expect("plans");
        assert_eq!(plan.same, 1);
        assert!(plan.changed.is_empty() && plan.writes.is_empty());
        assert!(!plan.would_change());
    }

    #[test]
    fn plan_writes_a_core_only_change_into_the_mode_collection() {
        let groups = summary(json!([{"name": "Default", "rate": 2000}]));
        let live = LiveGroups::from_observed(
            "PlantA",
            &observed(&[("Default", json!({"rate": 1000, "mode": "Direct"}))], &[]),
        );
        let plan = plan_groups("PlantA", &groups, &live, &[], &BTreeSet::new()).expect("plans");
        assert_eq!(plan.changed[0].properties, ["rate"]);
        let writes: Vec<(&str, bool)> = plan
            .writes
            .iter()
            .map(|w| (w.path.as_str(), w.pre.is_some()))
            .collect();
        assert_eq!(
            writes,
            [
                ("ignition/tag-group/PlantA/Default/config.json", false),
                ("ignition/tag-group/PlantA/Default/resource.json", false),
            ]
        );
        let resource: Value = serde_json::from_slice(&plan.writes[1].bytes).unwrap();
        assert_eq!(resource["attributes"], json!({}), "no core uuid is copied");
    }

    #[test]
    fn a_referenced_group_is_never_removed() {
        let groups = summary(json!([]));
        let live =
            LiveGroups::from_observed("PlantA", &observed(&[], &[("Gone", json!({"rate": 1}))]));
        let referenced: BTreeSet<String> = ["Gone".to_string()].into();
        let plan = plan_groups("PlantA", &groups, &live, &["Gone".to_string()], &referenced)
            .expect("plans");
        assert!(plan.removed.is_empty() && plan.deletes.is_empty());
        assert_eq!(plan.drift, ["Gone"]);
    }

    #[test]
    fn a_recorded_group_only_in_core_is_not_deleted() {
        let groups = summary(json!([]));
        let live =
            LiveGroups::from_observed("PlantA", &observed(&[("Gone", json!({"rate": 1}))], &[]));
        let plan = plan_groups(
            "PlantA",
            &groups,
            &live,
            &["Gone".to_string()],
            &BTreeSet::new(),
        )
        .expect("plans");
        assert!(plan.removed.is_empty() && plan.deletes.is_empty());
        assert_eq!(plan.drift, ["Gone"]);
    }

    // ---- check_group_refs -------------------------------------------------

    fn node(path: &str, json: Value) -> NativeNode {
        NativeNode {
            path: path.to_string(),
            kind: NodeKind::Tag,
            json,
            file: format!("tags/PlantA/{path}.json"),
        }
    }

    #[test]
    fn group_refs_on_top_level_members_and_instances_must_exist() {
        let groups = summary(json!([{"name": "Fast"}]));
        let live: BTreeSet<String> = ["Default".to_string()].into();
        let ok = [
            node("A", json!({"tagType": "AtomicTag", "tagGroup": "Fast"})),
            node("B", json!({"tagType": "AtomicTag", "tagGroup": "Default"})),
            node("C", json!({"tagType": "AtomicTag"})),
        ];
        let refs: Vec<&NativeNode> = ok.iter().collect();
        check_group_refs("PlantA", &refs, &groups, &live).expect("every group exists");

        let bad = [
            node("D", json!({"tagType": "AtomicTag", "tagGroup": "fast"})),
            node(
                "I",
                json!({"tagType": "UdtInstance", "tagGroup": "Missing",
                       "tags": [{"name": "M", "tagType": "AtomicTag", "tagGroup": "Absent"}]}),
            ),
        ];
        let refs: Vec<&NativeNode> = bad.iter().collect();
        let err = check_group_refs("PlantA", &refs, &groups, &live).expect_err("missing groups");
        assert_eq!(err.code(), "deploy_tree_invalid");
        let text = findings(&err);
        for needle in ["D", "fast", "I", "Missing", "I/M", "Absent"] {
            assert!(text.contains(needle), "{needle}: {text}");
        }
    }
}
