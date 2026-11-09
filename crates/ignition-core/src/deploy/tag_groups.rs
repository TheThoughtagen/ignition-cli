//! Tag groups of a project's owned providers (Phase 26, 26-10, D-09, D-12).
//!
//! 8.3.6 has no tag-group REST route (C14: `resources/list/ignition/tag-group`
//! answers 404, and `tags/export` carries no groups), so a provider's groups
//! are deployed as the gateway's own `tag-group` resource units,
//! `ignition/tag-group/<P>/<Group>/{config.json, resource.json}`, through the
//! file engine's host commit and a config scan (D-12). The repo side is the
//! git module's `tags/<P>/.tag-groups.json`, a lossy summary
//! (`GitTagGroupManager.serializeTagGroup`): each group's `name` and only
//! [`GROUP_PROPERTIES`]. So a group is rendered from its live unit with the
//! summary's properties laid over it, and every field the summary lacks keeps
//! its live value ([`render_group_units`]).
//!
//! [`plan_groups`] reads a provider's units in `core` and in the target's mode
//! collection, the mode copy overriding `core` as the gateway reads them, and
//! writes every change into the mode collection, where the file engine places
//! tag-group units (a pair syncs them there, p26-groups F-P7):
//!
//! - **added**: in the summary, not live: both files, with the gateway's
//!   defaults C14 recorded below the summary's properties;
//! - **changed**: a summary property differs from the effective live unit:
//!   `config.json`, and `resource.json` too when the mode collection has no
//!   copy yet (the `p26-groups` form, without the core copy's attributes);
//! - **removed**: this project's tag record lists it, the summary dropped it,
//!   and its unit is in the mode collection: both files are deleted;
//! - **drift**: live and neither in the summary nor removable: not recorded,
//!   only in `core` (a `core` path is never written for a tag group), or
//!   still named by a repo tag. Kept, never deleted;
//! - **same**: every summary property equals the live one.
//!
//! [`check_group_refs`] is R3-GRP: a tag naming a group the provider lacks
//! imports clean (C13, F11), so before any write every `tagGroup` a sent node
//! names must be in the summary or live.
//!
//! Pure: the caller reads the units and writes them.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::{Map, Value};

use super::component_problem;
use super::layout::RelPath;
use super::native_tags::NativeNode;
use super::node;
use super::tag_plan::NodeChange;
use super::transport::winps::windows_component_problem;
use super::transport::{FileBlob, ObservedFiles};
use crate::error::{CoreError, DeployFinding};

/// The properties `.tag-groups.json` may carry besides `name`, with the JSON
/// type each must have (`GitTagGroupManager.serializeTagGroup`). The unit's
/// `config` object uses the same names (the A2 tree and C14 units carry
/// `rate`, `mode` and `oneShot` under them; the leased and driven names are
/// the module's and are not captured in a unit).
pub const GROUP_PROPERTIES: [(&str, PropertyKind); 7] = [
    ("rate", PropertyKind::Integer),
    ("mode", PropertyKind::String),
    ("leasedRate", PropertyKind::Integer),
    ("drivingExpression", PropertyKind::String),
    ("oneShot", PropertyKind::Bool),
    ("drivingComparison", PropertyKind::String),
    ("drivingComparisonValue", PropertyKind::Number),
];

/// The JSON type a summary property must have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyKind {
    /// A whole number (`getAsInt`).
    Integer,
    /// Any number (`getAsDouble`).
    Number,
    /// A string.
    String,
    /// A boolean.
    Bool,
}

impl PropertyKind {
    fn holds(self, value: &Value) -> bool {
        match self {
            Self::Integer => value.as_i64().is_some(),
            Self::Number => value.is_number(),
            Self::String => value.is_string(),
            Self::Bool => value.is_boolean(),
        }
    }

    fn word(self) -> &'static str {
        match self {
            Self::Integer => "a whole number",
            Self::Number => "a number",
            Self::String => "a string",
            Self::Bool => "a boolean",
        }
    }
}

/// The `config` a new tag group's unit gets below the summary's properties:
/// the gateway's own `Default Historical` unit as C14 recorded it.
fn default_config() -> Map<String, Value> {
    let Value::Object(map) = serde_json::json!({
        "mode": "Direct",
        "oneShot": false,
        "opcDataMode": "Subscribed",
        "optWriteTimeout": 60000,
        "optWrites": false,
        "rate": 10000,
        "readAfterWrite": false
    }) else {
        unreachable!("a JSON object literal")
    };
    map
}

