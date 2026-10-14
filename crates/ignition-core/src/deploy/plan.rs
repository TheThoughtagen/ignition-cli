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
//! - under a type or tag provider the manifest no longer allows (reported in
//!   `warnings`, never deleted), or
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
//! input gives the same flags. `destructive` is true for `removed` entries
//! and for any entry with delete-set files; `destructive_reason` is then
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
//! ```
//!
//! One `E` line per entry that is not `unmanaged`, in unit-key byte order,
//! each followed by its `F` lines in path byte order; then one `D` line per
//! delete-set path in byte order (none without a ledger). `status` and `core` are
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

use super::layout::{RelPath, TAG_TYPES};
use super::ledger::Ledger;
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
    /// True for `removed` entries and entries with delete-set files.
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
    /// True when anything would be added, changed or removed. `unmanaged`
    /// never counts (D-06).
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

/// The delete set (module doc) and the warnings it raised, both in path
/// byte order.
fn delete_set(
    rendered: &RenderedTree,
    observed: &ObservedFiles,
    ledger: Option<&Ledger>,
    scope: &Scope,
) -> (Vec<RelPath>, Vec<String>) {
    let Some(ledger) = ledger else {
        return (Vec::new(), Vec::new());
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
        if !in_scope(scope, &path) {
            warnings.push(format!(
                "ledger file {path} is under a type or tag provider the manifest no longer allows; it is not deleted"
            ));
            continue;
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
    (delete, warnings)
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
/// allowlist (it bounds what the ledger may delete).
pub fn plan(
    rendered: &RenderedTree,
    observed: &ObservedFiles,
    ledger: Option<&Ledger>,
    scope: &Scope,
) -> Plan {
    let desired = group(&rendered.files, |bytes: &Vec<u8>| sha256_hex(bytes));
    let core = group(&observed.core, blob_digest);
    let mode = group(&observed.mode, blob_digest);
    let (delete, ledger_warnings) = delete_set(rendered, observed, ledger, scope);
    let mut deleted: BTreeMap<String, usize> = BTreeMap::new();
    for path in &delete {
        *deleted.entry(unit_key(path, &observed.mode)).or_default() += 1;
    }

    let keys: BTreeSet<&String> = desired
        .keys()
        .chain(core.keys())
        .chain(mode.keys())
        .collect();
    let empty = BTreeMap::new();

    let mut entries = Vec::with_capacity(keys.len());
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
        let destructive = status == PlanStatus::Removed || deleted_here > 0;
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
    summary.has_changes = summary.added + summary.changed + summary.removed > 0;

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

    let plan_sha256 = sha256_hex(&canonical_text(
        &rendered.render_sha256(),
        &entries,
        &delete,
    ));
    Plan {
        entries,
        summary,
        delete_set: DeleteSet { files: delete },
        plan_sha256,
        warnings,
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

/// The canonical text `plan_sha256` hashes (see the module doc).
fn canonical_text(render_sha256: &str, entries: &[PlanEntry], delete_set: &[RelPath]) -> Vec<u8> {
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
        );
        assert!(p.entries.is_empty());
        assert!(!p.summary.has_changes);
    }
}
