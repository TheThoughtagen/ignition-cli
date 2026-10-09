//! The itop transition layer — the ONLY writer of
//! [`crate::itop::state::TopState`], driven by [`TopEvent`]s (the
//! cockpit's Elm-style `update`, scoped to itop).
//!
//! Modal arbitration: an open modal consumes EVERY keystroke (the
//! cockpit's Focus::Modal rule, folded into one Option). Worker spawns
//! ride the same convention as the cockpit's update — spawn helpers
//! take `&mut TopState`, read the rails, and stand alone outside a
//! tokio runtime (unit tests transition state without spawning).

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::itop::event::TopEvent;
use crate::itop::state::{Family, Modal, SortKey, ThreadSnapshot, TopState};
use crate::itop::worker::{
    spawn_kill, spawn_sample_once, spawn_sample_worker, spawn_script_probe,
    spawn_thread_diagnostics,
};

/// Process one event. Pure over the state (spawns aside).
pub fn update(state: &mut TopState, event: TopEvent) {
    match event {
        TopEvent::Input(event) => match event {
            Event::Key(key) => handle_key(state, key),
            Event::Resize(_, _) => {} // the next draw adopts the size
            _ => {}                   // mouse/focus events carry no itop meaning
        },
        TopEvent::Tick => {} // the staleness readout is computed at render
        TopEvent::Sample { era, sample } => {
            if era != state.era {
                return; // stale world — drop whole (Pitfall 9)
            }
            state.apply_sample(sample, std::time::Instant::now());
            state.clamp_selection();
        }
        // One-shot ops apply REGARDLESS of era: they are not tied to a
        // sample-worker world, and a stale result can never corrupt the
        // sample data. Dropping them on a world change strands their
        // busy guards (review: the `e` probe stays disabled and the
        // kill status shows "terminating …" forever if `+`/`-` lands
        // mid-op). Samples keep the era gate above (Pitfall 9).
        TopEvent::Killed { label, result, .. } => match result {
            Ok(summary) => state.status_msg = Some((summary, false)),
            Err(err) => {
                state.status_msg = Some((format!("kill {label} failed: {err}"), true));
            }
        },
        TopEvent::ScriptProbe { result, .. } => {
            state.probe_busy = false; // always — see the one-shot note above
            match result {
                Ok(done) => {
                    let document = serde_json::to_string_pretty(&*done)
                        .unwrap_or_else(|_| format!("{done:?}"));
                    let elapsed = done.elapsed_ms;
                    state.modal = Some(Modal::ScriptProbe {
                        result: Ok(document.clone()),
                    });
                    state.script_result = Some(Ok(document));
                    state.status_msg = Some((
                        format!("scriptExec probe done ({elapsed} ms route-side)"),
                        false,
                    ));
                }
                Err(err) => {
                    state.modal = Some(Modal::ScriptProbe {
                        result: Err(err.clone()),
                    });
                    state.script_result = Some(Err(err.clone()));
                    state.status_msg = Some((format!("scriptExec probe refused: {err}"), true));
                }
            }
        }
        TopEvent::ThreadDiagnostics {
            at,
            dump,
            deadlocked,
        } => {
            state.threads_busy = false; // always — the one-shot note above
            let (dump, dump_error) = match dump {
                Ok(dump) => (Some(dump), None),
                Err(err) => (None, Some(err)),
            };
            let (deadlocked, deadlocks_error) = match deadlocked {
                Ok(ids) => (Some(ids), None),
                Err(err) => (None, Some(err)),
            };
            state.thread_dump = Some(Box::new(ThreadSnapshot {
                at,
                dump,
                dump_error,
                deadlocked,
                deadlocks_error,
            }));
            state.clamp_selection();
            // The status line summarizes the op — the table itself
            // carries the detail (cpu column + the ⚠ deadlock marks).
            let Some(snap) = state.thread_dump.as_ref() else {
                return;
            };
            let status = match (&snap.dump, snap.deadlocked.as_deref()) {
                (Some(dump), Some([])) => (
                    format!("thread dump: {} threads · no deadlocks", dump.threads.len()),
                    false,
                ),
                (Some(dump), Some(ids)) => (
                    format!(
                        "thread dump: {} threads · ⚠ {} DEADLOCKED",
                        dump.threads.len(),
                        ids.len()
                    ),
                    true,
                ),
                (Some(dump), None) => (
                    format!(
                        "thread dump: {} threads · deadlocks check failed",
                        dump.threads.len()
                    ),
                    true,
                ),
                (None, _) => (
                    format!(
                        "thread dump failed: {}",
                        snap.dump_error.as_deref().unwrap_or("unknown error")
                    ),
                    true,
                ),
            };
            state.status_msg = Some(status);
        }
    }
}