/// The `resource.json` of a unit the mode collection has no copy of: the
/// form `p26-groups` proved (no attributes, so no `core` uuid is copied),
/// compact with a final newline.
pub const NEW_RESOURCE_JSON: &[u8] =
    b"{\"scope\":\"A\",\"version\":1,\"restricted\":false,\"overridable\":true,\"files\":[\"config.json\"],\"attributes\":{}}\n";

/// The two files of a tag-group unit.
pub const UNIT_FILES: [&str; 2] = ["config.json", "resource.json"];

/// One group of `.tag-groups.json`.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupSummary {
    /// The group's name.
    pub name: String,
    /// Its [`GROUP_PROPERTIES`] as the summary holds them, `name` excluded.
    pub properties: BTreeMap<String, Value>,
}

/// Parse `bytes`, the summary file `file`, strictly: a JSON object holding
/// only `tagGroups`, an array of objects each with a `name` and only
/// [`GROUP_PROPERTIES`] of their types. A name must be one safe path
/// component on Linux and Windows (the unit is a directory of that name), and
/// names are unique. Every defect is `deploy_tree_invalid` naming `file`; no
/// message quotes a value.
pub fn parse_group_summary(
    file: &str,
    bytes: &[u8],
) -> Result<BTreeMap<String, GroupSummary>, CoreError> {
    node::classify(bytes).map_err(|why| invalid(file, format!("is not valid JSON: {why}")))?;
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| invalid(file, "is not valid JSON".to_string()))?;
    parse_group_value(file, &value)
}

/// [`parse_group_summary`] of a summary already read as JSON (the native
/// reader keeps `.tag-groups.json` as a value).
pub fn parse_group_value(
    file: &str,
    value: &Value,
) -> Result<BTreeMap<String, GroupSummary>, CoreError> {
    let Some(root) = value.as_object() else {
        return Err(invalid(
            file,
            "must be a JSON object holding `tagGroups`".to_string(),
        ));
    };
    if let Some(key) = root.keys().find(|k| *k != "tagGroups") {
        return Err(invalid(
            file,
            format!("holds the unknown key `{key}` (only `tagGroups` is read)"),
        ));
    }
    let Some(items) = root.get("tagGroups").and_then(Value::as_array) else {
        return Err(invalid(file, "`tagGroups` must be an array".to_string()));
    };
    let mut findings = Vec::new();
    let mut out = BTreeMap::new();
    for (i, item) in items.iter().enumerate() {
        let Some(object) = item.as_object() else {
            findings.push(finding(file, format!("tagGroups[{i}] must be an object")));
            continue;
        };
        let Some(name) = object.get("name").and_then(Value::as_str) else {
            findings.push(finding(
                file,
                format!("tagGroups[{i}] has no string `name`"),
            ));
            continue;
        };
        if let Some(why) = name_problem(name) {
            findings.push(finding(
                file,
                format!("tagGroups[{i}]: the group name {why}"),
            ));
            continue;
        }
        let mut properties = BTreeMap::new();
        for (key, value) in object {
            if key == "name" {
                continue;
            }
            match GROUP_PROPERTIES.iter().find(|(known, _)| known == key) {
                None => findings.push(finding(
                    file,
                    format!(
                        "tagGroups[{i}] ({name}) holds the unknown property `{key}`; the git \
                         module's summary carries only {}",
                        GROUP_PROPERTIES
                            .iter()
                            .map(|(k, _)| *k)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                )),
                Some((_, kind)) if !kind.holds(value) => findings.push(finding(
                    file,
                    format!("tagGroups[{i}] ({name}): `{key}` must be {}", kind.word()),
                )),
                Some(_) => {
                    properties.insert(key.clone(), value.clone());
                }
            }
        }
        if out.contains_key(name) {
            findings.push(finding(
                file,
                format!("tagGroups[{i}]: the group {name} is listed twice"),
            ));
            continue;
        }
        out.insert(
            name.to_string(),
            GroupSummary {
                name: name.to_string(),
                properties,
            },
        );
    }
    if !findings.is_empty() {
        return Err(CoreError::deploy_findings(findings));
    }
    Ok(out)
}

/// Why `name` cannot be a tag group's unit directory, or `None`: the tag-name
/// checks (Pitfall 8), on Linux and Windows.
fn name_problem(name: &str) -> Option<String> {
    if let Some(why) = component_problem(name) {
        return Some(why.to_string());
    }
    if name.chars().any(char::is_control) {
        return Some("holds a control character".to_string());
    }
    windows_component_problem(name)
}

fn finding(file: &str, message: String) -> DeployFinding {
    DeployFinding {
        file: file.to_string(),
        line: None,
        column: None,
        message,
    }
}

fn invalid(file: &str, why: String) -> CoreError {
    CoreError::deploy_findings(vec![finding(file, why)])
}

/// The files of one tag-group unit in one collection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LiveUnit {
    /// `config.json`, when present.
    pub config: Option<FileBlob>,
    /// `resource.json`, when present.
    pub resource: Option<FileBlob>,
}

