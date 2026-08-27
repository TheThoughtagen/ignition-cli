//! Tags screen (06-04): the k9s-style object browser — the provider
//! list (the native `tags provider list`, no deployed routes needed),
//! Enter-descend into each provider's tag tree via one-shot
//! `tags_browse` workers (an indented PATH list, one stack level per
//! folder — the CLI human browse's stacked twin), and the detail pane
//! with the on-demand `tags_read` value (quality strings are DATA,
//! never parsed into errors — the 05-04 convention).
//!
//! Render is PURE over [`AppState`] — navigation and spawns live in
//! [`crate::update`], gateway I/O in [`crate::workers::watch`].

use ratatui::Frame;
use ratatui::layout::Constraint::{Length, Min};
use ratatui::layout::{Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Cell, Paragraph, Row, Table};

use ignition_core::actions::tags::TagProviderRow;

use crate::state::{AppState, DetailRead};

/// Render the tags body: the browser pane + the one-row status line.
pub fn render(state: &AppState, frame: &mut Frame, area: Rect) {
    let [browser, status] = Layout::vertical([Min(0), Length(1)]).areas(area);
    render_browser(state, frame, browser);
    render_status(state, frame, status);
}

/// The browser pane: the open detail, the provider list, or the
/// current tree level (the stack's top) — whichever owns the surface.
fn render_browser(state: &AppState, frame: &mut Frame, area: Rect) {
    if state.tags.detail.is_some() {
        render_detail(state, frame, area);
    } else if state.tags.stack.is_empty() {
        render_providers(state, frame, area);
    } else {
        render_tree(state, frame, area);
    }
}

/// One provider row: name, enabled, tag count, health, managed flag.
fn provider_cells(provider: &TagProviderRow) -> Vec<Cell<'static>> {
    vec![
        Cell::from(if provider.managed {
            format!("{} (managed)", provider.name)
        } else {
            provider.name.clone()
        }),
        Cell::from(if provider.enabled { "yes" } else { "no" }),
        Cell::from(
            provider
                .tag_count
                .map(|count| count.to_string())
                .unwrap_or_else(|| "-".into()),
        ),
        Cell::from(provider.health.clone().unwrap_or_else(|| "-".into())),
    ]
}

