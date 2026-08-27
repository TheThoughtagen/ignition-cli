//! The dashboard's interval refresh worker (06-02) — research Pattern 2:
//! compose the existing action fns AS-IS (the key_link contract —
//! `workers/*` never re-implement a client call), per-call `.ok()` so
//! one failing endpoint degrades its PANEL, never the dashboard.
//!
//! A dead gateway therefore yields four honest panel errors — the UI
//! is never frozen or blank (must-have truth #2).

use std::sync::Arc;
use std::time::Duration;

use ignition_core::actions::{inspect, sessions};
use ignition_core::client::ReqwestGatewayApi;
use tokio::sync::{mpsc, watch};

use crate::event::AppEvent;
use crate::state::AppState;

/// The LOCKED refresh period — panels update every 5 s with zero
/// keystrokes (must-have truth #1).
pub const REFRESH_PERIOD: Duration = Duration::from_secs(5);

/// One dashboard refresh: per-panel `Option<T>` result + per-panel
/// error string. Every panel renders its own Loading/Loaded/Error state
/// from exactly these fields.
#[derive(Debug, Default)]
pub struct Snapshot {
    /// `ign status` (gateway identity, state, runtime, license).
    pub status: Option<inspect::StatusResult>,
    /// Why the status panel errored, when it did.
    pub status_error: Option<String>,
    /// `ign modules` (healthy list).
    pub modules: Option<inspect::ModulesResult>,
    /// Why the modules panel errored, when it did.
    pub modules_error: Option<String>,
    /// `ign metrics` (current gauges + thread counts).
    pub metrics: Option<inspect::MetricsResult>,
    /// Why the metrics panel errored, when it did.
    pub metrics_error: Option<String>,
    /// `ign sessions` (all three families merged).
    pub sessions: Option<sessions::SessionsResult>,
    /// Why the sessions panel errored, when it did.
    pub sessions_error: Option<String>,
}
/// Split one action result into its per-panel (data, error) pair — the
/// `.ok()` degradation, error message included.
fn degrade<T>(result: Result<T, ignition_core::error::CoreError>) -> (Option<T>, Option<String>) {
    match result {
        Ok(value) => (Some(value), None),
        Err(err) => (None, Some(err.to_string())),
    }
}

/// Compose the four dashboard reads CONCURRENTLY, each degraded
/// independently — the per-panel contract. Takes the worker's client
/// handle by shared ref; the action fns are the free fns over
/// `&dyn GatewayApi` (the key_link contract — `(&*api)` at every call).
pub async fn snapshot(api: &Arc<ReqwestGatewayApi>) -> Snapshot {
    let (status, modules, metrics, sessions) = futures_util::future::join4(
        inspect::status(&**api),
        inspect::modules(&**api, false),
        inspect::metrics(&**api, false),
        sessions::sessions(&**api, None),
    )
    .await;
    let (status, status_error) = degrade(status);
    let (modules, modules_error) = degrade(modules);
    let (metrics, metrics_error) = degrade(metrics);
    let (sessions, sessions_error) = degrade(sessions);
    Snapshot {
        status,
        status_error,
        modules,
        modules_error,
        metrics,
        metrics_error,
        sessions,
        sessions_error,
    }
}

/// The interval worker: one snapshot per `period`, sent as a
/// [`AppEvent::Refresh`] stamped with the spawn-era. `select!`s against
/// the shutdown watch so profile-switch teardown stops it; exits when
/// the loop is gone (`tx.send` errors).
pub async fn refresh_worker(
    api: Arc<ReqwestGatewayApi>,
    tx: mpsc::UnboundedSender<AppEvent>,
    mut shutdown: watch::Receiver<bool>,
    era: u64,
    period: Duration,
) {
    let mut tick = tokio::time::interval(period);
    // A slow snapshot (dead gateway, 30 s client timeout) must not
    // queue a burst of catch-up ticks — space refreshes ≥ period.
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = tick.tick() => {
                let snap = snapshot(&api).await;
                if tx.send(AppEvent::Refresh { era, snapshot: Box::new(snap) }).is_err() {
                    return; // the loop is gone — stop.
                }
            }
            // `changed()` also resolves Err when the sender is dropped
            // without a signal — both mean stop.
            _ = shutdown.changed() => return,
        }
    }
}