/// One key event, modal-arbitrated.
fn handle_key(state: &mut TopState, key: KeyEvent) {
    if state.modal.is_some() {
        handle_modal_key(state, key);
    } else {
        handle_table_key(state, key);
    }
}

/// Keys while a modal is open — the modal consumes everything.
fn handle_modal_key(state: &mut TopState, key: KeyEvent) {
    let Some(modal) = state.modal.take() else {
        return;
    };
    match modal {
        Modal::Help => {
            // Any key dismisses the overlay.
            state.modal = None;
        }
        Modal::ScriptProbe { .. } => {
            // Any key dismisses the document — the result stays in
            // `script_result` until the next probe replaces it.
            state.modal = None;
        }
        Modal::Filter { mut buffer } => match key.code {
            KeyCode::Enter => {
                state.set_filter(&buffer);
                state.modal = None;
            }
            KeyCode::Esc => {
                // Close keeping the active filter; a second Esc (no
                // filter set) is handled by the table layer's quit-free
                // Esc rule.
                state.modal = None;
            }
            KeyCode::Backspace => {
                buffer.pop();
                state.modal = Some(Modal::Filter { buffer });
            }
            KeyCode::Char(ch) => {
                buffer.push(ch);
                state.modal = Some(Modal::Filter { buffer });
            }
            _ => state.modal = Some(Modal::Filter { buffer }),
        },
        Modal::ConfirmKill { target } => match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                state.modal = None;
                spawn_kill(state, target);
            }
            _ => state.modal = None, // n / Esc / anything — cancel
        },
        Modal::ThreadStack {
            name,
            lines,
            mut scroll,
        } => {
            // Scrollable (a real stack outgrows any modal): navigation
            // keys move the offset; Esc/Enter/q close.
            let page = 10usize;
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    scroll = (scroll + 1).min(lines.len().saturating_sub(1));
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    scroll = scroll.saturating_sub(1);
                }
                KeyCode::PageDown | KeyCode::Char('J') => {
                    scroll = (scroll + page).min(lines.len().saturating_sub(1));
                }
                KeyCode::PageUp | KeyCode::Char('K') => {
                    scroll = scroll.saturating_sub(page);
                }
                KeyCode::Home | KeyCode::Char('g') => scroll = 0,
                KeyCode::End | KeyCode::Char('G') => scroll = lines.len().saturating_sub(1),
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
                    return; // modal stays closed — the take() already cleared it
                }
                _ => {}
            }
            state.modal = Some(Modal::ThreadStack {
                name,
                lines,
                scroll,
            });
        }
    }
}

