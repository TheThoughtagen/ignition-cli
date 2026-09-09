//! Diagnostics-bundle actions (09-05, EXT-02; 09-07 Invalid semantics)
//! — generate / status / wait / download over the wire landed in this
//! phase.
//!
//! **The wait rides the ONE poll engine** ([`crate::poll`], the
//! restart_and_wait shape): the probe polls
//! `GET /data/api/v1/diagnostics/bundle/status` and reports, in THIS
//! order —
//!
//! - `PollState::Pending("state … — still generating")` while
//!   `Generating` (`is_generating`);
//! - `Err(CoreError::BundleNotAvailable { state })` IMMEDIATELY when
//!   `is_bundle_unavailable` (`Invalid` — the captured TERMINAL
//!   steady state, 09-UAT.md Gap 3): zero further polls — only a
//!   fresh generate changes this state, so waiting is structurally
//!   futile (exit 6, `bundle_not_available`);
//! - `PollState::Done(wire)` when the state is a captured
//!   non-generating, non-unavailable state (`Valid` — terminal
//!   success);
//! - `PollState::Pending("unknown state … — still waiting")` for a
//!   state OUTSIDE the captured vocabulary (Pitfall 2's honest-
//!   unknowns rule — a state the captures never saw must not be
//!   declared terminal; it keeps polling).
//!
//! Deadline expiry is the poll engine's `CoreError::Network {
//! source: None }` convention (exit 4, `network_error` slug — NO new
//! slug) naming the subject; the last observation rides the dedicated
//! `observation` field, so a deadline the gateway ANSWERED leads with
//! "no terminal state" and never claims unreachability (09-07).
//!
//! The download applies the default naming when the caller passes no
//! `--output`: the backup/logs `.part` rename pattern — stream to
//! `ignition-diagnostics-bundle-<unix_ts>.zip.part`, rename to the
//! `Content-Disposition` basename when the gateway sends one
//! (sanitized), else the fallback. Bytes are never touched by this
//! layer (the streaming pipeline owns them).

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::client::GatewayApi;
use crate::client::diagnostics::{self};
use crate::error::CoreError;
use crate::poll::{self, PollConfig, PollState};

// The wire re-exports for the CLI/TUI render seams (the actions
// module is the import home for envelope payloads).
pub use crate::client::diagnostics::{
    BUNDLE_CAPTURED_STATES, BUNDLE_DOWNLOAD_TIMEOUT, BUNDLE_GENERATING_STATES,
    BUNDLE_UNAVAILABLE_STATES, BundleStatusWire, is_bundle_unavailable, is_generating,
};

/// `ign diagnostics bundle generate` / `status` / `wait` — the three
/// status-carrying verbs return the CAPTURED wire directly (the
/// envelope's `data.state` / `data.fileSize` are the gateway's own
/// shape; no wrapper re-keys it).
///
/// `ign diagnostics bundle download` output model — all keys always.
#[derive(Debug, Serialize)]
pub struct BundleDownloadResult {
    /// The file written (as resolved: the `-o` override, the
    /// gateway's `Content-Disposition` basename, or the
    /// `ignition-diagnostics-bundle-<unix_ts>.zip` fallback).
    pub file: String,
    /// Bytes written (== the status `fileSize` on the captures).
    pub bytes: u64,
    /// Response `Content-Type` — `application/zip;charset=utf-8` on
    /// the captures.
    pub content_type: Option<String>,
}

/// Start bundle generation — thin wrapper; the 200 body IS the fresh
/// status wire (live capture).
pub async fn bundle_generate(api: &dyn GatewayApi) -> Result<BundleStatusWire, CoreError> {
    api.bundle_generate().await
}

/// Read the bundle status — thin wrapper.
pub async fn bundle_status(api: &dyn GatewayApi) -> Result<BundleStatusWire, CoreError> {
    api.bundle_status().await
}

