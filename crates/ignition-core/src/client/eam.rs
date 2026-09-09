//! EAM capability constants + models (07-02, BKUP-02) — the
//! Enterprise Administration Module's wire, live-proven on 8.3.3
//! during 07-RESEARCH (trimmed openapi extract in the phase dir).
//!
//! TWO seams, one family (the 05-04 tag split precedent):
//!
//! 1. **RUNTIME reads** under `/data/eam/api/v1` — task HISTORY (the
//!    `{items, metadata}` list envelope) and FORCE dispatch (Task 3).
//!    Every runtime endpoint 403s with "This operation can only be
//!    performed when EAM is configured as a controller" on a stock
//!    gateway — message-classified into
//!    [`crate::error::CoreError::EamNotController`] at the classify
//!    seam (path-scoped; never a misleading `auth_rejected`).
//! 2. **TASK DEFINITIONS as config resources** under
//!    `/data/api/v1/resources/com.inductiveautomation.eam/eam-tasks`
//!    — the standard config-resource family (the tag-provider
//!    pattern: array-body POST for create, list/find reads; find
//!    answers the definition + a `scheduledTaskState` healthcheck).
//!
//! Two-layer naming (the LOCKED convention): client models stay
//! wire-faithful camelCase (history items carry the gateway's own
//! `taskName`/`taskStart`/… keys; epoch-ms times as JSON numbers,
//! passthrough); unit-explicit keys live at the ACTIONS layer where
//! useful. Definition records are passthrough shapes (the
//! TagProviderRecord pattern — `config.profile.type` /
//! `scheduleMode` / settings ride as raw JSON).
//!
//! WRITE SURFACE (10-02, capture-locked per 10-LIVE-CAPTURES.md —
//! both rigs, 8.3.3 + 8.3.6):
//!
//! - **Runtime lifecycle verbs** (`suspend`/`resume`/`cancel`) —
//!   POST, empty body, **204 on success** (suspend/resume also PERSIST
//!   `config.profile.isSuspended` into the definition; cancel of a
//!   task with nothing pending is a silent 204). Failures are NOT
//!   4xx: an unknown name (or an untriggered task, or an OnDemand
//!   task) answers **500 Jetty HTML** — suspend's message is
//!   INDISTINGUISHABLE across those causes; resume's names the task
//!   (`No NamedResourceHandler found for task '<name>'`); cancel
//!   never fails observed. There is no 404 on this seam.
//! - **`scheduled/{true|false}` read** — the pending-execution
//!   envelope (`{items, metadata}`), 13 wire keys per item; the
//!   `taskState`/`type` vocabularies are capture-locked STRING consts
//!   (below), never enums.
//! - **Full-record modify (PUT)** — the SAME resource path as create,
//!   single-element ARRAY body carrying the ORIGINAL `signature`
//!   (echo semantics: sent keys land verbatim; the 200
//!   `{success, changes[{name,type,collection,newSignature}], problem}`
//!   body's `newSignature` is authoritative for the NEXT mutation —
//!   signatures are per-write, never cacheable). Rename via PUT is
//!   NOT supported (changed name + original signature ⇒ 404 empty).
//! - **Signature-keyed DELETE** — `/{name}/{signature}` with
//!   `?collection=core` (the COLLECTION, never the type token —
//!   `collection=eam-tasks` 404s) and `confirm=true` only when the
//!   caller explicitly opts in (a lone-resource delete SUCCEEDS
//!   without it; the confirm-demand shape is UNOBSERVED — §3d — so
//!   it is never hard-coded).
//!
//! **FINDING for 10-03/10-04 (recorded here, NOT classified):** a
//! signature mismatch on PUT/DELETE answers **HTTP 500** with a JSON
//! `{success:false, changes:[], problem{message, stacktrace}}` body
//! whose message contains the stable substring `signature mismatch`
//! (drift-proof across 8.3.3/8.3.6; the surrounding prose and stack
//! frames drift, and 8.3.3 LEAKS the live signature). The existing
//! taxonomy has no honest slug for this (500 → `Internal`, exit 1);
//! adding one is a Three-Place decision for the actions/CLI plans —
//! the client layer carries the captured `problem` verbatim in the
//! [`ModifyOutcome`]/[`DeleteOutcome`] models so the caller can
//! detect it without a new classification site.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::client::projects::encode_segment;

