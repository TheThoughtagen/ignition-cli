//! The cockpit's Elm-style update — PURE AND SYNC.
//!
//! This file must never await anything: gateway I/O lives only in
//! `workers/*` (research anti-pattern #1 — the cardinal sin this
//! architecture exists to prevent). `update` takes the state and one
//! event, mutates, returns; that is all.

use crossterm::event::{Event, KeyCode, KeyModifiers};

use crate::event::AppEvent;
use crate::state::{AppState, Modal, Screen};
use crate::workers;

/// Fold one event into the state. The select loop calls this exactly
/// once per event; drawing happens in the loop, never here.
pub fn update(state: &mut AppState, event: AppEvent) {
    match event {
        AppEvent::Input(event) => handle_input(state, event),
        // The tick is the staleness floor for rendering; the shell keeps
        // no timed state. (Timed logic lives in workers.)
        AppEvent::Tick => {}
        // Worker failures surface as data — a modal the user dismisses,
        // never a panic (research Pitfall 2: workers must not unwind).
        AppEvent::Error(message) => {
            state.open_modal(Modal::Result_ {
                title: "error".to_string(),
                lines: vec![message],
            });
        }
        // The dashboard refresh: store, clear busy, record freshness.
        // Stale-era snapshots are dropped whole (Pitfall 9) — a result
        // from the pre-switch profile never lands, not even partially.
        AppEvent::Refresh { era, snapshot } => {
            if workers::is_current(state.era, era) {
                state.dashboard.snapshot = Some(*snapshot);
                state.dashboard.last_refresh = Some(std::time::Instant::now());
                state.dashboard.busy = false;
            }
        }
    }
}

/// Key routing: modal first (when open), then screen-global keys.
fn handle_input(state: &mut AppState, event: Event) {
    // Release/Repeat events must not double-fire on kitty-protocol
    // terminals — keep Press only (research Pitfall 5).
    let Some(key) = event.as_key_press_event() else {
        return;
    };

    // Ctrl-C ALWAYS quits — raw mode disables ISIG, so Ctrl-C arrives
    // here as a KeyEvent, never as SIGINT (research Pitfall 4). Even
    // with a modal open: the escape hatch must not be escapable.
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        state.should_quit = true;
        return;
    }

    if state.modal.is_some() {
        handle_modal_input(state, key.code, key.modifiers);
        return;
    }

    match key.code {
        // `q` (no modal) and Esc both quit the cockpit.
        KeyCode::Char('q') | KeyCode::Esc => state.should_quit = true,
        // Tab / Shift+Tab cycle screens (wrap-around).
        KeyCode::Tab => state.screen = state.screen.next(),
        KeyCode::BackTab => state.screen = state.screen.prev(),
        // Everything else belongs to the active screen's keymap.
        _ => handle_screen_keys(state, key.code),
    }
}

/// Screen-local keymaps (no modal open). Screens not yet wired (06-03+)
/// take no keys beyond the global set.
fn handle_screen_keys(state: &mut AppState, code: KeyCode) {
    if state.screen == Screen::Dashboard {
        dashboard_keys(state, code);
    }
}

/// The dashboard keymap (06-02 Task 1): `r` refreshes now (busy
/// guarded), Up/Down move the sessions-table cursor.
fn dashboard_keys(state: &mut AppState, code: KeyCode) {
    match code {
        KeyCode::Char('r') => workers::refresh::spawn_refresh_once(state),
        KeyCode::Up => move_session_selection(state, -1),
        KeyCode::Down => move_session_selection(state, 1),
        _ => {}
    }
}

/// Move the sessions-table cursor within the flattened row set. Clamped
/// (no wrap — a table cursor that wraps is a lie about adjacency).
fn move_session_selection(state: &mut AppState, delta: i32) {
    let Some(snapshot) = &state.dashboard.snapshot else {
        return;
    };
    let Some(sessions) = &snapshot.sessions else {
        return;
    };
    let len = crate::state::session_rows(sessions).len();
    if len == 0 {
        return;
    }
    let next = match state.dashboard.sessions_table.selected() {
        None => 0,
        Some(idx) if delta < 0 => idx.saturating_sub(1),
        Some(idx) => (idx + 1).min(len - 1),
    };
    state.dashboard.sessions_table.select(Some(next));
}

