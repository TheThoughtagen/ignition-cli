//! Project actions (03-01, PROJ-01/02): list with inheritance info,
//! new, copy, rename, set (reparent), delete — serde models OUT, no
//! printing (ARCHITECTURE.md layering: the Phase-6 TUI rides this same
//! layer).
//!
//! 03-02 (PROJ-03/04) adds export/import: the export result carries
//! the static scope metadata (what a project ZIP does and does not
//! contain — roadmap criterion 4), and the import action owns the
//! collision policy — the abort pre-check refuses via `project_find`
//! BEFORE any upload; overwrite skips the pre-check (the server is
//! the authority) and dispatch guards it as destructive.
//!
//! Two-column naming (LOCKED): client models stay wire-faithful; these
//! action results re-expose the SELECTED fields under unit-explicit
//! snake_case keys, ALL keys always present (null when absent) — the
//! stable agent shape; agents must never key-hunt.
//!
//! Every mutation READS BACK via `project_find` — the create/copy/
//! rename/modify response bodies are unverified LOW (the restart
//! `literal true` precedent), so the record the gateway answers with
//! IS the truth the CLI reports.
//!
//! The `parents`/`parents/{name}` endpoints stay OUT of scope: the
//! server is the reparent authority (cycle guard), and PROJ-01's
//! inheritance info comes from the list items themselves.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::client::GatewayApi;
use crate::client::projects::{ProjectCreate, ProjectModify, ProjectRecord};
use crate::client::query::ListQuery;
use crate::error::CoreError;

/// What a project export INCLUDES — the static, documented-once
/// arrays (HIGH confidence: verified from a real git-module-managed
/// 8.3 export tree). Data, not prose — agents key off them (roadmap
/// criterion 4).
pub const EXPORT_INCLUDES: &[&str] = &[
    "views",
    "scripts",
    "named-queries",
    "vision-windows",
    "perspective-themes-styles",
    "reporting",
    "alarm-notification-profiles",
    "webdev-routes",
    "translations",
    "sfc-charts",
];

/// What a project export EXCLUDES — tag providers, tags, and UDTs are
/// GATEWAY CONFIGURATION, not project resources (the git-module
/// convention keeps a separate `tags/` tree precisely because of
/// this).
pub const EXPORT_EXCLUDES: &[&str] = &[
    "tag-providers",
    "tags",
    "udts",
    "gateway-config",
    "database-connections",
    "users-roles",
    "alarm-journal",
    "certificates",
];

/// The scope metadata carried in BOTH export and import JSON data —
/// identical consts, so the statement "what this ZIP does and does
/// not contain" never drifts between the two commands.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExportScope {
    /// Resource families present in a project ZIP.
    pub includes: Vec<&'static str>,
    /// Resource families that live in gateway config instead.
    pub excludes: Vec<&'static str>,
}

impl ExportScope {
    /// Build from the static consts (the single source).
    pub fn new() -> Self {
        Self {
            includes: EXPORT_INCLUDES.to_vec(),
            excludes: EXPORT_EXCLUDES.to_vec(),
        }
    }
}

impl Default for ExportScope {
    fn default() -> Self {
        Self::new()
    }
}

/// Import sanity limit — 512 MB (a real project export is MB-scale;
/// anything past this is a wrong file, not a project). Checked
/// BEFORE any network I/O.
pub const IMPORT_MAX_BYTES: usize = 512 * 1024 * 1024;

/// The local-file-header magic every ZIP carries (`PK\x03\x04`) —
/// the cheap wrong-file guard (Don't-Hand-Roll table: the gateway
/// validates imports; this catches the common mistake).
const ZIP_MAGIC: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];

/// The import collision policy. REST exposes exactly abort and
/// overwrite — `merge` is the Designer import popup's vocabulary and
/// is rejected at the CLI value-enum level (README documents it as
/// Designer-only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CollisionPolicy {
    /// Refuse when the project already exists (default) — the find
    /// pre-check fires BEFORE any upload.
    Abort,
    /// Replace the ENTIRE project: resources absent from the ZIP are
    /// DELETED (replace, not merge — Pitfall 4). Destructive: the CLI
    /// guards it with `--yes`.
    Overwrite,
}

impl CollisionPolicy {
    /// The stable agent-facing label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Abort => "abort",
            Self::Overwrite => "overwrite",
        }
    }
}

/// The >512 MB refusal as a pure size check — testable without a
/// half-gigabyte allocation.
fn import_size_error(len: usize) -> Option<CoreError> {
    (len > IMPORT_MAX_BYTES).then(|| CoreError::InvalidImportFile {
        reason: format!(
            "{len} bytes exceeds the {} MB sanity limit",
            IMPORT_MAX_BYTES / (1024 * 1024)
        ),
    })
}

/// The cheap wrong-file guards, both usage-class (exit 2): the
/// `PK\x03\x04` magic and the 512 MB sanity limit. Runs BEFORE any
/// network I/O (the find pre-check included).
fn validate_import(zip: &[u8]) -> Result<(), CoreError> {
    if !zip.starts_with(&ZIP_MAGIC) {
        return Err(CoreError::InvalidImportFile {
            reason: "missing ZIP magic (PK\\x03\\x04) — not a project export archive".to_string(),
        });
    }
    if let Some(err) = import_size_error(zip.len()) {
        return Err(err);
    }
    // (05-07, Rule 2) Full-structure validation BEFORE any upload:
    // live-witnessed on 8.3.3, a TRUNCATED zip (valid magic, broken
    // tail) imports with `{"success":true,"changes":[]}` and — on
    // overwrite — REPLACES the project with the partial contents
    // (data loss wearing a success face). Walking every member and
    // decompressing it catches truncation/corruption here, where the
    // refusal names the caller's own file to fix (exit 2, zero
    // network).
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).map_err(|err| {
        CoreError::InvalidImportFile {
            reason: format!("not a readable ZIP archive: {err}"),
        }
    })?;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|err| CoreError::InvalidImportFile {
                reason: format!("cannot read import archive member {index}: {err}"),
            })?;
        let name = file.name().to_string();
        let mut sink = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut sink).map_err(|err| {
            CoreError::InvalidImportFile {
                reason: format!("cannot decompress import member {name:?}: {err}"),
            }
        })?;
    }
    Ok(())
}