/// The EAM runtime base (module-scoped prefix — classify()'s
/// controller-403 arm keys on this). Task 3's force method is its
/// first production caller (the gate comes off then).
pub(crate) const EAM_BASE: &str = "/data/eam/api/v1";

/// GET path — task run history (the standard list envelope).
pub(crate) const EAM_HISTORY_PATH: &str = "/data/eam/api/v1/eam-tasks/history";

/// The history list's DEFAULT limit — EAM history grows unboundedly
/// and the server default is unlimited (the logs family's Pitfall-9
/// discipline: an explicit limit ALWAYS rides the wire).
pub(crate) const EAM_HISTORY_DEFAULT_LIMIT: i64 = 200;

/// POST path — force-dispatch a task now (Task 3; owner = the task
/// healthcheck's `scheduledTaskState.details.owner`, live-captured
/// fallback `"eam"`). 204 is the live-proven success shape.
pub(crate) fn eam_force_path(owner: &str, name: &str) -> String {
    format!("{EAM_BASE}/eam-tasks/force/{owner}/{name}")
}

/// The task-definition config-resource id (the 05-04 tag-provider
/// pattern rides again: array-body POST/PUT, standard list/find).
pub(crate) const EAM_TASKS_RESOURCE: &str = "com.inductiveautomation.eam/eam-tasks";

/// GET path — the task-definition resource list.
pub(crate) fn eam_tasks_list_path() -> String {
    format!("/data/api/v1/resources/list/{EAM_TASKS_RESOURCE}")
}

/// GET path — one definition's full record (`/find/{enc}`) incl. the
/// `scheduledTaskState` healthcheck (`currentState`/`nextScheduled`/
/// `owner`) and the `signature` mutations need.
pub(crate) fn eam_task_find_path(name: &str) -> String {
    format!(
        "/data/api/v1/resources/find/{EAM_TASKS_RESOURCE}/{}",
        encode_segment(name)
    )
}

/// POST path — create task definitions (the body is a JSON ARRAY of
/// definition records; the tag-provider create shape).
pub(crate) fn eam_tasks_create_path() -> String {
    format!("/data/api/v1/resources/{EAM_TASKS_RESOURCE}")
}

/// PUT path — full-record modify of task definitions. The SAME
/// resource path as create (10-LIVE-CAPTURES §6a: the array-body PUT
/// with the ORIGINAL signature is the modify wire shape).
pub(crate) fn eam_tasks_modify_path() -> String {
    eam_tasks_create_path()
}

/// DELETE path — delete-by-signature, the byte-twin of
/// `tag_provider_delete_path` (`/{name}/{signature}`; both segments
/// percent-encoded through the ONE locked encoder — the signature
/// comes from find, the live-proven chain). Query params ride the
/// impl (`collection=core` always; `confirm=true` only on explicit
/// opt-in — 10-LIVE-CAPTURES §3b/§3c/Decision 3).
pub(crate) fn eam_task_delete_path(name: &str, signature: &str) -> String {
    format!(
        "/data/api/v1/resources/{EAM_TASKS_RESOURCE}/{}/{}",
        encode_segment(name),
        encode_segment(signature)
    )
}

/// POST path — suspend a task's scheduler trigger (lifecycle).
/// `{name}` rides RAW — gateway identifiers are URL-safe like the
/// force path (10-LIVE-CAPTURES §1: no encoding observed).
pub(crate) fn eam_task_suspend_path(name: &str) -> String {
    format!("{EAM_BASE}/eam-tasks/suspend/{name}")
}

