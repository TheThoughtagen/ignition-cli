//! Cockpit chrome rendering (Phase 6 research, Pattern 4).
//!
//! 06-01 ships the shell chrome: the top tab bar (every screen, active
//! highlighted) + the active screen's body. Pure over
//! [`AppState`] — no I/O, no awaits.

use ratatui::layout::Constraint::Length;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Tabs;
use ratatui::Frame;

use crate::state::{AppState, Screen};

/// Render the whole cockpit: tab bar + the active screen's body.
pub fn render(state: &AppState, frame: &mut Frame) {
    let chunks =
        ratatui::layout::Layout::vertical([Length(1), Length(frame.area().height.saturating_sub(1))])
            .split(frame.area());

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
    frame.render_widget(Tabs::new(titles), chunks[0]);

    // Placeholder body until each screen plan owns its module.
    let body = ratatui::widgets::Paragraph::new(format!(
        "{} — not yet wired",
        state.screen.title()
    ));
    frame.render_widget(body, chunks[1]);
}
