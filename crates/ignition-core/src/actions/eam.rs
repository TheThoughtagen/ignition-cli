//! EAM task actions (07-02, BKUP-02) — the read-heavy surface with
//! guarded writes, now carrying the full write LIFECYCLE (10-03).
//! Reads: run history (the runtime seam — the controller state gate
//! classifies honestly) and task definitions (the config-resource
//! seam — available on stock gateways). Writes: create (the guard
//! ladder), suspend/resume/cancel (find-first lifecycle verbs),
//! modify (full-record RMW) and delete (signature-keyed) — every
//! verb preceded by the blast-radius preview composer
//! ([`build_blast_radius`]) when the caller needs the pre-flight.
//!
//! Two-layer naming (LOCKED): the CLIENT models are wire-faithful;
//! HERE the agent-stable summary re-exposes under unit-explicit keys
//! (`name`/`task_type`/`schedule_mode`/`current_state`), and history
//! items pass through VERBATIM (the gateway's own camelCase keys —
//! execution outcomes are DATA: a `Failed` level with GNET
//! not-connected detail is an exit-0 read, never hidden, research
//! Pitfall 3).

use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::client::GatewayApi;
use crate::client::eam::{
    DeleteOutcome, EamHistoryItem, EamScheduledTask, EamTaskRecord, ModifyOutcome, ResourceChange,
};
use crate::error::CoreError;

/// The planner-locked create ladder's verdict (07-02 Task 3) — a
/// PURE function over `(task_type, schedule_mode)` so main.rs
/// (pre-resolution, zero network) and the TUI (Confirm gating) and
/// the action (authoritative re-check) all classify IDENTICALLY.
///
/// | verdict | meaning |
/// |---|---|
/// | `Unguarded` | `eam_backup` + OnDemand — fires only when forced, never mutates targets autonomously |
/// | `NeedsYes` | mutating types (restart/send*/licenses) OR any non-OnDemand schedule (arms autonomous actions) |
/// | `Refused` | `eam_restoreBackup`/`eam_installModules`/`eam_remoteUpgrade` — fleet-destructive, EXT-03 (v2) scope |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskCreateVerdict {
    /// No `--yes` needed (OnDemand backup).
    Unguarded,
    /// `require_confirmation` must fire.
    NeedsYes,
    /// Outright refusal (`EamTaskTypeRefused`).
    Refused,
}

/// The openapi taxonomy's REFUSED set — fleet-destructive types the
/// CLI refuses outright (push backups/modules/upgrades to every
/// agent target).
const REFUSED_TYPES: [&str; 3] = [
    "eam_restoreBackup",
    "eam_installModules",
    "eam_remoteUpgrade",
];

/// The taxonomy's mutating-but-allowed set — they act on target
/// agents when dispatched, so their DEFINITIONS need `--yes`.
const MUTATING_TYPES: [&str; 7] = [
    "eam_restart",
    "eam_sendProject",
    "eam_sendResource",
    "eam_sendTags",
    "eam_activateLicense",
    "eam_updateLicense",
    "eam_unactivateLicense",
];

/// THE guard ladder (pure): refused types first (highest rung);
/// then any non-OnDemand schedule (arms autonomous actions); then
/// the mutating type set; `eam_backup` + OnDemand lands unguarded.
/// An UNKNOWN type classifies `NeedsYes` — fail-safe (a `--yes`
/// costs nothing; an unrecognized fleet verb firing unguarded could
/// cost plenty; the server's own validation remains the backstop).
pub fn task_create_guard(task_type: &str, schedule_mode: &str) -> TaskCreateVerdict {
    if REFUSED_TYPES.contains(&task_type) {
        return TaskCreateVerdict::Refused;
    }
    if !schedule_mode.eq("OnDemand") {
        return TaskCreateVerdict::NeedsYes;
    }
    if MUTATING_TYPES.contains(&task_type) || task_type != "eam_backup" {
        return TaskCreateVerdict::NeedsYes;
    }
    TaskCreateVerdict::Unguarded
}

/// `ign eam task new` output model — all keys always.
#[derive(Debug, Serialize)]
pub struct EamTaskCreateResult {
    /// The created definition's name.
    pub name: String,
    /// The `profile.type` token.
    pub task_type: String,
    /// The `profile.scheduleMode` token.
    pub schedule_mode: String,
    /// The composed definition body that rode the array POST
    /// (verbatim — the agent's read-back of what was created).
    pub definition: Value,
}

/// `ign eam task force` output model — all keys always.
#[derive(Debug, Serialize)]
pub struct EamTaskForceResult {
    /// The dispatched task's name.
    pub task: String,
    /// The owner the force POST targeted (from the healthcheck's
    /// `scheduledTaskState.details.owner`, fallback `"eam"`).
    pub owner: String,
    /// Always `true` on this shape — the 2xx IS dispatch acceptance
    /// (execution outcomes land in history as data).
    pub dispatched: bool,
    /// The newest matching history entry after dispatch (null when
    /// none is visible yet) — its `level`/`detail` honestly surface
    /// GNET-not-connected / trial-expired outcomes.
    pub history: Option<EamHistoryItem>,
    /// The composed blast-radius preview (10-03, EAMW-04: force
    /// composes with the preview surface) — the same pre-flight the
    /// CLI's confirmation prompt renders: targets, pending
    /// executions, the factual controller impact. Additive field;
    /// the serde shape stays agent-stable (all keys always).
    pub preview: BlastRadiusPreview,
}

/// One `--setting K=V` parsed with scalar auto-typing (the 05-04
/// tags-write `--value` precedent): a value that parses cleanly as
/// bool (`true`/`false`) or integer serializes as a JSON bool /
/// number; anything else stays a string. Arrays/objects are OUT of
/// scope for K=V (the `--definition` path owns them).
pub fn parse_setting(raw: &str) -> Result<(String, Value), CoreError> {
    let Some((key, value)) = raw.split_once('=') else {
        return Err(CoreError::InvalidInput {
            reason: format!(
                "--setting expects K=V (got {raw:?}) — a value that parses as \
                 bool/int rides typed, anything else stays a string; arrays and \
                 objects need --definition <PATH>"
            ),
        });
    };
    if key.is_empty() || value.is_empty() {
        return Err(CoreError::InvalidInput {
            reason: format!("--setting expects non-empty K and V (got {raw:?})"),
        });
    }
    Ok((key.to_string(), auto_type(value)))
}

/// The scalar auto-typing rule: `true`/`false` → JSON bool; a clean
/// i64 parse → JSON number; anything else → the string verbatim.
fn auto_type(value: &str) -> Value {
    if value == "true" {
        return Value::Bool(true);
    }
    if value == "false" {
        return Value::Bool(false);
    }
    if let Ok(int) = value.parse::<i64>() {
        return Value::Number(int.into());
    }
    Value::String(value.to_string())
}

/// Deep-merge `overlay` onto `base` (objects merge recursively —
/// base keys win only when the overlay carries nothing at that
/// path; arrays and scalars REPLACE, never merge — the documented
/// settings-merge semantics for `--definition`).
fn deep_merge(base: &mut Value, overlay: &Value) {
    match (base, overlay) {
        (Value::Object(base_map), Value::Object(overlay_map)) => {
            for (key, overlay_value) in overlay_map {
                match base_map.get_mut(key) {
                    Some(base_value @ Value::Object(_)) if overlay_value.is_object() => {
                        deep_merge(base_value, overlay_value);
                    }
                    _ => {
                        base_map.insert(key.clone(), overlay_value.clone());
                    }
                }
            }
        }
        (base, overlay) => *base = overlay.clone(),
    }
}

/// Compose the `eam task new` definition body (pure — the
/// unit-testable core of [`eam_task_create`]). The live 8.3.3
/// controller requires the profile/settings SPLIT (captured in
/// `.planning/debug/eam-working-definition.json`; the pre-split body
/// 422'd with "Settings cannot be null"):
///
/// - `config.profile` = `{type, scheduleMode}` ONLY (`isSuspended` is
///   server-owned — never sent on create);
/// - `config.settings` = `{targetGateways, targetGroups}` + every
///   `--setting K=V` (auto-typed scalars) with the `--definition`
///   overlay deep-merged over the composed SETTINGS object (objects
///   merge, arrays/scalars replace).
///
/// Zero `--target` values default to the controller itself —
/// `targetGateways: ["_controller"]`, the live-captured zero-config
/// default on a controller-mode gateway; explicit targets replace it
/// wholesale. `targetGroups` is always `[]` (no `--group` flag
/// exists).
fn compose_task_definition(
    name: &str,
    task_type: &str,
    targets: &[String],
    settings: &[String],
    definition: Option<&Value>,
    schedule_mode: &str,
) -> Result<Value, CoreError> {
    let mut profile = Map::new();
    profile.insert("type".to_string(), Value::String(task_type.to_string()));
    profile.insert(
        "scheduleMode".to_string(),
        Value::String(schedule_mode.to_string()),
    );

    let mut composed_settings = Map::new();
    composed_settings.insert(
        "targetGateways".to_string(),
        if targets.is_empty() {
            Value::Array(vec![Value::String("_controller".to_string())])
        } else {
            Value::Array(targets.iter().map(|t| Value::String(t.clone())).collect())
        },
    );
    composed_settings.insert("targetGroups".to_string(), Value::Array(vec![]));
    for raw in settings {
        let (key, value) = parse_setting(raw)?;
        composed_settings.insert(key, value);
    }
    let mut settings_value = Value::Object(composed_settings);
    if let Some(overlay) = definition {
        deep_merge(&mut settings_value, overlay);
    }

    Ok(serde_json::json!({
        "name": name,
        "config": {
            "profile": Value::Object(profile),
            "settings": settings_value,
        },
    }))
}