/// The `r` keystroke: one immediate refresh. The `dashboard_busy` guard
/// refuses to stack on an in-flight refresh; the flag clears when the
/// next (current-era) `Refresh` event lands.
///
/// Outside a tokio runtime (state-machine unit tests) the state
/// transition stands alone and nothing spawns — `update` must never
/// panic by construction.
pub fn spawn_refresh_once(state: &mut AppState) {
    if state.dashboard.busy {
        return; // the busy guard — keystrokes cannot stack refreshes
    }
    let Some(client) = state.client.as_ref().map(|handle| handle.0.clone()) else {
        return;
    };
    let Some(tx) = state.events_tx.clone() else {
        return;
    };
    state.dashboard.busy = true;
    let era = state.era;
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            let snap = snapshot(&client).await;
            let _ = tx.send(AppEvent::Refresh {
                era,
                snapshot: Box::new(snap),
            });
        });
    }
}

/// Cross-module test fixtures (cfg(test)): the wiremock gateway the
/// composition + render proofs share.
#[cfg(test)]
pub(crate) mod test_support {
    /// Mount the eight read endpoints the four dashboard actions hit,
    /// with one KNOWN row each (module `persp`, perspective session
    /// `ps-1`) — the shared fixture for composition + render proofs.
    pub(crate) async fn mount_gateway(server: &wiremock::MockServer) {
        use wiremock::ResponseTemplate;
        macro_rules! get {
            ($path:expr, $body:expr) => {
                wiremock::Mock::given(wiremock::matchers::method("GET"))
                    .and(wiremock::matchers::path($path))
                    .respond_with(ResponseTemplate::new(200).set_body_json($body))
                    .expect(1..)
                    .mount(server)
                    .await;
            };
        }
        get!(
            "/data/api/v1/gateway-info",
            serde_json::json!({
                "name": "whiskeyhouse",
                "edition": "standard",
                "ignitionVersion": "8.3.6 (b2026042713)",
                "license": {"mode": "trial"}
            })
        );
        get!(
            "/data/api/v1/overview",
            serde_json::json!({
                "version": "8.3.6 (b2026042713)",
                "uptime": 338_137i64,
                "memory": [338_137_088i64, 1_073_741_824i64],
                "cpu": 0.0031,
                "license": {"state": "trial", "trialRemaining": 7017}
            })
        );
        get!("/StatusPing", serde_json::json!({"state": "RUNNING"}));
        get!(
            "/data/api/v1/modules/healthy",
            serde_json::json!({
                "items": [{
                    "id": "com.inductiveautomation.perspective",
                    "name": "Perspective",
                    "version": "8.3.6",
                    "state": "ACTIVE",
                    "licenseState": "Active"
                }],
                "metadata": {"total": 1}
            })
        );
        get!(
            "/data/api/v1/systemPerformance/currentGauges",
            serde_json::json!({"cpu": 4.88, "heapMemory": 240_000_000i64, "maxMemory": 1_073_741_824i64})
        );
        get!(
            "/data/api/v1/systemPerformance/threads",
            serde_json::json!({"running": 32, "waiting": 39, "timedWaiting": 51, "blocked": 0})
        );
        get!(
            "/data/api/v1/designers",
            serde_json::json!({"items": [], "metadata": {"total": 0}})
        );
        get!(
            "/data/perspective/api/v1/sessions/",
            serde_json::json!({
                "items": [{
                    "id": "ps-1",
                    "username": "admin",
                    "project": "whiskeyhouse",
                    "authorized": true
                }],
                "metadata": {"total": 1}
            })
        );
        get!(
            "/data/vision/api/v1/clients",
            serde_json::json!({"items": [], "metadata": {"total": 0}})
        );
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::mount_gateway;
    use super::{REFRESH_PERIOD, refresh_worker, snapshot, spawn_refresh_once};
    use crate::event::AppEvent;
    use crate::state::AppState;

    /// The composition proof: `snapshot` runs the four action fns
    /// against a wiremock gateway and every panel populates.
    #[tokio::test]
    async fn snapshot_composes_all_four_panels() {
        let server = wiremock::MockServer::start().await;
        mount_gateway(&server).await;
        let api = std::sync::Arc::new(ignition_core::client::ReqwestGatewayApi::for_tests(
            &server.uri(),
            None,
        ));

        let snap = snapshot(&api).await;

        let status = snap.status.expect("status panel populated");
        assert_eq!(status.gateway.ignition_version, "8.3.6 (b2026042713)");
        assert_eq!(status.state, "RUNNING");
        assert!(snap.status_error.is_none());

        let modules = snap.modules.expect("modules panel populated");
        assert_eq!(modules.items.len(), 1);
        assert_eq!(modules.items[0].id, "com.inductiveautomation.perspective");
        assert!(snap.modules_error.is_none());

        let metrics = snap.metrics.expect("metrics panel populated");
        assert_eq!(metrics.current.cpu, 4.88);
        assert!(snap.metrics_error.is_none());

        let sessions = snap.sessions.as_ref().unwrap_or_else(|| {
            panic!(
                "sessions panel populated — error: {:?}",
                snap.sessions_error
            )
        });
        assert_eq!(sessions.perspective.len(), 1);
        assert!(snap.sessions_error.is_none());
    }

    /// Per-panel degradation: a dead gateway errors every panel HONESTLY
    /// (data None + error string set) — never a panic, never a blank.
    #[tokio::test]
    async fn dead_gateway_degrades_every_panel_with_errors() {
        // Nothing listens here — every call fails fast.
        let api = std::sync::Arc::new(ignition_core::client::ReqwestGatewayApi::for_tests(
            "http://127.0.0.1:1/",
            None,
        ));
        let snap = snapshot(&api).await;
        assert!(snap.status.is_none() && snap.status_error.is_some());
        assert!(snap.modules.is_none() && snap.modules_error.is_some());
        assert!(snap.metrics.is_none() && snap.metrics_error.is_some());
        assert!(snap.sessions.is_none() && snap.sessions_error.is_some());
    }

    /// The worker loop: first tick fires immediately (interval
    /// semantics), the event carries the spawn-era, and the shutdown
    /// watch TERMINATES the loop.
    #[tokio::test]
    async fn refresh_worker_reports_and_terminates_on_shutdown() {
        let api = std::sync::Arc::new(ignition_core::client::ReqwestGatewayApi::for_tests(
            "http://127.0.0.1:1/",
            None,
        ));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        let worker = tokio::spawn(refresh_worker(
            api.clone(),
            tx,
            shutdown_rx,
            7,
            REFRESH_PERIOD,
        ));

        // First tick is immediate: a Refresh event arrives (all-error
        // snapshot against the dead endpoint) stamped with era 7.
        let event = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
            .await
            .expect("first refresh within 5s")
            .expect("worker holds the sender");
        match event {
            AppEvent::Refresh { era, snapshot } => {
                assert_eq!(era, 7);
                assert!(snapshot.status.is_none());
                assert!(snapshot.status_error.is_some());
            }
            other => panic!("expected Refresh, got {other:?}"),
        }

        // Shutdown stops the worker promptly (not at the next tick).
        shutdown_tx.send(true).expect("worker holds the receiver");
        tokio::time::timeout(std::time::Duration::from_secs(5), worker)
            .await
            .expect("worker exits on shutdown")
            .expect("worker task not cancelled");
    }

    /// The busy guard: `spawn_refresh_once` refuses to stack while busy,
    /// and works as a pure state transition outside a runtime.
    #[test]
    fn manual_refresh_is_busy_guarded() {
        let mut state = AppState::new();
        assert!(!state.dashboard.busy);

        // Rails present (client + sender): the transition runs.
        state.client = Some(crate::state::ClientHandle(std::sync::Arc::new(
            ignition_core::client::ReqwestGatewayApi::for_tests("http://127.0.0.1:1/", None),
        )));
        state.events_tx = Some(tokio::sync::mpsc::unbounded_channel().0);
        spawn_refresh_once(&mut state);
        assert!(state.dashboard.busy, "first refresh marks busy");

        // Busy → refused (no stack, no second transition).
        spawn_refresh_once(&mut state);
        assert!(state.dashboard.busy);
    }
}
