//! Worker lifecycle helpers — the shared conventions every 06-02..06-06
//! worker follows (research Pattern 2 + Pitfall 9).
//!
//! Workers own ALL gateway I/O (`workers/*` are the only place actions
//! are awaited) and report back over the AppEvent channel, stamped with
//! the era they were spawned under. The shell ships the helpers; the
//! first real worker (dashboard refresh) arrives in 06-02.

use tokio::sync::watch;

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
pub fn shutdown_channel() -> (watch::Sender<bool>, watch::Receiver<bool>) {
    watch::channel(false)
}

/// Whether a worker's stamped era still matches the state's — the
/// stale-event gate update() applies to inbound worker results.
pub fn is_current(state_era: u64, worker_era: u64) -> bool {
    state_era == worker_era
}

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