/// `ign eam task new` — compose the definition, run the ladder's
/// authoritative re-check, POST the array body.
///
/// Composition (the live 8.3.3 `config.settings` shape — see
/// [`compose_task_definition`]): `{name, config: {profile: {type,
/// scheduleMode}, settings: {targetGateways, targetGroups, ...--setting
/// K=V}}}`; a `--definition` file's top-level object deep-merges over
/// the composed SETTINGS (the typed/array path — mutually exclusive
/// with `--setting` at clap). The refusal ladder runs AGAIN here
/// (main.rs already guarded by verdict; the re-check keeps the pure fn
/// authoritative in core — the double-check is cheap).
pub async fn eam_task_create(
    api: &dyn GatewayApi,
    name: &str,
    task_type: &str,
    targets: &[String],
    settings: &[String],
    definition: Option<&Value>,
    schedule_mode: &str,
) -> Result<EamTaskCreateResult, CoreError> {
    // The ladder is authoritative HERE (the CLI's pre-resolution
    // guard is the fast path; this is the correctness path).
    if let TaskCreateVerdict::Refused = task_create_guard(task_type, schedule_mode) {
        return Err(CoreError::EamTaskTypeRefused {
            task_type: task_type.to_string(),
        });
    }

    let composed = compose_task_definition(
        name,
        task_type,
        targets,
        settings,
        definition,
        schedule_mode,
    )?;
    api.eam_task_create(&composed).await?;
    Ok(EamTaskCreateResult {
        name: name.to_string(),
        task_type: task_type.to_string(),
        schedule_mode: schedule_mode.to_string(),
        definition: composed,
    })
}

/// `ign eam task force` — the preview IS the pre-flight (EAMW-04):
/// [`build_blast_radius`] runs the find (owner resolution via the
/// healthcheck's `scheduledTaskState.details.owner`, live-captured
/// fallback `"eam"`) AND the pending reads the confirmation prompt
/// renders → force POST (2xx = dispatched) → history re-read (the
/// newest matching entry, `level`/`detail` as data). Correctness
/// over latency (the 05-04 precondition precedent): the extra
/// scheduled reads are the confirmation surface's data source.
pub async fn eam_task_force(
    api: &dyn GatewayApi,
    name: &str,
) -> Result<EamTaskForceResult, CoreError> {
    let preview = build_blast_radius(api, "force", name).await?;
    let owner = preview.owner.clone().unwrap_or_else(|| "eam".to_string());

    api.eam_task_force(&owner, name).await?;

    let history = api
        .eam_task_history(Some(20), Some(name))
        .await
        .ok()
        .and_then(|page| {
            page.items.into_iter().find(|item| {
                let forced = format!("{name} (forced)");
                item.task_name == name || item.task_name == forced
            })
        });

    Ok(EamTaskForceResult {
        task: name.to_string(),
        owner,
        dispatched: true,
        history,
        preview,
    })
}

// ---- 10-03 Task 1: the runtime lifecycle (suspend/resume/cancel) ----
//
// Capture-locked per 10-LIVE-CAPTURES.md (both rigs, 8.3.3 + 8.3.6):
// the verbs 204 on success and PERSIST `config.profile.isSuspended`
// (Decision 1); failures are 500s with indistinguishable Jetty HTML
// (§7) — so find-before-write is the ONLY honest name validation,
// and each action carries an authoritative re-check of verb
// applicability BEFORE the write fires (the task_create double-check
// pattern: main.rs/TUI pre-resolve on the same pure fns).

/// `ign eam task suspend|resume|cancel` output model — all keys
/// always (the agent-stable shape; nulls are honest absence).
#[derive(Debug, Serialize)]
pub struct EamLifecycleResult {
    /// The lifecycle target's name.
    pub task: String,
    /// What ran: `"suspended"`, `"resumed"`, or `"cancelled"`.
    pub action: String,
    /// The find healthcheck's `currentState` BEFORE the write (the
    /// captured vocabulary rides [`crate::client::eam::EAM_CURRENT_STATES`]).
    pub previous_state: Option<String>,
    /// The post-write find read-back of `config.profile.isSuspended`
    /// — the capture-locked persistence proof (suspend ⇒ `true`,
    /// resume ⇒ `false`, 10-LIVE-CAPTURES Decision 1). `null` for
    /// cancel (no definition flag rides that verb).
    pub config_suspended: Option<bool>,
    /// cancel only: the POST-write pending read filtered to the task
    /// — `null` when nothing was pending (the honest no-op) or the
    /// pending row is gone (the cancel landed). Suspend/resume leave
    /// it `null` (they target the scheduler trigger, not a row).
    pub pending: Option<EamScheduledTask>,
    /// Whether the lifecycle POST actually rode the wire. Carries the
    /// cancel no-op honesty (`fired: false` ⇔ the no-op result's
    /// "`cancelled: false`") — a write we declined to fire is
    /// reported, never disguised as a success.
    pub fired: bool,
    /// Why nothing fired: `"no pending execution"` on the cancel
    /// no-op, or the gateway's own `canCancel=false` report on an
    /// uncancellable row. Always `null` on a fired write.
    pub reason: Option<String>,
}

/// The lifecycle name precheck (pure, ZERO network — the
/// guard-ladder three-place rule: main.rs pre-resolution, the TUI's
/// Confirm gating, and the action's authoritative re-check all call
/// THIS fn). Name-empty/whitespace refusals ONLY — every other
/// applicability question needs the find (the action's Tier-3 job),
/// and inventing state rules the captures don't support is exactly
/// the wire-dishonesty pitfall.
pub fn lifecycle_precheck(action: &str, task_name: &str) -> Result<(), CoreError> {
    if task_name.trim().is_empty() {
        return Err(CoreError::InvalidInput {
            reason: format!("eam task {action}: the task name must not be empty/whitespace"),
        });
    }
    Ok(())
}

/// The suspend re-check (pure): the capture-locked refusal branch.
/// A suspended task's trigger is parked (`nextScheduled: "N/A"`,
/// `currentState: "Suspended"` — 10-LIVE-CAPTURES §1c), so the
/// gateway's own answer to the POST is the INDISTINGUISHABLE 500
/// "Task could not be suspended" (§1a/§7 — same answer an
/// unknown-name or OnDemand suspend gets). When find PROVES
/// `isSuspended: true`, refuse pre-write exit 2 naming the task —
/// a distinct, actionable message instead of the ambiguous page.
/// Anything else (`false`, absent, unparseable) fires — no invented
/// state machine.
pub fn suspend_recheck(record: &EamTaskRecord) -> Result<(), CoreError> {
    let suspended = record
        .config
        .get("profile")
        .and_then(|profile| profile.get("isSuspended"))
        .and_then(Value::as_bool);
    if suspended == Some(true) {
        return Err(CoreError::InvalidInput {
            reason: format!(
                "task {:?} is already suspended (config.profile.isSuspended=true) — \
                 nothing to suspend; `eam task resume` it first",
                record.name
            ),
        });
    }
    Ok(())
}

/// The cancel decision (pure) over the task's pending row (from the
/// scheduled reads, filtered to the name). Every branch mirrors a
/// captured fact — nothing here is an invented state machine:
///
/// - [`CancelDecision::Fire`] — a pending row with `canCancel: true`
///   (the captured `Scheduled` cell, §2): the gateway says the
///   cancel CAN fire.
/// - [`CancelDecision::NoPending`] — no row: the gateway's own
///   answer to cancel-with-nothing-pending is a SILENT 204 (§7) —
///   mirrored as an honest no-op result WITHOUT the wire round trip
///   (`fired: false`, reason `"no pending execution"`).
/// - [`CancelDecision::NotPermitted`] — a row whose `canCancel` is
///   `false` (an unobserved cell — Running rows never materialized
///   on the capture rigs): the gateway's own capability flag says
///   no; the flag is reported verbatim, the POST is not fired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelDecision {
    /// Fire the cancel POST (a pending, cancellable execution exists).
    Fire,
    /// Honest no-op — nothing pending (`fired: false`).
    NoPending,
    /// Honest no-op — the row's `canCancel` is `false` (`fired: false`).
    NotPermitted,
}

/// The pure decision read (see [`CancelDecision`]).
pub fn cancel_decision(pending: Option<&EamScheduledTask>) -> CancelDecision {
    match pending {
        None => CancelDecision::NoPending,
        Some(row) if row.can_cancel => CancelDecision::Fire,
        Some(_) => CancelDecision::NotPermitted,
    }
}