/// POST path — resume a suspended task (lifecycle; the inverse of
/// [`eam_task_suspend_path`], same raw-name rule).
pub(crate) fn eam_task_resume_path(name: &str) -> String {
    format!("{EAM_BASE}/eam-tasks/resume/{name}")
}

/// POST path — cancel a task's PENDING execution (lifecycle). 204 is
/// the answer whether or not anything was pending (10-LIVE-CAPTURES
/// §7: cancel of an unknown name is also a silent 204).
pub(crate) fn eam_task_cancel_path(name: &str) -> String {
    format!("{EAM_BASE}/eam-tasks/cancel/{name}")
}

/// GET path — the pending-execution read. The LITERAL captured
/// segments: the path takes the WORD `true`/`false` (10-LIVE-CAPTURES
/// §2: "segment takes the literal word, no encoding surprises").
pub(crate) fn eam_tasks_scheduled_path(running: bool) -> String {
    format!("{EAM_BASE}/eam-tasks/scheduled/{running}")
}

/// The CAPTURED `taskState` vocabulary on the scheduled seam —
/// exactly the values both rigs ever answered (10-LIVE-CAPTURES §2 +
/// Decision 2). A `taskState` OUTSIDE this set is honest UNKNOWN:
/// `Running`/`Pending` rows were never capturable (no connected-agent
/// rig) and MUST still parse — the field stays a plain [`String`]
/// (the 09-05 String-vocabulary decision; NEVER an enum). `pub` like
/// the BUNDLE_*_STATES precedent (the actions/CLI tiers consult it).
pub const EAM_TASK_STATES: &[&str] = &[
    // 8.3.6 rig A + 8.3.3 rig B, scheduled/false (10-LIVE-CAPTURES §2).
    "Scheduled",
    // Captured via the find healthcheck `currentState` after a 204
    // suspend — a suspended task vanishes FROM scheduled/false
    // (10-LIVE-CAPTURES §1c + Decision 1).
    "Suspended",
];

/// The CAPTURED `currentState` vocabulary on the find healthcheck
/// (`scheduledTaskState.currentState`) — the lifecycle companion the
/// suspend/resume verbs persist (10-LIVE-CAPTURES §1 + Decision 1).
/// String consts, never an enum; unobserved values passthrough.
pub const EAM_CURRENT_STATES: &[&str] = &[
    // Fresh OnDemand task, pre-trigger (10-LIVE-CAPTURES §1a).
    "Stopped",
    // Scheduled task with a broken/no-op schedule (stats NPE ride-along).
    "Errored",
    // Post-204-suspend read-back (10-LIVE-CAPTURES §1c).
    "Suspended",
];

/// One history item — wire-faithful camelCase (the live-captured
/// shape; `taskName` carries `" (forced)"` on forced runs, `level`
/// e.g. `Failed`, times epoch-ms as the gateway serialized them).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EamHistoryItem {
    /// `taskId` — the run's id, a UUID STRING on 8.3.3 controllers
    /// (wire-faithful; the live capture serializes
    /// `"a2f4dab1-9a8f-4feb-9306-29e261f60453"`).
    #[serde(rename = "taskId", default)]
    pub task_id: String,
    /// `taskName` — the definition name (+ `" (forced)"` on forced
    /// runs).
    #[serde(rename = "taskName", default)]
    pub task_name: String,
    /// `taskStart` — epoch-ms.
    #[serde(rename = "taskStart", default)]
    pub task_start: i64,
    /// `taskEnd` — epoch-ms (null while a run is in flight).
    #[serde(rename = "taskEnd", default)]
    pub task_end: Option<i64>,
    /// `target` — the agent the run dispatched to (e.g.
    /// `_controller`).
    #[serde(rename = "target", default)]
    pub target: Option<String>,
    /// `level` — the outcome class (`Failed`, …) — DATA, never
    /// parsed into an error.
    #[serde(rename = "level", default)]
    pub level: Option<String>,
    /// `detail` — the gateway's own outcome text (GNET
    /// not-connected / trial-expired honesty rides VERBATIM here).
    #[serde(rename = "detail", default)]
    pub detail: Option<String>,
    /// `taskType` — the profile type token (`eam_backup`, …).
    #[serde(rename = "taskType", default)]
    pub task_type: Option<String>,
}

