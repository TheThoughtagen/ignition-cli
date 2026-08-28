//! Rig screen (06-06): the local Docker compose rig — the one-shot
//! status summary (the allowlist [`RigStatusResult`] rendered as
//! containers + state; a DOWN rig is data, not an error) and, when
//! the pane is on, the RAW compose-logs stream (`rig logs --tail 200
//! --follow` — the registry's [`crate::routes::Mapping::Streamed`]
//! case, raw lines, never envelope-shaped). The action menu (the
//! full RigCommand verb set with the CLI's exact confirm split)
//! rides `a` — see [`crate::update`].
//!
//! Render is PURE over [`AppState`] — navigation and spawns live in
//! [`crate::update`], docker I/O in [`crate::workers::rig_stream`].

use ratatui::Frame;
use ratatui::layout::Constraint::{Length, Min, Percentage};
use ratatui::layout::{Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use ignition_core::actions::rig::{RigStatusResult, StatusService};

use crate::state::{AppState, RIG_LOG_RING_CAP};

/// Render the rig body: the status summary, the optional logs pane
/// (bottom 40% when on), and the one-row status line.
pub fn render(state: &AppState, frame: &mut Frame, area: Rect) {
    if state.rig.logs_on {
        let [summary, stream, status] =
            Layout::vertical([Min(0), Percentage(40), Length(1)]).areas(area);
        render_summary(state, frame, summary);
        render_stream(state, frame, stream);
        render_status(state, frame, status);
    } else {
        let [summary, status] = Layout::vertical([Min(0), Length(1)]).areas(area);
        render_summary(state, frame, summary);
        render_status(state, frame, status);
    }
}

/// One indented `label value` row under a section header — the
/// label column padded to nine so values clear the longest label
/// (`compose`) by two spaces (the projects screen's field_line
/// shape, widened for the section indent).
fn field_line(label: &str, value: &str) -> Line<'static> {
    Line::from(format!("  {label:<9} {value}"))
}

/// A bold section header (the dashboard table-header convention —
/// structure and grouping, not color).
fn section(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        title.to_string(),
        Style::default().add_modifier(Modifier::BOLD),
    ))
}

/// Fit a path-ish value into `width` columns from the TAIL (the
/// discriminating end of a compose path) with a leading ellipsis
/// when chars must drop — long real-world paths stay identifiable
/// instead of clipping invisibly at the pane border.
fn fit_tail(value: &str, width: usize) -> String {
    let len = value.chars().count();
    if len <= width {
        return value.to_string();
    }
    let keep = width.saturating_sub(1);
    format!("…{}", value.chars().skip(len - keep).collect::<String>())
}

/// Where a field_line's value starts: the 2-space section indent +
/// the 9-wide label column + one separator space.
const VALUE_COLUMN: usize = 12;

/// The published-ports cell for one service row: `host→target/tcp`
/// pairs, comma-joined (`-` when the service publishes nothing).
fn ports_cell(service: &StatusService) -> String {
    if service.publishers.is_empty() {
        return "-".to_string();
    }
    service
        .publishers
        .iter()
        .map(|publisher| {
            format!(
                "{}→{}",
                publisher
                    .published_port
                    .map_or_else(|| "?".to_string(), |port| port.to_string()),
                publisher
                    .target_port
                    .map_or_else(|| "?".to_string(), |port| port.to_string())
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The status summary pane: the UP/DOWN headline with its port
/// occupancy, then blank-separated sections — identity
/// (rig/compose file), one row per compose service (name state
/// health ports), and the named volumes — with the label column
/// aligned and long values fitted to the pane width. The tri-state:
/// Loading before the first load, the honest error when it failed,
/// the allowlist when it landed.
fn render_summary(state: &AppState, frame: &mut Frame, area: Rect) {
    let rig = &state.rig;
    let title = match &rig.status {
        Some(status) => format!("rig — {}", status.rig),
        None => "rig".to_string(),
    };
    let block = Block::bordered().title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    match (&rig.status, &rig.status_error) {
        (None, None) => {
            frame.render_widget(Paragraph::new(Line::from("Loading…")), inner);
        }
        (None, Some(message)) => {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled(
                        "status error".to_string(),
                        Style::default().add_modifier(Modifier::BOLD),
                    )),
                    Line::from(
                        message
                            .chars()
                            .take(inner.width.saturating_sub(1) as usize)
                            .collect::<String>(),
                    ),
                ]),
                inner,
            );
        }
        (Some(status), _) => {
            frame.render_widget(
                Paragraph::new(summary_lines(status, inner.width as usize)),
                inner,
            );
        }
    }
}

