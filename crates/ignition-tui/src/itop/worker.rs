//! The itop sample worker — one composed gateway sample per interval,
//! the cockpit's refresh-worker pattern (Pattern 2) scaled to itop's
//! six-section world.
//!
//! The key_link contract holds: the worker COMPOSES the existing
//! action fns AS-IS (`(&*api)` at every call) and never re-implements
//! a client call. Per-call degradation: one failing endpoint degrades
//! its SECTION, never the whole sample — a dead gateway yields six
//! honest section errors, never a frozen or blank table.

use std::sync::Arc;
use std::time::Duration;

use ignition_core::actions::{connections, inspect, sessions, tags};
use ignition_core::client::ReqwestGatewayApi;
use ignition_core::error::CoreError;
use tokio::sync::{mpsc, watch};

use crate::itop::event::TopEvent;
use crate::itop::state::{KillTarget, Modal, TopState};

/// One composed gateway sample — per-section `Option<T>` data + error
/// string. Every section renders its own Loading/Loaded/Error state
/// from exactly these fields (the dashboard Snapshot convention).
#[derive(Debug, Default)]
pub struct TopSample {
    /// `ign status` — identity (name/version/edition), RUNNING state,
    /// uptime, memory pair, disk, license incl. trial countdown.
    pub status: Option<inspect::StatusResult>,
    /// Why the status section errored, when it did.
    pub status_error: Option<String>,
    /// `ign metrics` — CPU PERCENT gauges + thread counts.
    pub metrics: Option<inspect::MetricsResult>,
    /// Why the metrics section errored, when it did.
    pub metrics_error: Option<String>,
    /// `ign modules` (healthy list).
    pub modules: Option<inspect::ModulesResult>,
    /// Why the modules section errored, when it did.
    pub modules_error: Option<String>,
    /// `ign sessions` — all three families merged.
    pub sessions: Option<sessions::SessionsResult>,
    /// Why the sessions section errored, when it did.
    pub sessions_error: Option<String>,
    /// `ign connections` — DB + OPC.
    pub connections: Option<connections::ConnectionsResult>,
    /// Why the connections section errored, when it did.
    pub connections_error: Option<String>,
    /// `ign tags provider list`.
    pub providers: Option<tags::TagProvidersResult>,
    /// Why the providers section errored, when it did.
    pub providers_error: Option<String>,
    /// The gateway's OWN performance history (`metrics_historic`) —
    /// set ONLY on a worker's first sample (the bootstrap seed: the
    /// sparklines start full from the gateway's ring instead of
    /// growing from zero). Subsequent samples ride the local rings.
    pub history: Option<ignition_core::client::metrics::PerformanceCharts>,
}

/// Split one action result into its per-section (data, error) pair —
/// the `.ok()` degradation, error message included (the refresh
/// worker's `degrade` convention).
fn degrade<T>(result: Result<T, CoreError>) -> (Option<T>, Option<String>) {
    match result {
        Ok(value) => (Some(value), None),
        Err(err) => (None, Some(err.to_string())),
    }
}

/// Compose the six reads CONCURRENTLY, each degraded independently —
/// the per-section contract. Takes the worker's client handle by
/// shared ref; the action fns are the free fns over `&dyn GatewayApi`
/// (the key_link contract at every call). `seed_history` additionally
/// fetches the gateway's OWN performance history (`metrics_historic`)
/// — the bootstrap seed so the sparklines start full; the worker sets
/// it on its FIRST tick only (the local rings continue from there).
pub async fn sample(api: &Arc<ReqwestGatewayApi>, seed_history: bool) -> TopSample {
    if seed_history {
        // Seven reads — the bootstrap sample carries the history seed.
        let (status, metrics, modules, sessions, connections, providers, history) = tokio::join!(
            inspect::status(&**api),
            inspect::metrics(&**api, false),
            inspect::modules(&**api, false),
            sessions::sessions(&**api, None),
            connections::connections(&**api, None),
            tags::tag_provider_list(&**api),
            inspect::metrics(&**api, true), // --history: the charts seed
        );
        let history = history.ok().and_then(|m| m.history);
        compose(
            status,
            metrics,
            modules,
            sessions,
            connections,
            providers,
            history,
        )
    } else {
        let (status, metrics, modules, sessions, connections, providers) = tokio::join!(
            inspect::status(&**api),
            inspect::metrics(&**api, false),
            inspect::modules(&**api, false),
            sessions::sessions(&**api, None),
            connections::connections(&**api, None),
            tags::tag_provider_list(&**api),
        );
        compose(
            status,
            metrics,
            modules,
            sessions,
            connections,
            providers,
            None,
        )
    }
}