/// The provider list level with its tri-state: Loading before the
/// first load, the honest error when it failed, the rows when they
/// landed — "no providers" (a state, not a crash) when empty.
fn render_providers(state: &AppState, frame: &mut Frame, area: Rect) {
    let block = Block::bordered().title("tags — providers");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let tags = &state.tags;
    match (&tags.providers, &tags.providers_error) {
        (None, None) => {
            frame.render_widget(Paragraph::new(Line::from("Loading…")), inner);
        }
        (None, Some(message)) => {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled(
                        "provider load error",
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
        (Some(rows), _) => {
            if rows.is_empty() {
                frame.render_widget(Paragraph::new(Line::from("no tag providers")), inner);
                return;
            }
            let header = Row::new(vec![
                Cell::from("provider"),
                Cell::from("enabled"),
                Cell::from("tags"),
                Cell::from("health"),
            ])
            .style(Style::default().add_modifier(Modifier::BOLD));
            let table_rows: Vec<Row> = rows
                .iter()
                .map(|row| Row::new(provider_cells(row)))
                .collect();
            let widths = [Min(10), Length(7), Length(5), Length(9)];
            let table = Table::new(table_rows, widths)
                .header(header)
                .row_highlight_style(Style::default().add_modifier(Modifier::BOLD));
            let mut table_state = state.tags.providers_table;
            frame.render_stateful_widget(table, inner, &mut table_state);
        }
    }
}

/// One tree row: the watch marker (06-04 Task 2), the depth indent,
/// the FULL bracketed path (the known-fullPath row contract — an
/// indented path list), folders slashed, then type + dataType.
fn tree_cells(depth: usize, row: &ignition_core::actions::tags::BrowseRow) -> Vec<Cell<'static>> {
    let slash = if row.has_children { "/" } else { "" };
    let indent = "  ".repeat(depth.saturating_sub(1));
    vec![
        Cell::from(format!("{indent}{}{slash}", row.path)),
        Cell::from(row.tag_type.clone()),
        Cell::from(row.data_type.clone().unwrap_or_else(|| "-".into())),
    ]
}

/// The current tree level (the stack's top): the browse path rides
/// the title, rows are the level's entries with the same tri-state.
fn render_tree(state: &AppState, frame: &mut Frame, area: Rect) {
    let Some(level) = state.tags.stack.last() else {
        return;
    };
    let block = Block::bordered().title(format!("tags — {}", level.path));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    match (&level.entries, &level.error) {
        (None, None) => {
            frame.render_widget(Paragraph::new(Line::from("Loading…")), inner);
        }
        (None, Some(message)) => {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled(
                        "browse error",
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
        (Some(rows), _) => {
            if rows.is_empty() {
                frame.render_widget(Paragraph::new(Line::from("(empty folder)")), inner);
                return;
            }
            let header = Row::new(vec![
                Cell::from("path"),
                Cell::from("type"),
                Cell::from("dataType"),
            ])
            .style(Style::default().add_modifier(Modifier::BOLD));
            let depth = state.tags.stack.len();
            let table_rows: Vec<Row> = rows
                .iter()
                .map(|row| Row::new(tree_cells(depth, row)))
                .collect();
            let widths = [Min(24), Length(11), Length(8)];
            let table = Table::new(table_rows, widths)
                .header(header)
                .row_highlight_style(Style::default().add_modifier(Modifier::BOLD));
            let mut table_state = state.tags.tree_table;
            frame.render_stateful_widget(table, inner, &mut table_state);
        }
    }
}

/// The detail pane: node info from the browse row plus the on-demand
/// read (value raw JSON, quality/timestamp verbatim — quality IS
/// data). Enter refires the read; Esc ascends back to the tree.
fn render_detail(state: &AppState, frame: &mut Frame, area: Rect) {
    let block = Block::bordered().title("tags — detail");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(detail) = &state.tags.detail else {
        return;
    };
    let mut lines = vec![
        Line::from(format!("path    {}", detail.path)),
        Line::from(format!("name    {}", detail.name)),
        Line::from(format!("type    {}", detail.tag_type)),
        Line::from(format!(
            "dtype   {}",
            detail.data_type.as_deref().unwrap_or("-")
        )),
        Line::default(),
    ];
    match &detail.read {
        DetailRead::Loading => lines.push(Line::from("value   Loading…")),
        DetailRead::Error(message) => {
            lines.push(Line::from(Span::styled(
                "read error",
                Style::default().add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(
                message
                    .chars()
                    .take(inner.width as usize - 1)
                    .collect::<String>(),
            ));
        }
        DetailRead::Loaded(row) => {
            lines.push(Line::from(format!("value   {}", row.value)));
            lines.push(Line::from(format!("quality {}", row.quality)));
            lines.push(Line::from(format!("time    {}", row.timestamp)));
        }
    }
    lines.push(Line::default());
    lines.push(Line::from("Enter re-read · Esc back"));
    frame.render_widget(Paragraph::new(lines), inner);
}

/// The status row: the current level, stack depth, the key hints.
fn render_status(state: &AppState, frame: &mut Frame, area: Rect) {
    let tags = &state.tags;
    let level = if tags.detail.is_some() {
        "detail".to_string()
    } else if let Some(top) = tags.stack.last() {
        top.path.clone()
    } else {
        "providers".to_string()
    };
    let text = format!(
        " {level} · depth {} · Enter open · Esc back",
        tags.stack.len()
    );
    frame.render_widget(Paragraph::new(Line::from(text)), area);
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::render;
    use crate::state::{AppState, BrowseLevel, DetailRead, TagsData, TagsDetail};

    fn provider(name: &str) -> ignition_core::actions::tags::TagProviderRow {
        ignition_core::actions::tags::TagProviderRow {
            name: name.to_string(),
            enabled: true,
            tag_count: Some(12),
            health: Some("OK".into()),
            managed: name == "System",
        }
    }

    fn browse_row(
        path: &str,
        name: &str,
        tag_type: &str,
        has_children: bool,
    ) -> ignition_core::actions::tags::BrowseRow {
        ignition_core::actions::tags::BrowseRow {
            path: path.to_string(),
            name: name.to_string(),
            tag_type: tag_type.to_string(),
            has_children,
            data_type: Some("Int4".into()),
        }
    }

    /// Render the Tags screen on an 80x24 TestBackend; joined rows.
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

    /// A Tags state seeded with two providers — the root-level render
    /// fixture.
    fn state_with_providers() -> AppState {
        let mut state = AppState::new();
        state.tags = TagsData {
            providers: Some(vec![provider("default"), provider("System")]),
            ..Default::default()
        };
        state
    }

    /// The provider table renders names (managed flagged), tag counts,
    /// health, and the pane title.
    #[test]
    fn provider_table_renders_rows() {
        let state = state_with_providers();
        let rows = rendered(&state);
        let text = rows.join("\n");

        assert!(
            rows.iter().any(|row| row.contains("tags — providers")),
            "pane title: {text}"
        );
        assert!(text.contains("default"), "provider name: {text}");
        assert!(text.contains("System (managed)"), "managed flag: {text}");
        assert!(text.contains("OK"), "health: {text}");
        assert!(text.contains("12"), "tag count: {text}");
    }

    /// The tri-states: Loading before the first load, the honest
    /// error after a failed one.
    #[test]
    fn provider_tri_states_render() {
        let loading = AppState::new();
        let rows = rendered(&loading);
        assert!(
            rows.iter().any(|row| row.contains("Loading")),
            "loading state: {rows:?}"
        );

        let mut errored = AppState::new();
        errored.tags.providers_error = Some("routes_not_deployed (exit 6)".into());
        let rows = rendered(&errored);
        let text = rows.join("\n");
        assert!(text.contains("provider load error"), "banner: {text}");
        assert!(text.contains("routes_not_deployed"), "detail: {text}");
    }

    /// THE known-fullPath row proof: a seeded tree level renders its
    /// entries' full bracketed paths (folders slashed), indented one
    /// depth, with the level path on the title.
    #[test]
    fn tree_level_renders_known_full_paths() {
        let mut state = AppState::new();
        state.tags.stack.push(BrowseLevel {
            path: "[default]P5".into(),
            entries: Some(vec![
                browse_row("[default]P5/T1", "T1", "AtomicTag", false),
                browse_row("[default]P5/Motor", "Motor", "UdtInstance", true),
            ]),
            error: None,
            selected: None,
        });
        state.tags.tree_table.select(Some(0));
        let rows = rendered(&state);
        let text = rows.join("\n");

        assert!(
            rows.iter().any(|row| row.contains("tags — [default]P5")),
            "level title carries the path: {text}"
        );
        assert!(
            text.contains("[default]P5/T1"),
            "the known fullPath row renders: {text}"
        );
        assert!(
            text.contains("[default]P5/Motor/"),
            "folders render slashed: {text}"
        );
        assert!(text.contains("AtomicTag"), "type column: {text}");
    }

    /// The detail pane: node info + the landed read (value/quality/
    /// timestamp verbatim) + the key hints.
    #[test]
    fn detail_pane_renders_info_and_read() {
        let mut state = AppState::new();
        state.tags.detail = Some(TagsDetail {
            path: "[default]P5/T1".into(),
            name: "T1".into(),
            tag_type: "AtomicTag".into(),
            data_type: Some("Int4".into()),
            read: DetailRead::Loaded(ignition_core::actions::tags::TagReadRow {
                path: "[default]P5/T1".into(),
                value: serde_json::json!(42),
                quality: "Good".into(),
                timestamp: "Mon Aug 24 00:00:00 UTC 2026".into(),
            }),
        });
        let rows = rendered(&state);
        let text = rows.join("\n");

        assert!(text.contains("[default]P5/T1"), "path: {text}");
        assert!(text.contains("AtomicTag"), "type: {text}");
        assert!(text.contains("value   42"), "value: {text}");
        assert!(text.contains("quality Good"), "quality verbatim: {text}");
        assert!(
            text.contains("Mon Aug 24 00:00:00 UTC 2026"),
            "timestamp verbatim: {text}"
        );
        assert!(text.contains("Enter re-read"), "hints: {text}");
    }

    /// The detail's Loading and Error states render honestly (the
    /// 06-02 pattern applied to the pane).
    #[test]
    fn detail_loading_and_error_states_render() {
        let mut state = AppState::new();
        state.tags.detail = Some(TagsDetail {
            path: "[default]T1".into(),
            name: "T1".into(),
            tag_type: "AtomicTag".into(),
            data_type: None,
            read: DetailRead::Loading,
        });
        let rows = rendered(&state);
        assert!(
            rows.iter().any(|row| row.contains("Loading")),
            "loading: {rows:?}"
        );

        state.tags.detail = Some(TagsDetail {
            path: "[default]T1".into(),
            name: "T1".into(),
            tag_type: "AtomicTag".into(),
            data_type: None,
            read: DetailRead::Error("routes_not_deployed (exit 6)".into()),
        });
        let rows = rendered(&state);
        let text = rows.join("\n");
        assert!(text.contains("read error"), "error banner: {text}");
        assert!(text.contains("routes_not_deployed"), "error detail: {text}");
    }
}
