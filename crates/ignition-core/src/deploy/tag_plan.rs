//! The plan of a project's owned tags against the gateway (Phase 26,
//! TAGDEP-04, TAGDEP-11).
//!
//! [`plan_provider`] compares one owned provider as the repo holds it
//! ([`NativeProvider`]) with the provider's live export cut into the same
//! nodes ([`super::tag_canon::flatten_export`]) and with this project's
//! previous tag record of that provider ([`TagRecord`], 26-06). Every node
//! lands in exactly one class:
//!
//! - **added**: a repo node the gateway lacks. Created with the repo's JSON as
//!   it is, values included (D-03, D-10: repo values only seed new tags). A
//!   recorded node that is gone live is also listed in `recreated`.
//! - **changed**: present on both sides, and the configuration differs once
//!   the AtomicTag runtime keys are removed on both sides and R3-DV is applied
//!   ([`super::tag_canon::diff_properties`]). An instance `parameters` value, a
//!   member override and a type member's default (below `_types_/`) are
//!   configuration, so they count. A `tagType` difference is `restart_likely`
//!   (N1).
//! - **kept_values**: equal configuration, but a runtime value differs. Only
//!   AtomicTag runtime keys can put a node here. Reported, never applied
//!   (D-03).
//! - **same**: equal on both sides, values included.
//! - **removed**: listed by this provider's record, gone from the repo and
//!   present live (D-02). `already_absent` when it is gone live too. A
//!   removed node with live nodes below it is removed only when every one of
//!   them is removed too; otherwise it stays, as drift.
//! - **drift**: a live node the repo lacks that is not removed. Kept and
//!   reported, never deleted (D-01).
//!
//! With no record (the first deploy of the provider by this tool, the
//! adoption of cut-over step 5) nothing is ever removed: any added or changed
//! node is a `first_change`, and a provider whose nodes all compare equal is
//! `record_only`. A plan whose removals cover every node the record lists is
//! `provider_emptied` (Pitfall 2).
//!
//! Pure. [`TagPlan::plan_sha256`] hashes the whole plan.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::Value;

use super::native_tags::{NativeProvider, NonCanonical};
use super::projects::ProviderSettings;
use super::tag_canon::{CanonOpts, canonical, diff_properties, in_types, strip_runtime_values};
use super::tag_record::TagRecord;
use super::tree::sha256_hex;
use crate::client::tags::TagProviderRecord;

/// One changed node: its provider-relative path and the names of the
/// properties that differ. Never a value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NodeChange {
    /// Provider-relative path.
    pub path: String,
    /// The differing property names, byte-sorted.
    pub properties: Vec<String>,
    /// The node's `tagType` differs (a Folder becoming a UdtInstance and the
    /// like): two rehearsals needed a gateway restart before such a node read
    /// well (N1).
    pub restart_likely: bool,
}

/// The plan of one owned provider. Paths are provider-relative and
/// byte-sorted.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ProviderPlan {
    /// The provider's name.
    pub provider: String,
    /// Repo nodes the gateway lacks.
    pub added: Vec<String>,
    /// The added nodes this provider's record lists: placed by an earlier
    /// deploy and gone live since.
    pub recreated: Vec<String>,
    /// Nodes whose configuration differs.
    pub changed: Vec<NodeChange>,
    /// Nodes whose configuration is equal and whose runtime value differs:
    /// the live value is kept (D-03).
    pub kept_values: Vec<String>,
    /// Recorded nodes the repo no longer holds that are live (D-02).
    pub removed: Vec<String>,
    /// Recorded nodes the repo no longer holds that are gone live already.
    pub already_absent: Vec<String>,
    /// Live nodes the repo lacks that are not removed (D-01).
    pub drift: Vec<String>,
    /// Nodes equal on both sides, values included.
    pub same: usize,
    /// No record, and the plan adds or changes a node: the first deploy would
    /// change what the gateway runs.
    pub first_change: bool,
    /// No record, and nothing is added or changed: the run only writes the
    /// record.
    pub record_only: bool,
    /// The removals cover every node the record lists.
    pub provider_emptied: bool,
    /// The run id of the record the plan was drawn against.
    pub record_run_id: Option<String>,
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
    /// sha256 over the whole plan: every provider's lists, counts, flags and
    /// record run id, `tags_sha256` and `live_sha256`.
    pub plan_sha256: String,
}

