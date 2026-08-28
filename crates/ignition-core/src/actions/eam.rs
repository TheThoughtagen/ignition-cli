//! EAM task actions (07-02, BKUP-02) — the read-heavy surface with
//! guarded writes. Reads: run history (the runtime seam — the
//! controller state gate classifies honestly) and task definitions
//! (the config-resource seam — available on stock gateways).
//!
//! Two-layer naming (LOCKED): the CLIENT models are wire-faithful;
//! HERE the agent-stable summary re-exposes under unit-explicit keys
//! (`name`/`task_type`/`schedule_mode`/`current_state`), and history
//! items pass through VERBATIM (the gateway's own camelCase keys —
//! execution outcomes are DATA: a `Failed` level with GNET
//! not-connected detail is an exit-0 read, never hidden, research
//! Pitfall 3).

use serde::Serialize;

use crate::client::GatewayApi;
use crate::client::eam::{EamHistoryItem, EamTaskRecord};
use crate::error::CoreError;

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
    use super::{EamTaskRecord, summary_from};

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
}