/// One task-definition record — passthrough (the TagProviderRecord
/// pattern): `config.profile.{type,scheduleMode}` + settings ride as
/// raw JSON; find answers additionally carry the
/// `scheduledTaskState` healthcheck and the mutation `signature`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EamTaskRecord {
    /// Resource name (the task definition's name).
    #[serde(default)]
    pub name: String,
    /// Definition config — `profile.type` / `profile.scheduleMode` /
    /// `profile.settings` raw passthrough.
    #[serde(default)]
    pub config: serde_json::Value,
    /// The record's mutation signature (find records carry it).
    #[serde(default)]
    pub signature: Option<String>,
    /// The `scheduledTaskState` healthcheck (find answers carry it:
    /// `currentState` / `nextScheduled` / `owner` under `details`).
    #[serde(rename = "scheduledTaskState", default)]
    pub scheduled_task_state: Option<serde_json::Value>,
    /// `collection`, `type`, `enabled`, … resource keys round-trip.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// One row of the `scheduled/{running}` pending-execution read —
/// wire-faithful, ALL 13 captured keys (10-LIVE-CAPTURES §2 +
/// Decision 2: the extract documented only 10; the live wire carries
/// `isForced`/`isRunning`/`progress` too). Unknown keys are ignored
/// by serde (read-only model — no round-trip needed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EamScheduledTask {
    /// The task definition's name.
    #[serde(default)]
    pub name: String,
    /// The task's owner (the force-verb's owner segment source).
    #[serde(default)]
    pub owner: String,
    /// `type` — a HUMAN LABEL (`"Collect Backup"`), NOT the
    /// `profile.type` token (`eam_backup`): the runtime seam and the
    /// config seam use DIFFERENT vocabularies — passthrough both,
    /// never normalize (the history-`taskType` precedent, confirmed
    /// on this seam; 10-LIVE-CAPTURES §2).
    #[serde(rename = "type", default)]
    pub task_type: Option<String>,
    /// `execStart` — epoch-ms while running; `null` on the captured
    /// scheduled rows (10-LIVE-CAPTURES §2).
    #[serde(rename = "execStart", default)]
    pub exec_start: Option<i64>,
    /// The gateway's own row message (empty string captured).
    #[serde(default)]
    pub message: String,
    /// Whether the execution repeats (the scheduled cron task: `true`).
    #[serde(default)]
    pub repeats: bool,
    /// `canPause` — gateway-owned capability flag (the captured
    /// `Scheduled` cell: `true`).
    #[serde(rename = "canPause", default)]
    pub can_pause: bool,
    /// `canResume` — the captured `Scheduled` cell: `false`.
    #[serde(rename = "canResume", default)]
    pub can_resume: bool,
    /// `canCancel` — the captured `Scheduled` cell: `true`.
    #[serde(rename = "canCancel", default)]
    pub can_cancel: bool,
    /// `taskState` — CAPTURE-LOCKED String vocabulary
    /// ([`EAM_TASK_STATES`]; observed `"Scheduled"`/`"Suspended"`,
    /// `Running`/`Pending` UNOBSERVED — passthrough, NEVER an enum;
    /// 10-LIVE-CAPTURES Decision 2).
    #[serde(rename = "taskState", default)]
    pub task_state: String,
    /// `isForced` — beyond-the-extract live key (false on the
    /// captured rows; the force dispatch never produced a row).
    #[serde(rename = "isForced", default)]
    pub is_forced: bool,
    /// `isRunning` — beyond-the-extract live key (false captured).
    #[serde(rename = "isRunning", default)]
    pub is_running: bool,
    /// `progress` — JSON float (`0.0` captured on both rigs).
    #[serde(default)]
    pub progress: f64,
}

