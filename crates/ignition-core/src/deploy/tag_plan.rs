//! The plan of a project's owned tags against the gateway (Phase 26).
//!
//! [`plan_tags`] compares each owned provider as the repo holds it
//! ([`NativeProvider`]) with the provider's live export cut into the same
//! nodes ([`super::tag_canon::flatten_export`]):
//!
//! - **added**: a repo node the gateway lacks. Created with the repo's JSON as
//!   it is, values included (D-03, D-10: repo values only seed new tags).
//! - **changed**: present on both sides, and the configuration differs once
//!   the AtomicTag runtime keys are removed on both sides and R3-DV is applied
//!   ([`super::tag_canon::diff_properties`]). An instance `parameters` value or a
//!   member override is configuration, so it counts.
//! - **same**: present on both sides and equal configuration. A value that
//!   differs only at runtime is kept live and never counted (D-03).
//! - **drift**: a live node the repo lacks. Kept and reported, never deleted
//!   (D-01).
//!
//! Pure. Plan 26-07 replaces this thin planner with the full one (removals
//! from the deploy record, kept values, `plan_sha256`).

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::Value;

use super::native_tags::NativeProvider;
use super::tag_canon::{CanonOpts, canonical, diff_properties, in_types, strip_runtime_values};
use super::tree::sha256_hex;

/// One changed node: its provider-relative path and the names of the
/// properties that differ. Never a value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NodeChange {
    /// Provider-relative path.
    pub path: String,
    /// The differing property names, byte-sorted.
    pub properties: Vec<String>,
}

/// The plan of one owned provider. Paths are provider-relative and
/// byte-sorted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderPlan {
    /// The provider's name.
    pub provider: String,
    /// Repo nodes the gateway lacks.
    pub added: Vec<String>,
    /// Nodes whose configuration differs.
    pub changed: Vec<NodeChange>,
    /// Nodes equal on both sides.
    pub same: usize,
    /// Live nodes the repo lacks (D-01).
    pub drift: Vec<String>,
}

/// The plan of every owned provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagPlan {
    /// One plan per owned provider, byte-sorted by name.
    pub providers: Vec<ProviderPlan>,
    /// sha256 over the repo's owned tag content: the byte-sorted list of
    /// `(provider, path, canonical JSON)`, values included. Offline.
    pub tags_sha256: String,
    /// sha256 over the owned providers' live nodes as the plan read them, in
    /// canonical form with the AtomicTag runtime keys removed (R3-VK), so a
    /// runtime write never changes it.
    pub live_sha256: String,
}

impl TagPlan {
    /// Whether applying the plan would change the gateway.
    pub fn would_change(&self) -> bool {
        self.providers
            .iter()
            .any(|p| !p.added.is_empty() || !p.changed.is_empty())
    }
}

/// Plan `repo` (the owned providers) against `live` (each provider's flattened
/// export, keyed by the provider's name). A provider `live` lacks is planned
/// against an empty export.
pub fn plan_tags(
    repo: &[NativeProvider],
    live: &BTreeMap<String, BTreeMap<String, Value>>,
) -> TagPlan {
    let empty = BTreeMap::new();
    let mut owned: Vec<&NativeProvider> = repo.iter().collect();
    owned.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()));
    let mut providers = Vec::new();
    let mut live_digest = Vec::new();
    for provider in owned {
        let live_nodes = live.get(&provider.name).unwrap_or(&empty);
        let mut plan = ProviderPlan {
            provider: provider.name.clone(),
            added: Vec::new(),
            changed: Vec::new(),
            same: 0,
            drift: Vec::new(),
        };
        for (path, node) in &provider.nodes {
            match live_nodes.get(path) {
                None => plan.added.push(path.clone()),
                Some(live_node) => {
                    let properties = diff_properties(
                        &node.json,
                        live_node,
                        path,
                        CanonOpts {
                            strip_values: true,
                            created: false,
                        },
                    );
                    if properties.is_empty() {
                        plan.same += 1;
                    } else {
                        plan.changed.push(NodeChange {
                            path: path.clone(),
                            properties,
                        });
                    }
                }
            }
        }
        for (path, live_node) in live_nodes {
            if !provider.nodes.contains_key(path) {
                plan.drift.push(path.clone());
            }
            let mut stripped = live_node.clone();
            strip_runtime_values(&mut stripped, path);
            live_digest.push(Value::Array(vec![
                Value::String(provider.name.clone()),
                Value::String(path.clone()),
                canonical(&stripped),
            ]));
        }
        providers.push(plan);
    }
    TagPlan {
        providers,
        tags_sha256: repo_tags_sha256(repo),
        live_sha256: digest(live_digest),
    }
}

