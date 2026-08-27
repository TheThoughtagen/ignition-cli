//! Alarms screen (06-03): the polled active-alarm table — FULL UUIDs
//! (the 05-08 copy-paste-verbatim convention), source, name, state,
//! priority — with the loading/error/empty tri-state, a per-row ack
//! form modal (username REQUIRED — the 3-arg wire form), and the
//! on-demand history browse. Poll age rides the status row.

use ratatui::Frame;
use ratatui::layout::Constraint::{Length, Min};
use ratatui::layout::{Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Cell, Clear, Paragraph, Row, Table};

use ignition_core::actions::tags::AlarmRow;
use ratatui::layout::Constraint::Ratio;

use crate::state::{AppState, Modal};

/// Render the alarms body: the table pane + the one-row status line.
pub fn render(state: &AppState, frame: &mut Frame, area: Rect) {
    let [table, status] = Layout::vertical([Min(0), Length(1)]).areas(area);
    render_table(state, frame, table);
    render_status(state, frame, status);
}

/// One alarm row of the table: the FULL event UUID (never truncated,
/// never elided — copy-paste-verbatim), source, name, state verbatim.
/// Priority rides the ack result modal (the 80-col table cannot buy a
/// fifth column without lying about the UUID).
fn alarm_cells(alarm: &AlarmRow) -> Vec<Cell<'static>> {
    vec![
        Cell::from(alarm.event_id.clone()),
        Cell::from(alarm.source.clone()),
        Cell::from(alarm.name.clone().unwrap_or_else(|| "-".into())),
        Cell::from(alarm.state.clone()),
    ]
}

/// The active-alarm table with its tri-state: Loading before the first
/// poll, the honest error when a poll failed, the rows when they
/// landed — "no active alarms" (a state, not a crash) when empty.
fn render_table(state: &AppState, frame: &mut Frame, area: Rect) {
    let block = Block::bordered().title("alarms");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let alarms = &state.alarms;
    match (&alarms.active, &alarms.error) {
        (None, None) => {
            frame.render_widget(Paragraph::new(Line::from("Loading…")), inner);
        }
        (None, Some(message)) => {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled(
                        "poll error",
                        Style::default().add_modifier(Modifier::BOLD),
                    )),
                    Line::from(
                        message
                            .chars()
                            .take(inner.width as usize - 1)
                            .collect::<String>(),
                    ),
                ]),
                inner,
            );
        }
        (Some(rows), error) => {
            if rows.is_empty() {
                let mut lines = vec![Line::from("no active alarms")];
                if let Some(message) = error {
                    lines.push(Line::from(format!("(last poll note: {message})")));
                }
                frame.render_widget(Paragraph::new(lines), inner);
                return;
            }
            let header = Row::new(vec![
                Cell::from("event id"),
                Cell::from("source"),
                Cell::from("name"),
                Cell::from("state"),
            ])
            .style(Style::default().add_modifier(Modifier::BOLD));
            let rows: Vec<Row> = rows
                .iter()
                .map(|alarm| Row::new(alarm_cells(alarm)))
                .collect();
            // The UUID column is a FIXED 36 — the must-have; the source
            // column flexes (invisible below ~80 cols, room on wide
            // terminals). Constraints total 72 + 3 spacing = 75 ≤ 76
            // inner width at 80 cols, so the UUID never compresses.
            let widths = [Length(36), Min(6), Length(14), Length(17)];
            let table = Table::new(rows, widths)
                .header(header)
                .row_highlight_style(Style::default().add_modifier(Modifier::BOLD));
            // render takes &AppState — a copied cursor keeps update the
            // single owner of selection mutations (TableState is Copy).
            let mut table_state = state.alarms.table;
            frame.render_stateful_widget(table, inner, &mut table_state);
        }
    }
}

/// The status row: row count, poll age, the key hints.
fn render_status(state: &AppState, frame: &mut Frame, area: Rect) {
    let alarms = &state.alarms;
    let count = match &alarms.active {
        Some(rows) => format!("{} active", rows.len()),
        None => "—".to_string(),
    };
    let age = alarms
        .last_poll
        .map(|at| format!("polled {}s ago", at.elapsed().as_secs()))
        .unwrap_or_else(|| "not polled yet".to_string());
    let busy = if alarms.busy { " · refreshing" } else { "" };
    let text = format!(" {count} · {age}{busy} · a ack · h history · ↑↓ select");
    frame.render_widget(Paragraph::new(Line::from(text)), area);
}

