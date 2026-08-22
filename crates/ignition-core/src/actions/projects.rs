//! Project actions (03-01, PROJ-01/02): list with inheritance info,
//! new, copy, rename, set (reparent), delete — serde models OUT, no
//! printing (ARCHITECTURE.md layering: the Phase-6 TUI rides this same
//! layer).
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

use serde::Serialize;

use crate::client::GatewayApi;
use crate::client::projects::{ProjectCreate, ProjectModify, ProjectRecord};
use crate::client::query::ListQuery;
use crate::error::CoreError;

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
    /// every deleted name.
    #[derive(Default)]
    struct ProjectsRig {
        creates: Mutex<Vec<ProjectCreate>>,
        modifies: Mutex<Vec<(String, ProjectModify)>>,
        deletes: Mutex<Vec<String>>,
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
        async fn projects(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<ListEnvelope<ProjectRecord>, CoreError> {
            Ok(page(vec![record("PlantFloor"), record("Base")]))
        }
        async fn project_find(&self, _name: &str) -> Result<ProjectRecord, CoreError> {
            Ok(record("whatever-the-rig-is-asked-for"))
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
}
