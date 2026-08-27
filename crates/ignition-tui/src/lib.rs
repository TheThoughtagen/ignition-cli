//! TUI cockpit for `ign` — the Phase 6 foundation.
//!
//! 06-01 ships the shell: the event-driven async loop (research Pattern
//! 1, adapted from ratatui's official async example), Elm-style
//! [`state`]/[`update`], profile→client [`context`] resolution, and the
//! tab-bar chrome. Screen plans (06-02..06-06) plug into this loop
//! additively — per-screen state, workers, and UI modules.
//!
//! Terminal lifecycle is `ratatui::init()`/`ratatui::restore()` — the
//! official helpers install raw mode, the alternate screen, AND a
//! restore-on-panic hook (research Pitfall 2). Callers guarantee a TTY
//! (the `ign tui` dispatch arm refuses piped stdout first).

pub mod context;
pub mod event;
pub mod routes;
pub mod state;
pub mod ui;
pub mod update;
pub mod workers;

use std::time::Duration;

use futures_util::StreamExt;
use ignition_core::error::CoreError;
use tokio::sync::mpsc;

use crate::event::AppEvent;
use crate::state::{AppState, ClientHandle};
use crate::update::update;

/// The cockpit's redraw staleness floor. The loop draws after EVERY
/// processed event; the tick guarantees a redraw even when the world is
/// quiet (LOCKED cadence — not tick-only).
const TICK: Duration = Duration::from_millis(250);

/// Open the cockpit over the resolved profile context.
///
/// Resolution failures (no profile, missing secret) return BEFORE the
/// terminal is touched — the user sees the normal stderr envelope and
/// exit taxonomy, not a flash of alternate screen. Every path after
/// `ratatui::init()` runs through `ratatui::restore()` (Ok, Err, and
/// the init-installed panic hook).
pub async fn run(profile_flag: Option<String>) -> Result<(), CoreError> {
    // The cockpit owns a live client from the first frame: the
    // dashboard's refresh worker spawns against it (06-02).
    let (_profile_name, client) = context::resolve(profile_flag.as_deref())?;

    let mut terminal = ratatui::init();
    let app_result = run_loop(&mut terminal, client).await;
    ratatui::restore();
    app_result
}

/// State wiring + worker spawn + the select loop, then worker teardown.
async fn run_loop(
    terminal: &mut ratatui::DefaultTerminal,
    client: std::sync::Arc<ignition_core::client::ReqwestGatewayApi>,
) -> Result<(), CoreError> {
    let mut state = AppState::new();
    let mut crossterm_events = crossterm::event::EventStream::new();
    let mut tick = tokio::time::interval(TICK);
    let (events_tx, mut events_rx) = mpsc::unbounded_channel::<AppEvent>();

    // The dashboard's interval refresh worker (06-02): one spawn per
    // world. The shell keeps the sender (the loop's rail stays armed
    // even if this worker is the only sender); profile switch (06-02
    // Task 3) re-spawns against a new client under a new era.
    let (shutdown_tx, shutdown_rx) = workers::shutdown_channel();
    state.client = Some(ClientHandle(client.clone()));
    state.events_tx = Some(events_tx.clone());
    state.refresh_shutdown = Some(shutdown_tx);
    let era = workers::new_era(&mut state);
    tokio::spawn(workers::refresh::refresh_worker(
        client,
        events_tx,
        shutdown_rx,
        era,
        workers::refresh::REFRESH_PERIOD,
    ));

    let result = event_loop(
        terminal,
        &mut state,
        &mut events_rx,
        &mut crossterm_events,
        &mut tick,
    )
    .await;

    // Stop the refresh worker — the loop is done, the world is gone.
    if let Some(shutdown) = &state.refresh_shutdown {
        let _ = shutdown.send(true);
    }
    result
}

/// The select loop (research Pattern 1): crossterm `EventStream` (input)
/// + a 250 ms tick + the AppEvent channel (worker results).
///
/// The EventStream arm matches EXHAUSTIVELY — a bare `Some(Ok(e)) =`
/// pattern would permanently disable the arm after a terminal-stream
/// error and the app would freeze for input (research Pitfall 3).
async fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    state: &mut AppState,
    events_rx: &mut mpsc::UnboundedReceiver<AppEvent>,
    crossterm_events: &mut crossterm::event::EventStream,
    tick: &mut tokio::time::Interval,
) -> Result<(), CoreError> {
    loop {
        tokio::select! {
            _ = tick.tick() => {
                update(state, AppEvent::Tick);
                draw(terminal, state)?;
            }
            ev = crossterm_events.next() => match ev {
                Some(Ok(event)) => {
                    update(state, AppEvent::Input(event));
                    if state.should_quit {
                        return Ok(());
                    }
                    draw(terminal, state)?;
                }
                // A terminal-stream error is fatal-but-clean: restore
                // runs in run()'s tail, the error flows the normal
                // envelope path.
                Some(Err(err)) => {
                    return Err(CoreError::Internal(format!(
                        "terminal input stream failed: {err}"
                    )));
                }
                None => return Ok(()),
            },
            Some(app_event) = events_rx.recv() => {
                update(state, app_event);
                if state.should_quit {
                    return Ok(());
                }
                draw(terminal, state)?;
            }
        }
    }
}

/// One diffed redraw; render is pure over the state.
fn draw(terminal: &mut ratatui::DefaultTerminal, state: &AppState) -> Result<(), CoreError> {
    terminal
        .draw(|frame| ui::render(state, frame))
        .map(|_| ())
        .map_err(|err| CoreError::Internal(format!("terminal draw failed: {err}")))
}