/// The ack form overlay (the screen-owned modal pattern): target UUID,
/// username (required) + note (optional) fields, the edited one
/// arrowed, Enter submits — disabled until the username is non-empty.
pub fn render_ack_overlay(modal: &Modal, frame: &mut Frame) {
    let Modal::Ack {
        event_id,
        username,
        note,
        field,
    } = modal
    else {
        return;
    };
    let area = frame.area().centered(Ratio(1, 2), Length(9));
    frame.render_widget(Clear, area);
    let cursor = |active: bool| if active { "▸ " } else { "  " };
    // The target UUID rides BARE on its own line — 36 chars fit the
    // half-width modal's inner area where a prefixed line would not,
    // and a bare UUID is one clean selection for copy-paste.
    let lines = vec![
        Line::from(event_id.clone()),
        Line::default(),
        Line::from(format!(
            "{}user  {username}{}",
            cursor(*field == 0),
            if *field == 0 { "▏" } else { "" }
        )),
        Line::from(format!(
            "{}note  {note}{}",
            cursor(*field == 1),
            if *field == 1 { "▏" } else { "" }
        )),
        Line::default(),
        Line::from(if username.trim().is_empty() {
            "username REQUIRED (the 3-arg wire form)"
        } else {
            "Enter acknowledge · Esc cancel"
        }),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title("ack alarm")),
        area,
    );
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::render;
    use crate::state::{AppState, Modal};

    fn alarm(
        event_id: &str,
        name: Option<&str>,
        state: &str,
    ) -> ignition_core::actions::tags::AlarmRow {
        ignition_core::actions::tags::AlarmRow {
            event_id: event_id.to_string(),
            source: "prov:default".into(),
            state: state.into(),
            priority: "High".into(),
            name: name.map(str::to_string),
        }
    }

    /// Render the Alarms screen on an 80x24 TestBackend; joined rows.
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

    /// A two-alarm table renders BOTH full UUIDs verbatim (the 05-08
    /// copy-paste convention — the must-have render proof) plus the
    /// states and the status row.
    #[test]
    fn two_alarm_table_shows_full_uuids() {
        let mut state = AppState::new();
        state.alarms.active = Some(vec![
            alarm(
                "c0ffee00-1234-5678-9abc-def012345678",
                Some("PumpCavitation"),
                "Active, Unacknowledged",
            ),
            alarm(
                "deadbeef-aaaa-bbbb-cccc-dddddddddddd",
                None,
                "Clear, Unacknowledged",
            ),
        ]);
        state.alarms.last_poll = Some(std::time::Instant::now());
        let rows = rendered(&state);
        let text = rows.join("\n");

        for uuid in [
            "c0ffee00-1234-5678-9abc-def012345678",
            "deadbeef-aaaa-bbbb-cccc-dddddddddddd",
        ] {
            assert!(text.contains(uuid), "full UUID {uuid} verbatim: {text}");
        }
        assert!(text.contains("PumpCavitation"), "name column: {text}");
        assert!(
            text.contains("Active, Unack"),
            "state column (verbatim, may truncate): {text}"
        );
        assert!(
            rows.iter().any(|row| row.contains("2 active")),
            "status row counts rows: {text}"
        );
        assert!(
            rows.iter().any(|row| row.contains("a ack")),
            "status row hints ack: {text}"
        );
    }

    /// The empty state says "no active alarms" — a state, never a
    /// crash — and the not-yet-polled state says Loading.
    #[test]
    fn empty_and_loading_states_render() {
        let mut state = AppState::new();
        state.alarms.active = Some(Vec::new());
        state.alarms.last_poll = Some(std::time::Instant::now());
        let rows = rendered(&state);
        assert!(
            rows.iter().any(|row| row.contains("no active alarms")),
            "empty state: {rows:?}"
        );

        let fresh = AppState::new();
        let rows = rendered(&fresh);
        assert!(
            rows.iter().any(|row| row.contains("Loading")),
            "loading state: {rows:?}"
        );
    }

    /// The poll-error state replaces the table with the honest error.
    #[test]
    fn poll_error_state_renders_honestly() {
        let mut state = AppState::new();
        state.alarms.active = None;
        state.alarms.error = Some("routes_not_deployed (exit 6)".into());
        let rows = rendered(&state);
        let text = rows.join("\n");
        assert!(text.contains("poll error"), "error banner: {text}");
        assert!(text.contains("routes_not_deployed"), "error detail: {text}");
    }

    /// The ack form overlay: target UUID, both fields, the cursor, and
    /// the required-username hint while empty.
    #[test]
    fn ack_overlay_renders_fields_and_requirement() {
        let mut state = AppState::new();
        state.open_modal(Modal::Ack {
            event_id: "c0ffee00-1234-5678-9abc-def012345678".into(),
            username: String::new(),
            note: String::new(),
            field: 0,
        });
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("test terminal");
        terminal
            .draw(|frame| super::render_ack_overlay(state.modal.as_ref().expect("modal"), frame))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let text: String = (0..buffer.area.height)
            .flat_map(|y| (0..buffer.area.width).map(move |x| (x, y)))
            .map(|(x, y)| buffer[(x, y)].symbol().to_string())
            .collect();

        assert!(text.contains("ack alarm"), "title: {text}");
        assert!(
            text.contains("c0ffee00-1234-5678-9abc-def012345678"),
            "target UUID: {text}"
        );
        assert!(
            text.contains("username REQUIRED"),
            "requirement hint while empty: {text}"
        );
        assert!(text.contains("▸ user"), "cursor on username: {text}");
    }
}