/// Bundle the degraded section pairs into a sample (the two `sample`
/// arms share this constructor; the history seed differs).
#[allow(clippy::too_many_arguments)]
fn compose(
    status: Result<inspect::StatusResult, CoreError>,
    metrics: Result<inspect::MetricsResult, CoreError>,
    modules: Result<inspect::ModulesResult, CoreError>,
    sessions: Result<sessions::SessionsResult, CoreError>,
    connections: Result<connections::ConnectionsResult, CoreError>,
    providers: Result<tags::TagProvidersResult, CoreError>,
    history: Option<ignition_core::client::metrics::PerformanceCharts>,
) -> TopSample {
    let (status, status_error) = degrade(status);
    let (metrics, metrics_error) = degrade(metrics);
    let (modules, modules_error) = degrade(modules);
    let (sessions, sessions_error) = degrade(sessions);
    let (connections, connections_error) = degrade(connections);
    let (providers, providers_error) = degrade(providers);
    TopSample {
        status,
        status_error,
        metrics,
        metrics_error,
        modules,
        modules_error,
        sessions,
        sessions_error,
        connections,
        connections_error,
        providers,
        providers_error,
        history,
    }
}

/// The interval worker: one sample per `period`, sent as
/// [`TopEvent::Sample`] stamped with the spawn-era. `select!`s against
/// the shutdown watch so an interval-change respawn stops it. A
/// `true` on the PAUSED watch makes a tick a no-op (pause skips
/// SAMPLING, not the worker — resume needs no respawn, the cockpit's
/// profile-switch respawn machinery stays interval-only).
pub async fn sample_worker(
    api: Arc<ReqwestGatewayApi>,
    tx: mpsc::UnboundedSender<TopEvent>,
    mut shutdown: watch::Receiver<bool>,
    paused: watch::Receiver<bool>,
    era: u64,
    period: Duration,
) {
    let mut tick = tokio::time::interval(period);
    // A slow sample (dead gateway, 30 s client timeout) must not queue
    // a burst of catch-up ticks — space samples ≥ period.
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // The FIRST sample of this worker's life seeds the sparklines from
    // the gateway's own performance history (charts bootstrap); after
    // it, the local rings continue at itop's cadence.
    let mut first_tick = true;
    loop {
        tokio::select! {
            _ = tick.tick() => {
                if *paused.borrow() {
                    continue; // paused — the tick burns, the gateway rests
                }
                let snap = sample(&api, first_tick).await;
                first_tick = false;
                if tx.send(TopEvent::Sample { era, sample: Box::new(snap) }).is_err() {
                    return; // the loop is gone — stop.
                }
            }
            // `changed()` also resolves Err when the sender is dropped
            // without a signal — both mean stop.
            _ = shutdown.changed() => return,
        }
    }
}

/// Spawn the sample worker for the CURRENT world: a fresh shutdown
/// watch, a fresh era, the state's client + rail + cadence + pause
/// receiver. The interval-change respawn path signals the OLD shutdown
/// before calling here (update owns that ordering — the profile-switch
/// precedent).
///
/// Outside a tokio runtime (unit tests) the rails + era transition
/// stand alone and nothing spawns.
pub fn spawn_sample_worker(state: &mut TopState) {
    let Some(client) = state.client.as_ref().map(|api| api.clone()) else {
        return;
    };
    let Some(tx) = state.events_tx.clone() else {
        return;
    };
    let Some(paused_tx) = state.paused_tx.as_ref() else {
        return;
    };
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    state.sample_shutdown = Some(shutdown_tx);
    state.era += 1;
    let era = state.era;
    let paused_rx = paused_tx.subscribe();
    let period = Duration::from_secs(state.interval_secs);
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(sample_worker(
            client,
            tx,
            shutdown_rx,
            paused_rx,
            era,
            period,
        ));
    }
}

