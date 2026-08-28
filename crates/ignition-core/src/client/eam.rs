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
#[cfg_attr(not(test), expect(dead_code))]
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
#[expect(dead_code)] // Task 3's create method is the first caller
pub(crate) fn eam_tasks_create_path() -> String {
    format!("/data/api/v1/resources/{EAM_TASKS_RESOURCE}")
}

/// One history item — wire-faithful camelCase (the live-captured
/// shape; `taskName` carries `" (forced)"` on forced runs, `level`
/// e.g. `Failed`, times epoch-ms as the gateway serialized them).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EamHistoryItem {
    /// `taskId` — the run's id.
    #[serde(rename = "taskId", default)]
    pub task_id: i64,
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

#[cfg(test)]
mod tests {
    use super::{EamHistoryItem, EamTaskRecord};

    /// History items parse under the live-captured wire keys — the
    /// forced-suffix taskName and the Failed level ride VERBATIM
    /// (research Pitfall 3: execution outcomes are DATA).
    #[test]
    fn history_item_parses_the_live_shape() {
        let item: EamHistoryItem = serde_json::from_value(serde_json::json!({
            "taskId": 42,
            "taskName": "nightly-backup (forced)",
            "taskStart": 1787930000000_i64,
            "taskEnd": 1787930009000_i64,
            "target": "_controller",
            "level": "Failed",
            "detail": "Gateway network for agent '_controller' is currently not connected",
            "taskType": "eam_backup"
        }))
        .expect("live-captured shape parses");
        assert_eq!(item.task_id, 42);
        assert_eq!(item.task_name, "nightly-backup (forced)");
        assert_eq!(item.level.as_deref(), Some("Failed"));
        assert!(
            item.detail
                .as_deref()
                .is_some_and(|d| d.contains("not connected"))
        );

        // A running task: no taskEnd, no detail.
        let running: EamHistoryItem = serde_json::from_value(serde_json::json!({
            "taskId": 43,
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
}
