//! Logs screen (06-03): the live tail stream — ring-backed lines with
//! render-side level filtering (ERROR red, WARN yellow, INFO default,
//! DEBUG/TRACE dim), scrollback over the retained entries, and the
//! follow toggle. The status row names the filter, follow state, ring
//! fill, and eviction count.
//!
//! The window math keys off the FILTERED view: `scroll_offset` counts
//! filtered lines above the bottom edge, so scrolling and filtering
//! compose without surprises.

use ratatui::Frame;
use ratatui::layout::Constraint::{Length, Min};
use ratatui::layout::{Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::state::{AppState, LOG_RING_CAP};

/// Render the logs body: the stream pane + the one-row status line.
pub fn render(state: &AppState, frame: &mut Frame, area: Rect) {
    let [stream, status] = Layout::vertical([Min(0), Length(1)]).areas(area);
    render_stream(state, frame, stream);
    render_status(state, frame, status);
}

/// One retained entry as a display line: `LEVEL time logger message`,
/// level color-coded. The level span carries the color; the rest stays
/// default so long messages never inherit a shouty hue.
fn log_line(entry: &ignition_core::client::logs::LogEntry) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{:>5}", entry.level), level_style(&entry.level)),
        Span::raw(" "),
        Span::raw(time_of_day(entry.timestamp)),
        Span::raw(" "),
        Span::raw(entry.logger_name.chars().take(28).collect::<String>()),
        Span::raw("  "),
        Span::raw(entry.message.clone()),
    ])
}

/// Level → color (must-have: ERROR red, WARN yellow, INFO default,
/// DEBUG dim; FATAL joins ERROR, TRACE joins DEBUG).
fn level_style(level: &str) -> Style {
    match level {
        "ERROR" | "FATAL" => Style::default().fg(Color::Red),
        "WARN" => Style::default().fg(Color::Yellow),
        "DEBUG" | "TRACE" => Style::default().add_modifier(Modifier::DIM),
        _ => Style::default(),
    }
}

/// Epoch-ms → `HH:MM:SS.mmm` UTC (time-of-day only — the pane keeps
/// its width for logger + message; no timezone machinery, the raw
/// epoch-ms always rides --json on the CLI side).
fn time_of_day(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let millis = ms.rem_euclid(1000);
    let time_of_day = secs.rem_euclid(86_400);
    let (hour, minute, second) = (
        time_of_day / 3600,
        (time_of_day % 3600) / 60,
        time_of_day % 60,
    );
    format!("{hour:02}:{minute:02}:{second:02}.{millis:03}")
}

/// The stream pane: the filtered ring windowed onto the visible
/// height, ending `scroll_offset` lines above the newest.
fn render_stream(state: &AppState, frame: &mut Frame, area: Rect) {
    let block = Block::bordered().title("logs");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let logs = &state.logs;
    let filtered: Vec<_> = logs
        .ring
        .iter()
        .filter(|entry| logs.filter.matches(&entry.level))
        .collect();
    // Display-side clamp: ring turnover can shrink the filtered set
    // below an offset armed earlier — never show a blank-beyond-top.
    let offset = logs.scroll_offset.min(filtered.len().saturating_sub(1));
    let end = filtered.len().saturating_sub(offset);
    let start = end.saturating_sub(inner.height as usize);
    let lines: Vec<Line> = filtered[start..end]
        .iter()
        .map(|entry| log_line(entry))
        .collect();

    frame.render_widget(Paragraph::new(lines), inner);
}

