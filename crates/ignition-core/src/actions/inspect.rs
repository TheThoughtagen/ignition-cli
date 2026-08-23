//! Inspection actions (02-02, HLTH-01/02/07): `status`, `modules`,
//! `metrics` — serde models OUT, no printing (ARCHITECTURE.md layering:
//! the Phase-6 TUI rides this same layer).
//!
//! Error contract: `status` is a read of a HEALTHY gateway — a failed
//! sub-call is an error (exit per taxonomy), never silently degraded.
//! Fields the gateway omits serialize as absent (`skip_serializing_if`)
//! — the envelope never carries nulls for unknown state.

use serde::Serialize;

use crate::client::GatewayApi;
use crate::client::metrics::{CurrentGauges, PerformanceCharts, ThreadCounts};
use crate::client::query::ListQuery;
use crate::client::status::{DiskInfo, JavaInfo, ModuleInfo, OsInfo, OverviewLicense};
use crate::client::version::LicenseInfo;
use crate::error::CoreError;

/// `ign status` output model — gateway_info + overview + status_ping
/// merged. Declaration order = golden field order. Key names are the
/// documented contract: `gateway {name, ignition_version, edition,
/// license}`, `state`, `overview {java, os, uptime_ms, memory,
/// cpu_fraction, disk, license {state, trial_remaining_s}}` — honest
/// units (`_ms`, `_s`, `_fraction`) instead of the gateway's bare
/// `uptime`/`cpu`/`trialRemaining`.
#[derive(Debug, Serialize)]
pub struct StatusResult {
    /// Identity block from `/data/api/v1/gateway-info`.
    pub gateway: StatusGateway,
    /// Running state from the unauthenticated `/StatusPing`
    /// (`"RUNNING"` / `"STARTING"` / …).
    pub state: String,
    /// Runtime block from `/data/api/v1/overview`.
    pub overview: StatusOverview,
}

/// Identity half of [`StatusResult`].
#[derive(Debug, Serialize)]
pub struct StatusGateway {
    /// Gateway display name, when reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Version + build revision, e.g. `"8.3.6 (b2026042713)"`.
    pub ignition_version: String,
    /// Edition, when reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edition: Option<String>,
    /// License summary from gateway-info (`{mode, …}`), when reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<LicenseInfo>,
}

/// Runtime half of [`StatusResult`] — the overview fields agents and
/// humans actually read, under unit-explicit names.
#[derive(Debug, Serialize)]
pub struct StatusOverview {
    /// JVM block `{version, vendor, name}`, when reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub java: Option<JavaInfo>,
    /// OS block `{name, arch, version}`, when reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os: Option<OsInfo>,
    /// Uptime in epoch MILLISECONDS (gateway key: `uptime`).
    pub uptime_ms: i64,
    /// `[used, max]` heap bytes.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub memory: Vec<i64>,
    /// CPU utilization as a 0–1 FRACTION (gateway key: `cpu`; the
    /// `systemPerformance/currentGauges` endpoint reports PERCENT —
    /// `ign metrics` is that one's home).
    pub cpu_fraction: f64,
    /// Disk block `{total, used}` bytes, when reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk: Option<DiskInfo>,
    /// License state incl. the trial countdown, when reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<StatusLicense>,
}

/// License block of [`StatusOverview`] — `trial_remaining_s` keeps the
/// seconds unit in the KEY itself.
#[derive(Debug, Serialize)]
pub struct StatusLicense {
    /// `"trial"` / `"licensed"` / …
    pub state: String,
    /// Trial countdown in SECONDS, when reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trial_remaining_s: Option<i64>,
}

/// `ign modules` output model.
#[derive(Debug, Serialize)]
pub struct ModulesResult {
    /// The module rows (healthy or quarantined per the flag).
    pub items: Vec<ModuleInfo>,
    /// Whether the quarantined list was requested.
    pub quarantined: bool,
}

/// `ign metrics` output model — current gauges + thread counts always;
/// historic charts only under `--history`.
#[derive(Debug, Serialize)]
pub struct MetricsResult {
    /// Current CPU (percent) / heap / max-heap gauges.
    pub current: CurrentGauges,
    /// Thread execution counts.
    pub threads: ThreadCounts,
    /// Historic chart datapoints (`--history` only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<PerformanceCharts>,
}

