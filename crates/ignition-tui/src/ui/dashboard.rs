//! Dashboard screen — placeholder until 06-02 owns this module
//! (status/modules/metrics/sessions panels).

use ratatui::Frame;
use ratatui::widgets::Block;

use crate::state::AppState;

/// Render the dashboard body. 06-02 replaces this placeholder with the
/// live refresh panels.
pub fn render(_state: &AppState, frame: &mut Frame, area: ratatui::layout::Rect) {
    frame.render_widget(Block::bordered().title("Dashboard — not yet wired"), area);
}