/// [`TagPlan::tags_sha256`] of `repo`, offline: the byte-sorted list of
/// `(provider, path, canonical JSON)` over every node of every owned
/// provider, values included. `ign deploy projects --tags-sha256` is compared
/// with it before any contact (26-05, TAGDEP-10).
pub fn repo_tags_sha256(repo: &[NativeProvider]) -> String {
    let entries = repo
        .iter()
        .flat_map(|provider| {
            provider.nodes.iter().map(|(path, node)| {
                Value::Array(vec![
                    Value::String(provider.name.clone()),
                    Value::String(path.clone()),
                    canonical(&node.json),
                ])
            })
        })
        .collect();
    digest(entries)
}

/// sha256 of the compact JSON of `entries` sorted by provider then path
/// (bytes). The JSON array frames every entry, so no two lists share bytes.
fn digest(mut entries: Vec<Value>) -> String {
    let key = |v: &Value| -> (Vec<u8>, Vec<u8>) {
        let part = |i: usize| v[i].as_str().unwrap_or("").as_bytes().to_vec();
        (part(0), part(1))
    };
    entries.sort_by_key(key);
    sha256_hex(&serde_json::to_vec(&Value::Array(entries)).unwrap_or_default())
}

/// One provider's counts in `data.tags.providers`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProviderSummary {
    /// The provider's name.
    pub provider: String,
    /// How many nodes would be added.
    pub added: usize,
    /// How many nodes differ in configuration.
    pub changed: usize,
    /// How many nodes are equal.
    pub same: usize,
    /// How many live nodes the repo lacks (kept).
    pub drift: usize,
}

/// One changed node in `data.tags.changed`: the full tag path
/// (`[provider]path`) and the property names that differ.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChangedTag {
    /// `[provider]path`.
    pub path: String,
    /// The differing property names.
    pub properties: Vec<String>,
}

/// `data.tags` of `ign deploy projects`: null for a project that owns no tag
/// provider. Paths are full tag paths (`[provider]path`), byte-sorted and
/// capped with the full count beside each list. Names, counts and hashes
/// only, never a value. Later plans add keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TagsReport {
    /// One entry per owned provider, byte-sorted by name.
    pub providers: Vec<ProviderSummary>,
    /// [`TagPlan::tags_sha256`].
    pub tags_sha256: String,
    /// [`TagPlan::live_sha256`].
    pub live_sha256: String,
    /// Nodes the run creates.
    pub added: Vec<String>,
    /// Full count of `added`.
    pub added_count: usize,
    /// Nodes whose configuration differs.
    pub changed: Vec<ChangedTag>,
    /// Full count of `changed`.
    pub changed_count: usize,
    /// Live nodes the repo lacks: kept, never deleted (D-01).
    pub drift: Vec<String>,
    /// Full count of `drift`.
    pub drift_count: usize,
    /// Warnings about the owned providers (26-05): for example that no
    /// gateway-config manifest was found to check ownership against. Texts
    /// name paths and providers only.
    pub warnings: Vec<String>,
}

impl TagsReport {
    /// The report of `plan`, each list capped at `cap` entries.
    pub fn from_plan(plan: &TagPlan, cap: usize) -> TagsReport {
        let full = |provider: &str, path: &str| format!("[{provider}]{path}");
        let mut added = Vec::new();
        let mut changed = Vec::new();
        let mut drift = Vec::new();
        for p in &plan.providers {
            added.extend(p.added.iter().map(|path| full(&p.provider, path)));
            changed.extend(p.changed.iter().map(|c| ChangedTag {
                path: full(&p.provider, &c.path),
                properties: c.properties.clone(),
            }));
            drift.extend(p.drift.iter().map(|path| full(&p.provider, path)));
        }
        added.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        changed.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
        drift.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        let (added_count, changed_count, drift_count) = (added.len(), changed.len(), drift.len());
        added.truncate(cap);
        changed.truncate(cap);
        drift.truncate(cap);
        TagsReport {
            providers: plan
                .providers
                .iter()
                .map(|p| ProviderSummary {
                    provider: p.provider.clone(),
                    added: p.added.len(),
                    changed: p.changed.len(),
                    same: p.same,
                    drift: p.drift.len(),
                })
                .collect(),
            tags_sha256: plan.tags_sha256.clone(),
            live_sha256: plan.live_sha256.clone(),
            added,
            added_count,
            changed,
            changed_count,
            drift,
            drift_count,
            warnings: Vec::new(),
        }
    }
}

