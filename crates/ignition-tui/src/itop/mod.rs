//! itop — htop for Ignition: the standalone live monitor.
//!
//! `ign top` (alias `ign itop`) opens a focused, full-screen monitor
//! over ONE gateway: header gauges (cpu/heap), sparkline history, and
//! a sortable/filterable "process list" of the gateway's live
//! entities (modules, designer/perspective/vision sessions, DB/OPC
//! connections, tag providers), with htop's gestures (pause, interval
//! +/-, sort, filter, kill = terminate a session).
//!
//! Deliberately NOT a cockpit tab: htop is a single-view instrument,
//! so itop owns its Elm loop (event/state/update/ui/worker) and shares
//! only the cockpit's vocabulary — context resolution, the theme
//! palette, the action layer, and the era/shutdown conventions. The
//! cockpit (`ign tui`) never imports this module.

pub mod event;
pub mod state;
pub mod ui;
pub mod update;
pub mod worker;

use std::time::Duration;

use futures_util::StreamExt;
use ignition_core::error::CoreError;
use tokio::sync::mpsc;

use crate::context;
use crate::itop::event::TopEvent;
use crate::itop::state::TopState;
use crate::itop::update::update;

/// The redraw staleness floor — the cockpit's TICK cadence verbatim
/// (one shared heartbeat across both surfaces).
const TICK: Duration = Duration::from_millis(250);

/// Open itop over the resolved profile context.
///
/// Resolution shares the cockpit's rules exactly: degraded-load
/// failures (`[ui]` contents, `poll_interval_secs` type/clamp) warn
/// and default so a config typo can never kill startup; resolution
/// failures (raw TOML, profile deserialize, selection, auth) return
/// BEFORE the terminal is touched with the normal stderr envelope and
/// exit-3 taxonomy. Every path after `ratatui::init()` runs through
/// `ratatui::restore()` (Ok, Err, and the init-installed panic hook).
/// The caller (`ign top`'s dispatch arm) owns the TTY guard.
pub async fn run(profile_flag: Option<&str>) -> Result<(), CoreError> {
    let ctx = context::resolve(profile_flag)?;

    let mut terminal = ratatui::init();
    // The cursor never parks inside the monitor (the cockpit's
    // 09-UAT Gap 2 rule). Best-effort chrome — a failed hide never
    // kills the run, and restore re-shows the cursor either way.
    let _ = terminal.hide_cursor();
    let app_result = run_loop(&mut terminal, ctx).await;
    ratatui::restore();
    app_result
}

/// State wiring + worker spawn + the select loop, then teardown.
async fn run_loop(
    terminal: &mut ratatui::DefaultTerminal,
    ctx: context::ResolvedContext,
) -> Result<(), CoreError> {
    let mut state = TopState::new(&ctx);
    let mut crossterm_events = crossterm::event::EventStream::new();
    let mut tick = tokio::time::interval(TICK);
    let (events_tx, mut events_rx) = mpsc::unbounded_channel::<TopEvent>();
    let (paused_tx, _paused_rx_keepalive) = tokio::sync::watch::channel(false);

    // Adopt the rails in one block (the ResolvedContext adoption
    // convention), then spawn the sample worker for the first world.
    state.events_tx = Some(events_tx.clone());
    state.paused_tx = Some(paused_tx);
    worker::spawn_sample_worker(&mut state);

    let result = event_loop(
        terminal,
        &mut state,
        &mut events_rx,
        &mut crossterm_events,
        &mut tick,
    )
    .await;

    // Stop the sample worker — the loop is done, the world is gone
    // (the refresh worker's teardown convention).
    if let Some(shutdown) = &state.sample_shutdown {
        let _ = shutdown.send(true);
    }
    result
}

/// The select loop — the cockpit's Pattern-1 shape scoped to itop:
/// crossterm `EventStream` (input) + a 250 ms tick + the TopEvent
/// channel (worker results). The EventStream arm matches
/// EXHAUSTIVELY — a bare `Some(Ok(e))` pattern would permanently
/// disable the arm after a terminal-stream error (research Pitfall 3).
async fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    state: &mut TopState,
    events_rx: &mut mpsc::UnboundedReceiver<TopEvent>,
    crossterm_events: &mut crossterm::event::EventStream,
    tick: &mut tokio::time::Interval,
) -> Result<(), CoreError> {
    loop {
        tokio::select! {
            _ = tick.tick() => {
                update(state, TopEvent::Tick);
                draw(terminal, state)?;
            }
            ev = crossterm_events.next() => match ev {
                Some(Ok(event)) => {
                    update(state, TopEvent::Input(event));
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
            Some(top_event) = events_rx.recv() => {
                update(state, top_event);
                if state.should_quit {
                    return Ok(());
                }
                draw(terminal, state)?;
            }
        }
    }
}

/// One diffed redraw; render is pure over the state.
fn draw(terminal: &mut ratatui::DefaultTerminal, state: &TopState) -> Result<(), CoreError> {
    terminal
        .draw(|frame| ui::render(state, frame))
        .map(|_| ())
        .map_err(|err| CoreError::Internal(format!("terminal draw failed: {err}")))
}
