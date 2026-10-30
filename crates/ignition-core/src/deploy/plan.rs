//! The plan engine (Phase 22): the rendered tree against what is live.
//!
//! Pure: it takes a [`RenderedTree`] and an [`ObservedFiles`] and returns a
//! [`Plan`]. No I/O, no clock, no network; the transport reads, this
//! compares.
//!
//! ## Units
//!
//! The plan reasons about **units**, not files. A file's unit is the key of
//! its *unit directory*: the nearest ancestor directory (the file's own
//! directory included) that holds `resource.json` or `unary-resource.json`
//! in the same layer, else the file's parent directory ([`unit_key`]).
//! Resources therefore group per `<module>/<type>/<name>`, singletons per
//! `<module>/<type>`, and tag files per folder
//! (`ignition/tag-definition/<P>/<Folder>`), never one entry per provider.
//!
//! ## Basis (D-17)
//!
//! A rendered unit is compared with its **effective live copy**: the copy in
//! the mode collection when the unit exists there, else the copy in `core`
//! (the gateway overrides whole resources, and tag folders, by name). Each
//! entry's `live_in` names where live copies sit, the ordered subset of
//! `["core", "mode"]`, so a `same` unit that is not yet in the mode
//! collection is still visible.
//!
//! | status | meaning |
//! |---|---|
//! | `added` | rendered, no effective live copy |
//! | `changed` | rendered, effective copy differs in file set or any byte |
//! | `same` | rendered, effective copy has the same files, byte-equal |
//! | `removed` | not rendered, and the declared-names ledger lists a file of the unit that still exists in the mode collection (the delete set) |
//! | `unmanaged` | live, of an allowlisted type or declared provider, not rendered; never deleted, never counted in `has_changes` (D-06) |
//!
//! A unit with a live file the host cannot serve
//! ([`ObservedFiles::unreadable`]: on a Windows sftp host, a host path of
//! `LONG_PATH_LIMIT`, 240, characters or more) is no entry at all. It is
//! listed in [`Plan::unreadable`] ([`UnreadableUnit`]) and counted in
//! `summary.unreadable`; it is wholly unreadable (both collections, readable
//! siblings included), never treated as absent and never in the delete set.
//! `has_changes` is true when such a unit is declared or in the ledger; one
//! that is neither is left alone like `unmanaged` and raises a warning. Linux
//! and local hosts never report one.
//!
//! `core` relates the `core` collection's copy of the unit to the rendered
//! unit: `none` (no core copy), `identical` (same files, byte-equal),
//! `differs`, or `hidden` (a tag-definition or tag-type-definition folder
//! defined in the mode collection whose core copy holds files the rendered
//! unit lacks: the gateway overrides these by folder, so those core files
//! are invisible), or `shadows_removal` (a `removed` unit that also has a
//! `core` copy: deleting from the mode collection will not take effect, the
//! gateway keeps running the `core` copy).
//!
//! Comparison is byte-exact per file: a whitespace-only or key-order change
//! is `changed`.
//!
//! ## The delete set and `removed` (D-05, 22-03)
//!
//! The delete set is exactly the declared-names ledger's files minus the
//! rendered files, restricted to files that exist in the mode collection. No
//! ledger (a first deploy) gives an empty delete set; a ledger that exists
//! but is unusable is refused before this function runs (the caller parses
//! it with [`Ledger::parse`]). A ledger path is never in the delete set when
//! it is:
//!
//! - rendered (byte-equal path), or equal to a rendered path under ASCII
//!   case folding (a case-insensitive host would delete the rendered file;
//!   this one is reported in `warnings`),
//! - under an excluded tag provider ([`super::exclude`]): one warning per
//!   provider, with the file count, even for a ledger written before the
//!   provider was excluded and even when the scope names the provider,
//! - under a type or tag provider the manifest no longer allows (reported in
//!   `warnings`, never deleted),
//! - of an unreadable unit (marks the unit `in_ledger`; never read as
//!   absent), or
//! - `config-mode.json` or under a dot-named component.
//!
//! Paths compare as exact UTF-8 bytes. A rendered unit that loses only some
//! of its ledger files is `changed` and deletes just those files; a unit that
//! is not rendered and has delete-set files is `removed`; `unmanaged` is a
//! live unit with no delete-set file. Plan never writes or rewrites the
//! ledger.
//!
//! ## Flags (22-03)
//!
//! Derived only from status, delete-set membership and type, so the same
//! input gives the same flags. `destructive` is true for `removed` entries,
//! for any entry with delete-set files, and for an `added` or `changed`
//! tag-definition or tag-type-definition unit that drops tag data the gateway
//! runs now ([`tag_data_dropped`], 261030-8m8 CR-01): its effective live copy
//! holds a `tags.json` or `udts.json` (any letter case) the rendered unit
//! lacks, or its `core` relation is `hidden`. No file is deleted there, but
//! the gateway loads only the member files a marker lists (CD5c) and a
//! mode-collection folder hides its `core` copy whole (CD5a S4, CD5-CTL), so
//! those tags or UDT types leave the running gateway. Each such entry also
//! raises one plan warning naming the unit and the dropped paths (never
//! content). A `same` unit is never flagged this way: the apply does not touch
//! it. `destructive_reason` is then
//! `tag_data_loss` (`ignition/tag-provider`, `tag-group`,
//! `tag-type-definition`, `tag-definition`), `connection_removed`
//! (`ignition/database-connection`, `opc-connection`, `identity-provider`),
//! `storage_removed` (`ignition/store-and-forward-engine`, `alarm-journal`),
//! else `resource_removed` for `removed` and `files_removed` for `changed`.
//! `lockout_risk` is true for every entry that is not `same` and not
//! `unmanaged` (one the plan would touch) of `ignition/identity-provider` or
//! `ignition/security-zone`.
//!
//! ## `plan_sha256` (D-08)
//!
//! The lowercase hex SHA-256 of this UTF-8 text, byte for byte. `LF` is
//! `0x0A`, `NUL` is `0x00`:
//!
//! ```text
//! ign-deploy:plan:v1 LF
//! render_sha256 NUL <render_sha256 hex> LF
//! E NUL <key> NUL <status> NUL <core> NUL <live_in joined by ","> LF
//! F NUL <collection path> NUL <live sha256 or -> NUL <desired sha256 or -> LF
//! D NUL <collection path> LF
//! U NUL <key> NUL <live_in joined by ","> NUL <declared 0|1> NUL <in_ledger 0|1> LF
//! ```
//!
//! One `E` line per entry that is not `unmanaged`, in unit-key byte order,
//! each followed by its `F` lines in path byte order; then one `D` line per
//! delete-set path in byte order (none without a ledger); then one `U` line
//! per unreadable unit in key byte order. `U` lines appear only when a unit
//! is unreadable, so the header stays `v1` and every earlier hash is
//! unchanged. `status` and `core` are
//! the serialized words (`added`, `identical`, ...). Drift, diff text and
//! warnings are excluded on purpose, as are `unmanaged` entries, so a review
//! hash is reproducible with or without `--base` and unrelated live
//! resources never invalidate it. Changing this text means re-goldening
//! (`PLAN_GOLDEN` in `tests/deploy_plan.rs`) and Phase 23's recomputation.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

use serde::Serialize;
use sha2::{Digest, Sha256};

use similar::TextDiff;

