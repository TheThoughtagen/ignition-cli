//! The raw api-call action (09-03, EXT-01) — `ign api call`'s verb
//! layer: the usage guards run HERE too (in-process callers cannot
//! skip them; main.rs runs them even EARLIER, pre-resolve, so the
//! failure envelope's profile is null and zero construction happens),
//! then the client passthrough and the envelope outcome model.

use serde::Serialize;

use crate::client::GatewayApi;
use crate::client::apicall::{self, ApiCallRequest};
use crate::error::CoreError;

/// `ign api call` result — the outcome the envelope's `data` carries:
/// what was asked plus the gateway's verbatim answer (ALL keys
/// always; `result.data` serializes inline — gateway-verbatim, key
/// order preserved, the README's documented contract exception).
#[derive(Debug, Serialize)]
pub struct ApiCallOutcome {
    /// The HTTP method as sent (normalized uppercase at the CLI).
    pub method: String,
    /// The path as sent.
    pub path: String,
    /// The gateway's answer: HTTP status + verbatim JSON body.
    pub result: apicall::ApiCallData,
}

/// `ign api call` — refusal + path validation FIRST (the action-level
/// re-check: main.rs's pre-resolve guards are for the CLI's exit
/// contract; this one keeps in-process callers honest), then the raw
/// call through the seam.
pub async fn api_call(
    api: &dyn GatewayApi,
    call: ApiCallRequest,
) -> Result<ApiCallOutcome, CoreError> {
    apicall::refuse_auth_headers(&call.headers)?;
    apicall::validate_path(&call.path)?;
    let result = api.api_call(&call).await?;
    Ok(ApiCallOutcome {
        method: call.method,
        path: call.path,
        result,
    })
}

#[cfg(test)]
mod tests {
    use super::{ApiCallOutcome, api_call};
    use crate::client::GatewayApi;
    use crate::client::apicall::{ApiCallData, ApiCallRequest};
    use crate::error::CoreError;

    /// A rig that ANSWERS api calls (so the guard-then-delegate order
    /// is observable) and is unreachable everywhere else — the
    /// established action-double shape.
    struct ApiCallRig;

