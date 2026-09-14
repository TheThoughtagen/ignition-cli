//! Profile switcher modals (06-02): the list (active marked, moving
//! selection) + the two-field add form. Rendering lives HERE (the
//! screen-owned module); `ui::render_modal` delegates the two variants.
//!
//! Auth refs stay on the CLI form per the LOCKED modal-depth decision —
//! the add form carries the common fields (name, url) plus a hint line.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use crate::state::Modal;

use super::theme;

/// Render the switcher list modal inside `area` (caller centered +
/// cleared it): every profile name, the active one bolded + marked,
/// the cursor arrowed, key hints at the bottom. Names ride `text`,
/// hints `muted` (12-04 round 2); the active row keeps BOLD.
pub fn render_profiles(palette: &theme::Palette, modal: &Modal, frame: &mut Frame, area: Rect) {
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
            let style = theme::text(palette);
            if Some(name) == active.as_ref() {
                Line::from(format!("{marker}{name} · active"))
                    .style(style.add_modifier(Modifier::BOLD))
            } else {
                Line::from(format!("{marker}{name}")).style(style)
            }
        })
        .collect();
    if names.is_empty() {
        lines.push(Line::from(Span::styled(
            "no profiles configured",
            theme::text(palette),
        )));
    }
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        "Enter switch · a add · Esc close",
        theme::muted(palette),
    )));
    lines.push(Line::from(Span::styled(
        "auth refs: `ign profile add --help`",
        theme::muted(palette),
    )));
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .title("profiles")
                .border_style(theme::border(palette))
                .title_style(theme::title(palette)),
        ),
        area,
    );
}

/// Render the add form: name + url fields, the edited one arrowed,
/// Tab toggles, Enter submits. Field values ride `text`, the field
/// labels + hints ride `muted`.
pub fn render_add(palette: &theme::Palette, modal: &Modal, frame: &mut Frame, area: Rect) {
    let Modal::ProfileAdd { name, url, field } = modal else {
        return;
    };
    let cursor = |active: bool| if active { "▸ " } else { "  " };
    let field_line = |marker_label: String, value: String| {
        Line::from(vec![
            Span::styled(marker_label, theme::muted(palette)),
            Span::styled(value, theme::text(palette)),
        ])
    };
    let lines = vec![
        field_line(
            format!("{}name  ", cursor(*field == 0)),
            format!("{name}{}", if *field == 0 { "▏" } else { "" }),
        ),
        field_line(
            format!("{}url   ", cursor(*field == 1)),
            format!("{url}{}", if *field == 1 { "▏" } else { "" }),
        ),
        Line::default(),
        Line::from(Span::styled(
            "Tab next field · Enter add · Esc cancel",
            theme::muted(palette),
        )),
        Line::from(Span::styled(
            "auth refs (token/keyring/basic): `ign profile add --help`",
            theme::muted(palette),
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .title("profile add")
                .border_style(theme::border(palette))
                .title_style(theme::title(palette)),
        ),
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
/// the two variants it delegates here). The palette rides through —
/// the modal's chrome + content are themed like every other surface.
pub fn render_overlay(palette: &theme::Palette, modal: &Modal, frame: &mut Frame) {
    let area = modal_area(modal, &frame.area());
    frame.render_widget(Clear, area);
    match modal {
        Modal::Profiles { .. } => render_profiles(palette, modal, frame, area),
        Modal::ProfileAdd { .. } => render_add(palette, modal, frame, area),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::render_overlay;
    use crate::state::Modal;
    use crate::ui::theme::{Theme, Tier};

    /// Render a modal on an 80x24 TestBackend; joined buffer text.
    /// Runs the dark @ C16 palette — the tests below pin TEXT content,
    /// so any tier works; this one keeps the render-site call honest.
    fn rendered(modal: Modal) -> String {
        let palette = Theme::by_name("dark")
            .expect("dark in registry")
            .resolve(Tier::C16);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("test terminal");
        terminal
            .draw(|frame| render_overlay(&palette, &modal, frame))
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
