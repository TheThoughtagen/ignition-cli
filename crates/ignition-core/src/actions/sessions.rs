//! Session actions (02-03, HLTH-08): merged list + terminate — serde
//! models OUT, no printing (ARCHITECTURE.md layering: the Phase-6 TUI
//! rides this same layer).
//!
//! Stable data shape for agents: `sessions` always serializes ALL THREE
//! family keys (`designers`, `perspective`, `vision`) — a `--type`
//! filter leaves the excluded keys present as EMPTY arrays, and only
//! the requested family's endpoint is CALLED (no wasted round-trips).

use serde::Serialize;

use crate::client::GatewayApi;
use crate::client::query::ListQuery;
use crate::client::sessions::{DesignerInfo, PerspectiveSession, VisionClient};
use crate::error::CoreError;

/// Which session family a filter or termination targets. Serialized
/// kebab-case (`"designer"` / `"perspective"` / `"vision"`) — the same
/// tokens `--type` accepts on the CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionType {
    /// Designer sessions (terminate = prune).
    Designer,
    /// Perspective browser sessions (terminate carries the message).
    Perspective,
    /// Vision clients (terminate = close).
    Vision,
}

impl SessionType {
    /// The kebab-case token (CLI/display form).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Designer => "designer",
            Self::Perspective => "perspective",
            Self::Vision => "vision",
        }
    }
}

impl std::fmt::Display for SessionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `ign sessions` output model — all three families, always present.
#[derive(Debug, Serialize)]
pub struct SessionsResult {
    /// Active Designer sessions (empty when filtered out).
    pub designers: Vec<DesignerInfo>,
    /// Active Perspective sessions (empty when filtered out).
    pub perspective: Vec<PerspectiveSession>,
    /// Active Vision clients (empty when filtered out).
    pub vision: Vec<VisionClient>,
}

/// `ign sessions terminate` output model.
#[derive(Debug, Serialize)]
pub struct TerminateResult {
    /// The family that was targeted (kebab-case in JSON).
    pub kind: SessionType,
    /// The terminated session/client id.
    pub id: String,
}

/// Merge the session families (or just the requested one). Filtered-out
/// families are present-but-empty in the result and their endpoints are
/// NEVER called.
pub async fn sessions(
    api: &dyn GatewayApi,
    type_filter: Option<SessionType>,
) -> Result<SessionsResult, CoreError> {
    let query = ListQuery::default();
    let (designers, perspective, vision) = match type_filter {
        None => (
            api.designers(&query).await?.items,
            api.perspective_sessions(&query).await?.items,
            api.vision_clients(&query).await?.items,
        ),
        Some(SessionType::Designer) => (api.designers(&query).await?.items, Vec::new(), Vec::new()),
        Some(SessionType::Perspective) => (
            Vec::new(),
            api.perspective_sessions(&query).await?.items,
            Vec::new(),
        ),
        Some(SessionType::Vision) => (
            Vec::new(),
            Vec::new(),
            api.vision_clients(&query).await?.items,
        ),
    };
    Ok(SessionsResult {
        designers,
        perspective,
        vision,
    })
}

