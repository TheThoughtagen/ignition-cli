//! Projects screen (06-05): the object-list → detail browser — the
//! project table (name/title/enabled/parent), Enter into a project's
//! detail pane (the find's full record + the resources list, whose
//! export-zip surgery lives inside the actions layer — invisible
//! here), Enter again into a resource's detail (the flat
//! `{project, path, content_kind, content}` get with a SCROLLABLE
//! content preview; binary fencing rides the action's exit-6 refusal
//! surfaced verbatim). The action menus (project/resource/webdev
//! families) ride `a` — see [`crate::update`].
//!
//! Render is PURE over [`AppState`] — navigation and spawns live in
//! [`crate::update`], gateway I/O in [`crate::workers::ops`].

use ratatui::Frame;
use ratatui::layout::Constraint::{Length, Min, Percentage};
use ratatui::layout::{Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Cell, Paragraph, Row, Table};

use ignition_core::actions::projects::ProjectSummary;
use ignition_core::actions::resources::ResourceGetResult;
use ignition_core::client::projects::ProjectRecord;

use crate::state::{AppState, ProjectRecordState, ResourceDetail, ResourceGetState};

/// Render the projects body: whichever level owns the surface (the
/// resource preview, the project detail, or the list) + the one-row
/// status line.
pub fn render(state: &AppState, frame: &mut Frame, area: Rect) {
    let [body, status] = Layout::vertical([Min(0), Length(1)]).areas(area);
    if state.projects.resource.is_some() {
        render_resource(state, frame, body);
    } else if state.projects.detail.is_some() {
        render_detail(state, frame, body);
    } else {
        render_list(state, frame, body);
    }
    render_status(state, frame, status);
}

/// One project row: name, title, enabled, parent (the list shape's
/// "enabled-ish summary").
fn project_cells(project: &ProjectSummary) -> Vec<Cell<'static>> {
    vec![
        Cell::from(project.name.clone()),
        Cell::from(project.title.clone().unwrap_or_else(|| "-".into())),
        Cell::from(if project.enabled { "yes" } else { "no" }),
        Cell::from(project.parent.clone().unwrap_or_else(|| "-".into())),
    ]
}

/// The project list level with its tri-state: Loading before the
/// first load, the honest error when it failed, the rows when they
/// landed — "no projects" (a state, not a crash) when empty.
fn render_list(state: &AppState, frame: &mut Frame, area: Rect) {
    let block = Block::bordered().title("projects");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let projects = &state.projects;
    match (&projects.list, &projects.list_error) {
        (None, None) => {
            frame.render_widget(Paragraph::new(Line::from("Loading…")), inner);
        }
        (None, Some(message)) => {
            frame.render_widget(error_lines("list error", message, inner), inner);
        }
        (Some(rows), _) => {
            if rows.is_empty() {
                frame.render_widget(Paragraph::new(Line::from("no projects")), inner);
                return;
            }
            let header = Row::new(vec![
                Cell::from("name"),
                Cell::from("title"),
                Cell::from("enabled"),
                Cell::from("parent"),
            ])
            .style(Style::default().add_modifier(Modifier::BOLD));
            let table_rows: Vec<Row> = rows
                .iter()
                .map(|row| Row::new(project_cells(row)))
                .collect();
            let widths = [Min(12), Min(12), Length(7), Min(8)];
            let table = Table::new(table_rows, widths)
                .header(header)
                .row_highlight_style(Style::default().add_modifier(Modifier::BOLD));
            let mut table_state = state.projects.list_table;
            frame.render_stateful_widget(table, inner, &mut table_state);
        }
    }
}

/// The shared per-pane error shape (the tags screen's convention).
fn error_lines(banner: &str, message: &str, inner: Rect) -> Paragraph<'static> {
    Paragraph::new(vec![
        Line::from(Span::styled(
            banner.to_string(),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(
            message
                .chars()
                .take(inner.width as usize - 1)
                .collect::<String>(),
        ),
    ])
}

/// One record field line, `label value` with `-` for None.
fn field_line(label: &str, value: Option<&str>) -> Line<'static> {
    Line::from(format!("{label:<7} {}", value.unwrap_or("-")))
}

