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
pub mod state;
pub mod ui;
pub mod update;
pub mod workers;

use std::time::Duration;

use futures_util::StreamExt;
use ignition_core::error::CoreError;
use tokio::sync::mpsc;

use crate::event::AppEvent;
use crate::state::AppState;
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
    // The shell wires zero gateway calls; resolving first keeps the
    // day-one contract honest (the cockpit opens only against a real
    // profile) while 06-02's workers take the client from here.
    let _context = context::resolve(profile_flag.as_deref())?;

    let mut terminal = ratatui::init();
    let app_result = run_loop(&mut terminal).await;
    ratatui::restore();
    app_result
}

/// The select loop (research Pattern 1): crossterm `EventStream` (input)
/// + a 250 ms tick + the AppEvent channel (worker results, later plans).
///
/// The EventStream arm matches EXHAUSTIVELY — a bare `Some(Ok(e)) =`
/// pattern would permanently disable the arm after a terminal-stream
/// error and the app would freeze for input (research Pitfall 3).
async fn run_loop(terminal: &mut ratatui::DefaultTerminal) -> Result<(), CoreError> {
    let mut state = AppState::new();
    let mut crossterm_events = crossterm::event::EventStream::new();
    let mut tick = tokio::time::interval(TICK);
    // The shell spawns no workers, but the AppEvent arm is the loop's
    // third rail — holding the sender keeps the channel open (recv()
    // pends, the arm stays armed) so 06-02+ workers just clone it.
    let events_tx = mpsc::unbounded_channel::<AppEvent>();
    let mut events_rx = events_tx.1;

    loop {
        tokio::select! {
            _ = tick.tick() => {
                update(&mut state, AppEvent::Tick);
                draw(terminal, &state)?;
            }
            ev = crossterm_events.next() => match ev {
                Some(Ok(event)) => {
                    update(&mut state, AppEvent::Input(event));
                    if state.should_quit {
                        return Ok(());
                    }
                    draw(terminal, &state)?;
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
                update(&mut state, app_event);
                if state.should_quit {
                    return Ok(());
                }
                draw(terminal, &state)?;
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