/// Strip any path components from a `Content-Disposition` basename —
/// the gateway names exports well, but a disposition value is header
/// input and never deserves path trust. `.`/`..`/empty refuse (the
/// caller falls back to `<name>.zip`).
fn sanitize_basename(raw: &str) -> Option<String> {
    let name = raw.rsplit(['/', '\\']).next().unwrap_or(raw).trim();
    if name.is_empty() || name == "." || name == ".." {
        None
    } else {
        Some(name.to_string())
    }
}

/// A filesystem-safe fallback stem for the default export name — a
/// project name is a single segment on the wire, but defense-in-depth
/// replaces any separator that somehow rides along.
fn safe_fallback_stem(name: &str) -> String {
    name.replace(['/', '\\'], "_")
}

/// One project row — the six fields PROJ-01 names.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProjectSummary {
    /// Project name (unique key).
    pub name: String,
    /// Display title (null when unset).
    pub title: Option<String>,
    /// Long description (null when unset).
    pub description: Option<String>,
    /// Whether the project runs.
    pub enabled: bool,
    /// Parent project name — the inheritance link (null at the root).
    pub parent: Option<String>,
    /// Whether THIS project may serve as a parent (null when the
    /// gateway did not report it).
    pub inheritable: Option<bool>,
}

impl ProjectSummary {
    /// Select the six stable fields from a full wire record.
    fn from_record(record: &ProjectRecord) -> Self {
        Self {
            name: record.name.clone(),
            title: record.title.clone(),
            description: record.description.clone(),
            enabled: record.enabled,
            parent: record.parent.clone(),
            inheritable: record.inheritable,
        }
    }
}

/// `ign project list` output model.
#[derive(Debug, Serialize)]
pub struct ProjectsResult {
    /// Every runnable project.
    pub projects: Vec<ProjectSummary>,
}

/// `project new` flags — only provided fields ride the create body
/// (absent = NOT SENT, Pitfall 5); `enabled` is the CLI `--disabled`
/// flag inverted at the dispatch seam.
#[derive(Debug, Default, Clone)]
pub struct NewOptions {
    /// Whether the project starts enabled.
    pub enabled: bool,
    /// Display title.
    pub title: Option<String>,
    /// Long description.
    pub description: Option<String>,
    /// Parent project (inheritance).
    pub parent: Option<String>,
    /// Whether this project may serve as a parent.
    pub inheritable: Option<bool>,
}

/// `project set` flags — ONLY the `Some` fields ride the modify body
/// (absent flag = don't touch — Pitfall 5's modify half).
#[derive(Debug, Default, Clone)]
pub struct SetOptions {
    /// Display title.
    pub title: Option<String>,
    /// Long description.
    pub description: Option<String>,
    /// Parent project — the inheritance move.
    pub parent: Option<String>,
    /// Whether the project runs.
    pub enabled: Option<bool>,
    /// Whether this project may serve as a parent.
    pub inheritable: Option<bool>,
}

impl SetOptions {
    /// Which fields this set touches, in flag order — the human
    /// renderer's `set <fields> on <name>` line.
    fn fields_set(&self) -> Vec<String> {
        let mut fields = Vec::new();
        if self.title.is_some() {
            fields.push("title".to_string());
        }
        if self.description.is_some() {
            fields.push("description".to_string());
        }
        if self.parent.is_some() {
            fields.push("parent".to_string());
        }
        if self.enabled.is_some() {
            fields.push("enabled".to_string());
        }
        if self.inheritable.is_some() {
            fields.push("inheritable".to_string());
        }
        fields
    }
}

/// `ign project copy` output model: the source plus the destination's
/// read-back record (flat in JSON).
#[derive(Debug, Serialize)]
pub struct ProjectCopyResult {
    /// The source name.
    pub from: String,
    /// The destination's read-back record.
    #[serde(flatten)]
    pub project: ProjectSummary,
}

/// `ign project rename` output model: previous name plus the renamed
/// project's read-back record (flat).
#[derive(Debug, Serialize)]
pub struct ProjectRenameResult {
    /// The name before the rename.
    pub previous_name: String,
    /// The renamed project's read-back record.
    #[serde(flatten)]
    pub project: ProjectSummary,
}

/// `ign project set` output model: the read-back record (flat, the
/// stable agent shape) plus which fields this set touched —
/// display-only, serde-skipped so it NEVER appears in JSON.
#[derive(Debug, Serialize)]
pub struct ProjectSetResult {
    /// The fields this set touched (human rendering only).
    #[serde(skip)]
    pub fields: Vec<String>,
    /// The post-set read-back record.
    #[serde(flatten)]
    pub project: ProjectSummary,
}

/// `ign project delete` output model.
#[derive(Debug, Serialize)]
pub struct ProjectDeleteResult {
    /// The deleted project's name.
    pub deleted: String,
}

/// `ign project export` output model: `{project, file, bytes, scope}`
/// — the FILE is the artifact; stdout stays data-only.
#[derive(Debug, Serialize)]
pub struct ExportResult {
    /// The exported project's name.
    pub project: String,
    /// Path of the file written (the `-o` value, or the resolved
    /// default name).
    pub file: String,
    /// Bytes streamed to disk (chunk-counted).
    pub bytes: u64,
    /// What the ZIP does and does not contain (roadmap criterion 4).
    pub scope: ExportScope,
}

/// `ign project import` output model: `{name, collision_policy,
/// bytes, scope, outcome}` — `outcome` is the opaque server answer
/// (an object when JSON, else the success fallback).
#[derive(Debug, Serialize)]
pub struct ImportResult {
    /// The name imported under.
    pub name: String,
    /// The policy that ran (`abort` | `overwrite`).
    pub collision_policy: String,
    /// Bytes uploaded.
    pub bytes: usize,
    /// What the ZIP does and does not contain — the SAME consts as
    /// export's, so the pair never drifts.
    pub scope: ExportScope,
    /// The server's opaque answer.
    pub outcome: serde_json::Value,
}

// ---- Cross-gateway diff & sync (07-01, SYNC-01/02) -----------------------
//
// The promotion pair: see exactly what differs between two gateways'
// copy of a project, then push selected resources across. Both
// orchestrate over TWO `GatewayApi` handles (source A, target B) and
// ride the pure diff engine in [`crate::client::resources`]
// (normalized member compare — the volatility guard) plus the 05-02
// surgery helpers (replace_member's descriptor-merge landing rules
// ride free).