impl TagPlan {
    /// Whether a run has anything to do: a node to add, change or remove,
    /// or a provider whose run only writes the tag record (`record_only`,
    /// the adoption of cut-over step 5).
    pub fn would_change(&self) -> bool {
        self.providers.iter().any(|p| {
            !p.added.is_empty() || !p.changed.is_empty() || !p.removed.is_empty() || p.record_only
        })
    }
}

/// `v`'s `tagType`, when it is a string.
fn tag_type(v: &Value) -> Option<&str> {
    v.get("tagType").and_then(Value::as_str)
}

/// Whether the repo node and the live node at `path`, equal in configuration,
/// differ in a runtime value: an AtomicTag `value`, top level or member.
/// `timestamp` and `quality` alone never count.
fn value_differs(repo: &Value, live: &Value, path: &str) -> bool {
    diff_properties(
        repo,
        live,
        path,
        CanonOpts {
            strip_values: false,
            created: false,
            exports: false,
        },
    )
    .iter()
    .any(|p| p == "value" || p.ends_with(".value"))
}

/// Plan the owned provider `repo` against its live nodes `live` and this
/// project's previous record of it, `record` (`None`: no `ign` deploy of the
/// provider yet). See the module documentation for the classes.
pub fn plan_provider(
    repo: &NativeProvider,
    live: &BTreeMap<String, Value>,
    record: Option<&TagRecord>,
) -> ProviderPlan {
    let mut plan = ProviderPlan {
        provider: repo.name.clone(),
        record_run_id: record.map(|r| r.run_id.clone()),
        ..ProviderPlan::default()
    };
    let recorded: BTreeSet<&str> = record
        .map(|r| r.nodes.iter().map(String::as_str).collect())
        .unwrap_or_default();
    for (path, node) in &repo.nodes {
        let Some(live_node) = live.get(path) else {
            plan.added.push(path.clone());
            if recorded.contains(path.as_str()) {
                plan.recreated.push(path.clone());
            }
            continue;
        };
        let opts = CanonOpts {
            // Below `_types_/` a member's value is the type's default:
            // configuration, never stripped.
            strip_values: !in_types(path),
            created: false,
            exports: false,
        };
        let properties = diff_properties(&node.json, live_node, path, opts);
        if !properties.is_empty() {
            plan.changed.push(NodeChange {
                path: path.clone(),
                properties,
                restart_likely: tag_type(&node.json) != tag_type(live_node),
            });
        } else if value_differs(&node.json, live_node, path) {
            plan.kept_values.push(path.clone());
        } else {
            plan.same += 1;
        }
    }
    // D-02: only what this provider's record lists and the repo dropped.
    let mut removable: BTreeSet<&str> = BTreeSet::new();
    for path in &recorded {
        if repo.nodes.contains_key(*path) {
            continue;
        }
        if live.contains_key(*path) {
            removable.insert(path);
        } else {
            plan.already_absent.push((*path).to_string());
        }
    }
    // A node with a live node below it that stays (drift, or anything not
    // removable) stays too. Deepest first, so a child is decided before its
    // parent looks at it.
    let mut deepest_first: Vec<&str> = removable.iter().copied().collect();
    deepest_first.sort_by_key(|p| std::cmp::Reverse(p.matches('/').count()));
    for path in deepest_first {
        let prefix = format!("{path}/");
        let held = live
            .keys()
            .any(|k| k.starts_with(&prefix) && !removable.contains(k.as_str()));
        if held {
            removable.remove(path);
        }
    }
    plan.removed = removable.iter().map(|p| (*p).to_string()).collect();
    plan.drift = live
        .keys()
        .filter(|p| !repo.nodes.contains_key(*p) && !removable.contains(p.as_str()))
        .cloned()
        .collect();
    match record {
        None => {
            plan.first_change = !plan.added.is_empty() || !plan.changed.is_empty();
            plan.record_only = !plan.first_change;
        }
        Some(record) => {
            plan.provider_emptied = !plan.removed.is_empty()
                && record
                    .nodes
                    .iter()
                    .all(|p| removable.contains(p.as_str()) || plan.already_absent.contains(p));
        }
    }
    plan
}

