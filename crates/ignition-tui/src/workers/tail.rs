//! The Logs screen's tail worker (06-03) — the channel-sink streaming
//! pattern: compose [`ignition_core::actions::logs::tail`] AS-IS (the
//! key_link contract) with a sink closure that forwards every entry to
//! the AppEvent rail, and `select!` the whole tail future against the
//! screen's shutdown watch so LEAVING the screen stops it even between
//! entries (must-have truth #3: the UI never blocks on gateway I/O).
//!
//! The sink's sync `send` on the UNBOUNDED channel is legal inside a
//! sync closure (LOCKED 06-01 transport decision — `tail`'s sink is
//! `FnMut + Send`, and 06-02's `+ Send` probe futures already let the
//! whole future cross `tokio::spawn`).

use std::sync::Arc;
use std::time::Duration;

use ignition_core::actions;
use ignition_core::client::ReqwestGatewayApi;
use ignition_core::client::logs::LogEntry;
use tokio::sync::{mpsc, watch};

use crate::event::AppEvent;
use crate::state::AppState;

/// The poll cadence between tail pages — the CLI's `--interval`
/// default (2 s), inherited verbatim.
pub const TAIL_INTERVAL: Duration = Duration::from_secs(2);

/// Stream entries to the rail until shutdown (or a tail error). No
/// deadline — the screen-scoped lifetime is the shutdown watch, not a
/// timeout. Per-line era stamps are deliberately absent (the ring's
/// turnover is the acceptance policy); the worker exits on send-error
/// (the loop is gone) OR shutdown.
pub async fn tail_worker(
    api: Arc<ReqwestGatewayApi>,
    tx: mpsc::UnboundedSender<AppEvent>,
    mut shutdown: watch::Receiver<bool>,
    min_level: Option<&'static str>,
    since_ms: Option<i64>,
    interval: Duration,
) {
    let sink_tx = tx.clone();
    let mut sink = move |entry: &LogEntry| {
        let _ = sink_tx.send(AppEvent::LogLine(entry.clone()));
    };
    tokio::select! {
        // `deadline: None` — runs until shutdown; deadline expiry (the
        // graceful-Ok path) is unreachable with Duration::MAX.
        result = actions::logs::tail(
            &*api,
            None, // logger filter: the TUI tail carries no logger scope
            min_level,
            since_ms,
            interval,
            None,
            &mut sink,
        ) => {
            // A tail that ends with an error surfaces as data (the
            // dismissable error modal) — never a panic (Pitfall 2).
            if let Err(err) = result {
                let _ = tx.send(AppEvent::Error(format!("log tail: {err}")));
            }
        }
        // Leaving the screen / filter restart / profile switch. The
        // dropped-sender case resolves Err here too — both mean stop.
        _ = shutdown.changed() => {}
    }
}

/// Stop the running tail worker (if any): signal the watch and drop
/// the sender. Idempotent.
pub fn stop_tail(state: &mut AppState) {
    if let Some(shutdown) = state.logs.tail_shutdown.take() {
        let _ = shutdown.send(true);
    }
}