/// One `project.json` semantic-field difference — `(field, a, b)`
/// surfaced as named keys (the flat agent shape).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectMetaDelta {
    /// The compared field (`title` | `enabled` | `parent`).
    pub field: String,
    /// Profile A's value (stringified; `null` when absent).
    pub a: String,
    /// Profile B's value (stringified; `null` when absent).
    pub b: String,
}

/// `ign project diff` output model — the flat agent shape, ALL keys
/// always. `scope` is the literal `"project"` (the scope-honesty
/// mandate: tag providers live on a different seam, README documents
/// the promotion pipe); `profile_a`/`profile_b` ride the DATA while
/// the envelope keeps its single active-profile field (the frozen
/// one-field envelope).
#[derive(Debug, Serialize)]
pub struct ProjectDiffResult {
    /// Always `"project"` — the diff's scope contract.
    pub scope: &'static str,
    /// The baseline profile (A).
    pub profile_a: String,
    /// The compared profile (B — statuses are B-relative-to-A).
    pub profile_b: String,
    /// The project compared.
    pub project: String,
    /// Root `project.json` semantic-field differences (title/enabled/
    /// parent) — empty when none.
    pub project_meta: Vec<ProjectMetaDelta>,
    /// The four member counts.
    pub summary: crate::client::resources::DiffSummary,
    /// One row per resource member, path-sorted.
    pub entries: Vec<crate::client::resources::MemberDiffEntry>,
}

/// `ign project diff A B --project NAME` — export both sides (A
/// first), run the normalized member compare plus the project.json
/// meta delta. A missing project on either side surfaces through
/// export's existing not-found path; the same profile twice is a
/// usage-class refusal (exit 2) before any network I/O.
pub async fn project_diff(
    api_a: &dyn GatewayApi,
    api_b: &dyn GatewayApi,
    project: &str,
    profile_a: &str,
    profile_b: &str,
) -> Result<ProjectDiffResult, CoreError> {
    if profile_a == profile_b {
        return Err(CoreError::InvalidInput {
            reason: "diffing a profile against itself is a no-op — name two \
                     different profiles"
                .to_string(),
        });
    }
    let zip_a = crate::actions::resources::export_zip_bytes(api_a, project).await?;
    let zip_b = crate::actions::resources::export_zip_bytes(api_b, project).await?;
    let diff = crate::client::resources::diff_members(&zip_a, &zip_b)?;
    let project_meta = crate::client::resources::project_meta_delta(&zip_a, &zip_b)?
        .into_iter()
        .map(|(field, a, b)| ProjectMetaDelta { field, a, b })
        .collect();
    Ok(ProjectDiffResult {
        scope: "project",
        profile_a: profile_a.to_string(),
        profile_b: profile_b.to_string(),
        project: project.to_string(),
        project_meta,
        summary: diff.summary,
        entries: diff.entries,
    })
}

/// What `project sync` promotes from A into B (07-01, SYNC-02) — at
/// least one half is required (the CLI validates pre-resolution; the
/// action re-validates for its other callers).
#[derive(Debug, Default, Clone)]
pub struct SyncSelection {
    /// Explicit `--resource` user paths (repeatable; combines with
    /// `all_changed`).
    pub resources: Vec<String>,
    /// `--all-changed`: take the diff's `added`+`changed` paths —
    /// never `removed` (deletion is the separate `--delete`
    /// opt-in's job).
    pub all_changed: bool,
}

/// `ign project sync` output model — the flat agent shape, ALL keys
/// always (empty vecs when none). Direction is ALWAYS explicit A→B
/// (source A, target B).
#[derive(Debug, Serialize)]
pub struct ProjectSyncResult {
    /// Always `"project"` — the sync's scope contract.
    pub scope: &'static str,
    /// The source profile (A).
    pub profile_a: String,
    /// The target profile (B).
    pub profile_b: String,
    /// The project promoted.
    pub project: String,
    /// The user paths promoted A→B (upserted).
    pub synced: Vec<String>,
    /// The user paths removed from B (`--delete` only).
    pub removed: Vec<String>,
}

/// `ign project sync A B --project NAME` — the guarded promotion.
/// Order is the contract: export A then B → resolve the selection
/// (explicit `--resource` paths must exist in A unless `--delete`
/// wants them removed from B; `--all_changed` rides the diff) →
/// splice A's member bytes into B's zip via the surgery helpers
/// (`replace_member`'s descriptor-merge landing rules ride free —
/// 05-07's put-new hazard is handled) → optional `remove_member`
/// passes for deletions → `validate_import` + ONE overwrite-import
/// into B. B's root `project.json` is never touched (only resource
/// members splice). An EMPTY effective selection performs NO import
/// (zero writes) and reports empty lists.
pub async fn project_sync(
    api_a: &dyn GatewayApi,
    api_b: &dyn GatewayApi,
    project: &str,
    selection: &SyncSelection,
    delete: bool,
    profile_a: &str,
    profile_b: &str,
) -> Result<ProjectSyncResult, CoreError> {
    if selection.resources.is_empty() && !selection.all_changed {
        return Err(CoreError::InvalidInput {
            reason: "sync needs a selection — pass --resource PATH (repeatable) \
                     and/or --all-changed"
                .to_string(),
        });
    }
    let zip_a = crate::actions::resources::export_zip_bytes(api_a, project).await?;
    let zip_b = crate::actions::resources::export_zip_bytes(api_b, project).await?;

    // Resolve the selection: upserts (A's bytes land in B) and — only
    // under --delete — removals (B loses what A no longer has).
    let mut upserts: Vec<String> = Vec::new();
    let mut removals: Vec<String> = Vec::new();
    for path in &selection.resources {
        match crate::client::resources::read_member(&zip_a, path) {
            Ok(_) => upserts.push(path.clone()),
            // An explicit path absent in A is a DELETION request under
            // --delete (removed from B below); without --delete it is
            // the missing-member shape.
            Err(CoreError::NotFound { .. }) if delete => removals.push(path.clone()),
            Err(other) => return Err(other),
        }
    }
    if selection.all_changed {
        // LABEL RECONCILIATION (must_haves over the plan sketch): the
        // diff speaks B-relative-to-A (`added` = in B only, `removed`
        // = in A only) while sync speaks A→B promotion. For A's
        // resources to LAND in B, the upsert set is everything A has
        // that B lacks or differs on — the diff's `removed` (A-only)
        // and `changed` (differing) — and the `--delete` removal set
        // is B's extras, the diff's `added` (B-only). Pushing the
        // diff's `added` set would read members A does not have.
        for entry in crate::client::resources::diff_members(&zip_a, &zip_b)?.entries {
            match entry.status {
                crate::client::resources::MemberStatus::Removed
                | crate::client::resources::MemberStatus::Changed => {
                    upserts.push(entry.path);
                }
                crate::client::resources::MemberStatus::Added if delete => {
                    removals.push(entry.path);
                }
                _ => {}
            }
        }
    }
    upserts.sort();
    upserts.dedup();
    removals.sort();
    removals.dedup();

    // The surgery: splice A's members into B's zip, then drop the
    // removals. replace_member's put-new descriptor rules ride free.
    let mut surgical = zip_b;
    for path in &upserts {
        let bytes = crate::client::resources::read_member(&zip_a, path)?;
        surgical = crate::client::resources::replace_member(&surgical, path, &bytes)?;
    }
    for path in &removals {
        surgical = crate::client::resources::remove_member(&surgical, path)?;
    }

    // Zero-write honesty: an empty selection (nothing to upsert,
    // nothing to remove) performs NO import — a whole-project
    // overwrite-import of an unchanged zip is not a no-op on the
    // gateway, so it must never fire without work to do.
    if !upserts.is_empty() || !removals.is_empty() {
        validate_import(&surgical)?;
        api_b.project_import(project, surgical, true).await?;
    }
    Ok(ProjectSyncResult {
        scope: "project",
        profile_a: profile_a.to_string(),
        profile_b: profile_b.to_string(),
        project: project.to_string(),
        synced: upserts,
        removed: removals,
    })
}