/// Plan `repo` (the owned providers) against `live` (each provider's flattened
/// export, keyed by the provider's name) and `records` (this project's tag
/// record of each provider, keyed by the provider's name; a provider without
/// one is a first deploy). A provider `live` lacks is planned against an
/// empty export. Records are per provider: one provider's record never
/// removes a node of another.
pub fn plan_tags(
    repo: &[NativeProvider],
    live: &BTreeMap<String, BTreeMap<String, Value>>,
    records: &BTreeMap<String, TagRecord>,
) -> TagPlan {
    let empty = BTreeMap::new();
    let mut owned: Vec<&NativeProvider> = repo.iter().collect();
    owned.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()));
    let mut providers = Vec::new();
    let mut live_digest = Vec::new();
    for provider in owned {
        let live_nodes = live.get(&provider.name).unwrap_or(&empty);
        providers.push(plan_provider(
            provider,
            live_nodes,
            records.get(&provider.name),
        ));
        for (path, live_node) in live_nodes {
            let mut stripped = live_node.clone();
            strip_runtime_values(&mut stripped, path);
            live_digest.push(Value::Array(vec![
                Value::String(provider.name.clone()),
                Value::String(path.clone()),
                canonical(&stripped),
            ]));
        }
    }
    let tags_sha256 = repo_tags_sha256(repo);
    let live_sha256 = digest(live_digest);
    let plan_sha256 = sha256_hex(
        &serde_json::to_vec(&serde_json::json!({
            "providers": providers,
            "tags_sha256": tags_sha256,
            "live_sha256": live_sha256,
        }))
        .unwrap_or_default(),
    );
    TagPlan {
        providers,
        tags_sha256,
        live_sha256,
        plan_sha256,
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
    /// How many added nodes the record lists (placed before, gone live).
    pub recreated: usize,
    /// How many nodes differ only in a runtime value (kept, D-03).
    pub kept_values: usize,
    /// How many recorded nodes the repo dropped that are live.
    pub removed: usize,
    /// How many recorded nodes the repo dropped that are gone live already.
    pub already_absent: usize,
    /// How many of the provider's repo files the git module would rewrite.
    pub non_canonical: usize,
    /// No record of this provider, and the plan adds or changes a node.
    pub first_change: bool,
    /// No record of this provider, and nothing is added or changed.
    pub record_only: bool,
    /// The run id of the record the plan was drawn against; null without one.
    pub record_run_id: Option<String>,
}

/// One changed node in `data.tags.changed`: the full tag path
/// (`[provider]path`) and the property names that differ.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChangedTag {
    /// `[provider]path`.
    pub path: String,
    /// The differing property names.
    pub properties: Vec<String>,
    /// The node's `tagType` differs (N1).
    pub restart_likely: bool,
}