/// The project detail level: the find's full record (left) + the
/// resources list (right). Both halves carry their own tri-state —
/// one dead half never blanks the other (the per-panel degrade
/// convention).
fn render_detail(state: &AppState, frame: &mut Frame, area: Rect) {
    let Some(detail) = &state.projects.detail else {
        return;
    };
    let [record_pane, resources_pane] =
        Layout::horizontal([Percentage(45), Percentage(55)]).areas(area);

    // Left: the record.
    let block = Block::bordered().title(format!("project — {}", detail.name));
    let inner = block.inner(record_pane);
    frame.render_widget(block, record_pane);
    match &detail.record {
        ProjectRecordState::Loading => {
            frame.render_widget(Paragraph::new(Line::from("Loading…")), inner);
        }
        ProjectRecordState::Error(message) => {
            frame.render_widget(error_lines("find error", message, inner), inner);
        }
        ProjectRecordState::Loaded(record) => {
            frame.render_widget(Paragraph::new(record_lines(record)), inner);
        }
    }

    // Right: the resources list.
    let block = Block::bordered().title(format!("resources — {}", detail.name));
    let inner = block.inner(resources_pane);
    frame.render_widget(block, resources_pane);
    match (&detail.resources, &detail.resources_error) {
        (None, None) => {
            frame.render_widget(Paragraph::new(Line::from("Loading…")), inner);
        }
        (None, Some(message)) => {
            frame.render_widget(error_lines("resources error", message, inner), inner);
        }
        (Some(paths), _) => {
            if paths.is_empty() {
                frame.render_widget(Paragraph::new(Line::from("no resources")), inner);
                return;
            }
            let header = Row::new(vec![Cell::from("path")])
                .style(Style::default().add_modifier(Modifier::BOLD));
            let table_rows: Vec<Row> = paths
                .iter()
                .map(|path| Row::new(vec![Cell::from(path.clone())]))
                .collect();
            let table = Table::new(table_rows, [Min(20)])
                .header(header)
                .row_highlight_style(Style::default().add_modifier(Modifier::BOLD));
            let mut table_state = detail.resources_table;
            frame.render_stateful_widget(table, inner, &mut table_state);
        }
    }
}

/// The record's display lines: the six summary fields PLUS the
/// defaultDb/tagProvider/userSource passthrough the detail pane
/// uniquely shows.
fn record_lines(record: &ProjectRecord) -> Vec<Line<'static>> {
    vec![
        field_line("name", Some(&record.name)),
        field_line("title", record.title.as_deref()),
        field_line("desc", record.description.as_deref()),
        field_line("enabled", Some(if record.enabled { "yes" } else { "no" })),
        field_line("parent", record.parent.as_deref()),
        field_line(
            "inherit",
            record
                .inheritable
                .map(|flag| if flag { "yes" } else { "no" }),
        ),
        field_line("db", record.default_db.as_deref()),
        field_line("tagprov", record.tag_provider.as_deref()),
        field_line("usersrc", record.user_source.as_deref()),
    ]
}

/// The resource detail level: the flat get shape's identity fields +
/// the SCROLLABLE content preview (JSON pretty-printed, text raw —
/// the pure derivation lives on [`ResourceDetail::content_lines`]).
/// Up/Down scroll; Enter refires the get. Binary fencing (the
/// action's exit-6 `resource_binary` refusal) surfaces as the Error
/// state verbatim — never a blank pane.
fn render_resource(state: &AppState, frame: &mut Frame, area: Rect) {
    let Some(resource) = &state.projects.resource else {
        return;
    };
    let block =
        Block::bordered().title(format!("resource — {}/{}", resource.project, resource.path));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    match &resource.state {
        ResourceGetState::Loading => {
            frame.render_widget(Paragraph::new(Line::from("Loading…")), inner);
        }
        ResourceGetState::Error(message) => {
            frame.render_widget(error_lines("get error", message, inner), inner);
        }
        ResourceGetState::Loaded(result) => {
            let body_height = inner.height;
            let mut lines = vec![
                Line::from(format!("project {}", resource.project)),
                Line::from(format!("path    {}", resource.path)),
                Line::from(format!("kind    {}", result.content_kind)),
                Line::default(),
            ];
            lines.extend(content_preview_lines(result));
            lines.push(Line::default());
            lines.push(Line::from("↑↓ scroll · Enter re-get · Esc back"));
            // Clamp the scroll to the content (the state side only
            // advances; the truth of the height lives here).
            let max_scroll = lines
                .len()
                .saturating_sub(body_height as usize)
                .min(u16::MAX as usize) as u16;
            let scroll = resource.scroll.min(max_scroll);
            frame.render_widget(Paragraph::new(lines).scroll((scroll, 0)), inner);
        }
    }
}

/// The preview lines: text renders raw, anything else pretty-JSON
/// (the action's own kind classification IS the renderer's).
fn content_preview_lines(result: &ResourceGetResult) -> Vec<Line<'static>> {
    ResourceDetail::content_lines(result)
        .into_iter()
        .map(Line::from)
        .collect()
}