use super::exclude;
use super::layout::{RelPath, TAG_TYPES, resource_key};
use super::ledger::Ledger;
use super::lint::{scan_embedded, scan_secrets};
use super::node::{Node, classify};
use super::rules::GATEWAY_OWNED_FILES;
use super::transport::{
    DeleteSet, FileBlob, ObservedFiles, READ_MAX_ENTRIES, READ_MAX_FILE_BYTES,
    READ_MAX_TOTAL_BYTES, Scope,
};
use super::tree::{
    MarkerFile, OpenError, RENDER_MARKER, RenderMarker, RenderedTree, TMP_SUFFIX,
    open_regular_file, sha256_hex, walk_capped,
};
use crate::error::CoreError;

/// First line of the canonical `plan_sha256` text.
pub const PLAN_HASH_HEADER: &str = "ign-deploy:plan:v1";

/// The plan status of one unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PlanStatus {
    /// Rendered, no effective live copy.
    Added,
    /// Rendered, effective copy differs.
    Changed,
    /// Not rendered, with delete-set files in the mode collection.
    Removed,
    /// Rendered, effective copy byte-equal.
    Same,
    /// Live, allowlisted, not rendered; never deleted (D-06).
    Unmanaged,
}

impl PlanStatus {
    /// The status word as serialized and printed.
    pub fn word(self) -> &'static str {
        match self {
            PlanStatus::Added => "added",
            PlanStatus::Changed => "changed",
            PlanStatus::Removed => "removed",
            PlanStatus::Same => "same",
            PlanStatus::Unmanaged => "unmanaged",
        }
    }
}

/// How the `core` copy of a unit relates to the rendered unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreRelation {
    /// The unit has no `core` copy.
    None,
    /// The `core` copy has the same files, byte-equal to the render.
    Identical,
    /// The `core` copy exists and differs.
    Differs,
    /// A tag folder defined in the mode collection hides `core`-only files.
    Hidden,
    /// Status `removed` while a `core` copy exists: deleting from the mode
    /// collection will not take effect.
    ShadowsRemoval,
}

impl CoreRelation {
    /// The relation word as serialized and printed.
    pub fn word(self) -> &'static str {
        match self {
            CoreRelation::None => "none",
            CoreRelation::Identical => "identical",
            CoreRelation::Differs => "differs",
            CoreRelation::Hidden => "hidden",
            CoreRelation::ShadowsRemoval => "shadows_removal",
        }
    }
}

/// One file of a unit: its live and rendered digests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanFile {
    /// Collection-relative path.
    pub path: String,
    /// SHA-256 of the effective live copy; null when the file is not live.
    pub live_sha256: Option<String>,
    /// SHA-256 of the rendered file; null when the file is not rendered.
    pub desired_sha256: Option<String>,
}

/// One unit of the plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanEntry {
    /// The unit key (the unit directory, collection-relative).
    pub key: String,
    /// Module id, e.g. `ignition`.
    pub module: String,
    /// Resource type, e.g. `database-translator`.
    #[serde(rename = "type")]
    pub ty: String,
    /// Resource name (or tag provider and folder path); null for a singleton.
    pub name: Option<String>,
    /// The unit's status.
    pub status: PlanStatus,
    /// Where live copies sit: the ordered subset of `["core", "mode"]`.
    pub live_in: Vec<String>,
    /// The `core` copy's relation to the rendered unit.
    pub core: CoreRelation,
    /// True for `removed` entries, entries with delete-set files, and
    /// `added` or `changed` tag units that drop live tag data (module doc).
    pub destructive: bool,
    /// Why the entry is destructive (module doc); null when it is not.
    pub destructive_reason: Option<String>,
    /// True when a unit of a security type that the plan touches could lock
    /// the operator out of the gateway.
    pub lockout_risk: bool,
    /// Three-way drift against a `--base` render: true when any file's live
    /// sha256 differs from both the base and the desired sha256. Null
    /// without `--base`. Excluded from `plan_sha256`.
    pub drift: Option<bool>,
    /// The unit's files, byte-sorted.
    pub files: Vec<PlanFile>,
    /// Redacted unified text diff of the unit's differing files (`--diff`);
    /// null without it, and for `same` and `unmanaged` entries. Excluded from
    /// `plan_sha256`.
    pub diff: Option<String>,
}

/// Counts by status.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PlanSummary {
    /// Units that would be added.
    pub added: usize,
    /// Units that would change.
    pub changed: usize,
    /// Units that would be removed.
    pub removed: usize,
    /// Units already equal.
    pub same: usize,
    /// Live units the declared tree does not manage.
    pub unmanaged: usize,
    /// Units the host holds but could not serve whole
    /// ([`Plan::unreadable`]); see `has_changes`.
    pub unreadable: usize,
    /// True when anything would be added, changed or removed, or when an
    /// unreadable unit is declared or listed in the ledger (the deploy must
    /// act on a unit it cannot see). `unmanaged` never counts (D-06), and
    /// neither does an unreadable unit that is neither declared nor in the
    /// ledger: like `unmanaged`, it is left alone, so a pull followed by
    /// render and plan still reports no change. Its `U` line still keeps
    /// `plan_sha256` distinct from a plan without it.
    pub has_changes: bool,
    /// Entries with `drift: true`; null without `--base`.
    pub drifted: Option<usize>,
}

/// The plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Plan {
    /// Units in key byte order.
    pub entries: Vec<PlanEntry>,
    /// Counts by status.
    pub summary: PlanSummary,
    /// The files plan proposes to delete (module doc).
    pub delete_set: DeleteSet,
    /// Lowercase hex SHA-256 of the canonical plan text (module doc).
    pub plan_sha256: String,
    /// Plan-level warnings.
    pub warnings: Vec<String>,
    /// Units the host holds but could not serve whole, in key byte order
    /// (module doc). Never an entry, never in the delete set.
    pub unreadable: Vec<UnreadableUnit>,
}

/// A unit with at least one live file the host could not serve
/// ([`ObservedFiles::unreadable`]): on a Windows sftp host, a host path of
/// `LONG_PATH_LIMIT` (240) characters or more. The whole unit is
/// unreadable, in both collections: it is never planned from its readable
/// files, never treated as absent and never deleted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnreadableUnit {
    /// The unit key, as an entry would carry it.
    pub key: String,
    /// Where the unit has live files, readable or not: the ordered subset of
    /// `["core", "mode"]`.
    pub live_in: Vec<String>,
    /// The declared tree renders this unit.
    pub declared: bool,
    /// The declared-names ledger lists a file of this unit.
    pub in_ledger: bool,
    /// The collection-relative paths that could not be read, byte-sorted.
    pub paths: Vec<String>,
    /// The longest host path length among `paths`, in characters.
    pub longest: usize,
}

/// What [`unreadable_unit_keys`] knows about one unreadable unit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnreadableKey {
    /// `core` and/or `mode`: the collections holding an unreadable file.
    pub collections: BTreeSet<String>,
    /// The unreadable collection-relative paths, byte-sorted and unique.
    pub paths: Vec<String>,
    /// The longest host path length among `paths`.
    pub longest: usize,
}

/// The layer the unit keys of `collection` (`core` or `mode`) are computed
/// over: the collection's observed paths plus its unreadable paths, so a
/// `resource.json` marker groups readable and unreadable files alike. With
/// no unreadable path it holds exactly the observed paths, so every key is
/// the one [`unit_key`] gives over the observed layer.
pub fn unit_key_layer(observed: &ObservedFiles, collection: &str) -> BTreeMap<RelPath, ()> {
    let files = if collection == "core" {
        &observed.core
    } else {
        &observed.mode
    };
    files
        .keys()
        .cloned()
        .chain(
            observed
                .unreadable
                .iter()
                .filter(|u| u.collection == collection)
                .map(|u| u.path.clone()),
        )
        .map(|p| (p, ()))
        .collect()
}