/// `data.tags` of `ign deploy projects`: null for a project that owns no tag
/// provider. Paths are full tag paths (`[provider]path`), byte-sorted and
/// capped with the full count beside each list. Names, counts and hashes
/// only, never a value. Keys are only ever added.
#[derive(Debug, Clone, PartialEq, Serialize)]
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
    /// Warnings about the owned providers (26-05, 26-07): for example that
    /// no gateway-config manifest was found to check ownership against, or a
    /// credential-like value the run does not send. Texts name paths and
    /// providers only.
    pub warnings: Vec<String>,
    /// [`TagPlan::plan_sha256`] (26-07).
    pub plan_sha256: String,
    /// Added nodes the record lists: placed by an earlier deploy, gone live.
    pub recreated: Vec<String>,
    /// Full count of `recreated`.
    pub recreated_count: usize,
    /// Nodes whose runtime value differs: the live value is kept (D-03).
    pub kept_values: Vec<String>,
    /// Full count of `kept_values`.
    pub kept_values_count: usize,
    /// Recorded nodes the repo dropped that are live (D-02).
    pub removed: Vec<String>,
    /// Full count of `removed`.
    pub removed_count: usize,
    /// Recorded nodes the repo dropped that are gone live already.
    pub already_absent: Vec<String>,
    /// Full count of `already_absent`.
    pub already_absent_count: usize,
    /// Repo files the git module would rewrite, one entry per reason
    /// (TAGDEP-12).
    pub non_canonical: Vec<NonCanonical>,
    /// Full count of `non_canonical`.
    pub non_canonical_count: usize,
    /// Changed nodes the run sent with `Overwrite` and their live values
    /// carried from a fresh export (R3-CARRY, R3-RACE): a runtime write in
    /// that window is overwritten. Empty in `--check` (26-08).
    pub carried: Vec<String>,
    /// Full count of `carried`.
    pub carried_count: usize,
    /// The run's tag verify (26-08); null in `--check` and before the tag
    /// step ran.
    pub verify: Option<TagVerify>,
    /// Each owned provider's settings against the live provider (26-10,
    /// D-09, D-12), byte-sorted by provider. Applied first in a run.
    pub provider_settings: Vec<ProviderSettingsPlan>,
    /// Each owned provider's tag groups against its live units (26-10), for
    /// the providers whose repo holds `.tag-groups.json`, byte-sorted by
    /// provider. Written after the provider settings, before the tags.
    pub groups: Vec<super::tag_groups::GroupPlan>,
    /// The automatic rollback of a run that failed after its first tag write
    /// (26-11, D-07); null in `--check` and for a run that needed none. A
    /// `--check --rollback-run` (26-12) reports here what the undo would do.
    pub rollback: Option<TagRollback>,
    /// The host journal of an earlier run of this project on this gateway
    /// (26-12), null when there is none. A `--check` reports it; an apply
    /// refuses `deploy_journal_present`.
    pub journal: Option<TagJournal>,
    /// The backup's copy of the owned tags on a redundant pair (26-13): the
    /// wait after a green run or a verified rollback. Null in `--check`,
    /// without a backup node and when the run ended before either.
    pub backup: Option<super::tag_converge::TagsBackupReport>,
}

/// `data.tags.journal` (26-12): the host journal an earlier tag-owning run of
/// the project left, as the plan found it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TagJournal {
    /// The run that left it.
    pub run_id: String,
    /// `applying` or `rolling_back`.
    pub state: String,
    /// When that run (or its rollback) started, UTC.
    pub started_at: String,
}

/// `data.tags.rollback` (26-11): what the automatic rollback of a run did.
/// Paths, provider names and counts only, never a value. Present only on the
/// verdict outcomes `tags_rolled_back` and `rolled_back`; a rollback that
/// cannot finish is `rollback_failed` and prints no `data`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TagRollback {
    /// Why the rollback ran: `tag_step_failed`, `tag_verify_failed`,
    /// `project_import_failed` or `project_unverified`; `rollback_run` for
    /// `--rollback-run` (26-12).
    pub cause: String,
    /// What failed, as the run saw it: paths, property names and error
    /// slugs only.
    pub detail: String,
    /// Full tag paths the rollback sent back from their pre-image, byte-sorted
    /// and capped.
    pub reimported: Vec<String>,
    /// Full count of `reimported`.
    pub reimported_count: usize,
    /// The part of `reimported` sent `Overwrite` with live values carried
    /// from an export read right before the batch (R3-RB, R3-CARRY).
    pub carried: Vec<String>,
    /// Full count of `carried`.
    pub carried_count: usize,
    /// Nodes the run created that no delete can remove (R3-DEL). Always
    /// empty here: a rollback holding any ends `rollback_failed`.
    pub undeletable: Vec<String>,
    /// Full count of `undeletable`.
    pub undeletable_count: usize,
    /// Whether the run's tag-group units were put back from its snapshot.
    pub groups_restored: bool,
    /// Owned providers whose settings were put back from
    /// `export/tags/<P>.provider.json`, byte-sorted.
    pub provider_restored: Vec<String>,
    /// Owned providers the run created and the rollback deleted (with the
    /// tags the run put in them), byte-sorted.
    pub provider_deleted: Vec<String>,
    /// Whether the project was imported again from `export/<project>.zip`.
    pub project_reimported: bool,
    /// The read-back of the owned providers against their pre-images, export
    /// against export (R3-VK, R3-KEY, `defaultValue` as configuration).
    pub verify: Option<TagVerify>,
}

