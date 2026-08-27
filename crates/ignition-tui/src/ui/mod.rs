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
        Modal::Input { .. } => 5,
        Modal::Result_ { lines, .. } => lines.len().saturating_add(4).min(9),
        Modal::Actions { .. } => crate::state::ACTIONS.len().saturating_add(3).min(11),
        // The profiles modals compute their own centered geometry in
        // the delegated render — these values keep the match total.
        Modal::Profiles { .. } | Modal::ProfileAdd { .. } => 5,
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
        Modal::Input { title, buffer } => {
            let text = vec![
                Line::from(format!("{buffer}▏")),
                Line::default(),
                Line::from("Enter to accept · Esc to cancel"),
            ];
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

        // A placeholder screen (Logs, until 06-03) still renders the
        // bordered block with its title.
        let mut logs = AppState::new();
        logs.screen = Screen::Logs;
        let rows = rendered_rows(&logs);
        assert!(
            rows[1].starts_with("┌Logs — not yet wired"),
            "placeholder block title: {}",
            rows[1]
        );
        assert_eq!(
            rows[23],
            "└".to_string() + &"─".repeat(78) + "┘",
            "placeholder block bottom border"
        );
    }

    /// Switching screens moves the placeholder to that screen's module
    /// (per-screen dispatch proof).
    #[test]
    fn screen_dispatch_renders_the_active_screen() {
        let mut state = AppState::new();
        state.screen = Screen::Alarms;
        let rows = rendered_rows(&state);
        assert!(
            rows[1].starts_with("┌Alarms — not yet wired"),
            "Alarms placeholder rendered: {}",
            rows[1]
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
}