/// Poll the status until the bundle is ready. The captured
/// TERMINAL-unavailable states (`Invalid`) refuse IMMEDIATELY (exit
/// 6, `bundle_not_available` — no further polls, only a fresh
/// generate changes them); `Valid` is terminal success; unknown
/// states keep polling (honest unknowns); deadline → the poll
/// engine's Network-class timeout (exit 4, `network_error`) with the
/// last observation riding the dedicated field — a deadline the
/// gateway ANSWERED never claims unreachability (09-07).
pub async fn bundle_wait(
    api: &dyn GatewayApi,
    interval: Duration,
    timeout: Duration,
) -> Result<BundleStatusWire, CoreError> {
    let cfg = PollConfig {
        subject: "diagnostics bundle generation".to_string(),
        interval,
        deadline: timeout,
        ..PollConfig::default()
    };
    // The terminal wire rides the Mutex the state borrows (poll's T
    // is `()`; Mutex not Cell so the probe future is Send — the
    // 06-02 TUI spawns it). The wait_module shape verbatim.
    let mut final_wire = Mutex::new(None);
    poll::poll(cfg, &mut final_wire, |final_wire| {
        Box::pin(async {
            let wire = api.bundle_status().await?;
            let state = wire.state.as_str();
            if diagnostics::is_generating(state) {
                Ok(PollState::<()>::Pending(Some(format!(
                    "state {state:?} — still generating"
                ))))
            } else if diagnostics::is_bundle_unavailable(state) {
                // The unavailable arm MUST precede the captured-Done
                // arm (Invalid is in BUNDLE_CAPTURED_STATES too):
                // abort immediately — zero further polls, the poll
                // engine never retries a non-transient error class.
                Err(CoreError::BundleNotAvailable {
                    state: state.to_string(),
                })
            } else if diagnostics::BUNDLE_CAPTURED_STATES.contains(&state) {
                *final_wire.get_mut().expect("terminal wire") = Some(wire);
                Ok(PollState::<()>::Done(()))
            } else {
                Ok(PollState::<()>::Pending(Some(format!(
                    "unknown state {state:?} — still waiting"
                ))))
            }
        })
    })
    .await?;
    Ok(
        std::mem::take(&mut *final_wire.lock().expect("terminal wire"))
            .expect("poll returns only on Done — the wire has landed"),
    )
}

/// A filesystem-safe fallback filename strip (the backup sanitizer's
/// twin — disposition basenames are gateway-controlled, never trusted
/// into a path with separators intact).
fn sanitize_basename(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.replace(['/', '\\'], "_"))
}

/// The default bundle filename when the gateway sends no
/// `Content-Disposition` — the logs-download naming precedent
/// (timestamped, deterministic).
fn default_bundle_name(now_secs: u64) -> String {
    format!("ignition-diagnostics-bundle-{now_secs}.zip")
}