/// What a run does with one owned provider's settings (26-10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderSettingsAction {
    /// The live provider holds the repo's settings: nothing is written.
    Same,
    /// The gateway lacks the provider: it is created from the settings.
    Create,
    /// The live settings differ: the provider is modified with the
    /// signature of a fresh read.
    Modify,
}

impl ProviderSettingsAction {
    /// The word `data.tags.provider_settings[].action` carries.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Same => "same",
            Self::Create => "create",
            Self::Modify => "modify",
        }
    }
}

/// One entry of `data.tags.provider_settings` (26-10): the provider, what the
/// run does and, for `modify`, the differing property names (`enabled`,
/// `description`, `config.<path>`), byte-sorted. Names only, never a value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProviderSettingsPlan {
    /// The provider.
    pub provider: String,
    /// What the run does.
    pub action: ProviderSettingsAction,
    /// The differing property names; empty unless `modify`.
    pub properties: Vec<String>,
}

/// Plan `provider`'s repo `settings` against its live record, `None` when the
/// gateway has no such provider (26-10). `enabled` and `description` compare
/// as values (an absent or null description is none). `config` compares
/// whole, key order aside: the repo holds the configuration as the gateway's
/// resource holds it (26-05), so a key the gateway fills that the repo lacks
/// differs, as does one the gateway drops (C11 dropped an unknown key).
pub fn plan_provider_settings(
    provider: &str,
    settings: &ProviderSettings,
    live: Option<&TagProviderRecord>,
) -> ProviderSettingsPlan {
    let Some(live) = live else {
        return ProviderSettingsPlan {
            provider: provider.to_string(),
            action: ProviderSettingsAction::Create,
            properties: Vec::new(),
        };
    };
    let mut properties = Vec::new();
    if settings.enabled != live.enabled {
        properties.push("enabled".to_string());
    }
    if settings.description.as_deref() != live.description() {
        properties.push("description".to_string());
    }
    differing_paths("config", &settings.config, &live.config, &mut properties);
    properties.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    ProviderSettingsPlan {
        provider: provider.to_string(),
        action: if properties.is_empty() {
            ProviderSettingsAction::Same
        } else {
            ProviderSettingsAction::Modify
        },
        properties,
    }
}

/// The dotted paths at which `a` and `b` differ: objects are walked key by
/// key, anything else compares whole; a key on one side only differs.
fn differing_paths(prefix: &str, a: &Value, b: &Value, out: &mut Vec<String>) {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let keys: BTreeSet<&String> = x.keys().chain(y.keys()).collect();
            for key in keys {
                let path = format!("{prefix}.{key}");
                match (x.get(key), y.get(key)) {
                    (Some(u), Some(v)) => differing_paths(&path, u, v, out),
                    _ => out.push(path),
                }
            }
        }
        _ if a == b => {}
        _ => out.push(prefix.to_string()),
    }
}

/// `data.tags.verify`: the run's read-back of every owned provider after the
/// tag writes (R3-VK, R3-KEY, R3-DV).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TagVerify {
    /// Every repo node equals the export in configuration and every drift
    /// node equals its pre-image.
    pub equal: bool,
    /// Two consecutive reads agreed before the settle bound passed.
    pub settled: bool,
    /// How many times every owned provider was exported.
    pub reads: usize,
    /// Seconds from the first read to the last.
    pub elapsed_s: f64,
    /// The nodes that differ, `[provider]path (properties)`, byte-sorted and
    /// capped. Property names only, never a value.
    pub mismatches: Vec<String>,
    /// Full count of `mismatches`.
    pub mismatches_count: usize,
}

/// A list of full tag paths: byte-sorted, then capped at `cap`, with the
/// full count.
fn full_list(mut items: Vec<String>, cap: usize) -> (Vec<String>, usize) {
    items.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    let count = items.len();
    items.truncate(cap);
    (items, count)
}

