//! Dashboard screen (06-02): the four live panels — status, modules,
//! metrics, sessions — plus the bottom status line. Every panel renders
//! its own Loading/Loaded/Error state from the latest
//! [`Snapshot`](crate::workers::refresh::Snapshot); a dead gateway shows
//! errors, never a frozen or blank UI (must-have truth #2).

use ratatui::Frame;
use ratatui::layout::Constraint::{Length, Min, Ratio};
use ratatui::layout::{Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Row, Table};

use crate::state::{AppState, session_rows};
use crate::workers::refresh::Snapshot;

/// Render the dashboard: 2×2 panel grid + a one-row status line.
pub fn render(state: &AppState, frame: &mut Frame, area: Rect) {
    let [body, status_line] = Layout::vertical([Min(0), Length(1)]).areas(area);
    let [left, right] = Layout::horizontal([Ratio(1, 2), Ratio(1, 2)]).areas(body);
    let [status_pane, metrics_pane] = Layout::vertical([Ratio(2, 3), Ratio(1, 3)]).areas(left);
    let [modules_pane, sessions_pane] = Layout::vertical([Ratio(1, 2), Ratio(1, 2)]).areas(right);

    let snapshot = state.dashboard.snapshot.as_ref();

    render_status(snapshot, frame, status_pane);
    render_metrics(snapshot, frame, metrics_pane);
    render_modules(snapshot, frame, modules_pane);
    render_sessions(state, snapshot, frame, sessions_pane);
    render_status_line(state, frame, status_line);
}

/// The status panel: gateway identity, running state, uptime, cpu,
/// memory essentials — derived from the typed result, minimal
/// formatting (human summary fields, not serde-pretty).
fn render_status(snapshot: Option<&Snapshot>, frame: &mut Frame, area: Rect) {
    let block = Block::bordered().title("status");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let text = match panel(snapshot, |s| (&s.status, &s.status_error)) {
        PanelState::Loading => vec![Line::from("Loading…")],
        PanelState::Error(message) => vec![
            Line::from(Span::styled("error", Style::default().fg(Color::Red))),
            Line::from(
                message
                    .chars()
                    .take(inner.width as usize - 1)
                    .collect::<String>(),
            ),
        ],
        PanelState::Loaded(result) => {
            let mut lines = vec![
                Line::from(format!(
                    "gateway  {} ({})",
                    result.gateway.name.as_deref().unwrap_or("unnamed"),
                    result.gateway.edition.as_deref().unwrap_or("?")
                )),
                Line::from(format!("version  {}", result.gateway.ignition_version)),
                Line::from(format!("state    {}", result.state)),
                Line::from(format!(
                    "uptime   {}",
                    fmt_duration(result.overview.uptime_ms)
                )),
                Line::from(format!(
                    "cpu      {:.1}%",
                    result.overview.cpu_fraction * 100.0
                )),
            ];
            if let [used, max, ..] = result.overview.memory.as_slice() {
                lines.push(Line::from(format!(
                    "heap     {} / {}",
                    fmt_mib(*used),
                    fmt_mib(*max)
                )));
            }
            if let Some(license) = &result.overview.license {
                lines.push(Line::from(match license.trial_remaining_s {
                    Some(secs) => format!(
                        "license  {} (trial {} left)",
                        license.state,
                        fmt_duration(secs * 1000)
                    ),
                    None => format!("license  {}", license.state),
                }));
            }
            lines
        }
    };
    frame.render_widget(Paragraph::new(text), inner);
}

/// The modules panel: name / state / license table.
fn render_modules(snapshot: Option<&Snapshot>, frame: &mut Frame, area: Rect) {
    let block = Block::bordered().title("modules");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let text = match panel(snapshot, |s| (&s.modules, &s.modules_error)) {
        PanelState::Loading => vec![Line::from("Loading…")],
        PanelState::Error(message) => vec![
            Line::from(Span::styled("error", Style::default().fg(Color::Red))),
            Line::from(
                message
                    .chars()
                    .take(inner.width as usize - 1)
                    .collect::<String>(),
            ),
        ],
        PanelState::Loaded(result) => result
            .items
            .iter()
            .map(|module| {
                Line::from(format!(
                    "{:<20} {:<8} {}",
                    module.name.chars().take(20).collect::<String>(),
                    module.state.as_deref().unwrap_or("?"),
                    module.license_state.as_deref().unwrap_or("?")
                ))
            })
            .collect(),
    };
    frame.render_widget(Paragraph::new(text), inner);
}