/// The unreadable units of `observed`, by unit key ([`unit_key_layer`]).
/// The one definition pull, plan and verify share: a unit with an
/// unreadable file in either collection is wholly unreadable.
pub fn unreadable_unit_keys(observed: &ObservedFiles) -> BTreeMap<String, UnreadableKey> {
    let mut out: BTreeMap<String, UnreadableKey> = BTreeMap::new();
    if observed.unreadable.is_empty() {
        return out;
    }
    let core_layer = unit_key_layer(observed, "core");
    let mode_layer = unit_key_layer(observed, "mode");
    for u in &observed.unreadable {
        let layer = match u.collection.as_str() {
            "core" => &core_layer,
            "mode" => &mode_layer,
            _ => continue,
        };
        let info = out.entry(unit_key(&u.path, layer)).or_default();
        info.collections.insert(u.collection.clone());
        info.paths.push(u.path.as_str().to_string());
        info.longest = info.longest.max(u.length);
    }
    for info in out.values_mut() {
        info.paths.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        info.paths.dedup();
    }
    out
}

/// The unit key of `path` within `layer`: the nearest ancestor directory
/// (the file's own directory included) holding `resource.json` or
/// `unary-resource.json` in `layer`, else the file's parent directory.
/// Never climbs above `<module>/<type>`.
pub fn unit_key<V>(path: &RelPath, layer: &BTreeMap<RelPath, V>) -> String {
    let parts: Vec<&str> = path.components().collect();
    if parts.len() < 2 {
        return path.as_str().to_string();
    }
    let parent_len = parts.len() - 1;
    for end in (2..=parent_len).rev() {
        let dir = parts[..end].join("/");
        for marker in ["resource.json", "unary-resource.json"] {
            if let Ok(candidate) = RelPath::new(&format!("{dir}/{marker}"))
                && layer.contains_key(&candidate)
            {
                return dir;
            }
        }
    }
    parts[..parent_len].join("/")
}

/// path -> sha256, grouped by unit key.
type Units = BTreeMap<String, BTreeMap<String, String>>;

fn group<'a, V: 'a>(layer: &'a BTreeMap<RelPath, V>, digest: impl Fn(&'a V) -> String) -> Units {
    let mut units = Units::new();
    for (path, value) in layer {
        units
            .entry(unit_key(path, layer))
            .or_default()
            .insert(path.as_str().to_string(), digest(value));
    }
    units
}

/// [`group`], with unit keys computed over `keys` ([`unit_key_layer`]).
fn group_keyed<'a, V: 'a>(
    layer: &'a BTreeMap<RelPath, V>,
    keys: &BTreeMap<RelPath, ()>,
    digest: impl Fn(&'a V) -> String,
) -> Units {
    let mut units = Units::new();
    for (path, value) in layer {
        units
            .entry(unit_key(path, keys))
            .or_default()
            .insert(path.as_str().to_string(), digest(value));
    }
    units
}

fn blob_digest(blob: &FileBlob) -> String {
    blob.sha256.clone()
}

/// Whether `key` is a tag-definition or tag-type-definition unit below a
/// provider (`ignition/<type>/<provider>[/...]`). The gateway overrides these
/// by folder, so a mode-collection copy hides `core` files it does not hold.
fn is_tag_folder(key: &str) -> bool {
    let mut parts = key.split('/');
    parts.next() == Some("ignition")
        && matches!(parts.next(), Some("tag-definition" | "tag-type-definition"))
        && parts.next().is_some()
}

/// Whether the `core` copy holds a file the rendered unit lacks.
fn hides(core: &BTreeMap<String, String>, rendered: &BTreeMap<String, String>) -> bool {
    core.keys().any(|path| !rendered.contains_key(path))
}

/// The member data files a tag marker lists (CD5c).
const TAG_MEMBER_FILES: [&str; 2] = ["tags.json", "udts.json"];

/// What an `added` or `changed` tag unit drops from the running gateway
/// (261030-8m8 CR-01), as a plan warning, or `None` when it drops nothing.
///
/// - `live`: the effective live copy's `tags.json` / `udts.json` (any letter
///   case, so a spelling the host folds is never missed) that the rendered
///   unit lacks. The rewritten marker no longer lists them, so the gateway
///   stops loading them (CD5c), even though no file is deleted.
/// - `hidden`: with `core == Hidden`, the `core` files the rendered unit
///   lacks. The mode copy overrides the folder whole (CD5a S4, CD5-CTL).
///
/// Paths only, never content.
fn tag_data_dropped(
    key: &str,
    effective: Option<&BTreeMap<String, String>>,
    core: Option<&BTreeMap<String, String>>,
    relation: CoreRelation,
    rendered: &BTreeMap<String, String>,
) -> Option<String> {
    let live: Vec<&str> = effective
        .into_iter()
        .flat_map(BTreeMap::keys)
        .filter(|path| {
            let name = path.rsplit('/').next().unwrap_or_default();
            TAG_MEMBER_FILES
                .iter()
                .any(|m| m.eq_ignore_ascii_case(name))
                && !rendered.contains_key(*path)
        })
        .map(String::as_str)
        .collect();
    let hidden: Vec<&str> = if relation == CoreRelation::Hidden {
        core.into_iter()
            .flat_map(BTreeMap::keys)
            .filter(|path| !rendered.contains_key(*path))
            .map(String::as_str)
            .collect()
    } else {
        Vec::new()
    };
    if live.is_empty() && hidden.is_empty() {
        return None;
    }
    let mut what = Vec::new();
    if !live.is_empty() {
        what.push(format!(
            "the live {} not rendered, and the gateway loads only the member files a tag folder's unary-resource.json lists (CD5c)",
            if live.len() == 1 {
                format!("{} is", live[0])
            } else {
                format!("{} are", live.join(", "))
            }
        ));
    }
    if !hidden.is_empty() {
        what.push(format!(
            "the mode-collection folder hides the core {} (CD5a)",
            hidden.join(", ")
        ));
    }
    Some(format!(
        "{key} drops tag data the gateway runs now: {}; it is destructive (tag_data_loss), so apply needs --allow-destructive",
        what.join("; ")
    ))
}

/// Whether the manifest's `scope` allows `path`: its `<module>/<type>` is a
/// listed type, or it is under `ignition/<tag type>/<provider>` for a listed
/// tag provider.
fn in_scope(scope: &Scope, path: &RelPath) -> bool {
    let parts: Vec<&str> = path.components().collect();
    if parts.len() < 3 {
        return false;
    }
    let (module, ty) = (parts[0], parts[1]);
    if scope.types.iter().any(|(m, t)| m == module && t == ty) {
        return true;
    }
    module == "ignition"
        && TAG_TYPES.contains(&ty)
        && scope.tag_providers.iter().any(|p| p == parts[2])
}

/// The unit keys a ledger path falls in, over the `core` and the mode key
/// layers ([`unit_key_layer`]).
struct KeyLayers<'a> {
    core: &'a BTreeMap<RelPath, ()>,
    mode: &'a BTreeMap<RelPath, ()>,
}

