//! The Alarms screen's interval poll worker (06-03) — the second
//! streaming pattern: an interval poll of
//! [`ignition_core::actions::tags::tags_alarms_active`] (the WebDev
//! route precondition rides inside the action — inherited for free),
//! era-stamped onto every report like the dashboard's refresh worker.
//! 06-04's tag workers (provider list, browse, detail read, live
//! watch) reuse these shapes from the same module.

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

/// The project the tags family rides — the CLI's own `--project`
/// default (`ign-cli`, the same deployed-route host the alarms
/// family uses).
pub const TAGS_PROJECT: &str = "ign-cli";

/// The LOCKED live-watch poll period — the watched set re-reads
/// every 2 s (the plan's cadence).
pub const WATCH_PERIOD: Duration = Duration::from_secs(2);

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

// ---- Tags screen workers (06-04) ----
//
// Three one-shots (provider list, browse, detail read) + the live
// watch interval (06-04 Task 2). Every one composes ignition-core
// action fns AS-IS — the webdev route preconditions ride inside the
// actions, so a route-less gateway degrades to the honest Error
// states with the action's own hint text.

/// One-shot provider list (the Tags screen's entry load and the
/// create/delete refresh trigger): reports [`AppEvent::TagsProviders`]
/// era-stamped. Busy-guarded so repeated entries cannot stack.
pub fn spawn_providers_once(state: &mut AppState) {
    if state.tags.providers_busy {
        return;
    }
    let Some(client) = state.client.as_ref().map(|handle| handle.0.clone()) else {
        return;
    };
    let Some(tx) = state.events_tx.clone() else {
        return;
    };
    state.tags.providers_busy = true;
    let era = state.era;
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            let result = actions::tags::tag_provider_list(&*client)
                .await
                .map(|result| result.providers)
                .map_err(|err| err.to_string());
            let _ = tx.send(AppEvent::TagsProviders { era, result });
        });
    }
}

/// One-shot browse of `path` (the descend worker — the CLI's
/// `tags browse PATH` with the display defaults): reports
/// [`AppEvent::TagsBrowse`] carrying the path (the stack level's
/// identity; a popped level's late result drops at the lookup).
pub fn spawn_browse(state: &mut AppState, path: &str) {
    let Some(client) = state.client.as_ref().map(|handle| handle.0.clone()) else {
        return;
    };
    let Some(tx) = state.events_tx.clone() else {
        return;
    };
    let path = path.to_string();
    let era = state.era;
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            let result = actions::tags::tags_browse(&*client, TAGS_PROJECT, &path, None, false)
                .await
                .map(|result| result.entries)
                .map_err(|err| err.to_string());
            let _ = tx.send(AppEvent::TagsBrowse { era, path, result });
        });
    }
}

/// One-shot detail read (the pane's on-demand current value): reports
/// [`AppEvent::TagDetailRead`] stamped with the detail-open's `seq` —
/// the request-id gate that drops reads for left/replaced panes.
pub fn spawn_detail_read(state: &mut AppState, seq: u64, path: &str) {
    let Some(client) = state.client.as_ref().map(|handle| handle.0.clone()) else {
        return;
    };
    let Some(tx) = state.events_tx.clone() else {
        return;
    };
    let path = path.to_string();
    let era = state.era;
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            let result =
                actions::tags::tags_read(&*client, TAGS_PROJECT, std::slice::from_ref(&path))
                    .await
                    .map_err(|err| err.to_string());
            // The route is always batch; the detail pane read exactly
            // one path — a missing row is the honest internal-class
            // error, not a silent blank.
            let result = match result {
                Ok(rows) => match rows.results.into_iter().next() {
                    Some(row) => Ok(row),
                    None => Err(format!(
                        "tags route read returned no row for {path:?} (unexpected shape)"
                    )),
                },
                Err(message) => Err(message),
            };
            let _ = tx.send(AppEvent::TagDetailRead { era, seq, result });
        });
    }
}