/// `ign diagnostics bundle download` — stream the ZIP to disk. With
/// `--output` the bytes land exactly there; the default naming rides
/// the `.part` rename pattern (a failed download leaves no
/// half-written impostor).
pub async fn bundle_download(
    api: &dyn GatewayApi,
    out: Option<&Path>,
) -> Result<BundleDownloadResult, CoreError> {
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default();
    if let Some(out) = out {
        let meta = api.bundle_download(out).await?;
        return Ok(BundleDownloadResult {
            file: out.display().to_string(),
            bytes: meta.bytes,
            content_type: meta.content_type,
        });
    }
    let fallback = default_bundle_name(now_secs);
    let part = PathBuf::from(format!("{fallback}.part"));
    let meta = match api.bundle_download(&part).await {
        Ok(meta) => meta,
        Err(err) => {
            let _ = std::fs::remove_file(&part); // best-effort
            return Err(err);
        }
    };
    let final_name = meta
        .filename
        .as_deref()
        .and_then(sanitize_basename)
        .unwrap_or(fallback);
    if let Err(err) = std::fs::rename(&part, &final_name) {
        let _ = std::fs::remove_file(&part); // best-effort
        return Err(CoreError::Internal(format!(
            "cannot finalize bundle {final_name}: {err}"
        )));
    }
    Ok(BundleDownloadResult {
        file: final_name,
        bytes: meta.bytes,
        content_type: meta.content_type,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;
    use std::time::Duration;

    use super::bundle_wait;
    use crate::client::GatewayApi;
    use crate::client::diagnostics::BundleStatusWire;
    use crate::error::CoreError;

    /// A scripted status rig: pops states in order; an exhausted
    /// script replays the fallback forever (the deadline path needs a
    /// never-terminal answer without an unbounded script).
    struct WaitRig {
        states: Mutex<VecDeque<&'static str>>,
        fallback: &'static str,
        calls: Mutex<usize>,
    }

    impl WaitRig {
        fn with(states: &[&'static str], fallback: &'static str) -> Self {
            Self {
                states: Mutex::new(states.iter().copied().collect()),
                fallback,
                calls: Mutex::new(0),
            }
        }

        fn calls(&self) -> usize {
            *self.calls.lock().expect("calls lock")
        }
    }

    #[async_trait::async_trait]
    impl GatewayApi for WaitRig {
        async fn bundle_generate(
            &self,
        ) -> Result<crate::client::diagnostics::BundleStatusWire, CoreError> {
            unreachable!("not part of this action")
        }
        async fn bundle_status(&self) -> Result<BundleStatusWire, CoreError> {
            let mut queue = self.states.lock().expect("states lock");
            *self.calls.lock().expect("calls lock") += 1;
            let state = queue.pop_front().unwrap_or(self.fallback);
            Ok(BundleStatusWire {
                state: state.to_string(),
                file_size: (state == "Valid").then_some(61053),
                extra: Default::default(),
            })
        }
        async fn bundle_download(
            &self,
            _out: &std::path::Path,
        ) -> Result<crate::client::projects::ExportMeta, CoreError> {
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
        async fn api_call(
            &self,
            _call: &crate::client::apicall::ApiCallRequest,
        ) -> Result<crate::client::apicall::ApiCallData, CoreError> {
            unreachable!("not part of this action")
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
    }

    /// Generating → Generating → Valid: Done on the THIRD probe (the
    /// wait flips exactly when the captured terminal state appears).
    #[tokio::test]
    async fn wait_polls_until_a_captured_terminal_state() {
        let rig = WaitRig::with(&["Generating", "Generating", "Valid"], "Generating");
        let out = bundle_wait(&rig, Duration::from_millis(1), Duration::from_secs(10))
            .await
            .expect("terminal state reached");
        assert_eq!(out.state, "Valid");
        assert_eq!(out.file_size, Some(61_053));
        assert_eq!(rig.calls(), 3, "exactly three probes");
    }

    /// Immediate terminal: the FIRST probe answers (poll runs the
    /// first probe with no initial sleep).
    #[tokio::test]
    async fn wait_returns_on_the_first_probe_when_already_terminal() {
        let rig = WaitRig::with(&["Valid"], "Valid");
        let out = bundle_wait(&rig, Duration::from_millis(1), Duration::from_secs(10))
            .await
            .expect("immediate terminal");
        assert_eq!(out.state, "Valid");
        assert_eq!(rig.calls(), 1);
    }

    /// An UNKNOWN state (outside the captured vocabulary) is NOT
    /// terminal — the wait keeps polling to the deadline, which is
    /// the poll engine's Network{source: None} convention (exit 4,
    /// `network_error` — no new slug) with the last observation (the
    /// unknown state) riding the message.
    #[tokio::test]
    async fn unknown_states_keep_polling_until_the_deadline() {
        let rig = WaitRig::with(&["Generating", "Mystery"], "Mystery");
        let err = bundle_wait(&rig, Duration::from_millis(1), Duration::from_millis(40))
            .await
            .expect_err("deadline must expire");
        assert!(
            matches!(&err, CoreError::Network { source: None, .. }),
            "deadline = Network with no transport source: {err}"
        );
        assert_eq!(err.exit_code(), 4);
        assert_eq!(err.code(), "network_error");
        let message = err.to_string();
        assert!(
            message.contains("diagnostics bundle generation"),
            "subject named: {message}"
        );
        assert!(
            message.contains("unknown state") && message.contains("Mystery"),
            "the final status rides the deadline observation: {message}"
        );
        assert!(
            !message.contains("unreachable"),
            "the gateway ANSWERED — the deadline message never claims unreachability (09-07): {message}"
        );
        assert!(
            rig.calls() > 2,
            "unknown states keep polling: {}",
            rig.calls()
        );
    }

    /// THE 09-07 Gap-3 semantics: `Invalid` is the captured TERMINAL
    /// steady state ("no current bundle") — the wait refuses
    /// IMMEDIATELY (exit 6, `bundle_not_available`) after EXACTLY the
    /// two probes, with NO deadline wait: polling cannot change this
    /// state, only a fresh generate can. The message names the
    /// observed state and the generate command.
    #[tokio::test]
    async fn invalid_state_exits_immediately_bundle_not_available() {
        let rig = WaitRig::with(&["Generating", "Invalid"], "Invalid");
        let err = bundle_wait(&rig, Duration::from_millis(1), Duration::from_secs(10))
            .await
            .expect_err("Invalid is terminal-unavailable");
        assert_eq!(err.exit_code(), 6, "target state");
        assert_eq!(err.code(), "bundle_not_available");
        assert_eq!(
            rig.calls(),
            2,
            "EXACTLY two probes — immediate exit, no further polls, no deadline wait"
        );
        let message = err.to_string();
        assert!(
            message.contains("Invalid"),
            "the observed state is named: {message}"
        );
        assert!(
            message.contains("generate"),
            "the fresh-generate fix is named: {message}"
        );
        let hint = err.hint().expect("hint required");
        assert!(
            hint.contains("ign diagnostics bundle generate"),
            "the hint names the generate command verbatim: {hint}"
        );
    }

    /// The download fallback name is deterministic and timestamped
    /// (the logs-download naming precedent).
    #[test]
    fn default_bundle_name_shape() {
        assert_eq!(
            super::default_bundle_name(1_788_748_551),
            "ignition-diagnostics-bundle-1788748551.zip"
        );
    }

    /// The sanitizer strips separators from gateway-controlled
    /// disposition names (the backup twin).
    #[test]
    fn basename_sanitizer_strips_separators() {
        assert_eq!(
            super::sanitize_basename("diag.zip").as_deref(),
            Some("diag.zip")
        );
        assert_eq!(
            super::sanitize_basename("../../etc/passwd").as_deref(),
            Some(".._.._etc_passwd")
        );
        assert_eq!(super::sanitize_basename("   "), None, "blank names nothing");
    }
}