/// The delete set (module doc), the warnings it raised (the per-path
/// warnings in path byte order, then one warning per excluded tag provider
/// of `excluded`, the effective exclusion set, named as `excluded` spells it
/// and counting every case spelling of it in the ledger, in provider byte
/// order), and
/// the keys of the `unreadable` units the ledger lists.
fn delete_set(
    rendered: &RenderedTree,
    observed: &ObservedFiles,
    ledger: Option<&Ledger>,
    scope: &Scope,
    excluded: &[String],
    layers: &KeyLayers<'_>,
    unreadable: &BTreeMap<String, UnreadableKey>,
) -> (Vec<RelPath>, Vec<String>, BTreeSet<String>) {
    let Some(ledger) = ledger else {
        return (Vec::new(), Vec::new(), BTreeSet::new());
    };
    let rendered_folded: BTreeSet<String> = rendered
        .files
        .keys()
        .map(|p| p.as_str().to_ascii_lowercase())
        .collect();
    let mut listed: Vec<&String> = ledger.files.iter().collect();
    listed.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    listed.dedup();

    let mut delete = Vec::new();
    let mut warnings = Vec::new();
    let mut excluded_files: BTreeMap<String, usize> = BTreeMap::new();
    let mut ledgered: BTreeSet<String> = BTreeSet::new();
    for raw in listed {
        let Ok(path) = RelPath::new(raw) else {
            continue;
        };
        if path
            .components()
            .any(|c| c.starts_with('.') || GATEWAY_OWNED_FILES.contains(&c))
        {
            continue;
        }
        // An excluded provider is never written or deleted: checked before
        // the scope, which matches bytes exactly and might name it.
        if let Some(provider) = exclude::excluded_provider(&path, excluded) {
            // counted under the exclusion entry's spelling, so every case
            // spelling the ledger uses lands in one count (C4)
            let entry = excluded
                .iter()
                .find(|e| e.eq_ignore_ascii_case(provider))
                .map_or(provider, String::as_str);
            *excluded_files.entry(entry.to_string()).or_default() += 1;
            continue;
        }
        if !in_scope(scope, &path) {
            warnings.push(format!(
                "ledger file {path} is under a type or tag provider the manifest no longer allows; it is not deleted"
            ));
            continue;
        }
        // An unreadable unit is never absent: checked before the absent
        // test below, which would otherwise read it as already gone.
        if !unreadable.is_empty() {
            let hit = [unit_key(&path, layers.mode), unit_key(&path, layers.core)]
                .into_iter()
                .find(|key| unreadable.contains_key(key));
            if let Some(key) = hit {
                ledgered.insert(key);
                continue;
            }
        }
        if rendered.files.contains_key(&path) || !observed.mode.contains_key(&path) {
            continue;
        }
        if rendered_folded.contains(&path.as_str().to_ascii_lowercase()) {
            warnings.push(format!(
                "ledger file {path} differs from a rendered file only by case; it is not deleted because a case-insensitive host would delete the rendered file"
            ));
            continue;
        }
        delete.push(path);
    }
    // String order is byte order
    for (provider, count) in excluded_files {
        warnings.push(format!(
            "ledger lists {count} file(s) under tag provider {provider}, which is excluded (exclude_tag_providers or the built-in System default); they are not deleted, because an excluded provider is never written or deleted"
        ));
    }
    (delete, warnings, ledgered)
}

/// The `destructive_reason` of a destructive entry (module doc).
fn destructive_reason(module: &str, ty: &str, status: PlanStatus) -> &'static str {
    match (module, ty) {
        ("ignition", "tag-provider" | "tag-group" | "tag-type-definition" | "tag-definition") => {
            "tag_data_loss"
        }
        ("ignition", "database-connection" | "opc-connection" | "identity-provider") => {
            "connection_removed"
        }
        ("ignition", "store-and-forward-engine" | "alarm-journal") => "storage_removed",
        _ if status == PlanStatus::Removed => "resource_removed",
        _ => "files_removed",
    }
}

