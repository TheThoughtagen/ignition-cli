//! Projects screen — placeholder until 06-06 owns this module
//! (project/resource browser + actions + profile switcher).

use ratatui::Frame;
use ratatui::widgets::Block;

use crate::state::AppState;

/// Render the projects body. 06-06 replaces this placeholder with the
/// project/resource browser.
pub fn render(_state: &AppState, frame: &mut Frame, area: ratatui::layout::Rect) {
    frame.render_widget(Block::bordered().title("Projects — not yet wired"), area);
}