/// `ign project list` — every runnable project with inheritance info
/// (the standard `limit=-1` UI convention).
pub async fn projects(api: &dyn GatewayApi) -> Result<ProjectsResult, CoreError> {
    let page = api.projects(&ListQuery::default()).await?;
    Ok(ProjectsResult {
        projects: page.items.iter().map(ProjectSummary::from_record).collect(),
    })
}

/// `ign project new` — create, then `find` read-back (validates the
/// create and fills the result; the create response body itself is
/// unverified LOW).
pub async fn project_new(
    api: &dyn GatewayApi,
    name: &str,
    opts: &NewOptions,
) -> Result<ProjectSummary, CoreError> {
    let body = ProjectCreate {
        name: name.to_string(),
        enabled: opts.enabled,
        title: opts.title.clone(),
        description: opts.description.clone(),
        parent: opts.parent.clone(),
        inheritable: opts.inheritable,
        default_db: None,
        tag_provider: None,
        user_source: None,
    };
    api.project_create(&body).await?;
    let record = api.project_find(name).await?;
    Ok(ProjectSummary::from_record(&record))
}

/// `ign project copy` — copy all resources, then `find(to)` read-back.
pub async fn project_copy(
    api: &dyn GatewayApi,
    from: &str,
    to: &str,
) -> Result<ProjectCopyResult, CoreError> {
    api.project_copy(from, to).await?;
    let record = api.project_find(to).await?;
    Ok(ProjectCopyResult {
        from: from.to_string(),
        project: ProjectSummary::from_record(&record),
    })
}

/// `ign project rename` — native rename, then `find(new)` read-back.
pub async fn project_rename(
    api: &dyn GatewayApi,
    old: &str,
    new: &str,
) -> Result<ProjectRenameResult, CoreError> {
    api.project_rename(old, new).await?;
    let record = api.project_find(new).await?;
    Ok(ProjectRenameResult {
        previous_name: old.to_string(),
        project: ProjectSummary::from_record(&record),
    })
}

/// `ign project set` — build the modify body from `Some`-fields ONLY
/// (absent flag = don't touch), PUT, then read-back. `--parent` IS the
/// inheritance move.
pub async fn project_set(
    api: &dyn GatewayApi,
    name: &str,
    opts: &SetOptions,
) -> Result<ProjectSetResult, CoreError> {
    let body = ProjectModify {
        enabled: opts.enabled,
        title: opts.title.clone(),
        description: opts.description.clone(),
        parent: opts.parent.clone(),
        inheritable: opts.inheritable,
        default_db: None,
        tag_provider: None,
        user_source: None,
    };
    api.project_modify(name, &body).await?;
    let record = api.project_find(name).await?;
    Ok(ProjectSetResult {
        fields: opts.fields_set(),
        project: ProjectSummary::from_record(&record),
    })
}

/// `ign project delete` — the obedient arm; the `--yes` guard belongs
/// to the CLI CALLER (it refuses pre-resolution, the LOCKED 02-03
/// shape). The wire request always carries `confirm=true`.
pub async fn project_delete(
    api: &dyn GatewayApi,
    name: &str,
) -> Result<ProjectDeleteResult, CoreError> {
    api.project_delete(name).await?;
    Ok(ProjectDeleteResult {
        deleted: name.to_string(),
    })
}

/// `ign project export` — stream the project ZIP to disk. With `-o`
/// the bytes land at exactly that path; without one, the stream goes
/// to `<name>.zip.part` in the working directory and atomically
/// renames to the SANITIZED `Content-Disposition` basename (path
/// components stripped) or the `<name>.zip` fallback — the `.part`
/// is removed best-effort on error, so a failed export leaves no
/// half-written impostor.
pub async fn project_export(
    api: &dyn GatewayApi,
    name: &str,
    output: Option<&Path>,
) -> Result<ExportResult, CoreError> {
    let scope = ExportScope::new();
    if let Some(out) = output {
        let meta = api.project_export_to_file(name, out).await?;
        return Ok(ExportResult {
            project: name.to_string(),
            file: out.display().to_string(),
            bytes: meta.bytes,
            scope,
        });
    }

    // Default naming: stream to <fallback>.part, then rename to the
    // disposition basename (or the fallback) once the meta arrives.
    let fallback = format!("{}.zip", safe_fallback_stem(name));
    let part = PathBuf::from(format!("{fallback}.part"));
    let meta = match api.project_export_to_file(name, &part).await {
        Ok(meta) => meta,
        Err(err) => {
            let _ = std::fs::remove_file(&part); // best-effort
            return Err(err);
        }
    };
    let final_name = meta
        .filename
        .as_deref()
        .and_then(sanitize_basename)
        .unwrap_or(fallback);
    if let Err(err) = std::fs::rename(&part, &final_name) {
        let _ = std::fs::remove_file(&part); // best-effort
        return Err(CoreError::Internal(format!(
            "cannot finalize export {final_name}: {err}"
        )));
    }
    Ok(ExportResult {
        project: name.to_string(),
        file: final_name,
        bytes: meta.bytes,
        scope,
    })
}