/// Merge gateway_info + overview + status_ping into one payload. A
/// failed sub-call IS an error (exit per taxonomy) — status reads a
/// healthy gateway, it does not guess at a sick one.
pub async fn status(api: &dyn GatewayApi) -> Result<StatusResult, CoreError> {
    let info = api.gateway_info().await?;
    let overview = api.overview().await?;
    let ping = api.status_ping().await?;
    Ok(StatusResult {
        gateway: StatusGateway {
            name: info.name,
            ignition_version: info.ignition_version,
            edition: info.edition,
            license: info.license,
        },
        state: ping.state,
        overview: StatusOverview {
            java: overview.java,
            os: overview.os,
            uptime_ms: overview.uptime,
            memory: overview.memory,
            cpu_fraction: overview.cpu,
            disk: overview.disk,
            license: overview.license.map(
                |OverviewLicense {
                     state,
                     trial_remaining_s,
                     ..
                 }| {
                    StatusLicense {
                        state,
                        trial_remaining_s,
                    }
                },
            ),
        },
    })
}

/// List healthy (default) or quarantined modules — `limit = -1` (the
/// UI's "everything" convention).
pub async fn modules(api: &dyn GatewayApi, quarantined: bool) -> Result<ModulesResult, CoreError> {
    let page = api.modules(quarantined, &ListQuery::default()).await?;
    Ok(ModulesResult {
        items: page.items,
        quarantined,
    })
}

/// Current gauges + thread counts; historic charts only when asked (the
/// charts body is the heaviest of the three — default output stays lean).
pub async fn metrics(
    api: &dyn GatewayApi,
    include_history: bool,
) -> Result<MetricsResult, CoreError> {
    let current = api.metrics_current().await?;
    let threads = api.metrics_threads().await?;
    let history = if include_history {
        Some(api.metrics_historic().await?)
    } else {
        None
    };
    Ok(MetricsResult {
        current,
        threads,
        history,
    })
}

#[cfg(test)]
mod tests {
    use super::{MetricsResult, ModulesResult, StatusResult, metrics, modules, status};
    use crate::client::GatewayApi;
    use crate::client::metrics::{CurrentGauges, Datapoint, PerformanceCharts, ThreadCounts};
    use crate::client::query::{ListEnvelope, ListMetadata};
    use crate::client::status::{ModuleInfo, Overview, StatusPing};
    use crate::client::version::{GatewayInfo, LicenseInfo};
    use crate::error::CoreError;

    use std::sync::Mutex;

    /// Captured-shaped healthy-rig double (01-04 pattern: outcomes are
    /// values; `CoreError`s that need a `reqwest::Error` are constructed
    /// lazily). Records every `modules(quarantined)` flag it serves.
    struct HealthyRig {
        modules_flags: Mutex<Vec<bool>>,
    }

    fn overview_fixture() -> Overview {
        serde_json::from_value(serde_json::json!({
            "version": "8.3.6 (b2026042713)",
            "java": {"version": "17.0.11", "vendor": "Azul Systems, Inc.", "name": "OpenJDK 64-Bit Server VM"},
            "os": {"name": "Linux", "arch": "amd64", "version": "5.15.0"},
            "uptime": 338137,
            "memory": [338137088i64, 1073741824i64],
            "cpu": 0.0031,
            "disk": {"total": 62661259264i64, "used": 12272824320i64},
            "license": {"state": "trial", "trialRemaining": 7017}
        }))
        .expect("fixture overview parses")
    }

    fn gauges_fixture() -> CurrentGauges {
        serde_json::from_value(serde_json::json!({
            "cpu": 4.88, "heapMemory": 240000000i64, "maxMemory": 1073741824i64
        }))
        .expect("fixture gauges parse")
    }

    fn charts_fixture() -> PerformanceCharts {
        PerformanceCharts {
            cpu_datapoints: vec![Datapoint {
                hist_id: 1,
                timestamp: 1787346747022,
                value: 4.88,
            }],
            heap_memory_datapoints: vec![Datapoint {
                hist_id: 2,
                timestamp: 1787346747022,
                value: 240000000.0,
            }],
            non_heap_memory_datapoints: Vec::new(),
        }
    }

    #[async_trait::async_trait]
    impl GatewayApi for HealthyRig {