/// Keystrokes while a modal is open: Esc pops the modal (before any
/// quit interpretation — one Esc closes the dialog, a second quits);
/// an Input modal additionally edits its buffer (hand-rolled — no
/// tui-input dependency).
fn handle_modal_input(state: &mut AppState, code: KeyCode, modifiers: KeyModifiers) {
    // Plain-text chars route to the Input buffer FIRST — typing 'q' in
    // a username prompt must insert 'q', not quit.
    if let Some(Modal::Input { buffer, .. }) = state.modal.as_mut()
        && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        match code {
            KeyCode::Char(c) => {
                buffer.push(c);
                return;
            }
            KeyCode::Backspace => {
                buffer.pop();
                return;
            }
            _ => {}
        }
    }

    // Esc cancels/closes whatever modal is open (never quits from
    // behind a modal).
    if code == KeyCode::Esc {
        state.close_modal();
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    use super::update;
    use crate::event::AppEvent;
    use crate::state::{AppState, Modal, Screen};

    fn key(code: KeyCode, modifiers: KeyModifiers) -> AppEvent {
        AppEvent::Input(Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }))
    }

    /// `q` with no modal quits.
    #[test]
    fn q_quits() {
        let mut state = AppState::new();
        update(&mut state, key(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(state.should_quit);
    }

    /// `q` with a modal open does NOT quit (the modal owns the key).
    #[test]
    fn q_does_not_quit_behind_a_modal() {
        let mut state = AppState::new();
        state.open_modal(Modal::Confirm {
            title: "delete?".into(),
            body: "this is destructive".into(),
        });
        update(&mut state, key(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(!state.should_quit, "modal swallows q");
        assert!(state.modal.is_some());
    }

    /// Ctrl-C ALWAYS quits — even behind a modal (the escape hatch).
    #[test]
    fn ctrl_c_quits_even_with_modal_open() {
        let mut state = AppState::new();
        state.open_modal(Modal::Result_ {
            title: "error".into(),
            lines: vec!["boom".into()],
        });
        update(&mut state, key(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(state.should_quit);
    }

    /// Esc pops the modal FIRST; only a second Esc (no modal) quits.
    #[test]
    fn esc_pops_modal_before_quitting() {
        let mut state = AppState::new();
        state.open_modal(Modal::Confirm {
            title: "t".into(),
            body: "b".into(),
        });
        update(&mut state, key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!state.should_quit, "first Esc closes the modal only");
        assert!(state.modal.is_none());

        update(&mut state, key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(state.should_quit, "second Esc (no modal) quits");
    }

    /// Tab cycles with wrap-around; Backtab cycles backwards.
    #[test]
    fn tab_cycles_screens_with_wraparound() {
        let mut state = AppState::new();
        assert_eq!(state.screen, Screen::Dashboard);

        update(&mut state, key(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(state.screen, Screen::Logs);
        update(&mut state, key(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(state.screen, Screen::Tags);
        update(&mut state, key(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(state.screen, Screen::Alarms);
        update(&mut state, key(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(state.screen, Screen::Projects);
        update(&mut state, key(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(state.screen, Screen::Rig);
        // Wrap-around: Rig → Dashboard.
        update(&mut state, key(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(state.screen, Screen::Dashboard);

        // Backtab wraps the other way: Dashboard → Rig.
        update(&mut state, key(KeyCode::BackTab, KeyModifiers::SHIFT));
        assert_eq!(state.screen, Screen::Rig);
    }

    /// Non-Press key events (Release/Repeat on kitty-protocol
    /// terminals) are ignored — no double-firing.
    #[test]
    fn non_press_key_events_are_ignored() {
        let mut state = AppState::new();
        let release = AppEvent::Input(Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Release,
            state: KeyEventState::NONE,
        }));
        update(&mut state, release);
        assert!(!state.should_quit, "Release must not quit");

        let repeat = AppEvent::Input(Event::Key(KeyEvent {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Repeat,
            state: KeyEventState::NONE,
        }));
        update(&mut state, repeat);
        assert_eq!(state.screen, Screen::Dashboard, "Repeat must not cycle");
    }

    /// The Input modal edits its buffer: chars append, Backspace pops,
    /// Esc cancels.
    #[test]
    fn modal_input_buffer_edits() {
        let mut state = AppState::new();
        state.open_modal(Modal::Input {
            title: "username".into(),
            buffer: String::new(),
        });

        for ch in ['a', 'd', 'm'] {
            update(&mut state, key(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        update(&mut state, key(KeyCode::Backspace, KeyModifiers::NONE));
        update(&mut state, key(KeyCode::Char('n'), KeyModifiers::NONE));

        assert_eq!(
            state.modal,
            Some(Modal::Input {
                title: "username".into(),
                buffer: "adn".into(), // "adm" − backspace + "n"
            })
        );

        // Esc cancels the modal entirely.
        update(&mut state, key(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(state.modal, None);
        assert!(!state.should_quit);
    }

    /// Worker errors surface as a dismissable result modal.
    #[test]
    fn error_events_open_a_result_modal() {
        let mut state = AppState::new();
        update(&mut state, AppEvent::Error("gateway unreachable".into()));
        assert!(matches!(state.modal, Some(Modal::Result_ { .. })));
        assert!(!state.should_quit);
    }

    /// A current-era Refresh fills DashboardData and clears the busy
    /// guard (the 'r' keystroke's exit condition).
    #[test]
    fn refresh_fills_dashboard_data_and_clears_busy() {
        let mut state = AppState::new();
        state.dashboard.busy = true;
        let era = state.era;

        update(
            &mut state,
            AppEvent::Refresh {
                era,
                snapshot: Box::new(crate::workers::refresh::Snapshot::default()),
            },
        );

        assert!(state.dashboard.snapshot.is_some(), "snapshot stored");
        assert!(state.dashboard.last_refresh.is_some(), "freshness recorded");
        assert!(!state.dashboard.busy, "busy clears");
    }

    /// A stale-era Refresh is dropped WHOLE — a snapshot from the
    /// pre-switch profile never lands, not even partially (Pitfall 9).
    #[test]
    fn stale_era_refresh_is_dropped() {
        let mut state = AppState::new();
        state.dashboard.busy = true;
        let stale_era = state.era.wrapping_sub(1);

        update(
            &mut state,
            AppEvent::Refresh {
                era: stale_era,
                snapshot: Box::new(crate::workers::refresh::Snapshot::default()),
            },
        );

        assert!(state.dashboard.snapshot.is_none(), "stale snapshot dropped");
        assert!(state.dashboard.last_refresh.is_none());
        assert!(state.dashboard.busy, "stale refresh does not clear busy");
    }

    /// `r` on the Dashboard triggers a refresh through the busy guard
    /// (state-machine half; the spawn is runtime-gated).
    #[test]
    fn r_key_triggers_manual_refresh_once() {
        let mut state = AppState::new();
        state.client = Some(crate::state::ClientHandle(std::sync::Arc::new(
            ignition_core::client::ReqwestGatewayApi::for_tests("http://127.0.0.1:1/", None),
        )));
        state.events_tx = Some(tokio::sync::mpsc::unbounded_channel().0);
        assert!(!state.dashboard.busy);
        update(&mut state, key(KeyCode::Char('r'), KeyModifiers::NONE));
        assert!(state.dashboard.busy, "manual refresh marks busy");

        // Second `r` while busy: refused (no stack).
        update(&mut state, key(KeyCode::Char('r'), KeyModifiers::NONE));
        assert!(state.dashboard.busy);
    }

    /// Up/Down move the sessions cursor within the flattened rows,
    /// clamped; without a snapshot they are no-ops.
    #[test]
    fn up_down_move_the_sessions_cursor() {
        let mut state = AppState::new();

        // No snapshot yet: no-op.
        update(&mut state, key(KeyCode::Down, KeyModifiers::NONE));
        assert!(state.dashboard.sessions_table.selected().is_none());

        // One selectable row.
        let snapshot = crate::workers::refresh::Snapshot {
            sessions: Some(ignition_core::actions::sessions::SessionsResult {
                designers: vec![
                    serde_json::from_value(serde_json::json!({"id": "d-1", "user": "admin"}))
                        .expect("designer fixture"),
                ],
                perspective: Vec::new(),
                vision: Vec::new(),
            }),
            ..Default::default()
        };
        state.dashboard.snapshot = Some(snapshot);

        update(&mut state, key(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(state.dashboard.sessions_table.selected(), Some(0));

        // Clamp at len-1 (one row → stays 0).
        update(&mut state, key(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(state.dashboard.sessions_table.selected(), Some(0));
    }
}
