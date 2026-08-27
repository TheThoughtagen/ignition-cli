//! Rig screen — placeholder until 06-06 owns this module (local Docker
//! compose rig status + lifecycle actions).

use ratatui::Frame;
use ratatui::widgets::Block;

use crate::state::AppState;

/// Render the rig body. 06-06 replaces this placeholder with the rig
/// status panel.
pub fn render(_state: &AppState, frame: &mut Frame, area: ratatui::layout::Rect) {
    frame.render_widget(Block::bordered().title("Rig — not yet wired"), area);
}