        async fn trial_status_wire(&self) -> Result<crate::client::trial::TrialWire, CoreError> {
            unreachable!("not part of this action")
        }
        async fn banners(&self) -> Result<crate::client::trial::BannerSet, CoreError> {
            unreachable!("not part of this action")
        }
        async fn trial_reset_wire(&self) -> Result<crate::client::trial::TrialWire, CoreError> {
            unreachable!("not part of this action")
        }
        async fn gateway_info(&self) -> Result<GatewayInfo, CoreError> {
            Ok(GatewayInfo {
                name: Some("ign-mock".into()),
                redundancy_role: Some("Independent".into()),
                edition: Some("standard".into()),
                ignition_version: "8.3.6 (b2026042713)".into(),
                jvm_version: Some("17.0.11".into()),
                license: Some(LicenseInfo {
                    mode: "Trial".into(),
                    expiration_date: Some("2026-08-24T19:00:00Z".into()),
                }),
                endpoint: None,
            })
        }
        async fn overview(&self) -> Result<Overview, CoreError> {
            Ok(overview_fixture())
        }
        async fn status_ping(&self) -> Result<StatusPing, CoreError> {
            Ok(StatusPing {
                state: "RUNNING".into(),
            })
        }
        async fn modules(
            &self,
            quarantined: bool,
            _query: &crate::client::query::ListQuery,
        ) -> Result<ListEnvelope<ModuleInfo>, CoreError> {
            self.modules_flags
                .lock()
                .expect("flags lock")
                .push(quarantined);
            let item = ModuleInfo {
                id: "com.inductiveautomation.perspective".into(),
                name: "Perspective".into(),
                version: "8.3.6".into(),
                state: Some("ACTIVE".into()),
                license_state: Some("ACTIVATED".into()),
                vendor_name: Some("Inductive Automation".into()),
                startup_time: Some("2026-08-21 22:03:29".into()),
                extra: Default::default(),
            };
            Ok(ListEnvelope {
                items: vec![item],
                metadata: ListMetadata {
                    total: 1,
                    matching: 1,
                    limit: -1,
                    offset: 0,
                },
            })
        }
        async fn metrics_current(&self) -> Result<CurrentGauges, CoreError> {
            Ok(gauges_fixture())
        }
        async fn metrics_historic(&self) -> Result<PerformanceCharts, CoreError> {
            Ok(charts_fixture())
        }
        async fn metrics_threads(&self) -> Result<ThreadCounts, CoreError> {
            Ok(ThreadCounts {
                running: 32,
                waiting: 39,
                timed_waiting: 51,
                blocked: 0,
                extra: Default::default(),
            })
        }
        async fn designers(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<ListEnvelope<crate::client::sessions::DesignerInfo>, CoreError> {
            unreachable!("not part of this double's actions")
        }
        async fn perspective_sessions(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<ListEnvelope<crate::client::sessions::PerspectiveSession>, CoreError> {
            unreachable!("not part of this double's actions")
        }
        async fn vision_clients(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<ListEnvelope<crate::client::sessions::VisionClient>, CoreError> {
            unreachable!("not part of this double's actions")
        }
        async fn terminate_perspective_session(
            &self,
            _id: &str,
            _message: Option<&str>,
        ) -> Result<(), CoreError> {
            unreachable!("not part of this double's actions")
        }
        async fn terminate_vision_client(&self, _id: &str) -> Result<(), CoreError> {
            unreachable!("not part of this double's actions")
        }
        async fn prune_designer(&self, _id: &str) -> Result<(), CoreError> {
            unreachable!("not part of this double's actions")
        }
        async fn database_connections(
            &self,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::connections::GatewayConnection>,
            CoreError,
        > {
            unreachable!("not part of this double's actions")
        }
        async fn opc_connections(
            &self,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::connections::GatewayConnection>,
            CoreError,
        > {
            unreachable!("not part of this double's actions")
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
        async fn project_resources(
            &self,
            _project: &str,
            _prefix: Option<&str>,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::resources::ResourceEntry>,
            CoreError,
        > {
            unreachable!("not part of this action")
        }
        async fn project_resource_get(
            &self,
            _project: &str,
            _path: &str,
        ) -> Result<crate::client::resources::ResourceContent, CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_resource_put(
            &self,
            _project: &str,
            _path: &str,
            _body: Vec<u8>,
            _content_type: &str,
        ) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_resource_delete(
            &self,
            _project: &str,
            _path: &str,
        ) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
    }

    /// status merges all three sources under the documented keys —
    /// `uptime_ms`/`cpu_fraction`/`trial_remaining_s` honest names.
    #[tokio::test]
    async fn status_merges_gateway_overview_and_ping() {
        let rig = HealthyRig {
            modules_flags: Mutex::new(Vec::new()),
        };
        let result: StatusResult = status(&rig).await.expect("healthy rig merges");

        // The documented data keys serialize exactly (string-level, like
        // the error-envelope golden — key order is contract). Serialized
        // FIRST: the field assertions below move out of `result`.
        let json = serde_json::to_string(&result).expect("serialize");
        assert_eq!(
            json,
            concat!(
                r#"{"gateway":{"name":"ign-mock","ignition_version":"8.3.6 (b2026042713)","edition":"standard","#,
                r#""license":{"mode":"Trial","expirationDate":"2026-08-24T19:00:00Z"}},"state":"RUNNING","#,
                r#""overview":{"java":{"version":"17.0.11","vendor":"Azul Systems, Inc.","name":"OpenJDK 64-Bit Server VM"},"#,
                r#""os":{"name":"Linux","arch":"amd64","version":"5.15.0"},"uptime_ms":338137,"memory":[338137088,1073741824],"#,
                r#""cpu_fraction":0.0031,"disk":{"total":62661259264,"used":12272824320},"#,
                r#""license":{"state":"trial","trial_remaining_s":7017}}}"#
            ),
            "data keys are the documented contract"
        );

        assert_eq!(result.gateway.ignition_version, "8.3.6 (b2026042713)");
        assert_eq!(result.gateway.name.as_deref(), Some("ign-mock"));
        assert_eq!(result.state, "RUNNING");
        assert_eq!(result.overview.uptime_ms, 338137);
        assert!((result.overview.cpu_fraction - 0.0031).abs() < f64::EPSILON);
        let license = result.overview.license.expect("license block");
        assert_eq!(license.state, "trial");
        assert_eq!(license.trial_remaining_s, Some(7017));
    }

    /// A failed sub-call is an error (exit per taxonomy) — never a
    /// degraded payload. BrokenOverview: gateway_info OK, overview 401.
    struct BrokenOverview;

    #[async_trait::async_trait]
    impl GatewayApi for BrokenOverview {

        async fn trial_status_wire(&self) -> Result<crate::client::trial::TrialWire, CoreError> {
            unreachable!("not part of this action")
        }
        async fn banners(&self) -> Result<crate::client::trial::BannerSet, CoreError> {
            unreachable!("not part of this action")
        }
        async fn trial_reset_wire(&self) -> Result<crate::client::trial::TrialWire, CoreError> {
            unreachable!("not part of this action")
        }
        async fn gateway_info(&self) -> Result<GatewayInfo, CoreError> {
            Ok(GatewayInfo {
                name: None,
                redundancy_role: None,
                edition: None,
                ignition_version: "8.3.6 (b2026042713)".into(),
                jvm_version: None,
                license: None,
                endpoint: None,
            })
        }
        async fn overview(&self) -> Result<Overview, CoreError> {
            Err(CoreError::Auth {
                status: 401,
                endpoint: Some("http://gw.example.com/data/api/v1/overview".into()),
            })
        }
        async fn status_ping(&self) -> Result<StatusPing, CoreError> {
            unreachable!("status() must fail at overview() before pinging")
        }
        async fn modules(
            &self,
            _quarantined: bool,
            _query: &crate::client::query::ListQuery,
        ) -> Result<ListEnvelope<ModuleInfo>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn metrics_current(&self) -> Result<CurrentGauges, CoreError> {
            unreachable!("not part of this action")
        }
        async fn metrics_historic(&self) -> Result<PerformanceCharts, CoreError> {
            unreachable!("not part of this action")
        }
        async fn metrics_threads(&self) -> Result<ThreadCounts, CoreError> {
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
        async fn project_resources(
            &self,
            _project: &str,
            _prefix: Option<&str>,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::resources::ResourceEntry>,
            CoreError,
        > {
            unreachable!("not part of this action")
        }
        async fn project_resource_get(
            &self,
            _project: &str,
            _path: &str,
        ) -> Result<crate::client::resources::ResourceContent, CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_resource_put(
            &self,
            _project: &str,
            _path: &str,
            _body: Vec<u8>,
            _content_type: &str,
        ) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_resource_delete(
            &self,
            _project: &str,
            _path: &str,
        ) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
    }

    #[tokio::test]
    async fn status_propagates_subcall_errors() {
        let err = status(&BrokenOverview).await.expect_err("401 propagates");
        assert!(matches!(&err, CoreError::Auth { status: 401, .. }));
        assert_eq!(err.exit_code(), 5);
    }

    /// modules() forwards the quarantined flag and wraps the envelope.
    #[tokio::test]
    async fn modules_forwards_the_quarantined_flag() {
        let rig = HealthyRig {
            modules_flags: Mutex::new(Vec::new()),
        };
        let healthy: ModulesResult = modules(&rig, false).await.expect("healthy list");
        let quarantined: ModulesResult = modules(&rig, true).await.expect("quarantined list");
        assert!(!healthy.quarantined && healthy.items.len() == 1);
        assert!(quarantined.quarantined);
        assert_eq!(
            *rig.modules_flags.lock().expect("flags lock"),
            vec![false, true],
            "the flag reached the client seam in call order"
        );
    }

    /// metrics() defaults to current+threads (no charts call); with
    /// history the charts ride along.
    #[tokio::test]
    async fn metrics_history_is_opt_in() {
        let rig = HealthyRig {
            modules_flags: Mutex::new(Vec::new()),
        };
        let lean: MetricsResult = metrics(&rig, false).await.expect("lean metrics");
        assert!(lean.history.is_none());
        assert_eq!(lean.threads.running, 32);
        assert!((lean.current.cpu - 4.88).abs() < f64::EPSILON);

        let full: MetricsResult = metrics(&rig, true).await.expect("full metrics");
        let charts = full.history.expect("charts included");
        assert_eq!(charts.cpu_datapoints.len(), 1);
    }
}