/// Terminate one session, mapping the family to its endpoint (designer →
/// prune, perspective → terminate with the optional message, vision →
/// terminate). Confirmation guarding belongs to the CALLER (the CLI
/// refuses without `--yes` before any API construction) — the action is
/// the obedient arm.
pub async fn terminate_session(
    api: &dyn GatewayApi,
    kind: SessionType,
    id: &str,
    message: Option<&str>,
) -> Result<TerminateResult, CoreError> {
    match kind {
        SessionType::Designer => api.prune_designer(id).await?,
        SessionType::Perspective => api.terminate_perspective_session(id, message).await?,
        SessionType::Vision => api.terminate_vision_client(id).await?,
    }
    Ok(TerminateResult {
        kind,
        id: id.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::{SessionType, TerminateResult, sessions, terminate_session};
    use crate::client::GatewayApi;
    use crate::client::query::{ListEnvelope, ListMetadata};
    use crate::client::sessions::{DesignerInfo, PerspectiveSession, VisionClient};
    use crate::error::CoreError;

    use std::sync::Mutex;

    /// A recording double: counts every list call per family and every
    /// terminate call (kind + id + message), serving one item per list.
    #[derive(Default)]
    struct SessionsRig {
        list_calls: Mutex<Vec<&'static str>>,
        terminates: Mutex<Vec<(&'static str, String, Option<String>)>>,
    }

    fn designer(id: &str) -> DesignerInfo {
        DesignerInfo {
            id: id.into(),
            address: "192.168.1.50:52526".into(),
            user: "admin".into(),
            project: "MyProject".into(),
            memory: serde_json::json!({"used": 1}),
            uptime: 600000,
            lastcomm: 1787346747022,
            timeout: 3600000,
            timezone: "America/New_York".into(),
            extra: Default::default(),
        }
    }

    fn perspective(id: &str) -> PerspectiveSession {
        PerspectiveSession {
            id: id.into(),
            username: "admin".into(),
            authorized: true,
            project: "MyProject".into(),
            client_address: "10.0.0.5".into(),
            last_comm: 1787346747022,
            active_pages: 1,
            user_agent: "Mozilla/5.0".into(),
            extra: Default::default(),
        }
    }

    fn vision(id: &str) -> VisionClient {
        VisionClient {
            id: id.into(),
            address: "10.0.0.9:443".into(),
            user: "operator".into(),
            project: "PlantFloor".into(),
            memory: serde_json::json!({"used": 1}),
            uptime: 120000,
            lastcomm: 1787346747022,
            timeout: 3600000,
            timezone: "UTC".into(),
            tag_count: 1523,
            extra: Default::default(),
        }
    }

    fn page<T>(items: Vec<T>) -> ListEnvelope<T> {
        ListEnvelope {
            items,
            metadata: ListMetadata {
                total: 1,
                matching: 1,
                limit: -1,
                offset: 0,
            },
        }
    }

    #[async_trait::async_trait]
    impl GatewayApi for SessionsRig {
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
        ) -> Result<ListEnvelope<DesignerInfo>, CoreError> {
            self.list_calls.lock().unwrap().push("designers");
            Ok(page(vec![designer("d-1")]))
        }
        async fn perspective_sessions(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<ListEnvelope<PerspectiveSession>, CoreError> {
            self.list_calls.lock().unwrap().push("perspective");
            Ok(page(vec![perspective("psess-1")]))
        }
        async fn vision_clients(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<ListEnvelope<VisionClient>, CoreError> {
            self.list_calls.lock().unwrap().push("vision");
            Ok(page(vec![vision("v-1")]))
        }
        async fn terminate_perspective_session(
            &self,
            id: &str,
            message: Option<&str>,
        ) -> Result<(), CoreError> {
            self.terminates.lock().unwrap().push((
                "perspective",
                id.into(),
                message.map(str::to_string),
            ));
            Ok(())
        }
        async fn terminate_vision_client(&self, id: &str) -> Result<(), CoreError> {
            self.terminates
                .lock()
                .unwrap()
                .push(("vision", id.into(), None));
            Ok(())
        }
        async fn prune_designer(&self, id: &str) -> Result<(), CoreError> {
            self.terminates
                .lock()
                .unwrap()
                .push(("designer", id.into(), None));
            Ok(())
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
            unreachable!("not part of this double's actions")
        }
        async fn logs_download(&self) -> Result<crate::client::logs::LogDownload, CoreError> {
            unreachable!("not part of this double's actions")
        }
        async fn loggers(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<ListEnvelope<crate::client::logs::LoggerInfo>, CoreError> {
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
    }

    /// Unfiltered: all three families called and present. Filtered: ONLY
    /// the requested family is called; the others stay present-but-empty
    /// (the stable agent shape).
    #[tokio::test]
    async fn sessions_filter_calls_only_the_requested_family() {
        let rig = SessionsRig::default();
        let merged = sessions(&rig, None).await.expect("merged list");
        assert_eq!(merged.designers.len(), 1);
        assert_eq!(merged.perspective.len(), 1);
        assert_eq!(merged.vision.len(), 1);
        assert_eq!(
            *rig.list_calls.lock().unwrap(),
            vec!["designers", "perspective", "vision"]
        );

        let rig = SessionsRig::default();
        let filtered = sessions(&rig, Some(SessionType::Perspective))
            .await
            .expect("filtered list");
        assert!(filtered.designers.is_empty(), "excluded key stays present");
        assert_eq!(filtered.perspective.len(), 1);
        assert!(filtered.vision.is_empty());
        assert_eq!(
            *rig.list_calls.lock().unwrap(),
            vec!["perspective"],
            "no round-trips for excluded families"
        );

        // The JSON shape keeps all three keys (agent contract).
        let json = serde_json::to_value(&filtered).expect("serialize");
        let mut keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["designers", "perspective", "vision"]);
    }

    /// Termination maps kind → endpoint exactly: designer → prune,
    /// perspective → terminate (message rides along), vision →
    /// terminate.
    #[tokio::test]
    async fn terminate_maps_each_kind_to_its_endpoint() {
        let rig = SessionsRig::default();
        let result: TerminateResult =
            terminate_session(&rig, SessionType::Perspective, "psess-1", Some("bye"))
                .await
                .expect("perspective terminates");
        assert_eq!(result.kind, SessionType::Perspective);
        assert_eq!(result.id, "psess-1");
        assert_eq!(
            serde_json::to_value(&result).unwrap()["kind"],
            "perspective",
            "kind serializes kebab-case"
        );

        terminate_session(&rig, SessionType::Designer, "d-1", Some("ignored"))
            .await
            .expect("designer prunes (message not applicable)");
        terminate_session(&rig, SessionType::Vision, "v-1", None)
            .await
            .expect("vision terminates");
        assert_eq!(
            *rig.terminates.lock().unwrap(),
            vec![
                ("perspective", "psess-1".into(), Some("bye".into())),
                ("designer", "d-1".into(), None),
                ("vision", "v-1".into(), None),
            ]
        );
    }
}