/// The one-shot sample (the `r` keystroke): immediate, busy-guarded —
/// keystrokes cannot stack on an in-flight sample; the guard clears
/// when the next current-era Sample lands (the dashboard's
/// `spawn_refresh_once` convention).
///
/// Outside a tokio runtime the state transition stands alone and
/// nothing spawns.
pub fn spawn_sample_once(state: &mut TopState) {
    if state.refresh_busy {
        return; // the busy guard
    }
    let Some(client) = state.client.as_ref().map(|api| api.clone()) else {
        return;
    };
    let Some(tx) = state.events_tx.clone() else {
        return;
    };
    state.refresh_busy = true;
    let era = state.era;
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            let snap = sample(&client, false).await;
            let _ = tx.send(TopEvent::Sample {
                era,
                sample: Box::new(snap),
            });
        });
    }
}

/// Spawn the session-terminate op (the confirmed kill): one action
/// call, the result rendered into the status line by the `Killed`
/// event (the cockpit's ActionDone one-mechanism display, scoped to a
/// line). The `--message` prompt is deliberately absent — itop's kill
/// is an ops gesture, not a notification channel.
///
/// Outside a tokio runtime nothing spawns (the state transition still
/// ran — the modal closed on arming).
pub fn spawn_kill(state: &mut TopState, target: KillTarget) {
    let Some(client) = state.client.as_ref().map(|api| api.clone()) else {
        return;
    };
    let Some(tx) = state.events_tx.clone() else {
        return;
    };
    let era = state.era;
    let label = target.label.clone();
    state.status_msg = Some((format!("terminating {label}…"), false));
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            let result = sessions::terminate_session(&*client, target.kind, &target.id, None).await;
            let rendered = match result {
                Ok(done) => Ok(format!("terminated {} ({})", done.id, done.kind)),
                Err(err) => Err(err.to_string()),
            };
            let _ = tx.send(TopEvent::Killed {
                era,
                label,
                result: rendered,
            });
        });
    }
}

/// The scriptExec diagnostic probe's gateway-side code — a JMX
/// snapshot through gateway-scope Jython (`java.lang.management` —
/// review fix: the factory lives in the management subpackage, not
/// `java.lang`; the previous import raised NameError before any row
/// was produced): thread pool internals, JVM
/// memory pools, and class-loader counts that NO REST endpoint
/// exposes. Every value is defensive (`-1` when a bean is denied by
/// the security manager), so the probe degrades per-ROW, never fails
/// whole. `_result` is the route's result carrier (the 05-01 route
/// contract).
pub const SCRIPT_PROBE_CODE: &str = r#"
from java.lang.management import ManagementFactory
_rt = ManagementFactory.getRuntimeMXBean()
_mem = ManagementFactory.getMemoryMXBean()
_th = ManagementFactory.getThreadMXBean()
_cl = ManagementFactory.getClassLoadingMXBean()
_result = {
    'jvmUptimeMs': _rt.getUptime(),
    'heapUsed': _mem.getHeapMemoryUsage().getUsed(),
    'heapCommitted': _mem.getHeapMemoryUsage().getCommitted(),
    'nonHeapUsed': _mem.getNonHeapMemoryUsage().getUsed(),
    'threadsLive': _th.getThreadCount(),
    'threadsPeak': _th.getPeakThreadCount(),
    'threadsDaemon': _th.getDaemonThreadCount(),
    'classesLoaded': _cl.getTotalLoadedClassCount(),
    'classesUnloaded': _cl.getUnloadedClassCount(),
}
"#;