/// Spawn the tail worker for the CURRENT world: a fresh shutdown
/// channel, the filter's wire `min_level`, and `since` = the ring's
/// newest timestamp (re-entry and filter restarts resume exactly past
/// what the ring already holds — no duplicate flood, no gap). No era
/// bump: the global era belongs to WORLD changes (profile switches),
/// and per-line stamps are plan-locked out.
///
/// Outside a tokio runtime (state-machine unit tests) the rails
/// transition stands alone and nothing spawns.
pub fn spawn_tail(state: &mut AppState) {
    let Some(client) = state.client.as_ref().map(|handle| handle.0.clone()) else {
        return;
    };
    let Some(tx) = state.events_tx.clone() else {
        return;
    };
    stop_tail(state); // a prior worker (filter change, re-entry) stops first
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    state.logs.tail_shutdown = Some(shutdown_tx);
    let min_level = state.logs.filter.wire();
    let since_ms = state.logs.ring.back().map(|entry| entry.timestamp);
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(tail_worker(
            client,
            tx,
            shutdown_rx,
            min_level,
            since_ms,
            TAIL_INTERVAL,
        ));
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{TAIL_INTERVAL, tail_worker};
    use crate::event::AppEvent;

    fn entry(timestamp: i64, message: &str) -> ignition_core::client::logs::LogEntry {
        ignition_core::client::logs::LogEntry {
            timestamp,
            logger_name: "GatewayManager".into(),
            level: "INFO".into(),
            message: message.into(),
            stack: Vec::new(),
            mdc: Default::default(),
            extra: Default::default(),
        }
    }

    /// The channel-sink proof: entries a wiremock gateway serves reach
    /// the rail as `AppEvent::LogLine` in timestamp order, and the
    /// shutdown watch TERMINATES the worker between pages (the tail
    /// future is dropped mid-poll — leaving the screen stops it).
    #[tokio::test]
    async fn tail_worker_streams_lines_to_the_rail_and_stops_on_shutdown() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/data/api/v1/logs"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "items": [
                        {"timestamp": 1005, "loggerName": "GatewayManager",
                         "level": "INFO", "message": "first"},
                        {"timestamp": 1010, "loggerName": "GatewayManager",
                         "level": "WARN", "message": "second"}
                    ],
                    "metadata": {"total": 2}
                })),
            )
            .expect(1..)
            .mount(&server)
            .await;
        let api = std::sync::Arc::new(ignition_core::client::ReqwestGatewayApi::for_tests(
            &server.uri(),
            None,
        ));

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let worker = tokio::spawn(tail_worker(
            api,
            tx,
            shutdown_rx,
            None,
            Some(1000),
            TAIL_INTERVAL,
        ));

        // Both page entries stream through the sink in timestamp order.
        let first = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("first line within 5s")
            .expect("worker holds the sender");
        match first {
            AppEvent::LogLine(entry) => {
                assert_eq!((entry.timestamp, entry.message.as_str()), (1005, "first"));
            }
            other => panic!("expected LogLine, got {other:?}"),
        }
        let second = rx.recv().await.expect("second line");
        assert!(matches!(&second, AppEvent::LogLine(e) if e.message == "second"));

        // Shutdown stops the worker promptly — mid-wait, not at a page.
        shutdown_tx.send(true).expect("worker holds the receiver");
        tokio::time::timeout(Duration::from_secs(5), worker)
            .await
            .expect("worker exits on shutdown")
            .expect("worker task not cancelled");
    }

    /// A tail error (auth fail-fast against a dead endpoint is a
    /// network retry… so pin the ERROR path with a 401 fixture) —
    /// wait: a 401 surfaces as AppEvent::Error (data, never a panic).
    #[tokio::test]
    async fn tail_worker_surfaces_errors_as_data() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/data/api/v1/logs"))
            .respond_with(wiremock::ResponseTemplate::new(401))
            .expect(1..)
            .mount(&server)
            .await;
        let api = std::sync::Arc::new(ignition_core::client::ReqwestGatewayApi::for_tests(
            &server.uri(),
            None,
        ));

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let worker = tokio::spawn(tail_worker(
            api,
            tx,
            shutdown_rx,
            None,
            None,
            Duration::from_millis(10),
        ));

        let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("error surfaces within 5s")
            .expect("worker holds the sender");
        match event {
            AppEvent::Error(message) => {
                assert!(message.starts_with("log tail: "), "prefixed: {message}");
            }
            other => panic!("expected Error, got {other:?}"),
        }
        // The worker ran to its own completion (no panic, no hang).
        tokio::time::timeout(Duration::from_secs(5), worker)
            .await
            .expect("worker finishes after the error")
            .expect("worker task not cancelled");
    }

    /// `stop_tail` + `spawn_tail` rails transition without a runtime
    /// (the state-machine half; nothing spawns — proven by the guard).
    #[test]
    fn spawn_tail_rails_stand_alone_without_a_runtime() {
        use super::{spawn_tail, stop_tail};
        use crate::state::AppState;

        let mut state = AppState::new();
        state.client = Some(crate::state::ClientHandle(std::sync::Arc::new(
            ignition_core::client::ReqwestGatewayApi::for_tests("http://127.0.0.1:1/", None),
        )));
        state.events_tx = Some(tokio::sync::mpsc::unbounded_channel().0);

        spawn_tail(&mut state);
        assert!(
            state.logs.tail_shutdown.is_some(),
            "shutdown rail armed (nothing spawned outside a runtime)"
        );
        stop_tail(&mut state);
        assert!(state.logs.tail_shutdown.is_none(), "rail cleared");
    }

    /// Compile-shape pin: the entry type the sink forwards is the real
    /// client model (the event variant carries it verbatim).
    #[test]
    fn sink_forwards_the_real_log_entry_type() {
        let entry = entry(1, "shape");
        let event = AppEvent::LogLine(entry.clone());
        assert!(matches!(event, AppEvent::LogLine(_)));
        assert_eq!(entry.message, "shape");
    }
}