/// The live-watch loop (06-04 Task 2): one `tags_read` over the
/// WHOLE watched set per period, sent as [`AppEvent::TagWatch`]
/// stamped with the spawn-era AND the worker generation — a
/// set-change respawn retires this worker through `gen` (the global
/// era stays world-scoped per 06-03's lock; bumping it here would
/// retire the dashboard refresh worker), while the era still gates
/// profile switches. `select!` against the shutdown watch so screen
/// exit / set change / profile switch stop it promptly.
pub async fn tag_watch_worker(
    api: Arc<ReqwestGatewayApi>,
    tx: mpsc::UnboundedSender<AppEvent>,
    mut shutdown: watch::Receiver<bool>,
    era: u64,
    generation: u64,
    paths: Vec<String>,
    period: Duration,
) {
    let mut tick = tokio::time::interval(period);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = tick.tick() => {
                let result = actions::tags::tags_read(&*api, TAGS_PROJECT, &paths)
                    .await
                    .map(|result| result.results)
                    .map_err(|err| err.to_string());
                if tx
                    .send(AppEvent::TagWatch { era, generation, result })
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

/// Stop the running watch worker (if any): signal the watch and drop
/// the sender. Idempotent.
pub fn stop_tag_watch(state: &mut AppState) {
    if let Some(shutdown) = state.tags.watch_shutdown.take() {
        let _ = shutdown.send(true);
    }
}

/// (Re)spawn the watch worker for the CURRENT watched set: a prior
/// worker stops first (set change = shutdown + respawn with the new
/// set under a bumped `gen` — the local stale gate); an EMPTY set
/// just stops. No era bump: the global era belongs to WORLD changes
/// (profile switches) per 06-03's lock.
///
/// Outside a tokio runtime (state-machine unit tests) the rails
/// transition stands alone and nothing spawns.
pub fn spawn_tag_watch(state: &mut AppState) {
    stop_tag_watch(state); // a prior worker (set change, re-entry) stops first
    if state.tags.watched.is_empty() {
        return;
    }
    let Some(client) = state.client.as_ref().map(|handle| handle.0.clone()) else {
        return;
    };
    let Some(tx) = state.events_tx.clone() else {
        return;
    };
    state.tags.watch_gen += 1;
    let generation = state.tags.watch_gen;
    let paths: Vec<String> = state.tags.watched.iter().cloned().collect();
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    state.tags.watch_shutdown = Some(shutdown_tx);
    let era = state.era;
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(tag_watch_worker(
            client,
            tx,
            shutdown_rx,
            era,
            generation,
            paths,
            WATCH_PERIOD,
        ));
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

    // ---- Tags screen workers (06-04 Task 1) ----

    use super::{spawn_browse, spawn_detail_read, spawn_providers_once};

    /// The tags one-shot rails stand alone without a runtime: the
    /// provider load arms its busy guard (and refuses to stack);
    /// browse/detail-read transitions never panic.
    #[test]
    fn tags_one_shot_rails_stand_alone_without_a_runtime() {
        use crate::state::AppState;

        let mut state = AppState::new();
        state.client = Some(crate::state::ClientHandle(std::sync::Arc::new(
            ignition_core::client::ReqwestGatewayApi::for_tests("http://127.0.0.1:1/", None),
        )));
        state.events_tx = Some(tokio::sync::mpsc::unbounded_channel().0);

        spawn_providers_once(&mut state);
        assert!(state.tags.providers_busy, "provider load armed");
        spawn_providers_once(&mut state);
        assert!(state.tags.providers_busy, "busy guard refuses to stack");

        spawn_browse(&mut state, "[default]");
        spawn_detail_read(&mut state, 1, "[default]T1");
    }

    /// The detail read against a dead endpoint degrades to the Err
    /// payload — era + seq stamped, data never a panic (the alarms
    /// worker test's shape).
    #[tokio::test]
    async fn detail_read_degrades_to_err_on_a_dead_endpoint() {
        use crate::state::AppState;

        let mut state = AppState::new();
        state.client = Some(crate::state::ClientHandle(std::sync::Arc::new(
            ignition_core::client::ReqwestGatewayApi::for_tests("http://127.0.0.1:1/", None),
        )));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        state.events_tx = Some(tx);
        spawn_detail_read(&mut state, 7, "[default]T1");

        let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("read settles within 5s")
            .expect("worker holds the sender");
        match event {
            AppEvent::TagDetailRead { era, seq, result } => {
                assert_eq!(era, state.era, "era-stamped");
                assert_eq!(seq, 7, "seq-stamped (the request-id)");
                assert!(result.is_err(), "dead endpoint degrades to Err");
            }
            other => panic!("expected TagDetailRead, got {other:?}"),
        }
    }

    // ---- live watch (06-04 Task 2) ----

    use super::{WATCH_PERIOD, spawn_tag_watch, tag_watch_worker};

    /// The watch loop: the first (immediate) tick reports the poll's
    /// outcome era+gen stamped — a dead endpoint degrades to the Err
    /// payload (data, never a panic) — and the shutdown watch
    /// TERMINATES the loop promptly (a set-change respawn's stop).
    #[tokio::test]
    async fn watch_worker_reports_and_terminates_on_shutdown() {
        let api = std::sync::Arc::new(ignition_core::client::ReqwestGatewayApi::for_tests(
            "http://127.0.0.1:1/",
            None,
        ));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        let worker = tokio::spawn(tag_watch_worker(
            api,
            tx,
            shutdown_rx,
            4,
            2, // gen 2 — a respawn superseding gen 1
            vec!["[default]T1".to_string()],
            WATCH_PERIOD,
        ));

        let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("first poll within 5s")
            .expect("worker holds the sender");
        match event {
            AppEvent::TagWatch {
                era,
                generation,
                result,
            } => {
                assert_eq!((era, generation), (4, 2), "era+generation stamped");
                assert!(result.is_err(), "dead endpoint degrades to Err: {result:?}");
            }
            other => panic!("expected TagWatch, got {other:?}"),
        }

        shutdown_tx.send(true).expect("worker holds the receiver");
        tokio::time::timeout(Duration::from_secs(5), worker)
            .await
            .expect("worker exits on shutdown")
            .expect("worker task not cancelled");
    }

    /// The spawn rail: a non-empty set arms the worker under a bumped
    /// gen; the empty-set spawn is a plain stop (rail cleared, gen
    /// untouched). Nothing spawns outside a runtime (the rails
    /// transition stands alone).
    #[test]
    fn watch_spawn_rails_stand_alone_without_a_runtime() {
        use crate::state::AppState;

        let mut state = AppState::new();
        state.client = Some(crate::state::ClientHandle(std::sync::Arc::new(
            ignition_core::client::ReqwestGatewayApi::for_tests("http://127.0.0.1:1/", None),
        )));
        state.events_tx = Some(tokio::sync::mpsc::unbounded_channel().0);

        spawn_tag_watch(&mut state);
        assert!(
            state.tags.watch_shutdown.is_none() && state.tags.watch_gen == 0,
            "empty set: a plain stop, nothing armed"
        );

        state.tags.watched.insert("[default]T1".into());
        spawn_tag_watch(&mut state);
        assert!(state.tags.watch_shutdown.is_some(), "rail armed");
        assert_eq!(state.tags.watch_gen, 1, "gen bumped");

        state.tags.watched.clear();
        spawn_tag_watch(&mut state);
        assert!(
            state.tags.watch_shutdown.is_none(),
            "the empty-set respawn stops the worker"
        );
    }
}