/// The summary's display lines from the allowlist result — the
/// 06-UAT readability fix: the derived state gets its own prominent
/// headline line, then GROUPED sections (state facts / identity /
/// services / volumes) separated by blank rows, with the label
/// column aligned and the compose path tail-fitted to the pane so
/// long real-world paths stay identifiable. Layout and grouping
/// only — no new color usage (the monochrome-theme overhaul is
/// backlog).
fn summary_lines(status: &RigStatusResult, width: usize) -> Vec<Line<'static>> {
    let down = status.services.is_empty();
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let mut lines = vec![
        // State prominence: the one-line answer first — derived state
        // plus its port-occupancy subordinate fact on a single bold
        // headline (compact enough that the full section skeleton
        // still fits the 80x24 logs-on split).
        Line::from(Span::styled(
            format!(
                "STATE  {} · PORTS  {}",
                if down { "DOWN" } else { "UP" },
                if status.ports_free { "free" } else { "held" }
            ),
            bold,
        )),
        Line::from(""),
        section("identity"),
        field_line("rig", &status.rig),
    ];
    // The allowlist carries rig and project separately; today both
    // are the resolved project name, so the row renders only when
    // they actually differ (a COMPOSE_PROJECT_NAME rename is exactly
    // when the distinction matters).
    if status.project != status.rig {
        lines.push(field_line("project", &status.project));
    }
    lines.push(field_line(
        "compose",
        &fit_tail(
            &status.compose_file,
            width.saturating_sub(VALUE_COLUMN).max(8),
        ),
    ));
    lines.push(Line::from(""));
    lines.push(section("services"));
    if status.services.is_empty() {
        lines.push(Line::from(Span::styled(
            "  none running — the rig is down (exit-0 data, `up` brings it back)".to_string(),
            Style::default().add_modifier(Modifier::DIM),
        )));
    } else {
        for service in &status.services {
            let health = service.health.clone().unwrap_or_else(|| "-".into());
            let exit = service
                .exit_code
                .map(|code| format!(" exit:{code}"))
                .unwrap_or_default();
            lines.push(Line::from(format!(
                "  {:<14} {:<9} {:<9} {}{exit}",
                service.name,
                service.state,
                health,
                ports_cell(service),
            )));
        }
    }
    lines.push(Line::from(""));
    lines.push(section("volumes"));
    if status.volumes.is_empty() {
        lines.push(Line::from("  -"));
    } else {
        for volume in &status.volumes {
            lines.push(Line::from(format!("  {volume}")));
        }
    }
    lines
}

