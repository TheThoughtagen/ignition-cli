//! Logs screen — placeholder until 06-03 owns this module (tail view +
//! level filter + scrollback).

use ratatui::Frame;
use ratatui::widgets::Block;

use crate::state::AppState;

/// Render the logs body. 06-03 replaces this placeholder with the live
/// tail view.
pub fn render(_state: &AppState, frame: &mut Frame, area: ratatui::layout::Rect) {
    frame.render_widget(Block::bordered().title("Logs — not yet wired"), area);
}
