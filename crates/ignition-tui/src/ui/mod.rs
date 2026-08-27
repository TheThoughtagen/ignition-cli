//! Cockpit chrome: tab bar + active screen + modal overlay (Phase 6
//! research, Pattern 4).
//!
//! PURE over [`AppState`] — no I/O, no awaits. The dispatch below is the
//! ONLY thing later screen plans touch here: each screen owns its whole
//! `ui/<screen>.rs` module and this file just routes to it.

pub mod alarms;
pub mod dashboard;
pub mod logs;
pub mod profiles;
pub mod projects;
pub mod rig;
pub mod tags;

use ratatui::Frame;
use ratatui::layout::Constraint::{Length, Min, Ratio};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Tabs};

use crate::state::{AppState, Modal, Screen};

/// Render the whole cockpit: top tab bar (every screen, active
/// highlighted) + the active screen's body + the modal overlay.
pub fn render(state: &AppState, frame: &mut Frame) {
    let [tabs, body] = ratatui::layout::Layout::vertical([Length(1), Min(0)]).areas(frame.area());

    render_tab_bar(state, frame, tabs);

    match state.screen {
        Screen::Dashboard => dashboard::render(state, frame, body),
        Screen::Logs => logs::render(state, frame, body),
        Screen::Tags => tags::render(state, frame, body),
        Screen::Alarms => alarms::render(state, frame, body),
        Screen::Projects => projects::render(state, frame, body),
        Screen::Rig => rig::render(state, frame, body),
    }

    if let Some(modal) = &state.modal {
        render_modal(modal, frame);
    }
}

/// The tab bar: every screen, active tab bolded.
fn render_tab_bar(state: &AppState, frame: &mut Frame, area: ratatui::layout::Rect) {
    let titles: Vec<Line> = Screen::ALL
        .iter()
        .map(|screen| {
            Line::from(if *screen == state.screen {
                Span::styled(
                    screen.title().to_string(),
                    Style::default().add_modifier(Modifier::BOLD),
                )
            } else {
                Span::raw(screen.title().to_string())
            })
        })
        .collect();
    frame.render_widget(Tabs::new(titles), area);
}