/// Compare `rendered` with `observed`. `ledger` is the parsed declared-names
/// ledger, `None` for a first deploy; `scope` is the manifest's current
/// allowlist (it bounds what the ledger may delete); `excluded` is the
/// effective exclusion set ([`super::exclude::effective`]): ledger files
/// under an excluded tag provider are never deleted.
pub fn plan(
    rendered: &RenderedTree,
    observed: &ObservedFiles,
    ledger: Option<&Ledger>,
    scope: &Scope,
    excluded: &[String],
) -> Plan {
    let unreadable_keys = unreadable_unit_keys(observed);
    let core_layer = unit_key_layer(observed, "core");
    let mode_layer = unit_key_layer(observed, "mode");
    let mut desired = group(&rendered.files, |bytes: &Vec<u8>| sha256_hex(bytes));
    let mut core = group_keyed(&observed.core, &core_layer, blob_digest);
    let mut mode = group_keyed(&observed.mode, &mode_layer, blob_digest);
    let layers = KeyLayers {
        core: &core_layer,
        mode: &mode_layer,
    };
    let (delete, ledger_warnings, ledgered) = delete_set(
        rendered,
        observed,
        ledger,
        scope,
        excluded,
        &layers,
        &unreadable_keys,
    );
    let mut deleted: BTreeMap<String, usize> = BTreeMap::new();
    for path in &delete {
        *deleted.entry(unit_key(path, &mode_layer)).or_default() += 1;
    }

    // No partial units: an unreadable unit leaves every grouping before the
    // keys are built (key byte order, as the map iterates).
    let mut unreadable = Vec::with_capacity(unreadable_keys.len());
    for (key, info) in &unreadable_keys {
        let declared = desired.remove(key).is_some();
        let in_core = core.remove(key).is_some() || info.collections.contains("core");
        let in_mode = mode.remove(key).is_some() || info.collections.contains("mode");
        let mut live_in = Vec::new();
        if in_core {
            live_in.push("core".to_string());
        }
        if in_mode {
            live_in.push("mode".to_string());
        }
        unreadable.push(UnreadableUnit {
            key: key.clone(),
            live_in,
            declared,
            in_ledger: ledgered.contains(key),
            paths: info.paths.clone(),
            longest: info.longest,
        });
    }

    let keys: BTreeSet<&String> = desired
        .keys()
        .chain(core.keys())
        .chain(mode.keys())
        .collect();
    let empty = BTreeMap::new();

    let mut entries = Vec::with_capacity(keys.len());
    let mut tag_loss_warnings: Vec<String> = Vec::new();
    for key in keys {
        let wanted = desired.get(key);
        let core_copy = core.get(key);
        let mode_copy = mode.get(key);
        let effective = mode_copy.or(core_copy);

        let mut live_in = Vec::new();
        if core_copy.is_some() {
            live_in.push("core".to_string());
        }
        if mode_copy.is_some() {
            live_in.push("mode".to_string());
        }

        let deleted_here = deleted.get(key).copied().unwrap_or(0);
        let status = match (wanted, effective) {
            (Some(_), None) => PlanStatus::Added,
            (Some(w), Some(e)) if w == e && deleted_here == 0 => PlanStatus::Same,
            (Some(_), Some(_)) => PlanStatus::Changed,
            (None, _) if deleted_here > 0 => PlanStatus::Removed,
            (None, _) => PlanStatus::Unmanaged,
        };
        let rendered_files = wanted.unwrap_or(&empty);
        let core_relation = match core_copy {
            None => CoreRelation::None,
            Some(_) if status == PlanStatus::Removed => CoreRelation::ShadowsRemoval,
            Some(c) if is_tag_folder(key) && mode_copy.is_some() && hides(c, rendered_files) => {
                CoreRelation::Hidden
            }
            Some(c) if c == rendered_files => CoreRelation::Identical,
            Some(_) => CoreRelation::Differs,
        };

        let paths: BTreeSet<&String> = wanted
            .into_iter()
            .chain(effective)
            .flat_map(BTreeMap::keys)
            .collect();
        let files = paths
            .into_iter()
            .map(|path| PlanFile {
                path: path.clone(),
                live_sha256: effective.and_then(|e| e.get(path)).cloned(),
                desired_sha256: wanted.and_then(|w| w.get(path)).cloned(),
            })
            .collect();

        let parts: Vec<&str> = key.split('/').collect();
        let module = parts.first().copied().unwrap_or_default();
        let ty = parts.get(1).copied().unwrap_or_default();
        // CR-01 (261030-8m8): a tag unit the apply writes can drop live tag
        // data without deleting a file. A `same` unit is not touched.
        let dropped = (matches!(status, PlanStatus::Added | PlanStatus::Changed)
            && is_tag_folder(key))
        .then(|| tag_data_dropped(key, effective, core_copy, core_relation, rendered_files))
        .flatten();
        let destructive = status == PlanStatus::Removed || deleted_here > 0 || dropped.is_some();
        tag_loss_warnings.extend(dropped);
        let lockout_risk = matches!(
            status,
            PlanStatus::Added | PlanStatus::Changed | PlanStatus::Removed
        ) && module == "ignition"
            && matches!(ty, "identity-provider" | "security-zone");
        entries.push(PlanEntry {
            key: key.clone(),
            module: module.to_string(),
            ty: ty.to_string(),
            name: (parts.len() >= 3).then(|| parts[2..].join("/")),
            status,
            live_in,
            core: core_relation,
            destructive,
            destructive_reason: destructive
                .then(|| destructive_reason(module, ty, status).to_string()),
            lockout_risk,
            drift: None,
            files,
            diff: None,
        });
    }

    let mut summary = PlanSummary::default();
    for entry in &entries {
        match entry.status {
            PlanStatus::Added => summary.added += 1,
            PlanStatus::Changed => summary.changed += 1,
            PlanStatus::Removed => summary.removed += 1,
            PlanStatus::Same => summary.same += 1,
            PlanStatus::Unmanaged => summary.unmanaged += 1,
        }
    }
    summary.unreadable = unreadable.len();
    // P6: an unreadable unit the deploy must act on is a change
    summary.has_changes = summary.added + summary.changed + summary.removed > 0
        || unreadable.iter().any(|u| u.declared || u.in_ledger);

    let mut warnings: Vec<String> = observed
        .local_units
        .iter()
        .map(|unit| {
            format!(
                "{unit} also exists in the `local` collection, which the gateway loads over the mode collection; the plan does not describe what the gateway runs for it"
            )
        })
        .collect();
    warnings.extend(ledger_warnings);
    warnings.extend(tag_loss_warnings);
    for unit in unreadable.iter().filter(|u| !u.declared && !u.in_ledger) {
        warnings.push(format!(
            "{} has a live path of {} characters, which the host's sftp server cannot read; it is not declared, so it is reported as unreadable and left alone",
            unit.key, unit.longest
        ));
    }

    let plan_sha256 = sha256_hex(&canonical_text(
        &rendered.render_sha256(),
        &entries,
        &delete,
        &unreadable,
    ));
    Plan {
        entries,
        summary,
        delete_set: DeleteSet { files: delete },
        plan_sha256,
        warnings,
        unreadable,
    }
}

/// A previous render (a `deploy render --out` directory), re-hashed from its
/// real bytes (`--base`, 22-04).
///
/// `render.json` is read for identity (`gateway`, `env`, `mode`) and as a
/// consistency check only: its file list, every per-file sha256 and its
/// `render_sha256` must agree with what [`BaseTree::read`] recomputes from the
/// directory, so a crash-unsafe refresh (21-SECURITY R-03 / WR-03), a leftover
/// `.ign-render-tmp` file, or any hand edit is refused instead of being
/// trusted. File bytes are hashed while streaming and never kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseTree {
    /// The directory as given.
    pub dir: String,
    /// The recomputed `render_sha256` (equals the marker's).
    pub render_sha256: String,
    /// Recomputed sha256 per collection path (`render.json` excluded).
    pub files: BTreeMap<RelPath, String>,
}

/// `--base <dir>: <problem>[ (first difference: <path>)]`.
fn base_refusal(dir: &Path, problem: impl std::fmt::Display, first: Option<&str>) -> CoreError {
    let at = first.map_or_else(String::new, |p| format!(" (first difference: {p})"));
    CoreError::InvalidInput {
        reason: format!("--base {}: {problem}{at}", dir.display()),
    }
}

/// The first path (byte order) held by exactly one of the two sets.
fn first_difference<'a>(a: &'a BTreeSet<String>, b: &'a BTreeSet<String>) -> Option<&'a String> {
    a.symmetric_difference(b)
        .min_by(|x, y| x.as_bytes().cmp(y.as_bytes()))
}

impl BaseTree {
    /// Read and verify `dir` as a clean render of `gateway`, `env`, `mode`.
    ///
    /// `dir` must be a real directory (not a symlink). It is walked with
    /// `symlink_metadata`, capped at [`READ_MAX_ENTRIES`]; a symlink, special
    /// file or non-UTF-8 name anywhere under it is refused. `render.json`
    /// must parse as a schema 1 [`RenderMarker`] of the same gateway,
    /// environment and mode; the walked files must be exactly the marker's
    /// files; every recomputed sha256, size and the recomputed listing digest
    /// must equal the marker's. Every refusal is `invalid_input` naming the
    /// directory, the problem and the first differing path; none echoes file
    /// content.
    pub fn read(dir: &Path, gateway: &str, env: &str, mode: &str) -> Result<BaseTree, CoreError> {
        let meta = match std::fs::symlink_metadata(dir) {
            Ok(meta) => meta,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(base_refusal(dir, "does not exist", None));
            }
            Err(e) => {
                return Err(base_refusal(
                    dir,
                    format!("cannot inspect it ({})", e.kind()),
                    None,
                ));
            }
        };
        if meta.file_type().is_symlink() {
            return Err(base_refusal(
                dir,
                "is a symlink; symlinks are refused",
                None,
            ));
        }
        if !meta.file_type().is_dir() {
            return Err(base_refusal(dir, "is not a directory", None));
        }

        let mut walked: BTreeMap<String, std::path::PathBuf> = BTreeMap::new();
        let mut dirs: BTreeSet<String> = BTreeSet::new();
        walk_capped(dir, "", &mut walked, &mut dirs, READ_MAX_ENTRIES)
            .map_err(|why| base_refusal(dir, why, None))?;

        let Some(marker_path) = walked.remove(RENDER_MARKER) else {
            return Err(base_refusal(
                dir,
                format!("holds no {RENDER_MARKER}; it is not a `deploy render --out` directory"),
                Some(RENDER_MARKER),
            ));
        };
        let marker = read_marker(dir, &marker_path)?;
        if marker.schema_version != 1 {
            return Err(base_refusal(
                dir,
                format!(
                    "{RENDER_MARKER} has schema_version {}, expected 1",
                    marker.schema_version
                ),
                None,
            ));
        }
        for (what, found, expected) in [
            ("gateway", &marker.gateway, gateway),
            ("env", &marker.env, env),
            ("mode", &marker.mode, mode),
        ] {
            if found != expected {
                return Err(base_refusal(
                    dir,
                    format!("{RENDER_MARKER} {what} is {found}, this plan's is {expected}"),
                    None,
                ));
            }
        }