/// One element of a mutation's `changes[]` array — the invariant
/// `{name, type, collection, newSignature}` shape captured across
/// create/modify/delete/module-settings (10-LIVE-CAPTURES §9 +
/// Decision 8). `newSignature` is the POST-write signature
/// (server-derived, opaque, rig-varying — never cacheable across
/// writes; for DELETE it is the deleted resource's final signature).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceChange {
    /// The mutated resource's name.
    #[serde(default)]
    pub name: String,
    /// `type` — the resource type token (e.g.
    /// `com.inductiveautomation.eam/eam-tasks`).
    #[serde(rename = "type", default)]
    pub resource_type: String,
    /// The config-module collection (`core` — the delete query param
    /// must carry this VALUE, never the type token).
    #[serde(default)]
    pub collection: String,
    /// `newSignature` — the post-write signature
    /// (authoritative-for-next-mutation; `None` only if the gateway
    /// ever omits the key — lenient).
    #[serde(rename = "newSignature", default)]
    pub new_signature: Option<String>,
}

/// The `problem{message, stacktrace}` shape — present + non-null ONLY
/// on the semantic-refusal 500s (the signature-mismatch family;
/// 10-LIVE-CAPTURES §10 + Decision 9). Message prose and stack frames
/// DRIFT between 8.3.3/8.3.6; the `signature mismatch` substring is
/// stable (classify on it, never on frames — and only at a layer
/// that owns the slug decision).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MutationProblem {
    /// The gateway's message (may LEAK the live signature on 8.3.3 —
    /// surface sanitized, not raw).
    #[serde(default)]
    pub message: String,
    /// The Java stack frames (drifting server internals; carried
    /// honestly, never parsed).
    #[serde(default)]
    pub stacktrace: Vec<String>,
}

/// The captured mutation-success envelope
/// `{success, changes[], problem}` — the 200 body of a full-record
/// PUT modify (10-LIVE-CAPTURES §6a/§9). `problem` is `null` on every
/// captured success; a `success:false` + `problem` answer is the
/// signature-mismatch refusal (HTTP 500 — see the module-doc
/// FINDING).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModifyOutcome {
    /// `success` — the body-level verdict (false on semantic refusals).
    #[serde(default)]
    pub success: bool,
    /// Per-array-item change records.
    #[serde(default)]
    pub changes: Vec<ResourceChange>,
    /// The semantic-refusal problem (None on successes).
    #[serde(default)]
    pub problem: Option<MutationProblem>,
}

/// The DELETE mutation envelope — [`ModifyOutcome`] plus the 4th key
/// `references` (`[]` on every captured success, `null` on the 500
/// problem shape; 10-LIVE-CAPTURES §3b/§9). The element shape of a
/// NON-empty `references` (the confirm-demand affected-resources
/// list) is UNOBSERVED (§3d) — raw passthrough, never guessed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeleteOutcome {
    /// `success` — the body-level verdict.
    #[serde(default)]
    pub success: bool,
    /// Per-array-item change records.
    #[serde(default)]
    pub changes: Vec<ResourceChange>,
    /// The semantic-refusal problem (None on successes).
    #[serde(default)]
    pub problem: Option<MutationProblem>,
    /// Affected/dependent resources — `[]` captured on success,
    /// `null` on the failure shape; element shape UNOBSERVED.
    #[serde(default)]
    pub references: Option<Vec<serde_json::Value>>,
}

#[cfg(test)]
mod tests {
    use super::{EamHistoryItem, EamTaskRecord};