/// The status row: filter, follow state, ring fill + evictions, and
/// the key hints.
fn render_status(state: &AppState, frame: &mut Frame, area: Rect) {
    let logs = &state.logs;
    let follow = if logs.follow { "on" } else { "off" };
    let dropped = if logs.dropped > 0 {
        format!(" (+{} dropped)", logs.dropped)
    } else {
        String::new()
    };
    let text = format!(
        " filter:{} follow:{} ring:{}/{}{dropped} · l filter · f follow · a actions",
        logs.filter.label(),
        follow,
        logs.ring.len(),
        LOG_RING_CAP,
    );
    frame.render_widget(Paragraph::new(Line::from(text)), area);
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::render;
    use crate::state::{AppState, LOG_RING_CAP, LogLevelFilter, LogsData};

    fn entry(timestamp: i64, level: &str, message: &str) -> ignition_core::client::logs::LogEntry {
        ignition_core::client::logs::LogEntry {
            timestamp,
            logger_name: "GatewayManager".into(),
            level: level.into(),
            message: message.into(),
            stack: Vec::new(),
            mdc: Default::default(),
            extra: Default::default(),
        }
    }

    /// Render the Logs screen on an 80x24 TestBackend; joined rows.
    fn rendered(state: &AppState) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("test terminal");
        terminal
            .draw(|frame| render(state, frame, frame.area()))
            .expect("draw");
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

    /// A seeded ring renders its entries and the status row names the
    /// filter + ring size (the must-have render proof).
    #[test]
    fn seeded_ring_renders_lines_and_status() {
        let mut state = AppState::new();
        state
            .logs
            .push_line(entry(1_000, "INFO", "gateway started"));
        state
            .logs
            .push_line(entry(2_000, "WARN", "disk usage high"));
        state.logs.push_line(entry(3_000, "ERROR", "module fault"));
        let rows = rendered(&state);

        let text = rows.join("\n");
        assert!(text.contains("INFO"), "INFO line renders: {text}");
        assert!(text.contains("gateway started"), "message renders: {text}");
        assert!(text.contains("module fault"), "latest line renders: {text}");
        assert!(
            rows.iter().any(|row| row.contains("filter:all")),
            "status row shows the filter indicator: {text}"
        );
        assert!(
            rows.iter().any(|row| row.contains("ring:3")),
            "status row shows the ring size: {text}"
        );
        assert!(
            rows.iter().any(|row| row.contains("follow:on")),
            "status row shows follow state: {text}"
        );
    }

    /// Time-of-day formatting: known instants.
    #[test]
    fn time_of_day_formats_known_instants() {
        assert_eq!(super::time_of_day(0), "00:00:00.000");
        assert_eq!(super::time_of_day(1_787_346_747_022), "21:12:27.022");
        assert_eq!(super::time_of_day(86_400_999), "00:00:00.999");
    }

    /// Level colors: ERROR red, WARN yellow, DEBUG dim, INFO default —
    /// asserted at the buffer-cell style level.
    #[test]
    fn level_colors_are_coded() {
        let mut state = AppState::new();
        state.logs.push_line(entry(1, "ERROR", "e"));
        state.logs.push_line(entry(2, "WARN", "w"));
        state.logs.push_line(entry(3, "DEBUG", "d"));
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("test terminal");
        terminal
            .draw(|frame| render(&state, frame, frame.area()))
            .expect("draw");
        let buffer = terminal.backend().buffer();

        // Find each level token's row; read the color at the FIRST
        // character of the level string (inside the styled span — the
        // span includes the right-align pad, so index at the token
        // start itself, not past it).
        let mut colors = std::collections::HashMap::new();
        for y in 0..buffer.area.height {
            let row: String = (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect();
            for token in ["ERROR", "WARN", "DEBUG"] {
                if let Some(at) = row.find(token) {
                    colors.insert(token.to_string(), buffer[(at as u16, y)].fg);
                }
            }
        }
        assert_eq!(
            colors.get("ERROR"),
            Some(&ratatui::style::Color::Red),
            "ERROR is red: {colors:?}"
        );
        assert_eq!(
            colors.get("WARN"),
            Some(&ratatui::style::Color::Yellow),
            "WARN is yellow: {colors:?}"
        );
        let debug_cell_fg = *colors.get("DEBUG").expect("DEBUG line present");
        assert_ne!(
            debug_cell_fg,
            ratatui::style::Color::Red,
            "DEBUG not red; dim is a modifier (asserting not-shouty)"
        );
    }

    /// The render-side filter: with filter=Warn, only WARN+ entries
    /// render (INFO/DEBUG vanish) and the status row names the filter.
    #[test]
    fn render_side_filter_hides_below_threshold() {
        let mut state = AppState::new();
        state.logs.push_line(entry(1, "DEBUG", "dbg"));
        state.logs.push_line(entry(2, "INFO", "nfo"));
        state.logs.push_line(entry(3, "WARN", "wrn"));
        state.logs.push_line(entry(4, "ERROR", "err"));
        state.logs.filter = LogLevelFilter::Warn;
        let rows = rendered(&state);

        let text = rows.join("\n");
        assert!(
            text.contains("wrn") && text.contains("err"),
            "Warn+ shows: {text}"
        );
        assert!(!text.contains("dbg"), "DEBUG hidden: {text}");
        assert!(!text.contains("nfo"), "INFO hidden: {text}");
        assert!(
            rows.iter().any(|row| row.contains("filter:warn")),
            "filter indicator: {text}"
        );
    }

    /// Scrollback windowing: with 3 filtered lines, 2 visible rows,
    /// and scroll_offset=2, the OLDEST line renders (the window's top).
    #[test]
    fn scrollback_windows_the_older_lines() {
        let mut state = AppState::new();
        for (i, msg) in ["old-line", "mid-line", "new-line"].into_iter().enumerate() {
            let mut e = entry(i as i64 + 1, "INFO", msg);
            e.logger_name = "GM".into(); // short logger so 60 cols fit the message
            state.logs.push_line(e);
        }
        state.logs.scroll_offset = 2; // two above the bottom
        state.logs.follow = false;

        // Shrink the pane: 2 inner rows inside the bordered block.
        let mut terminal = Terminal::new(TestBackend::new(60, 6)).expect("test terminal");
        terminal
            .draw(|frame| render(&state, frame, frame.area()))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let text: String = (0..buffer.area.height)
            .flat_map(|y| (0..buffer.area.width).map(move |x| (x, y)))
            .map(|(x, y)| buffer[(x, y)].symbol().to_string())
            .collect();

        assert!(text.contains("old-line"), "scrolled to the top: {text}");
        assert!(
            !text.contains("new-line"),
            "bottom line out of window: {text}"
        );
    }

    /// The ring cap + eviction accounting (the memory-bounded proof):
    /// pushing 10,100 entries keeps exactly 10,000 with the oldest
    /// dropped and the counter honest.
    #[test]
    fn ring_caps_at_10k_and_counts_evictions() {
        let mut logs = LogsData::default();
        for i in 0..(LOG_RING_CAP + 100) {
            logs.push_line(entry(i as i64, "INFO", "fill"));
        }
        assert_eq!(logs.ring.len(), LOG_RING_CAP, "capped at 10,000");
        assert_eq!(logs.dropped, 100, "100 evictions counted");
        assert_eq!(
            logs.ring.front().map(|e| e.timestamp),
            Some(100),
            "the oldest 100 were evicted from the front"
        );
    }

    /// The filter threshold matcher: filter=Warn passes WARN/ERROR/
    /// FATAL and rejects TRACE/DEBUG/INFO; unknown levels rank with
    /// INFO; All passes everything.
    #[test]
    fn filter_threshold_matches_levels() {
        assert!(LogLevelFilter::All.matches("TRACE"));
        assert!(LogLevelFilter::All.matches("GARBAGE"));

        let warn = LogLevelFilter::Warn;
        assert!(warn.matches("WARN"));
        assert!(warn.matches("ERROR"));
        assert!(warn.matches("FATAL"));
        assert!(!warn.matches("INFO"));
        assert!(!warn.matches("DEBUG"));
        assert!(!warn.matches("TRACE"));

        // Unknown ranks with INFO: visible under Info, hidden under Warn.
        assert!(LogLevelFilter::Info.matches("AUDIT"));
        assert!(!warn.matches("AUDIT"));
    }

    /// The wire tokens the tail restart carries as min_level.
    #[test]
    fn filter_wire_tokens() {
        assert_eq!(LogLevelFilter::All.wire(), None);
        assert_eq!(LogLevelFilter::Warn.wire(), Some("WARN"));
        assert_eq!(LogLevelFilter::Error.wire(), Some("ERROR"));
    }

    /// `filtered_len` — the scroll bound over the FILTERED view.
    #[test]
    fn filtered_len_counts_only_passing_entries() {
        let mut logs = LogsData::default();
        logs.push_line(entry(1, "INFO", "a"));
        logs.push_line(entry(2, "ERROR", "b"));
        logs.push_line(entry(3, "DEBUG", "c"));
        logs.filter = LogLevelFilter::Error;
        assert_eq!(logs.filtered_len(), 1);
    }
}