        let mut listed: BTreeMap<&str, &MarkerFile> = BTreeMap::new();
        for entry in &marker.files {
            if listed.insert(entry.path.as_str(), entry).is_some() {
                return Err(base_refusal(
                    dir,
                    format!("{RENDER_MARKER} lists a path twice"),
                    Some(&entry.path),
                ));
            }
        }
        let found: BTreeSet<String> = walked.keys().cloned().collect();
        let named: BTreeSet<String> = listed.keys().map(|p| (*p).to_string()).collect();
        if let Some(path) = first_difference(&found, &named) {
            let problem = if path.ends_with(TMP_SUFFIX) {
                "holds a leftover temporary file from an interrupted render".to_string()
            } else if found.contains(path) {
                format!("holds a file not listed in {RENDER_MARKER}")
            } else {
                format!("is missing a file listed in {RENDER_MARKER}")
            };
            return Err(base_refusal(dir, problem, Some(path)));
        }
        for d in &dirs {
            let prefix = format!("{d}/");
            if !named.iter().any(|p| p.starts_with(&prefix)) {
                return Err(base_refusal(
                    dir,
                    "holds a directory with no rendered file",
                    Some(d),
                ));
            }
        }

        let mut files: BTreeMap<RelPath, String> = BTreeMap::new();
        let mut total: u64 = 0;
        for (rel, path) in &walked {
            let entry = listed[rel.as_str()];
            let key = RelPath::new(rel).map_err(|why| {
                base_refusal(
                    dir,
                    format!("{RENDER_MARKER} lists an unusable path ({why})"),
                    Some(rel),
                )
            })?;
            let (sha256, len) = hash_base_file(dir, rel, path, &mut total)?;
            if sha256 != entry.sha256 || len != entry.bytes {
                return Err(base_refusal(
                    dir,
                    format!("a file differs from the sha256 recorded in {RENDER_MARKER}"),
                    Some(rel),
                ));
            }
            files.insert(key, sha256);
        }

        let render_sha256 = listing_digest(&files);
        if render_sha256 != marker.render_sha256 {
            return Err(base_refusal(
                dir,
                format!("the files do not match the render_sha256 recorded in {RENDER_MARKER}"),
                None,
            ));
        }
        Ok(BaseTree {
            dir: dir.to_string_lossy().into_owned(),
            render_sha256,
            files,
        })
    }
}

/// `render.json`, read through the descriptor-relative helper and parsed
/// without echoing anything the file says.
fn read_marker(dir: &Path, path: &Path) -> Result<RenderMarker, CoreError> {
    let before = std::fs::symlink_metadata(path).map_err(|e| {
        base_refusal(
            dir,
            format!("cannot inspect {RENDER_MARKER} ({})", e.kind()),
            None,
        )
    })?;
    let (file, held) = open_base_file(dir, RENDER_MARKER, &before)?;
    if held.len() > READ_MAX_FILE_BYTES {
        return Err(base_refusal(
            dir,
            format!("{RENDER_MARKER} is over the {READ_MAX_FILE_BYTES} byte cap"),
            None,
        ));
    }
    let mut bytes = Vec::new();
    file.take(READ_MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| {
            base_refusal(
                dir,
                format!("cannot read {RENDER_MARKER} ({})", e.kind()),
                None,
            )
        })?;
    if bytes.len() as u64 > READ_MAX_FILE_BYTES {
        return Err(base_refusal(
            dir,
            format!("{RENDER_MARKER} is over the {READ_MAX_FILE_BYTES} byte cap"),
            None,
        ));
    }
    serde_json::from_slice(&bytes).map_err(|_| {
        base_refusal(
            dir,
            format!("{RENDER_MARKER} is not a valid render marker"),
            None,
        )
    })
}

fn open_base_file(
    dir: &Path,
    rel: &str,
    before: &std::fs::Metadata,
) -> Result<(std::fs::File, std::fs::Metadata), CoreError> {
    open_regular_file(dir, Path::new(rel), before).map_err(|e| match e {
        OpenError::Changed => base_refusal(dir, "changed while it was read", Some(rel)),
        OpenError::Io(e) => {
            base_refusal(dir, format!("cannot read a file ({})", e.kind()), Some(rel))
        }
    })
}

/// Stream one base file through SHA-256 (never keeping its bytes), enforcing
/// the per-file and total byte caps. Returns the digest and the byte count.
fn hash_base_file(
    dir: &Path,
    rel: &str,
    path: &Path,
    total: &mut u64,
) -> Result<(String, u64), CoreError> {
    let before = std::fs::symlink_metadata(path).map_err(|e| {
        base_refusal(
            dir,
            format!("cannot inspect a file ({})", e.kind()),
            Some(rel),
        )
    })?;
    if !before.file_type().is_file() {
        return Err(base_refusal(dir, "is no longer a regular file", Some(rel)));
    }
    let (file, held) = open_base_file(dir, rel, &before)?;
    if held.len() > READ_MAX_FILE_BYTES {
        return Err(base_refusal(
            dir,
            format!("a file is over the {READ_MAX_FILE_BYTES} byte cap"),
            Some(rel),
        ));
    }
    *total += held.len();
    if *total > READ_MAX_TOTAL_BYTES {
        return Err(base_refusal(
            dir,
            format!("the files exceed {READ_MAX_TOTAL_BYTES} bytes in total"),
            Some(rel),
        ));
    }
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    let mut len: u64 = 0;
    let mut reader = file.take(READ_MAX_FILE_BYTES + 1);
    loop {
        let n = reader.read(&mut buf).map_err(|e| {
            base_refusal(dir, format!("cannot read a file ({})", e.kind()), Some(rel))
        })?;
        if n == 0 {
            break;
        }
        len += n as u64;
        if len > READ_MAX_FILE_BYTES {
            return Err(base_refusal(
                dir,
                format!("a file grew past the {READ_MAX_FILE_BYTES} byte cap while it was read"),
                Some(rel),
            ));
        }
        hasher.update(&buf[..n]);
    }
    Ok((format!("{:x}", hasher.finalize()), len))
}

/// The `render_sha256` listing digest over `path NUL sha256 LF` lines, the
/// same text [`RenderedTree::render_sha256`] hashes.
fn listing_digest(files: &BTreeMap<RelPath, String>) -> String {
    let mut hasher = Sha256::new();
    for (path, sha256) in files {
        hasher.update(path.as_str().as_bytes());
        hasher.update([0u8]);
        hasher.update(sha256.as_bytes());
        hasher.update(b"\n");
    }
    format!("{:x}", hasher.finalize())
}

/// Mark each entry's `drift` against `base` and count them.
///
/// An entry drifted when any of its files has a live sha256 that differs from
/// both the base sha256 and the desired sha256; an absent side counts as its
/// own value (`None`). This includes `unmanaged` entries (a unit created on
/// the gateway outside the pipeline) and an `added` entry whose live copy was
/// deleted outside the pipeline. The flag is not part of `plan_sha256`.
pub fn apply_base(plan: &mut Plan, base: &BaseTree) {
    let mut drifted = 0;
    for entry in &mut plan.entries {
        let drift = entry.files.iter().any(|file| {
            let base_sha = RelPath::new(&file.path)
                .ok()
                .and_then(|path| base.files.get(&path));
            file.live_sha256.as_ref() != base_sha && file.live_sha256 != file.desired_sha256
        });
        entry.drift = Some(drift);
        drifted += usize::from(drift);
    }
    plan.summary.drifted = Some(drifted);
}