    /// History items parse under the live-captured wire keys — the
    /// forced-suffix taskName and the Failed level ride VERBATIM
    /// (research Pitfall 3: execution outcomes are DATA). `taskId`
    /// is the captured UUID STRING (8.3.3 wire-faithful — the
    /// 07-UAT gap-1 shape, raw capture in .planning/debug).
    #[test]
    fn history_item_parses_the_live_shape() {
        let item: EamHistoryItem = serde_json::from_value(serde_json::json!({
            "taskId": "a2f4dab1-9a8f-4feb-9306-29e261f60453",
            "taskName": "nightly-backup (forced)",
            "taskStart": 1787930000000_i64,
            "taskEnd": 1787930009000_i64,
            "target": "_controller",
            "level": "Failed",
            "detail": "Gateway network for agent '_controller' is currently not connected",
            "taskType": "eam_backup"
        }))
        .expect("live-captured shape parses");
        assert_eq!(item.task_id, "a2f4dab1-9a8f-4feb-9306-29e261f60453");
        assert_eq!(item.task_name, "nightly-backup (forced)");
        assert_eq!(item.level.as_deref(), Some("Failed"));
        assert!(
            item.detail
                .as_deref()
                .is_some_and(|d| d.contains("not connected"))
        );

        // A running task: no taskEnd, no detail.
        let running: EamHistoryItem = serde_json::from_value(serde_json::json!({
            "taskId": "b3c5ebc2-0b90-40fc-8417-3af372071546",
            "taskName": "nightly-backup",
            "taskStart": 1787930000000_i64
        }))
        .expect("sparse shape parses (tolerant defaults)");
        assert_eq!(running.task_end, None);
        assert_eq!(running.detail, None);
    }

    /// Definition records parse with config passthrough + the find
    /// shape's scheduledTaskState/extra keys round-tripping (the
    /// list shape carries neither state nor signature).
    #[test]
    fn task_record_parses_list_and_find_shapes() {
        let listed: EamTaskRecord = serde_json::from_value(serde_json::json!({
            "name": "nightly-backup",
            "config": {
                "profile": {
                    "type": "eam_backup",
                    "scheduleMode": "OnDemand",
                    "settings": {"targetGateways": [], "targetGroups": [], "concurrentBackups": 0, "forceBackups": false}
                }
            },
            "collection": "eam-tasks",
            "type": "com.inductiveautomation.eam"
        }))
        .expect("list shape parses");
        assert_eq!(
            listed.config["profile"]["type"],
            serde_json::json!("eam_backup")
        );
        assert_eq!(listed.signature, None, "list records carry no signature");
        assert_eq!(
            listed.extra.get("collection"),
            Some(&serde_json::json!("eam-tasks")),
            "resource keys round-trip"
        );

        let found: EamTaskRecord = serde_json::from_value(serde_json::json!({
            "name": "nightly-backup",
            "config": {"profile": {"type": "eam_backup", "scheduleMode": "OnDemand"}},
            "signature": "abc123",
            "scheduledTaskState": {
                "currentState": "IDLE",
                "details": {"owner": "eam", "nextScheduled": None::<String>}
            }
        }))
        .expect("find shape parses");
        assert_eq!(found.signature.as_deref(), Some("abc123"));
        let state = found.scheduled_task_state.expect("state present");
        assert_eq!(state["currentState"], serde_json::json!("IDLE"));
        assert_eq!(state["details"]["owner"], serde_json::json!("eam"));
    }

    /// The force path embeds owner + name raw (both are gateway
    /// identifiers — `[A-Za-z0-9._-]`, URL-safe like logger names).
    #[test]
    fn force_path_is_the_module_scoped_shape() {
        assert_eq!(
            super::eam_force_path("eam", "nightly-backup"),
            "/data/eam/api/v1/eam-tasks/force/eam/nightly-backup"
        );
    }