/// The raw logs pane: the retained compose lines windowed onto the
/// visible height (follow the newest — the raw-pane contract, no
/// envelope, no level coloring: compose passthrough lines).
fn render_stream(state: &AppState, frame: &mut Frame, area: Rect) {
    let block = Block::bordered().title("rig logs");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    let ring = &state.rig.logs;
    let start = ring.len().saturating_sub(inner.height as usize);
    let lines: Vec<Line> = ring
        .iter()
        .skip(start)
        .map(|line| Line::from(line.chars().take(inner.width as usize).collect::<String>()))
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

/// The status row: pane state, ring fill + evictions, key hints.
fn render_status(state: &AppState, frame: &mut Frame, area: Rect) {
    let rig = &state.rig;
    let dropped = if rig.logs_dropped > 0 {
        format!(" (+{} dropped)", rig.logs_dropped)
    } else {
        String::new()
    };
    let pane = if rig.logs_on {
        format!("logs:{}/{}{dropped}", rig.logs.len(), RIG_LOG_RING_CAP)
    } else {
        "logs:off".to_string()
    };
    let text = format!(" {pane} · a actions · r refresh · l logs");
    frame.render_widget(Paragraph::new(Line::from(text)), area);
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::render;
    use crate::state::AppState;

    fn status_fixture() -> ignition_core::actions::rig::RigStatusResult {
        ignition_core::actions::rig::RigStatusResult {
            rig: "fixture-rig".into(),
            project: "fixture-rig".into(),
            compose_file: "/rigs/docker/compose.yml".into(),
            services: vec![ignition_core::actions::rig::StatusService {
                name: "ignition".into(),
                state: "running".into(),
                health: Some("healthy".into()),
                exit_code: None,
                publishers: vec![ignition_core::actions::rig::StatusPublisher {
                    published_port: Some(9088),
                    target_port: Some(8088),
                    protocol: Some("tcp".into()),
                }],
            }],
            volumes: vec!["gw-data".into()],
            ports_free: false,
        }
    }

    /// Render the Rig screen on an 80x24 TestBackend; joined rows.
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

    /// Strip the pane borders + padding from a rendered row so
    /// structural assertions compare the CONTENT (exact rows,
    /// blanks) rather than border furniture.
    fn inner(row: &str) -> String {
        row.trim().trim_matches('│').trim().to_string()
    }

    /// A landed status renders the grouped summary: the rig-named
    /// pane title, the prominent state headline, the blank-separated
    /// sections (identity / services / volumes), the service row,
    /// the status hints.
    #[test]
    fn landed_status_renders_the_summary() {
        let mut state = AppState::new();
        state.rig.status = Some(status_fixture());
        let rows = rendered(&state);
        let text = rows.join("\n");

        assert!(
            rows[0].starts_with("┌rig — fixture-rig"),
            "pane titled for the rig: {}",
            rows[0]
        );
        assert!(
            rows.iter()
                .any(|row| inner(row) == "STATE  UP · PORTS  held"),
            "the state headline carries derived state + occupancy: {text}"
        );
        for header in ["identity", "services", "volumes"] {
            let index = rows
                .iter()
                .position(|row| inner(row) == header)
                .unwrap_or_else(|| panic!("section header {header} renders: {text}"));
            assert!(
                index > 0 && inner(&rows[index - 1]).is_empty(),
                "blank row separates {header} from the previous group: {text}"
            );
        }
        assert!(
            text.contains("fixture-rig") && text.contains("/rigs/docker/compose.yml"),
            "identity rows render: {text}"
        );
        assert!(
            text.contains("gw-data"),
            "volumes render as their own rows: {text}"
        );
        assert!(
            text.contains("ignition") && text.contains("running") && text.contains("healthy"),
            "service row renders: {text}"
        );
        assert!(
            text.contains("9088→8088"),
            "the published-port mapping renders: {text}"
        );
        assert!(
            rows.iter().any(|row| row.contains("a actions")),
            "status hints render: {text}"
        );
        assert!(
            rows.iter().any(|row| row.contains("logs:off")),
            "pane-off state names itself: {text}"
        );
    }

    /// A DOWN rig renders the honest data state (empty services —
    /// never an error pane) on the same section skeleton.
    #[test]
    fn down_rig_renders_as_data() {
        let mut state = AppState::new();
        let mut fixture = status_fixture();
        fixture.services = Vec::new();
        fixture.ports_free = true;
        state.rig.status = Some(fixture);
        let rows = rendered(&state);
        let text = rows.join("\n");
        assert!(
            rows.iter()
                .any(|row| inner(row) == "STATE  DOWN · PORTS  free"),
            "down is the headline state with free occupancy: {text}"
        );
        assert!(
            text.contains("rig is down"),
            "the down hint renders under the services section: {text}"
        );
    }

    /// A real-world compose path (75+ chars) tail-fits to the pane
    /// width with a leading ellipsis — the discriminating end stays
    /// visible instead of clipping at the border.
    #[test]
    fn long_compose_path_tail_fits_the_pane() {
        let mut state = AppState::new();
        let mut fixture = status_fixture();
        fixture.compose_file =
            "/Users/pmannion/Documents/whiskeyhouse/ignition-git-module/docker/docker-compose.yml"
                .into();
        state.rig.status = Some(fixture);
        let rows = rendered(&state);
        let row = rows
            .iter()
            .find(|row| row.contains("compose"))
            .expect("the compose row renders");
        let content = inner(row);
        assert!(
            content.starts_with("compose   …"),
            "the fitted value leads with an ellipsis: {content}"
        );
        assert!(
            content.chars().count() <= 78,
            "the row fits the 78-column inner width: {content}"
        );
        assert!(
            content.ends_with("docker/docker-compose.yml"),
            "the path's discriminating tail survives: {content}"
        );
    }

    /// The Loading + error tri-states render.
    #[test]
    fn loading_and_error_states_render() {
        let state = AppState::new();
        let rows = rendered(&state);
        assert!(
            rows.join("\n").contains("Loading…"),
            "Loading before the first load"
        );

        let mut errored = AppState::new();
        errored.rig.status_error = Some("docker: command not found".into());
        let rows = rendered(&errored);
        let text = rows.join("\n");
        assert!(text.contains("status error"), "the error banner: {text}");
        assert!(
            text.contains("docker: command not found"),
            "the error message: {text}"
        );
    }

    /// The seeded logs pane renders its raw lines under the summary
    /// (the Streamed mapping's display proof).
    #[test]
    fn seeded_logs_pane_renders_raw_lines() {
        let mut state = AppState::new();
        state.rig.status = Some(status_fixture());
        state.rig.logs_on = true;
        for line in [
            "gateway starting",
            "SRConfig | loaded",
            " commissioning wizard ready",
        ] {
            state.rig.push_log_line(line.to_string());
        }
        let rows = rendered(&state);
        let text = rows.join("\n");

        assert!(text.contains("┌rig logs"), "the pane title: {text}");
        assert!(
            text.contains("gateway starting") && text.contains("commissioning wizard ready"),
            "raw compose lines render verbatim: {text}"
        );
        assert!(
            rows.iter().any(|row| row.contains("logs:3/10000")),
            "the status row names the ring fill: {text}"
        );
    }

    /// The ring cap + eviction accounting (the 06-03 twin): pushing
    /// past the cap keeps the newest lines and counts the evictions.
    #[test]
    fn rig_ring_caps_and_counts() {
        let mut rig = crate::state::RigData::default();
        for i in 0..(crate::state::RIG_LOG_RING_CAP + 5) {
            rig.push_log_line(format!("line-{i}"));
        }
        assert_eq!(rig.logs.len(), crate::state::RIG_LOG_RING_CAP);
        assert_eq!(rig.logs_dropped, 5);
        assert_eq!(
            rig.logs.front().map(String::as_str),
            Some("line-5"),
            "the oldest five evicted from the front"
        );
    }
}