/// `ign project import` — order is the contract: magic/size guards
/// (exit 2, zero network) → abort-policy find pre-check (`Ok` →
/// [`CoreError::ProjectExists`] BEFORE any upload) → the raw-body
/// upload with the policy as the wire's `overwrite` query param.
/// Overwrite runs NO pre-check — the server is the authority — and
/// the CLI guards it as destructive upstream of this action.
pub async fn project_import(
    api: &dyn GatewayApi,
    name: &str,
    zip: Vec<u8>,
    policy: CollisionPolicy,
) -> Result<ImportResult, CoreError> {
    let bytes = zip.len();
    let scope = ExportScope::new();
    validate_import(&zip)?;
    if matches!(policy, CollisionPolicy::Abort) && api.project_find(name).await.is_ok() {
        return Err(CoreError::ProjectExists {
            name: name.to_string(),
            endpoint: None,
        });
    }
    let overwrite = matches!(policy, CollisionPolicy::Overwrite);
    let outcome = api.project_import(name, zip, overwrite).await?;
    Ok(ImportResult {
        name: name.to_string(),
        collision_policy: policy.label().to_string(),
        bytes,
        scope,
        outcome: outcome.response,
    })
}

#[cfg(test)]
mod tests {
    use super::{NewOptions, ProjectSummary, SetOptions, project_new, projects};
    use crate::client::GatewayApi;
    use crate::client::projects::{ProjectCreate, ProjectModify, ProjectRecord};
    use crate::client::query::{ListEnvelope, ListMetadata};
    use crate::error::CoreError;

    use std::sync::Mutex;

    /// A recording double: serves one record per find (create/copy/
    /// rename/set read-backs), remembers every create/modify body and
    /// every deleted name. 03-02 grows it into the export/import
    /// double: find honors an `absent` switch (the collision
    /// pre-check's both answers), export writes a fixture ZIP, import
    /// records (name, bytes, overwrite) — the Task-2 action proofs key
    /// off those recordings.
    #[derive(Default)]
    struct ProjectsRig {
        creates: Mutex<Vec<ProjectCreate>>,
        modifies: Mutex<Vec<(String, ProjectModify)>>,
        deletes: Mutex<Vec<String>>,
        finds: Mutex<Vec<String>>,
        exports: Mutex<Vec<String>>,
        imports: Mutex<Vec<(String, usize, bool)>>,
        /// Whether `find` answers 404-NotFound instead of Ok — the
        /// collision pre-check's two outcomes (default: the project
        /// exists, preserving the create/copy/rename/set read-backs).
        absent: bool,
    }

    impl ProjectsRig {
        /// A minimal VALID ZIP fixture (real archive — the action's
        /// import guard walks every member since 05-07; the old
        /// magic-bytes-plus-junk shape now refuses, correctly).
        fn zip_fixture() -> Vec<u8> {
            use std::io::Write as _;
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            let options = zip::write::SimpleFileOptions::default();
            writer
                .start_file("project.json", options)
                .expect("fixture member starts");
            writer
                .write_all(br#"{"title":"fixture"}"#)
                .expect("fixture member writes");
            writer.finish().expect("fixture finalizes").into_inner()
        }
    }

    fn record(name: &str) -> ProjectRecord {
        ProjectRecord {
            name: name.into(),
            title: Some(format!("{name} title")),
            description: None,
            enabled: true,
            parent: Some("Base".into()),
            inheritable: Some(false),
            default_db: None,
            tag_provider: None,
            user_source: None,
            extra: Default::default(),
        }
    }

    fn page(items: Vec<ProjectRecord>) -> ListEnvelope<ProjectRecord> {
        let total = items.len() as i64;
        ListEnvelope {
            items,
            metadata: ListMetadata {
                total,
                matching: total,
                limit: -1,
                offset: 0,
            },
        }
    }

