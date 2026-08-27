//! The cockpit's Elm-style update — PURE AND SYNC.
//!
//! This file must never await anything: gateway I/O lives only in
//! `workers/*` (research anti-pattern #1 — the cardinal sin this
//! architecture exists to prevent). `update` takes the state and one
//! event, mutates, returns; that is all.

use std::sync::Arc;

use crossterm::event::{Event, KeyCode, KeyModifiers};
use ignition_core::client::ReqwestGatewayApi;

use crate::event::AppEvent;
use crate::state::{
    ACTIONS, AppState, Modal, PendingAction, PendingInput, Screen, SessionRow, session_rows,
};
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
                scroll: 0,
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
        // A one-shot action finished: clear the busy guard and show the
        // result modal (pretty JSON or the error — one mechanism for
        // every verb). Stale-era results are dropped (Pitfall 9).
        AppEvent::ActionDone { era, label, result } => {
            if workers::is_current(state.era, era) {
                state.dashboard.in_flight = None;
                let lines: Vec<String> = match result {
                    Ok(json) => json.lines().map(str::to_string).collect(),
                    Err(message) => message.lines().map(str::to_string).collect(),
                };
                state.open_modal(Modal::Result_ {
                    title: label.to_string(),
                    lines,
                    scroll: 0,
                });
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

/// The dashboard keymap (06-02): `r` refreshes now (busy guarded),
/// `a` opens the actions menu, `t`/Enter terminate the selected
/// session (confirm-gated), Up/Down move the sessions-table cursor.
fn dashboard_keys(state: &mut AppState, code: KeyCode) {
    match code {
        KeyCode::Char('r') => workers::refresh::spawn_refresh_once(state),
        KeyCode::Char('a') => state.open_modal(Modal::Actions { selected: 0 }),
        KeyCode::Char('t') | KeyCode::Enter => open_terminate_confirm(state),
        KeyCode::Up => move_session_selection(state, -1),
        KeyCode::Down => move_session_selection(state, 1),
        _ => {}
    }
}

/// The dashboard's client Arc, cloned out of the state handle.
fn client_arc(state: &AppState) -> Option<Arc<ReqwestGatewayApi>> {
    state.client.as_ref().map(|handle| handle.0.clone())
}

/// The currently selected sessions row (snapshot + cursor), if any.
fn selected_session_row(state: &AppState) -> Option<SessionRow> {
    let snapshot = state.dashboard.snapshot.as_ref()?;
    let sessions = snapshot.sessions.as_ref()?;
    let index = state.dashboard.sessions_table.selected()?;
    session_rows(sessions).into_iter().nth(index)
}

/// `t`/Enter on a selected session: arm the pending action and open the
/// Confirm modal — the TUI-side `--yes` for the guarded verb. No
/// selection, no modal.
fn open_terminate_confirm(state: &mut AppState) {
    if let Some(row) = selected_session_row(state) {
        state.dashboard.pending = Some(PendingAction::TerminateSession {
            kind: row.kind,
            id: row.id.clone(),
        });
        state.open_modal(Modal::Confirm {
            title: "terminate session".to_string(),
            body: format!("terminate {} session {} ({})?", row.kind, row.id, row.user),
        });
    }
}

/// Execute actions-menu entry `index` (Enter in the Actions modal).
/// Everything except `wait module` (Input prompt) and `restart`
/// (Confirm first — the guarded verb) spawns immediately.
fn execute_menu_action(state: &mut AppState, index: usize) {
    match ACTIONS.get(index).copied() {
        Some("version") => {
            if let Some(client) = client_arc(state) {
                workers::spawn_action(state, "version", async move {
                    ignition_core::actions::version::version(
                        Some(&*client),
                        env!("CARGO_PKG_VERSION"),
                    )
                    .await
                });
            }
        }
        Some("connections") => {
            if let Some(client) = client_arc(state) {
                workers::spawn_action(state, "connections", async move {
                    ignition_core::actions::connections::connections(&*client, None).await
                });
            }
        }
        Some("wait gateway") => {
            if let Some(client) = client_arc(state) {
                workers::spawn_action(state, "wait gateway", async move {
                    ignition_core::actions::restart::wait_gateway(
                        &*client,
                        ignition_core::actions::restart::DEFAULT_INTERVAL,
                        ignition_core::actions::restart::READINESS_TIMEOUT,
                    )
                    .await
                });
            }
        }
        Some("wait restart") => {
            if let Some(client) = client_arc(state) {
                workers::spawn_action(state, "wait restart", async move {
                    ignition_core::actions::restart::wait_restart(
                        &*client,
                        ignition_core::actions::restart::DEFAULT_INTERVAL,
                        ignition_core::actions::restart::RESTART_TIMEOUT,
                        ignition_core::actions::restart::RESTART_FLOOR,
                    )
                    .await
                });
            }
        }
        Some("wait module") => {
            state.dashboard.pending_input = Some(PendingInput::WaitModule);
            state.open_modal(Modal::Input {
                title: "module id".to_string(),
                buffer: String::new(),
            });
        }
        Some("doctor") => {
            if let (Some(client), Some(url)) = (client_arc(state), state.profile_url.clone()) {
                workers::spawn_action(state, "doctor", async move {
                    // doctor NEVER fails — the diagnosis completing IS
                    // success (failing checks are data).
                    Ok(ignition_core::actions::doctor::doctor(
                        &*client,
                        &url,
                        true, // the cockpit's REQUIRED credential always resolved
                        &ignition_core::actions::doctor::DoctorOptions::default(),
                    )
                    .await)
                });
            }
        }
        Some("restart") => {
            // The guarded verb: the Confirm modal opens FIRST; accept ≡
            // `--yes` (the TUI owns its confirmation, the action fn
            // itself is called unguarded), cancel spawns nothing.
            state.dashboard.pending = Some(PendingAction::Restart);
            state.open_modal(Modal::Confirm {
                title: "restart gateway".to_string(),
                body: "restart the gateway now? (takes it down for ~1 min)".to_string(),
            });
        }
        _ => {}
    }
}

/// Execute a confirmed pending action (Confirm-modal `y`).
fn execute_pending(state: &mut AppState, pending: &PendingAction) {
    match pending {
        PendingAction::Restart => {
            if let Some(client) = client_arc(state) {
                workers::spawn_action(state, "restart", async move {
                    ignition_core::actions::restart::restart(&*client).await
                });
            }
        }
        PendingAction::TerminateSession { kind, id } => {
            if let Some(client) = client_arc(state) {
                let kind = *kind;
                let id = id.clone();
                workers::spawn_action(state, "sessions terminate", async move {
                    ignition_core::actions::sessions::terminate_session(&*client, kind, &id, None)
                        .await
                });
            }
        }
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

/// Keystrokes while a modal is open: the modal-specific acceptors
/// first (Actions menu nav, Confirm `y`, Input Enter, Result_ scroll),
/// then the Input buffer editing, then Esc (closes, clearing pending).
fn handle_modal_input(state: &mut AppState, code: KeyCode, modifiers: KeyModifiers) {
    // The Actions menu: Up/Down move, Enter executes. Long waits run in
    // the worker with NO UI block — only the status line's in-flight
    // label shows while they run.
    if let Some(Modal::Actions { selected }) = state.modal.as_mut()
        && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        match code {
            KeyCode::Up => {
                *selected = selected.saturating_sub(1);
                return;
            }
            KeyCode::Down => {
                *selected = (*selected + 1).min(ACTIONS.len() - 1);
                return;
            }
            KeyCode::Enter => {
                let index = *selected;
                state.close_modal();
                clear_pending(state);
                execute_menu_action(state, index);
                return;
            }
            _ => {}
        }
    }

    // Confirm accept: `y` executes the pending action (the TUI-side
    // `--yes`); anything but `y` falls through (Esc cancels).
    if matches!(state.modal, Some(Modal::Confirm { .. })) && code == KeyCode::Char('y') {
        state.close_modal();
        let pending = state.dashboard.pending.take();
        if let Some(pending) = pending {
            execute_pending(state, &pending);
        }
        return;
    }

    // Input accept: Enter routes the buffer by what the form was for.
    if let Some(Modal::Input { buffer, .. }) = &state.modal
        && code == KeyCode::Enter
    {
        let value = buffer.clone();
        state.close_modal();
        match (state.dashboard.pending_input.take(), value.is_empty()) {
            (Some(PendingInput::WaitModule), false) => {
                if let Some(client) = client_arc(state) {
                    workers::spawn_action(state, "wait module", async move {
                        ignition_core::actions::restart::wait_module(
                            &*client,
                            &value,
                            ignition_core::actions::restart::DEFAULT_INTERVAL,
                            ignition_core::actions::restart::READINESS_TIMEOUT,
                        )
                        .await
                    });
                }
            }
            _ => clear_pending(state),
        }
        return;
    }

    // Result modal: PgUp/PgDn scroll (clamped to the content).
    if let Some(Modal::Result_ { lines, scroll, .. }) = state.modal.as_mut() {
        match code {
            KeyCode::PageUp => {
                *scroll = scroll.saturating_sub(1);
                return;
            }
            KeyCode::PageDown => {
                *scroll = (*scroll + 1).min(lines.len() as u16);
                return;
            }
            _ => {}
        }
    }

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
    // behind a modal) — and cancels any pending action/input with it,
    // so a stale Confirm can never arm a later, unrelated `y`.
    if code == KeyCode::Esc {
        state.close_modal();
        clear_pending(state);
    }
}

/// Cancel any armed modal payload (Esc / menu-Enter fresh start).
fn clear_pending(state: &mut AppState) {
    state.dashboard.pending = None;
    state.dashboard.pending_input = None;
}

#[cfg(test)]
mod tests {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    use super::update;
    use crate::event::AppEvent;
    use crate::state::{AppState, Modal, PendingAction, PendingInput, Screen};

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
            scroll: 0,
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

    /// A dashboard state with one designer session selected — the
    /// terminate-flow fixture.
    fn state_with_selected_session() -> AppState {
        let mut state = AppState::new();
        state.client = Some(crate::state::ClientHandle(std::sync::Arc::new(
            ignition_core::client::ReqwestGatewayApi::for_tests("http://127.0.0.1:1/", None),
        )));
        state.events_tx = Some(tokio::sync::mpsc::unbounded_channel().0);
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
        state.dashboard.sessions_table.select(Some(0));
        state
    }

    /// `t` on a selected session arms the pending action and opens the
    /// Confirm modal — nothing runs yet (guarded verb).
    #[test]
    fn t_opens_terminate_confirm_without_running() {
        let mut state = state_with_selected_session();
        update(&mut state, key(KeyCode::Char('t'), KeyModifiers::NONE));

        assert!(matches!(state.modal, Some(Modal::Confirm { .. })));
        assert_eq!(
            state.dashboard.pending,
            Some(PendingAction::TerminateSession {
                kind: ignition_core::actions::sessions::SessionType::Designer,
                id: "d-1".to_string(),
            })
        );
        assert!(state.dashboard.in_flight.is_none(), "nothing spawned yet");
    }

    /// Confirm-cancel spawns nothing: Esc clears the pending action AND
    /// the modal — a stale Confirm can never arm a later `y`.
    #[test]
    fn confirm_cancel_spawns_nothing_and_clears_pending() {
        let mut state = state_with_selected_session();
        update(&mut state, key(KeyCode::Char('t'), KeyModifiers::NONE));
        update(&mut state, key(KeyCode::Esc, KeyModifiers::NONE));

        assert!(state.modal.is_none(), "modal closed");
        assert!(state.dashboard.pending.is_none(), "pending cleared");
        assert!(state.dashboard.in_flight.is_none(), "nothing in flight");
    }

    /// Confirm-accept ≡ `--yes`: the terminate moves to in-flight.
    #[test]
    fn confirm_accept_moves_terminate_to_in_flight() {
        let mut state = state_with_selected_session();
        update(&mut state, key(KeyCode::Char('t'), KeyModifiers::NONE));
        update(&mut state, key(KeyCode::Char('y'), KeyModifiers::NONE));

        assert!(state.modal.is_none(), "confirm closed on accept");
        assert!(state.dashboard.pending.is_none(), "pending consumed");
        assert_eq!(
            state.dashboard.in_flight,
            Some("sessions terminate"),
            "terminate is in flight"
        );
    }

    /// Actions menu: `a` opens it, Enter on `restart` (the guarded
    /// verb, last entry) opens the Confirm modal — spawning nothing
    /// until accepted.
    #[test]
    fn actions_menu_restart_requires_confirm_first() {
        let mut state = state_with_selected_session();

        update(&mut state, key(KeyCode::Char('a'), KeyModifiers::NONE));
        assert!(matches!(state.modal, Some(Modal::Actions { selected: 0 })));

        // Down ×6 → `restart` (last of 7 entries).
        for _ in 0..6 {
            update(&mut state, key(KeyCode::Down, KeyModifiers::NONE));
        }
        assert_eq!(
            state.modal,
            Some(Modal::Actions { selected: 6 }),
            "cursor clamped at restart"
        );

        update(&mut state, key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(state.modal, Some(Modal::Confirm { .. })));
        assert_eq!(state.dashboard.pending, Some(PendingAction::Restart));
        assert!(state.dashboard.in_flight.is_none(), "restart waits for y");

        // `y` ≡ --yes: the action fn fires (unguarded — the TUI owned
        // the confirmation).
        update(&mut state, key(KeyCode::Char('y'), KeyModifiers::NONE));
        assert_eq!(state.dashboard.in_flight, Some("restart"));
    }

    /// Actions menu: Enter on `version` (first entry) goes in-flight
    /// immediately (unguarded read).
    #[test]
    fn actions_menu_version_spawns_directly() {
        let mut state = state_with_selected_session();
        update(&mut state, key(KeyCode::Char('a'), KeyModifiers::NONE));
        update(&mut state, key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(state.dashboard.in_flight, Some("version"));
    }

    /// Actions menu: `wait module` prompts for the id; Enter with a
    /// typed id spawns, empty input cancels.
    #[test]
    fn wait_module_prompts_then_spawns_with_typed_id() {
        let mut state = state_with_selected_session();

        // Down ×4 → `wait module`.
        update(&mut state, key(KeyCode::Char('a'), KeyModifiers::NONE));
        for _ in 0..4 {
            update(&mut state, key(KeyCode::Down, KeyModifiers::NONE));
        }
        update(&mut state, key(KeyCode::Enter, KeyModifiers::NONE));

        assert!(matches!(state.modal, Some(Modal::Input { .. })));
        assert_eq!(
            state.dashboard.pending_input,
            Some(PendingInput::WaitModule)
        );
        assert!(state.dashboard.in_flight.is_none(), "waiting for the id");

        // Type the id, Enter → in flight.
        for ch in ['v', 'i', 's', 'i', 'o', 'n'] {
            update(&mut state, key(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        update(&mut state, key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(state.dashboard.in_flight, Some("wait module"));

        // Empty input cancels (pending cleared, nothing in flight).
        let mut fresh = state_with_selected_session();
        update(&mut fresh, key(KeyCode::Char('a'), KeyModifiers::NONE));
        for _ in 0..4 {
            update(&mut fresh, key(KeyCode::Down, KeyModifiers::NONE));
        }
        update(&mut fresh, key(KeyCode::Enter, KeyModifiers::NONE));
        update(&mut fresh, key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(fresh.modal.is_none());
        assert!(fresh.dashboard.in_flight.is_none());
    }

    /// ActionDone (current era) clears in-flight and opens the result
    /// modal with the pretty-JSON lines; a stale-era result is dropped.
    #[test]
    fn action_done_opens_result_modal_and_stale_is_dropped() {
        let mut state = state_with_selected_session();
        let era = state.era;

        update(
            &mut state,
            AppEvent::ActionDone {
                era,
                label: "wait gateway",
                result: Ok("{\n  \"target\": \"gateway\"\n}".to_string()),
            },
        );
        assert!(state.dashboard.in_flight.is_none(), "guard clears");
        match &state.modal {
            Some(Modal::Result_ {
                title,
                lines,
                scroll,
            }) => {
                assert_eq!(title, "wait gateway");
                assert_eq!(lines.len(), 3, "pretty JSON split to lines");
                assert_eq!(*scroll, 0);
            }
            other => panic!("result modal open, got {other:?}"),
        }

        // Stale era: dropped whole — an error from the pre-switch
        // profile must not clear the current guard or open anything.
        state.dashboard.in_flight = Some("version");
        let modal_before = state.modal.clone();
        update(
            &mut state,
            AppEvent::ActionDone {
                era: era.wrapping_sub(1),
                label: "version",
                result: Err("old world".to_string()),
            },
        );
        assert_eq!(state.dashboard.in_flight, Some("version"), "stale dropped");
        assert_eq!(state.modal, modal_before, "stale opens nothing");
    }

    /// Result modal scrolls: PgUp floors at 0, PgDown advances (clamped
    /// later at the modal render by content height).
    #[test]
    fn result_modal_pages() {
        let mut state = AppState::new();
        state.open_modal(Modal::Result_ {
            title: "wait gateway".into(),
            lines: vec!["one".into(), "two".into(), "three".into()],
            scroll: 0,
        });

        update(&mut state, key(KeyCode::PageUp, KeyModifiers::NONE));
        assert_eq!(
            state.modal,
            Some(Modal::Result_ {
                title: "wait gateway".into(),
                lines: vec!["one".into(), "two".into(), "three".into()],
                scroll: 0
            }),
            "PgUp floors at 0"
        );

        update(&mut state, key(KeyCode::PageDown, KeyModifiers::NONE));
        assert!(matches!(&state.modal, Some(Modal::Result_ { scroll, .. }) if *scroll == 1));
    }
}