    /// The lifecycle verb paths ride the runtime seam with the name
    /// RAW (the force-path rule; 10-LIVE-CAPTURES §1/§7 — no encoding
    /// observed on either rig).
    #[test]
    fn lifecycle_paths_are_the_captured_shapes() {
        assert_eq!(
            super::eam_task_suspend_path("nightly-backup"),
            "/data/eam/api/v1/eam-tasks/suspend/nightly-backup"
        );
        assert_eq!(
            super::eam_task_resume_path("nightly-backup"),
            "/data/eam/api/v1/eam-tasks/resume/nightly-backup"
        );
        assert_eq!(
            super::eam_task_cancel_path("nightly-backup"),
            "/data/eam/api/v1/eam-tasks/cancel/nightly-backup"
        );
    }

    /// The scheduled read takes the LITERAL word segments
    /// (`true`/`false` — 10-LIVE-CAPTURES §2).
    #[test]
    fn scheduled_path_takes_the_literal_bool_word() {
        assert_eq!(
            super::eam_tasks_scheduled_path(true),
            "/data/eam/api/v1/eam-tasks/scheduled/true"
        );
        assert_eq!(
            super::eam_tasks_scheduled_path(false),
            "/data/eam/api/v1/eam-tasks/scheduled/false"
        );
    }

    /// Modify rides the SAME resource path as create (the array-body
    /// PUT, §6a); delete is the byte-twin `/{name}/{signature}` shape
    /// with the locked per-segment encoder (hyphens over-encode —
    /// safe; the tag-provider delete-path precedent).
    #[test]
    fn mutation_paths_are_the_config_resource_shapes() {
        assert_eq!(
            super::eam_tasks_modify_path(),
            "/data/api/v1/resources/com.inductiveautomation.eam/eam-tasks"
        );
        assert_eq!(
            super::eam_tasks_create_path(),
            super::eam_tasks_modify_path(),
            "modify and create share ONE resource path"
        );
        assert_eq!(
            super::eam_task_delete_path("nightly-backup", "sig-abc123"),
            "/data/api/v1/resources/com.inductiveautomation.eam/eam-tasks/nightly%2Dbackup/sig%2Dabc123"
        );
    }

    /// The captured vocabularies are STRING consts (never enums) —
    /// pinned verbatim per rig provenance (10-LIVE-CAPTURES
    /// Decisions 1-2).
    #[test]
    fn captured_vocabularies_are_the_string_const_sets() {
        assert_eq!(super::EAM_TASK_STATES, &["Scheduled", "Suspended"]);
        assert_eq!(
            super::EAM_CURRENT_STATES,
            &["Stopped", "Errored", "Suspended"]
        );
    }

