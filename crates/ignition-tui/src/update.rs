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
    TAG_ACTIONS, TagsForm, session_rows,
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
        // The Tags provider list landed (06-04): replace the table
        // (Ok) or degrade to the honest error state (Err). Stale eras
        // drop whole; the selection clamps into the new rows and
        // auto-lands on the first row (Enter-descend needs one).
        AppEvent::TagsProviders { era, result } => {
            if workers::is_current(state.era, era) {
                state.tags.providers_busy = false;
                match result {
                    Ok(rows) => {
                        if rows.is_empty() {
                            state.tags.providers_table.select(None);
                        } else if let Some(index) = state.tags.providers_table.selected() {
                            state
                                .tags
                                .providers_table
                                .select(Some(index.min(rows.len() - 1)));
                        } else {
                            state.tags.providers_table.select(Some(0));
                        }
                        state.tags.providers = Some(rows);
                        state.tags.providers_error = None;
                    }
                    Err(message) => {
                        state.tags.providers = None;
                        state.tags.providers_error = Some(message);
                    }
                }
            }
        }
        // One browse landed (06-04): the path names the stack level it
        // fills — a popped level's late result finds no level and
        // drops whole (the level lookup IS the stale gate on top of
        // the era).
        AppEvent::TagsBrowse { era, path, result } => {
            if workers::is_current(state.era, era)
                && let Some(level) = state.tags.stack.iter_mut().rev().find(|l| l.path == path)
            {
                match result {
                    Ok(entries) => {
                        if entries.is_empty() {
                            state.tags.tree_table.select(None);
                        } else if let Some(index) = state.tags.tree_table.selected() {
                            state
                                .tags
                                .tree_table
                                .select(Some(index.min(entries.len() - 1)));
                        } else {
                            state.tags.tree_table.select(Some(0));
                        }
                        level.entries = Some(entries);
                        level.error = None;
                    }
                    Err(message) => {
                        level.entries = None;
                        level.error = Some(message);
                    }
                }
            }
        }
        // The detail pane's read landed (06-04): applied only when the
        // seq (the open's request-id) still matches — a read for a
        // left/replaced pane drops; the era gates profile switches.
        AppEvent::TagDetailRead { era, seq, result } => {
            if workers::is_current(state.era, era)
                && state.tags.detail.is_some()
                && state.tags.detail_seq == seq
                && let Some(detail) = state.tags.detail.as_mut()
            {
                detail.read = match result {
                    Ok(row) => crate::state::DetailRead::Loaded(row),
                    Err(message) => crate::state::DetailRead::Error(message),
                };
            }
        }
        // One live-watch poll landed (06-04): the whole set's rows
        // replace the table with per-row changed markers (value or
        // quality diffs — a timestamp bump alone is not a change); an
        // error degrades to the honest error state (the alarms
        // convention). Stale eras (profile switch) AND stale gens
        // (superseded worker after a set change) drop whole.
        AppEvent::TagWatch {
            era,
            generation,
            result,
        } => {
            if workers::is_current(state.era, era) && state.tags.watch_gen == generation {
                match result {
                    Ok(rows) => {
                        let previous: std::collections::BTreeMap<
                            &str,
                            &ignition_core::actions::tags::TagReadRow,
                        > = state
                            .tags
                            .watch_rows
                            .iter()
                            .map(|row| (row.path.as_str(), row))
                            .collect();
                        let changed = rows
                            .iter()
                            .filter(|row| {
                                previous.get(row.path.as_str()).is_none_or(|prev| {
                                    prev.value != row.value || prev.quality != row.quality
                                })
                            })
                            .map(|row| row.path.clone())
                            .collect();
                        state.tags.watch_rows = rows;
                        state.tags.watch_changed = changed;
                        state.tags.watch_error = None;
                    }
                    Err(message) => {
                        state.tags.watch_rows.clear();
                        state.tags.watch_changed.clear();
                        state.tags.watch_error = Some(message);
                    }
                }
            }
        }
        // One alarms poll result (06-03): replace the table (Ok) or
        // degrade to the honest error state (Err). Stale eras drop
        // whole (Pitfall 9); the selection clamps into the new rows.
        AppEvent::Alarms { era, result } => {
            if workers::is_current(state.era, era) {
                state.alarms.busy = false;
                state.alarms.last_poll = Some(std::time::Instant::now());
                match result {
                    Ok(rows) => {
                        if rows.is_empty() {
                            state.alarms.table.select(None);
                        } else if state
                            .alarms
                            .table
                            .selected()
                            .is_some_and(|index| index >= rows.len())
                        {
                            state.alarms.table.select(Some(rows.len() - 1));
                        }
                        state.alarms.active = Some(rows);
                        state.alarms.error = None;
                    }
                    Err(message) => {
                        state.alarms.active = None;
                        state.alarms.error = Some(message);
                    }
                }
            }
        }
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
                // The ack-refresh trigger: a landed (or failed) ack
                // means the active table changed — poll once NOW
                // instead of waiting out the ≤5 s interval.
                if label == "alarms ack" {
                    workers::watch::spawn_alarms_once(state);
                }
                // A landed provider mutation refreshes the Tags
                // screen's provider list (the ack-refresh pattern's
                // tags twin).
                if matches!(label, "providers create" | "providers delete") {
                    workers::watch::spawn_providers_once(state);
                }
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
        // `q` (no modal) quits the cockpit.
        KeyCode::Char('q') => state.should_quit = true,
        // Esc ascends the Tags screen's navigation stack FIRST — one
        // level per press, the navigation-honesty contract (detail →
        // tree → … → providers); only at the bottom of the stack does
        // it fall through to the global quit.
        KeyCode::Esc => {
            let ascended = state.screen == Screen::Tags && tags_ascend(state);
            if !ascended {
                state.should_quit = true;
            }
        }
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
/// (must-have truth #3), leaving Alarms stops the poll, and leaving
/// Tags stops the live watch; entering re-arms them (the tail
/// resuming past the ring's newest entry — no duplicate flood; the
/// watch resuming the retained set).
fn set_screen(state: &mut AppState, screen: Screen) {
    if state.screen == screen {
        return;
    }
    match state.screen {
        Screen::Logs => workers::tail::stop_tail(state),
        Screen::Alarms => workers::watch::stop_alarms(state),
        Screen::Tags => workers::watch::stop_tag_watch(state),
        _ => {}
    }
    state.screen = screen;
    match state.screen {
        Screen::Logs => workers::tail::spawn_tail(state),
        Screen::Alarms => workers::watch::spawn_alarms(state),
        // Entering Tags loads the provider list (one-shot, busy
        // guarded — the screen's entry read) and resumes the live
        // watch over the retained set (empty set = a no-op stop).
        Screen::Tags => {
            workers::watch::spawn_providers_once(state);
            workers::watch::spawn_tag_watch(state);
        }
        _ => {}
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
    // The screen-scoped workers stop too, and their data clears: the
    // new world's screens start from the new gateway.
    if let Some(shutdown) = &state.refresh_shutdown {
        let _ = shutdown.send(true);
    }
    workers::tail::stop_tail(state);
    workers::watch::stop_alarms(state);
    workers::watch::stop_tag_watch(state);
    state.client = Some(crate::state::ClientHandle(api));
    state.profile_url = Some(url);
    state.profile = Some(resolved_name.clone());
    state.dashboard = crate::state::DashboardData::default();
    state.logs = crate::state::LogsData::default();
    state.alarms = crate::state::AlarmsData::default();
    state.tags = crate::state::TagsData::default();
    state.close_modal();
    // Re-spawn under a new era (bumps) + post the banner through the
    // rail so the loop redraws on it.
    workers::refresh::spawn_refresh(state);
    match state.screen {
        Screen::Logs => workers::tail::spawn_tail(state),
        Screen::Alarms => workers::watch::spawn_alarms(state),
        Screen::Tags => {
            workers::watch::spawn_providers_once(state);
            workers::watch::spawn_tag_watch(state);
        }
        _ => {}
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
        Screen::Alarms => alarms_keys(state, code),
        Screen::Tags => tags_keys(state, code),
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

/// The Alarms keymap (06-03): Up/Down move the table cursor, `a`
/// opens the ack form on the selected alarm (username REQUIRED — NOT
/// confirm-gated: acknowledging never un-acknowledges, the CLI family
/// is unguarded too), `h` browses the journal history for the last 24
/// hours (one-shot worker + the result modal — the LOCKED one-mechanism
/// display for raw journal rows).
fn alarms_keys(state: &mut AppState, code: KeyCode) {
    match code {
        KeyCode::Up => move_alarm_selection(state, -1),
        KeyCode::Down => move_alarm_selection(state, 1),
        KeyCode::Char('a') => open_ack_modal(state),
        KeyCode::Char('h') => spawn_alarms_history(state),
        _ => {}
    }
}

/// The currently selected alarm row, if any.
fn selected_alarm_row(state: &AppState) -> Option<ignition_core::actions::tags::AlarmRow> {
    let active = state.alarms.active.as_ref()?;
    let index = state.alarms.table.selected()?;
    active.get(index).cloned()
}

/// Move the alarms-table cursor (clamped, like the sessions table).
fn move_alarm_selection(state: &mut AppState, delta: i32) {
    let Some(active) = &state.alarms.active else {
        return;
    };
    if active.is_empty() {
        return;
    }
    let next = match state.alarms.table.selected() {
        None => 0,
        Some(index) if delta < 0 => index.saturating_sub(1),
        Some(index) => (index + 1).min(active.len() - 1),
    };
    state.alarms.table.select(Some(next));
}

/// `a` on a selected alarm: the ack form carrying the id AS SHOWN (the
/// full UUID from the table; the ACTION expands short prefixes itself —
/// 05-08 behavior inherited).
fn open_ack_modal(state: &mut AppState) {
    if let Some(alarm) = selected_alarm_row(state) {
        state.open_modal(Modal::Ack {
            event_id: alarm.event_id,
            username: String::new(),
            note: String::new(),
            field: 0,
        });
    }
}

/// The history browse: journal rows for the last 24 h — one-shot
/// worker over `tags_alarms_history`, result in the scrollable modal.
/// A journal-less default rig refuses with the provisioning hint (the
/// action's own alarm_journal_missing path) — surfaced as data.
fn spawn_alarms_history(state: &mut AppState) {
    if let Some(client) = client_arc(state) {
        let (start_ms, end_ms) = history_window_24h();
        workers::spawn_action(state, "alarms history", async move {
            ignition_core::actions::tags::tags_alarms_history(
                &*client,
                workers::watch::ALARMS_PROJECT,
                start_ms,
                end_ms,
            )
            .await
        });
    }
}

/// The history window: the trailing 24 hours in epoch-ms (the TUI's
/// fixed browse policy — the CLI's --start/--end stay on the command
/// line where they belong).
fn history_window_24h() -> (i64, i64) {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as i64)
        .unwrap_or_default();
    (now_ms.saturating_sub(24 * 60 * 60 * 1000), now_ms)
}

// ---- Tags screen (06-04) ----

/// The Tags keymap (06-04): Up/Down (j/k) move the current level's
/// cursor, Enter descends (provider → its tree; folder → deeper) or
/// opens the tag detail (with the on-demand read), `w` toggles live
/// watch on the selected tag, `a` opens the actions menu (the
/// remaining tags verbs), Esc ascends one level (handled in
/// [`handle_input`] — the global key owns Esc).
fn tags_keys(state: &mut AppState, code: KeyCode) {
    match code {
        KeyCode::Up | KeyCode::Char('k') => move_tags_selection(state, -1),
        KeyCode::Down | KeyCode::Char('j') => move_tags_selection(state, 1),
        KeyCode::Enter => tags_enter(state),
        KeyCode::Char('w') => toggle_watch(state),
        KeyCode::Char('a') => state.open_modal(Modal::TagsActions { selected: 0 }),
        _ => {}
    }
}

/// Move the current level's cursor — the provider table at the root,
/// the tree table inside the stack, nothing while the detail pane is
/// open. Clamped (no wrap — a cursor that wraps lies about adjacency).
fn move_tags_selection(state: &mut AppState, delta: i32) {
    if state.tags.detail.is_some() {
        return;
    }
    let len = if state.tags.stack.is_empty() {
        state.tags.providers.as_ref().map_or(0, Vec::len)
    } else {
        state
            .tags
            .stack
            .last()
            .and_then(|level| level.entries.as_ref())
            .map_or(0, Vec::len)
    };
    if len == 0 {
        return;
    }
    let table = if state.tags.stack.is_empty() {
        &mut state.tags.providers_table
    } else {
        &mut state.tags.tree_table
    };
    let next = match table.selected() {
        None => 0,
        Some(index) if delta < 0 => index.saturating_sub(1),
        Some(index) => (index + 1).min(len - 1),
    };
    table.select(Some(next));
}

/// The currently selected provider row, if any.
fn selected_provider_row(state: &AppState) -> Option<ignition_core::actions::tags::TagProviderRow> {
    let rows = state.tags.providers.as_ref()?;
    let index = state.tags.providers_table.selected()?;
    rows.get(index).cloned()
}

/// The selected row of the CURRENT tree level (the stack's top).
fn selected_tree_row(state: &AppState) -> Option<ignition_core::actions::tags::BrowseRow> {
    let level = state.tags.stack.last()?;
    let entries = level.entries.as_ref()?;
    let index = state.tags.tree_table.selected()?;
    entries.get(index).cloned()
}

/// Enter: descend or open the detail. Provider level → the provider's
/// tree root (`[name]`); a folder row (has_children) → its path; a
/// leaf row → the detail pane with the on-demand read. Inside the
/// detail, Enter REFIRES the read (on-demand refresh).
fn tags_enter(state: &mut AppState) {
    if state.tags.detail.is_some() {
        refire_detail_read(state);
        return;
    }
    if state.tags.stack.is_empty() {
        if let Some(provider) = selected_provider_row(state) {
            let path = format!("[{}]", provider.name);
            push_browse_level(state, &path);
        }
        return;
    }
    if let Some(row) = selected_tree_row(state) {
        if row.has_children {
            push_browse_level(state, &row.path);
        } else {
            open_detail(state, &row);
        }
    }
}

/// Push a new (loading) level for `path` and spawn its browse — the
/// descend half of the navigation state machine. The cursor we leave
/// behind rides the parent level (restored on ascend).
fn push_browse_level(state: &mut AppState, path: &str) {
    if let Some(level) = state.tags.stack.last_mut() {
        level.selected = state.tags.tree_table.selected();
    }
    state.tags.stack.push(crate::state::BrowseLevel {
        path: path.to_string(),
        ..Default::default()
    });
    state.tags.tree_table.select(None);
    workers::watch::spawn_browse(state, path);
}

/// Open the detail pane for a leaf row and fire its on-demand read
/// under a fresh seq (the request-id gate).
fn open_detail(state: &mut AppState, row: &ignition_core::actions::tags::BrowseRow) {
    state.tags.detail_seq += 1;
    let seq = state.tags.detail_seq;
    state.tags.detail = Some(crate::state::TagsDetail {
        path: row.path.clone(),
        name: row.name.clone(),
        tag_type: row.tag_type.clone(),
        data_type: row.data_type.clone(),
        read: crate::state::DetailRead::Loading,
    });
    let path = row.path.clone();
    workers::watch::spawn_detail_read(state, seq, &path);
}

/// Refire the open detail pane's read (Enter in the detail): a fresh
/// seq retires the in-flight read and a new Loading state arms.
fn refire_detail_read(state: &mut AppState) {
    let Some(detail) = state.tags.detail.as_mut() else {
        return;
    };
    state.tags.detail_seq += 1;
    let seq = state.tags.detail_seq;
    detail.read = crate::state::DetailRead::Loading;
    let path = detail.path.clone();
    workers::watch::spawn_detail_read(state, seq, &path);
}

/// Esc's Tags-screen half: ascend EXACTLY one level — detail → tree
/// (pop) → … → providers. Returns whether the key was consumed; at
/// the provider level it returns false and the global Esc (quit)
/// takes over (navigation honesty: provider list ← tree ← detail).
fn tags_ascend(state: &mut AppState) -> bool {
    if state.tags.detail.is_some() {
        state.tags.detail = None;
        return true;
    }
    match state.tags.stack.pop() {
        Some(_) => {
            // Restore the parent level's saved cursor (the provider
            // level's cursor lives in its own TableState throughout).
            if let Some(parent) = state.tags.stack.last() {
                state.tags.tree_table.select(parent.selected);
            }
            true
        }
        None => false,
    }
}

/// The watch-toggle's path source: the open detail's tag, else the
/// selected row of the current tree level (the provider list level
/// carries no watchable tag).
fn watchable_path(state: &AppState) -> Option<String> {
    if let Some(detail) = &state.tags.detail {
        return Some(detail.path.clone());
    }
    selected_tree_row(state).map(|row| row.path)
}

/// `w`: toggle the selected tag in the live-watch set, then
/// (re)spawn the watch worker for the new set — a set change is a
/// shutdown + respawn under a bumped `gen` (the local stale gate);
/// emptying the set stops the worker outright.
fn toggle_watch(state: &mut AppState) {
    let Some(path) = watchable_path(state) else {
        return;
    };
    if !state.tags.watched.remove(&path) {
        state.tags.watched.insert(path);
    }
    workers::watch::spawn_tag_watch(state);
}

// ---- Tags actions menu (06-04 Task 3) ----

/// The selected tag path at menu time — the write/config/export/
/// history forms' prefill source. The detail pane's path, else the
/// selected tree row, else the selected provider's bracket path (a
/// whole provider is exportable; per-tag verbs that cannot use it
/// refuse honestly at the action layer).
fn tags_context_path(state: &AppState) -> Option<String> {
    if let Some(detail) = &state.tags.detail {
        return Some(detail.path.clone());
    }
    if let Some(row) = selected_tree_row(state) {
        return Some(row.path);
    }
    selected_provider_row(state).map(|provider| format!("[{}]", provider.name))
}

/// The provider name context for the udt/import prefills: the
/// current tree level's bracket, else the selected provider, else
/// the CLI family's own `default`.
fn tags_context_provider(state: &AppState) -> String {
    if let Some(provider) = state
        .tags
        .stack
        .last()
        .and_then(|level| level.path.strip_prefix('['))
        .and_then(|rest| rest.split(']').next())
        .filter(|name| !name.is_empty())
    {
        return provider.to_string();
    }
    if let Some(row) = selected_provider_row(state) {
        return row.name;
    }
    "default".to_string()
}

/// Open a tags Input form: arm the form slot, then the modal (title,
/// optional hint (line breaks are preserved), optional prefill).
fn open_tags_input(
    state: &mut AppState,
    form: TagsForm,
    title: String,
    hint: Option<String>,
    prefill: String,
) {
    state.tags.pending_form = Some(form);
    state.open_modal(Modal::Input {
        title,
        hint,
        buffer: prefill,
    });
}

/// The write-scalar-is-JSON rule (05-04), the TUI twin of main.rs's
/// `parse_write_scalar`: parse as JSON — a scalar rides typed, an
/// array/object refuses (the invalid_input message), unparseable
/// text rides as a STRING. Pure — unit-pinned.
fn parse_write_value(raw: &str) -> Result<serde_json::Value, String> {
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(value) if !value.is_array() && !value.is_object() => Ok(value),
        Ok(_) => Err(format!(
            "value must be a JSON scalar (number, bool, null, or string) — \
             arrays/objects cannot ride the tag write slot: {raw:?}"
        )),
        Err(_) => Ok(serde_json::Value::String(raw.to_string())),
    }
}

/// The import policy line: `abort`/`overwrite` (case-insensitive);
/// merge is Designer-only (the CLI's own refusal language).
fn parse_import_policy(
    raw: &str,
) -> Result<ignition_core::actions::projects::CollisionPolicy, String> {
    match raw.to_ascii_lowercase().as_str() {
        "abort" => Ok(ignition_core::actions::projects::CollisionPolicy::Abort),
        "overwrite" => Ok(ignition_core::actions::projects::CollisionPolicy::Overwrite),
        _ => Err(
            "collision policy must be `abort` or `overwrite` (merge is Designer-only)".to_string(),
        ),
    }
}

/// Exact CLI synopsis for the active tags form. `?` opens this in the
/// shared result pane rather than trying to reproduce every rich clap
/// form in the cockpit.
fn tags_cli_form(form: &TagsForm) -> String {
    match form {
        TagsForm::WriteValue { path } => {
            format!("ign tags write {path:?} --value <JSON_SCALAR>")
        }
        TagsForm::ProviderCreateName => "ign tags provider create <NAME>".to_string(),
        TagsForm::ProviderDeleteName => "ign tags provider delete <NAME> --yes".to_string(),
        TagsForm::ConfigGetPath => "ign tags config get <PATH>".to_string(),
        TagsForm::ConfigCreatePath => "ign tags config create <PATH> --file <FILE>".to_string(),
        TagsForm::ConfigCreateFile { path } => {
            format!("ign tags config create {path:?} --file <FILE>")
        }
        TagsForm::ConfigEditPath => "ign tags config edit <PATH> --file <FILE>".to_string(),
        TagsForm::ConfigEditFile { path } => {
            format!("ign tags config edit {path:?} --file <FILE>")
        }
        TagsForm::ConfigDeletePath => "ign tags config delete <PATH> --yes".to_string(),
        TagsForm::ExportFile { path } => {
            format!("ign tags export {path:?} --output <FILE>")
        }
        TagsForm::ImportFile => {
            "ign tags import --file <FILE> --provider <NAME> [--collision-policy abort|overwrite]"
                .to_string()
        }
        TagsForm::ImportProvider { file } => format!(
            "ign tags import --file {file:?} --provider <NAME> [--collision-policy abort|overwrite]"
        ),
        TagsForm::ImportPolicy { file, provider } => format!(
            "ign tags import --file {file:?} --provider {provider:?} --collision-policy <abort|overwrite>"
        ),
        TagsForm::UdtTypesProvider => "ign tags udt types --provider <NAME>".to_string(),
        TagsForm::UdtDefName => "ign tags udt def <NAME> --provider <PROVIDER>".to_string(),
        TagsForm::UdtDefProvider { name } => {
            format!("ign tags udt def {name:?} --provider <PROVIDER>")
        }
        TagsForm::HistoryQueryPath => {
            "ign tags history query <PATH> --start <TIME> --end <TIME>".to_string()
        }
    }
}

/// Execute a Tags-actions-menu entry `index` (Enter in the
/// TagsActions modal). Unguarded reads spawn immediately; inputs
/// prompt; the guarded verbs (provider delete, config delete, import
/// overwrite) arm their Confirm gates at the form-accept step — the
/// TUI's `--yes` mirrors of main.rs's `require_confirmation` set.
fn execute_tags_menu_action(state: &mut AppState, index: usize) {
    match TAG_ACTIONS.get(index).copied() {
        Some("write") => {
            let Some(path) = tags_context_path(state) else {
                open_error_modal(
                    state,
                    "tags write",
                    "no tag selected — browse into a provider first",
                );
                return;
            };
            open_tags_input(
                state,
                TagsForm::WriteValue { path },
                "write value".to_string(),
                Some("JSON scalar; bare text stays string\narrays/objects are invalid".to_string()),
                String::new(),
            );
        }
        Some("providers list") => {
            if let Some(client) = client_arc(state) {
                workers::spawn_action(state, "providers list", async move {
                    ignition_core::actions::tags::tag_provider_list(&*client).await
                });
            }
        }
        Some("providers create") => {
            open_tags_input(
                state,
                TagsForm::ProviderCreateName,
                "new provider name".to_string(),
                Some(
                    "STANDARD profile (DB-backed stays CLI-only)\npress ? for the CLI form"
                        .to_string(),
                ),
                String::new(),
            );
        }
        Some("providers delete") => {
            let prefill = if state.tags.stack.is_empty() {
                selected_provider_row(state)
                    .map(|row| row.name)
                    .unwrap_or_default()
            } else {
                String::new()
            };
            open_tags_input(
                state,
                TagsForm::ProviderDeleteName,
                "provider name to delete".to_string(),
                Some("guarded — a Confirm gate arms next".to_string()),
                prefill,
            );
        }
        Some("config get") => {
            open_tags_input(
                state,
                TagsForm::ConfigGetPath,
                "config get — tag path".to_string(),
                Some("full tag path, e.g. [default]P5/T1".to_string()),
                tags_context_path(state).unwrap_or_default(),
            );
        }
        Some("config create") => {
            open_tags_input(
                state,
                TagsForm::ConfigCreatePath,
                "config create — tag path".to_string(),
                Some(
                    "common fields: path + JSON definition file\npress ? for the CLI form"
                        .to_string(),
                ),
                tags_context_path(state).unwrap_or_default(),
            );
        }
        Some("config edit") => {
            open_tags_input(
                state,
                TagsForm::ConfigEditPath,
                "config edit — tag path".to_string(),
                Some(
                    "common fields: path + JSON definition file\npress ? for the CLI form"
                        .to_string(),
                ),
                tags_context_path(state).unwrap_or_default(),
            );
        }
        Some("config delete") => {
            open_tags_input(
                state,
                TagsForm::ConfigDeletePath,
                "config delete — tag path".to_string(),
                Some("guarded — a Confirm gate arms next".to_string()),
                tags_context_path(state).unwrap_or_default(),
            );
        }
        Some("export") => {
            let Some(path) = tags_context_path(state) else {
                open_error_modal(
                    state,
                    "tags export",
                    "no tag selected — browse into a provider first",
                );
                return;
            };
            let prefill =
                ignition_core::actions::tags::default_export_file_name(std::slice::from_ref(&path));
            open_tags_input(
                state,
                TagsForm::ExportFile { path },
                "export — output file".to_string(),
                Some("`-o -` (stdout) is CLI-only in the TUI".to_string()),
                prefill,
            );
        }
        Some("import") => {
            open_tags_input(
                state,
                TagsForm::ImportFile,
                "import — export file path".to_string(),
                Some("the `tags export` JSON shape\npress ? for the CLI form".to_string()),
                String::new(),
            );
        }
        Some("udt types") => {
            open_tags_input(
                state,
                TagsForm::UdtTypesProvider,
                "udt types — provider".to_string(),
                None,
                tags_context_provider(state),
            );
        }
        Some("udt def") => {
            open_tags_input(
                state,
                TagsForm::UdtDefName,
                "udt def — type name".to_string(),
                Some("the provider prompts next".to_string()),
                String::new(),
            );
        }
        Some("history query") => {
            open_tags_input(
                state,
                TagsForm::HistoryQueryPath,
                "history query — tag path".to_string(),
                Some("trailing 24 h window (the alarms-history policy)".to_string()),
                tags_context_path(state).unwrap_or_default(),
            );
        }
        _ => {}
    }
}

/// Route an accepted tags Input form (Enter): each arm fires its
/// one-shot (through [`workers::spawn_action`] — the LOCKED
/// result-modal display) or chains the next prompt. An EMPTY value
/// cancels (the wait-module precedent). File inputs are read INSIDE
/// the spawned worker (I/O lives in workers; `-`/stdin is refused —
/// the cockpit owns the terminal input).
fn accept_tags_form(state: &mut AppState, value: &str) {
    let Some(form) = state.tags.pending_form.take() else {
        return;
    };
    if value.trim().is_empty() {
        return; // empty accepts cancel — nothing armed, nothing fired
    }
    let value = value.trim().to_string();
    match form {
        TagsForm::WriteValue { path } => match parse_write_value(&value) {
            Ok(parsed) => {
                if let Some(client) = client_arc(state) {
                    workers::spawn_action(state, "tags write", async move {
                        ignition_core::actions::tags::tags_write(
                            &*client,
                            workers::watch::TAGS_PROJECT,
                            &path,
                            parsed,
                        )
                        .await
                    });
                }
            }
            Err(reason) => open_error_modal(state, "tags write", &reason),
        },
        TagsForm::ProviderCreateName => {
            if let Some(client) = client_arc(state) {
                let name = value.clone();
                workers::spawn_action(state, "providers create", async move {
                    ignition_core::actions::tags::tag_provider_create(&*client, &name).await
                });
            }
        }
        TagsForm::ProviderDeleteName => {
            state.dashboard.pending = Some(PendingAction::TagsProviderDelete {
                name: value.clone(),
            });
            state.open_modal(Modal::Confirm {
                title: "providers delete".to_string(),
                body: format!("delete tag provider {value:?}? every tag it holds is destroyed"),
            });
        }
        TagsForm::ConfigGetPath => {
            if let Some(client) = client_arc(state) {
                let path = value.clone();
                workers::spawn_action(state, "config get", async move {
                    ignition_core::actions::tags::tags_config_get(
                        &*client,
                        workers::watch::TAGS_PROJECT,
                        &path,
                    )
                    .await
                });
            }
        }
        TagsForm::ConfigCreatePath => {
            open_tags_input(
                state,
                TagsForm::ConfigCreateFile { path: value },
                "config create — definition file".to_string(),
                Some("JSON file path (no `-` stdin in the TUI)".to_string()),
                String::new(),
            );
        }
        TagsForm::ConfigCreateFile { path } => {
            if let Some(client) = client_arc(state) {
                let file = value.clone();
                workers::spawn_action(state, "config create", async move {
                    let definition = workers::watch::read_json_file(&file).await?;
                    ignition_core::actions::tags::tags_config_create(
                        &*client,
                        workers::watch::TAGS_PROJECT,
                        &path,
                        &definition,
                    )
                    .await
                });
            }
        }
        TagsForm::ConfigEditPath => {
            open_tags_input(
                state,
                TagsForm::ConfigEditFile { path: value },
                "config edit — definition file".to_string(),
                Some("JSON file path (no `-` stdin in the TUI)".to_string()),
                String::new(),
            );
        }
        TagsForm::ConfigEditFile { path } => {
            if let Some(client) = client_arc(state) {
                let file = value.clone();
                workers::spawn_action(state, "config edit", async move {
                    let definition = workers::watch::read_json_file(&file).await?;
                    ignition_core::actions::tags::tags_config_edit(
                        &*client,
                        workers::watch::TAGS_PROJECT,
                        &path,
                        &definition,
                    )
                    .await
                });
            }
        }
        TagsForm::ConfigDeletePath => {
            state.dashboard.pending = Some(PendingAction::TagsConfigDelete {
                path: value.clone(),
            });
            state.open_modal(Modal::Confirm {
                title: "config delete".to_string(),
                body: format!("delete the tag configuration at {value:?}?"),
            });
        }
        TagsForm::ExportFile { path } => {
            if let Some(client) = client_arc(state) {
                let out = std::path::PathBuf::from(&value);
                workers::spawn_action(state, "tags export", async move {
                    ignition_core::actions::tags::tags_export(
                        &*client,
                        workers::watch::TAGS_PROJECT,
                        std::slice::from_ref(&path),
                        Some(&out),
                    )
                    .await
                });
            }
        }
        TagsForm::ImportFile => {
            open_tags_input(
                state,
                TagsForm::ImportProvider { file: value },
                "import — target provider".to_string(),
                Some("must exist (`providers create` first)".to_string()),
                tags_context_provider(state),
            );
        }
        TagsForm::ImportProvider { file } => {
            open_tags_input(
                state,
                TagsForm::ImportPolicy {
                    file,
                    provider: value,
                },
                "import — collision policy".to_string(),
                Some("abort | overwrite (overwrite is Confirm-gated)".to_string()),
                "abort".to_string(),
            );
        }
        TagsForm::ImportPolicy { file, provider } => match parse_import_policy(&value) {
            Ok(ignition_core::actions::projects::CollisionPolicy::Abort) => {
                if let Some(client) = client_arc(state) {
                    workers::spawn_action(state, "tags import", async move {
                        let payload = workers::watch::read_json_file(&file).await?;
                        ignition_core::actions::tags::tags_import(
                            &*client,
                            workers::watch::TAGS_PROJECT,
                            &provider,
                            payload,
                            ignition_core::actions::projects::CollisionPolicy::Abort,
                        )
                        .await
                    });
                }
            }
            Ok(ignition_core::actions::projects::CollisionPolicy::Overwrite) => {
                // The guarded arm: overwrite replaces existing tags —
                // the Confirm modal IS the TUI's `--yes` (05-05).
                state.dashboard.pending =
                    Some(PendingAction::TagsImportOverwrite { file, provider });
                state.open_modal(Modal::Confirm {
                    title: "tags import".to_string(),
                    body: "import with collision-policy OVERWRITE? existing top-level \
                           tags at the target are replaced"
                        .to_string(),
                });
            }
            Err(reason) => open_error_modal(state, "tags import", &reason),
        },
        TagsForm::UdtTypesProvider => {
            if let Some(client) = client_arc(state) {
                let provider = value.clone();
                workers::spawn_action(state, "udt types", async move {
                    ignition_core::actions::tags::tags_udt_types(
                        &*client,
                        workers::watch::TAGS_PROJECT,
                        &provider,
                    )
                    .await
                });
            }
        }
        TagsForm::UdtDefName => {
            open_tags_input(
                state,
                TagsForm::UdtDefProvider { name: value },
                "udt def — provider".to_string(),
                None,
                tags_context_provider(state),
            );
        }
        TagsForm::UdtDefProvider { name } => {
            if let Some(client) = client_arc(state) {
                let provider = value.clone();
                workers::spawn_action(state, "udt def", async move {
                    ignition_core::actions::tags::tags_udt_def(
                        &*client,
                        workers::watch::TAGS_PROJECT,
                        &provider,
                        &name,
                    )
                    .await
                });
            }
        }
        TagsForm::HistoryQueryPath => {
            if let Some(client) = client_arc(state) {
                let path = value.clone();
                let (start_ms, end_ms) = history_window_24h();
                workers::spawn_action(state, "history query", async move {
                    ignition_core::actions::tags::tags_history_query(
                        &*client,
                        workers::watch::TAGS_PROJECT,
                        &[path],
                        start_ms,
                        end_ms,
                        None,
                        None,
                    )
                    .await
                });
            }
        }
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
                hint: None,
                buffer: String::new(),
            });
        }
        Some("loggers set") => {
            state.dashboard.pending_input = Some(PendingInput::LoggersSetLine);
            state.open_modal(Modal::Input {
                title: "LOGGER LEVEL".to_string(),
                hint: None,
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
                hint: None,
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
        // The confirmed tags mutations fire unguarded — the TUI owned
        // the `--yes` (the CLI guard contract, caller-owns-guard).
        PendingAction::TagsProviderDelete { name } => {
            if let Some(client) = client_arc(state) {
                let name = name.clone();
                workers::spawn_action(state, "providers delete", async move {
                    ignition_core::actions::tags::tag_provider_delete(&*client, &name).await
                });
            }
        }
        PendingAction::TagsConfigDelete { path } => {
            if let Some(client) = client_arc(state) {
                let path = path.clone();
                workers::spawn_action(state, "config delete", async move {
                    ignition_core::actions::tags::tags_config_delete(
                        &*client,
                        workers::watch::TAGS_PROJECT,
                        &[path],
                    )
                    .await
                });
            }
        }
        PendingAction::TagsImportOverwrite { file, provider } => {
            if let Some(client) = client_arc(state) {
                let file = file.clone();
                let provider = provider.clone();
                workers::spawn_action(state, "tags import", async move {
                    let payload = workers::watch::read_json_file(&file).await?;
                    ignition_core::actions::tags::tags_import(
                        &*client,
                        workers::watch::TAGS_PROJECT,
                        &provider,
                        payload,
                        ignition_core::actions::projects::CollisionPolicy::Overwrite,
                    )
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

    // The ack form (06-03): Tab toggles username/note; Enter submits —
    // a NO-OP while the username is empty (OK disabled until non-empty,
    // the must-have). Not confirm-gated (the CLI verb isn't either).
    if let Some(Modal::Ack {
        event_id,
        username,
        note,
        field,
    }) = state.modal.as_mut()
    {
        if code == KeyCode::Tab {
            *field = (*field + 1) % 2;
            return;
        }
        if code == KeyCode::Enter {
            if username.trim().is_empty() {
                return; // required-username gate — the form cannot OK
            }
            let event_id = event_id.clone();
            let username = username.clone();
            let note = note.clone();
            state.close_modal();
            if let Some(client) = client_arc(state) {
                workers::spawn_action(state, "alarms ack", async move {
                    ignition_core::actions::tags::tags_alarms_ack(
                        &*client,
                        workers::watch::ALARMS_PROJECT,
                        &[event_id],
                        &note,
                        &username,
                    )
                    .await
                });
            }
            return;
        }
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

    // The Tags actions menu (06-04): the same nav shape, over the
    // remaining tags family verbs.
    if let Some(Modal::TagsActions { selected }) = state.modal.as_mut()
        && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        match code {
            KeyCode::Up => {
                *selected = selected.saturating_sub(1);
                return;
            }
            KeyCode::Down => {
                *selected = (*selected + 1).min(TAG_ACTIONS.len() - 1);
                return;
            }
            KeyCode::Enter => {
                let index = *selected;
                state.close_modal();
                clear_pending(state);
                execute_tags_menu_action(state, index);
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
        // The Tags screen's forms route first — they carry their own
        // pending slot on TagsData (06-04's small-form router).
        if state.tags.pending_form.is_some() {
            accept_tags_form(state, &value);
            return;
        }
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

    // Rich tags forms expose their exact CLI equivalent through `?`.
    // The result pane replaces the form; Esc returns to the screen and
    // clears the pending payload, so no stale form can later fire.
    if matches!(state.modal, Some(Modal::Input { .. }))
        && code == KeyCode::Char('?')
        && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        && let Some(form) = state.tags.pending_form.take()
    {
        state.open_modal(Modal::Result_ {
            title: "CLI form".to_string(),
            lines: vec![tags_cli_form(&form)],
            scroll: 0,
        });
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
            // The ack form's active field (username / note).
            (
                Some(Modal::Ack {
                    username,
                    note,
                    field,
                    ..
                }),
                KeyCode::Char(c),
            ) => {
                match field {
                    0 => username.push(c),
                    _ => note.push(c),
                }
                return;
            }
            (
                Some(Modal::Ack {
                    username,
                    note,
                    field,
                    ..
                }),
                KeyCode::Backspace,
            ) => {
                match field {
                    0 => {
                        username.pop();
                    }
                    _ => {
                        note.pop();
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
    state.tags.pending_form = None;
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
            hint: None,
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
                hint: None,
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

    // ---- Alarms screen (06-03 Task 3) ----

    fn alarm_row(event_id: &str) -> ignition_core::actions::tags::AlarmRow {
        ignition_core::actions::tags::AlarmRow {
            event_id: event_id.to_string(),
            source: "prov:default".into(),
            state: "Active, Unacknowledged".into(),
            priority: "High".into(),
            name: Some("PumpCavitation".into()),
        }
    }

    /// An Alarms-screen state with rails armed, entering via Tab×3
    /// (Dashboard → Logs → Tags → Alarms) so the transition hooks arm
    /// the poll rail like production.
    fn alarms_screen_with_rails() -> AppState {
        let mut state = AppState::new();
        state.client = Some(crate::state::ClientHandle(std::sync::Arc::new(
            ignition_core::client::ReqwestGatewayApi::for_tests("http://127.0.0.1:1/", None),
        )));
        state.events_tx = Some(tokio::sync::mpsc::unbounded_channel().0);
        for _ in 0..3 {
            update(&mut state, key(KeyCode::Tab, KeyModifiers::NONE));
        }
        assert_eq!(state.screen, Screen::Alarms);
        state
    }

    /// Entering Alarms arms the poll rail; leaving stops it.
    #[test]
    fn alarms_screen_entry_and_exit_arm_and_stop_the_poll_rail() {
        let mut state = alarms_screen_with_rails();
        assert!(
            state.alarms.shutdown.is_some(),
            "entering Alarms arms the poll rail"
        );

        update(&mut state, key(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(state.screen, Screen::Projects);
        assert!(
            state.alarms.shutdown.is_none(),
            "leaving Alarms stops the poll worker"
        );
    }

    /// A current-era poll fills the table (and records freshness,
    /// clears the busy guard); a stale-era poll drops whole.
    #[test]
    fn alarms_poll_fills_the_table_and_stale_drops() {
        let mut state = alarms_screen_with_rails();
        state.alarms.busy = true;
        let era = state.era;
        let rows = vec![alarm_row("c0ffee00-1234-5678-9abc-def012345678")];

        update(
            &mut state,
            AppEvent::Alarms {
                era,
                result: Ok(rows),
            },
        );
        assert_eq!(
            state.alarms.active.as_ref().map(|rows| rows.len()),
            Some(1),
            "table filled"
        );
        assert!(state.alarms.error.is_none());
        assert!(state.alarms.last_poll.is_some(), "freshness recorded");
        assert!(!state.alarms.busy, "busy clears");

        // Stale era: dropped whole — old-world alarms never land.
        state.alarms.busy = true;
        let before = state.alarms.active.clone();
        update(
            &mut state,
            AppEvent::Alarms {
                era: era.wrapping_sub(1),
                result: Ok(vec![]),
            },
        );
        assert_eq!(state.alarms.active, before, "stale poll dropped");
        assert!(state.alarms.busy, "stale poll does not clear busy");
    }

    /// A poll error degrades to the honest error state (data dropped).
    #[test]
    fn alarms_poll_error_degrades_the_table() {
        let mut state = alarms_screen_with_rails();
        let era = state.era;
        update(
            &mut state,
            AppEvent::Alarms {
                era,
                result: Ok(vec![alarm_row("c0ffee00-1234-5678-9abc-def012345678")]),
            },
        );
        assert!(state.alarms.active.is_some());

        update(
            &mut state,
            AppEvent::Alarms {
                era,
                result: Err("routes_not_deployed (exit 6)".into()),
            },
        );
        assert!(state.alarms.active.is_none(), "rows dropped on error");
        assert_eq!(
            state.alarms.error.as_deref(),
            Some("routes_not_deployed (exit 6)")
        );
    }

    /// The ack form: `a` opens it carrying the selected row's id AS
    /// SHOWN; Enter with an EMPTY username is a NO-OP (OK disabled);
    /// typed username + Enter moves the ack to in-flight — NOT
    /// confirm-gated (the CLI verb isn't either).
    #[test]
    fn ack_form_requires_username_and_spawns_on_accept() {
        let mut state = alarms_screen_with_rails();
        let era = state.era;
        update(
            &mut state,
            AppEvent::Alarms {
                era,
                result: Ok(vec![alarm_row("c0ffee00-1234-5678-9abc-def012345678")]),
            },
        );
        update(&mut state, key(KeyCode::Down, KeyModifiers::NONE));
        state.alarms.table.select(Some(0));

        // `a` opens the form with the full UUID target.
        update(&mut state, key(KeyCode::Char('a'), KeyModifiers::NONE));
        match &state.modal {
            Some(Modal::Ack { event_id, .. }) => {
                assert_eq!(event_id, "c0ffee00-1234-5678-9abc-def012345678");
            }
            other => panic!("ack form open, got {other:?}"),
        }

        // Enter with empty username: NO-OP (modal stays, nothing runs).
        update(&mut state, key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(
            matches!(state.modal, Some(Modal::Ack { .. })),
            "empty username cannot OK"
        );
        assert!(state.dashboard.in_flight.is_none());

        // Type the username into field 0, tab to note, type, Enter.
        for ch in "operator".chars() {
            update(&mut state, key(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        update(&mut state, key(KeyCode::Tab, KeyModifiers::NONE));
        for ch in "seen".chars() {
            update(&mut state, key(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        update(&mut state, key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(state.modal.is_none(), "form closed on accept");
        assert_eq!(
            state.dashboard.in_flight,
            Some("alarms ack"),
            "ack in flight (unguarded — no Confirm gate)"
        );
    }

    /// The ack-refresh trigger: a landed `alarms ack` ActionDone arms
    /// the one-shot poll (busy) — the active table refreshes NOW, not
    /// at the next 5 s tick. Other labels do not trigger.
    #[test]
    fn ack_landing_triggers_an_immediate_active_poll() {
        let mut state = alarms_screen_with_rails();
        let era = state.era;

        update(
            &mut state,
            AppEvent::ActionDone {
                era,
                label: "alarms ack",
                result: Ok("{\"acknowledged\": 1}".into()),
            },
        );
        assert!(
            matches!(&state.modal, Some(Modal::Result_ { title, .. }) if title == "alarms ack"),
            "result modal shows the ack outcome"
        );
        assert!(state.alarms.busy, "one-shot poll armed by the ack");

        // A different action's completion does not trigger.
        state.alarms.busy = false;
        update(
            &mut state,
            AppEvent::ActionDone {
                era,
                label: "version",
                result: Ok("{}".into()),
            },
        );
        assert!(!state.alarms.busy, "non-ack labels do not trigger");
    }

    /// `h` browses history: the one-shot verb moves to in-flight (the
    /// 24 h window is computed at spawn; the journal-less refusal will
    /// surface as the action's own data).
    #[test]
    fn h_spawns_the_history_browse() {
        let mut state = alarms_screen_with_rails();
        update(&mut state, key(KeyCode::Char('h'), KeyModifiers::NONE));
        assert_eq!(state.dashboard.in_flight, Some("alarms history"));
    }

    // ---- Tags screen (06-04 Task 1) ----

    fn provider_row(name: &str) -> ignition_core::actions::tags::TagProviderRow {
        ignition_core::actions::tags::TagProviderRow {
            name: name.to_string(),
            enabled: true,
            tag_count: Some(12),
            health: Some("OK".into()),
            managed: false,
        }
    }

    fn browse_row(
        path: &str,
        name: &str,
        tag_type: &str,
        has_children: bool,
    ) -> ignition_core::actions::tags::BrowseRow {
        ignition_core::actions::tags::BrowseRow {
            path: path.to_string(),
            name: name.to_string(),
            tag_type: tag_type.to_string(),
            has_children,
            data_type: Some("Int4".into()),
        }
    }

    fn read_row(path: &str, value: serde_json::Value) -> ignition_core::actions::tags::TagReadRow {
        ignition_core::actions::tags::TagReadRow {
            path: path.to_string(),
            value,
            quality: "Good".into(),
            timestamp: "Mon Aug 24 00:00:00 UTC 2026".into(),
        }
    }

    /// A Tags-screen state with rails armed, entering via Tab×2
    /// (Dashboard → Logs → Tags) so the transition hooks fire the
    /// provider load like production.
    fn tags_screen_with_rails() -> AppState {
        let mut state = AppState::new();
        state.client = Some(crate::state::ClientHandle(std::sync::Arc::new(
            ignition_core::client::ReqwestGatewayApi::for_tests("http://127.0.0.1:1/", None),
        )));
        state.events_tx = Some(tokio::sync::mpsc::unbounded_channel().0);
        for _ in 0..2 {
            update(&mut state, key(KeyCode::Tab, KeyModifiers::NONE));
        }
        assert_eq!(state.screen, Screen::Tags);
        state
    }

    /// Entering Tags fires the provider load (busy guard armed); a
    /// landed list fills the table with the cursor on the first row;
    /// a stale-era list drops whole.
    #[test]
    fn tags_entry_loads_providers_and_events_fill_or_drop() {
        let mut state = tags_screen_with_rails();
        assert!(
            state.tags.providers_busy,
            "entering Tags fires the provider load"
        );

        let era = state.era;
        update(
            &mut state,
            AppEvent::TagsProviders {
                era,
                result: Ok(vec![provider_row("default"), provider_row("line0")]),
            },
        );
        assert_eq!(
            state.tags.providers.as_ref().map(|rows| rows.len()),
            Some(2),
            "table filled"
        );
        assert!(!state.tags.providers_busy, "busy clears");
        assert_eq!(
            state.tags.providers_table.selected(),
            Some(0),
            "cursor lands"
        );

        // Stale era: dropped whole (Pitfall 9).
        state.tags.providers_busy = true;
        update(
            &mut state,
            AppEvent::TagsProviders {
                era: era.wrapping_sub(1),
                result: Ok(vec![]),
            },
        );
        assert_eq!(
            state.tags.providers.as_ref().map(|rows| rows.len()),
            Some(2),
            "stale list dropped"
        );
        assert!(state.tags.providers_busy, "stale does not clear busy");
    }

    /// A provider-load error degrades to the honest error state.
    #[test]
    fn tags_provider_error_degrades() {
        let mut state = tags_screen_with_rails();
        let era = state.era;
        update(
            &mut state,
            AppEvent::TagsProviders {
                era,
                result: Err("gateway unreachable".into()),
            },
        );
        assert!(state.tags.providers.is_none(), "rows dropped");
        assert_eq!(
            state.tags.providers_error.as_deref(),
            Some("gateway unreachable")
        );
    }

    /// THE descend state machine: Enter on a selected provider pushes
    /// a loading `[name]` level; the matching-path browse fills it; a
    /// wrong-path result (a popped level's late answer) drops.
    #[test]
    fn enter_on_provider_descends_and_browse_fills_the_matching_level() {
        let mut state = tags_screen_with_rails();
        let era = state.era;
        update(
            &mut state,
            AppEvent::TagsProviders {
                era,
                result: Ok(vec![provider_row("default")]),
            },
        );

        update(&mut state, key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(state.tags.stack.len(), 1, "one level pushed");
        assert_eq!(state.tags.stack[0].path, "[default]", "provider root");
        assert!(state.tags.stack[0].entries.is_none(), "level is Loading");

        // A late result for a path nobody holds drops whole.
        update(
            &mut state,
            AppEvent::TagsBrowse {
                era,
                path: "[gone]".into(),
                result: Ok(vec![browse_row("[gone]X", "X", "AtomicTag", false)]),
            },
        );
        assert!(state.tags.stack[0].entries.is_none(), "wrong path dropped");

        // The matching result fills the level.
        update(
            &mut state,
            AppEvent::TagsBrowse {
                era,
                path: "[default]".into(),
                result: Ok(vec![
                    browse_row("[default]P5", "P5", "Folder", true),
                    browse_row("[default]T1", "T1", "AtomicTag", false),
                ]),
            },
        );
        assert_eq!(
            state.tags.stack[0].entries.as_ref().map(|rows| rows.len()),
            Some(2)
        );
        assert_eq!(state.tags.tree_table.selected(), Some(0));
    }

    /// A browse error degrades the level to the honest error state
    /// (require_routes denials surface with the action's hint text).
    #[test]
    fn browse_error_degrades_the_level() {
        let mut state = tags_screen_with_rails();
        let era = state.era;
        update(
            &mut state,
            AppEvent::TagsProviders {
                era,
                result: Ok(vec![provider_row("default")]),
            },
        );
        update(&mut state, key(KeyCode::Enter, KeyModifiers::NONE));
        update(
            &mut state,
            AppEvent::TagsBrowse {
                era,
                path: "[default]".into(),
                result: Err("routes_not_deployed (exit 6)".into()),
            },
        );
        assert!(state.tags.stack[0].entries.is_none());
        assert_eq!(
            state.tags.stack[0].error.as_deref(),
            Some("routes_not_deployed (exit 6)")
        );
    }

    /// Enter on a FOLDER descends (pushes its path); Enter on a LEAF
    /// opens the detail with the read Loading under a fresh seq.
    #[test]
    fn folders_descend_and_leaves_open_the_detail() {
        let mut state = tags_screen_with_rails();
        let era = state.era;
        update(
            &mut state,
            AppEvent::TagsProviders {
                era,
                result: Ok(vec![provider_row("default")]),
            },
        );
        update(&mut state, key(KeyCode::Enter, KeyModifiers::NONE));
        update(
            &mut state,
            AppEvent::TagsBrowse {
                era,
                path: "[default]".into(),
                result: Ok(vec![
                    browse_row("[default]P5", "P5", "Folder", true),
                    browse_row("[default]T1", "T1", "AtomicTag", false),
                ]),
            },
        );

        // Enter on the folder (row 0): descend.
        update(&mut state, key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(state.tags.stack.len(), 2, "folder descends");
        assert_eq!(state.tags.stack[1].path, "[default]P5");

        // Esc back, Down to the leaf, Enter: the detail opens.
        update(&mut state, key(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(state.tags.stack.len(), 1, "ascended one level");
        update(&mut state, key(KeyCode::Down, KeyModifiers::NONE));
        let seq = state.tags.detail_seq;
        update(&mut state, key(KeyCode::Enter, KeyModifiers::NONE));
        let detail = state.tags.detail.as_ref().expect("detail open");
        assert_eq!(detail.path, "[default]T1");
        assert!(
            matches!(detail.read, crate::state::DetailRead::Loading),
            "read fired under Loading"
        );
        assert_eq!(state.tags.detail_seq, seq + 1, "seq bumped on open");
    }

    /// The detail read lands only under its matching seq — a read for
    /// a replaced pane drops (the request-id gate).
    #[test]
    fn detail_read_lands_only_under_its_seq() {
        let mut state = tags_screen_with_rails();
        let era = state.era;
        state.tags.detail_seq = 5;
        state.tags.detail = Some(crate::state::TagsDetail {
            path: "[default]T1".into(),
            name: "T1".into(),
            tag_type: "AtomicTag".into(),
            data_type: None,
            read: crate::state::DetailRead::Loading,
        });

        update(
            &mut state,
            AppEvent::TagDetailRead {
                era,
                seq: 4,
                result: Ok(read_row("[default]OLD", serde_json::json!(0))),
            },
        );
        assert!(
            matches!(
                state.tags.detail.as_ref().expect("detail").read,
                crate::state::DetailRead::Loading
            ),
            "stale-seq read dropped"
        );

        update(
            &mut state,
            AppEvent::TagDetailRead {
                era,
                seq: 5,
                result: Ok(read_row("[default]T1", serde_json::json!(42))),
            },
        );
        match &state.tags.detail.as_ref().expect("detail").read {
            crate::state::DetailRead::Loaded(row) => {
                assert_eq!(row.value, serde_json::json!(42));
                assert_eq!(row.quality, "Good");
            }
            other => panic!("read landed, got {other:?}"),
        }
    }

    /// Navigation honesty: Esc ascends EXACTLY one level per press —
    /// detail → tree (deeper → … → root) → providers → quit — and
    /// the saved cursor restores on ascend.
    #[test]
    fn esc_ascends_exactly_one_level_per_press() {
        let mut state = tags_screen_with_rails();
        let era = state.era;
        update(
            &mut state,
            AppEvent::TagsProviders {
                era,
                result: Ok(vec![provider_row("default")]),
            },
        );
        // providers → [default] → [default]P5 → detail
        update(&mut state, key(KeyCode::Enter, KeyModifiers::NONE));
        update(
            &mut state,
            AppEvent::TagsBrowse {
                era,
                path: "[default]".into(),
                result: Ok(vec![browse_row("[default]P5", "P5", "Folder", true)]),
            },
        );
        update(&mut state, key(KeyCode::Enter, KeyModifiers::NONE));
        update(
            &mut state,
            AppEvent::TagsBrowse {
                era,
                path: "[default]P5".into(),
                result: Ok(vec![browse_row("[default]P5/T1", "T1", "AtomicTag", false)]),
            },
        );
        // Move the root-level cursor to row 0 (saved on descend).
        update(&mut state, key(KeyCode::Down, KeyModifiers::NONE));
        update(&mut state, key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(state.tags.detail.is_some());
        assert_eq!(state.tags.stack.len(), 2);

        // Esc 1: detail → the P5 level.
        update(&mut state, key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(state.tags.detail.is_none(), "detail closes, tree stays");
        assert_eq!(state.tags.stack.len(), 2, "exactly one level of ascent");

        // Esc 2: P5 → the provider root; the saved cursor restores.
        update(&mut state, key(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(state.tags.stack.len(), 1);
        assert_eq!(state.tags.tree_table.selected(), Some(0), "cursor restored");

        // Esc 3: root → the provider list.
        update(&mut state, key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(state.tags.stack.is_empty());
        assert!(!state.should_quit, "still on the Tags screen");

        // Esc 4: at the bottom — the global Esc quits.
        update(&mut state, key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(state.should_quit, "Esc at the provider level quits");
    }

    /// Up/Down move the current level's cursor (clamped) and are
    /// no-ops without rows; j/k mirror them.
    #[test]
    fn tags_cursor_moves_and_clamps() {
        let mut state = tags_screen_with_rails();
        let era = state.era;
        update(
            &mut state,
            AppEvent::TagsProviders {
                era,
                result: Ok(vec![provider_row("default"), provider_row("p2")]),
            },
        );

        update(&mut state, key(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(state.tags.providers_table.selected(), Some(1));
        update(&mut state, key(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(state.tags.providers_table.selected(), Some(1), "clamped");
        update(&mut state, key(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(state.tags.providers_table.selected(), Some(0), "k ascends");
    }

    // ---- Tags live watch (06-04 Task 2) ----

    /// A Tags state sitting on a loaded tree level with the cursor on
    /// a leaf row — the watch-flow fixture.
    fn tags_state_on_tree() -> AppState {
        let mut state = tags_screen_with_rails();
        let era = state.era;
        update(
            &mut state,
            AppEvent::TagsProviders {
                era,
                result: Ok(vec![provider_row("default")]),
            },
        );
        update(&mut state, key(KeyCode::Enter, KeyModifiers::NONE));
        update(
            &mut state,
            AppEvent::TagsBrowse {
                era,
                path: "[default]".into(),
                result: Ok(vec![
                    browse_row("[default]P5", "P5", "Folder", true),
                    browse_row("[default]T1", "T1", "AtomicTag", false),
                    browse_row("[default]T2", "T2", "AtomicTag", false),
                ]),
            },
        );
        update(&mut state, key(KeyCode::Down, KeyModifiers::NONE)); // cursor → T1
        state
    }

    /// `w` toggles the selected tag into the watched set and (re)spawns
    /// the worker under a bumped gen; un-watching the last path STOPS
    /// the worker outright (empty set). A set change retires the prior
    /// gen — the local stale gate (the global era stays world-scoped
    /// per 06-03's lock).
    #[test]
    fn w_toggles_watch_and_respawns_under_a_bumped_gen() {
        let mut state = tags_state_on_tree();

        update(&mut state, key(KeyCode::Char('w'), KeyModifiers::NONE));
        assert!(state.tags.watched.contains("[default]T1"));
        assert!(state.tags.watch_shutdown.is_some(), "worker rail armed");
        let first_gen = state.tags.watch_gen;
        assert_eq!(first_gen, 1, "gen bumped on the first spawn");

        // Watch a second tag: the set changes → respawn retires gen 1.
        update(&mut state, key(KeyCode::Down, KeyModifiers::NONE)); // → T2
        update(&mut state, key(KeyCode::Char('w'), KeyModifiers::NONE));
        assert_eq!(state.tags.watched.len(), 2);
        assert_eq!(state.tags.watch_gen, 2, "set change bumps the gen");
        assert!(
            !crate::workers::is_current(state.tags.watch_gen, first_gen),
            "the first worker's gen is stale (is_current check)"
        );

        // Un-watch both: the empty set stops the worker.
        update(&mut state, key(KeyCode::Char('w'), KeyModifiers::NONE)); // T2 out
        assert_eq!(state.tags.watched.len(), 1);
        assert!(state.tags.watch_shutdown.is_some(), "still one path");
        update(&mut state, key(KeyCode::Up, KeyModifiers::NONE)); // → T1
        update(&mut state, key(KeyCode::Char('w'), KeyModifiers::NONE)); // T1 out
        assert!(state.tags.watched.is_empty());
        assert!(
            state.tags.watch_shutdown.is_none(),
            "empty set stops the worker"
        );
    }

    /// `w` in the detail pane toggles the DETAIL's path; `w` at the
    /// provider level (no tree row) is a no-op.
    #[test]
    fn w_in_detail_toggles_the_detail_path_and_provider_level_is_a_noop() {
        let mut state = tags_state_on_tree();
        update(&mut state, key(KeyCode::Enter, KeyModifiers::NONE)); // detail on T1
        update(&mut state, key(KeyCode::Char('w'), KeyModifiers::NONE));
        assert!(state.tags.watched.contains("[default]T1"));

        // Provider level: no watchable path.
        update(&mut state, key(KeyCode::Esc, KeyModifiers::NONE)); // detail → tree
        update(&mut state, key(KeyCode::Esc, KeyModifiers::NONE)); // tree → providers
        let watched_before = state.tags.watched.clone();
        update(&mut state, key(KeyCode::Char('w'), KeyModifiers::NONE));
        assert_eq!(state.tags.watched, watched_before, "no-op at the root");
    }

    /// TagWatch lands only under the current era+gen: rows replace the
    /// table, the changed-marker keys on value/quality diffs (a
    /// timestamp-only bump is NOT a change), and a stale-gen poll
    /// (superseded worker) drops whole.
    #[test]
    fn tag_watch_event_updates_rows_marks_changes_and_drops_stale_gens() {
        let mut state = tags_state_on_tree();
        let era = state.era;
        update(&mut state, key(KeyCode::Char('w'), KeyModifiers::NONE));
        let generation = state.tags.watch_gen;

        // First poll: both rows are NEW → both marked changed.
        update(
            &mut state,
            AppEvent::TagWatch {
                era,
                generation,
                result: Ok(vec![
                    read_row("[default]T1", serde_json::json!(7)),
                    read_row("[default]P5", serde_json::json!(null)),
                ]),
            },
        );
        assert_eq!(state.tags.watch_rows.len(), 2);
        assert!(state.tags.watch_changed.contains("[default]T1"));

        // Second poll: T1's value moved (marked), P5 identical (not).
        update(
            &mut state,
            AppEvent::TagWatch {
                era,
                generation,
                result: Ok(vec![
                    read_row("[default]T1", serde_json::json!(8)),
                    read_row("[default]P5", serde_json::json!(null)),
                ]),
            },
        );
        assert!(
            state.tags.watch_changed.contains("[default]T1"),
            "value moved"
        );
        assert!(
            !state.tags.watch_changed.contains("[default]P5"),
            "identical row is not a change"
        );

        // Stale gen (a superseded worker's in-flight poll): dropped.
        let before = state.tags.watch_rows.clone();
        update(
            &mut state,
            AppEvent::TagWatch {
                era,
                generation: generation.wrapping_sub(1),
                result: Ok(vec![]),
            },
        );
        assert_eq!(state.tags.watch_rows, before, "stale-gen poll dropped");

        // Stale era (profile switch): dropped.
        update(
            &mut state,
            AppEvent::TagWatch {
                era: era.wrapping_sub(1),
                generation,
                result: Ok(vec![]),
            },
        );
        assert_eq!(state.tags.watch_rows, before, "stale-era poll dropped");
    }

    /// A watch poll error degrades to the honest error state (rows
    /// cleared) — the alarms convention.
    #[test]
    fn watch_error_degrades_the_table() {
        let mut state = tags_state_on_tree();
        let era = state.era;
        update(&mut state, key(KeyCode::Char('w'), KeyModifiers::NONE));
        let generation = state.tags.watch_gen;
        update(
            &mut state,
            AppEvent::TagWatch {
                era,
                generation,
                result: Ok(vec![read_row("[default]T1", serde_json::json!(1))]),
            },
        );
        assert!(!state.tags.watch_rows.is_empty());

        update(
            &mut state,
            AppEvent::TagWatch {
                era,
                generation,
                result: Err("routes_not_deployed (exit 6)".into()),
            },
        );
        assert!(state.tags.watch_rows.is_empty(), "rows dropped on error");
        assert_eq!(
            state.tags.watch_error.as_deref(),
            Some("routes_not_deployed (exit 6)")
        );
    }

    /// Leaving the Tags screen stops the watch worker; re-entering
    /// resumes it over the RETAINED set (the tail's re-entry shape).
    #[test]
    fn leaving_tags_stops_the_watch_and_reentry_resumes_it() {
        let mut state = tags_state_on_tree();
        update(&mut state, key(KeyCode::Char('w'), KeyModifiers::NONE));
        assert!(state.tags.watch_shutdown.is_some());

        update(&mut state, key(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(state.screen, Screen::Alarms);
        assert!(
            state.tags.watch_shutdown.is_none(),
            "leaving Tags stops the watch worker"
        );
        assert_eq!(state.tags.watched.len(), 1, "the SET is retained");

        // BackTab returns Alarms → Tags (Tab would land on Projects).
        update(&mut state, key(KeyCode::BackTab, KeyModifiers::SHIFT));
        assert_eq!(state.screen, Screen::Tags);
        assert!(
            state.tags.watch_shutdown.is_some(),
            "re-entry resumes the watch over the retained set"
        );
    }

    // ---- Tags actions menu (06-04 Task 3) ----

    /// Open the Tags menu and run entry `index` (0-based).
    fn run_tags_menu(state: &mut AppState, index: usize) {
        update(state, key(KeyCode::Char('a'), KeyModifiers::NONE));
        assert!(
            matches!(state.modal, Some(Modal::TagsActions { selected: 0 })),
            "a opens the tags menu"
        );
        for _ in 0..index {
            update(state, key(KeyCode::Down, KeyModifiers::NONE));
        }
        update(state, key(KeyCode::Enter, KeyModifiers::NONE));
    }

    /// Type `text` into the open Input modal and accept it.
    fn submit_tags_input(state: &mut AppState, text: &str) {
        for ch in text.chars() {
            update(state, key(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        update(state, key(KeyCode::Enter, KeyModifiers::NONE));
    }

    /// Accept the current prefilled value without editing it.
    fn accept_tags_input(state: &mut AppState) {
        update(state, key(KeyCode::Enter, KeyModifiers::NONE));
    }

    /// Replace an Input modal's prefill, then accept it.
    fn replace_tags_input(state: &mut AppState, text: &str) {
        let len = match &state.modal {
            Some(Modal::Input { buffer, .. }) => buffer.chars().count(),
            other => panic!("expected tags Input, got {other:?}"),
        };
        for _ in 0..len {
            update(state, key(KeyCode::Backspace, KeyModifiers::NONE));
        }
        submit_tags_input(state, text);
    }

    /// The write-scalar rule (the parse unit): scalars ride typed,
    /// bare text becomes a STRING, arrays/objects refuse with the
    /// 05-04 message.
    #[test]
    fn write_value_parse_follows_the_scalar_rule() {
        let parse = super::parse_write_value;
        assert_eq!(parse("42").unwrap(), serde_json::json!(42));
        assert_eq!(parse("1.5").unwrap(), serde_json::json!(1.5));
        assert_eq!(parse("true").unwrap(), serde_json::json!(true));
        assert_eq!(parse("null").unwrap(), serde_json::json!(null));
        assert_eq!(
            parse("hello world").unwrap(),
            serde_json::json!("hello world"),
            "unparseable text rides as a string"
        );
        let array = parse("[1,2]").expect_err("arrays refuse");
        assert!(array.contains("JSON scalar"), "names the rule: {array}");
        let object = parse("{\"a\":1}").expect_err("objects refuse");
        assert!(
            object.contains("arrays/objects"),
            "names the refusal: {object}"
        );
    }

    /// The import policy line parses abort/overwrite
    /// case-insensitively and refuses anything else (merge is
    /// Designer-only).
    #[test]
    fn import_policy_parses_and_refuses() {
        use ignition_core::actions::projects::CollisionPolicy;
        let parse = super::parse_import_policy;
        assert_eq!(parse("abort").unwrap(), CollisionPolicy::Abort);
        assert_eq!(parse("OVERWRITE").unwrap(), CollisionPolicy::Overwrite);
        let merged = parse("merge").expect_err("merge is Designer-only");
        assert!(
            merged.contains("abort` or `overwrite"),
            "names the values: {merged}"
        );
    }

    /// `write`: the Input opens with the JSON-scalar HINT and the
    /// selected path; a scalar accepts into in-flight, an array
    /// refuses with the error modal and fires nothing.
    #[test]
    fn write_form_hints_the_rule_and_enforces_it() {
        let mut state = tags_state_on_tree(); // cursor on [default]T1

        run_tags_menu(&mut state, 0);
        match &state.modal {
            Some(Modal::Input { hint, .. }) => {
                assert!(
                    hint.as_deref().is_some_and(|h| h.contains("JSON scalar")),
                    "the hint states the JSON-scalar rule: {hint:?}"
                );
            }
            other => panic!("write form open, got {other:?}"),
        }

        // An array refuses — error modal, nothing in flight.
        submit_tags_input(&mut state, "[1,2]");
        assert!(
            matches!(&state.modal, Some(Modal::Result_ { title, .. }) if title == "tags write"),
            "array refusal surfaces the error modal"
        );
        assert!(state.dashboard.in_flight.is_none(), "nothing fired");

        // A scalar accepts — the write moves to in-flight.
        let mut fresh = tags_state_on_tree();
        run_tags_menu(&mut fresh, 0);
        submit_tags_input(&mut fresh, "42");
        assert_eq!(fresh.dashboard.in_flight, Some("tags write"));
    }

    /// `providers delete`: name Input → Confirm gate (nothing fires
    /// before `y`); Esc cancels with the pending cleared; `y` fires
    /// the unguarded action (the TUI's `--yes`).
    #[test]
    fn providers_delete_is_confirm_gated() {
        let mut state = tags_state_on_tree();

        run_tags_menu(&mut state, 3); // providers delete
        assert!(matches!(state.modal, Some(Modal::Input { .. })));
        submit_tags_input(&mut state, "scratch");
        assert!(
            matches!(state.modal, Some(Modal::Confirm { .. })),
            "the Confirm gate arms"
        );
        assert_eq!(
            state.dashboard.pending,
            Some(PendingAction::TagsProviderDelete {
                name: "scratch".into()
            })
        );
        assert!(state.dashboard.in_flight.is_none(), "nothing before y");

        // Esc cancels: pending cleared, still nothing in flight.
        update(&mut state, key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(state.dashboard.pending.is_none());
        assert!(state.dashboard.in_flight.is_none());

        // The accept twin: y fires.
        let mut fresh = tags_state_on_tree();
        run_tags_menu(&mut fresh, 3);
        submit_tags_input(&mut fresh, "scratch");
        update(&mut fresh, key(KeyCode::Char('y'), KeyModifiers::NONE));
        assert_eq!(fresh.dashboard.in_flight, Some("providers delete"));
    }

    /// `config delete`: path Input (prefilled with the selected tag)
    /// → Confirm gate → y fires; the same cancel contract.
    #[test]
    fn config_delete_is_confirm_gated_with_the_selection_prefill() {
        let mut state = tags_state_on_tree(); // cursor on [default]T1

        run_tags_menu(&mut state, 7); // config delete
        match &state.modal {
            Some(Modal::Input { buffer, .. }) => {
                assert_eq!(buffer, "[default]T1", "path prefilled from the selection");
            }
            other => panic!("config delete form open, got {other:?}"),
        }
        accept_tags_input(&mut state);
        assert!(matches!(state.modal, Some(Modal::Confirm { .. })));
        assert_eq!(
            state.dashboard.pending,
            Some(PendingAction::TagsConfigDelete {
                path: "[default]T1".into()
            })
        );
        update(&mut state, key(KeyCode::Char('y'), KeyModifiers::NONE));
        assert_eq!(state.dashboard.in_flight, Some("config delete"));
    }

    /// `import`: the three-step chain (file → provider → policy).
    /// ABORT fires directly (its collisions refuse at the action's
    /// own zero-write pre-check); OVERWRITE arms the Confirm gate; a
    /// bad policy line refuses and arms nothing.
    #[test]
    fn import_chain_abort_fires_and_overwrite_is_confirm_gated() {
        // abort: the full chain lands in-flight with no Confirm.
        let mut state = tags_state_on_tree();
        run_tags_menu(&mut state, 9); // import
        submit_tags_input(&mut state, "p5.json"); // file
        replace_tags_input(&mut state, "p5import"); // provider
        accept_tags_input(&mut state); // default policy = abort
        assert!(state.modal.is_none(), "abort fires unguarded");
        assert_eq!(state.dashboard.in_flight, Some("tags import"));

        // overwrite: the same chain arms the Confirm gate instead.
        let mut fresh = tags_state_on_tree();
        run_tags_menu(&mut fresh, 9);
        submit_tags_input(&mut fresh, "p5.json");
        replace_tags_input(&mut fresh, "p5import");
        replace_tags_input(&mut fresh, "overwrite");
        assert!(matches!(fresh.modal, Some(Modal::Confirm { .. })));
        assert_eq!(
            fresh.dashboard.pending,
            Some(PendingAction::TagsImportOverwrite {
                file: "p5.json".into(),
                provider: "p5import".into()
            })
        );
        assert!(fresh.dashboard.in_flight.is_none(), "overwrite waits for y");
        update(&mut fresh, key(KeyCode::Char('y'), KeyModifiers::NONE));
        assert_eq!(fresh.dashboard.in_flight, Some("tags import"));

        // A bad policy line: error modal, nothing armed or fired.
        let mut bad = tags_state_on_tree();
        run_tags_menu(&mut bad, 9);
        submit_tags_input(&mut bad, "p5.json");
        replace_tags_input(&mut bad, "p5import");
        replace_tags_input(&mut bad, "merge");
        assert!(
            matches!(&bad.modal, Some(Modal::Result_ { title, .. }) if title == "tags import"),
            "bad policy surfaces the error modal"
        );
        assert!(bad.dashboard.pending.is_none());
        assert!(bad.dashboard.in_flight.is_none());
    }

    /// `export` prefills the 05-05 default file name for the selected
    /// path (the context path); `udt types` prefills the provider
    /// context; `history query` prefills the path — and each accepts
    /// into in-flight.
    #[test]
    fn export_udt_and_history_forms_prefill_and_fire() {
        let mut state = tags_state_on_tree(); // cursor on [default]T1

        // export: prefill = default_export_file_name([default]T1) = T1.json
        run_tags_menu(&mut state, 8);
        match &state.modal {
            Some(Modal::Input { buffer, .. }) => {
                assert_eq!(buffer, "T1.json", "the 05-05 default naming prefills");
            }
            other => panic!("export form open, got {other:?}"),
        }
        accept_tags_input(&mut state);
        assert_eq!(state.dashboard.in_flight, Some("tags export"));

        // udt types: provider prefill = the tree's provider (default).
        let mut fresh = tags_state_on_tree();
        run_tags_menu(&mut fresh, 10);
        match &fresh.modal {
            Some(Modal::Input { buffer, .. }) => assert_eq!(buffer, "default"),
            other => panic!("udt types form open, got {other:?}"),
        }
        accept_tags_input(&mut fresh);
        assert_eq!(fresh.dashboard.in_flight, Some("udt types"));

        // history query: path prefill, fires the 24 h browse.
        let mut again = tags_state_on_tree();
        run_tags_menu(&mut again, 12);
        match &again.modal {
            Some(Modal::Input { buffer, .. }) => assert_eq!(buffer, "[default]T1"),
            other => panic!("history form open, got {other:?}"),
        }
        accept_tags_input(&mut again);
        assert_eq!(again.dashboard.in_flight, Some("history query"));
    }

    /// The config create/edit chains: path Input → definition-file
    /// Input → in-flight; the file read rides the WORKER (the state
    /// machine only arms the label).
    #[test]
    fn config_create_and_edit_chain_path_then_file() {
        let mut state = tags_state_on_tree();
        run_tags_menu(&mut state, 5); // config create
        replace_tags_input(&mut state, "[default]New");
        assert!(
            matches!(&state.modal, Some(Modal::Input { title, .. }) if title.contains("definition file")),
            "the definition-file prompt chains next"
        );
        submit_tags_input(&mut state, "new.json");
        assert_eq!(state.dashboard.in_flight, Some("config create"));

        let mut fresh = tags_state_on_tree();
        run_tags_menu(&mut fresh, 6); // config edit
        accept_tags_input(&mut fresh);
        submit_tags_input(&mut fresh, "t1.json");
        assert_eq!(fresh.dashboard.in_flight, Some("config edit"));
    }

    /// Rich config forms advertise and open their exact CLI synopsis
    /// through `?`, preserving the locked modal-depth escape hatch.
    #[test]
    fn config_edit_question_mark_opens_the_cli_form() {
        let mut state = tags_state_on_tree();
        run_tags_menu(&mut state, 6); // config edit
        match &state.modal {
            Some(Modal::Input { hint, .. }) => assert!(
                hint.as_deref()
                    .is_some_and(|text| text.contains("press ? for the CLI form")),
                "rich form advertises the CLI escape hatch: {hint:?}"
            ),
            other => panic!("config edit form open, got {other:?}"),
        }

        update(&mut state, key(KeyCode::Char('?'), KeyModifiers::NONE));
        match &state.modal {
            Some(Modal::Result_ { title, lines, .. }) => {
                assert_eq!(title, "CLI form");
                assert!(
                    lines
                        .iter()
                        .any(|line| line.contains("ign tags config edit")),
                    "exact command synopsis is shown: {lines:?}"
                );
            }
            other => panic!("CLI help pane open, got {other:?}"),
        }
        assert!(
            state.tags.pending_form.is_none(),
            "opening help disarms the replaced input form"
        );
    }

    /// A landed provider mutation triggers the provider-list reload
    /// (the ack-refresh pattern's tags twin); other labels do not.
    #[test]
    fn provider_mutations_trigger_a_list_reload() {
        let mut state = tags_state_on_tree();
        let era = state.era;
        assert!(!state.tags.providers_busy);

        update(
            &mut state,
            AppEvent::ActionDone {
                era,
                label: "providers create",
                result: Ok("{\"name\": \"p2\"}".into()),
            },
        );
        assert!(state.tags.providers_busy, "the reload armed");

        state.tags.providers_busy = false;
        update(
            &mut state,
            AppEvent::ActionDone {
                era,
                label: "version",
                result: Ok("{}".into()),
            },
        );
        assert!(!state.tags.providers_busy, "other labels do not trigger");
    }

    /// Esc clears an armed tags form — a canceled form can never arm
    /// a later Enter (the 06-02 cancel-clears-everything contract).
    #[test]
    fn esc_clears_an_armed_tags_form() {
        let mut state = tags_state_on_tree();
        run_tags_menu(&mut state, 0); // write
        assert!(state.tags.pending_form.is_some());
        update(&mut state, key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(state.modal.is_none());
        assert!(state.tags.pending_form.is_none(), "the form slot cleared");
    }
}