    #[async_trait::async_trait]
    impl GatewayApi for ApiCallRig {
        async fn bundle_generate(
            &self,
        ) -> Result<crate::client::diagnostics::BundleStatusWire, CoreError> {
            unreachable!("not part of this action")
        }
        async fn bundle_status(
            &self,
        ) -> Result<crate::client::diagnostics::BundleStatusWire, CoreError> {
            unreachable!("not part of this action")
        }
        async fn bundle_download(
            &self,
            _out: &std::path::Path,
        ) -> Result<crate::client::projects::ExportMeta, CoreError> {
            unreachable!("not part of this action")
        }
        async fn api_call(&self, _call: &ApiCallRequest) -> Result<ApiCallData, CoreError> {
            Ok(ApiCallData {
                status: 200,
                data: serde_json::value::RawValue::from_string("{}".to_string()).expect("fixture"),
            })
        }
        async fn license_status(
            &self,
        ) -> Result<crate::client::license::LicenseStatusWire, CoreError> {
            unreachable!("not part of this action")
        }
        async fn redundancy_status(
            &self,
        ) -> Result<crate::client::redundancy::RedundancyStatusWire, CoreError> {
            unreachable!("not part of this action")
        }
        async fn gan_status(&self) -> Result<crate::client::gan::GanStatusWire, CoreError> {
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
        ) -> Result<crate::client::query::ListEnvelope<crate::client::status::ModuleInfo>, CoreError>
        {
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
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::sessions::DesignerInfo>,
            CoreError,
        > {
            unreachable!("not part of this action")
        }
        async fn perspective_sessions(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::sessions::PerspectiveSession>,
            CoreError,
        > {
            unreachable!("not part of this action")
        }
        async fn vision_clients(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::sessions::VisionClient>,
            CoreError,
        > {
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
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::connections::GatewayConnection>,
            CoreError,
        > {
            unreachable!("not part of this action")
        }
        async fn opc_connections(
            &self,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::connections::GatewayConnection>,
            CoreError,
        > {
            unreachable!("not part of this action")
        }
        async fn logs(
            &self,
            _filter: &crate::client::logs::LogQuery,
        ) -> Result<crate::client::query::ListEnvelope<crate::client::logs::LogEntry>, CoreError>
        {
            unreachable!("not part of this action")
        }
        async fn logs_download(&self) -> Result<crate::client::logs::LogDownload, CoreError> {
            unreachable!("not part of this action")
        }
        async fn loggers(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<crate::client::query::ListEnvelope<crate::client::logs::LoggerInfo>, CoreError>
        {
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
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::projects::ProjectRecord>,
            CoreError,
        > {
            unreachable!("not part of this action")
        }
        async fn project_find(
            &self,
            _name: &str,
        ) -> Result<crate::client::projects::ProjectRecord, CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_create(
            &self,
            _body: &crate::client::projects::ProjectCreate,
        ) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_copy(&self, _from: &str, _to: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_rename(&self, _name: &str, _new_name: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_modify(
            &self,
            _name: &str,
            _body: &crate::client::projects::ProjectModify,
        ) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_delete(&self, _name: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_export_to_file(
            &self,
            _name: &str,
            _out: &std::path::Path,
        ) -> Result<crate::client::projects::ExportMeta, CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_import(
            &self,
            _name: &str,
            _zip: Vec<u8>,
            _overwrite: bool,
        ) -> Result<crate::client::projects::ImportOutcome, CoreError> {
            unreachable!("not part of this action")
        }
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
        async fn eam_task_history(
            &self,
            _limit: Option<u32>,
            _search: Option<&str>,
        ) -> Result<crate::client::query::ListEnvelope<crate::client::eam::EamHistoryItem>, CoreError>
        {
            unreachable!("not part of this action")
        }
        async fn eam_task_definitions(
            &self,
        ) -> Result<crate::client::query::ListEnvelope<crate::client::eam::EamTaskRecord>, CoreError>
        {
            unreachable!("not part of this action")
        }
        async fn eam_task_find(
            &self,
            _name: &str,
        ) -> Result<crate::client::eam::EamTaskRecord, CoreError> {
            unreachable!("not part of this action")
        }
        async fn eam_task_create(&self, _definition: &serde_json::Value) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn eam_task_force(&self, _owner: &str, _name: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn eam_task_suspend(&self, _name: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn eam_task_resume(&self, _name: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn eam_task_cancel(&self, _name: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn eam_tasks_scheduled(
            &self,
            _running: bool,
        ) -> Result<Vec<crate::client::eam::EamScheduledTask>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn eam_task_modify(
            &self,
            _definition: &serde_json::Value,
        ) -> Result<Option<crate::client::eam::ModifyOutcome>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn eam_task_delete(
            &self,
            _name: &str,
            _signature: &str,
            _confirm: bool,
        ) -> Result<crate::client::eam::DeleteOutcome, CoreError> {
            unreachable!("not part of this action")
        }
    }

    fn call(method: &str, path: &str, headers: Vec<(&str, &str)>) -> ApiCallRequest {
        ApiCallRequest {
            method: method.to_string(),
            path: path.to_string(),
            body: None,
            headers: headers
                .into_iter()
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect(),
            query: vec![],
        }
    }

    /// THE action-level guard: an auth-pattern header refuses BEFORE
    /// the client is touched (the rig would have answered — the
    /// refusal proves the re-check runs first; main.rs's pre-resolve
    /// guard is the CLI-facing twin).
    #[tokio::test]
    async fn action_refuses_auth_headers_before_the_client() {
        for headers in [
            vec![("Authorization", "Bearer x")],
            vec![("x-ignition-api-token", "name:key")],
            vec![("COOKIE", "session=1")],
        ] {
            let err = api_call(&ApiCallRig, call("GET", "/data/x", headers))
                .await
                .expect_err("auth-pattern header refuses at the action too");
            assert_eq!(err.code(), "invalid_input");
            assert_eq!(err.exit_code(), 2);
        }
    }

    /// The action re-checks the path contract the same way.
    #[tokio::test]
    async fn action_refuses_bad_paths_before_the_client() {
        for path in ["data/x", "//host/x", "/data/x?a=1"] {
            let err = api_call(&ApiCallRig, call("GET", path, vec![]))
                .await
                .expect_err("bad path refuses at the action too");
            assert_eq!(err.code(), "invalid_input");
        }
    }

    /// The happy pass-through: method/path echo plus the gateway's
    /// verbatim answer ride the outcome.
    #[tokio::test]
    async fn action_delegates_and_wraps_the_answer() {
        let outcome = api_call(&ApiCallRig, call("get", "/data/x", vec![]))
            .await
            .expect("clean call passes");
        let ApiCallOutcome {
            method,
            path,
            result,
        } = outcome;
        assert_eq!(method, "get");
        assert_eq!(path, "/data/x");
        assert_eq!(result.status, 200);
        assert_eq!(result.data.get(), "{}");
        // The outcome serializes with the RawValue inline (the
        // envelope shape's source of truth).
        let json = serde_json::to_string(&ApiCallOutcome {
            method: "GET".to_string(),
            path: "/data/x".to_string(),
            result,
        })
        .expect("outcome serializes");
        assert!(
            json.contains(r#""data":{}"#),
            "RawValue rides inline: {json}"
        );
    }
}
