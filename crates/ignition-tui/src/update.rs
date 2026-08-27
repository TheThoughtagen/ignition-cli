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
    ACTIONS, AppState, LOG_ACTIONS, Modal, PendingAction, PendingInput, Screen, SessionRow,
    session_rows,
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
        // One streamed log entry (06-03): append to the ring under its
        // cap. Not era-stamped (plan-locked: ring turnover is the
        // acceptance policy; the worker's shutdown watch is the scope).
        AppEvent::LogLine(entry) => state.logs.push_line(entry),
        // The dashboard refresh: store, clear busy, record freshness.
        // Stale-era snapshots are dropped whole (Pitfall 9) — a result
        // from the pre-switch profile never lands, not even partially.
        // A landed refresh also retires the switch banner (the new
        // world's data just arrived — the confirmation served).
        AppEvent::Refresh { era, snapshot } => {
            if workers::is_current(state.era, era) {
                state.dashboard.snapshot = Some(*snapshot);
                state.dashboard.last_refresh = Some(std::time::Instant::now());
                state.dashboard.busy = false;
                state.banner = None;
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
        // A profile switch landed: the era-gated banner confirmation
        // ("profile: NAME" in the status line until the new world's
        // first refresh retires it). Stale banners drop (Pitfall 9).
        AppEvent::ProfileChanged { era, name } => {
            if workers::is_current(state.era, era) {
                state.banner = Some(format!("profile: {name}"));
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
        // Tab / Shift+Tab cycle screens (wrap-around). Screen-scoped
        // workers (the logs tail) start/stop on the transition.
        KeyCode::Tab => set_screen(state, state.screen.next()),
        KeyCode::BackTab => set_screen(state, state.screen.prev()),
        // `p` opens the profile switcher from ANY screen (no modal).
        KeyCode::Char('p') => open_profiles_modal(state),
        // Everything else belongs to the active screen's keymap.
        _ => handle_screen_keys(state, key.code),
    }
}

/// Change the active screen, starting/stopping the screen-scoped
/// workers on the transition: LEAVING the Logs screen stops the tail
/// (must-have truth #3), entering it re-arms the tail resuming past
/// the ring's newest entry (no duplicate flood).
fn set_screen(state: &mut AppState, screen: Screen) {
    if state.screen == screen {
        return;
    }
    if state.screen == Screen::Logs {
        workers::tail::stop_tail(state);
    }
    state.screen = screen;
    if state.screen == Screen::Logs {
        workers::tail::spawn_tail(state);
    }
}

/// Open the profile switcher: the config's profile names (BTreeMap
/// order), the active one marked. A config that fails to load opens
/// the error modal instead (local I/O, sync, milliseconds — not
/// gateway I/O).
fn open_profiles_modal(state: &mut AppState) {
    match ignition_core::config::load(&ignition_core::config::config_path()) {
        Ok(config) => {
            let names: Vec<String> = config.profiles.keys().cloned().collect();
            state.open_modal(Modal::Profiles {
                names,
                active: config.active.clone(),
                selected: 0,
            });
        }
        Err(err) => open_error_modal(state, "profile list failed", &err.to_string()),
    }
}

/// The small error display (a dismissable result modal).
fn open_error_modal(state: &mut AppState, title: &str, message: &str) {
    state.open_modal(Modal::Result_ {
        title: title.to_string(),
        lines: message.lines().map(str::to_string).collect(),
        scroll: 0,
    });
}

/// Switch to `name` — the LOCKED sequence, ordered so a failure keeps
/// the old world WHOLE (atomic from the user's perspective):
///
/// 1. `context::rebuild` — resolves the named profile's client; a
///    failure (unknown name, missing secret) shows the error modal and
///    NOTHING persisted, era untouched, old workers still running.
/// 2. `profile::use_profile` — persists the active name; a save
///    failure keeps the old world (config unwritten).
/// 3. Adopt: stop the old refresh worker, swap client/url/name, reset
///    the dashboard to Loading, re-spawn under a bumped era, and post
///    the era-stamped `ProfileChanged` banner.
fn switch_profile(state: &mut AppState, name: &str) {
    let rebuilt = match crate::context::rebuild(name) {
        Ok((resolved_name, url, api)) => (resolved_name, url, api),
        Err(err) => {
            open_error_modal(state, "profile switch failed", &err.to_string());
            return;
        }
    };
    if let Err(err) =
        ignition_core::actions::profile::use_profile(&ignition_core::config::config_path(), name)
    {
        open_error_modal(state, "profile switch failed", &err.to_string());
        return;
    }

    let (resolved_name, url, api) = rebuilt;
    // Stop the old world's refresh worker BEFORE adopting (its results
    // are already stale — the era bump below formally retires them).
    // The screen-scoped tail stops too, and its ring clears: the new
    // world's Logs screen starts from the new gateway's buffer.
    if let Some(shutdown) = &state.refresh_shutdown {
        let _ = shutdown.send(true);
    }
    workers::tail::stop_tail(state);
    state.client = Some(crate::state::ClientHandle(api));
    state.profile_url = Some(url);
    state.profile = Some(resolved_name.clone());
    state.dashboard = crate::state::DashboardData::default();
    state.logs = crate::state::LogsData::default();
    state.close_modal();
    // Re-spawn under a new era (bumps) + post the banner through the
    // rail so the loop redraws on it.
    workers::refresh::spawn_refresh(state);
    if state.screen == Screen::Logs {
        workers::tail::spawn_tail(state);
    }
    if let Some(tx) = &state.events_tx {
        let _ = tx.send(AppEvent::ProfileChanged {
            era: state.era,
            name: resolved_name,
        });
    }
}

/// Submit the profile add form: `profile::add` with default auth (the
/// generic `IGNITION_TOKEN` env path — auth refs stay on the CLI form
/// per the LOCKED modal-depth decision), then re-open the switcher
/// with the refreshed list.
fn submit_profile_add(state: &mut AppState) {
    let Some(Modal::ProfileAdd { name, url, .. }) = &state.modal else {
        return;
    };
    let (name, url) = (name.clone(), url.clone());
    if name.is_empty() || url.is_empty() {
        open_error_modal(state, "profile add", "name and url are both required");
        return;
    }
    match ignition_core::actions::profile::add(
        &ignition_core::config::config_path(),
        &name,
        &url,
        None,
        ignition_core::config::AuthRef::default(),
        false,
    ) {
        Ok(_) => open_profiles_modal(state),
        Err(err) => open_error_modal(state, "profile add failed", &err.to_string()),
    }
}

/// Screen-local keymaps (no modal open). Screens not yet wired (06-04+)
/// take no keys beyond the global set.
fn handle_screen_keys(state: &mut AppState, code: KeyCode) {
    match state.screen {
        Screen::Dashboard => dashboard_keys(state, code),
        Screen::Logs => logs_keys(state, code),
        _ => {}
    }
}

/// The Logs keymap (06-03): `l` cycles the level filter (restarting
/// the tail with the new min_level — the filter also applies at render
/// over the retained ring), `f` toggles follow, j/k/Up/Down/PgUp/PgDn
/// scroll the filtered view (scrolling up disables follow), `G`/End
/// jump back to the newest line and re-enable follow.
fn logs_keys(state: &mut AppState, code: KeyCode) {
    match code {
        KeyCode::Char('l') => {
            state.logs.filter = state.logs.filter.next();
            // Shutdown + respawn with the new min_level (the render-side
            // filter keeps already-received entries visible).
            workers::tail::spawn_tail(state);
        }
        KeyCode::Char('f') => state.logs.toggle_follow(),
        KeyCode::Char('k') | KeyCode::Up => state.logs.scroll_up(1),
        KeyCode::PageUp => state.logs.scroll_up(10),
        KeyCode::Char('j') | KeyCode::Down => state.logs.scroll_down(1),
        KeyCode::PageDown => state.logs.scroll_down(10),
        KeyCode::Char('g') | KeyCode::Char('G') | KeyCode::End => state.logs.jump_to_end(),
        // The loggers family lives behind the actions menu (the tail
        // keeps streaming independently — the menu verbs are one-shot
        // workers on a separate spawn).
        KeyCode::Char('a') => state.open_modal(Modal::LogsActions { selected: 0 }),
        _ => {}
    }
}

/// Parse the `loggers set` input line: exactly `LOGGER LEVEL`, the
/// level one of the seven wire tokens (case-insensitive — normalized
/// uppercase). A String error so the caller can open the error modal
/// (the clap value_enum refusal's TUI twin).
const WIRE_LEVELS: [&str; 7] = ["TRACE", "DEBUG", "INFO", "WARN", "ERROR", "FATAL", "OFF"];

fn parse_logger_level_line(line: &str) -> Result<(String, String), String> {
    let mut parts = line.split_whitespace();
    match (parts.next(), parts.next(), parts.next()) {
        (Some(logger), Some(level), None) => {
            let level = level.to_ascii_uppercase();
            if WIRE_LEVELS.contains(&level.as_str()) {
                Ok((logger.to_string(), level))
            } else {
                Err(format!(
                    "unknown level {level:?} — expected one of {}",
                    WIRE_LEVELS.join(" ")
                ))
            }
        }
        _ => Err("expected `LOGGER LEVEL` (e.g. `GatewayManager WARN`)".to_string()),
    }
}

/// Execute a Logs-actions-menu entry `index` (Enter in the LogsActions
/// modal): `loggers list` prompts for the optional search, `loggers
/// set` prompts for the `LOGGER LEVEL` line, `loggers reset` goes
/// straight to the Confirm gate. The two mutations are `--yes`-guarded
/// in the CLI (main.rs) — the Confirm modal IS the TUI's `--yes`.
fn execute_logs_menu_action(state: &mut AppState, index: usize) {
    match LOG_ACTIONS.get(index).copied() {
        Some("loggers list") => {
            state.dashboard.pending_input = Some(PendingInput::LoggersSearch);
            state.open_modal(Modal::Input {
                title: "logger search (optional)".to_string(),
                buffer: String::new(),
            });
        }
        Some("loggers set") => {
            state.dashboard.pending_input = Some(PendingInput::LoggersSetLine);
            state.open_modal(Modal::Input {
                title: "LOGGER LEVEL".to_string(),
                buffer: String::new(),
            });
        }
        Some("loggers reset") => {
            state.dashboard.pending = Some(PendingAction::LoggersReset);
            state.open_modal(Modal::Confirm {
                title: "loggers reset".to_string(),
                body: "reset ALL logger levels to their defaults?".to_string(),
            });
        }
        _ => {}
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
        // The confirmed loggers mutations fire unguarded — the TUI owned
        // the `--yes` (the CLI guard contract, caller-owns-guard).
        PendingAction::LoggersSet { logger, level } => {
            if let Some(client) = client_arc(state) {
                let logger = logger.clone();
                let level = level.clone();
                workers::spawn_action(state, "loggers set", async move {
                    ignition_core::actions::logs::set_logger_level(&*client, &logger, &level).await
                });
            }
        }
        PendingAction::LoggersReset => {
            if let Some(client) = client_arc(state) {
                workers::spawn_action(state, "loggers reset", async move {
                    ignition_core::actions::logs::reset_logger_levels(&*client).await
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
    // The profile switcher: Up/Down move, Enter switches, `a` opens
    // the add form.
    if let Some(Modal::Profiles {
        names, selected, ..
    }) = state.modal.as_mut()
        && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        match code {
            KeyCode::Up => {
                *selected = selected.saturating_sub(1);
                return;
            }
            KeyCode::Down => {
                if !names.is_empty() {
                    *selected = (*selected + 1).min(names.len() - 1);
                }
                return;
            }
            KeyCode::Char('a') => {
                state.open_modal(Modal::ProfileAdd {
                    name: String::new(),
                    url: String::new(),
                    field: 0,
                });
                return;
            }
            KeyCode::Enter => {
                let pick = (*selected).min(names.len().saturating_sub(1));
                let Some(name) = names.get(pick).cloned() else {
                    return;
                };
                switch_profile(state, &name);
                return;
            }
            _ => {}
        }
    }

    // The profile add form: Tab toggles the edited field, Enter
    // submits; plain chars/Backspace route to the active field below.
    if let Some(Modal::ProfileAdd { field, .. }) = state.modal.as_mut()
        && code == KeyCode::Tab
    {
        *field = (*field + 1) % 2;
        return;
    }
    if matches!(state.modal, Some(Modal::ProfileAdd { .. })) && code == KeyCode::Enter {
        submit_profile_add(state);
        return;
    }

    // The Logs actions menu: the same nav shape as the dashboard's
    // menu, over the loggers family.
    if let Some(Modal::LogsActions { selected }) = state.modal.as_mut()
        && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        match code {
            KeyCode::Up => {
                *selected = selected.saturating_sub(1);
                return;
            }
            KeyCode::Down => {
                *selected = (*selected + 1).min(LOG_ACTIONS.len() - 1);
                return;
            }
            KeyCode::Enter => {
                let index = *selected;
                state.close_modal();
                clear_pending(state);
                execute_logs_menu_action(state, index);
                return;
            }
            _ => {}
        }
    }

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
            // The optional search: empty = no filter (the bare list).
            (Some(PendingInput::LoggersSearch), _) => {
                if let Some(client) = client_arc(state) {
                    let search = if value.is_empty() { None } else { Some(value) };
                    workers::spawn_action(state, "loggers list", async move {
                        ignition_core::actions::logs::loggers(&*client, search.as_deref()).await
                    });
                }
            }
            // The `LOGGER LEVEL` line: parsed BEFORE the Confirm gate
            // arms; a bad line opens the error modal and arms nothing.
            (Some(PendingInput::LoggersSetLine), false) => match parse_logger_level_line(&value) {
                Ok((logger, level)) => {
                    let body = format!("set {logger} to {level}?");
                    state.dashboard.pending = Some(PendingAction::LoggersSet { logger, level });
                    state.open_modal(Modal::Confirm {
                        title: "loggers set".to_string(),
                        body,
                    });
                }
                Err(reason) => open_error_modal(state, "loggers set", &reason),
            },
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

    // Plain-text chars route to the Input buffer (or the add form's
    // active field) FIRST — typing 'q' in a username prompt must
    // insert 'q', not quit.
    if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
        match (state.modal.as_mut(), code) {
            (Some(Modal::Input { buffer, .. }), KeyCode::Char(c)) => {
                buffer.push(c);
                return;
            }
            (Some(Modal::Input { buffer, .. }), KeyCode::Backspace) => {
                buffer.pop();
                return;
            }
            (
                Some(Modal::ProfileAdd {
                    name, url, field, ..
                }),
                KeyCode::Char(c),
            ) => {
                match field {
                    0 => name.push(c),
                    _ => url.push(c),
                }
                return;
            }
            (
                Some(Modal::ProfileAdd {
                    name, url, field, ..
                }),
                KeyCode::Backspace,
            ) => {
                match field {
                    0 => {
                        name.pop();
                    }
                    _ => {
                        url.pop();
                    }
                }
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

    // ---- Profile switcher (06-02 Task 3) — isolated-config tests ----
    // env is process-global and lib tests run in parallel: env-mutating
    // tests hold this lock for their whole scope (the context.rs
    // pattern, replicated locally).

    /// Two profiles (dev active, prod), token auth via per-test env
    /// vars — the context.rs fixture, written to a temp config.
    fn isolated_profiles() -> (tempfile::TempDir, Vec<String>) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");
        let mut config = ignition_core::config::Config {
            active: Some("dev".into()),
            ..Default::default()
        };
        for (name, port) in [("dev", 9088), ("prod", 9443)] {
            config.profiles.insert(
                name.into(),
                ignition_core::config::Profile {
                    url: format!("http://localhost:{port}/").parse().expect("url"),
                    label: None,
                    ssl_verify: true,
                    auth: ignition_core::config::AuthRef::default(),
                    webdev_secret: None,
                },
            );
        }
        ignition_core::config::save(&path, &config).expect("save config");

        let mut set_vars = Vec::new();
        for (name, token) in [("dev", "t-dev"), ("prod", "t-prod")] {
            let var = format!("IGNITION_TOKEN_{}", name.to_uppercase());
            unsafe { std::env::set_var(&var, token) };
            set_vars.push(var);
        }
        unsafe { std::env::set_var("IGNITION_CLI_CONFIG", &path) };
        (dir, set_vars)
    }

    /// Scoped env teardown BEFORE the guard drops.
    fn teardown_profiles(vars: &[String]) {
        unsafe { std::env::remove_var("IGNITION_CLI_CONFIG") };
        for var in vars {
            unsafe { std::env::remove_var(var) };
        }
    }

    /// `p` opens the switcher listing every configured profile with the
    /// active one marked.
    #[test]
    fn p_opens_the_profiles_modal() {
        let _guard = crate::ENV_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let (_dir, vars) = isolated_profiles();

        let mut state = AppState::new();
        update(&mut state, key(KeyCode::Char('p'), KeyModifiers::NONE));
        match &state.modal {
            Some(Modal::Profiles {
                names,
                active,
                selected,
            }) => {
                assert_eq!(names, &vec!["dev".to_string(), "prod".to_string()]);
                assert_eq!(active.as_deref(), Some("dev"), "active marked");
                assert_eq!(*selected, 0);
            }
            other => panic!("profiles modal open, got {other:?}"),
        }

        teardown_profiles(&vars);
    }

    /// The switch state machine, happy path: rebuild → persist → era
    /// bump → OLD shutdown signalled → new rails → dashboard reset →
    /// ProfileChanged posted through the rail.
    #[tokio::test]
    async fn profile_switch_happy_path_retargets_the_world() {
        let _guard = crate::ENV_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let (_dir, vars) = isolated_profiles();

        let mut state = AppState::new();
        state.client = Some(crate::state::ClientHandle(std::sync::Arc::new(
            ignition_core::client::ReqwestGatewayApi::for_tests("http://127.0.0.1:1/", None),
        )));
        state.profile = Some("dev".into());
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        state.events_tx = Some(tx);
        // Pre-arm the rails the way run_loop does (a live shutdown
        // switch the switch must signal + replace).
        let (shutdown_tx, _old_rx) = tokio::sync::watch::channel(false);
        state.refresh_shutdown = Some(shutdown_tx);
        state.dashboard.snapshot = Some(crate::workers::refresh::Snapshot::default());

        // Open the switcher, land on prod, Enter.
        update(&mut state, key(KeyCode::Char('p'), KeyModifiers::NONE));
        update(&mut state, key(KeyCode::Down, KeyModifiers::NONE));
        let era_before = state.era;
        let old_shutdown_rx = state
            .refresh_shutdown
            .as_ref()
            .expect("rails before switch")
            .subscribe();
        update(&mut state, key(KeyCode::Enter, KeyModifiers::NONE));

        // Adopted: profile + era + FRESH shutdown rail + reset dashboard.
        assert_eq!(state.profile.as_deref(), Some("prod"), "profile adopted");
        assert_eq!(state.era, era_before + 1, "era bumped exactly once");
        assert!(
            state.dashboard.snapshot.is_none(),
            "dashboard reset to Loading"
        );
        assert!(state.modal.is_none(), "switcher closed on success");
        let new_shutdown_rx = state
            .refresh_shutdown
            .as_ref()
            .expect("rails after switch")
            .subscribe();
        assert!(
            !new_shutdown_rx.same_channel(&old_shutdown_rx),
            "a fresh shutdown rail armed for the new world"
        );

        // Persisted: config.active now names prod.
        let config = ignition_core::config::load(&ignition_core::config::config_path())
            .expect("config reloads");
        assert_eq!(config.active.as_deref(), Some("prod"));

        // The banner event rode the rail, stamped with the NEW era.
        match rx.try_recv() {
            Ok(AppEvent::ProfileChanged { era, name }) => {
                assert_eq!((era, name.as_str()), (state.era, "prod"));
            }
            other => panic!("ProfileChanged posted, got {other:?}"),
        }
        // And the arm sets the banner (the status line's source).
        let new_era = state.era;
        update(
            &mut state,
            AppEvent::ProfileChanged {
                era: new_era,
                name: "prod".into(),
            },
        );
        assert_eq!(state.banner.as_deref(), Some("profile: prod"));

        teardown_profiles(&vars);
    }

    /// A failed rebuild keeps the OLD world whole: error modal, era
    /// untouched, config.active unwritten (the switch is atomic —
    /// rebuild runs BEFORE the persist).
    #[test]
    fn failed_rebuild_keeps_old_client_and_era() {
        let _guard = crate::ENV_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let (_dir, vars) = isolated_profiles();
        // Break prod's secret: remove its token so rebuild fails with
        // SecretUnavailable (the REQUIRED-credential chain).
        unsafe { std::env::remove_var("IGNITION_TOKEN_PROD") };

        let mut state = AppState::new();
        state.client = Some(crate::state::ClientHandle(std::sync::Arc::new(
            ignition_core::client::ReqwestGatewayApi::for_tests("http://127.0.0.1:1/", None),
        )));
        state.profile = Some("dev".into());
        state.events_tx = Some(tokio::sync::mpsc::unbounded_channel().0);
        let era_before = state.era;

        update(&mut state, key(KeyCode::Char('p'), KeyModifiers::NONE));
        update(&mut state, key(KeyCode::Down, KeyModifiers::NONE));
        update(&mut state, key(KeyCode::Enter, KeyModifiers::NONE));

        assert!(
            matches!(state.modal, Some(Modal::Result_ { title, .. }) if title == "profile switch failed"),
            "the failed rebuild surfaces as the error modal"
        );
        assert_eq!(state.era, era_before, "era untouched — old workers live");
        assert_eq!(state.profile.as_deref(), Some("dev"), "old profile kept");
        let config = ignition_core::config::load(&ignition_core::config::config_path())
            .expect("config reloads");
        assert_eq!(
            config.active.as_deref(),
            Some("dev"),
            "persist never ran — config unwritten"
        );

        teardown_profiles(&vars);
    }

    /// The era-drop unit (Pitfall 9's profile-switch shape): a worker
    /// stamped with era N reports after the switch bumped to N+1 —
    /// `is_current` is false and update drops the Refresh whole.
    #[test]
    fn stale_era_worker_results_drop_after_switch() {
        let mut state = AppState::new();
        let old_era = state.era;
        crate::workers::new_era(&mut state); // the switch bump
        assert!(!crate::workers::is_current(state.era, old_era));

        state.dashboard.busy = true;
        update(
            &mut state,
            AppEvent::Refresh {
                era: old_era,
                snapshot: Box::new(crate::workers::refresh::Snapshot::default()),
            },
        );
        assert!(state.dashboard.snapshot.is_none(), "old-world data dropped");
        assert!(state.dashboard.busy, "old-world busy state untouched");
    }

    /// The add form: two fields, Tab toggles, Enter persists via
    /// `profile::add` and re-opens the refreshed switcher.
    #[test]
    fn profile_add_form_persists_and_refreshes_the_list() {
        let _guard = crate::ENV_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let (_dir, vars) = isolated_profiles();

        let mut state = AppState::new();
        state.events_tx = Some(tokio::sync::mpsc::unbounded_channel().0);

        // p → a opens the form.
        update(&mut state, key(KeyCode::Char('p'), KeyModifiers::NONE));
        update(&mut state, key(KeyCode::Char('a'), KeyModifiers::NONE));
        assert!(matches!(state.modal, Some(Modal::ProfileAdd { .. })));

        // Type the name, Tab to url, type it, Enter.
        for ch in ['s', 't', 'a', 'g', 'e'] {
            update(&mut state, key(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        update(&mut state, key(KeyCode::Tab, KeyModifiers::NONE));
        for ch in ['h', 't', 't', 'p', ':', '/', '/', 'x'] {
            update(&mut state, key(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        update(&mut state, key(KeyCode::Enter, KeyModifiers::NONE));

        // Back on the switcher, now listing the new profile.
        match &state.modal {
            Some(Modal::Profiles { names, active, .. }) => {
                assert!(names.contains(&"stage".to_string()), "added to the list");
                // set_active=false on the TUI add: dev stays active.
                assert_eq!(active.as_deref(), Some("dev"));
            }
            other => panic!("refreshed switcher, got {other:?}"),
        }
        // And on disk.
        let config = ignition_core::config::load(&ignition_core::config::config_path())
            .expect("config reloads");
        assert!(config.profiles.contains_key("stage"));

        teardown_profiles(&vars);
    }

    // ---- Logs screen (06-03 Task 1) ----

    fn log_entry(
        timestamp: i64,
        level: &str,
        message: &str,
    ) -> ignition_core::client::logs::LogEntry {
        ignition_core::client::logs::LogEntry {
            timestamp,
            logger_name: "GatewayManager".into(),
            level: level.into(),
            message: message.into(),
            stack: Vec::new(),
            mdc: Default::default(),
            extra: Default::default(),
        }
    }

    /// Entering the Logs screen arms the tail rail; leaving stops it
    /// (must-have truth #3 — the state-machine half; nothing spawns
    /// outside a runtime).
    #[test]
    fn logs_screen_entry_and_exit_arm_and_stop_the_tail_rail() {
        let mut state = AppState::new();
        state.client = Some(crate::state::ClientHandle(std::sync::Arc::new(
            ignition_core::client::ReqwestGatewayApi::for_tests("http://127.0.0.1:1/", None),
        )));
        state.events_tx = Some(tokio::sync::mpsc::unbounded_channel().0);

        // Dashboard → Logs (one Tab): the rail arms.
        update(&mut state, key(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(state.screen, Screen::Logs);
        assert!(
            state.logs.tail_shutdown.is_some(),
            "entering Logs arms the tail shutdown rail"
        );

        // Logs → Tags: the rail signals + clears.
        update(&mut state, key(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(state.screen, Screen::Tags);
        assert!(
            state.logs.tail_shutdown.is_none(),
            "leaving Logs stops the tail worker"
        );
    }

    /// The follow/scroll state machine: scrolling up disables follow,
    /// `g`/End re-enables (offset 0), `f` toggles with a snap to
    /// bottom on re-enable, scrolling back down to the bottom
    /// re-enables follow.
    #[test]
    fn logs_follow_scroll_state_machine() {
        let mut state = AppState::new();
        state.screen = Screen::Logs;
        for i in 0..30 {
            state.logs.push_line(log_entry(i, "INFO", "fill"));
        }
        assert!(state.logs.follow, "follow starts on");

        // PageUp: offset 10, follow off.
        update(&mut state, key(KeyCode::PageUp, KeyModifiers::NONE));
        assert_eq!(state.logs.scroll_offset, 10);
        assert!(!state.logs.follow, "scrolling up disables follow");

        // k: one more line up.
        update(&mut state, key(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(state.logs.scroll_offset, 11);

        // j: back down one — still off (not at the bottom yet).
        update(&mut state, key(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(state.logs.scroll_offset, 10);
        assert!(!state.logs.follow);

        // `g`: jump to end, follow re-enabled.
        update(&mut state, key(KeyCode::Char('g'), KeyModifiers::NONE));
        assert_eq!(state.logs.scroll_offset, 0);
        assert!(state.logs.follow);

        // `f`: toggles follow OFF (stays where it is at the bottom).
        update(&mut state, key(KeyCode::Char('f'), KeyModifiers::NONE));
        assert!(!state.logs.follow);
        // `f` again: back ON (snaps to bottom — already there).
        update(&mut state, key(KeyCode::Char('f'), KeyModifiers::NONE));
        assert!(state.logs.follow);

        // Down-scrolling to the bottom re-enables follow.
        update(&mut state, key(KeyCode::PageUp, KeyModifiers::NONE));
        assert!(!state.logs.follow);
        update(&mut state, key(KeyCode::PageDown, KeyModifiers::NONE));
        assert_eq!(state.logs.scroll_offset, 0);
        assert!(state.logs.follow, "reaching the bottom re-enables follow");

        // Scroll is clamped: 30 entries → max offset 29, further
        // PageUps clamp (never past the first line).
        for _ in 0..10 {
            update(&mut state, key(KeyCode::PageUp, KeyModifiers::NONE));
        }
        assert_eq!(state.logs.scroll_offset, 29, "clamped at first line");
    }

    /// `l` cycles the level filter through the full ring (All → … →
    /// Error → All).
    #[test]
    fn logs_l_cycles_the_level_filter() {
        let mut state = AppState::new();
        state.screen = Screen::Logs;
        let cycle = [
            crate::state::LogLevelFilter::Trace,
            crate::state::LogLevelFilter::Debug,
            crate::state::LogLevelFilter::Info,
            crate::state::LogLevelFilter::Warn,
            crate::state::LogLevelFilter::Error,
            crate::state::LogLevelFilter::All,
        ];
        for expected in cycle {
            update(&mut state, key(KeyCode::Char('l'), KeyModifiers::NONE));
            assert_eq!(state.logs.filter, expected);
        }
    }

    /// LogLine events append to the ring under the cap (update-side
    /// proof; era plays no part per the plan-locked decision).
    #[test]
    fn log_line_events_fill_the_ring() {
        let mut state = AppState::new();
        for i in 0..5 {
            update(
                &mut state,
                AppEvent::LogLine(log_entry(i, "WARN", "streamed")),
            );
        }
        assert_eq!(state.logs.ring.len(), 5);
        assert_eq!(state.logs.dropped, 0);
        assert_eq!(
            state.logs.ring.back().map(|e| e.message.as_str()),
            Some("streamed")
        );
    }

    // ---- Logs actions menu / loggers family (06-03 Task 2) ----

    /// The `LOGGER LEVEL` line parses (case-normalized) and refuses
    /// junk with honest reasons (the clap value_enum refusal's twin).
    #[test]
    fn logger_level_line_parses_and_refuses() {
        let parse = super::parse_logger_level_line;
        assert_eq!(
            parse("GatewayManager warn").unwrap(),
            ("GatewayManager".to_string(), "WARN".to_string())
        );
        assert_eq!(
            parse("  Thread$1  ERROR  ").unwrap(),
            ("Thread$1".to_string(), "ERROR".to_string())
        );
        assert!(parse("GatewayManager").is_err(), "missing level");
        assert!(parse("a b c").is_err(), "extra token");
        assert!(parse("").is_err(), "empty");
        let bad = parse("GatewayManager LOUD").expect_err("unknown level");
        assert!(bad.contains("unknown level"), "names the problem: {bad}");
        assert!(bad.contains("TRACE"), "names the valid set: {bad}");
    }

    /// A Logs-screen state with rails armed (the menu-flow fixture —
    /// nothing actually spawns outside a runtime). Enters via Tab so
    /// the screen-transition hooks arm the tail rail like production.
    fn logs_screen_with_rails() -> AppState {
        let mut state = AppState::new();
        state.client = Some(crate::state::ClientHandle(std::sync::Arc::new(
            ignition_core::client::ReqwestGatewayApi::for_tests("http://127.0.0.1:1/", None),
        )));
        state.events_tx = Some(tokio::sync::mpsc::unbounded_channel().0);
        update(&mut state, key(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(state.screen, Screen::Logs);
        state
    }

    /// `a` opens the Logs menu; Enter on `loggers reset` opens the
    /// Confirm gate and spawns NOTHING; Esc cancels with the pending
    /// cleared (the --yes contract, cancel-spawns-nothing).
    #[test]
    fn logs_menu_reset_is_confirm_gated_and_cancel_spawns_nothing() {
        let mut state = logs_screen_with_rails();

        update(&mut state, key(KeyCode::Char('a'), KeyModifiers::NONE));
        assert_eq!(
            state.modal,
            Some(Modal::LogsActions { selected: 0 }),
            "a opens the loggers menu"
        );

        // Down ×2 → `loggers reset` (third entry).
        for _ in 0..2 {
            update(&mut state, key(KeyCode::Down, KeyModifiers::NONE));
        }
        update(&mut state, key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(
            matches!(state.modal, Some(Modal::Confirm { .. })),
            "reset opens the Confirm gate"
        );
        assert_eq!(
            state.dashboard.pending,
            Some(PendingAction::LoggersReset),
            "pending armed"
        );
        assert!(
            state.dashboard.in_flight.is_none(),
            "nothing spawns before y"
        );

        // Esc cancels: pending cleared, still nothing in flight.
        update(&mut state, key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(state.modal.is_none());
        assert!(state.dashboard.pending.is_none());
        assert!(state.dashboard.in_flight.is_none());
    }

    /// Confirm-accept on reset ≡ `--yes`: the mutation moves to
    /// in-flight.
    #[test]
    fn logs_menu_reset_accept_fires_the_unguarded_action() {
        let mut state = logs_screen_with_rails();
        update(&mut state, key(KeyCode::Char('a'), KeyModifiers::NONE));
        for _ in 0..2 {
            update(&mut state, key(KeyCode::Down, KeyModifiers::NONE));
        }
        update(&mut state, key(KeyCode::Enter, KeyModifiers::NONE));
        update(&mut state, key(KeyCode::Char('y'), KeyModifiers::NONE));

        assert!(state.modal.is_none(), "confirm closed on accept");
        assert_eq!(
            state.dashboard.in_flight,
            Some("loggers reset"),
            "reset in flight after y"
        );
    }

    /// `loggers set`: Input prompt → parse → Confirm gate → y fires.
    /// A bad line opens the error modal and arms NOTHING.
    #[test]
    fn logs_menu_set_prompts_parses_then_confirms() {
        let mut state = logs_screen_with_rails();

        // Down ×1 → `loggers set`.
        update(&mut state, key(KeyCode::Char('a'), KeyModifiers::NONE));
        update(&mut state, key(KeyCode::Down, KeyModifiers::NONE));
        update(&mut state, key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(state.modal, Some(Modal::Input { .. })));
        assert_eq!(
            state.dashboard.pending_input,
            Some(PendingInput::LoggersSetLine)
        );

        // Type the line (case-mixed — normalized), Enter → Confirm.
        for ch in "GatewayManager warn".chars() {
            update(&mut state, key(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        update(&mut state, key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(state.modal, Some(Modal::Confirm { .. })));
        assert_eq!(
            state.dashboard.pending,
            Some(PendingAction::LoggersSet {
                logger: "GatewayManager".into(),
                level: "WARN".into()
            })
        );

        // y ≡ --yes: the unguarded action fn moves to in-flight.
        update(&mut state, key(KeyCode::Char('y'), KeyModifiers::NONE));
        assert_eq!(state.dashboard.in_flight, Some("loggers set"));

        // The bad-line twin: junk level → error modal, nothing armed.
        let mut fresh = logs_screen_with_rails();
        update(&mut fresh, key(KeyCode::Char('a'), KeyModifiers::NONE));
        update(&mut fresh, key(KeyCode::Down, KeyModifiers::NONE));
        update(&mut fresh, key(KeyCode::Enter, KeyModifiers::NONE));
        for ch in "GatewayManager LOUD".chars() {
            update(&mut fresh, key(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        update(&mut fresh, key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(
            matches!(&fresh.modal, Some(Modal::Result_ { title, .. }) if title == "loggers set"),
            "bad line surfaces the error modal"
        );
        assert!(fresh.dashboard.pending.is_none(), "nothing armed");
        assert!(fresh.dashboard.in_flight.is_none());
    }

    /// `loggers list`: the optional search — any value (including
    /// empty) spawns the one-shot list; the tail rail stays armed
    /// throughout (the streams are independent workers).
    #[test]
    fn logs_menu_list_spawns_with_or_without_search() {
        let mut state = logs_screen_with_rails();
        update(&mut state, key(KeyCode::Char('a'), KeyModifiers::NONE));
        update(&mut state, key(KeyCode::Enter, KeyModifiers::NONE)); // entry 0 = list
        assert!(matches!(state.modal, Some(Modal::Input { .. })));

        for ch in "Thread".chars() {
            update(&mut state, key(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        update(&mut state, key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(state.dashboard.in_flight, Some("loggers list"));
        assert!(
            state.logs.tail_shutdown.is_some(),
            "the tail keeps streaming independently"
        );

        // Empty input = no filter (the bare list), also spawns.
        let mut fresh = logs_screen_with_rails();
        update(&mut fresh, key(KeyCode::Char('a'), KeyModifiers::NONE));
        update(&mut fresh, key(KeyCode::Enter, KeyModifiers::NONE));
        update(&mut fresh, key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(fresh.dashboard.in_flight, Some("loggers list"));
    }
}