/// One provider's tag-group units as a host holds them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LiveGroups {
    /// The units in `core`, keyed by group name.
    pub core: BTreeMap<String, LiveUnit>,
    /// The units in the target's mode collection, keyed by group name.
    pub mode: BTreeMap<String, LiveUnit>,
}

/// The collection path of `file` of `group`'s unit of `provider`.
pub fn unit_path(provider: &str, group: &str, file: &str) -> Result<RelPath, CoreError> {
    RelPath::new(&format!("ignition/tag-group/{provider}/{group}/{file}")).map_err(|why| {
        CoreError::Internal(format!(
            "tag group {group} of provider {provider} gives no unit path ({why})"
        ))
    })
}

impl LiveGroups {
    /// `provider`'s units in `observed`: every `ignition/tag-group/<P>/<G>/`
    /// `config.json` and `resource.json`; anything else is ignored.
    pub fn from_observed(provider: &str, observed: &ObservedFiles) -> LiveGroups {
        let prefix = format!("ignition/tag-group/{provider}/");
        let pick = |files: &BTreeMap<RelPath, FileBlob>| {
            let mut units: BTreeMap<String, LiveUnit> = BTreeMap::new();
            for (path, blob) in files {
                let Some(rest) = path.as_str().strip_prefix(&prefix) else {
                    continue;
                };
                let Some((group, file)) = rest.split_once('/') else {
                    continue;
                };
                let unit = units.entry(group.to_string()).or_default();
                match file {
                    "config.json" => unit.config = Some(blob.clone()),
                    "resource.json" => unit.resource = Some(blob.clone()),
                    _ => {}
                }
            }
            units
        };
        LiveGroups {
            core: pick(&observed.core),
            mode: pick(&observed.mode),
        }
    }

    /// Every group name live in either collection.
    pub fn names(&self) -> BTreeSet<String> {
        self.core.keys().chain(self.mode.keys()).cloned().collect()
    }

    /// The unit the gateway reads for `group`: the mode copy when the mode
    /// collection has one, else the `core` one.
    pub fn effective(&self, group: &str) -> Option<&LiveUnit> {
        self.mode.get(group).or_else(|| self.core.get(group))
    }
}

/// The `config` object of a live unit's `config.json`.
fn live_config(group: &str, unit: &LiveUnit) -> Result<(Map<String, Value>, Value), CoreError> {
    let bad =
        |why: &str| CoreError::Internal(format!("the live config.json of tag group {group} {why}"));
    let Some(config) = &unit.config else {
        return Err(bad("is missing"));
    };
    let doc: Value = serde_json::from_slice(&config.bytes).map_err(|_| bad("is not valid JSON"))?;
    let inner = doc
        .get("config")
        .and_then(Value::as_object)
        .cloned()
        .ok_or_else(|| bad("holds no `config` object"))?;
    Ok((inner, doc))
}