/// The longest diff text one entry carries; longer diffs end with a
/// truncation marker.
pub const DIFF_MAX_BYTES: usize = 65_536;

/// Opening words of the line printed in place of a secret-bearing member.
pub const DIFF_WITHHELD: &str = "withheld: contains secret-bearing locations";

/// Fill `diff` on `added`, `changed` and `removed` entries (`--diff`).
///
/// Each file of the entry whose live and desired sha256 differ contributes one
/// section, in path order:
///
/// - a `.json` member whose present sides both parse ([`classify`]) and carry
///   no secret-bearing location on either side is a unified text diff
///   (`similar`, 3 lines of context, headers `live/<path>` and
///   `desired/<path>`; an absent side is empty);
/// - a `.json` member where [`scan_secrets`] (`config.json`) or
///   [`scan_embedded`] (every other `.json`, and tag files) reports any
///   refused location or warning on either side, or where the diff-only key
///   check ([`sensitive_key_hits`]) finds a non-empty string or non-null
///   scalar at or beneath an object key named like `password`, `passwd`,
///   `secret`, `token`, `apikey`, `api_key`, `credential`, `privatekey` or
///   `private_key` (ASCII case-insensitive substring, any file, with or
///   without an OpenAPI description) on either side, is
///   `<path>: withheld: contains secret-bearing locations (<n>); live <sha|->,
///   desired <sha|->`, where `<n>` counts locations only;
/// - any other member, and a `.json` member that does not parse, is
///   `<path>: differs (live <sha|->, desired <sha|->)`.
///
/// The text diff is the only place file bytes reach the plan, and only after
/// both sides passed the scan and the key check, so Embedded ciphertext and
/// plaintext secrets never leave it. The key check exists because the render
/// lints cover the desired (repo) side: the live side is gateway-written and
/// never linted, so `--diff` is deliberately stricter than render. It does
/// not touch the render lints or what render accepts. An entry's text is capped at [`DIFF_MAX_BYTES`] and then
/// ends with a truncation marker. The live side of an entry is its effective
/// copy (the mode collection when the unit exists there, else `core`), the
/// same basis the digests use.
pub fn render_diffs(plan: &mut Plan, rendered: &RenderedTree, observed: &ObservedFiles) {
    for entry in &mut plan.entries {
        if !matches!(
            entry.status,
            PlanStatus::Added | PlanStatus::Changed | PlanStatus::Removed
        ) {
            continue;
        }
        let layer = if entry.live_in.iter().any(|l| l == "mode") {
            &observed.mode
        } else {
            &observed.core
        };
        let mut text = String::new();
        for file in &entry.files {
            if file.live_sha256 == file.desired_sha256 {
                continue;
            }
            let Ok(path) = RelPath::new(&file.path) else {
                continue;
            };
            let live = layer.get(&path).map(|blob| blob.bytes.as_slice());
            let desired = rendered.files.get(&path).map(Vec::as_slice);
            text.push_str(&file_diff(
                &path,
                live,
                desired,
                file.live_sha256.as_deref(),
                file.desired_sha256.as_deref(),
            ));
        }
        entry.diff = (!text.is_empty()).then(|| cap_diff(text));
    }
}

/// One file's section of an entry's diff, newline-terminated.
fn file_diff(
    path: &RelPath,
    live: Option<&[u8]>,
    desired: Option<&[u8]>,
    live_sha: Option<&str>,
    desired_sha: Option<&str>,
) -> String {
    let shas = format!(
        "live {}, desired {}",
        live_sha.unwrap_or("-"),
        desired_sha.unwrap_or("-")
    );
    let hash_only = || format!("{path}: differs ({shas})\n");
    if !path.as_str().ends_with(".json") {
        return hash_only();
    }
    let mut locations = 0;
    for bytes in [live, desired].into_iter().flatten() {
        let Ok((_, node)) = classify(bytes) else {
            return hash_only();
        };
        locations += secret_locations(path, &node) + sensitive_key_hits(&node);
    }
    if locations > 0 {
        return format!("{path}: {DIFF_WITHHELD} ({locations}); {shas}\n");
    }
    // Both sides parsed as JSON, so they are valid UTF-8.
    let old = live.and_then(|b| std::str::from_utf8(b).ok()).unwrap_or("");
    let new = desired
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let diff = TextDiff::from_lines(old, new);
    let mut unified = diff.unified_diff();
    unified
        .context_radius(3)
        .header(&format!("live/{path}"), &format!("desired/{path}"));
    let mut text = unified.to_string();
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

/// How many secret-bearing locations (refused or warned) the render lints
/// find in one parsed member. Counts only; nothing the scan says is kept.
fn secret_locations(path: &RelPath, node: &Node) -> usize {
    let origin = path.as_str();
    let parts: Vec<&str> = path.components().collect();
    let tag_file =
        parts.first() == Some(&"ignition") && parts.get(1).is_some_and(|ty| TAG_TYPES.contains(ty));
    let scan = if tag_file {
        scan_embedded(origin, node)
    } else {
        let key = resource_key(path);
        let skip = if key.name.is_some() { 3 } else { 2 };
        let rest: Vec<&str> = parts.iter().skip(skip).copied().collect();
        scan_secrets(origin, &key, &rest.join("/"), node)
    };
    scan.refused.len() + scan.warnings.len()
}

/// Lowercase key-name fragments that mark a value as a secret for `--diff`.
const SENSITIVE_KEY_FRAGMENTS: [&str; 9] = [
    "password",
    "passwd",
    "secret",
    "token",
    "apikey",
    "api_key",
    "credential",
    "privatekey",
    "private_key",
];

/// Diff-only secret check, stricter than the render lints. The render lints
/// were designed for the desired (repo) side; the live side is gateway-written
/// and never linted, so a plaintext `"password": "x"` in a live non-config
/// `.json` or tag file would otherwise print verbatim in a text diff.
///
/// Walks every object in a parsed member and counts each non-empty string or
/// non-null scalar at or beneath an object key whose ASCII-lowercased name
/// contains one of [`SENSITIVE_KEY_FRAGMENTS`]. It applies whatever the file
/// name or the OpenAPI description says. Counts only: neither the value nor
/// the key path is kept.
fn sensitive_key_hits(node: &Node) -> usize {
    sensitive_walk(node, false)
}

fn sensitive_walk(node: &Node, under_secret_key: bool) -> usize {
    match node {
        Node::Obj(members) => members
            .iter()
            .map(|(key, value)| {
                let lowered = key.to_ascii_lowercase();
                let secret = under_secret_key
                    || SENSITIVE_KEY_FRAGMENTS
                        .iter()
                        .any(|fragment| lowered.contains(fragment));
                sensitive_walk(value, secret)
            })
            .sum(),
        Node::Arr(items) => items
            .iter()
            .map(|item| sensitive_walk(item, under_secret_key))
            .sum(),
        Node::Str(text) => usize::from(under_secret_key && !text.is_empty()),
        Node::Lit(lexeme) => usize::from(under_secret_key && lexeme != "null"),
    }
}

/// Cap an entry's diff at [`DIFF_MAX_BYTES`], ending a longer one with the
/// truncation marker; otherwise drop the trailing newline.
fn cap_diff(mut text: String) -> String {
    if text.len() > DIFF_MAX_BYTES {
        let mut cut = DIFF_MAX_BYTES;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&format!("... [diff truncated at {DIFF_MAX_BYTES} bytes]"));
    } else {
        while text.ends_with('\n') {
            text.pop();
        }
    }
    text
}