/// Keys in table mode.
fn handle_table_key(state: &mut TopState, key: KeyEvent) {
    // Ctrl-C always quits; plain keys follow.
    if matches!(key.code, KeyCode::Char('c')) && key.modifiers.contains(KeyModifiers::CONTROL) {
        state.should_quit = true;
        return;
    }
    match key.code {
        KeyCode::Char('q') => state.should_quit = true,
        KeyCode::Char(' ') | KeyCode::Char('p') => {
            if state.toggle_pause()
                && let Some(tx) = &state.paused_tx
            {
                let _ = tx.send(state.paused);
            }
        }
        KeyCode::Char('+') | KeyCode::Char('=') => {
            if state.adjust_interval(1) {
                respawn_sample_worker(state);
            }
        }
        KeyCode::Char('-') | KeyCode::Char('_') => {
            if state.adjust_interval(-1) {
                respawn_sample_worker(state);
            }
        }
        KeyCode::Char('r') => spawn_sample_once(state),
        KeyCode::Char('e') => spawn_script_probe(state),
        KeyCode::Char('s') => state.cycle_sort(),
        KeyCode::Char('S') => state.toggle_sort_dir(),
        KeyCode::Char('/') => {
            state.modal = Some(Modal::Filter {
                buffer: String::new(),
            })
        }
        KeyCode::Char('?') => state.modal = Some(Modal::Help),
        KeyCode::Char(ch @ '1'..='6') => {
            let family = Family::ALL[(ch as u8 - b'1') as usize];
            state.family = family;
            state.selected = 0;
            if family == Family::Threads {
                // Entering the threads view IS the fetch (the web UI's
                // click-to-load); a re-press re-dumps, busy-guarded.
                // First entry defaults the sort to CPU DESC — the hot
                // thread leads, the web column's instinct.
                spawn_thread_diagnostics(state);
                if state.sort_key == SortKey::Kind {
                    state.sort_key = SortKey::Cpu;
                }
            }
        }
        KeyCode::Enter => {
            // The threads view's stack viewer — inert elsewhere (the
            // Esc-without-filter rule: accidental nothing is fine).
            if let Some(modal) = state.armed_thread_stack() {
                state.modal = Some(modal);
            }
        }
        KeyCode::Char('x') | KeyCode::F(9) => arm_kill(state),
        // Vim-style + arrows + pagers — the cockpit's table navigation.
        KeyCode::Char('j') | KeyCode::Down => state.move_selection(1),
        KeyCode::Char('k') | KeyCode::Up => state.move_selection(-1),
        KeyCode::PageDown | KeyCode::Char('J') => state.move_selection(10),
        KeyCode::PageUp | KeyCode::Char('K') => state.move_selection(-10),
        KeyCode::Home | KeyCode::Char('g') => state.selected = 0,
        KeyCode::End | KeyCode::Char('G') => {
            let len = state.visible_rows().len();
            state.selected = len.saturating_sub(1);
        }
        KeyCode::Esc if state.filter.is_some() => {
            // Esc clears an active filter; with none set it is inert
            // (accidental quits are not itop's genre).
            state.filter = None;
            state.selected = 0;
        }
        _ => {}
    }
}

/// Arm the kill confirm for the selected row — session rows only (the
/// gate lives in `RowKind::session_type`; modules/connections/
/// providers get an honest refusal on the status line).
fn arm_kill(state: &mut TopState) {
    match state.armed_kill() {
        Some(target) => state.modal = Some(Modal::ConfirmKill { target }),
        None => {
            if state.visible_rows().is_empty() {
                return; // nothing selected — inert
            }
            state.status_msg = Some(("the selected row has no terminate action".into(), true));
        }
    }
}