/// The two files of `summary`'s unit of `provider`, at their collection
/// paths: `config.json` is the live unit's document (or a new one holding
/// [`default_config`]) with the summary's properties laid over its `config`,
/// in the gateway's form (two-space indent, sorted keys, no final newline);
/// `resource.json` is the live unit's bytes, or [`NEW_RESOURCE_JSON`] for a
/// group with no live unit.
pub fn render_group_units(
    provider: &str,
    summary: &GroupSummary,
    live: Option<&LiveUnit>,
) -> Result<Vec<(RelPath, Vec<u8>)>, CoreError> {
    let (mut config, mut doc) = match live {
        Some(unit) => live_config(&summary.name, unit)?,
        None => (default_config(), Value::Object(Map::new())),
    };
    for (key, value) in &summary.properties {
        config.insert(key.clone(), value.clone());
    }
    if let Value::Object(map) = &mut doc {
        map.insert("config".to_string(), Value::Object(config));
    }
    let config_bytes = serde_json::to_vec_pretty(&doc)
        .map_err(|e| CoreError::Internal(format!("a tag-group unit did not serialize: {e}")))?;
    let resource = match live.and_then(|unit| unit.resource.as_ref()) {
        Some(blob) => blob.bytes.clone(),
        None => NEW_RESOURCE_JSON.to_vec(),
    };
    Ok(vec![
        (
            unit_path(provider, &summary.name, "config.json")?,
            config_bytes,
        ),
        (
            unit_path(provider, &summary.name, "resource.json")?,
            resource,
        ),
    ])
}

/// One file a group plan writes into the mode collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupWrite {
    /// The collection path (a mode-collection apply key).
    pub path: RelPath,
    /// The bytes to write.
    pub bytes: Vec<u8>,
    /// The mode collection's current hash of `path`; `None` when absent.
    pub pre: Option<String>,
}

/// One file a group plan deletes from the mode collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupDelete {
    /// The collection path (a mode-collection apply key).
    pub path: RelPath,
    /// The mode collection's current hash of `path`.
    pub pre: String,
}

/// The plan of one provider's tag groups: an entry of `data.tags.groups`.
/// Names and property names only. `writes` and `deletes` are the files, for
/// the run; they are not reported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GroupPlan {
    /// The provider.
    pub provider: String,
    /// Groups the run creates, byte-sorted.
    pub added: Vec<String>,
    /// Groups whose summary properties differ (`path` is the group name).
    pub changed: Vec<NodeChange>,
    /// Groups the run deletes, byte-sorted.
    pub removed: Vec<String>,
    /// Live groups the run keeps that the summary lacks, byte-sorted.
    pub drift: Vec<String>,
    /// How many groups are equal.
    pub same: usize,
    /// The files written, byte-sorted by path.
    #[serde(skip)]
    pub writes: Vec<GroupWrite>,
    /// The files deleted, byte-sorted by path.
    #[serde(skip)]
    pub deletes: Vec<GroupDelete>,
}

impl GroupPlan {
    /// Whether the plan writes or deletes anything.
    pub fn would_change(&self) -> bool {
        !self.writes.is_empty() || !self.deletes.is_empty()
    }
}