/// Spawn the scriptExec diagnostic probe (the `e` keystroke): resolve
/// the profile's stored webdev_secret (the SAME structural gate as
/// `ign script run` — no persisted secret renders the honest refusal
/// INLINE with zero HTTP, even outside a runtime), then execute
/// [`SCRIPT_PROBE_CODE`] through the deployed route. The
/// [`TopEvent::ScriptProbe`] result opens the probe modal.
pub fn spawn_script_probe(state: &mut TopState) {
    if state.probe_busy {
        return; // the busy guard — probes cannot stack
    }
    let Some(client) = state.client.as_ref().map(|api| api.clone()) else {
        return;
    };
    let Some(tx) = state.events_tx.clone() else {
        return;
    };
    let Some(profile_name) = state.profile_name.clone() else {
        return;
    };
    // THE structural gate, inline and synchronous: loading the config
    // and finding no webdev_secret is the `ign webdev deploy
    // --with-script-exec`-was-never-run case — the same zero-HTTP
    // refusal script_run raises, rendered as the probe modal directly.
    // (script_run re-checks the gate itself; this inline copy exists
    // so the honest refusal needs no runtime and no task spawn.)
    let config = match ignition_core::config::load(&ignition_core::config::config_path()) {
        Ok(config) => config,
        Err(err) => {
            let message = err.to_string();
            state.script_result = Some(Err(message.clone()));
            state.modal = Some(Modal::ScriptProbe {
                result: Err(message),
            });
            return;
        }
    };
    let has_secret = config
        .profiles
        .get(&profile_name)
        .is_some_and(|profile| profile.webdev_secret.is_some());
    if !has_secret {
        let refusal = format!(
            "scriptExec is not configured for profile {profile_name:?} — run \
             `ign webdev deploy --with-script-exec` to install the diagnostic \
             route (and its secret) first"
        );
        state.script_result = Some(Err(refusal.clone()));
        state.modal = Some(Modal::ScriptProbe {
            result: Err(refusal),
        });
        return;
    }
    state.probe_busy = true;
    state.status_msg = Some(("running scriptExec probe…".into(), false));
    let era = state.era;
    let project = "ign-cli".to_string(); // the CLI-owned route project
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            let result = ignition_core::actions::script::script_run(
                &*client,
                &config,
                &profile_name,
                &project,
                SCRIPT_PROBE_CODE,
            )
            .await;
            let _ = tx.send(TopEvent::ScriptProbe {
                era,
                result: match result {
                    Ok(done) => Ok(Box::new(done)),
                    Err(err) => Err(err.to_string()),
                },
            });
        });
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    /// Mount the twelve read endpoints the six action fns hit, with
    /// one KNOWN row per family — the shared fixture for the
    /// composition and render proofs (the refresh worker's fixture
    /// plus connections and providers).
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
                "disk": {"total": 62_661_259_264i64, "used": 12_272_824_320i64},
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
            "/data/api/v1/systemPerformance/charts",
            serde_json::json!({
                "cpuChartDatapoints": [
                    {"histId": 1, "timestamp": 1787346747022i64, "value": 3.1},
                    {"histId": 2, "timestamp": 1787346807022i64, "value": 4.4}
                ],
                "memoryChartDatapoints": {
                    "heapMemoryDatapoints": [
                        {"histId": 3, "timestamp": 1787346747022i64, "value": 230_000_000.0}
                    ],
                    "nonHeapMemoryDatapoints": [
                        {"histId": 4, "timestamp": 1787346747022i64, "value": 52_000_000.0}
                    ]
                }
            })
        );
        get!(
            "/data/api/v1/designers",
            serde_json::json!({
                "items": [{
                    "id": "d-1",
                    "user": "admin",
                    "project": "whiskeyhouse",
                    "address": "10.0.0.9:52526",
                    "uptime": 120_000i64
                }],
                "metadata": {"total": 1}
            })
        );
        get!(
            "/data/perspective/api/v1/sessions/",
            serde_json::json!({
                "items": [{
                    "id": "ps-1",
                    "username": "admin",
                    "project": "whiskeyhouse",
                    "authorized": true,
                    "activePages": 2
                }],
                "metadata": {"total": 1}
            })
        );
        get!(
            "/data/vision/api/v1/clients",
            serde_json::json!({"items": [], "metadata": {"total": 0}})
        );
        get!(
            "/data/api/v1/resources/list/ignition/database-connection",
            serde_json::json!({
                "items": [{
                    "name": "mysql-batch",
                    "enabled": true,
                    "healthchecks": {"status": "HEALTHY"}
                }],
                "metadata": {"total": 1}
            })
        );
        get!(
            "/data/api/v1/resources/list/ignition/opc-connection",
            serde_json::json!({
                "items": [{
                    "name": "plc-main",
                    "enabled": true,
                    "healthchecks": {"status": "HEALTHY"}
                }],
                "metadata": {"total": 1}
            })
        );
        get!(
            "/data/api/v1/resources/list/ignition/tag-provider",
            serde_json::json!({
                "items": [{
                    "name": "default",
                    "enabled": true,
                    "metrics": {"tagCount": 12341}
                }],
                "metadata": {"total": 1}
            })
        );
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::mount_gateway;
    use super::{TopSample, sample, sample_worker, spawn_sample_once, spawn_sample_worker};
    use crate::itop::event::TopEvent;
    use crate::itop::state::TopState;

    /// The composition proof: `sample` runs the six action fns against
    /// a wiremock gateway and every section populates.
    #[tokio::test]
    async fn sample_composes_all_six_sections() {
        let server = wiremock::MockServer::start().await;
        mount_gateway(&server).await;
        let api = std::sync::Arc::new(ignition_core::client::ReqwestGatewayApi::for_tests(
            &server.uri(),
            None,
        ));

        let snap = sample(&api, true).await;

        // The history seed: the bootstrap sample carries the gateway's
        // OWN charts (cpu/heap/non-heap datapoints) so the sparklines
        // open full.
        let history = snap.history.as_ref().expect("history seed populated");
        assert_eq!(history.cpu_datapoints.len(), 2);
        assert_eq!(history.non_heap_memory_datapoints.len(), 1);

        let status = snap.status.expect("status section populated");
        assert_eq!(status.gateway.ignition_version, "8.3.6 (b2026042713)");
        assert_eq!(status.state, "RUNNING");
        assert!(snap.status_error.is_none());

        let metrics = snap.metrics.expect("metrics section populated");
        assert_eq!(metrics.current.cpu, 4.88);
        assert_eq!(metrics.threads.running, 32);
        assert!(snap.metrics_error.is_none());

        let modules = snap.modules.expect("modules section populated");
        assert_eq!(modules.items.len(), 1);
        assert!(snap.modules_error.is_none());

        let sessions = snap.sessions.expect("sessions section populated");
        assert_eq!(sessions.designers.len(), 1);
        assert_eq!(sessions.perspective.len(), 1);
        assert!(snap.sessions_error.is_none());

        let connections = snap.connections.expect("connections section populated");
        assert_eq!(connections.database.len(), 1);
        assert_eq!(connections.opc.len(), 1);
        assert!(snap.connections_error.is_none());

        let providers = snap.providers.expect("providers section populated");
        assert_eq!(providers.providers.len(), 1);
        assert!(snap.providers_error.is_none());
    }

    /// Per-section degradation: a dead gateway errors every section
    /// HONESTLY (data None + error string set) — never a panic, never
    /// a blank.
    #[tokio::test]
    async fn dead_gateway_degrades_every_section_with_errors() {
        // Nothing listens here — every call fails fast.
        let api = std::sync::Arc::new(ignition_core::client::ReqwestGatewayApi::for_tests(
            "http://127.0.0.1:1/",
            None,
        ));
        let snap = sample(&api, true).await;
        assert!(snap.status.is_none() && snap.status_error.is_some());
        assert!(snap.metrics.is_none() && snap.metrics_error.is_some());
        assert!(snap.modules.is_none() && snap.modules_error.is_some());
        assert!(snap.sessions.is_none() && snap.sessions_error.is_some());
        assert!(snap.connections.is_none() && snap.connections_error.is_some());
        assert!(snap.providers.is_none() && snap.providers_error.is_some());
    }

    /// The worker loop: first tick fires immediately, the event
    /// carries the spawn-era, and the shutdown watch TERMINATES the
    /// loop.
    #[tokio::test]
    async fn sample_worker_reports_and_terminates_on_shutdown() {
        let api = std::sync::Arc::new(ignition_core::client::ReqwestGatewayApi::for_tests(
            "http://127.0.0.1:1/",
            None,
        ));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let (_paused_tx, paused_rx) = tokio::sync::watch::channel(false);

        let worker = tokio::spawn(sample_worker(
            api.clone(),
            tx,
            shutdown_rx,
            paused_rx,
            7,
            std::time::Duration::from_millis(50),
        ));

        // First tick is immediate: a Sample event arrives (all-error
        // sample against the dead endpoint) stamped with era 7.
        let event = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
            .await
            .expect("first sample within 5s")
            .expect("worker holds the sender");
        match event {
            TopEvent::Sample { era, sample } => {
                assert_eq!(era, 7);
                let snap: TopSample = *sample;
                assert!(snap.status.is_none());
                assert!(snap.status_error.is_some());
            }
            other => panic!("expected Sample, got {other:?}"),
        }

        // Shutdown stops the worker promptly (not at the next tick).
        shutdown_tx.send(true).expect("worker holds the receiver");
        tokio::time::timeout(std::time::Duration::from_secs(5), worker)
            .await
            .expect("worker exits on shutdown")
            .expect("worker task not cancelled");
    }

    /// Pause SKIPS sampling without killing the worker: while paused,
    /// no Sample lands within the window; on resume, the next tick
    /// samples again.
    #[tokio::test]
    async fn paused_worker_skips_sampling_and_resumes() {
        let api = std::sync::Arc::new(ignition_core::client::ReqwestGatewayApi::for_tests(
            "http://127.0.0.1:1/",
            None,
        ));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let (paused_tx, paused_rx) = tokio::sync::watch::channel(true); // paused from birth

        let worker = tokio::spawn(sample_worker(
            api.clone(),
            tx,
            shutdown_rx,
            paused_rx,
            3,
            std::time::Duration::from_millis(50),
        ));

        // Paused: several tick periods pass, ZERO samples.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert!(rx.try_recv().is_err(), "a paused worker must not sample");

        // Resume: the next tick samples (an all-error sample — the
        // endpoint is dead; the SHAPE is what this proves).
        paused_tx.send(false).expect("worker holds the receiver");
        let event = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
            .await
            .expect("a sample lands after resume")
            .expect("worker holds the sender");
        match event {
            TopEvent::Sample { era, .. } => assert_eq!(era, 3),
            other => panic!("expected Sample, got {other:?}"),
        }

        // Teardown: drop the sender so the worker exits.
        drop(worker);
    }

    /// The spawn guards outside a runtime: rails + era transition
    /// stand alone and NOTHING spawns — the 06-02 convention (update
    /// must never panic by construction).
    #[test]
    fn spawns_stand_alone_without_a_runtime() {
        let mut state = TopState {
            client: Some(std::sync::Arc::new(
                ignition_core::client::ReqwestGatewayApi::for_tests("http://127.0.0.1:1/", None),
            )),
            events_tx: Some(tokio::sync::mpsc::unbounded_channel().0),
            ..TopState::default()
        };
        let era_before = state.era;

        // No pause watch yet: the sample worker's rails are incomplete
        // and nothing happens (no panic) — the world arms the pause
        // watch first, exactly as run_loop does.
        spawn_sample_worker(&mut state);
        assert!(
            state.sample_shutdown.is_none(),
            "without the pause watch the spawn refuses cleanly"
        );
        assert_eq!(state.era, era_before, "no world churn on a refused spawn");

        // Pause watch present: the spawn transitions (shutdown rail +
        // era) even though no worker task can start outside a runtime.
        let (paused_tx, _paused_rx) = tokio::sync::watch::channel(false);
        state.paused_tx = Some(paused_tx);
        spawn_sample_worker(&mut state);
        assert!(state.sample_shutdown.is_some(), "shutdown rail armed");
        assert_eq!(state.era, era_before + 1, "era transitioned");

        // One-shot: busy guard arms, second call refuses.
        spawn_sample_once(&mut state);
        assert!(state.refresh_busy, "first one-shot marks busy");
        spawn_sample_once(&mut state);
        assert!(state.refresh_busy, "busy guard refuses stacking");
    }
}