/// The modal overlay: centered rect (the 0.30 `Rect::centered` helpers —
/// never hand-rolled rect math) over a `Clear`ed region, so whatever the
/// screen drew underneath disappears.
fn render_modal(modal: &Modal, frame: &mut Frame) {
    // Height grows with the content the infrastructure shapes carry;
    // the Ratio(1,2)-wide centered geometry is the LOCKED look.
    let height = match modal {
        Modal::Confirm { body, .. } => body.lines().count().saturating_add(4).min(9),
        Modal::Input { hint, .. } => 5 + hint.as_deref().map_or(0, |text| text.lines().count()),
        Modal::Result_ { lines, .. } => lines.len().saturating_add(4).min(9),
        Modal::Actions { .. } => crate::state::ACTIONS.len().saturating_add(3).min(11),
        Modal::LogsActions { .. } => crate::state::LOG_ACTIONS.len().saturating_add(3),
        Modal::TagsActions { .. } => crate::state::TAG_ACTIONS.len().saturating_add(3),
        Modal::ProjectsActions { .. } => crate::state::PROJECT_ACTIONS.len().saturating_add(3),
        // The profiles modals compute their own centered geometry in
        // the delegated render — these values keep the match total.
        Modal::Profiles { .. } | Modal::ProfileAdd { .. } => 5,
        // The ack form owns its geometry (screen-owned render).
        Modal::Ack { .. } => 9,
    } as u16;
    let area = frame.area().centered(Ratio(1, 2), Length(height.max(5)));

    frame.render_widget(Clear, area);
    match modal {
        Modal::Confirm { title, body } => {
            let text = vec![
                Line::from(body.clone()),
                Line::default(),
                Line::from("y to confirm · Esc to cancel"),
            ];
            frame.render_widget(
                Paragraph::new(text).block(Block::bordered().title(title.clone())),
                area,
            );
        }
        // The profile switcher modals own their rendering (06-02's
        // screen-owned module) — centered geometry included.
        Modal::Profiles { .. } | Modal::ProfileAdd { .. } => {
            profiles::render_overlay(modal, frame);
        }
        // The ack form owns its rendering too (06-03's screen-owned
        // module — the alarms twin of the profiles pattern).
        Modal::Ack { .. } => {
            alarms::render_ack_overlay(modal, frame);
        }
        Modal::Input {
            title,
            hint,
            buffer,
        } => {
            let mut text = vec![Line::from(format!("{buffer}▏"))];
            if let Some(hint) = hint {
                // Rule reminders may use deliberate line breaks so
                // critical guidance is never clipped by the locked
                // half-width modal geometry.
                text.extend(hint.lines().map(|line| {
                    Line::from(Span::styled(
                        line.to_string(),
                        Style::default().add_modifier(Modifier::DIM),
                    ))
                }));
            }
            text.push(Line::default());
            text.push(Line::from("Enter to accept · Esc to cancel"));
            frame.render_widget(
                Paragraph::new(text).block(Block::bordered().title(title.clone())),
                area,
            );
        }
        Modal::Result_ {
            title,
            lines,
            scroll,
        } => {
            let text: Vec<Line> = lines
                .iter()
                .map(|line| Line::from(line.clone()))
                .chain([
                    Line::default(),
                    Line::from("PgUp/PgDn scroll · Esc to close"),
                ])
                .collect();
            frame.render_widget(
                Paragraph::new(text)
                    .scroll((*scroll, 0))
                    .block(Block::bordered().title(title.clone())),
                area,
            );
        }
        Modal::Actions { selected } => {
            let text: Vec<Line> = crate::state::ACTIONS
                .iter()
                .enumerate()
                .map(|(index, action)| {
                    let marker = if index == *selected { "▸ " } else { "  " };
                    Line::from(format!("{marker}{action}"))
                })
                .chain([Line::default(), Line::from("Enter to run · Esc to cancel")])
                .collect();
            frame.render_widget(
                Paragraph::new(text).block(Block::bordered().title("actions")),
                area,
            );
        }
        // The Logs screen's menu (06-03): the loggers family, same
        // shape as the dashboard's menu.
        Modal::LogsActions { selected } => {
            let text: Vec<Line> = crate::state::LOG_ACTIONS
                .iter()
                .enumerate()
                .map(|(index, action)| {
                    let marker = if index == *selected { "▸ " } else { "  " };
                    Line::from(format!("{marker}{action}"))
                })
                .chain([Line::default(), Line::from("Enter to run · Esc to cancel")])
                .collect();
            frame.render_widget(
                Paragraph::new(text).block(Block::bordered().title("actions")),
                area,
            );
        }
        // The Tags screen's menu (06-04): the remaining tags family
        // verbs, same shape as the dashboard's menu.
        Modal::TagsActions { selected } => {
            let text: Vec<Line> = crate::state::TAG_ACTIONS
                .iter()
                .enumerate()
                .map(|(index, action)| {
                    let marker = if index == *selected { "▸ " } else { "  " };
                    Line::from(format!("{marker}{action}"))
                })
                .chain([Line::default(), Line::from("Enter to run · Esc to cancel")])
                .collect();
            frame.render_widget(
                Paragraph::new(text).block(Block::bordered().title("actions")),
                area,
            );
        }
        // The Projects screen's menu (06-05): the project, resource,
        // and webdev family verbs, same shape as the dashboard's
        // menu.
        Modal::ProjectsActions { selected } => {
            let text: Vec<Line> = crate::state::PROJECT_ACTIONS
                .iter()
                .enumerate()
                .map(|(index, action)| {
                    let marker = if index == *selected { "▸ " } else { "  " };
                    Line::from(format!("{marker}{action}"))
                })
                .chain([Line::default(), Line::from("Enter to run · Esc to cancel")])
                .collect();
            frame.render_widget(
                Paragraph::new(text).block(Block::bordered().title("actions")),
                area,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::render;
    use crate::state::{AppState, Modal, Screen};

    /// Render `state` on an 80x24 TestBackend and return the visible
    /// rows as strings (trimmed of trailing spaces) — the headless
    /// harness pattern (research test pattern; NO insta dep).
    fn rendered_rows(state: &AppState) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("test terminal");
        terminal.draw(|frame| render(state, frame)).expect("draw");
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    /// The chrome renders: tab bar over every screen name + the active
    /// screen's pane — with NO gateway at all (the Dashboard renders
    /// its Loading panels; the still-placeholder screens render the
    /// bordered "not yet wired" block).
    #[test]
    fn chrome_renders_tab_bar_and_placeholder_pane() {
        let state = AppState::new();
        let rows = rendered_rows(&state);

        // Row 0: the tab bar — every screen, default divider, padding.
        assert_eq!(
            rows[0], " Dashboard │ Logs │ Tags │ Alarms │ Projects │ Rig",
            "tab bar lists every screen"
        );

        // The dashboard (no snapshot) shows Loading panels, never blank.
        assert!(
            rows.iter().any(|row| row.contains("Loading")),
            "dashboard panels render Loading before the first refresh"
        );

        // The Tags screen (06-04) renders the provider browser: the
        // bordered pane titled for the provider level, Loading before
        // the first list lands, the status row underneath.
        let mut tags = AppState::new();
        tags.screen = Screen::Tags;
        let rows = rendered_rows(&tags);
        assert!(
            rows[1].starts_with("┌tags — providers"),
            "tags pane title: {}",
            rows[1]
        );
        assert!(
            rows.iter().any(|row| row.contains("Loading")),
            "tags provider list renders Loading before the first load"
        );
        assert!(
            rows[23].contains("providers"),
            "the status row names the level: {}",
            rows[23]
        );
    }

    /// Switching screens moves the surface to that screen's module
    /// (per-screen dispatch proof). The Projects screen (06-05)
    /// renders its real browser now — the bordered list pane with
    /// its Loading state.
    #[test]
    fn screen_dispatch_renders_the_active_screen() {
        let mut state = AppState::new();
        state.screen = Screen::Projects;
        let rows = rendered_rows(&state);
        assert!(
            rows[1].starts_with("┌projects"),
            "Projects list pane rendered: {}",
            rows[1]
        );
        assert!(
            rows.iter().any(|row| row.contains("Loading")),
            "project list renders Loading before the first load"
        );
    }

    /// The active tab is the BOLD one (style-level assertion on the
    /// tab bar's first title span).
    #[test]
    fn active_tab_is_bolded() {
        let mut state = AppState::new();
        state.screen = Screen::Logs;
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("test terminal");
        terminal.draw(|frame| render(&state, frame)).expect("draw");
        let buffer = terminal.backend().buffer();

        // "Logs" starts after " Dashboard │ " = 13 cells.
        let logs_cell = &buffer[(13, 0)];
        assert!(
            logs_cell.modifier.contains(ratatui::style::Modifier::BOLD),
            "active tab (Logs) must be bold"
        );
        // "Dashboard" (inactive now) at x=1 is not.
        let dashboard_cell = &buffer[(1, 0)];
        assert!(
            !dashboard_cell
                .modifier
                .contains(ratatui::style::Modifier::BOLD),
            "inactive tab (Dashboard) must not be bold"
        );
    }

    /// A modal overlays the center of the frame: bordered, titled,
    /// inset from every edge (the Ratio(1,2) geometry), with the body
    /// bracketed by the MODAL's borders — not the pane's.
    #[test]
    fn modal_overlays_centered_and_clears_underneath() {
        let mut state = AppState::new();
        state.open_modal(Modal::Confirm {
            title: "confirm".into(),
            body: "tear down the rig?".into(),
        });
        let rows = rendered_rows(&state);

        // The modal's bordered title row. Centering proof: between the
        // frame edge and the modal's ┌ lies a wide gap (the pane's own
        // frame-edge border may legitimately remain — Clear wipes only
        // the modal's rect).
        let title_row = rows
            .iter()
            .find(|row| row.contains("┌confirm"))
            .expect("modal title row renders");
        let at = title_row.find("┌confirm").expect("just found");
        let prefix = &title_row[..at];
        assert!(
            prefix.chars().all(|c| c == ' ' || c == '│'),
            "only pane border + gap precede the centered modal: {title_row:?}"
        );
        assert!(
            prefix.chars().filter(|c| *c == ' ').count() >= 10,
            "modal is centered with a wide inset, not flush: {title_row:?}"
        );

        // Structure: the modal's own row is exactly one bordered block
        // row — title, dashes, corners (the pane's frame-edge right
        // border may trail it; strip it for the structural compare).
        let raw_row = title_row[at..].trim_end();
        let block_row = raw_row.strip_suffix('│').unwrap_or(raw_row).trim_end();
        let dashes = "─".repeat(
            block_row
                .chars()
                .count()
                .saturating_sub(1 + "confirm".len() + 1),
        );
        assert_eq!(
            block_row,
            format!("┌confirm{dashes}┐"),
            "title row is a clean bordered block: {block_row:?}"
        );

        // The body row is bracketed by the MODAL's border (inset inside
        // the pane), not by the pane's frame-edge border.
        let body_row = rows
            .iter()
            .find(|row| row.contains("tear down the rig?"))
            .expect("modal body renders");
        let interior = body_row.strip_prefix('│').unwrap_or(body_row).trim_start();
        assert!(
            interior.starts_with("│tear down the rig?"),
            "body sits inside the modal block: {body_row:?}"
        );

        // Clear proof: no row mixes a dashboard panel title with modal
        // content — the modal's rect carries only its own glyphs.
        assert!(
            !rows
                .iter()
                .any(|row| row.contains("confirm") && row.contains("status")),
            "modal rect wipes the underlying pane content"
        );
    }

    /// All three modal shapes render their payloads.
    #[test]
    fn all_modal_shapes_render() {
        // Input shows the buffer + cursor glyph.
        let mut state = AppState::new();
        state.open_modal(Modal::Input {
            title: "username".into(),
            hint: None,
            buffer: "adm".into(),
        });
        let rows = rendered_rows(&state);
        assert!(
            rows.iter().any(|row| row.contains("adm▏")),
            "input modal renders buffer + cursor: {rows:?}"
        );

        // Result_ shows its lines.
        let mut state = AppState::new();
        state.open_modal(Modal::Result_ {
            title: "error".into(),
            lines: vec!["gateway unreachable".into()],
            scroll: 0,
        });
        let rows = rendered_rows(&state);
        assert!(
            rows.iter().any(|row| row.contains("gateway unreachable")),
            "result modal renders lines: {rows:?}"
        );
    }

    /// The Actions menu modal renders every global verb with the
    /// selection marker, plus the scroll hint on the result modal.
    #[test]
    fn actions_menu_and_scrolling_result_render() {
        let mut state = AppState::new();
        state.open_modal(Modal::Actions { selected: 1 });
        let rows = rendered_rows(&state);
        let text = rows.join("\n");
        for verb in [
            "version",
            "connections",
            "wait gateway",
            "wait restart",
            "wait module",
            "doctor",
            "restart",
        ] {
            assert!(text.contains(verb), "menu lists {verb}");
        }
        assert!(
            rows.iter().any(|row| row.contains("▸ connections")),
            "selection marker on entry 1"
        );
        assert!(
            rows.iter().any(|row| row.contains("┌actions")),
            "menu is a bordered modal"
        );

        // The result modal advertises PgUp/PgDn.
        let mut state = AppState::new();
        state.open_modal(Modal::Result_ {
            title: "wait gateway".into(),
            lines: vec!["{}".into()],
            scroll: 0,
        });
        let rows = rendered_rows(&state);
        assert!(
            rows.iter().any(|row| row.contains("PgUp/PgDn scroll")),
            "result modal shows the scroll hint: {rows:?}"
        );
    }

    /// The Logs actions menu (06-03) renders the loggers family with
    /// the selection marker.
    #[test]
    fn logs_actions_menu_renders_the_loggers_family() {
        let mut state = AppState::new();
        state.screen = Screen::Logs;
        state.open_modal(Modal::LogsActions { selected: 2 });
        let rows = rendered_rows(&state);
        let text = rows.join("\n");
        for verb in ["loggers list", "loggers set", "loggers reset"] {
            assert!(text.contains(verb), "menu lists {verb}: {text}");
        }
        assert!(
            rows.iter().any(|row| row.contains("▸ loggers reset")),
            "selection marker on entry 2"
        );
    }

    /// The Tags actions menu (06-04) renders every remaining tags
    /// verb with the selection marker.
    #[test]
    fn tags_actions_menu_renders_the_tags_family() {
        let mut state = AppState::new();
        state.screen = Screen::Tags;
        state.open_modal(Modal::TagsActions { selected: 0 });
        let rows = rendered_rows(&state);
        let text = rows.join("\n");
        for verb in [
            "write",
            "providers list",
            "providers create",
            "providers delete",
            "config get",
            "config create",
            "config edit",
            "config delete",
            "export",
            "import",
            "udt types",
            "udt def",
            "history query",
        ] {
            assert!(text.contains(verb), "menu lists {verb}: {text}");
        }
        assert!(
            rows.iter().any(|row| row.contains("▸ write")),
            "selection marker on entry 0"
        );
    }

    /// The Projects actions menu (06-05) renders every project/
    /// resource/webdev verb with the selection marker.
    #[test]
    fn projects_actions_menu_renders_the_families() {
        let mut state = AppState::new();
        state.screen = Screen::Projects;
        state.open_modal(Modal::ProjectsActions { selected: 0 });
        let rows = rendered_rows(&state);
        let text = rows.join("\n");
        for verb in [
            "new",
            "copy",
            "rename",
            "set",
            "delete",
            "import",
            "export",
            "resource put",
            "resource delete",
            "webdev deploy",
            "webdev status",
        ] {
            assert!(text.contains(verb), "menu lists {verb}: {text}");
        }
        assert!(
            rows.iter().any(|row| row.contains("▸ new")),
            "selection marker on entry 0"
        );
    }

    /// The Input modal's hint line renders (06-04's write form states
    /// the JSON-scalar rule — the must-have).
    #[test]
    fn input_modal_renders_its_hint_line() {
        let mut state = AppState::new();
        state.open_modal(Modal::Input {
            title: "write value".into(),
            hint: Some("JSON scalar; bare text stays string\narrays/objects are invalid".into()),
            buffer: String::new(),
        });
        let rows = rendered_rows(&state);
        let text = rows.join("\n");
        assert!(
            text.contains("JSON scalar"),
            "the hint line renders: {text}"
        );
        assert!(
            text.contains("arrays/objects are invalid"),
            "the arrays/objects refusal rides the hint: {text}"
        );
    }
}
