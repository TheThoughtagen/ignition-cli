//! Alarms screen — placeholder until 06-05 owns this module (active
//! alarm panel + ack flow).

use ratatui::Frame;
use ratatui::widgets::Block;

use crate::state::AppState;

/// Render the alarms body. 06-05 replaces this placeholder with the
/// alarm panel.
pub fn render(_state: &AppState, frame: &mut Frame, area: ratatui::layout::Rect) {
    frame.render_widget(Block::bordered().title("Alarms — not yet wired"), area);
}
