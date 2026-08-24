//! Connection actions (02-03, HLTH-05/06): DB/OPC connection status —
//! serde models OUT, no printing. Mirrors [`super::sessions`]: both
//! family keys are ALWAYS present in the data shape; a filter excludes
//! without calling the other endpoint.

use serde::Serialize;

use crate::client::GatewayApi;
use crate::client::connections::GatewayConnection;
use crate::error::CoreError;

/// Which connection family a filter targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConnectionType {
    /// Database connections (`database-connection` resources).
    Database,
    /// OPC connections (`opc-connection` resources).
    Opc,
}

impl ConnectionType {
    /// The kebab-case token (CLI/display form).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Database => "database",
            Self::Opc => "opc",
        }
    }
}

impl std::fmt::Display for ConnectionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `ign connections` output model — both families, always present.
#[derive(Debug, Serialize)]
pub struct ConnectionsResult {
    /// Database connections (empty when filtered out).
    pub database: Vec<GatewayConnection>,
    /// OPC connections (empty when filtered out).
    pub opc: Vec<GatewayConnection>,
}

/// List DB/OPC connections (or just the requested family; the excluded
/// key stays present-but-empty and its endpoint is never called).
pub async fn connections(
    api: &dyn GatewayApi,
    filter: Option<ConnectionType>,
) -> Result<ConnectionsResult, CoreError> {
    let (database, opc) = match filter {
        None => (
            api.database_connections().await?.items,
            api.opc_connections().await?.items,
        ),
        Some(ConnectionType::Database) => (api.database_connections().await?.items, Vec::new()),
        Some(ConnectionType::Opc) => (Vec::new(), api.opc_connections().await?.items),
    };
    Ok(ConnectionsResult { database, opc })
}

#[cfg(test)]
mod tests {
    use super::{ConnectionType, connections};
    use crate::client::GatewayApi;
    use crate::client::connections::GatewayConnection;
    use crate::client::query::{ListEnvelope, ListMetadata};
    use crate::error::CoreError;

    use std::sync::Mutex;

    /// Recording double counting one list call per family.
    #[derive(Default)]
    struct ConnectionsRig {
        calls: Mutex<Vec<&'static str>>,
    }

    fn connection(name: &str) -> GatewayConnection {
        GatewayConnection {
            name: name.into(),
            enabled: true,
            healthchecks: serde_json::json!({"jdbc": "FAIR"}),
            extra: Default::default(),
        }
    }

    fn page(items: Vec<GatewayConnection>) -> ListEnvelope<GatewayConnection> {
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
    impl GatewayApi for ConnectionsRig {
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
        async fn database_connections(&self) -> Result<ListEnvelope<GatewayConnection>, CoreError> {
            self.calls.lock().unwrap().push("database");
            Ok(page(vec![connection("MyPostgres")]))
        }
        async fn opc_connections(&self) -> Result<ListEnvelope<GatewayConnection>, CoreError> {
            self.calls.lock().unwrap().push("opc");
            Ok(page(Vec::new()))
        }

        async fn logs(
            &self,
            _filter: &crate::client::logs::LogQuery,
        ) -> Result<crate::client::query::ListEnvelope<crate::client::logs::LogEntry>, CoreError>
        {
            unreachable!("not part of this double's actions")
        }
        async fn logs_download(&self) -> Result<crate::client::logs::LogDownload, CoreError> {
            unreachable!("not part of this double's actions")
        }
        async fn loggers(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<crate::client::query::ListEnvelope<crate::client::logs::LoggerInfo>, CoreError>
        {
            unreachable!("not part of this double's actions")
        }
        async fn set_logger_level(&self, _logger: &str, _level: &str) -> Result<(), CoreError> {
            unreachable!("not part of this double's actions")
        }
        async fn reset_logger_levels(&self) -> Result<(), CoreError> {
            unreachable!("not part of this double's actions")
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
    }

    /// Unfiltered: both families. Filtered: one call, other key empty.
    #[tokio::test]
    async fn connections_filter_calls_only_the_requested_family() {
        let rig = ConnectionsRig::default();
        let both = connections(&rig, None).await.expect("both families");
        assert_eq!(both.database.len(), 1);
        assert_eq!(both.opc.len(), 0);
        assert_eq!(*rig.calls.lock().unwrap(), vec!["database", "opc"]);

        let rig = ConnectionsRig::default();
        let one = connections(&rig, Some(ConnectionType::Database))
            .await
            .expect("filtered");
        assert_eq!(one.database.len(), 1);
        assert!(one.opc.is_empty());
        assert_eq!(*rig.calls.lock().unwrap(), vec!["database"]);

        let json = serde_json::to_value(&one).expect("serialize");
        let mut keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["database", "opc"], "both keys always present");
    }
}