/// Signal the old sample worker down, then spawn the new one — the
/// profile-switch teardown ordering (signal BEFORE adopt).
fn respawn_sample_worker(state: &mut TopState) {
    if let Some(shutdown) = &state.sample_shutdown {
        let _ = shutdown.send(true);
    }
    spawn_sample_worker(state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::itop::state::{DEFAULT_INTERVAL_SECS, Family};
    use crate::itop::worker::TopSample;
    use ignition_core::actions::sessions::SessionType;
    use tokio::sync::watch;

    /// A key event helper (no modifiers, no release kind).
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// Arm the full spawn rails (client + sender) on a state — the
    /// respawn/kill tests' shared setup (struct-update, so no bare
    /// Default + assignment sites).
    fn with_rails(state: TopState) -> TopState {
        TopState {
            client: Some(std::sync::Arc::new(
                ignition_core::client::ReqwestGatewayApi::for_tests("http://127.0.0.1:1/", None),
            )),
            events_tx: Some(tokio::sync::mpsc::unbounded_channel().0),
            ..state
        }
    }

    fn sample_event(era: u64) -> TopEvent {
        TopEvent::Sample {
            era,
            sample: Box::new(TopSample::default()),
        }
    }

    /// Stale-era samples drop whole — no data from a dead world ever
    /// lands (Pitfall 9). One-shot ops (kill) apply regardless of era
    /// (review: their results are not sample data; dropping strands
    /// the status line after `+`/`-` respawns the worker).
    #[test]
    fn stale_samples_drop_but_ops_apply() {
        let mut state = TopState {
            era: 2,
            ..TopState::default()
        };
        update(&mut state, sample_event(1));
        assert!(state.last.is_none(), "stale sample dropped");
        update(
            &mut state,
            TopEvent::Killed {
                era: 1,
                label: "perspective admin".into(),
                result: Ok("terminated ps-1 (perspective)".into()),
            },
        );
        assert!(
            state
                .status_msg
                .as_ref()
                .is_some_and(|(msg, _)| msg.contains("terminated")),
            "a one-shot op lands even from a dead era"
        );

        update(&mut state, sample_event(2));
        assert!(state.last.is_some(), "current-era sample lands");
    }

    /// A landing sample clears the one-shot busy guard and clamps the
    /// selection.
    #[test]
    fn sample_landing_clears_busy_and_clamps() {
        let mut state = TopState {
            refresh_busy: true,
            selected: 9,
            ..TopState::default()
        };
        update(&mut state, sample_event(0));
        assert!(!state.refresh_busy);
        assert_eq!(state.selected, 0, "empty list clamps to 0");
    }

    /// Pause toggles and syncs the watch the worker reads.
    #[test]
    fn pause_toggles_and_syncs_the_watch() {
        let (tx, rx) = watch::channel(false);
        let mut state = TopState {
            paused_tx: Some(tx),
            ..TopState::default()
        };

        update(
            &mut state,
            TopEvent::Input(Event::Key(key(KeyCode::Char(' ')))),
        );
        assert!(state.paused);
        assert!(*rx.borrow(), "the worker's pause watch sees true");

        update(
            &mut state,
            TopEvent::Input(Event::Key(key(KeyCode::Char('p')))),
        );
        assert!(!state.paused);
        assert!(!*rx.borrow());
    }

    /// Interval changes respawn the worker: a fresh shutdown rail is
    /// armed and the era bumps, so pre-respawn samples drop. Clamp-
    /// boundary hits do NOT churn the world.
    #[test]
    fn interval_change_respawns_the_worker() {
        // The respawn reads the FULL rails (client + sender + pause
        // watch) — arm them all, exactly as run_loop does.
        let (paused_tx, _rx) = watch::channel(false);
        let mut state = with_rails(TopState {
            paused_tx: Some(paused_tx),
            ..TopState::default()
        });

        update(
            &mut state,
            TopEvent::Input(Event::Key(key(KeyCode::Char('+')))),
        );
        assert_eq!(state.interval_secs, DEFAULT_INTERVAL_SECS + 1);
        assert!(state.sample_shutdown.is_some(), "fresh rail armed");
        assert_eq!(state.era, 1, "respawn bumped the era");

        // A sample stamped with the PRE-respawn era drops; the current
        // era's lands (the respawn's stale gate, behaviorally).
        update(&mut state, sample_event(0));
        assert!(state.last.is_none(), "pre-respawn sample dropped");
        update(&mut state, sample_event(1));
        assert!(state.last.is_some(), "current-era sample lands");

        // Floor: minus at 1s is a no-op — no era churn.
        state.interval_secs = 1;
        let era_before = state.era;
        update(
            &mut state,
            TopEvent::Input(Event::Key(key(KeyCode::Char('-')))),
        );
        assert_eq!(state.interval_secs, 1);
        assert_eq!(state.era, era_before, "clamp hit must not respawn");
    }

    /// The full kill flow: x arms the Confirm modal ONLY for session
    /// rows; y spawns (status line shows the in-flight note — no
    /// runtime here, so the spawn itself stands down); Esc cancels.
    #[test]
    fn kill_flow_arms_confirms_and_cancels() {
        let mut state = with_rails(TopState::default());
        let sample = TopSample {
            modules: Some(ignition_core::actions::inspect::ModulesResult {
                items: vec![ignition_core::client::status::ModuleInfo {
                    id: "mod".into(),
                    name: "A Module".into(),
                    version: "1.0".into(),
                    state: Some("ACTIVE".into()),
                    license_state: None,
                    vendor_name: None,
                    startup_time: None,
                    extra: Default::default(),
                }],
                quarantined: false,
            }),
            ..TopSample::default()
        };
        state.last = Some(Box::new(sample));

        // A module row: x refuses honestly.
        update(
            &mut state,
            TopEvent::Input(Event::Key(key(KeyCode::Char('x')))),
        );
        assert!(state.modal.is_none(), "modules never arm a kill");
        assert!(
            state
                .status_msg
                .as_ref()
                .is_some_and(|(msg, is_err)| *is_err && msg.contains("no terminate")),
            "the refusal is an error-styled status line"
        );

        // A session row: x arms the confirm.
        let sample = TopSample {
            sessions: Some(ignition_core::actions::sessions::SessionsResult {
                designers: vec![],
                perspective: vec![ignition_core::client::sessions::PerspectiveSession {
                    id: "ps-9".into(),
                    username: "admin".into(),
                    authorized: true,
                    project: "whiskeyhouse".into(),
                    client_address: String::new(),
                    last_comm: 0,
                    active_pages: 1,
                    user_agent: String::new(),
                    extra: Default::default(),
                }],
                vision: vec![],
            }),
            ..TopSample::default()
        };
        state.last = Some(Box::new(sample));
        update(
            &mut state,
            TopEvent::Input(Event::Key(key(KeyCode::Char('x')))),
        );
        let Some(Modal::ConfirmKill { target }) = state.modal.clone() else {
            panic!("session row arms the confirm modal");
        };
        assert_eq!(target.id, "ps-9");
        assert_eq!(target.kind, SessionType::Perspective);

        // Esc cancels; then re-arm and y spawns.
        update(&mut state, TopEvent::Input(Event::Key(key(KeyCode::Esc))));
        assert!(state.modal.is_none(), "cancel closes the modal");
        update(
            &mut state,
            TopEvent::Input(Event::Key(key(KeyCode::Char('x')))),
        );
        update(
            &mut state,
            TopEvent::Input(Event::Key(key(KeyCode::Char('y')))),
        );
        assert!(state.modal.is_none(), "confirm closes the modal");
        assert!(
            state
                .status_msg
                .as_ref()
                .is_some_and(|(msg, _)| msg.contains("terminating")),
            "the in-flight note rides the status line"
        );
    }

    /// The filter modal: chars land in the buffer, Enter applies
    /// (lowercased), Esc closes keeping the active filter, table-Esc
    /// clears it.
    #[test]
    fn filter_modal_applies_and_clears() {
        let mut state = TopState::default();
        update(
            &mut state,
            TopEvent::Input(Event::Key(key(KeyCode::Char('/')))),
        );
        assert!(matches!(state.modal, Some(Modal::Filter { .. })));
        for ch in ['P', 'e', 'r', 's'] {
            update(
                &mut state,
                TopEvent::Input(Event::Key(key(KeyCode::Char(ch)))),
            );
        }
        update(
            &mut state,
            TopEvent::Input(Event::Key(key(KeyCode::Backspace))),
        );
        update(&mut state, TopEvent::Input(Event::Key(key(KeyCode::Enter))));
        assert_eq!(
            state.filter.as_deref(),
            Some("per"),
            "lowercased after a backspace, applied"
        );
        assert!(state.modal.is_none());

        // Table Esc clears the filter.
        update(&mut state, TopEvent::Input(Event::Key(key(KeyCode::Esc))));
        assert!(state.filter.is_none());

        // Enter applies again...
        update(
            &mut state,
            TopEvent::Input(Event::Key(key(KeyCode::Char('/')))),
        );
        update(
            &mut state,
            TopEvent::Input(Event::Key(key(KeyCode::Char('x')))),
        );
        update(&mut state, TopEvent::Input(Event::Key(key(KeyCode::Enter))));
        assert_eq!(state.filter.as_deref(), Some("x"));

        // ...and a MODAL Esc (typed buffer unapplied) closes without
        // clearing or changing the ACTIVE filter.
        update(
            &mut state,
            TopEvent::Input(Event::Key(key(KeyCode::Char('/')))),
        );
        update(
            &mut state,
            TopEvent::Input(Event::Key(key(KeyCode::Char('y')))),
        );
        update(&mut state, TopEvent::Input(Event::Key(key(KeyCode::Esc))));
        assert_eq!(
            state.filter.as_deref(),
            Some("x"),
            "the active filter survives a modal Esc"
        );
        assert!(state.modal.is_none());
    }

    /// Family number keys switch the view and reset the selection.
    #[test]
    fn family_keys_switch_the_view() {
        let mut state = TopState::default();
        update(
            &mut state,
            TopEvent::Input(Event::Key(key(KeyCode::Char('3')))),
        );
        assert_eq!(state.family, Family::Sessions);
        assert_eq!(state.selected, 0);
        update(
            &mut state,
            TopEvent::Input(Event::Key(key(KeyCode::Char('1')))),
        );
        assert_eq!(state.family, Family::All);
    }

    /// Quit keys: q quits, Ctrl-C quits, Esc without a filter does
    /// NOT quit.
    #[test]
    fn quit_keys_and_inert_esc() {
        let mut state = TopState::default();
        update(
            &mut state,
            TopEvent::Input(Event::Key(key(KeyCode::Char('q')))),
        );
        assert!(state.should_quit);

        let mut state = TopState::default();
        update(
            &mut state,
            TopEvent::Input(Event::Key(KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL,
            ))),
        );
        assert!(state.should_quit);

        let mut state = TopState::default();
        update(&mut state, TopEvent::Input(Event::Key(key(KeyCode::Esc))));
        assert!(!state.should_quit, "inert Esc never quits");
    }

    /// The help modal opens with ? and any key dismisses.
    #[test]
    fn help_opens_and_dismisses() {
        let mut state = TopState::default();
        update(
            &mut state,
            TopEvent::Input(Event::Key(key(KeyCode::Char('?')))),
        );
        assert!(matches!(state.modal, Some(Modal::Help)));
        update(
            &mut state,
            TopEvent::Input(Event::Key(key(KeyCode::Char('q')))),
        );
        assert!(state.modal.is_none(), "help consumed the q — no quit");
        assert!(!state.should_quit);
    }

    /// The probe's honest INLINE refusal: a profile without a stored
    /// webdev_secret → the ScriptProbe modal opens with the refusal,
    /// zero HTTP, no busy flag (the structural gate runs synchronously
    /// — the `ign script run` zero-requests rule). The isolated config
    /// rides IGNITION_CLI_CONFIG under ENV_LOCK.
    #[test]
    fn probe_refuses_inline_without_a_secret() {
        let _guard = crate::ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "[profiles.p]\nurl = \"http://127.0.0.1:1/\"\nauth = { token_env = \"NOPE\" }\n",
        )
        .expect("write isolated config");
        // SAFETY: single-threaded under ENV_LOCK.
        unsafe { std::env::set_var("IGNITION_CLI_CONFIG", &path) };
        let mut state = with_rails(TopState {
            profile_name: Some("p".into()),
            ..TopState::default()
        });
        update(
            &mut state,
            TopEvent::Input(Event::Key(key(KeyCode::Char('e')))),
        );
        assert!(!state.probe_busy, "a refusal never arms the busy guard");
        match state.modal {
            Some(Modal::ScriptProbe {
                result: Err(refusal),
            }) => {
                assert!(
                    refusal.contains("webdev deploy") || refusal.contains("not configured"),
                    "the refusal names the fix: {refusal}"
                );
            }
            other => panic!("expected the probe refusal modal, got {other:?}"),
        }
        // SAFETY: single-threaded under ENV_LOCK.
        unsafe { std::env::remove_var("IGNITION_CLI_CONFIG") };
    }

    /// A stored webdev_secret arms the busy guard and (outside a
    /// runtime) spawns nothing — the state transition still ran.
    #[test]
    fn probe_with_secret_arms_busy_without_a_runtime() {
        let _guard = crate::ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "[profiles.p]\nurl = \"http://127.0.0.1:1/\"\nauth = { token_env = \"NOPE\" }\nwebdev_secret = \"canary\"\n",
        )
        .expect("write isolated config");
        // SAFETY: single-threaded under ENV_LOCK.
        unsafe { std::env::set_var("IGNITION_CLI_CONFIG", &path) };
        let mut state = with_rails(TopState {
            profile_name: Some("p".into()),
            ..TopState::default()
        });
        update(
            &mut state,
            TopEvent::Input(Event::Key(key(KeyCode::Char('e')))),
        );
        assert!(state.probe_busy, "the probe marks busy");
        update(
            &mut state,
            TopEvent::Input(Event::Key(key(KeyCode::Char('e')))),
        );
        assert!(state.probe_busy, "busy guard refuses stacking");
        // SAFETY: single-threaded under ENV_LOCK.
        unsafe { std::env::remove_var("IGNITION_CLI_CONFIG") };
    }

    /// The probe event lands: busy clears, the modal opens with the
    /// pretty document, the status line names the route-side elapsed.
    /// The probe applies regardless of era (review: dropping a
    /// stale-era probe left `probe_busy` stuck true — every later `e`
    /// refused until itop restarted).
    #[test]
    fn script_probe_event_lands_regardless_of_era() {
        let mut state = with_rails(TopState {
            probe_busy: true,
            era: 3,
            ..TopState::default()
        });
        let done = ignition_core::actions::script::ScriptRunResult {
            stdout: String::new(),
            result: serde_json::json!({"threadsLive": 71}),
            elapsed_ms: 12,
        };
        update(
            &mut state,
            TopEvent::ScriptProbe {
                era: 2,
                result: Ok(Box::new(done)),
            },
        );
        assert!(
            !state.probe_busy,
            "the probe lands from any era — busy always clears"
        );
        assert!(
            state.modal.is_some(),
            "the probe document modal opened from a dead era too"
        );

        let done = ignition_core::actions::script::ScriptRunResult {
            stdout: String::new(),
            result: serde_json::json!({"threadsLive": 71}),
            elapsed_ms: 12,
        };
        update(
            &mut state,
            TopEvent::ScriptProbe {
                era: 3,
                result: Ok(Box::new(done)),
            },
        );
        assert!(!state.probe_busy);
        match state.modal {
            Some(Modal::ScriptProbe {
                result: Ok(document),
            }) => {
                assert!(document.contains("threadsLive"), "the JMX rows render");
                assert!(document.contains("71"));
            }
            other => panic!("expected the probe document modal, got {other:?}"),
        }
        assert!(
            state
                .status_msg
                .as_ref()
                .is_some_and(|(msg, _)| msg.contains("12 ms")),
            "the status line names the route-side elapsed"
        );
    }

    /// A fixture dump — two threads, one at 3.75% cpu holding a
    /// monitor, one blocked waiting on it, id 7 deadlocked.
    fn fixture_dump() -> ignition_core::client::threads::FormattedThreadDump {
        serde_json::from_value(serde_json::json!({
            "version": "dump-version-1",
            "threads": [
                {"name": "Perspective-Worker-3", "id": 42, "state": "RUNNABLE",
                 "daemon": true, "cpuUsage": 3.75,
                 "lockedMonitors": [{"lock": "<0x1a2b> (a Worker)", "frame": "Worker.lock - line 88"}]},
                {"name": "pool-2-thread-1", "id": 7, "state": "BLOCKED",
                 "cpuUsage": 0.0,
                 "waitingFor": {"lock": "<0x1a2b> (a Worker)"}}
            ]
        }))
        .expect("the fixture dump parses")
    }

    fn thread_diagnostics_event(
        dump: ignition_core::client::threads::FormattedThreadDump,
    ) -> TopEvent {
        TopEvent::ThreadDiagnostics {
            at: std::time::Instant::now(),
            dump: Ok(Box::new(dump)),
            deadlocked: Ok(vec![7]),
        }
    }

    /// Key 6 enters the threads view AND arms the dump op (busy-guarded
    /// on re-press); the first entry defaults the sort to cpu desc —
    /// the hot thread leads, the web column's instinct.
    #[test]
    fn six_enters_threads_and_arms_the_dump() {
        let mut state = with_rails(TopState::default());
        update(
            &mut state,
            TopEvent::Input(Event::Key(key(KeyCode::Char('6')))),
        );
        assert_eq!(state.family, Family::Threads);
        assert!(state.threads_busy, "entering the view fetches the dump");
        assert_eq!(state.sort_key, SortKey::Cpu, "cpu desc is the view default");

        update(
            &mut state,
            TopEvent::Input(Event::Key(key(KeyCode::Char('6')))),
        );
        assert!(state.threads_busy, "re-press re-runs, still guarded");

        // Away and back: 1 leaves the view, 6 re-enters + re-dumps.
        update(
            &mut state,
            TopEvent::Input(Event::Key(key(KeyCode::Char('1')))),
        );
        assert_eq!(state.family, Family::All);
        assert_eq!(state.sort_key, SortKey::Cpu, "the user's sort persists");
    }

    /// The thread-diagnostics event lands regardless of era (the
    /// one-shot convention): busy clears, the snapshot stores both
    /// degraded calls, and the status line names the deadlock count.
    #[test]
    fn thread_diagnostics_event_lands_regardless_of_era() {
        let mut state = with_rails(TopState {
            threads_busy: true,
            family: Family::Threads, // the dump landed in the threads view
            era: 3,
            ..TopState::default()
        });
        update(
            &mut state,
            thread_diagnostics_event(fixture_dump()), // lands from any era
        );
        assert!(
            !state.threads_busy,
            "the dump lands from any era — busy always clears"
        );
        let snapshot = state.thread_dump.as_ref().expect("the snapshot stored");
        assert_eq!(
            snapshot.dump.as_ref().expect("dump ok").threads.len(),
            2,
            "both threads stored"
        );
        assert_eq!(
            snapshot.deadlocked.as_deref(),
            Some(&[7][..]),
            "the deadlocked ids stored"
        );
        let (msg, is_err) = state.status_msg.as_ref().expect("status set");
        assert!(msg.contains("2 threads"), "{msg}");
        assert!(msg.contains("⚠ 1 DEADLOCKED"), "{msg}");
        assert!(*is_err, "a deadlock is the alarm case");

        // The rows flatten: the threads view's visible list.
        let rows = state.visible_rows();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "Perspective-Worker-3", "cpu desc — hot first");
        assert!((rows[0].cpu.unwrap() - 3.75).abs() < f64::EPSILON);
        assert!(!rows[0].deadlocked);
        assert_eq!(rows[1].name, "pool-2-thread-1");
        assert!(rows[1].deadlocked, "tid 7 is in the deadlocked list");
        assert!(
            rows[1].detail.contains("waiting <0x1a2b> (a Worker)"),
            "the waiting-for lock rides detail verbatim"
        );
    }

    /// Enter on a thread row opens the scrollable stack modal — header
    /// facts, the hold/wait annotations, frames verbatim; j/k scroll,
    /// Esc closes.
    #[test]
    fn enter_opens_the_thread_stack_modal() {
        let mut state = with_rails(TopState {
            era: 1,
            ..TopState::default()
        });
        update(
            &mut state,
            TopEvent::Input(Event::Key(key(KeyCode::Char('6')))),
        );
        // Simulate the op landing in-era: busy clears, the snapshot
        // stores (selection 0 is the hot thread under the cpu-desc
        // default `6` armed).
        state.threads_busy = false;
        state.thread_dump = Some(Box::new(ThreadSnapshot {
            at: std::time::Instant::now(),
            dump: Some(Box::new(fixture_dump())),
            dump_error: None,
            deadlocked: Some(vec![7]),
            deadlocks_error: None,
        }));

        update(&mut state, TopEvent::Input(Event::Key(key(KeyCode::Enter))));
        match state.modal.as_ref() {
            Some(Modal::ThreadStack {
                name,
                lines,
                scroll,
            }) => {
                assert_eq!(name, "Perspective-Worker-3", "the selected row's thread");
                assert_eq!(*scroll, 0);
                assert!(lines[0].contains("RUNNABLE"), "{}", lines[0]);
                assert!(lines[0].contains("cpu 3.8%"), "{}", lines[0]);
                assert!(
                    lines.iter().any(|line| {
                        line.contains("holds <0x1a2b> (a Worker) — Worker.lock - line 88")
                    }),
                    "the locked monitor renders with its frame"
                );
                assert!(
                    lines
                        .iter()
                        .any(|line| line == "(the dump carries no stack for this thread)"),
                    "the honest absence renders when the dump has no stack"
                );
            }
            other => panic!("expected the thread stack modal, got {other:?}"),
        }

        // j scrolls, Esc closes.
        update(
            &mut state,
            TopEvent::Input(Event::Key(key(KeyCode::Char('j')))),
        );
        match state.modal.as_ref() {
            Some(Modal::ThreadStack { scroll, .. }) => assert_eq!(*scroll, 1, "j scrolled"),
            other => panic!("scroll kept the modal, got {other:?}"),
        }
        update(&mut state, TopEvent::Input(Event::Key(key(KeyCode::Esc))));
        assert!(state.modal.is_none(), "esc closes the stack");
    }
}