/// The added nodes of `plan` that head an import: an added node whose parent
/// is not itself added (its parent exists live, or it sits at the provider
/// root). Types first, then the rest, each in byte order of the path. Every
/// added node below a head travels inside the head's body.
pub fn import_heads(plan: &ProviderPlan) -> Vec<String> {
    let added: BTreeSet<&str> = plan.added.iter().map(String::as_str).collect();
    let mut heads: Vec<String> = plan
        .added
        .iter()
        .filter(|path| parent_of(path).is_none_or(|parent| !added.contains(parent)))
        .cloned()
        .collect();
    heads.sort_by(|a, b| (!in_types(a), a.as_bytes()).cmp(&(!in_types(b), b.as_bytes())));
    heads
}

/// The provider-relative parent of `path`, or `None` at the provider root.
pub fn parent_of(path: &str) -> Option<&str> {
    path.rsplit_once('/').map(|(parent, _)| parent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deploy::native_tags::{NativeNode, NodeKind};
    use serde_json::json;

    fn provider(name: &str, nodes: &[(&str, Value)]) -> NativeProvider {
        NativeProvider {
            name: name.to_string(),
            nodes: nodes
                .iter()
                .map(|(path, json)| {
                    (
                        (*path).to_string(),
                        NativeNode {
                            path: (*path).to_string(),
                            kind: if json["tagType"] == "Folder" {
                                NodeKind::Folder
                            } else {
                                NodeKind::Tag
                            },
                            json: json.clone(),
                            file: format!("tags/{name}/{path}.json"),
                        },
                    )
                })
                .collect(),
            groups: None,
        }
    }

    fn live(name: &str, nodes: &[(&str, Value)]) -> BTreeMap<String, BTreeMap<String, Value>> {
        BTreeMap::from([(
            name.to_string(),
            nodes
                .iter()
                .map(|(p, v)| ((*p).to_string(), v.clone()))
                .collect(),
        )])
    }

    #[test]
    fn plan_classes_added_changed_same_drift() {
        let repo = provider(
            "PlantA",
            &[
                ("Line1", json!({"tagType":"Folder"})),
                ("Line1/New", json!({"tagType":"AtomicTag","value":1})),
                (
                    "Line1/Doc",
                    json!({"tagType":"AtomicTag","documentation":"new"}),
                ),
                ("Line1/Run", json!({"tagType":"AtomicTag","value":1})),
            ],
        );
        let live = live(
            "PlantA",
            &[
                ("Line1", json!({"tagType":"Folder"})),
                (
                    "Line1/Doc",
                    json!({"tagType":"AtomicTag","documentation":"old"}),
                ),
                // Only the runtime value and the gateway's defaultValue differ.
                (
                    "Line1/Run",
                    json!({"tagType":"AtomicTag","value":9,"defaultValue":1}),
                ),
                ("Line1/Extra", json!({"tagType":"AtomicTag"})),
            ],
        );
        let plan = plan_tags(&[repo], &live, &BTreeMap::new());
        let p = &plan.providers[0];
        assert_eq!(p.added, ["Line1/New"]);
        assert_eq!(
            p.changed,
            [NodeChange {
                path: "Line1/Doc".into(),
                properties: vec!["documentation".into()],
                restart_likely: false,
            }]
        );
        assert_eq!(p.same, 1);
        assert_eq!(p.kept_values, ["Line1/Run"], "only the value differs");
        assert_eq!(p.drift, ["Line1/Extra"]);
        assert!(plan.would_change());

        let report = TagsReport::from_plan(&plan, 200);
        assert_eq!(report.added, ["[PlantA]Line1/New"]);
        assert_eq!(report.drift, ["[PlantA]Line1/Extra"]);
        assert_eq!(report.changed[0].path, "[PlantA]Line1/Doc");
        let json = serde_json::to_string(&report).expect("json");
        assert!(
            !json.contains("\"old\"") && !json.contains("\"new\""),
            "{json}"
        );
    }

    #[test]
    fn live_sha256_ignores_runtime_values_and_child_order() {
        let repo = provider("P", &[]);
        let a = live(
            "P",
            &[(
                "T",
                json!({"tagType":"AtomicTag","value":1,"documentation":"d"}),
            )],
        );
        let b = live(
            "P",
            &[(
                "T",
                json!({"documentation":"d","tagType":"AtomicTag","value":2}),
            )],
        );
        let c = live(
            "P",
            &[("T", json!({"tagType":"AtomicTag","documentation":"e"}))],
        );
        let sha = |l| plan_tags(std::slice::from_ref(&repo), &l, &BTreeMap::new()).live_sha256;
        assert_eq!(sha(a.clone()), sha(b));
        assert_ne!(sha(a), sha(c));
    }

    #[test]
    fn tags_sha256_covers_repo_content_including_values() {
        let one = provider("P", &[("T", json!({"tagType":"AtomicTag","value":1}))]);
        let two = provider("P", &[("T", json!({"tagType":"AtomicTag","value":2}))]);
        let none = BTreeMap::new();
        let sha = |p: &NativeProvider| {
            plan_tags(std::slice::from_ref(p), &none, &BTreeMap::new()).tags_sha256
        };
        assert_ne!(sha(&one), sha(&two));
        assert_eq!(sha(&one), sha(&one.clone()));
    }

    #[test]
    fn import_heads_are_the_topmost_added_nodes_types_first() {
        let plan = ProviderPlan {
            provider: "P".into(),
            added: vec![
                "A".into(),
                "A/B".into(),
                "A/B/C".into(),
                "Live/X".into(),
                "_types_/Ty".into(),
            ],
            ..ProviderPlan::default()
        };
        assert_eq!(import_heads(&plan), ["_types_/Ty", "A", "Live/X"]);
    }

    /// `tags_sha256` of the neutral fixture provider, pinned at the value the
    /// 26-03 build computed: the 26-04 reader and canonicalizer must not move
    /// it for the same repo content.
    #[test]
    fn tags_sha256_of_the_fixture_is_pinned() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/native-tags/good");
        let provider = crate::deploy::native_tags::read_provider(&repo, "PlantA").expect("fixture");
        let plan = plan_tags(&[provider], &BTreeMap::new(), &BTreeMap::new());
        assert_eq!(
            plan.tags_sha256,
            "e909778a479f949ffd1c2deb8ba1e228de0b81d69bb1ba1a0662d9334d7fc3d5"
        );
    }

    /// The live nodes of one provider, keyed by path.
    fn nodes(entries: &[(&str, Value)]) -> BTreeMap<String, Value> {
        entries
            .iter()
            .map(|(p, v)| ((*p).to_string(), v.clone()))
            .collect()
    }

    /// A previous deploy's record of `provider` listing `nodes`.
    fn record(provider: &str, nodes: &[&str], run_id: &str) -> TagRecord {
        let mut nodes: Vec<String> = nodes.iter().map(|n| (*n).to_string()).collect();
        nodes.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        TagRecord {
            schema_version: 1,
            gateway: "gw".to_string(),
            provider: provider.to_string(),
            project: "Proj".to_string(),
            release: "v1.0.0".to_string(),
            run_id: run_id.to_string(),
            deployed_at: "2026-11-01T10:00:00Z".to_string(),
            tags_sha256: "ab".repeat(32),
            nodes,
            groups: Vec::new(),
        }
    }

    fn folder() -> Value {
        json!({"tagType":"Folder"})
    }

    fn tag(value: i64) -> Value {
        json!({"tagType":"AtomicTag","valueSource":"memory","value":value})
    }

    /// added: a repo node the gateway lacks; a recorded one is also
    /// `recreated`.
    #[test]
    fn added_and_recorded_absent_live_is_recreated() {
        let repo = provider("P", &[("A", tag(1)), ("B", tag(2))]);
        let rec = record("P", &["B"], "run-1");
        let plan = plan_provider(&repo, &BTreeMap::new(), Some(&rec));
        assert_eq!(plan.added, ["A", "B"]);
        assert_eq!(plan.recreated, ["B"]);
        assert!(plan.removed.is_empty() && plan.drift.is_empty());
        assert_eq!(plan.record_run_id.as_deref(), Some("run-1"));
    }

    /// changed: configuration differs with the runtime keys stripped outside
    /// `_types_/`; instance parameters, member overrides and type member
    /// defaults are configuration, so they are `changed`, never
    /// `kept_values`.
    #[test]
    fn changed_carries_property_names_and_configuration_values() {
        let repo = provider(
            "P",
            &[
                (
                    "F/Doc",
                    json!({"tagType":"AtomicTag","documentation":"new","value":1}),
                ),
                (
                    "F/Inst",
                    json!({"tagType":"UdtInstance","typeId":"Ty","parameters":{"P1":{"dataType":"Integer","value":7}}}),
                ),
                (
                    "F/Over",
                    json!({"tagType":"UdtInstance","typeId":"Ty","tags":[{"name":"M","tagType":"AtomicTag","tooltip":"t"}]}),
                ),
                (
                    "_types_/Ty",
                    json!({"tagType":"UdtType","tags":[{"name":"M","tagType":"AtomicTag","value":0}]}),
                ),
            ],
        );
        let live = nodes(&[
            (
                "F/Doc",
                json!({"tagType":"AtomicTag","documentation":"old","value":5}),
            ),
            (
                "F/Inst",
                json!({"tagType":"UdtInstance","typeId":"Ty","parameters":{"P1":{"dataType":"Integer","value":8}}}),
            ),
            ("F/Over", json!({"tagType":"UdtInstance","typeId":"Ty"})),
            (
                "_types_/Ty",
                json!({"tagType":"UdtType","tags":[{"name":"M","tagType":"AtomicTag","value":4}]}),
            ),
        ]);
        let plan = plan_provider(&repo, &live, None);
        let got: Vec<(&str, Vec<&str>)> = plan
            .changed
            .iter()
            .map(|c| {
                (
                    c.path.as_str(),
                    c.properties.iter().map(String::as_str).collect(),
                )
            })
            .collect();
        assert_eq!(
            got,
            [
                ("F/Doc", vec!["documentation"]),
                ("F/Inst", vec!["parameters.P1.value"]),
                ("F/Over", vec!["tags.M.tooltip"]),
                ("_types_/Ty", vec!["tags.M.value"]),
            ]
        );
        assert!(plan.kept_values.is_empty(), "{:?}", plan.kept_values);
        assert!(plan.changed.iter().all(|c| !c.restart_likely));
    }

    /// kept_values: equal configuration, different value (D-03). Reported,
    /// never applied. Only AtomicTag runtime keys put a node here.
    #[test]
    fn kept_values_is_a_value_only_difference() {
        let repo = provider(
            "P",
            &[
                ("T", tag(1)),
                (
                    "I",
                    json!({"tagType":"UdtInstance","typeId":"Ty","tags":[{"name":"M","tagType":"AtomicTag","value":1}]}),
                ),
                ("Same", tag(3)),
            ],
        );
        let live = nodes(&[
            ("T", tag(2)),
            (
                "I",
                json!({"tagType":"UdtInstance","typeId":"Ty","tags":[{"name":"M","tagType":"AtomicTag","value":9}]}),
            ),
            ("Same", tag(3)),
        ]);
        let plan = plan_provider(&repo, &live, None);
        assert_eq!(plan.kept_values, ["I", "T"]);
        assert!(plan.changed.is_empty());
        assert_eq!(plan.same, 1);
    }

    /// R3-DV on existing nodes: a live defaultValue the repo file lacks never
    /// makes the node changed (the C12 shape is kept_values); a repo
    /// defaultValue that differs is changed.
    #[test]
    fn existing_default_value_follows_r3_dv() {
        let repo = provider(
            "P",
            &[
                ("C12", json!({"tagType":"AtomicTag","value":3})),
                ("Dv", json!({"tagType":"AtomicTag","value":3,"defaultValue":3})),
            ],
        );
        let live = nodes(&[
            ("C12", json!({"tagType":"AtomicTag","value":9,"defaultValue":3})),
            ("Dv", json!({"tagType":"AtomicTag","value":3,"defaultValue":4})),
        ]);
        let plan = plan_provider(&repo, &live, None);
        assert_eq!(plan.kept_values, ["C12"]);
        assert_eq!(plan.changed.len(), 1);
        assert_eq!(plan.changed[0].path, "Dv");
        assert_eq!(plan.changed[0].properties, ["defaultValue"]);
    }

    /// removed: recorded, gone from the repo, present live (D-02);
    /// already_absent when also gone live.
    #[test]
    fn removed_needs_the_record_and_the_live_node() {
        let repo = provider("P", &[("Keep", tag(1))]);
        let live = nodes(&[("Keep", tag(1)), ("Gone", tag(2))]);
        let rec = record("P", &["Keep", "Gone", "Absent"], "run-1");
        let plan = plan_provider(&repo, &live, Some(&rec));
        assert_eq!(plan.removed, ["Gone"]);
        assert_eq!(plan.already_absent, ["Absent"]);
        assert!(plan.drift.is_empty());
        assert!(!plan.provider_emptied);
    }

    /// drift: live-only and unrecorded, kept (D-01). A removed folder is
    /// removed only when every live node below it is removed too; a drift
    /// node below keeps it, and both stay drift.
    #[test]
    fn drift_is_kept_and_holds_its_folder() {
        let repo = provider("P", &[("Other", tag(1))]);
        let live = nodes(&[
            ("F", folder()),
            ("F/T", tag(1)),
            ("F/Live", tag(2)),
            ("G", folder()),
            ("G/Sub", folder()),
            ("G/Sub/T", tag(3)),
            ("Other", tag(1)),
            ("Stray", tag(4)),
        ]);
        let rec = record("P", &["F", "F/T", "G", "G/Sub", "G/Sub/T", "Other"], "r");
        let plan = plan_provider(&repo, &live, Some(&rec));
        assert_eq!(plan.removed, ["F/T", "G", "G/Sub", "G/Sub/T"]);
        assert_eq!(plan.drift, ["F", "F/Live", "Stray"]);
    }

    /// restart_likely: a changed node whose tagType differs (N1).
    #[test]
    fn a_tag_type_change_is_restart_likely() {
        let repo = provider(
            "P",
            &[("W", json!({"tagType":"UdtInstance","typeId":"Ty"}))],
        );
        let live = nodes(&[("W", folder())]);
        let plan = plan_provider(&repo, &live, None);
        assert_eq!(plan.changed.len(), 1);
        assert!(plan.changed[0].restart_likely);
        assert_eq!(plan.changed[0].properties, ["tagType", "typeId"]);
    }

    /// The first deploy (no record): nothing is ever removed; an added or a
    /// changed node is a first change; all equal plans only the record.
    #[test]
    fn first_deploy_removes_nothing() {
        let repo = provider("P", &[("A", tag(1)), ("B", tag(1))]);
        let live = nodes(&[("A", tag(1)), ("B", tag(7)), ("X", tag(1))]);
        let equal = plan_provider(&repo, &live, None);
        assert!(equal.removed.is_empty() && equal.already_absent.is_empty());
        assert_eq!(equal.drift, ["X"]);
        assert!(equal.record_only && !equal.first_change);
        assert!(equal.record_run_id.is_none());

        let added = plan_provider(&provider("P", &[("A", tag(1)), ("N", tag(1))]), &live, None);
        assert!(added.first_change && !added.record_only);
        let changed = plan_provider(
            &provider("P", &[("A", json!({"tagType":"AtomicTag","tooltip":"x"}))]),
            &live,
            None,
        );
        assert!(changed.first_change && !changed.record_only);
        assert!(changed.removed.is_empty(), "no record, no removal");

        // A recorded provider is never a first deploy.
        let rec = record("P", &["A", "B"], "r");
        let later = plan_provider(&provider("P", &[("A", tag(1)), ("N", tag(1))]), &live, Some(&rec));
        assert!(!later.first_change && !later.record_only);
    }

    /// provider_emptied: the plan removes every node the record lists.
    #[test]
    fn removing_every_recorded_node_empties_the_provider() {
        let repo = provider("P", &[("New", tag(1))]);
        let live = nodes(&[("X", tag(1)), ("Y", tag(1))]);
        let plan = plan_provider(&repo, &live, Some(&record("P", &["X", "Y", "Z"], "r")));
        assert_eq!(plan.removed, ["X", "Y"]);
        assert_eq!(plan.already_absent, ["Z"]);
        assert!(plan.provider_emptied);
        let kept = plan_provider(
            &provider("P", &[("X", tag(1))]),
            &live,
            Some(&record("P", &["X", "Y"], "r")),
        );
        assert_eq!(kept.removed, ["Y"]);
        assert!(!kept.provider_emptied);
    }

    /// Records are per provider: another provider's record never removes a
    /// node of this one.
    #[test]
    fn another_providers_record_removes_nothing_here() {
        let a = provider("A", &[("Line1", folder())]);
        let b = provider("B", &[("Line1", folder())]);
        let mut live = live("A", &[("Line1", folder()), ("Line1/Extra", tag(1))]);
        live.insert(
            "B".to_string(),
            nodes(&[("Line1", folder()), ("Line1/Extra", tag(1))]),
        );
        let records = BTreeMap::from([(
            "B".to_string(),
            record("B", &["Line1", "Line1/Extra"], "r"),
        )]);
        let plan = plan_tags(&[b, a], &live, &records);
        let (pa, pb) = (&plan.providers[0], &plan.providers[1]);
        assert_eq!(pa.provider, "A");
        assert!(pa.removed.is_empty());
        assert_eq!(pa.drift, ["Line1/Extra"]);
        assert!(pa.record_run_id.is_none() && pa.record_only);
        assert_eq!(pb.removed, ["Line1/Extra"]);
    }

    /// plan_sha256 is stable for the same inputs and moves with any list, a
    /// record's run id and the live digest.
    #[test]
    fn plan_sha256_is_stable_and_covers_every_input() {
        let repo = provider("P", &[("A", tag(1)), ("B", tag(1))]);
        let base_live = live("P", &[("A", tag(1)), ("B", tag(1)), ("C", tag(1))]);
        let records = |run: &str| {
            BTreeMap::from([("P".to_string(), record("P", &["A", "B"], run))])
        };
        let sha = |repo: &NativeProvider, live: &BTreeMap<String, BTreeMap<String, Value>>, run: &str| {
            plan_tags(std::slice::from_ref(repo), live, &records(run)).plan_sha256
        };
        let base = sha(&repo, &base_live, "r1");
        assert_eq!(base.len(), 64);
        assert_eq!(base, sha(&repo, &base_live, "r1"), "stable");
        assert_ne!(base, sha(&repo, &base_live, "r2"), "record run id");
        // A list changes: C is no longer drift.
        let fewer = live("P", &[("A", tag(1)), ("B", tag(1))]);
        assert_ne!(base, sha(&repo, &fewer, "r1"), "drift list");
        // Only a runtime value differs: kept_values moves, live_sha256 does not.
        let valued = live("P", &[("A", tag(5)), ("B", tag(1)), ("C", tag(1))]);
        let p1 = plan_tags(std::slice::from_ref(&repo), &base_live, &records("r1"));
        let p2 = plan_tags(std::slice::from_ref(&repo), &valued, &records("r1"));
        assert_eq!(p1.live_sha256, p2.live_sha256);
        assert_ne!(p1.plan_sha256, p2.plan_sha256, "kept_values list");
        // live_sha256 moves on a configuration change of a drift node.
        let config = live(
            "P",
            &[
                ("A", tag(1)),
                ("B", tag(1)),
                ("C", json!({"tagType":"AtomicTag","valueSource":"memory","value":1,"tooltip":"t"})),
            ],
        );
        let p3 = plan_tags(std::slice::from_ref(&repo), &config, &records("r1"));
        assert_eq!(p1.providers, p3.providers, "the lists are equal");
        assert_ne!(p1.live_sha256, p3.live_sha256);
        assert_ne!(p1.plan_sha256, p3.plan_sha256, "live_sha256");
    }
}