/// Plan `provider`'s groups: `summary` (the repo) against `live`, with the
/// groups this project's tag record lists (`recorded`) and the groups a repo
/// tag of the provider names (`referenced`). See the module doc.
pub fn plan_groups(
    provider: &str,
    summary: &BTreeMap<String, GroupSummary>,
    live: &LiveGroups,
    recorded: &[String],
    referenced: &BTreeSet<String>,
) -> Result<GroupPlan, CoreError> {
    let mut plan = GroupPlan {
        provider: provider.to_string(),
        added: Vec::new(),
        changed: Vec::new(),
        removed: Vec::new(),
        drift: Vec::new(),
        same: 0,
        writes: Vec::new(),
        deletes: Vec::new(),
    };
    for (name, group) in summary {
        let effective = live.effective(name);
        let in_mode = live.mode.get(name);
        let properties = match effective {
            None => None,
            Some(unit) => {
                let (config, _) = live_config(name, unit)?;
                let differing: Vec<String> = group
                    .properties
                    .iter()
                    .filter(|(key, value)| config.get(*key) != Some(*value))
                    .map(|(key, _)| key.clone())
                    .collect();
                Some(differing)
            }
        };
        match &properties {
            Some(differing) if differing.is_empty() => {
                plan.same += 1;
                continue;
            }
            Some(differing) => plan.changed.push(NodeChange {
                path: name.clone(),
                properties: differing.clone(),
                restart_likely: false,
            }),
            None => plan.added.push(name.clone()),
        }
        let units = render_group_units(provider, group, effective)?;
        for (path, bytes) in units {
            let file = path.as_str().rsplit('/').next().unwrap_or_default();
            let pre_blob = in_mode.and_then(|unit| match file {
                "config.json" => unit.config.as_ref(),
                _ => unit.resource.as_ref(),
            });
            if file == "resource.json" {
                // A mode copy keeps its own resource file; a group the mode
                // collection lacks gets the proven new form.
                if pre_blob.is_some() {
                    continue;
                }
                plan.writes.push(GroupWrite {
                    path,
                    bytes: NEW_RESOURCE_JSON.to_vec(),
                    pre: None,
                });
                continue;
            }
            plan.writes.push(GroupWrite {
                path,
                bytes,
                pre: pre_blob.map(|b| b.sha256.clone()),
            });
        }
    }
    let recorded: BTreeSet<&str> = recorded.iter().map(String::as_str).collect();
    for name in live.names() {
        if summary.contains_key(&name) {
            continue;
        }
        let removable = recorded.contains(name.as_str()) && !referenced.contains(&name);
        match live.mode.get(&name) {
            Some(unit) if removable => {
                for (file, blob) in [
                    ("config.json", &unit.config),
                    ("resource.json", &unit.resource),
                ] {
                    if let Some(blob) = blob {
                        plan.deletes.push(GroupDelete {
                            path: unit_path(provider, &name, file)?,
                            pre: blob.sha256.clone(),
                        });
                    }
                }
                plan.removed.push(name);
            }
            _ => plan.drift.push(name),
        }
    }
    plan.writes
        .sort_by(|a, b| a.path.as_str().as_bytes().cmp(b.path.as_str().as_bytes()));
    plan.deletes
        .sort_by(|a, b| a.path.as_str().as_bytes().cmp(b.path.as_str().as_bytes()));
    Ok(plan)
}

/// Every `tagGroup` string `json` names: its own and, through `tags`
/// arrays, those of its members and children, as `(member path, group)`.
fn group_refs(path: &str, json: &Value, out: &mut Vec<(String, String)>) {
    if let Some(group) = json.get("tagGroup").and_then(Value::as_str)
        && !group.is_empty()
    {
        out.push((path.to_string(), group.to_string()));
    }
    for child in json
        .get("tags")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let name = child.get("name").and_then(Value::as_str).unwrap_or("?");
        group_refs(&format!("{path}/{name}"), child, out);
    }
}

/// The groups any of `nodes` names (top level, members, children).
pub fn referenced_groups<'a>(nodes: impl IntoIterator<Item = &'a NativeNode>) -> BTreeSet<String> {
    let mut refs = Vec::new();
    for node in nodes {
        group_refs(&node.path, &node.json, &mut refs);
    }
    refs.into_iter().map(|(_, group)| group).collect()
}

/// R3-GRP: every `tagGroup` the `sent` nodes of `provider` name (their own,
/// their members' and their children's) must be a group of `summary` or of
/// `live`, compared exactly. Otherwise one `deploy_tree_invalid` with a
/// finding per node path and group, labelled by the node's repo file. The
/// import answer never reports a missing group (C13), so this is the only
/// check.
pub fn check_group_refs(
    provider: &str,
    sent: &[&NativeNode],
    summary: &BTreeMap<String, GroupSummary>,
    live: &BTreeSet<String>,
) -> Result<(), CoreError> {
    let mut findings = Vec::new();
    for node in sent {
        let mut refs = Vec::new();
        group_refs(&node.path, &node.json, &mut refs);
        for (path, group) in refs {
            if summary.contains_key(&group) || live.contains(&group) {
                continue;
            }
            findings.push(finding(
                &node.file,
                format!(
                    "tag {path} names tagGroup `{group}`, which is neither in \
                     tags/{provider}/.tag-groups.json nor a tag group of provider {provider} on \
                     the gateway; the import would accept it and the tag would never scan (C13)"
                ),
            ));
        }
    }
    if findings.is_empty() {
        Ok(())
    } else {
        Err(CoreError::deploy_findings(findings))
    }
}

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
        assert_eq!(groups["Slow"].properties.len(), 5);
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
