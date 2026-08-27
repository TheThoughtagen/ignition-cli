//! Tags screen — placeholder until 06-04 owns this module (tag browser +
//! live watch).

use ratatui::Frame;
use ratatui::widgets::Block;

use crate::state::AppState;

/// Render the tags body. 06-04 replaces this placeholder with the
/// browser + watch panes.
pub fn render(_state: &AppState, frame: &mut Frame, area: ratatui::layout::Rect) {
    frame.render_widget(Block::bordered().title("Tags — not yet wired"), area);
}