impl TagsReport {
    /// The report of `plan`, each list capped at `cap` entries.
    /// `non_canonical` holds each owned provider's form findings, keyed by
    /// the provider's name ([`super::native_tags::check_canonical_form`]).
    pub fn from_plan(
        plan: &TagPlan,
        non_canonical: &BTreeMap<String, Vec<NonCanonical>>,
        cap: usize,
    ) -> TagsReport {
        let full = |provider: &str, path: &str| format!("[{provider}]{path}");
        let paths = |pick: fn(&ProviderPlan) -> &Vec<String>| {
            plan.providers
                .iter()
                .flat_map(|p| pick(p).iter().map(|path| full(&p.provider, path)))
                .collect::<Vec<String>>()
        };
        let mut changed: Vec<ChangedTag> = plan
            .providers
            .iter()
            .flat_map(|p| {
                p.changed.iter().map(|c| ChangedTag {
                    path: full(&p.provider, &c.path),
                    properties: c.properties.clone(),
                    restart_likely: c.restart_likely,
                })
            })
            .collect();
        changed.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
        let changed_count = changed.len();
        changed.truncate(cap);
        let (added, added_count) = full_list(paths(|p| &p.added), cap);
        let (drift, drift_count) = full_list(paths(|p| &p.drift), cap);
        let (recreated, recreated_count) = full_list(paths(|p| &p.recreated), cap);
        let (kept_values, kept_values_count) = full_list(paths(|p| &p.kept_values), cap);
        let (removed, removed_count) = full_list(paths(|p| &p.removed), cap);
        let (already_absent, already_absent_count) = full_list(paths(|p| &p.already_absent), cap);
        let mut forms: Vec<NonCanonical> = plan
            .providers
            .iter()
            .flat_map(|p| {
                non_canonical
                    .get(&p.provider)
                    .into_iter()
                    .flatten()
                    .cloned()
            })
            .collect();
        forms.sort_by(|a, b| a.file.as_bytes().cmp(b.file.as_bytes()));
        let non_canonical_count = forms.len();
        forms.truncate(cap);
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
                    recreated: p.recreated.len(),
                    kept_values: p.kept_values.len(),
                    removed: p.removed.len(),
                    already_absent: p.already_absent.len(),
                    non_canonical: non_canonical.get(&p.provider).map_or(0, Vec::len),
                    first_change: p.first_change,
                    record_only: p.record_only,
                    record_run_id: p.record_run_id.clone(),
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
            plan_sha256: plan.plan_sha256.clone(),
            recreated,
            recreated_count,
            kept_values,
            kept_values_count,
            removed,
            removed_count,
            already_absent,
            already_absent_count,
            non_canonical: forms,
            non_canonical_count,
            carried: Vec::new(),
            carried_count: 0,
            verify: None,
            provider_settings: Vec::new(),
            groups: Vec::new(),
            rollback: None,
            journal: None,
            backup: None,
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

        let report = TagsReport::from_plan(&plan, &BTreeMap::new(), 200);
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
                (
                    "Dv",
                    json!({"tagType":"AtomicTag","value":3,"defaultValue":3}),
                ),
            ],
        );
        let live = nodes(&[
            (
                "C12",
                json!({"tagType":"AtomicTag","value":9,"defaultValue":3}),
            ),
            (
                "Dv",
                json!({"tagType":"AtomicTag","value":3,"defaultValue":4}),
            ),
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
        let later = plan_provider(
            &provider("P", &[("A", tag(1)), ("N", tag(1))]),
            &live,
            Some(&rec),
        );
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
        let records =
            BTreeMap::from([("B".to_string(), record("B", &["Line1", "Line1/Extra"], "r"))]);
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
        let records =
            |run: &str| BTreeMap::from([("P".to_string(), record("P", &["A", "B"], run))]);
        let sha =
            |repo: &NativeProvider, live: &BTreeMap<String, BTreeMap<String, Value>>, run: &str| {
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
                (
                    "C",
                    json!({"tagType":"AtomicTag","valueSource":"memory","value":1,"tooltip":"t"}),
                ),
            ],
        );
        let p3 = plan_tags(std::slice::from_ref(&repo), &config, &records("r1"));
        assert_eq!(p1.providers, p3.providers, "the lists are equal");
        assert_ne!(p1.live_sha256, p3.live_sha256);
        assert_ne!(p1.plan_sha256, p3.plan_sha256, "live_sha256");
    }
}
