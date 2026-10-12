//! The JVM-profiling screen — the SPECIAL MODE's surface. Three
//! rows: the PERSISTENT red warning banner (it never scrolls away —
//! the whole reason the mode is special), the body (flight-recorder
//! status + the last action's outcome), and the status line (gate
//! state, in-flight action, key hints). PURE over [`AppState`] — no
//! I/O (the ui/ doctrine).

use ratatui::Frame;
use ratatui::layout::Constraint::{Length, Min, Ratio};
use ratatui::layout::{Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};

use crate::state::AppState;
use crate::ui::theme;

/// Render the special mode: banner + body + status line.
pub fn render(state: &AppState, frame: &mut Frame, area: Rect) {
    let [banner, body, status_line] = Layout::vertical([Length(6), Min(0), Length(1)]).areas(area);
    let [jfr_pane, result_pane] = Layout::horizontal([Ratio(1, 2), Ratio(1, 2)]).areas(body);

    render_banner(state, frame, banner);
    render_jfr_pane(state, frame, jfr_pane);
    render_result_pane(state, frame, result_pane);
    render_status_line(state, frame, status_line);
}

/// The persistent red banner — `theme::error` border AND text (red at
/// every color tier, BOLD at mono — the error slot's whole job). It
/// is a fixed layout row, so no scroll, action, or result can push
/// the caution off the screen while the mode is open.
fn render_banner(state: &AppState, frame: &mut Frame, area: Rect) {
    let palette = &state.palette;
    let block = Block::bordered()
        .title(" ⚠ SPECIAL MODE — JVM PROFILING — ACTIVE GATEWAY INSTRUMENTATION ⚠ ")
        .border_style(theme::error(palette))
        .title_style(theme::error(palette).add_modifier(Modifier::BOLD));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let text = Paragraph::new(Line::from(Span::styled(
        ignition_core::actions::jvm::PRODUCTION_WARNING,
        theme::error(palette).add_modifier(Modifier::BOLD),
    )))
    .wrap(Wrap { trim: false });
    frame.render_widget(text, inner);
}

/// The flight-recorder pane: whether OUR recording exists, the JDK's
/// verbatim jfrCheck text, or the poll's honest error (an undeployed
/// scriptExec route lands here — the screen still renders).
fn render_jfr_pane(state: &AppState, frame: &mut Frame, area: Rect) {
    let palette = &state.palette;
    let jvm = &state.jvm;
    let block = Block::bordered()
        .title(" flight recorder (jfr) ")
        .border_style(theme::border(palette))
        .title_style(theme::title(palette));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    match (jvm.jfr_active, jvm.jfr_error.as_deref()) {
        (_, Some(message)) => {
            lines.push(Line::from(Span::styled(
                "status poll failed",
                theme::error(palette),
            )));
            lines.extend(
                message
                    .lines()
                    .map(|line| Line::from(Span::styled(line.to_string(), theme::text(palette)))),
            );
            lines.push(Line::default());
            lines.push(Line::from(Span::styled(
                "needs the scriptExec route: ign webdev deploy --with-script-exec",
                theme::muted(palette),
            )));
        }
        (Some(active), None) => {
            let (word, style) = if active {
                (
                    "ACTIVE — fetch + clean up with F",
                    theme::error(palette).add_modifier(Modifier::BOLD),
                )
            } else {
                ("none", theme::success(palette))
            };
            lines.push(Line::from(vec![
                Span::styled("ign-cli recording: ", theme::muted(palette)),
                Span::styled(word, style),
            ]));
        }
        (None, None) => {
            lines.push(Line::from(Span::styled("Loading…", theme::text(palette))));
        }
    }
    if let Some(recordings) = &jvm.jfr_recordings {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            "jfrCheck (verbatim):",
            theme::muted(palette),
        )));
        lines.extend(
            recordings
                .lines()
                .map(|line| Line::from(Span::styled(line.to_string(), theme::text(palette)))),
        );
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

/// The last action's outcome pane (sample rollup / start terms /
/// stop fetch outcome / the action's error) — the Result_ modal's
/// verbatim-lines convention, in-screen with the same scroll keys.
fn render_result_pane(state: &AppState, frame: &mut Frame, area: Rect) {
    let palette = &state.palette;
    let jvm = &state.jvm;
    let title = match (&jvm.last_label, &jvm.in_flight) {
        (Some(label), _) => format!(" {label} "),
        (None, Some(label)) => format!(" running: {label} "),
        (None, None) => " result ".to_string(),
    };
    let block = Block::bordered()
        .title(title)
        .border_style(theme::border(palette))
        .title_style(theme::title(palette));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if jvm.last_lines.is_empty() {
        let hint = match jvm.in_flight {
            Some(label) => {
                format!("{label} in flight — a 30s sample holds the gateway for its whole window…")
            }
            None => "nothing run yet — s sample · f jfr start · F jfr stop+fetch".to_string(),
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(hint, theme::muted(palette)))),
            inner,
        );
        return;
    }
    let lines: Vec<Line> = jvm
        .last_lines
        .iter()
        .map(|line| Line::from(Span::styled(line.clone(), theme::text(palette))))
        .collect();
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((jvm.last_scroll, 0)),
        inner,
    );
}

/// The status line: profile, the gate state (always ACKNOWLEDGED —
/// the screen is unreachable otherwise, and the line SAYS so), the
/// in-flight action, and the key hints.
fn render_status_line(state: &AppState, frame: &mut Frame, area: Rect) {
    let palette = &state.palette;
    let mut spans = vec![
        Span::styled("profile: ", theme::muted(palette)),
        Span::styled(
            state.profile.clone().unwrap_or_else(|| "—".to_string()),
            theme::accent(palette),
        ),
        Span::styled("  ·  gate: ", theme::muted(palette)),
        Span::styled(
            if state.jvm.acknowledged {
                "ACKNOWLEDGED"
            } else {
                "closed"
            },
            theme::error(palette).add_modifier(Modifier::BOLD),
        ),
    ];
    if let Some(label) = state.jvm.in_flight {
        spans.push(Span::styled("  ·  ", theme::muted(palette)));
        spans.push(Span::styled(
            format!("running: {label}"),
            theme::warning(palette),
        ));
    }
    // The live watch gets its own loud marker (it is CONTINUOUS
    // overhead — the line must say so while it runs).
    if state.jvm.watch_shutdown.is_some() {
        spans.push(Span::styled("  ·  ", theme::muted(palette)));
        spans.push(Span::styled(
            "sampling LIVE (10s @ 50ms) · w stop",
            theme::error(palette).add_modifier(Modifier::BOLD),
        ));
    }
    spans.push(Span::styled(
        "  ·  s sample 30s · w watch live · f jfr start 60s · F jfr stop+fetch · r refresh · PgUp/PgDn scroll · Esc exit · q quit",
        theme::muted(palette),
    ));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}