/// The find healthcheck's `currentState` (pure projection).
fn current_state_of(record: &EamTaskRecord) -> Option<String> {
    record
        .scheduled_task_state
        .as_ref()
        .and_then(|state| state.get("currentState"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// The definition flag `config.profile.isSuspended` (pure projection).
fn is_suspended_of(record: &EamTaskRecord) -> Option<bool> {
    record
        .config
        .get("profile")
        .and_then(|profile| profile.get("isSuspended"))
        .and_then(Value::as_bool)
}

/// The task's FIRST pending row (both literal segments — see
/// [`pending_rows_for`] for the both-segments discipline). The
/// cancel action's decision input.
async fn pending_row_for(
    api: &dyn GatewayApi,
    name: &str,
) -> Result<Option<EamScheduledTask>, CoreError> {
    Ok(pending_rows_for(api, name).await?.into_iter().next())
}

/// `ign eam task suspend` — find (the unknown-name 500 is
/// indistinguishable on this seam, §7 — the config-resource find's
/// `not_found` is the honest refusal) → the authoritative
/// already-suspended re-check → the POST → the find read-back that
/// reports the capture-locked `isSuspended` persistence (Decision 1).
/// A suspended task refuses exit 2 BEFORE the wire; an OnDemand or
/// untriggered task FIRES and reports the gateway's verbatim 500
/// (wire honesty over cleverness — no invented rules).
pub async fn eam_task_suspend(
    api: &dyn GatewayApi,
    name: &str,
) -> Result<EamLifecycleResult, CoreError> {
    lifecycle_precheck("suspend", name)?;
    let record = api.eam_task_find(name).await?;
    let previous_state = current_state_of(&record);
    suspend_recheck(&record)?;
    api.eam_task_suspend(name).await?;
    // Decision 1: the runtime verbs PERSIST the flag into the
    // definition — the read-back is the proof the result reports.
    let readback = api.eam_task_find(name).await?;
    Ok(EamLifecycleResult {
        task: name.to_string(),
        action: "suspended".to_string(),
        previous_state,
        config_suspended: is_suspended_of(&readback),
        pending: None,
        fired: true,
        reason: None,
    })
}

/// `ign eam task resume` — the suspend inverse with NO refusal
/// re-check: the capture answers resume of a never-suspended task
/// with a silent 204 (§1b) — the gateway accepts the verb regardless,
/// so we fire and report (wire honesty over cleverness). The
/// read-back reports the persisted flag honestly whatever it is.
pub async fn eam_task_resume(
    api: &dyn GatewayApi,
    name: &str,
) -> Result<EamLifecycleResult, CoreError> {
    lifecycle_precheck("resume", name)?;
    let record = api.eam_task_find(name).await?;
    let previous_state = current_state_of(&record);
    api.eam_task_resume(name).await?;
    let readback = api.eam_task_find(name).await?;
    Ok(EamLifecycleResult {
        task: name.to_string(),
        action: "resumed".to_string(),
        previous_state,
        config_suspended: is_suspended_of(&readback),
        pending: None,
        fired: true,
        reason: None,
    })
}

/// `ign eam task cancel` — find (unknown names answer a SILENT 204
/// on this seam, §7 — cancel can never be a name-validation tool;
/// the config-resource find's `not_found` is) → the pending reads →
/// the pure [`cancel_decision`] → fire or honest no-op → the
/// post-write pending read (`pending` reports what REMAINS).
pub async fn eam_task_cancel(
    api: &dyn GatewayApi,
    name: &str,
) -> Result<EamLifecycleResult, CoreError> {
    lifecycle_precheck("cancel", name)?;
    let record = api.eam_task_find(name).await?;
    let previous_state = current_state_of(&record);
    let pending = pending_row_for(api, name).await?;
    match cancel_decision(pending.as_ref()) {
        CancelDecision::Fire => {
            api.eam_task_cancel(name).await?;
            let after = pending_row_for(api, name).await?;
            Ok(EamLifecycleResult {
                task: name.to_string(),
                action: "cancelled".to_string(),
                previous_state,
                config_suspended: None,
                pending: after,
                fired: true,
                reason: None,
            })
        }
        CancelDecision::NoPending => Ok(EamLifecycleResult {
            task: name.to_string(),
            action: "cancelled".to_string(),
            previous_state,
            config_suspended: None,
            pending: None,
            fired: false,
            reason: Some("no pending execution".to_string()),
        }),
        CancelDecision::NotPermitted => Ok(EamLifecycleResult {
            task: name.to_string(),
            action: "cancelled".to_string(),
            previous_state,
            config_suspended: None,
            pending,
            fired: false,
            reason: Some(
                "the gateway reports canCancel=false for the pending execution".to_string(),
            ),
        }),
    }
}

// ---- 10-03 Task 2: config mutations (modify + delete) ----
//
// Both are capture-locked: the PUT is a FULL-RECORD echo-modify
// carrying the ORIGINAL signature (§6a — omitting config.settings is
// the 422 trap, §6b/Decision 6), and the DELETE is signature-keyed
// with the gateway-side confirm policy (§3 + Decision 3: NO confirm
// by default; the unobserved confirm-demand shape retries ONCE with
// confirm=true — never hard-coded).

/// A targeted modify request (the keys the caller wants changed —
/// everything else rides the found record untouched). Builder-ish:
/// start from [`TaskChange::default`], set what applies.
///
/// **Deliberately ABSENT, per wire honesty:**
///
/// - **rename** — capture §5/Decision 5: a PUT with a changed `name`
///   and the original signature answers **404 empty** (the modify
///   route resolves the resource BY the body's name and finds
///   nothing — no rename, no create, no error body). A rename verb
///   must compose create-new + delete-old; that composite is the
///   CLI tier's (10-04) documented workflow, NEVER this PUT.
/// - **suspend flag** — capture §6a proved only a full-record echo
///   (whose `isSuspended` happened to be `false`) landing; a
///   PUT-DRIVEN `isSuspended` mutation is unproven. The
///   capture-locked path for the flag is the suspend/resume VERBS
///   (Decision 1) — so `TaskChange` carries no flag field at all
///   rather than faking one.
#[derive(Debug, Clone, Default)]
pub struct TaskChange {
    /// Flip the record's top-level `enabled` key.
    pub enabled: Option<bool>,
    /// Rewrite the record's top-level `description` key.
    pub description: Option<String>,
    /// Rewrite `config.profile.scheduleMode`. NOTE: scheduleDetails
    /// rides ONLY via the found record's clone — schedule-mode
    /// changes that also need a new cron/delay string are out of
    /// this change's scope (the capture vocabulary: §12 — the wire
    /// accepts the mode change; an unsupported pairing lands as
    /// gateway-side state, visible in the read-back as data).
    pub schedule_mode: Option<String>,
    /// Deep-merge over the found `config.settings` (objects merge
    /// recursively, arrays/scalars replace — the documented settings
    /// semantics). The settings OBJECT always rides (the 422 trap).
    pub settings_overlay: Option<Value>,
}

/// The `ign eam task modify` output model — all keys always.
#[derive(Debug, Serialize)]
pub struct EamModifyResult {
    /// The post-write name (rename is NOT supported — always the
    /// name as found).
    pub task: String,
    /// The dotted key paths the change touched: `"enabled"`,
    /// `"description"`, `"config.profile.scheduleMode"`,
    /// `"config.settings"`.
    pub changed: Vec<String>,
    /// The verbatim PUT body (the single record — the client wraps
    /// it in the one-element array envelope): the agent's read-back
    /// of exactly what was sent, signature + collection included.
    pub definition: Value,
    /// The captured 200 outcome
    /// ([`ModifyOutcome`] — `changes[].newSignature` is
    /// authoritative for the NEXT mutation, §6a; its `problem` rides
    /// verbatim on the refusal shapes). `null` when a 2xx carried no
    /// body (the lenient client Option).
    pub put_outcome: Option<ModifyOutcome>,
    /// The post-PUT find, serialized — the echo semantics make the
    /// read-back meaningful (§6a: `newSignature` == read-back
    /// signature, mutated keys landed, unknown round-trip keys
    /// preserved). `null` when the read-back itself failed (the PUT
    /// already succeeded — a read blip must not mask a landed write).
    pub readback: Value,
}

/// The `ign eam task delete` output model — all keys always.
#[derive(Debug, Serialize)]
pub struct EamDeleteResult {
    /// The deleted (or refused) task's name.
    pub task: String,
    /// Whether the gateway answered the captured success shape
    /// (`success: true`, §3b).
    pub deleted: bool,
    /// The captured `changes[]` verbatim (the deleted resource's
    /// final signature rides `changes[].newSignature`, §9).
    pub changes: Vec<ResourceChange>,
    /// Agent/resource names the delete touched: the `changes[]`
    /// names plus any string-shaped `references` entries (the
    /// affected-resources element shape is UNOBSERVED — §3d — so
    /// the extraction is lenient: JSON strings ride; objects
    /// contribute a `name` key when present).
    pub affected: Vec<String>,
}

/// Apply the targeted mutations to the FULL-record clone (pure —
/// the unit-testable core of [`eam_task_modify`]). Returns the
/// dotted key paths touched. ONLY the targeted keys change:
/// `config.settings`, `signature`, `collection`, and every
/// unknown round-trip key ride the clone byte-for-byte (the
/// never-compose-from-scratch invariant — compose_task_definition
/// composes CREATE bodies and must never feed a modify).
fn apply_task_change(body: &mut Value, change: &TaskChange) -> Vec<String> {
    let mut changed = Vec::new();
    let TaskChange {
        enabled,
        description,
        schedule_mode,
        settings_overlay,
    } = change;
    if let Some(enabled) = enabled {
        *slot(body, "enabled") = Value::Bool(*enabled);
        changed.push("enabled".to_string());
    }
    if let Some(description) = description {
        *slot(body, "description") = Value::String(description.clone());
        changed.push("description".to_string());
    }
    if let Some(schedule_mode) = schedule_mode {
        let profile = slot(slot(body, "config"), "profile");
        if !profile.is_object() {
            *profile = Value::Object(Map::new());
        }
        *slot(profile, "scheduleMode") = Value::String(schedule_mode.clone());
        changed.push("config.profile.scheduleMode".to_string());
    }
    if let Some(overlay) = settings_overlay {
        let settings = slot(slot(body, "config"), "settings");
        deep_merge(settings, overlay);
        changed.push("config.settings".to_string());
    }
    changed
}

/// A mutable slot helper: ensures `parent` is an object and hands
/// back the (created-if-absent) entry for `key`.
fn slot<'a>(parent: &'a mut Value, key: &str) -> &'a mut Value {
    if !parent.is_object() {
        *parent = Value::Object(Map::new());
    }
    parent
        .as_object_mut()
        .expect("just ensured an object")
        .entry(key.to_string())
        .or_insert(Value::Null)
}

/// Serialize a record round-trip (the full-record clone — every key
/// the find answered, unknown ones included, rides the Value).
/// The model hoists the runtime healthcheck under a top-level
/// `scheduledTaskState` key; when the find answer carried the state
/// under `healthchecks` (the captured §0 envelope shape) that field
/// is `None` and would serialize a null PLACEHOLDER — dropped here,
/// so the clone is the find body + nothing (the PUT must never
/// carry a key the wire never answered).
fn record_to_value(record: &EamTaskRecord) -> Result<Value, CoreError> {
    let mut value = serde_json::to_value(record).map_err(|err| {
        CoreError::Internal(format!(
            "task record failed to serialize for the clone: {err}"
        ))
    })?;
    if value.get("scheduledTaskState") == Some(&Value::Null)
        && let Some(map) = value.as_object_mut()
    {
        map.remove("scheduledTaskState");
    }
    Ok(value)
}

/// The stale-signature diagnostic (the client/eam.rs FINDING +
/// capture Decision 4, classified HERE — the client stays
/// classification-free): a signature mismatch answers HTTP 500 with
/// a JSON `problem` whose stable `signature mismatch` substring
/// never reaches this layer (the classifier's Internal fallback
/// drops non-HTML bodies). Classify on EVIDENCE instead: a
/// post-failure find whose signature differs from the one we sent
/// PROVES a concurrent write (the capture also proves mismatches
/// leave the resource untouched — §3a/§4 read-backs) — the
/// client-fixable conflict rides exit 2 (re-run to apply against
/// the current signature). No evidence → the original error
/// propagates verbatim (no invented claims). No new slugs — the
/// existing taxonomy exclusively.
async fn reclassify_stale_signature(
    api: &dyn GatewayApi,
    name: &str,
    sent_signature: &str,
    err: CoreError,
) -> CoreError {
    let stale = matches!(
        api.eam_task_find(name).await,
        Ok(fresh) if fresh.signature.as_deref().is_some_and(|sig| sig != sent_signature)
    );
    if stale {
        return CoreError::InvalidInput {
            reason: format!(
                "definition {name:?} changed concurrently (signature mismatch on write) — \
                 the gateway answers mismatches with a 500 and leaves the resource untouched; \
                 re-run to apply against the current signature"
            ),
        };
    }
    err
}

/// `ign eam task modify` — the FULL-RECORD read-modify-write
/// (never compose-from-scratch: `config.settings: null` is the 422
/// trap, §6b/Decision 6 — the create-composer composes CREATE
/// bodies, not this): find (not_found honesty + the signature
/// source) → clone EVERY key find answered → apply ONLY the
/// targeted [`TaskChange`] keys → PUT the single-element array
/// carrying the ORIGINAL signature + collection → the post-PUT find
/// read-back (the echo semantics make it meaningful, §6a). An
/// all-`None` change refuses exit 2 pre-network (a no-op PUT would
/// still rotate the server-side signature).
pub async fn eam_task_modify(
    api: &dyn GatewayApi,
    name: &str,
    change: TaskChange,
) -> Result<EamModifyResult, CoreError> {
    lifecycle_precheck("modify", name)?;
    let TaskChange {
        enabled,
        description,
        schedule_mode,
        settings_overlay,
    } = &change;
    if enabled.is_none()
        && description.is_none()
        && schedule_mode.is_none()
        && settings_overlay.is_none()
    {
        return Err(CoreError::InvalidInput {
            reason: format!(
                "eam task modify {name:?}: no targeted keys — a modify must change \
                 something (enabled / description / schedule-mode / settings overlay)"
            ),
        });
    }

    let record = api.eam_task_find(name).await?;
    let signature = record
        .signature
        .clone()
        .ok_or_else(|| CoreError::InvalidInput {
            reason: format!(
                "the found record for {name:?} carries no mutation signature — modify \
             requires it (list-shape records don't carry one; re-find)"
            ),
        })?;
    let mut body = record_to_value(&record)?;
    let changed = apply_task_change(&mut body, &change);
    let definition = body.clone();

    let put_outcome = match api.eam_task_modify(&body).await {
        Ok(outcome) => outcome,
        Err(err) => {
            return Err(reclassify_stale_signature(api, name, &signature, err).await);
        }
    };

    let readback = match api.eam_task_find(name).await {
        Ok(fresh) => record_to_value(&fresh)?,
        Err(_) => Value::Null,
    };

    Ok(EamModifyResult {
        task: name.to_string(),
        changed,
        definition,
        put_outcome,
        readback,
    })
}

/// The lenient affected-names extraction (pure): `changes[]` names
/// are capture-proven; `references` element shape is UNOBSERVED
/// (§3d) — JSON strings ride, objects contribute a `name` key when
/// present, everything else is skipped honestly. Deduped, wire
/// order preserved.
fn affected_resources(outcome: &DeleteOutcome) -> Vec<String> {
    let mut names: Vec<String> = outcome
        .changes
        .iter()
        .map(|change| change.name.clone())
        .collect();
    if let Some(references) = &outcome.references {
        for reference in references {
            match reference {
                Value::String(name) => names.push(name.clone()),
                Value::Object(map) => {
                    if let Some(Value::String(name)) = map.get("name") {
                        names.push(name.clone());
                    }
                }
                _ => {}
            }
        }
    }
    let mut seen = BTreeSet::new();
    names
        .into_iter()
        .filter(|name| seen.insert(name.clone()))
        .collect()
}

/// `ign eam task delete` — find first (the `not_found` honesty AND
/// the signature source — never ask the caller for it) → DELETE
/// with `?collection=core` and NO confirm (capture §3b/Decision 3:
/// a lone-resource delete succeeds without it; hard-coding
/// `confirm=true` would bypass a genuine multi-resource warning we
/// cannot yet see) → on the body-level `success: false` (the
/// UNOBSERVED confirm-demand shape, §3d) retry ONCE with
/// `confirm=true` — the CLI's `--yes` gate already ran before this
/// correctness-path action. Signature-mismatch 500s classify on
/// evidence ([`reclassify_stale_signature`]).
pub async fn eam_task_delete(
    api: &dyn GatewayApi,
    name: &str,
) -> Result<EamDeleteResult, CoreError> {
    lifecycle_precheck("delete", name)?;
    let record = api.eam_task_find(name).await?;
    let signature = record
        .signature
        .clone()
        .ok_or_else(|| CoreError::InvalidInput {
            reason: format!(
                "the found record for {name:?} carries no mutation signature — delete \
             is signature-keyed (list-shape records don't carry one; re-find)"
            ),
        })?;

    let outcome = match api.eam_task_delete(name, &signature, false).await {
        Ok(outcome) if outcome.success => outcome,
        Ok(demand) => {
            // The confirm-demand shape (success:false + the
            // affected-resources evidence in references/changes) —
            // the one sanctioned retry, Decision 3 as written.
            let _ = demand;
            match api.eam_task_delete(name, &signature, true).await {
                Ok(retry) => retry,
                Err(err) => {
                    return Err(reclassify_stale_signature(api, name, &signature, err).await);
                }
            }
        }
        Err(err) => {
            return Err(reclassify_stale_signature(api, name, &signature, err).await);
        }
    };

    Ok(EamDeleteResult {
        task: name.to_string(),
        deleted: outcome.success,
        changes: outcome.changes.clone(),
        affected: affected_resources(&outcome),
    })
}

// ---- 10-03 Task 3: the blast-radius preview ----
//
// The read-only pre-flight that feeds every guard prompt: ONE
// composer (find + both scheduled segments) behind the pure
// projection all three caller tiers (CLI refusal message, TUI
// Confirm body, action re-checks) render identically. History is
// deliberately EXCLUDED: 10-RESEARCH lists it as optional, the
// radius answers who/what/targets/pending — the guards consult
// nothing history carries, and the read would double the pre-flight
// traffic for data nobody renders pre-write.

/// The blast-radius preview — the composed pre-write facts (all keys
/// always). Unknown-task names refuse `not_found` at the composer:
/// the preview IS the pre-flight, so a bad name never reaches any
/// confirm prompt.
#[derive(Debug, Serialize)]
pub struct BlastRadiusPreview {
    /// The target task's name.
    pub task: String,
    /// `config.profile.type` (the token, e.g. `eam_backup`) — the
    /// CONFIG seam's vocabulary (the scheduled rows' `type` is a
    /// DIFFERENT, human-label vocabulary — never conflated).
    pub task_type: Option<String>,
    /// `config.profile.scheduleMode`.
    pub schedule_mode: Option<String>,
    /// The find healthcheck's `currentState`.
    pub state: Option<String>,
    /// The healthcheck's `details.owner` (the force verb's owner
    /// segment source; `"eam"` fallback lives at the caller).
    pub owner: Option<String>,
    /// `config.profile.isSuspended` — the definition flag the
    /// lifecycle verbs persist (Decision 1).
    pub config_suspended: Option<bool>,
    /// `config.settings.targetGateways` — the AGENTS the write
    /// touches; empty when the record names none (the controller
    /// itself is then the effective target, per the create
    /// composer's zero-config default — reported as empty here
    /// because the found record, not the composer, owns the list).
    pub target_gateways: Vec<String>,
    /// The task's pending executions (from BOTH literal scheduled
    /// segments, filtered to the name; each row carries the
    /// gateway-owned canPause/canResume/canCancel + taskState).
    pub pending_executions: Vec<EamScheduledTask>,
    /// One factual sentence: what the verb does to the task and how
    /// many agents it touches. No dramatization — the CLI's
    /// require_confirmation string and any future TUI body render
    /// THIS text ([`render_preview_line`]).
    pub controller_impact: String,
    /// The verb the preview was composed for (`suspend`/`resume`/
    /// `cancel`/`force`/`modify`/`delete`).
    pub verb: String,
}

/// The agent-count fragment (`1 agent` / `2 agents` / `0 agents`).
fn agents_fragment(count: usize) -> String {
    format!("{count} agent{}", if count == 1 { "" } else { "s" })
}

/// The per-verb factual impact sentence (pure). Names the task, the
/// profile type when known, and the agent count; the pending count
/// only where the verb targets executions. Capture-honest: nothing
/// here predicts outcomes (execution results are history DATA).
fn controller_impact(
    verb: &str,
    task: &str,
    task_type: Option<&str>,
    agents: usize,
    pending: usize,
) -> String {
    let type_note = task_type.map(|t| format!(" ({t})")).unwrap_or_default();
    let agents = agents_fragment(agents);
    match verb {
        "suspend" => format!(
            "suspends task {task}{type_note} — future scheduled dispatches to {agents} stop until resumed"
        ),
        "resume" => format!(
            "resumes task {task}{type_note} — scheduled dispatches to {agents} can fire again"
        ),
        "cancel" if pending > 0 => format!(
            "cancels the pending execution of task {task}{type_note} — {pending} queued dispatch{} to {agents}",
            if pending == 1 { "" } else { "es" }
        ),
        "cancel" => format!("task {task}{type_note} has no pending execution to cancel"),
        "force" => format!("dispatches task {task}{type_note} now to {agents}"),
        "modify" => format!(
            "rewrites the definition of task {task}{type_note} — dispatch behavior to {agents} follows the new body"
        ),
        "delete" => {
            format!("deletes task {task}{type_note} permanently — dispatches to {agents} stop")
        }
        other => format!("examines task {task}{type_note} for {other} — targets {agents}"),
    }
}

/// The pure projection: find record + filtered pending rows + verb
/// → the preview (the async composer's testable core).
fn compose_blast_radius(
    record: &EamTaskRecord,
    pending_executions: Vec<EamScheduledTask>,
    verb: &str,
) -> BlastRadiusPreview {
    // Filter to THIS task here (the pure fn owns the rule, so every
    // caller — and every test — provably drops other tasks' rows).
    let pending_executions: Vec<EamScheduledTask> = pending_executions
        .into_iter()
        .filter(|row| row.name == record.name)
        .collect();
    let profile = record.config.get("profile");
    let task_type = profile
        .and_then(|p| p.get("type"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let schedule_mode = profile
        .and_then(|p| p.get("scheduleMode"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let config_suspended = profile
        .and_then(|p| p.get("isSuspended"))
        .and_then(Value::as_bool);
    let state = current_state_of(record);
    let owner = record
        .scheduled_task_state
        .as_ref()
        .and_then(|s| s.get("details"))
        .and_then(|d| d.get("owner"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let target_gateways: Vec<String> = record
        .config
        .get("settings")
        .and_then(|settings| settings.get("targetGateways"))
        .and_then(Value::as_array)
        .map(|gateways| {
            gateways
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let controller_impact = controller_impact(
        verb,
        &record.name,
        task_type.as_deref(),
        target_gateways.len(),
        pending_executions.len(),
    );
    BlastRadiusPreview {
        task: record.name.clone(),
        task_type,
        schedule_mode,
        state,
        owner,
        config_suspended,
        target_gateways,
        pending_executions,
        controller_impact,
        verb: verb.to_string(),
    }
}

/// The task's pending rows from BOTH literal scheduled segments
/// (`false` then `true` — 10-LIVE-CAPTURES §2), filtered to the
/// task name, tolerating the quiet empty-list body (§8). BOTH
/// segments always read: a Running row lives only in the `true`
/// segment (uncapturable on the 10-01 rigs — Decision 2) and a
/// short-circuit would report "nothing pending" while an execution
/// is in flight — the one lie this composer must never tell.
async fn pending_rows_for(
    api: &dyn GatewayApi,
    name: &str,
) -> Result<Vec<EamScheduledTask>, CoreError> {
    let mut rows = Vec::new();
    for running in [false, true] {
        rows.extend(
            api.eam_tasks_scheduled(running)
                .await?
                .into_iter()
                .filter(|row| row.name == name),
        );
    }
    Ok(rows)
}

/// Compose the blast-radius preview (the read-only pre-flight):
/// find first — an unknown name refuses `not_found` HERE, before
/// any confirm prompt or write (capture Decision 7: find-before-
/// write is the only honest name validation on the lifecycle seam)
/// — then both scheduled segments. History deliberately excluded
/// (module doc). One composer, every guarded verb.
pub async fn build_blast_radius(
    api: &dyn GatewayApi,
    verb: &str,
    task_name: &str,
) -> Result<BlastRadiusPreview, CoreError> {
    let record = api.eam_task_find(task_name).await?;
    let pending_executions = pending_rows_for(api, task_name).await?;
    Ok(compose_blast_radius(&record, pending_executions, verb))
}

/// The single-line render the CLI's `require_confirmation` operation
/// string embeds (ONE fn so the CLI refusal and any future TUI body
/// agree): `"{verb} {task}: {controller_impact} targets: [a, b]
/// pending: {n}"`.
pub fn render_preview_line(preview: &BlastRadiusPreview) -> String {
    format!(
        "{verb} {task}: {impact} targets: [{targets}] pending: {pending}",
        verb = preview.verb,
        task = preview.task,
        impact = preview.controller_impact,
        targets = preview.target_gateways.join(", "),
        pending = preview.pending_executions.len(),
    )
}

/// `ign eam history` output model — all keys always.
#[derive(Debug, Serialize)]
pub struct EamHistoryResult {
    /// The run items, wire-faithful passthrough (newest first as the
    /// gateway orders them).
    pub items: Vec<EamHistoryItem>,
    /// How many items came back (the explicit limit's page).
    pub count: usize,
}

/// One task-definition summary row (the agent-stable shape).
#[derive(Debug, Serialize)]
pub struct EamTaskSummary {
    /// Definition name.
    pub name: String,
    /// `config.profile.type` (`eam_backup`, …) — null when the
    /// record's config carries no profile type.
    pub task_type: Option<String>,
    /// `config.profile.scheduleMode` (`OnDemand`, …) — null when
    /// absent.
    pub schedule_mode: Option<String>,
    /// `scheduledTaskState.currentState` — null when the list shape
    /// carries no state (find answers do).
    pub current_state: Option<String>,
}

/// `ign eam tasks` output model — all keys always.
#[derive(Debug, Serialize)]
pub struct EamTasksResult {
    /// The definition summary rows.
    pub tasks: Vec<EamTaskSummary>,
}

/// `ign eam tasks <NAME>` output model — all keys always.
#[derive(Debug, Serialize)]
pub struct EamTaskDetailResult {
    /// Definition name.
    pub name: String,
    /// The full definition record (config + resource keys,
    /// passthrough as JSON).
    pub definition: serde_json::Value,
    /// The `scheduledTaskState` healthcheck (null when absent —
    /// `currentState`/`nextScheduled`/`owner` under `details`).
    pub state: serde_json::Value,
}

/// `ign eam history` — the runtime read (controller gate honestly
/// classified at the wire seam).
pub async fn eam_history(
    api: &dyn GatewayApi,
    limit: Option<u32>,
    search: Option<&str>,
) -> Result<EamHistoryResult, CoreError> {
    let page = api.eam_task_history(limit, search).await?;
    Ok(EamHistoryResult {
        count: page.items.len(),
        items: page.items,
    })
}

/// `ign eam tasks` — the definitions read (config-resource seam).
pub async fn eam_tasks(api: &dyn GatewayApi) -> Result<EamTasksResult, CoreError> {
    let page = api.eam_task_definitions().await?;
    Ok(EamTasksResult {
        tasks: page.items.iter().map(summary_from).collect(),
    })
}

/// `ign eam tasks <NAME>` — one definition's full record + state;
/// unknown names ride the config-resource `not_found` path.
pub async fn eam_task_detail(
    api: &dyn GatewayApi,
    name: &str,
) -> Result<EamTaskDetailResult, CoreError> {
    let record = api.eam_task_find(name).await?;
    Ok(EamTaskDetailResult {
        name: record.name.clone(),
        definition: serde_json::to_value(&record).unwrap_or(serde_json::Value::Null),
        state: record
            .scheduled_task_state
            .clone()
            .unwrap_or(serde_json::Value::Null),
    })
}

/// The summary projection from one record (the agent-stable keys).
fn summary_from(record: &EamTaskRecord) -> EamTaskSummary {
    let profile = record.config.get("profile");
    EamTaskSummary {
        name: record.name.clone(),
        task_type: profile
            .and_then(|p| p.get("type"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        schedule_mode: profile
            .and_then(|p| p.get("scheduleMode"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        current_state: record
            .scheduled_task_state
            .as_ref()
            .and_then(|state| state.get("currentState"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CancelDecision, EamLifecycleResult, EamTaskRecord, TaskChange, TaskCreateVerdict,
        affected_resources, apply_task_change, auto_type, cancel_decision, compose_blast_radius,
        compose_task_definition, deep_merge, lifecycle_precheck, parse_setting,
        render_preview_line, summary_from, suspend_recheck, task_create_guard,
    };
    use crate::client::eam::{DeleteOutcome, EamScheduledTask};

    /// The ladder EXHAUSTIVELY over the openapi taxonomy's 11 types
    /// × the schedule modes — the planner-locked breadth pinned as a
    /// pure function.
    #[test]
    fn guard_ladder_is_exhaustive_over_the_taxonomy() {
        // eam_backup + OnDemand = the ONLY unguarded cell.
        assert_eq!(
            task_create_guard("eam_backup", "OnDemand"),
            TaskCreateVerdict::Unguarded
        );

        // The refused trio — the ladder's top rung fires regardless
        // of schedule.
        for refused in [
            "eam_restoreBackup",
            "eam_installModules",
            "eam_remoteUpgrade",
        ] {
            assert_eq!(
                task_create_guard(refused, "OnDemand"),
                TaskCreateVerdict::Refused,
                "{refused} refuses even OnDemand"
            );
            assert_eq!(
                task_create_guard(refused, "Scheduled"),
                TaskCreateVerdict::Refused,
                "{refused} refuses under any schedule"
            );
        }

        // The mutating seven — --yes under OnDemand.
        for mutating in [
            "eam_restart",
            "eam_sendProject",
            "eam_sendResource",
            "eam_sendTags",
            "eam_activateLicense",
            "eam_updateLicense",
            "eam_unactivateLicense",
        ] {
            assert_eq!(
                task_create_guard(mutating, "OnDemand"),
                TaskCreateVerdict::NeedsYes,
                "{mutating} needs --yes"
            );
        }

        // ANY non-OnDemand schedule arms autonomous actions — even
        // eam_backup (the openapi schedule tokens + unknown modes).
        for mode in ["Immediate", "Scheduled", "AtTime", "AtDelay", "weird-mode"] {
            assert_eq!(
                task_create_guard("eam_backup", mode),
                TaskCreateVerdict::NeedsYes,
                "scheduleMode {mode} arms the task"
            );
        }

        // Unknown types classify fail-safe (guarded, never silently
        // unguarded — the server's validation is the backstop).
        assert_eq!(
            task_create_guard("eam_unknownFutureType", "OnDemand"),
            TaskCreateVerdict::NeedsYes
        );
    }

    /// The K=V scalar auto-typing rule: bool/int ride typed,
    /// everything else stays a string; malformed input refuses
    /// `invalid_input`.
    #[test]
    fn setting_parsing_auto_types_scalars() {
        assert_eq!(
            parse_setting("concurrentBackups=2").unwrap(),
            ("concurrentBackups".to_string(), serde_json::json!(2))
        );
        assert_eq!(
            parse_setting("forceBackups=true").unwrap(),
            ("forceBackups".to_string(), serde_json::json!(true))
        );
        assert_eq!(
            parse_setting("forceBackups=false").unwrap(),
            ("forceBackups".to_string(), serde_json::json!(false))
        );
        // Negative + big ints ride typed.
        assert_eq!(
            parse_setting("n=-7").unwrap(),
            ("n".to_string(), serde_json::json!(-7))
        );
        // Strings stay strings — including numeric-looking text
        // with units and values that aren't clean ints.
        assert_eq!(
            parse_setting("note=hello world").unwrap(),
            ("note".to_string(), serde_json::json!("hello world"))
        );
        assert_eq!(
            parse_setting("v=1.5").unwrap(),
            ("v".to_string(), serde_json::json!("1.5")),
            "floats are NOT auto-typed (the tags-write rule: bool/int only)"
        );

        let err = parse_setting("noequalsign").expect_err("refuses");
        assert_eq!(err.exit_code(), 2);
        assert_eq!(err.code(), "invalid_input");
        assert!(err.to_string().contains("--definition"));
        assert!(parse_setting("=v").is_err(), "empty key refuses");
        assert!(parse_setting("k=").is_err(), "empty value refuses");
    }

    /// The auto-typing helper's direct pins.
    #[test]
    fn auto_type_covers_bool_int_string() {
        assert_eq!(auto_type("true"), serde_json::json!(true));
        assert_eq!(auto_type("false"), serde_json::json!(false));
        assert_eq!(auto_type("42"), serde_json::json!(42));
        assert_eq!(auto_type("text"), serde_json::json!("text"));
        assert_eq!(auto_type("True"), serde_json::json!("True"), "case matters");
    }

    /// The --definition merge semantics: objects merge recursively,
    /// arrays and scalars REPLACE.
    #[test]
    fn deep_merge_merges_objects_replaces_arrays() {
        let mut base = serde_json::json!({
            "type": "eam_backup",
            "scheduleMode": "OnDemand",
            "targetGateways": ["gw-a"],
            "settingsNested": {"a": 1, "b": {"x": 1}}
        });
        deep_merge(
            &mut base,
            &serde_json::json!({
                "targetGateways": ["gw-b", "gw-c"],
                "targetGroups": [],
                "concurrentBackups": 2,
                "forceBackups": true,
                "settingsNested": {"b": {"y": 2}}
            }),
        );
        assert_eq!(
            base,
            serde_json::json!({
                "type": "eam_backup",
                "scheduleMode": "OnDemand",
                "targetGateways": ["gw-b", "gw-c"],
                "targetGroups": [],
                "concurrentBackups": 2,
                "forceBackups": true,
                "settingsNested": {"a": 1, "b": {"x": 1, "y": 2}}
            })
        );
    }

    /// The composition pins (07-05 gap 3 — the live `config.settings`
    /// shape): profile carries type/scheduleMode ONLY; settings owns
    /// targetGateways/targetGroups + the K=V scalars; a bare create
    /// (no --target) defaults to the controller itself.
    #[test]
    fn composition_splits_profile_and_settings_the_live_shape() {
        // Bare create: targetGateways defaults to ["_controller"]
        // (the live-captured zero-config default on a
        // controller-mode gateway).
        let bare =
            compose_task_definition("uat-backup-demo", "eam_backup", &[], &[], None, "OnDemand")
                .expect("bare composition");
        assert_eq!(bare["name"], serde_json::json!("uat-backup-demo"));
        assert_eq!(
            bare["config"]["profile"],
            serde_json::json!({"type": "eam_backup", "scheduleMode": "OnDemand"}),
            "profile carries type + scheduleMode ONLY (isSuspended is server-owned)"
        );
        assert_eq!(
            bare["config"]["settings"],
            serde_json::json!({"targetGateways": ["_controller"], "targetGroups": []})
        );

        // Explicit --target values replace the default wholesale.
        let targeted = compose_task_definition(
            "nightly-backup",
            "eam_backup",
            &["gw-a".to_string()],
            &[
                "concurrentBackups=2".to_string(),
                "forceBackups=true".to_string(),
            ],
            None,
            "OnDemand",
        )
        .expect("targeted composition");
        assert_eq!(
            targeted["config"]["settings"]["targetGateways"],
            serde_json::json!(["gw-a"])
        );
        assert_eq!(
            targeted["config"]["settings"]["concurrentBackups"],
            serde_json::json!(2),
            "K=V lands in config.SETTINGS"
        );
        assert!(
            targeted["config"]["profile"]
                .get("concurrentBackups")
                .is_none(),
            "profile carries NO settings keys"
        );
        assert!(
            targeted["config"]["profile"]
                .get("targetGateways")
                .is_none(),
            "targetGateways lives in settings, not profile"
        );

        // The --definition overlay deep-merges over the composed
        // SETTINGS object (arrays/scalars replace, objects merge).
        let overlayed = compose_task_definition(
            "t3",
            "eam_backup",
            &["gw-a".to_string()],
            &[],
            Some(&serde_json::json!({
                "targetGateways": ["gw-b", "gw-c"],
                "concurrentBackups": 5
            })),
            "OnDemand",
        )
        .expect("overlay composition");
        assert_eq!(
            overlayed["config"]["settings"]["targetGateways"],
            serde_json::json!(["gw-b", "gw-c"]),
            "the overlay's array REPLACES the composed default"
        );
        assert_eq!(overlayed["config"]["settings"]["concurrentBackups"], 5);
        assert_eq!(
            overlayed["config"]["settings"]["targetGroups"],
            serde_json::json!([]),
            "composed keys the overlay omits survive the merge"
        );
    }

    /// The summary projection carries the profile type/scheduleMode
    /// and degrades to nulls when the list shape carries neither the
    /// state nor a profile.
    #[test]
    fn summary_projects_the_agent_stable_keys() {
        let record: EamTaskRecord = serde_json::from_value(serde_json::json!({
            "name": "nightly-backup",
            "config": {"profile": {"type": "eam_backup", "scheduleMode": "OnDemand"}},
            "scheduledTaskState": {"currentState": "IDLE", "details": {"owner": "eam"}}
        }))
        .expect("record parses");
        let summary = summary_from(&record);
        assert_eq!(summary.name, "nightly-backup");
        assert_eq!(summary.task_type.as_deref(), Some("eam_backup"));
        assert_eq!(summary.schedule_mode.as_deref(), Some("OnDemand"));
        assert_eq!(summary.current_state.as_deref(), Some("IDLE"));

        let bare: EamTaskRecord = serde_json::from_value(serde_json::json!({
            "name": "bare"
        }))
        .expect("bare record parses");
        let summary = summary_from(&bare);
        assert_eq!(summary.task_type, None);
        assert_eq!(summary.schedule_mode, None);
        assert_eq!(summary.current_state, None);
    }

    // ---- 10-03 Task 1: the runtime lifecycle ----

    /// The lifecycle result model serializes ALL keys always (the
    /// agent-stable shape — nulls are honest absence, never omitted
    /// keys), for both a fired write and the cancel no-op.
    #[test]
    fn lifecycle_result_serializes_all_keys_always() {
        let fired = EamLifecycleResult {
            task: "nightly-backup".to_string(),
            action: "suspended".to_string(),
            previous_state: Some("Scheduled".to_string()),
            config_suspended: Some(true),
            pending: None,
            fired: true,
            reason: None,
        };
        let json = serde_json::to_value(&fired).expect("serializes");
        let map = json.as_object().expect("object shape");
        for key in [
            "task",
            "action",
            "previous_state",
            "config_suspended",
            "pending",
            "fired",
            "reason",
        ] {
            assert!(map.contains_key(key), "key {key} always rides");
        }
        assert_eq!(json["pending"], serde_json::Value::Null);
        assert_eq!(json["reason"], serde_json::Value::Null);
        assert_eq!(json["config_suspended"], serde_json::json!(true));

        let noop = EamLifecycleResult {
            task: "t".to_string(),
            action: "cancelled".to_string(),
            previous_state: None,
            config_suspended: None,
            pending: None,
            fired: false,
            reason: Some("no pending execution".to_string()),
        };
        let json = serde_json::to_value(&noop).expect("serializes");
        assert_eq!(json["fired"], serde_json::json!(false));
        assert_eq!(json["reason"], serde_json::json!("no pending execution"));
        assert_eq!(
            json["previous_state"],
            serde_json::Value::Null,
            "no state on a find that carried none — null, not omitted"
        );
    }

    /// The pure name precheck refuses empty/whitespace names exit 2
    /// (and ONLY those — every other question needs the find).
    #[test]
    fn lifecycle_precheck_refuses_empty_and_whitespace_names() {
        for name in ["", "   ", "\t\n"] {
            for action in ["suspend", "resume", "cancel"] {
                let err =
                    lifecycle_precheck(action, name).expect_err("empty/whitespace names refuse");
                assert_eq!(err.exit_code(), 2, "usage class");
                assert_eq!(err.code(), "invalid_input");
                let message = err.to_string();
                assert!(
                    message.contains(action),
                    "the refusal names the verb: {message}"
                );
            }
        }
        lifecycle_precheck("suspend", "nightly-backup").expect("real names pass");
        lifecycle_precheck("cancel", " x ")
            .expect("trimmed-nonempty passes (the gateway owns identifier rules)");
    }

    /// The suspend re-check (pure) refuses ONLY the capture-proven
    /// already-suspended case (exit 2 naming the task); false,
    /// absent, and unparseable flags all fire.
    #[test]
    fn suspend_recheck_refuses_only_already_suspended() {
        let record_of = |is_suspended: serde_json::Value| -> EamTaskRecord {
            serde_json::from_value(serde_json::json!({
                "name": "nightly-backup",
                "config": {"profile": {"isSuspended": is_suspended}}
            }))
            .expect("record parses")
        };

        let err = suspend_recheck(&record_of(serde_json::json!(true)))
            .expect_err("already-suspended refuses pre-write");
        assert_eq!(err.exit_code(), 2);
        assert_eq!(err.code(), "invalid_input");
        let message = err.to_string();
        assert!(
            message.contains("nightly-backup") && message.contains("already suspended"),
            "the refusal names the task + state: {message}"
        );

        suspend_recheck(&record_of(serde_json::json!(false)))
            .expect("false fires (the normal case)");
        suspend_recheck(&record_of(serde_json::Value::Null))
            .expect("absent flag fires (no invented rules)");
        suspend_recheck(&record_of(serde_json::json!("weird")))
            .expect("unparseable flag fires (the gateway's 500 is the honest answer)");
    }

    /// The cancel decision (pure) over the pending row — every
    /// branch mirrors a captured fact (the §2 can* truth cell, §7's
    /// silent-204 nothing-pending answer).
    #[test]
    fn cancel_decision_branches_mirror_the_captures() {
        assert_eq!(
            cancel_decision(None),
            CancelDecision::NoPending,
            "nothing pending → the honest no-op, no doomed POST"
        );

        let row_of = |can_cancel: bool| -> EamScheduledTask {
            serde_json::from_value(serde_json::json!({
                "name": "nightly-backup",
                "owner": "eam",
                "type": "Collect Backup",
                "execStart": null,
                "message": "",
                "repeats": true,
                "canPause": true,
                "canResume": false,
                "canCancel": can_cancel,
                "taskState": "Scheduled",
                "isForced": false,
                "isRunning": false,
                "progress": 0.0
            }))
            .expect("the captured row shape parses")
        };

        assert_eq!(
            cancel_decision(Some(&row_of(true))),
            CancelDecision::Fire,
            "the captured Scheduled cell (canCancel: true) fires"
        );
        assert_eq!(
            cancel_decision(Some(&row_of(false))),
            CancelDecision::NotPermitted,
            "the gateway's own canCancel=false is reported, not overridden"
        );
    }

    // ---- 10-03 Task 2: modify + delete ----

    /// The FULL-record fixture — the captured find shape (§0's key
    /// inventory: type/name/description/enabled/version/collection/
    /// collections/signature/config{profile,settings}/data/
    /// attributes/metrics/healthchecks) with the §6a baseline values.
    fn full_record_fixture() -> serde_json::Value {
        serde_json::json!({
            "type": "com.inductiveautomation.eam/eam-tasks",
            "name": "ign-p10-scratch-sched",
            "description": "scratch",
            "enabled": true,
            "version": 1,
            "collection": "core",
            "collections": ["core"],
            "signature": "e5ac8bee3a6ba85e40923c0e02d29507600c57519eb8e4d78bd8c258197fe9c6",
            "config": {
                "profile": {
                    "type": "eam_backup",
                    "isSuspended": false,
                    "scheduleMode": "Scheduled",
                    "scheduleDetails": "0/30 * * * * ?"
                },
                "settings": {
                    "targetGateways": ["_controller"],
                    "targetGroups": [],
                    "concurrentBackups": 0,
                    "forceBackups": false
                }
            },
            "data": ["config.json"],
            "attributes": {"uuid": "c1aa2b52-46ad-46ea-962b-9d2498f35db1", "enabled": true},
            "metrics": {},
            "healthchecks": {"scheduledTaskState": {"currentState": "Scheduled"}}
        })
    }

    /// THE never-compose-from-scratch invariant: the modify PUT body
    /// preserves the fixture's config.settings byte-equal on a
    /// settings-free change; ONLY targeted keys move; signature/
    /// collection/unknown round-trip keys ride the clone untouched.
    #[test]
    fn modify_put_body_preserves_the_fixture_record_except_targeted_keys() {
        // A description-only change: EVERYTHING else byte-equal.
        let mut body = full_record_fixture();
        let changed = apply_task_change(
            &mut body,
            &TaskChange {
                description: Some("rewritten note".to_string()),
                ..Default::default()
            },
        );
        assert_eq!(changed, vec!["description"]);
        let fixture = full_record_fixture();
        assert_eq!(
            body["config"]["settings"], fixture["config"]["settings"],
            "config.settings rides VERBATIM — omitting/reshaping it is the 422 trap"
        );
        assert_eq!(
            body["signature"], fixture["signature"],
            "the ORIGINAL signature"
        );
        assert_eq!(body["collection"], fixture["collection"]);
        assert_eq!(body["config"]["profile"], fixture["config"]["profile"]);
        assert_eq!(
            body["data"], fixture["data"],
            "unknown round-trip keys survive"
        );
        assert_eq!(body["attributes"], fixture["attributes"]);
        assert_eq!(body["description"], serde_json::json!("rewritten note"));

        // Targeted enabled + settings overlay: ONLY those keys move
        // (the overlay deep-merges; the rest of settings stays).
        let mut body = full_record_fixture();
        let changed = apply_task_change(
            &mut body,
            &TaskChange {
                enabled: Some(false),
                settings_overlay: Some(serde_json::json!({"concurrentBackups": 4})),
                ..Default::default()
            },
        );
        assert_eq!(changed, vec!["enabled", "config.settings"]);
        assert_eq!(body["enabled"], serde_json::json!(false));
        let mut expected_settings = fixture["config"]["settings"].clone();
        expected_settings["concurrentBackups"] = serde_json::json!(4);
        assert_eq!(body["config"]["settings"], expected_settings);
        assert_eq!(body["signature"], fixture["signature"]);

        // A schedule-mode change touches ONLY the profile's mode key.
        let mut body = full_record_fixture();
        let changed = apply_task_change(
            &mut body,
            &TaskChange {
                schedule_mode: Some("OnDemand".to_string()),
                ..Default::default()
            },
        );
        assert_eq!(changed, vec!["config.profile.scheduleMode"]);
        assert_eq!(
            body["config"]["profile"]["scheduleMode"],
            serde_json::json!("OnDemand")
        );
        assert_eq!(
            body["config"]["profile"]["scheduleDetails"],
            fixture["config"]["profile"]["scheduleDetails"],
            "scheduleDetails rides the clone (this change's scope is the mode key)"
        );
        assert_eq!(body["config"]["settings"], fixture["config"]["settings"]);
    }

    /// The modify result model serializes ALL keys always.
    #[test]
    fn modify_result_serializes_all_keys_always() {
        let result = super::EamModifyResult {
            task: "t".to_string(),
            changed: vec!["enabled".to_string()],
            definition: serde_json::json!({"name": "t"}),
            put_outcome: None,
            readback: serde_json::Value::Null,
        };
        let json = serde_json::to_value(&result).expect("serializes");
        for key in ["task", "changed", "definition", "put_outcome", "readback"] {
            assert!(
                json.as_object().unwrap().contains_key(key),
                "{key} always rides"
            );
        }
        assert_eq!(json["put_outcome"], serde_json::Value::Null);
    }

    /// The delete result's affected-names extraction: `changes[]`
    /// names are capture-proven; the `references` element shape is
    /// UNOBSERVED (§3d) — strings ride, name-keyed objects
    /// contribute, the rest is skipped honestly; dedup, wire order.
    #[test]
    fn delete_result_extracts_affected_names_from_changes_and_references() {
        let success: DeleteOutcome = serde_json::from_value(serde_json::json!({
            "success": true,
            "changes": [
                {
                    "name": "ign-p10-scratch-sched",
                    "type": "com.inductiveautomation.eam/eam-tasks",
                    "collection": "core",
                    "newSignature": "ec961ee921c63b18013094870ed2664331e965c4770fdf84bfe136e0b4164244"
                }
            ],
            "problem": null,
            "references": []
        }))
        .expect("the captured success body parses");
        assert_eq!(
            affected_resources(&success),
            vec!["ign-p10-scratch-sched".to_string()],
            "a lone-resource delete touches exactly the deleted resource"
        );

        // The confirm-demand shape is UNOBSERVED — the extraction is
        // lenient over spec-shaped reference elements (marked as
        // such; nothing here is capture-proven beyond `[]`/null).
        let demanded: DeleteOutcome = serde_json::from_value(serde_json::json!({
            "success": false,
            "changes": [
                {"name": "task-a", "type": "com.inductiveautomation.eam/eam-tasks", "collection": "core", "newSignature": "x"}
            ],
            "problem": null,
            "references": ["agent-b", {"name": "task-c"}, {"shapeless": true}, 42, "task-a"]
        }))
        .expect("the lenient shape parses");
        assert_eq!(
            affected_resources(&demanded),
            vec![
                "task-a".to_string(),
                "agent-b".to_string(),
                "task-c".to_string()
            ],
            "strings + name-keyed objects ride; shapeless elements skipped; dedup holds"
        );
    }

    // ---- 10-03 Task 3: the blast-radius preview ----

    /// The captured scheduled row, name-adjustable (the §2 verbatim
    /// shape) — the preview's pending-input fixture.
    fn scheduled_row(name: &str) -> EamScheduledTask {
        serde_json::from_value(serde_json::json!({
            "name": name,
            "owner": "eam",
            "type": "Collect Backup",
            "execStart": null,
            "message": "",
            "repeats": true,
            "canPause": true,
            "canResume": false,
            "canCancel": true,
            "taskState": "Scheduled",
            "isForced": false,
            "isRunning": false,
            "progress": 0.0
        }))
        .expect("the captured row shape parses")
    }

    /// The composer over fixture find + scheduled bodies: the task
    /// targets 2 agents with 1 pending execution; rows for OTHER
    /// tasks are filtered; the agent-stable keys ride.
    #[test]
    fn preview_composes_over_fixture_find_and_scheduled() {
        let record: EamTaskRecord = serde_json::from_value(serde_json::json!({
            "name": "ign-p10-scratch-sched",
            "config": {
                "profile": {"type": "eam_backup", "isSuspended": false, "scheduleMode": "Scheduled"},
                "settings": {"targetGateways": ["gw-a", "gw-b"], "targetGroups": []}
            },
            "signature": "sig",
            "scheduledTaskState": {"currentState": "Scheduled", "details": {"owner": "eam"}}
        }))
        .expect("fixture record parses");
        let pending = vec![
            scheduled_row("ign-p10-scratch-sched"),
            scheduled_row("some-other-task"),
        ];

        let preview = compose_blast_radius(&record, pending, "suspend");
        assert_eq!(preview.task, "ign-p10-scratch-sched");
        assert_eq!(preview.task_type.as_deref(), Some("eam_backup"));
        assert_eq!(preview.schedule_mode.as_deref(), Some("Scheduled"));
        assert_eq!(preview.state.as_deref(), Some("Scheduled"));
        assert_eq!(preview.owner.as_deref(), Some("eam"));
        assert_eq!(preview.config_suspended, Some(false));
        assert_eq!(
            preview.target_gateways,
            vec!["gw-a".to_string(), "gw-b".to_string()],
            "the AGENTS the write touches"
        );
        assert_eq!(
            preview.pending_executions.len(),
            1,
            "rows for other tasks are filtered out"
        );
        assert!(preview.pending_executions[0].can_cancel);
        assert_eq!(preview.verb, "suspend");
        let impact = &preview.controller_impact;
        assert!(
            impact.contains("ign-p10-scratch-sched")
                && impact.contains("eam_backup")
                && impact.contains("2 agents")
                && impact.contains("stop until resumed"),
            "the impact names task + type + agent count + consequence: {impact}"
        );

        // All keys always — serialization never drops a key.
        let json = serde_json::to_value(&preview).expect("serializes");
        for key in [
            "task",
            "task_type",
            "schedule_mode",
            "state",
            "owner",
            "config_suspended",
            "target_gateways",
            "pending_executions",
            "controller_impact",
            "verb",
        ] {
            assert!(
                json.as_object().unwrap().contains_key(key),
                "{key} always rides"
            );
        }
    }

    /// The empty case: a bare record (no settings, no healthcheck)
    /// and zero pending rows compose an honest preview — empty
    /// lists, nulls, and an impact that still names the task.
    #[test]
    fn preview_tolerates_empty_targets_and_pending() {
        let record: EamTaskRecord = serde_json::from_value(serde_json::json!({
            "name": "bare",
            "config": {}
        }))
        .expect("bare record parses");
        let preview = compose_blast_radius(&record, Vec::new(), "cancel");
        assert_eq!(preview.target_gateways, Vec::<String>::new());
        assert_eq!(preview.pending_executions, Vec::<EamScheduledTask>::new());
        assert_eq!(preview.task_type, None);
        assert_eq!(preview.owner, None);
        assert!(
            preview.controller_impact.contains("bare")
                && preview.controller_impact.contains("no pending execution"),
            "the cancel impact names the empty case factually: {}",
            preview.controller_impact
        );
    }

    /// Per-verb impact sentences DIFFER where the verbs differ —
    /// one composer, six honest sentences (plus the fallback).
    #[test]
    fn preview_impacts_differ_per_verb() {
        let record: EamTaskRecord = serde_json::from_value(serde_json::json!({
            "name": "nightly-backup",
            "config": {
                "profile": {"type": "eam_backup", "scheduleMode": "Scheduled"},
                "settings": {"targetGateways": ["gw-a"]}
            },
            "scheduledTaskState": {"currentState": "Scheduled", "details": {"owner": "eam"}}
        }))
        .expect("fixture record parses");
        let pending = vec![scheduled_row("nightly-backup")];

        let mut impacts = Vec::new();
        for verb in ["suspend", "resume", "cancel", "force", "modify", "delete"] {
            let preview = compose_blast_radius(&record, pending.clone(), verb);
            let impact = preview.controller_impact.clone();
            assert!(
                impact.contains("nightly-backup") && impact.contains("1 agent"),
                "{verb}'s impact names the task + agent count: {impact}"
            );
            impacts.push(impact);
        }
        let distinct: std::collections::BTreeSet<&String> = impacts.iter().collect();
        assert_eq!(
            distinct.len(),
            impacts.len(),
            "each verb's factual sentence is distinct: {impacts:?}"
        );
        // The pending-aware cancel names the execution count; the
        // empty-pending cancel says so.
        let cancel_with = compose_blast_radius(&record, pending.clone(), "cancel");
        assert!(cancel_with.controller_impact.contains("1 queued dispatch"));
        let cancel_without = compose_blast_radius(&record, Vec::new(), "cancel");
        assert!(
            cancel_without
                .controller_impact
                .contains("no pending execution")
        );
        // Unknown verbs get the factual fallback (never a panic).
        let odd = compose_blast_radius(&record, Vec::new(), "teleport");
        assert!(odd.controller_impact.contains("teleport"));
    }

    /// THE render pin: `"{verb} {task}: {impact} targets: [a, b]
    /// pending: {n}"` — the ONE line both the CLI refusal and the
    /// future TUI body embed.
    #[test]
    fn render_preview_line_contains_task_agents_verb_and_pending() {
        let record: EamTaskRecord = serde_json::from_value(serde_json::json!({
            "name": "ign-p10-scratch",
            "config": {
                "profile": {"type": "eam_backup", "scheduleMode": "OnDemand"},
                "settings": {"targetGateways": ["_controller"]}
            }
        }))
        .expect("fixture record parses");
        let preview = compose_blast_radius(
            &record,
            vec![
                scheduled_row("ign-p10-scratch"),
                scheduled_row("ign-p10-scratch"),
            ],
            "delete",
        );
        assert_eq!(
            preview.pending_executions.len(),
            2,
            "same-name rows from both segments both count"
        );
        let line = render_preview_line(&preview);
        assert_eq!(
            line,
            "delete ign-p10-scratch: deletes task ign-p10-scratch (eam_backup) permanently \
             — dispatches to 1 agent stop targets: [_controller] pending: 2"
        );
        assert!(line.contains("ign-p10-scratch"));
        assert!(line.contains("1 agent"));

        // Empty targets render as empty brackets (never "[ ]" or a
        // dropped key — the format is agent-stable).
        let bare = compose_blast_radius(
            &serde_json::from_value::<EamTaskRecord>(serde_json::json!({
                "name": "bare", "config": {}
            }))
            .expect("bare parses"),
            Vec::new(),
            "resume",
        );
        assert_eq!(
            render_preview_line(&bare),
            "resume bare: resumes task bare — scheduled dispatches to 0 agents can fire again targets: [] pending: 0"
        );
    }
}