/// The canonical text `plan_sha256` hashes (see the module doc).
fn canonical_text(
    render_sha256: &str,
    entries: &[PlanEntry],
    delete_set: &[RelPath],
    unreadable: &[UnreadableUnit],
) -> Vec<u8> {
    let mut text: Vec<u8> = Vec::new();
    text.extend_from_slice(PLAN_HASH_HEADER.as_bytes());
    text.push(b'\n');
    text.extend_from_slice(b"render_sha256\0");
    text.extend_from_slice(render_sha256.as_bytes());
    text.push(b'\n');
    for entry in entries.iter().filter(|e| e.status != PlanStatus::Unmanaged) {
        text.extend_from_slice(b"E\0");
        text.extend_from_slice(entry.key.as_bytes());
        text.push(0);
        text.extend_from_slice(entry.status.word().as_bytes());
        text.push(0);
        text.extend_from_slice(entry.core.word().as_bytes());
        text.push(0);
        text.extend_from_slice(entry.live_in.join(",").as_bytes());
        text.push(b'\n');
        for file in &entry.files {
            text.extend_from_slice(b"F\0");
            text.extend_from_slice(file.path.as_bytes());
            text.push(0);
            text.extend_from_slice(file.live_sha256.as_deref().unwrap_or("-").as_bytes());
            text.push(0);
            text.extend_from_slice(file.desired_sha256.as_deref().unwrap_or("-").as_bytes());
            text.push(b'\n');
        }
    }
    for path in delete_set {
        text.extend_from_slice(b"D\0");
        text.extend_from_slice(path.as_str().as_bytes());
        text.push(b'\n');
    }
    for unit in unreadable {
        text.extend_from_slice(b"U\0");
        text.extend_from_slice(unit.key.as_bytes());
        text.push(0);
        text.extend_from_slice(unit.live_in.join(",").as_bytes());
        text.push(0);
        text.push(if unit.declared { b'1' } else { b'0' });
        text.push(0);
        text.push(if unit.in_ledger { b'1' } else { b'0' });
        text.push(b'\n');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rp(s: &str) -> RelPath {
        RelPath::new(s).unwrap()
    }

    fn layer(paths: &[&str]) -> BTreeMap<RelPath, ()> {
        paths.iter().map(|p| (rp(p), ())).collect()
    }

    #[test]
    fn resource_files_group_under_their_resource_directory() {
        let l = layer(&[
            "ignition/database-translator/POSTGRES/config.json",
            "ignition/database-translator/POSTGRES/resource.json",
        ]);
        let key = unit_key(&rp("ignition/database-translator/POSTGRES/config.json"), &l);
        assert_eq!(key, "ignition/database-translator/POSTGRES");
    }

    #[test]
    fn singletons_group_per_type() {
        let l = layer(&[
            "ignition/general-alarm-settings/config.json",
            "ignition/general-alarm-settings/resource.json",
        ]);
        let key = unit_key(&rp("ignition/general-alarm-settings/config.json"), &l);
        assert_eq!(key, "ignition/general-alarm-settings");
    }

    #[test]
    fn nearest_marker_wins_for_tag_folders() {
        let l = layer(&[
            "ignition/tag-definition/P/unary-resource.json",
            "ignition/tag-definition/P/A/unary-resource.json",
            "ignition/tag-definition/P/A/tags.json",
            "ignition/tag-definition/P/B/tags.json",
        ]);
        assert_eq!(
            unit_key(&rp("ignition/tag-definition/P/A/tags.json"), &l),
            "ignition/tag-definition/P/A"
        );
        // no marker in B itself: the nearest ancestor with one is the provider
        assert_eq!(
            unit_key(&rp("ignition/tag-definition/P/B/tags.json"), &l),
            "ignition/tag-definition/P"
        );
    }

    /// CD5a/CD5b (261030-8m8): root-level tag files join the provider-root
    /// unit; each nested type folder with a marker is its own unit, the
    /// sub-folder-only `A/` included (Observation CD5b-A: marker).
    #[test]
    fn cd5_unit_keys() {
        let l = layer(&[
            "ignition/tag-definition/P/unary-resource.json",
            "ignition/tag-definition/P/tags.json",
            "ignition/tag-definition/P/udts.json",
            "ignition/tag-type-definition/P/unary-resource.json",
            "ignition/tag-type-definition/P/udts.json",
            "ignition/tag-type-definition/P/A/unary-resource.json",
            "ignition/tag-type-definition/P/A/B/unary-resource.json",
            "ignition/tag-type-definition/P/A/B/udts.json",
        ]);
        for (path, key) in [
            (
                "ignition/tag-definition/P/tags.json",
                "ignition/tag-definition/P",
            ),
            (
                "ignition/tag-definition/P/udts.json",
                "ignition/tag-definition/P",
            ),
            (
                "ignition/tag-type-definition/P/udts.json",
                "ignition/tag-type-definition/P",
            ),
            (
                "ignition/tag-type-definition/P/A/unary-resource.json",
                "ignition/tag-type-definition/P/A",
            ),
            (
                "ignition/tag-type-definition/P/A/B/udts.json",
                "ignition/tag-type-definition/P/A/B",
            ),
            (
                "ignition/tag-type-definition/P/A/B/unary-resource.json",
                "ignition/tag-type-definition/P/A/B",
            ),
        ] {
            assert_eq!(unit_key(&rp(path), &l), key, "{path}");
        }
        // without an A/B marker the file falls to the nearest marked ancestor
        let l = layer(&[
            "ignition/tag-type-definition/P/unary-resource.json",
            "ignition/tag-type-definition/P/A/unary-resource.json",
            "ignition/tag-type-definition/P/A/B/udts.json",
        ]);
        assert_eq!(
            unit_key(&rp("ignition/tag-type-definition/P/A/B/udts.json"), &l),
            "ignition/tag-type-definition/P/A"
        );
        let l = layer(&[
            "ignition/tag-type-definition/P/unary-resource.json",
            "ignition/tag-type-definition/P/A/B/udts.json",
        ]);
        assert_eq!(
            unit_key(&rp("ignition/tag-type-definition/P/A/B/udts.json"), &l),
            "ignition/tag-type-definition/P"
        );
    }

    #[test]
    fn no_marker_falls_back_to_the_parent() {
        let l = layer(&["ignition/database-translator/X/config.json"]);
        assert_eq!(
            unit_key(&rp("ignition/database-translator/X/config.json"), &l),
            "ignition/database-translator/X"
        );
    }

    #[test]
    fn empty_inputs_plan_empty() {
        let scope = Scope {
            mode: "dev".to_string(),
            types: Vec::new(),
            tag_providers: Vec::new(),
        };
        let p = plan(
            &RenderedTree::default(),
            &ObservedFiles::default(),
            None,
            &scope,
            &["System".to_string()],
        );
        assert!(p.entries.is_empty());
        assert!(!p.summary.has_changes);
    }
}
