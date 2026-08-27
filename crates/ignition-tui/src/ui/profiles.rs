//! Profile switcher modals (06-02): the list (active marked, moving
//! selection) + the two-field add form. Rendering lives HERE (the
//! screen-owned module); `ui::render_modal` delegates the two variants.
//!
//! Auth refs stay on the CLI form per the LOCKED modal-depth decision —
//! the add form carries the common fields (name, url) plus a hint line.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph};

use crate::state::Modal;

/// Render the switcher list modal inside `area` (caller centered +
/// cleared it): every profile name, the active one bolded + marked,
/// the cursor arrowed, key hints at the bottom.
pub fn render_profiles(modal: &Modal, frame: &mut Frame, area: Rect) {
    let Modal::Profiles {
        names,
        active,
        selected,
    } = modal
    else {
        return;
    };
    let mut lines: Vec<Line> = names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let marker = if index == *selected { "▸ " } else { "  " };
            if Some(name) == active.as_ref() {
                Line::from(format!("{marker}{name} · active"))
                    .style(Style::default().add_modifier(Modifier::BOLD))
            } else {
                Line::from(format!("{marker}{name}"))
            }
        })
        .collect();
    if names.is_empty() {
        lines.push(Line::from("no profiles configured"));
    }
    lines.push(Line::default());
    lines.push(Line::from("Enter switch · a add · Esc close"));
    lines.push(Line::from("auth refs: `ign profile add --help`"));
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title("profiles")),
        area,
    );
}

/// Render the add form: name + url fields, the edited one arrowed,
/// Tab toggles, Enter submits.
pub fn render_add(modal: &Modal, frame: &mut Frame, area: Rect) {
    let Modal::ProfileAdd { name, url, field } = modal else {
        return;
    };
    let cursor = |active: bool| if active { "▸ " } else { "  " };
    let lines = vec![
        Line::from(format!(
            "{}name  {name}{}",
            cursor(*field == 0),
            if *field == 0 { "▏" } else { "" }
        )),
        Line::from(format!(
            "{}url   {url}{}",
            cursor(*field == 1),
            if *field == 1 { "▏" } else { "" }
        )),
        Line::default(),
        Line::from("Tab next field · Enter add · Esc cancel"),
        Line::from("auth refs (token/keyring/basic): `ign profile add --help`"),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title("profile add")),
        area,
    );
}

/// The centered geometry both modals share (the chrome's Ratio(1,2)
/// look — this module owns its heights: the list grows with the
/// profile count, the form is fixed).
pub fn modal_area(modal: &Modal, frame: &Rect) -> Rect {
    let height = match modal {
        Modal::Profiles { names, .. } => names.len().saturating_add(5).min(13),
        Modal::ProfileAdd { .. } => 8,
        _ => 5,
    } as u16;
    frame.centered(
        ratatui::layout::Constraint::Ratio(1, 2),
        ratatui::layout::Constraint::Length(height.max(5)),
    )
}

/// Overlay entry: clear + render (mirrors ui::render_modal's shape for
/// the two variants it delegates here).
pub fn render_overlay(modal: &Modal, frame: &mut Frame) {
    let area = modal_area(modal, &frame.area());
    frame.render_widget(Clear, area);
    match modal {
        Modal::Profiles { .. } => render_profiles(modal, frame, area),
        Modal::ProfileAdd { .. } => render_add(modal, frame, area),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::render_overlay;
    use crate::state::Modal;

    /// Render a modal on an 80x24 TestBackend; joined buffer text.
    fn rendered(modal: Modal) -> String {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("test terminal");
        terminal
            .draw(|frame| render_overlay(&modal, frame))
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

    /// The switcher lists every profile, bolds + marks the active one,
    /// arrows the cursor, and shows the CLI hint for auth refs.
    #[test]
    fn profiles_modal_lists_marks_and_hints() {
        let modal = Modal::Profiles {
            names: vec!["dev".into(), "prod".into()],
            active: Some("dev".into()),
            selected: 0,
        };
        let text = rendered(modal);

        assert!(text.contains("┌profiles"), "bordered title: {text}");
        assert!(
            text.contains("▸ dev · active"),
            "active marked + cursor: {text}"
        );
        assert!(text.contains("  prod"), "other row unmarked: {text}");
        assert!(
            text.contains("ign profile add --help"),
            "auth-refs hint: {text}"
        );
    }

    /// The add form shows both fields with the cursor on the edited
    /// one and the Tab/Enter hints.
    #[test]
    fn add_form_renders_fields_and_hints() {
        let modal = Modal::ProfileAdd {
            name: "stage".into(),
            url: String::new(),
            field: 1,
        };
        let text = rendered(modal);

        assert!(text.contains("┌profile add"), "title: {text}");
        assert!(text.contains("name  stage"), "name field: {text}");
        assert!(text.contains("▸ url"), "cursor on url (field 1): {text}");
        assert!(text.contains("Tab next field"), "hints: {text}");
    }
}