/// The metrics panel: the key gauges (cpu %, heap, threads).
fn render_metrics(snapshot: Option<&Snapshot>, frame: &mut Frame, area: Rect) {
    let block = Block::bordered().title("metrics");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let text = match panel(snapshot, |s| (&s.metrics, &s.metrics_error)) {
        PanelState::Loading => vec![Line::from("Loading…")],
        PanelState::Error(message) => vec![
            Line::from(Span::styled("error", Style::default().fg(Color::Red))),
            Line::from(message.clone()),
        ],
        PanelState::Loaded(result) => vec![
            Line::from(format!("cpu     {:.1}%", result.current.cpu)),
            Line::from(format!(
                "heap    {} / {}",
                fmt_mib(result.current.heap_memory),
                fmt_mib(result.current.max_memory)
            )),
            Line::from(format!(
                "threads {} run / {} wait / {} blocked",
                result.threads.running, result.threads.waiting, result.threads.blocked
            )),
        ],
    };
    frame.render_widget(Paragraph::new(text), inner);
}

/// The sessions panel: selectable id / type / user table (the terminate
/// target — Task 2 wires `t`/Enter to the confirm modal).
fn render_sessions(state: &AppState, snapshot: Option<&Snapshot>, frame: &mut Frame, area: Rect) {
    let block = Block::bordered().title("sessions");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    match panel(snapshot, |s| (&s.sessions, &s.sessions_error)) {
        PanelState::Loading => {
            frame.render_widget(Paragraph::new("Loading…"), inner);
        }
        PanelState::Error(message) => {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled("error", Style::default().fg(Color::Red))),
                    Line::from(message.clone()),
                ]),
                inner,
            );
        }
        PanelState::Loaded(result) => {
            let rows = session_rows(result);
            if rows.is_empty() {
                frame.render_widget(Paragraph::new("no active sessions"), inner);
                return;
            }
            let table = Table::new(
                rows.iter()
                    .map(|row| {
                        Row::new(vec![
                            row.id.chars().take(20).collect::<String>(),
                            row.kind.as_str().to_string(),
                            row.user.clone(),
                        ])
                    })
                    .collect::<Vec<Row>>(),
                [
                    ratatui::layout::Constraint::Length(21),
                    ratatui::layout::Constraint::Length(11),
                    ratatui::layout::Constraint::Min(0),
                ],
            )
            .header(
                Row::new(vec!["id", "type", "user"])
                    .style(Style::default().add_modifier(Modifier::BOLD)),
            )
            .row_highlight_style(Style::default().bg(Color::DarkGray))
            .highlight_symbol("▸ ");
            // render takes &AppState — a copied cursor keeps update the
            // single owner of selection mutations (TableState is Copy).
            let mut table_state = state.dashboard.sessions_table;
            frame.render_stateful_widget(table, inner, &mut table_state);
        }
    }
}

/// The bottom status line: refresh freshness ("N s ago", live via the
/// 250 ms tick) + the busy markers + key hints. A running one-shot
/// action names itself ("running: wait gateway") — long waits never
/// block input, the label is the only footprint.
fn render_status_line(state: &AppState, frame: &mut Frame, area: Rect) {
    let freshness = match state.dashboard.last_refresh {
        None => "refresh: pending".to_string(),
        Some(at) => format!("refresh: {} s ago", at.elapsed().as_secs()),
    };
    let busy = if state.dashboard.busy {
        " · refreshing"
    } else {
        ""
    };
    let in_flight = state
        .dashboard
        .in_flight
        .map(|label| format!(" · running: {label}"))
        .unwrap_or_default();
    let text = format!("{freshness}{busy}{in_flight} · a actions · r refresh · t terminate");
    frame.render_widget(Paragraph::new(Line::from(text)), area);
}