/// The status row: the current level + the key hints.
fn render_status(state: &AppState, frame: &mut Frame, area: Rect) {
    let level = if state.projects.resource.is_some() {
        "resource"
    } else if state.projects.detail.is_some() {
        "project"
    } else {
        "list"
    };
    let count = state.projects.list.as_ref().map_or(0, Vec::len);
    let text = format!(" {level} · {count} projects · Enter open · a actions · Esc back",);
    frame.render_widget(Paragraph::new(Line::from(text)), area);
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::render;
    use crate::state::{
        AppState, ProjectDetail, ProjectRecordState, ProjectsData, ResourceDetail, ResourceGetState,
    };
    use ignition_core::actions::projects::ProjectSummary;
    use ignition_core::actions::resources::ResourceGetResult;
    use ignition_core::client::projects::ProjectRecord;

    fn project(name: &str) -> ProjectSummary {
        ProjectSummary {
            name: name.to_string(),
            title: Some(format!("{name} title")),
            description: None,
            enabled: true,
            parent: Some("Base".into()),
            inheritable: Some(false),
        }
    }

    fn record(name: &str) -> ProjectRecord {
        ProjectRecord {
            name: name.to_string(),
            title: Some(format!("{name} title")),
            description: None,
            enabled: true,
            parent: Some("Base".into()),
            inheritable: Some(false),
            default_db: Some("SQLite".to_string()),
            tag_provider: Some("default".to_string()),
            user_source: Some("default".to_string()),
            extra: Default::default(),
        }
    }

    fn resource_get(kind: &str, content: serde_json::Value) -> ResourceGetResult {
        ResourceGetResult {
            project: "PlantFloor".into(),
            path: "views/root.json".into(),
            content_kind: kind.to_string(),
            content,
        }
    }

    /// Render the Projects screen on an 80x24 TestBackend; joined rows.
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

    /// The project table renders names, titles, enabled, parents, and
    /// the pane title.
    #[test]
    fn project_table_renders_rows() {
        let mut state = AppState::new();
        state.projects = ProjectsData {
            list: Some(vec![project("PlantFloor"), project("Base")]),
            ..Default::default()
        };
        let rows = rendered(&state);
        let text = rows.join("\n");

        assert!(
            rows.iter().any(|row| row.contains("┌projects")),
            "pane title: {text}"
        );
        assert!(text.contains("PlantFloor"), "name: {text}");
        assert!(text.contains("PlantFloor title"), "title: {text}");
        assert!(text.contains("Base"), "parent column: {text}");
        assert!(text.contains("yes"), "enabled column: {text}");
    }

    /// The tri-states: Loading before the first load, the honest
    /// error after a failed one.
    #[test]
    fn list_tri_states_render() {
        let loading = AppState::new();
        let rows = rendered(&loading);
        assert!(
            rows.iter().any(|row| row.contains("Loading")),
            "loading state: {rows:?}"
        );

        let mut errored = AppState::new();
        errored.projects.list_error = Some("gateway unreachable (exit 4)".into());
        let rows = rendered(&errored);
        let text = rows.join("\n");
        assert!(text.contains("list error"), "banner: {text}");
        assert!(text.contains("gateway unreachable"), "detail: {text}");
    }

    /// The detail level: the record pane's full field set (including
    /// the db/tagprov/usersrc passthrough only the detail shows) + the
    /// resources pane's paths.
    #[test]
    fn detail_renders_record_and_resources() {
        let mut state = AppState::new();
        state.projects = ProjectsData {
            detail: Some(ProjectDetail {
                name: "PlantFloor".into(),
                record: ProjectRecordState::Loaded(record("PlantFloor")),
                resources: Some(vec![
                    "views/root.json".into(),
                    "script-python/e2e/scratch".into(),
                ]),
                resources_error: None,
                resources_table: Default::default(),
            }),
            ..Default::default()
        };
        let rows = rendered(&state);
        let text = rows.join("\n");

        assert!(
            rows.iter().any(|row| row.contains("project — PlantFloor")),
            "record pane title: {text}"
        );
        assert!(text.contains("SQLite"), "defaultDb passthrough: {text}");
        assert!(text.contains("tagprov"), "tagProvider field: {text}");
        assert!(text.contains("usersrc"), "userSource field: {text}");
        assert!(
            rows.iter()
                .any(|row| row.contains("resources — PlantFloor")),
            "resources pane title: {text}"
        );
        assert!(text.contains("views/root.json"), "resource path: {text}");
        assert!(
            text.contains("script-python/e2e/scratch"),
            "second resource: {text}"
        );
    }

    /// The detail's per-half tri-states: a Loading record beside
    /// landed resources (and the reverse) never blanks the other
    /// half.
    #[test]
    fn detail_halves_degrade_independently() {
        let mut state = AppState::new();
        state.projects = ProjectsData {
            detail: Some(ProjectDetail {
                name: "PlantFloor".into(),
                record: ProjectRecordState::Loading,
                resources: Some(vec!["views/root.json".into()]),
                resources_error: None,
                resources_table: Default::default(),
            }),
            ..Default::default()
        };
        let rows = rendered(&state);
        let text = rows.join("\n");
        assert!(text.contains("Loading"), "record half Loading: {text}");
        assert!(
            text.contains("views/root.json"),
            "resources half landed: {text}"
        );

        state.projects.detail = Some(ProjectDetail {
            name: "PlantFloor".into(),
            record: ProjectRecordState::Loaded(record("PlantFloor")),
            resources: None,
            resources_error: Some("not_found (exit 6)".into()),
            resources_table: Default::default(),
        });
        let rows = rendered(&state);
        let text = rows.join("\n");
        assert!(text.contains("PlantFloor title"), "record half landed");
        assert!(
            text.contains("resources error") && text.contains("not_found"),
            "resources error verbatim: {text}"
        );
    }

    /// The resource detail: identity fields, the JSON pretty preview,
    /// and the scroll hints; the SCROLL unit (Up/Down move the line
    /// offset, verified through the state — render clamps only).
    #[test]
    fn resource_detail_renders_preview_and_scrolls() {
        let mut state = AppState::new();
        state.projects = ProjectsData {
            resource: Some(ResourceDetail {
                project: "PlantFloor".into(),
                path: "views/root.json".into(),
                state: ResourceGetState::Loaded(resource_get(
                    "json",
                    serde_json::json!({"views": {"root": {"meta": 1}}, "extra": [1, 2, 3]}),
                )),
                scroll: 0,
            }),
            ..Default::default()
        };
        let rows = rendered(&state);
        let text = rows.join("\n");
        assert!(
            rows.iter()
                .any(|row| row.contains("resource — PlantFloor/views/root.json")),
            "pane title carries project/path: {text}"
        );
        assert!(text.contains("kind    json"), "kind field: {text}");
        assert!(text.contains("\"views\""), "pretty JSON preview: {text}");
        assert!(text.contains("Enter re-get"), "hints: {text}");

        // THE scroll unit: the pure derivation produces one line per
        // pretty-JSON line, so the scroll offset moves through real
        // content.
        let detail = state.projects.resource.as_ref().expect("resource");
        if let ResourceGetState::Loaded(result) = &detail.state {
            let lines = crate::state::ResourceDetail::content_lines(result);
            assert!(lines.len() > 8, "pretty JSON is multi-line: {lines:?}");
        }

        // Text content renders RAW (not JSON-quoted).
        state.projects.resource = Some(ResourceDetail {
            project: "PlantFloor".into(),
            path: "script-python/e2e/scratch".into(),
            state: ResourceGetState::Loaded(resource_get(
                "text",
                serde_json::Value::String("print 'hello'\nprint 'world'".into()),
            )),
            scroll: 0,
        });
        let rows = rendered(&state);
        let text = rows.join("\n");
        assert!(
            text.contains("print 'hello'"),
            "text preview is raw, not JSON-escaped: {text}"
        );
    }

    /// The resource detail's Loading and Error states render honestly
    /// (the binary fence — the action's exit-6 — surfaces verbatim).
    #[test]
    fn resource_loading_and_binary_error_states_render() {
        let mut state = AppState::new();
        state.projects.resource = Some(ResourceDetail {
            project: "PlantFloor".into(),
            path: "data.bin".into(),
            state: ResourceGetState::Loading,
            scroll: 0,
        });
        let rows = rendered(&state);
        assert!(
            rows.iter().any(|row| row.contains("Loading")),
            "loading: {rows:?}"
        );

        state.projects.resource = Some(ResourceDetail {
            project: "PlantFloor".into(),
            path: "comms/drivers.bin".into(),
            state: ResourceGetState::Error(
                "resource_binary: resource comms/drivers.bin is binary (exit 6)".into(),
            ),
            scroll: 0,
        });
        let rows = rendered(&state);
        let text = rows.join("\n");
        assert!(text.contains("get error"), "error banner: {text}");
        assert!(
            text.contains("binary"),
            "the exit-6 binary fence surfaces verbatim: {text}"
        );
    }
}
