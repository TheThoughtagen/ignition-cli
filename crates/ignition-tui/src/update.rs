//! The cockpit's Elm-style update — PURE AND SYNC.
//!
//! This file must never await anything: gateway I/O lives only in
//! `workers/*` (research anti-pattern #1 — the cardinal sin this
//! architecture exists to prevent). `update` takes the state and one
//! event, mutates, returns; that is all.

use crossterm::event::{Event, KeyCode, KeyModifiers};

use crate::event::AppEvent;
use crate::state::{AppState, Modal};

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
        _ => {}
    }
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
        update(
            &mut state,
            key(KeyCode::Char('c'), KeyModifiers::CONTROL),
        );
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
        update(
            &mut state,
            key(KeyCode::BackTab, KeyModifiers::SHIFT),
        );
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
}