    #[async_trait::async_trait]
    impl GatewayApi for ProjectsRig {
        async fn tag_provider_list(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::tags::TagProviderRecord>,
            CoreError,
        > {
            unreachable!("not part of this action")
        }
        async fn tag_provider_find(
            &self,
            _name: &str,
        ) -> Result<crate::client::tags::TagProviderRecord, CoreError> {
            unreachable!("not part of this action")
        }
        async fn tag_provider_create(
            &self,
            _body: &[crate::client::tags::TagProviderCreate],
        ) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn tag_provider_delete(
            &self,
            _name: &str,
            _signature: &str,
        ) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn trial_status_wire(&self) -> Result<crate::client::trial::TrialWire, CoreError> {
            unreachable!("not part of this action")
        }
        async fn banners(&self) -> Result<crate::client::trial::BannerSet, CoreError> {
            unreachable!("not part of this action")
        }
        async fn trial_reset_wire(&self) -> Result<crate::client::trial::TrialWire, CoreError> {
            unreachable!("not part of this action")
        }
        async fn backup_download(
            &self,
            _out: &std::path::Path,
            _backup_type: crate::client::backup::BackupType,
        ) -> Result<crate::client::projects::ExportMeta, CoreError> {
            unreachable!("not part of this action")
        }
        async fn backup_restore(&self, _gwbk: &std::path::Path) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn gateway_info(&self) -> Result<crate::client::version::GatewayInfo, CoreError> {
            unreachable!("not part of this action")
        }
        async fn overview(&self) -> Result<crate::client::status::Overview, CoreError> {
            unreachable!("not part of this action")
        }
        async fn status_ping(&self) -> Result<crate::client::status::StatusPing, CoreError> {
            unreachable!("not part of this action")
        }
        async fn modules(
            &self,
            _quarantined: bool,
            _query: &crate::client::query::ListQuery,
        ) -> Result<ListEnvelope<crate::client::status::ModuleInfo>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn metrics_current(
            &self,
        ) -> Result<crate::client::metrics::CurrentGauges, CoreError> {
            unreachable!("not part of this action")
        }
        async fn metrics_historic(
            &self,
        ) -> Result<crate::client::metrics::PerformanceCharts, CoreError> {
            unreachable!("not part of this action")
        }
        async fn metrics_threads(&self) -> Result<crate::client::metrics::ThreadCounts, CoreError> {
            unreachable!("not part of this action")
        }
        async fn designers(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<ListEnvelope<crate::client::sessions::DesignerInfo>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn perspective_sessions(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<ListEnvelope<crate::client::sessions::PerspectiveSession>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn vision_clients(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<ListEnvelope<crate::client::sessions::VisionClient>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn terminate_perspective_session(
            &self,
            _id: &str,
            _message: Option<&str>,
        ) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn terminate_vision_client(&self, _id: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn prune_designer(&self, _id: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn database_connections(
            &self,
        ) -> Result<ListEnvelope<crate::client::connections::GatewayConnection>, CoreError>
        {
            unreachable!("not part of this action")
        }
        async fn opc_connections(
            &self,
        ) -> Result<ListEnvelope<crate::client::connections::GatewayConnection>, CoreError>
        {
            unreachable!("not part of this action")
        }
        async fn logs(
            &self,
            _filter: &crate::client::logs::LogQuery,
        ) -> Result<ListEnvelope<crate::client::logs::LogEntry>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn logs_download(&self) -> Result<crate::client::logs::LogDownload, CoreError> {
            unreachable!("not part of this action")
        }
        async fn loggers(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<ListEnvelope<crate::client::logs::LoggerInfo>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn set_logger_level(&self, _logger: &str, _level: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn reset_logger_levels(&self) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn restart(&self) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn scan_projects(&self) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn security_properties(
            &self,
        ) -> Result<crate::client::restart::SecurityProperties, CoreError> {
            unreachable!("not part of this action")
        }
        async fn webdev_route_status(&self, _route: &str) -> Result<u16, CoreError> {
            unreachable!("not part of this action")
        }
        async fn webdev_route_call(
            &self,
            _project: &str,
            _route: &str,
            _body: &serde_json::Value,
            _extra_headers: &[(&str, &str)],
        ) -> Result<serde_json::Value, CoreError> {
            unreachable!("not part of this action")
        }
        async fn webdev_route_probe(
            &self,
            _project: &str,
            _route: &str,
            _extra_headers: &[(&str, &str)],
        ) -> Result<crate::client::webdev::RouteProbe, CoreError> {
            unreachable!("not part of this action")
        }
        async fn projects(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<ListEnvelope<ProjectRecord>, CoreError> {
            Ok(page(vec![record("PlantFloor"), record("Base")]))
        }
        async fn project_find(&self, name: &str) -> Result<ProjectRecord, CoreError> {
            self.finds.lock().unwrap().push(name.into());
            if self.absent {
                Err(CoreError::NotFound { endpoint: None })
            } else {
                Ok(record("whatever-the-rig-is-asked-for"))
            }
        }
        async fn project_create(&self, body: &ProjectCreate) -> Result<(), CoreError> {
            self.creates.lock().unwrap().push(body.clone());
            Ok(())
        }
        async fn project_copy(&self, _from: &str, _to: &str) -> Result<(), CoreError> {
            Ok(())
        }
        async fn project_rename(&self, _name: &str, _new_name: &str) -> Result<(), CoreError> {
            Ok(())
        }
        async fn project_modify(&self, name: &str, body: &ProjectModify) -> Result<(), CoreError> {
            self.modifies
                .lock()
                .unwrap()
                .push((name.into(), body.clone()));
            Ok(())
        }
        async fn project_delete(&self, name: &str) -> Result<(), CoreError> {
            self.deletes.lock().unwrap().push(name.into());
            Ok(())
        }
        async fn project_export_to_file(
            &self,
            name: &str,
            out: &std::path::Path,
        ) -> Result<crate::client::projects::ExportMeta, CoreError> {
            self.exports.lock().unwrap().push(name.into());
            let fixture = Self::zip_fixture();
            std::fs::write(out, &fixture)
                .map_err(|err| CoreError::Internal(format!("rig export write: {err}")))?;
            Ok(crate::client::projects::ExportMeta {
                filename: Some("rig-export.zip".into()),
                bytes: fixture.len() as u64,
                content_type: Some("application/zip".into()),
            })
        }
        async fn project_import(
            &self,
            name: &str,
            zip: Vec<u8>,
            overwrite: bool,
        ) -> Result<crate::client::projects::ImportOutcome, CoreError> {
            self.imports
                .lock()
                .unwrap()
                .push((name.into(), zip.len(), overwrite));
            Ok(crate::client::projects::ImportOutcome {
                response: serde_json::json!({"status": "success"}),
            })
        }
    }

    /// THE modify-body pin: `SetOptions` with only `--title` rides the
    /// wire as EXACTLY `{"title":"T"}` — no other keys, no `enabled`
    /// clobber, no `name`.
    #[test]
    fn set_options_only_title_serializes_exactly_title() {
        let opts = SetOptions {
            title: Some("T".into()),
            ..Default::default()
        };
        let body = ProjectModify {
            enabled: opts.enabled,
            title: opts.title.clone(),
            description: opts.description.clone(),
            parent: opts.parent.clone(),
            inheritable: opts.inheritable,
            default_db: None,
            tag_provider: None,
            user_source: None,
        };
        assert_eq!(
            serde_json::to_value(&body).expect("serializes"),
            serde_json::json!({"title": "T"})
        );
    }

    /// The list action selects the six stable fields (passthrough keys
    /// like `defaultDb` stay at the client seam, not the agent shape).
    #[tokio::test]
    async fn projects_action_selects_the_six_stable_fields() {
        let rig = ProjectsRig::default();
        let result = projects(&rig).await.expect("list");
        assert_eq!(result.projects.len(), 2);
        assert_eq!(
            result.projects[0],
            ProjectSummary {
                name: "PlantFloor".into(),
                title: Some("PlantFloor title".into()),
                description: None,
                enabled: true,
                parent: Some("Base".into()),
                inheritable: Some(false),
            }
        );
        // The agent shape carries ALL six keys, always.
        let json = serde_json::to_value(&result).expect("serialize");
        let mut keys: Vec<&str> = json["projects"][0]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "description",
                "enabled",
                "inheritable",
                "name",
                "parent",
                "title"
            ]
        );
    }

    /// new = create + find read-back: the create body carries ONLY the
    /// provided fields, and the result is the read-back record.
    #[tokio::test]
    async fn project_new_creates_then_reads_back() {
        let rig = ProjectsRig::default();
        let opts = NewOptions {
            enabled: true,
            title: Some("T".into()),
            description: None,
            parent: Some("Base".into()),
            inheritable: Some(true),
        };
        let summary = project_new(&rig, "child", &opts).await.expect("new");
        assert_eq!(summary.name, "whatever-the-rig-is-asked-for");

        let creates = rig.creates.lock().unwrap();
        assert_eq!(creates.len(), 1);
        assert_eq!(
            serde_json::to_value(&creates[0]).unwrap(),
            serde_json::json!({
                "name": "child",
                "enabled": true,
                "title": "T",
                "parent": "Base",
                "inheritable": true
            })
        );
    }

    /// set = modify-with-Somes + read-back; the result records which
    /// fields were touched (display-only — never in JSON) and the flat
    /// JSON stays the six-key record shape.
    #[tokio::test]
    async fn project_set_modifies_with_somes_and_reads_back() {
        let rig = ProjectsRig::default();
        let opts = SetOptions {
            title: Some("T".into()),
            parent: Some("Base".into()),
            ..Default::default()
        };
        let result = super::project_set(&rig, "x", &opts).await.expect("set");
        assert_eq!(result.fields, vec!["title", "parent"]);

        let modifies = rig.modifies.lock().unwrap();
        assert_eq!(modifies.len(), 1);
        assert_eq!(modifies[0].0, "x");
        assert_eq!(
            serde_json::to_value(&modifies[0].1).unwrap(),
            serde_json::json!({"title": "T", "parent": "Base"})
        );

        // JSON: flat record keys only — `fields` is serde-skipped.
        let json = serde_json::to_value(&result).expect("serialize");
        let keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "description",
                "enabled",
                "inheritable",
                "name",
                "parent",
                "title"
            ],
            "no `fields` key in the agent shape"
        );
    }

    /// delete = the obedient arm; the guard belongs to the CLI caller.
    #[tokio::test]
    async fn project_delete_records_the_name() {
        let rig = ProjectsRig::default();
        let result = super::project_delete(&rig, "gone").await.expect("delete");
        assert_eq!(result.deleted, "gone");
        assert_eq!(*rig.deletes.lock().unwrap(), vec!["gone".to_string()]);
    }

    /// THE magic-guard pin: a non-ZIP input refuses with exit 2
    /// `invalid_import_file` BEFORE any network I/O — neither the find
    /// pre-check nor the upload ever fires.
    #[tokio::test]
    async fn import_refuses_non_zip_before_any_network() {
        let rig = ProjectsRig::default();
        let err = super::project_import(
            &rig,
            "x",
            b"definitely not a zip".to_vec(),
            super::CollisionPolicy::Abort,
        )
        .await
        .expect_err("the magic guard refuses");
        assert_eq!(
            err.exit_code(),
            2,
            "usage class — the caller must fix the file"
        );
        assert_eq!(err.code(), "invalid_import_file");
        assert!(
            rig.finds.lock().unwrap().is_empty(),
            "zero pre-check calls — the guard runs first"
        );
        assert!(rig.imports.lock().unwrap().is_empty(), "zero uploads");
    }

    /// THE truncated-zip pin (05-07, Rule 2): a zip with VALID magic
    /// but a broken tail — the live-witnessed wipe shape (8.3.3
    /// answers success:true changes:[] and replaces the project with
    /// the partial contents) — refuses `invalid_import_file` exit 2
    /// BEFORE any network I/O.
    #[tokio::test]
    async fn import_refuses_truncated_zip_before_any_network() {
        let rig = ProjectsRig::default();
        let truncated = {
            let full = ProjectsRig::zip_fixture();
            // Keep the magic + most of the body, cut the central
            // directory — exactly the partially-written-writer shape
            // the spike produced.
            let cut = full.len() - 10;
            full[..cut].to_vec()
        };
        let err = super::project_import(&rig, "x", truncated, super::CollisionPolicy::Overwrite)
            .await
            .expect_err("the structure guard refuses");
        assert_eq!(err.exit_code(), 2);
        assert_eq!(err.code(), "invalid_import_file");
        assert!(
            rig.finds.lock().unwrap().is_empty() && rig.imports.lock().unwrap().is_empty(),
            "zero network of any kind — the structure guard runs before everything"
        );
    }

    /// The 512 MB sanity guard refuses with the same slug — checked
    /// through the pure size helper (no half-gigabyte allocation in a
    /// unit test); exactly-at-limit stays allowed.
    #[test]
    fn import_size_guard_refuses_over_512mb() {
        let err = super::import_size_error(super::IMPORT_MAX_BYTES + 1)
            .expect("one byte over the limit refuses");
        assert_eq!(err.exit_code(), 2);
        assert_eq!(err.code(), "invalid_import_file");
        let message = err.to_string();
        assert!(
            message.contains("512 MB"),
            "the reason names the limit: {message}"
        );
        assert!(
            super::import_size_error(super::IMPORT_MAX_BYTES).is_none(),
            "exactly at the limit is fine"
        );
    }

    /// THE collision pin: abort over an existing project (find → Ok)
    /// refuses with `project_exists` (exit 6) BEFORE the upload, and
    /// the hint names BOTH the overwrite flag and its replace-semantics
    /// warning (Pitfall 4).
    #[tokio::test]
    async fn import_abort_over_existing_refuses_project_exists() {
        let rig = ProjectsRig::default(); // find → Ok: the name exists
        let err = super::project_import(
            &rig,
            "PlantFloor",
            ProjectsRig::zip_fixture(),
            super::CollisionPolicy::Abort,
        )
        .await
        .expect_err("the collision pre-check refuses");
        assert!(
            matches!(&err, CoreError::ProjectExists { name, .. } if name == "PlantFloor"),
            "wrong class: {err}"
        );
        assert_eq!(err.exit_code(), 6);
        assert_eq!(err.code(), "project_exists");
        let hint = err.hint().expect("hint required");
        assert!(
            hint.contains("--collision-policy overwrite"),
            "hint names the flag: {hint}"
        );
        assert!(
            hint.contains("ENTIRE project") && hint.contains("Designer-only"),
            "hint warns replace-not-merge: {hint}"
        );
        assert!(
            rig.imports.lock().unwrap().is_empty(),
            "the refusal happened BEFORE any upload"
        );
        assert_eq!(*rig.finds.lock().unwrap(), vec!["PlantFloor".to_string()]);
    }

    /// Abort when the name is FREE: the pre-check passes (find → 404)
    /// and the upload fires with `overwrite=false`.
    #[tokio::test]
    async fn import_abort_when_free_uploads_without_overwrite() {
        let rig = ProjectsRig {
            absent: true,
            ..Default::default()
        };
        let result = super::project_import(
            &rig,
            "fresh",
            ProjectsRig::zip_fixture(),
            super::CollisionPolicy::Abort,
        )
        .await
        .expect("free name imports");
        assert_eq!(result.name, "fresh");
        assert_eq!(result.collision_policy, "abort");
        assert_eq!(result.bytes, ProjectsRig::zip_fixture().len());
        assert_eq!(
            result.scope,
            super::ExportScope::new(),
            "import carries the SAME scope consts as export"
        );
        assert_eq!(
            *rig.imports.lock().unwrap(),
            vec![("fresh".to_string(), ProjectsRig::zip_fixture().len(), false)]
        );
    }

    /// Overwrite: NO pre-check (the server is the authority) — zero
    /// find calls — and the upload fires with `overwrite=true`.
    #[tokio::test]
    async fn import_overwrite_skips_pre_check_and_uploads() {
        let rig = ProjectsRig::default(); // find would answer Ok; it must not be asked
        let result = super::project_import(
            &rig,
            "PlantFloor",
            ProjectsRig::zip_fixture(),
            super::CollisionPolicy::Overwrite,
        )
        .await
        .expect("overwrite imports without a pre-check");
        assert_eq!(result.collision_policy, "overwrite");
        assert!(
            rig.finds.lock().unwrap().is_empty(),
            "overwrite performs ZERO pre-check calls"
        );
        assert_eq!(
            *rig.imports.lock().unwrap(),
            vec![(
                "PlantFloor".to_string(),
                ProjectsRig::zip_fixture().len(),
                true
            )]
        );
    }

    /// Scope arrays are DATA (roadmap criterion 4): tag-providers sit
    /// under excludes, and the serialized shape is the two-key object
    /// agents key off.
    #[test]
    fn export_scope_arrays_are_data() {
        assert!(
            super::EXPORT_EXCLUDES.contains(&"tag-providers"),
            "the headline exclusion (tags are gateway config, not project export)"
        );
        assert!(super::EXPORT_EXCLUDES.contains(&"tags"));
        assert!(super::EXPORT_EXCLUDES.contains(&"udts"));
        assert!(super::EXPORT_INCLUDES.contains(&"views"));
        assert!(super::EXPORT_INCLUDES.contains(&"scripts"));
        assert!(super::EXPORT_INCLUDES.contains(&"named-queries"));
        let json = serde_json::to_value(super::ExportScope::new()).expect("scope serializes");
        assert_eq!(
            json["includes"]
                .as_array()
                .expect("includes is an array")
                .len(),
            super::EXPORT_INCLUDES.len()
        );
        assert_eq!(
            json["excludes"][0], "tag-providers",
            "declaration order is the agent-visible order"
        );
    }

    /// Export with `-o`: the bytes land at exactly the given path and
    /// the result carries file/bytes/scope.
    #[tokio::test]
    async fn export_to_explicit_path_streams_and_reports() {
        let rig = ProjectsRig::default();
        let dir = tempfile::tempdir().expect("tempdir");
        let out = dir.path().join("proj.zip");
        let result = super::project_export(&rig, "My Proj", Some(&out))
            .await
            .expect("export");
        assert_eq!(result.project, "My Proj");
        assert_eq!(result.file, out.display().to_string());
        assert_eq!(result.bytes as usize, ProjectsRig::zip_fixture().len());
        assert_eq!(
            std::fs::read(&out).expect("file written"),
            ProjectsRig::zip_fixture(),
            "the fixture landed byte-for-byte"
        );
        assert_eq!(result.scope, super::ExportScope::new());
        assert_eq!(*rig.exports.lock().unwrap(), vec!["My Proj".to_string()]);
    }

    /// Default-naming hygiene: a disposition basename is stripped to
    /// its final component (`.`/`..`/empty refuse → the caller falls
    /// back), and the `<name>.zip` fallback neutralizes separators.
    #[test]
    fn sanitize_basename_strips_path_components() {
        assert_eq!(
            super::sanitize_basename("MyProj-export.zip"),
            Some("MyProj-export.zip".to_string())
        );
        assert_eq!(
            super::sanitize_basename("../../etc/passwd"),
            Some("passwd".to_string()),
            "path components never survive"
        );
        assert_eq!(
            super::sanitize_basename(r"..\..\win\evil.zip"),
            Some("evil.zip".to_string())
        );
        assert_eq!(super::sanitize_basename(".."), None);
        assert_eq!(super::sanitize_basename("."), None);
        assert_eq!(super::sanitize_basename("   "), None);
        assert_eq!(super::safe_fallback_stem("a/b\\c"), "a_b_c");
    }

    /// THE same-profile refusal (07-01): diffing a profile against
    /// itself is usage-class (exit 2 `invalid_input`) BEFORE any
    /// export fires — zero network work on the refused call.
    #[tokio::test]
    async fn project_diff_same_profile_refuses_before_any_export() {
        let rig = ProjectsRig::default();
        let err = super::project_diff(&rig, &rig, "p", "dev", "dev")
            .await
            .expect_err("the same-profile refusal");
        assert_eq!(err.exit_code(), 2);
        assert_eq!(err.code(), "invalid_input");
        assert!(
            rig.exports.lock().unwrap().is_empty(),
            "zero exports — the refusal leads"
        );
    }

    /// THE selection-less sync refusal (07-01): no `--resource` and
    /// no `--all-changed` is usage-class exit 2 before any export.
    #[tokio::test]
    async fn project_sync_selection_less_refuses_before_any_export() {
        let rig = ProjectsRig::default();
        let err = super::project_sync(
            &rig,
            &rig,
            "p",
            &super::SyncSelection::default(),
            false,
            "a",
            "b",
        )
        .await
        .expect_err("the selection-less refusal");
        assert_eq!(err.exit_code(), 2);
        assert_eq!(err.code(), "invalid_input");
        assert!(rig.exports.lock().unwrap().is_empty());
    }
}