    /// The scheduled row parses the VERBATIM captured body
    /// (8.3.6 09:59:39Z, scheduled/false — 10-LIVE-CAPTURES §2),
    /// 13 keys wire-faithful, and an UNOBSERVED taskState
    /// (`"Running"`) still parses — the String-vocabulary discipline.
    #[test]
    fn scheduled_task_parses_the_verbatim_captured_row() {
        let captured: super::EamScheduledTask = serde_json::from_value(serde_json::json!({
            "name": "ign-p10-scratch-sched",
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
        .expect("the captured row parses");
        assert_eq!(captured.name, "ign-p10-scratch-sched");
        assert_eq!(captured.owner, "eam");
        assert_eq!(
            captured.task_type.as_deref(),
            Some("Collect Backup"),
            "the human label rides verbatim — never conflated with profile.type"
        );
        assert_eq!(captured.exec_start, None, "execStart null while scheduled");
        assert_eq!(captured.message, "");
        assert!(captured.repeats);
        assert!(captured.can_pause && !captured.can_resume && captured.can_cancel);
        assert_eq!(captured.task_state, "Scheduled");
        assert!(!captured.is_forced && !captured.is_running);
        assert_eq!(captured.progress, 0.0);

        // An UNOBSERVED state parses honestly (passthrough; never an
        // enum that would reject it — 10-LIVE-CAPTURES Decision 2).
        let running: super::EamScheduledTask = serde_json::from_value(serde_json::json!({
            "name": "t", "owner": "eam", "type": "Collect Backup",
            "execStart": 1788947896010_i64, "message": "executing",
            "repeats": false, "canPause": true, "canResume": true,
            "canCancel": true, "taskState": "Running",
            "isForced": true, "isRunning": true, "progress": 0.5
        }))
        .expect("unobserved Running/Pending rows MUST parse (spec-shaped cells)");
        assert_eq!(running.task_state, "Running");
        assert_eq!(running.progress, 0.5);
    }

    /// The modify 200 body parses the VERBATIM captured shape
    /// (8.3.6 09:58:12Z full-record modify — 10-LIVE-CAPTURES §6a/§9):
    /// success + changes[{name,type,collection,newSignature}] + null
    /// problem.
    #[test]
    fn modify_outcome_parses_the_verbatim_captured_body() {
        let outcome: super::ModifyOutcome = serde_json::from_value(serde_json::json!({
            "success": true,
            "changes": [
                {
                    "name": "ign-p10-scratch-sched",
                    "type": "com.inductiveautomation.eam/eam-tasks",
                    "collection": "core",
                    "newSignature": "0d0dfea2919abb1f02fc86baea73d99696626524169a9ac36526044f89ac16e0"
                }
            ],
            "problem": null
        }))
        .expect("the captured modify body parses");
        assert!(outcome.success);
        assert_eq!(outcome.changes.len(), 1);
        let change = &outcome.changes[0];
        assert_eq!(change.name, "ign-p10-scratch-sched");
        assert_eq!(
            change.resource_type,
            "com.inductiveautomation.eam/eam-tasks"
        );
        assert_eq!(change.collection, "core");
        assert_eq!(
            change.new_signature.as_deref(),
            Some("0d0dfea2919abb1f02fc86baea73d99696626524169a9ac36526044f89ac16e0")
        );
        assert_eq!(outcome.problem, None, "problem null on every success");
    }

    /// The delete 200 body parses with the 4th key `references`
    /// (`[]` on success — 10-LIVE-CAPTURES §3b/§9), and the
    /// signature-mismatch 500 problem shape parses honestly
    /// (`success:false` + drifting message + stack frames, §3a/§10).
    #[test]
    fn delete_outcome_parses_success_and_mismatch_shapes() {
        let success: super::DeleteOutcome = serde_json::from_value(serde_json::json!({
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
        .expect("the captured delete body parses");
        assert!(success.success);
        assert_eq!(
            success.references,
            Some(Vec::new()),
            "references [] on success, honestly empty"
        );

        // The 8.3.6 signature-mismatch 500 body (§3a verbatim): the
        // message carries the STABLE `signature mismatch` substring
        // inside DRIFTING prose (8.3.3's variant leaks the live
        // signature and differs in frames — never parsed).
        let mismatch: super::DeleteOutcome = serde_json::from_value(serde_json::json!({
            "success": false,
            "changes": [],
            "problem": {
                "message": "DELETE illegal: signature mismatch for 'ResourceId{resourcePath=com.inductiveautomation.eam/eam-tasks/ign-p10-scratch-sched, collectionName=core}'",
                "stacktrace": [
                    "com.inductiveautomation.ignition.common.resourcecollection.PushException: DELETE illegal: signature mismatch for …",
                    "\tat com.inductiveautomation.ignition.gateway.resourcecollection.ChangeOperationValidationHandler$AtomicPushValidationHandler.throwIfInvalid(ChangeOperationValidationHandler.java:61)"
                ]
            },
            "references": null
        }))
        .expect("the mismatch body parses");
        assert!(!mismatch.success);
        assert_eq!(mismatch.changes, Vec::<super::ResourceChange>::new());
        let problem = mismatch.problem.expect("the problem rides");
        assert!(
            problem.message.contains("signature mismatch"),
            "the stable substring survives drift"
        );
        assert_eq!(problem.stacktrace.len(), 2);
        assert_eq!(
            mismatch.references, None,
            "references null on the failure shape"
        );
    }
}
