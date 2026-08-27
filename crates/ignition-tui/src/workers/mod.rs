//! Worker lifecycle helpers — the shared conventions every 06-02..06-06
//! worker follows (research Pattern 2 + Pitfall 9).
//!
//! Workers own ALL gateway I/O (`workers/*` are the only place actions
//! are awaited) and report back over the AppEvent channel, stamped with
//! the era they were spawned under. The shell ships the helpers; the
//! first real worker (dashboard refresh) arrives in 06-02.

use crate::state::AppState;

/// Bump the state's era counter and return the fresh value — call when
/// the world under running workers changes (screen exit, profile
/// switch): results stamped with a prior era are stale and update()
/// drops them.
pub fn new_era(state: &mut AppState) -> u64 {
    state.era = state.era.wrapping_add(1);
    state.era
}

/// A fresh shutdown channel: send `true` to stop every worker holding
/// the receiver (the watch-channel cancellation answer — zero new deps;
/// research Pattern 2b).
pub fn shutdown_channel() -> (
    tokio::sync::watch::Sender<bool>,
    tokio::sync::watch::Receiver<bool>,
) {
    tokio::sync::watch::channel(false)
}

/// Whether a worker's stamped era still matches the state's — the
/// stale-event gate update() applies to inbound worker results.
pub fn is_current(state_era: u64, worker_era: u64) -> bool {
    state_era == worker_era
}

/// Spawn a ONE-SHOT action worker (06-02's pattern, verbatim from the
/// plan): `tokio::spawn` with the current era → run the future (an
/// ignition-core action fn, AS-IS) → send [`AppEvent::ActionDone`]
/// carrying `serde_json::to_string_pretty` of the typed result (or the
/// error's display string).
///
/// The `in_flight` label is the busy guard — one action at a time, so
/// long waits never stack and the status line can name what runs.
/// Outside a tokio runtime (state-machine unit tests) the guard + label
/// transition stands alone and nothing spawns.
pub fn spawn_action<T, F>(state: &mut crate::state::AppState, label: &'static str, fut: F)
where
    T: serde::Serialize + Send + 'static,
    F: std::future::Future<Output = Result<T, ignition_core::error::CoreError>> + Send + 'static,
{
    if state.dashboard.in_flight.is_some() {
        return; // one action at a time (the busy guard per action)
    }
    let Some(tx) = state.events_tx.clone() else {
        return;
    };
    state.dashboard.in_flight = Some(label);
    let era = state.era;
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            let result = match fut.await {
                Ok(value) => serde_json::to_string_pretty(&value).map_err(|err| err.to_string()),
                Err(err) => Err(err.to_string()),
            };
            let _ = tx.send(crate::event::AppEvent::ActionDone { era, label, result });
        });
    }
}

pub mod ops;
pub mod refresh;
pub mod rig_stream;
pub mod tail;
pub mod watch;

#[cfg(test)]
mod tests {
    use super::{is_current, new_era, shutdown_channel};
    use crate::state::AppState;

    /// new_era bumps monotonically and stamps what workers carry.
    #[test]
    fn era_bumps_and_gates_stale_results() {
        let mut state = AppState::new();
        assert_eq!(state.era, 0);

        let worker_era = new_era(&mut state);
        assert_eq!(worker_era, 1);
        assert!(is_current(state.era, worker_era));

        // The world changes (screen exit): the old worker is stale.
        new_era(&mut state);
        assert!(!is_current(state.era, worker_era));
    }

    /// The shutdown channel starts live and closes on a send.
    #[tokio::test]
    async fn shutdown_channel_signals_stop() {
        let (tx, mut rx) = shutdown_channel();
        assert!(!*rx.borrow(), "starts not-shutting-down");

        tx.send(true).expect("send while loop holds the receiver");
        // Wake any worker selected on `changed()`.
        rx.changed().await.expect("sender still alive");
        assert!(*rx.borrow());
    }
}