/// A panel's tri-state, projected out of the snapshot (RESEARCH
/// async-github LoadingState pattern — Loading, Loaded, Error; never
/// blank, never blocking).
enum PanelState<'a, T> {
    Loading,
    Loaded(&'a T),
    Error(&'a String),
}

fn panel<'a, T>(
    snapshot: Option<&'a Snapshot>,
    project: impl FnOnce(&'a Snapshot) -> (&'a Option<T>, &'a Option<String>),
) -> PanelState<'a, T> {
    let Some(snap) = snapshot else {
        return PanelState::Loading;
    };
    let (data, error) = project(snap);
    if let Some(value) = data {
        PanelState::Loaded(value)
    } else if let Some(message) = error {
        PanelState::Error(message)
    } else {
        // Unreachable by construction (the worker always sets one), but
        // honest: unknown is Loading, never blank.
        PanelState::Loading
    }
}

/// Epoch-ms → compact human duration ("3d 4h", "5m").
fn fmt_duration(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let (days, hours, mins) = (secs / 86_400, (secs % 86_400) / 3_600, (secs % 3_600) / 60);
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {mins}m")
    } else {
        format!("{mins}m")
    }
}

/// Bytes → MiB with one decimal.
fn fmt_mib(bytes: i64) -> String {
    format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::render;
    use crate::state::AppState;

    /// Render `state` on an 80x24 TestBackend; the joined buffer text.
    fn rendered(state: &AppState) -> String {
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
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Fresh state (no snapshot): all four panels render Loading —
    /// never blank (must-have truth #2's honest starting point).
    #[test]
    fn panels_render_loading_before_first_refresh() {
        let state = AppState::new();
        let text = rendered(&state);
        for title in ["status", "modules", "metrics", "sessions"] {
            assert!(text.contains(title), "panel title {title} renders");
        }
        assert!(text.contains("Loading…"), "loading state shows: {text}");
        assert!(text.contains("refresh: pending"), "status line: {text}");
    }

    /// A populated snapshot (via the REAL composition path — wiremock
    /// gateway → `snapshot()`) renders the known rows: gateway version,
    /// module row, session row, gauges (must-have truth #1's visible
    /// half; the 5 s cadence is the worker's, proven in refresh.rs).
    #[tokio::test]
    async fn populated_snapshot_renders_panel_rows() {
        let server = wiremock::MockServer::start().await;
        crate::workers::refresh::test_support::mount_gateway(&server).await;
        let api = std::sync::Arc::new(ignition_core::client::ReqwestGatewayApi::for_tests(
            &server.uri(),
            None,
        ));
        let snap = crate::workers::refresh::snapshot(&api).await;

        let mut state = AppState::new();
        state.dashboard.snapshot = Some(snap);
        let text = rendered(&state);

        assert!(text.contains("8.3.6 (b2026042713)"), "version row: {text}");
        assert!(text.contains("RUNNING"), "state row: {text}");
        assert!(text.contains("Perspective"), "module row: {text}");
        assert!(text.contains("ACTIVE"), "module state: {text}");
        assert!(text.contains("ps-1"), "session id: {text}");
        assert!(text.contains("perspective"), "session type: {text}");
        assert!(text.contains("4.9%"), "cpu gauge: {text}");
    }

    /// An all-error snapshot (dead gateway) renders per-panel errors —
    /// the never-frozen guarantee.
    #[tokio::test]
    async fn dead_gateway_renders_errors_not_blank() {
        let api = std::sync::Arc::new(ignition_core::client::ReqwestGatewayApi::for_tests(
            "http://127.0.0.1:1/",
            None,
        ));
        let snap = crate::workers::refresh::snapshot(&api).await;

        let mut state = AppState::new();
        state.dashboard.snapshot = Some(snap);
        let text = rendered(&state);

        // Four "error" headers (one per panel) + no Loading (data DID
        // arrive — it's error data).
        assert_eq!(
            text.matches("error").count(),
            4,
            "every panel errors: {text}"
        );
        assert!(!text.contains("Loading"), "not loading — errored: {text}");
    }
}
