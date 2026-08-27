//! The Alarms screen's interval poll worker (06-03) — the second
//! streaming pattern: an interval poll of
//! [`ignition_core::actions::tags::tags_alarms_active`] (the WebDev
//! route precondition rides inside the action — inherited for free),
//! era-stamped onto every report like the dashboard's refresh worker.
//! 06-04's tag watch reuses this shape.

use std::sync::Arc;
use std::time::Duration;

use ignition_core::actions;
use ignition_core::client::ReqwestGatewayApi;
use tokio::sync::{mpsc, watch};

use crate::event::AppEvent;
use crate::state::AppState;

/// The LOCKED poll period — active alarms refresh every 5 s.
pub const ALARMS_PERIOD: Duration = Duration::from_secs(5);

/// The project the alarm routes deploy into — the CLI family's own
/// default (`--project default ign-cli`).
pub const ALARMS_PROJECT: &str = "ign-cli";

/// The poll loop: one `tags_alarms_active` per period, sent as an
/// [`AppEvent::Alarms`] stamped with the spawn-era; `select!` against
/// the shutdown watch so leaving the screen stops it promptly.
pub async fn alarms_worker(
    api: Arc<ReqwestGatewayApi>,
    tx: mpsc::UnboundedSender<AppEvent>,
    mut shutdown: watch::Receiver<bool>,
    era: u64,
    period: Duration,
) {
    let mut tick = tokio::time::interval(period);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = tick.tick() => {
                let result = actions::tags::tags_alarms_active(
                    &*api,
                    ALARMS_PROJECT,
                    None,
                    None,
                    None,
                )
                .await
                .map(|result| result.alarms)
                .map_err(|err| err.to_string());
                if tx
                    .send(AppEvent::Alarms { era, result })
                    .is_err()
                {
                    return; // the loop is gone — stop.
                }
            }
            // A signal or a dropped sender both mean stop.
            _ = shutdown.changed() => return,
        }
    }
}

/// Stop the running alarms worker (if any): signal the watch and drop
/// the sender. Idempotent.
pub fn stop_alarms(state: &mut AppState) {
    if let Some(shutdown) = state.alarms.shutdown.take() {
        let _ = shutdown.send(true);
    }
}

/// Spawn the interval alarms worker for the CURRENT world: a fresh
/// shutdown channel, the CURRENT era (no bump — the global era belongs
/// to profile-switch worlds; screen-scoped workers live and die by
/// their own watches).
///
/// Outside a tokio runtime (state-machine unit tests) the rails
/// transition stands alone and nothing spawns.
pub fn spawn_alarms(state: &mut AppState) {
    let Some(client) = state.client.as_ref().map(|handle| handle.0.clone()) else {
        return;
    };
    let Some(tx) = state.events_tx.clone() else {
        return;
    };
    stop_alarms(state); // a prior worker (re-entry) stops first
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    state.alarms.shutdown = Some(shutdown_tx);
    let era = state.era;
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(alarms_worker(client, tx, shutdown_rx, era, ALARMS_PERIOD));
    }
}

/// One immediate poll (`spawn_refresh_once`'s twin): the ack-refresh
/// trigger — update calls this when an `alarms ack` action lands so
/// the active table reflects the acknowledgment NOW, not ≤5 s later.
/// Busy-guarded so repeated triggers cannot stack.
pub fn spawn_alarms_once(state: &mut AppState) {
    if state.alarms.busy {
        return;
    }
    let Some(client) = state.client.as_ref().map(|handle| handle.0.clone()) else {
        return;
    };
    let Some(tx) = state.events_tx.clone() else {
        return;
    };
    state.alarms.busy = true;
    let era = state.era;
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            let result =
                actions::tags::tags_alarms_active(&*client, ALARMS_PROJECT, None, None, None)
                    .await
                    .map(|result| result.alarms)
                    .map_err(|err| err.to_string());
            let _ = tx.send(AppEvent::Alarms { era, result });
        });
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{ALARMS_PERIOD, alarms_worker};
    use crate::event::AppEvent;

    /// The worker loop: the first (immediate) tick reports the poll's
    /// outcome era-stamped — a dead endpoint degrades to the Err
    /// payload (data, never a panic) — and the shutdown watch
    /// TERMINATES the loop promptly.
    #[tokio::test]
    async fn alarms_worker_reports_and_terminates_on_shutdown() {
        // Nothing listens here — the poll fails fast (webdev
        // precondition → network error), which IS the degrade path.
        let api = std::sync::Arc::new(ignition_core::client::ReqwestGatewayApi::for_tests(
            "http://127.0.0.1:1/",
            None,
        ));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        let worker = tokio::spawn(alarms_worker(api, tx, shutdown_rx, 4, ALARMS_PERIOD));

        let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("first poll within 5s")
            .expect("worker holds the sender");
        match event {
            AppEvent::Alarms { era, result } => {
                assert_eq!(era, 4, "era-stamped");
                assert!(result.is_err(), "dead endpoint degrades to Err: {result:?}");
            }
            other => panic!("expected Alarms, got {other:?}"),
        }

        shutdown_tx.send(true).expect("worker holds the receiver");
        tokio::time::timeout(Duration::from_secs(5), worker)
            .await
            .expect("worker exits on shutdown")
            .expect("worker task not cancelled");
    }

    /// The rails transition stands alone without a runtime (the
    /// state-machine half).
    #[test]
    fn alarms_rails_stand_alone_without_a_runtime() {
        use super::{spawn_alarms, spawn_alarms_once, stop_alarms};
        use crate::state::AppState;

        let mut state = AppState::new();
        state.client = Some(crate::state::ClientHandle(std::sync::Arc::new(
            ignition_core::client::ReqwestGatewayApi::for_tests("http://127.0.0.1:1/", None),
        )));
        state.events_tx = Some(tokio::sync::mpsc::unbounded_channel().0);

        spawn_alarms(&mut state);
        assert!(state.alarms.shutdown.is_some(), "rail armed");

        // The one-shot trigger arms busy (nothing spawns outside a
        // runtime) and refuses to stack.
        spawn_alarms_once(&mut state);
        assert!(state.alarms.busy);
        spawn_alarms_once(&mut state);
        assert!(state.alarms.busy, "busy guard refuses to stack");

        stop_alarms(&mut state);
        assert!(state.alarms.shutdown.is_none(), "rail cleared");
    }
}
