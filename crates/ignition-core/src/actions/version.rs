//! `version` action — CLI version always, gateway check when a client was
//! injected, implementing the LOCKED behavior matrix (research Pattern 6):
//!
//! | situation                        | output                          | exit |
//! |----------------------------------|---------------------------------|------|
//! | no client (no profile resolved)  | `cli_version` only              | 0    |
//! | reachable, ≥ 8.3.1               | `cli_version` + `gateway`       | 0    |
//! | answered, < 8.3.1 / unparseable  | `GatewayTooOld` envelope        | 6    |
//! | unreachable                      | `cli_version` + `warnings`      | 0    |
//!
//! LOCKED: unreachable degrades to a warning INSIDE `data` (never a
//! top-level envelope field — the LOCKED envelope never grows fields)
//! because version is a local-info command; hard-failing scripts on a
//! sleeping rig is hostile. The refusal contract applies only when the
//! gateway ANSWERED.

use serde::Serialize;

use crate::client::GatewayApi;
use crate::client::version::{GatewayInfo, MIN_GATEWAY, below_minimum};
use crate::error::CoreError;

/// `version` output model (declaration order = golden field order).
/// `gateway`/`warnings` are omitted when absent/empty, so a fresh install
/// keeps the bare `{"cli_version": …}` shape it has always had.
#[derive(Debug, Serialize)]
pub struct VersionResult {
    /// The CLI's own version.
    pub cli_version: &'static str,
    /// Gateway info when a profile resolved AND the gateway answered within
    /// the minimum. JSON-null-equivalent: the field is simply absent when
    /// there is nothing to report (`Value["gateway"]` is null either way).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway: Option<GatewayInfo>,
    /// Non-fatal degradation notes (e.g. gateway unreachable).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// The version action. The CLI resolves the profile + credential and
/// constructs the client first (the env overlay precedence belongs to the
/// dispatch site); `api: None` is the fresh-install / no-profile path.
/// Credential exhaustion for this *check* is degraded to header-less at
/// the dispatch site (version must not demand a secret) — every other
/// credential error already propagated before we got here.
pub async fn version(
    api: Option<&dyn GatewayApi>,
    cli_version: &'static str,
) -> Result<VersionResult, CoreError> {
    let mut result = VersionResult {
        cli_version,
        gateway: None,
        warnings: Vec::new(),
    };
    let Some(api) = api else {
        return Ok(result);
    };
    match api.gateway_info().await {
        Ok(info) => {
            if below_minimum(&info.ignition_version) {
                // CORE-08: the gateway ANSWERED, so the refusal contract
                // applies — refuse cleanly with the upgrade hint.
                return Err(CoreError::GatewayTooOld {
                    found: info.ignition_version.clone(),
                    minimum: MIN_GATEWAY.to_string(),
                    endpoint: info.endpoint.clone(),
                });
            }
            result.gateway = Some(info);
        }
        // LOCKED: only unreachable degrades to a warning; every other
        // class (auth, internal) propagates through the envelope.
        Err(CoreError::Network { url, .. }) => {
            result.warnings.push(format!("gateway unreachable: {url}"));
        }
        Err(err) => return Err(err),
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::version;
    use crate::client::GatewayApi;
    use crate::client::version::GatewayInfo;
    use crate::error::CoreError;

    /// Test double over the seam: constructs its outcome lazily so no
    /// `CoreError` ever needs `Clone` (Network carries a `reqwest::Error`).
    enum FakeOutcome {
        Ok(GatewayInfo),
        TooOld(String),
        Unreachable(String),
    }

    struct FakeApi(FakeOutcome);

    #[async_trait::async_trait]
    impl GatewayApi for FakeApi {
        async fn gateway_info(&self) -> Result<GatewayInfo, CoreError> {
            match &self.0 {
                FakeOutcome::Ok(info) => Ok(info.clone()),
                FakeOutcome::TooOld(found) => Err(CoreError::GatewayTooOld {
                    found: found.clone(),
                    minimum: "8.3.1".into(),
                    endpoint: Some("http://gw.example.com/data/api/v1/gateway-info".into()),
                }),
                FakeOutcome::Unreachable(url) => Err(CoreError::Network {
                    url: url.clone(),
                    // A real transport error via instant loopback refusal —
                    // reqwest::Error has no public constructor.
                    source: Some(
                        reqwest::get("http://127.0.0.1:1")
                            .await
                            .expect_err("dead port refuses"),
                    ),
                }),
            }
        }

        async fn modules(
            &self,
            _quarantined: bool,
            _query: &crate::client::query::ListQuery,
        ) -> Result<crate::client::query::ListEnvelope<crate::client::status::ModuleInfo>, CoreError>
        {
            unimplemented!("version FakeApi only serves gateway_info")
        }
        async fn overview(&self) -> Result<crate::client::status::Overview, CoreError> {
            unimplemented!("version FakeApi only serves gateway_info")
        }

        // The version matrix only exercises gateway_info — the Phase-2
        // capabilities are unimplemented in THIS double (inspect.rs's
        // fakes serve them).
        async fn status_ping(&self) -> Result<crate::client::status::StatusPing, CoreError> {
            unimplemented!("version FakeApi only serves gateway_info")
        }
        async fn metrics_current(
            &self,
        ) -> Result<crate::client::metrics::CurrentGauges, CoreError> {
            unimplemented!("version FakeApi only serves gateway_info")
        }
        async fn metrics_historic(
            &self,
        ) -> Result<crate::client::metrics::PerformanceCharts, CoreError> {
            unimplemented!("version FakeApi only serves gateway_info")
        }
        async fn metrics_threads(&self) -> Result<crate::client::metrics::ThreadCounts, CoreError> {
            unimplemented!("version FakeApi only serves gateway_info")
        }
        async fn designers(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::sessions::DesignerInfo>,
            CoreError,
        > {
            unimplemented!("version FakeApi only serves gateway_info")
        }
        async fn perspective_sessions(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::sessions::PerspectiveSession>,
            CoreError,
        > {
            unimplemented!("version FakeApi only serves gateway_info")
        }
        async fn vision_clients(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::sessions::VisionClient>,
            CoreError,
        > {
            unimplemented!("version FakeApi only serves gateway_info")
        }
        async fn terminate_perspective_session(
            &self,
            _id: &str,
            _message: Option<&str>,
        ) -> Result<(), CoreError> {
            unimplemented!("version FakeApi only serves gateway_info")
        }
        async fn terminate_vision_client(&self, _id: &str) -> Result<(), CoreError> {
            unimplemented!("version FakeApi only serves gateway_info")
        }
        async fn prune_designer(&self, _id: &str) -> Result<(), CoreError> {
            unimplemented!("version FakeApi only serves gateway_info")
        }
        async fn database_connections(
            &self,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::connections::GatewayConnection>,
            CoreError,
        > {
            unimplemented!("version FakeApi only serves gateway_info")
        }
        async fn opc_connections(
            &self,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::connections::GatewayConnection>,
            CoreError,
        > {
            unimplemented!("version FakeApi only serves gateway_info")
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
    }

    fn info(version: &str) -> GatewayInfo {
        GatewayInfo {
            name: None,
            redundancy_role: None,
            edition: Some("standard".into()),
            ignition_version: version.into(),
            jvm_version: None,
            license: None,
            endpoint: None,
        }
    }

    /// Matrix row 1: no client → cli_version only (fresh install shape).
    #[tokio::test]
    async fn no_client_reports_cli_version_only() {
        let result = version(None, "1.2.3").await.expect("always Ok");
        assert_eq!(result.cli_version, "1.2.3");
        assert_eq!(result.gateway, None);
        assert!(result.warnings.is_empty());
    }

    /// Matrix row 2: reachable + ≥ minimum → gateway attached.
    #[tokio::test]
    async fn reachable_modern_gateway_reported() {
        let api = FakeApi(FakeOutcome::Ok(info("8.3.2")));
        let result = version(Some(&api), "1.2.3").await.expect("exit 0");
        assert_eq!(
            result.gateway.as_ref().expect("gateway").ignition_version,
            "8.3.2"
        );
        assert!(result.warnings.is_empty());
    }

    /// Matrix row 3: answered but below minimum → GatewayTooOld (exit 6)
    /// with endpoint + hint naming the minimum.
    #[tokio::test]
    async fn too_old_gateway_refuses_exit_6() {
        let api = FakeApi(FakeOutcome::TooOld("8.1.14".into()));
        let err = version(Some(&api), "1.2.3").await.expect_err("refuse");
        match &err {
            CoreError::GatewayTooOld {
                found,
                minimum,
                endpoint,
            } => {
                assert_eq!(found, "8.1.14");
                assert_eq!(minimum, "8.3.1");
                assert!(endpoint.is_some(), "CORE-05 endpoint populated");
            }
            other => panic!("wrong error class: {other}"),
        }
        assert_eq!(err.exit_code(), 6);
        assert!(err.hint().expect("hint").contains("8.3.1"));
    }

    /// Matrix row 4 (LOCKED): unreachable → exit-0 warning inside data.
    #[tokio::test]
    async fn unreachable_gateway_degrades_to_warning() {
        let api = FakeApi(FakeOutcome::Unreachable(
            "http://127.0.0.1:1/data/api/v1/gateway-info".into(),
        ));
        let result = version(Some(&api), "1.2.3")
            .await
            .expect("exit 0, never a hard fail");
        assert_eq!(result.gateway, None);
        assert_eq!(result.warnings.len(), 1);
        assert!(
            result.warnings[0].contains("gateway unreachable"),
            "warning names the problem: {}",
            result.warnings[0]
        );
    }
}
