//! The itop renderer — a pure function of [`TopState`] onto one frame
//! (the cockpit's render discipline): htop's anatomy over gateway
//! vitals. Header gauges (cpu/heap), one identity line, sparkline
//! history, the live-entity table, and a two-line footer. Every color
//! names a Palette SLOT (the theme contract — never a Color literal).

use std::collections::VecDeque;

use ratatui::Frame;
use ratatui::layout::Constraint::{Fill, Length, Min, Ratio};
use ratatui::layout::{Layout, Rect};
use ratatui::style::Style;
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Clear, Gauge, Paragraph, Row as TRow, Sparkline, Table, Wrap,
};

use crate::itop::state::{
    FAILING_STATES, Family, HEALTHY_STATES, Modal, Row, TopState, human_bytes, human_ms,
};
use crate::ui::theme::{self, Palette};

/// Render the whole itop frame.
pub fn render(state: &TopState, frame: &mut Frame) {
    let [gauges, identity, threads, history, table, footer] = Layout::vertical([
        Length(3), // the two gauge bars
        Length(1), // the identity line
        Length(3), // the thread-state diagnostic bars
        Length(4), // the sparkline trio (cpu / heap / non-heap)
        Min(0),    // the entity table
        Length(2), // status line + key hints
    ])
    .areas(frame.area());

    render_gauges(state, frame, gauges);
    render_identity(state, frame, identity);
    render_threads(state, frame, threads);
    render_history(state, frame, history);
    render_table(state, frame, table);
    render_footer(state, frame, footer);

    if let Some(modal) = &state.modal {
        match modal {
            Modal::Help => render_help(state, frame),
            Modal::Filter { buffer } => render_filter(state, frame, buffer),
            Modal::ConfirmKill { target } => render_confirm_kill(state, frame, target),
            Modal::ScriptProbe { result } => render_script_probe(state, frame, result),
            Modal::ThreadStack {
                name,
                lines,
                scroll,
            } => render_thread_stack(state, frame, name, lines, *scroll),
        }
    }
}

/// The header gauges: CPU percent (the gauges endpoint's PERCENT
/// scale) and heap used/max — one bar each, thresholds colored from
/// the palette's status slots (success below 60, warning below 85,
/// error at and above).
fn render_gauges(state: &TopState, frame: &mut Frame, area: Rect) {
    let [cpu_area, heap_area] = Layout::horizontal([Fill(1), Fill(1)]).areas(area);
    let palette = &state.palette;

    let (cpu, heap, max) = state.vitals().unwrap_or((0.0, 0.0, 0.0));

    let cpu_style = threshold_style(palette, cpu);
    let cpu_gauge = Gauge::default()
        .block(
            Block::bordered()
                .title("cpu")
                .border_type(BorderType::Rounded)
                .border_style(theme::border(palette))
                .title_style(theme::title(palette)),
        )
        .gauge_style(cpu_style)
        .label(Span::styled(format!("{cpu:.1}%"), theme::emphasis(palette)))
        .ratio((cpu / 100.0).clamp(0.0, 1.0));
    frame.render_widget(cpu_gauge, cpu_area);

    let heap_ratio = if max > 0.0 { heap / max } else { 0.0 };
    let heap_pct = heap_ratio * 100.0;
    let heap_style = threshold_style(palette, heap_pct);
    let heap_gauge = Gauge::default()
        .block(
            Block::bordered()
                .title("heap")
                .border_type(BorderType::Rounded)
                .border_style(theme::border(palette))
                .title_style(theme::title(palette)),
        )
        .gauge_style(heap_style)
        .label(Span::styled(
            format!("{} / {}", human_bytes(heap), human_bytes(max)),
            theme::emphasis(palette),
        ))
        .ratio(heap_ratio.clamp(0.0, 1.0));
    frame.render_widget(heap_gauge, heap_area);
}

/// The threshold coloring shared by both bars — htop's meter colors,
/// mapped onto the palette's three status slots. At Mono every slot
/// is Reset, so the thresholds degrade to plain bars (the authored
/// mono guarantee).
fn threshold_style(palette: &Palette, percent: f64) -> Style {
    if percent >= 85.0 {
        theme::error(palette)
    } else if percent >= 60.0 {
        theme::warning(palette)
    } else {
        theme::success(palette)
    }
}

/// The thread-state diagnostic row — four JMX-mix bars (running /
/// waiting / timed-waiting / blocked) over the sample's TOTAL thread
/// count, the execution-state mix the Performance page draws. The
/// blocked bar is the honest alarm: any blocked thread colors the
/// whole bar `error` (a blocked gateway thread is never noise).
fn render_threads(state: &TopState, frame: &mut Frame, area: Rect) {
    let palette = &state.palette;
    let Some(metrics) = state.last.as_deref().and_then(|s| s.metrics.as_ref()) else {
        let block = Block::bordered()
            .title("threads")
            .border_type(BorderType::Rounded)
            .border_style(theme::border(palette))
            .title_style(theme::title(palette));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let note = match state
            .last
            .as_deref()
            .and_then(|s| s.metrics_error.as_deref())
        {
            Some(err) => Span::styled(
                err.chars()
                    .take(inner.width.saturating_sub(1) as usize)
                    .collect::<String>(),
                theme::error(palette),
            ),
            None => Span::styled("sampling…", theme::muted(palette)),
        };
        frame.render_widget(Paragraph::new(Line::from(note)), inner);
        return;
    };
    let t = &metrics.threads;
    let total = (t.running.max(0) + t.waiting.max(0) + t.timed_waiting.max(0) + t.blocked.max(0))
        .max(1) as f64;
    let slots = [
        ("running", t.running.max(0) as f64, theme::success(palette)),
        ("waiting", t.waiting.max(0) as f64, theme::accent(palette)),
        (
            "timed-waiting",
            t.timed_waiting.max(0) as f64,
            theme::warning(palette),
        ),
        ("blocked", t.blocked.max(0) as f64, theme::error(palette)),
    ];
    let [r, w, tw, b] =
        Layout::horizontal([Ratio(1, 4), Ratio(1, 4), Ratio(1, 4), Ratio(1, 4)]).areas(area);
    for (slot, area) in slots.iter().zip([r, w, tw, b]) {
        let (label, value, style) = slot;
        let pct = value / total;
        let gauge = Gauge::default()
            .block(
                Block::bordered()
                    .title(*label)
                    .border_type(BorderType::Rounded)
                    .border_style(theme::border(palette))
                    .title_style(theme::title(palette)),
            )
            .gauge_style(*style)
            .label(Span::styled(
                format!("{} · {:.0}%", value, pct * 100.0),
                theme::emphasis(palette),
            ))
            .ratio(pct.clamp(0.0, 1.0));
        frame.render_widget(gauge, area);
    }
}

/// The identity line: gateway name/version, RUNNING state, uptime,
/// thread counts, disk fill, license (with the trial countdown when
/// present). Labels ride `muted`, values ride `text`.
fn render_identity(state: &TopState, frame: &mut Frame, area: Rect) {
    let palette = &state.palette;
    let field = |label: &str, value: String| {
        vec![
            Span::styled(format!("{label} "), theme::muted(palette)),
            Span::styled(value, theme::text(palette)),
        ]
    };
    let mut spans: Vec<Span> = Vec::new();

    if let Some(status) = state.last.as_deref().and_then(|s| s.status.as_ref()) {
        spans.extend(field(
            "gateway",
            format!(
                "{} {}",
                status.gateway.name.as_deref().unwrap_or("unnamed"),
                status.gateway.ignition_version,
            ),
        ));
        spans.push(Span::styled(" · ", theme::muted(palette)));
        let state_span_style = if status.state.eq_ignore_ascii_case("running") {
            theme::success(palette)
        } else {
            theme::warning(palette)
        };
        spans.push(Span::styled("state ", theme::muted(palette)));
        spans.push(Span::styled(status.state.clone(), state_span_style));
        spans.push(Span::styled(" · ", theme::muted(palette)));
        spans.extend(field("uptime", human_ms(status.overview.uptime_ms)));
        spans.push(Span::styled(" · ", theme::muted(palette)));
        if let Some(metrics) = state.last.as_deref().and_then(|s| s.metrics.as_ref()) {
            let t = &metrics.threads;
            spans.extend(field(
                "threads",
                format!(
                    "R{} W{} T{} B{}",
                    t.running, t.waiting, t.timed_waiting, t.blocked
                ),
            ));
            spans.push(Span::styled(" · ", theme::muted(palette)));
        }
        if let Some(disk) = &status.overview.disk {
            let pct = if disk.total > 0 {
                (disk.used as f64 / disk.total as f64) * 100.0
            } else {
                0.0
            };
            spans.extend(field("disk", format!("{pct:.0}%")));
            spans.push(Span::styled(" · ", theme::muted(palette)));
        }
        if let Some(license) = &status.overview.license {
            let value = match license.trial_remaining_s {
                Some(secs) => format!("{} ({} left)", license.state, human_ms(secs * 1000)),
                None => license.state.clone(),
            };
            spans.extend(field("license", value));
        }
    } else if let Some(err) = state
        .last
        .as_deref()
        .and_then(|s| s.status_error.as_deref())
    {
        spans.push(Span::styled("gateway ", theme::muted(palette)));
        spans.push(Span::styled(
            err.chars()
                .take(area.width.saturating_sub(9) as usize)
                .collect::<String>(),
            theme::error(palette),
        ));
    } else {
        spans.push(Span::styled("sampling…", theme::muted(palette)));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// The sparkline pair: CPU percent (×10 — one decimal of resolution
/// inside Sparkline's u64 domain) and heap MiB. A ring gap (a
/// degraded section) simply leaves the sparkline shorter — honest.
fn render_history(state: &TopState, frame: &mut Frame, area: Rect) {
    let palette = &state.palette;
    let [cpu_area, heap_area, non_heap_area] =
        Layout::horizontal([Fill(1), Fill(1), Fill(1)]).areas(area);
    // `title.to_string()` detaches the Block from the borrowed title —
    // two call sites with differently-lived titles (one &'static, one
    // format!) share this helper, so it OWNS its title text.
    let block = |title: &str| -> Block<'static> {
        Block::bordered()
            .title(title.to_string())
            .border_type(BorderType::Rounded)
            .border_style(theme::border(palette))
            .title_style(theme::title(palette))
    };

    let mib = 1024.0 * 1024.0;
    let to_values = |ring: &VecDeque<f64>, scale: f64| -> Vec<u64> {
        ring.iter().map(|v| (v * scale).round() as u64).collect()
    };
    let cpu_values = to_values(&state.cpu_ring, 10.0);
    let heap_values = to_values(&state.heap_ring, 1.0 / mib);
    let non_heap_values = to_values(&state.nonheap_ring, 1.0 / mib);

    let cpu_title = format!("cpu % history ({} samples)", cpu_values.len());
    frame.render_widget(
        Sparkline::default()
            .block(block(&cpu_title))
            .style(theme::success(palette))
            .data(&cpu_values)
            .bar_set(symbols::bar::THREE_LEVELS),
        cpu_area,
    );
    frame.render_widget(
        Sparkline::default()
            .block(block(&format!(
                "heap MiB history ({} samples)",
                heap_values.len()
            )))
            .style(theme::accent(palette))
            .data(&heap_values)
            .bar_set(symbols::bar::THREE_LEVELS),
        heap_area,
    );
    frame.render_widget(
        Sparkline::default()
            .block(block(&format!(
                "non-heap MiB history ({} samples)",
                non_heap_values.len()
            )))
            .style(theme::warning(palette))
            .data(&non_heap_values)
            .bar_set(symbols::bar::THREE_LEVELS),
        non_heap_area,
    );
}

/// The entity table — the itop "process list". Column widths are
/// authored: KIND pinned (labels are short), STATE pinned, AGE pinned
/// (the condensed `2d 03h` form), NAME gets the Fill share, DETAIL
/// the rest.
fn render_table(state: &TopState, frame: &mut Frame, area: Rect) {
    let palette = &state.palette;
    let rows = state.visible_rows();

    // The threads view's title carries the dump's own truth: age,
    // deadlock banner, and busy state (the web UI's Diagnostics→
    // Threads header facts).
    let threads_note = if state.family == Family::Threads {
        match (&state.thread_dump, state.threads_busy) {
            (Some(snapshot), _) => {
                let age = std::time::Instant::now().saturating_duration_since(snapshot.at);
                let mut note = format!(" · dump {}s ago", age.as_secs());
                if let Some(ids) = &snapshot.deadlocked {
                    if ids.is_empty() {
                        note.push_str(" · no deadlocks");
                    } else {
                        note.push_str(&format!(" · ⚠ {} DEADLOCKED", ids.len()));
                    }
                }
                if let Some(err) = &snapshot.deadlocks_error {
                    note.push_str(&format!(" · deadlocks check failed: {err}"));
                }
                note
            }
            (None, true) => " · dumping threads…".to_string(),
            (None, false) => " · no dump yet — 6 fetches it".to_string(),
        }
    } else {
        String::new()
    };

    let title = match &state.filter {
        Some(filter) => format!(
            "entities — {}{threads_note} — /{filter}",
            state.family.label(),
        ),
        None => format!("entities — {}{threads_note}", state.family.label()),
    };
    let block = Block::bordered()
        .title(title)
        .border_type(BorderType::Rounded)
        .border_style(theme::border(palette))
        .title_style(theme::title(palette));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.last.is_none() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!("sampling… first sample within {}s", state.interval_secs),
                theme::muted(palette),
            )),
            inner,
        );
        return;
    }

    if rows.is_empty() {
        let why = if state.family == Family::Threads {
            // The threads view's honest empty ladder: busy → failed →
            // un-fetched → filtered-to-nothing.
            match &state.thread_dump {
                None => {
                    if state.threads_busy {
                        "dumping threads…".to_string()
                    } else {
                        "no thread dump yet — 6 fetches it".to_string()
                    }
                }
                Some(snapshot) => match (&snapshot.dump, &snapshot.dump_error) {
                    (None, Some(err)) => format!("thread dump failed: {err}"),
                    (None, None) => "thread dump pending…".to_string(),
                    (Some(_), _) => "no threads match the filter — Esc clears it".to_string(),
                },
            }
        } else if state.filter.is_some() {
            "no entities match the filter — Esc clears it".to_string()
        } else {
            "no entities in this view — 1 shows all".to_string()
        };
        frame.render_widget(
            Paragraph::new(Span::styled(why, theme::muted(palette))),
            inner,
        );
        return;
    }

    let header =
        TRow::new(["KIND", "NAME", "STATE", "CPU", "AGE", "DETAIL"]).style(theme::header(palette));
    let body: Vec<TRow> = rows.iter().map(|row| row_to_trow(row, palette)).collect();

    let mut table_state =
        ratatui::widgets::TableState::default().with_selected(Some(state.selected));
    let table = Table::new(
        body,
        [
            Length(10), // KIND
            Fill(1),    // NAME
            Length(13), // STATE
            Length(11), // CPU · tid (thread rows; `—` elsewhere)
            Length(9),  // AGE
            Fill(2),    // DETAIL
        ],
    )
    .header(header)
    .row_highlight_style(theme::selection(palette))
    .highlight_symbol("▸ ");
    frame.render_stateful_widget(table, inner, &mut table_state);
}

/// One table row, state-colored: the STATE span picks success/error/
/// warning by the honest-state word lists; a deadlocked thread's row
/// carries the ⚠ marker in the error slot; the CPU cell thresholds
/// like the header gauges. Everything else rides text/muted.
fn row_to_trow(row: &Row, palette: &Palette) -> TRow<'static> {
    let state_style = if row.deadlocked {
        // The JVM's deadlock detection outranks the word lists —
        // the alarm IS the state.
        theme::error(palette)
    } else if HEALTHY_STATES.contains(&row.state.to_lowercase().as_str()) {
        theme::success(palette)
    } else if FAILING_STATES.contains(&row.state.to_lowercase().as_str()) {
        theme::error(palette)
    } else {
        theme::warning(palette)
    };
    let state_text = if row.deadlocked {
        format!("⚠ {}", row.state)
    } else {
        row.state.clone()
    };
    let age = row.age_ms.map(human_ms).unwrap_or_else(|| "—".into());
    // The CPU cell carries the tid too (the web UI's Thread ID column,
    // folded in — a full column would starve NAME/DETAIL at 80 cols).
    let cpu = match (row.cpu, row.tid) {
        (Some(cpu), Some(tid)) => {
            Span::styled(format!("{cpu:.1}·{tid}"), threshold_style(palette, cpu))
        }
        (Some(cpu), None) => Span::styled(format!("{cpu:.1}"), threshold_style(palette, cpu)),
        (None, Some(tid)) => Span::styled(format!("—·{tid}"), theme::muted(palette)),
        (None, None) => Span::styled("—".to_string(), theme::muted(palette)),
    };
    TRow::new([
        Span::styled(row.kind.label().to_string(), theme::muted(palette)),
        Span::styled(row.name.clone(), theme::text(palette)),
        Span::styled(state_text, state_style),
        cpu,
        Span::styled(age, theme::text(palette)),
        Span::styled(row.detail.clone(), theme::muted(palette)),
    ])
}

/// The footer: line 1 is the live error readout (per-section errors
/// from the latest sample) or the transient status message; line 2 is
/// the state + key hints.
fn render_footer(state: &TopState, frame: &mut Frame, area: Rect) {
    let palette = &state.palette;
    let [status_line, keys_line] = Layout::vertical([Length(1), Length(1)]).areas(area);

    // Line 1: op status message wins; otherwise the section errors;
    // otherwise a quiet confirmation the world is green.
    let line1 = if let Some((msg, is_error)) = &state.status_msg {
        Line::from(Span::styled(
            msg.clone(),
            if *is_error {
                theme::error(palette)
            } else {
                theme::success(palette)
            },
        ))
    } else if let Some(sample) = state.last.as_deref() {
        let errors = section_errors(sample);
        if errors.is_empty() {
            Line::from(Span::styled("all sections healthy", theme::muted(palette)))
        } else {
            Line::from(Span::styled(errors, theme::error(palette)))
        }
    } else {
        Line::from(Span::styled("sampling…", theme::muted(palette)))
    };
    frame.render_widget(
        Paragraph::new(line1).wrap(Wrap { trim: false }),
        status_line,
    );

    // Line 2: cadence + pause + staleness + view state + hints.
    let now = std::time::Instant::now();
    let stale = state.staleness(now);
    let mode = if state.paused {
        Span::styled("PAUSED", theme::warning(palette))
    } else {
        Span::styled("live", theme::success(palette))
    };
    let age_part = match stale {
        Some(age) => {
            let budget = state.interval_secs * 3;
            let style = if age.as_secs() > budget {
                theme::warning(palette)
            } else {
                theme::muted(palette)
            };
            Span::styled(format!(" · sample {}s ago", age.as_secs()), style)
        }
        None => Span::styled(" · no sample yet", theme::muted(palette)),
    };
    let sort_dir = if state.sort_rev { "↓" } else { "↑" };
    let filter_part = match &state.filter {
        Some(filter) => format!(" · /{filter}"),
        None => String::new(),
    };
    // The threads view swaps its family-specific gestures into the
    // hint line (the web page's click affordances, keyed).
    let hints = if state.family == Family::Threads {
        "  │  q quit · space pause · +/- interval · r refresh · s sort · 1-6 view · ⏎ stack · 6 re-dump · x kill · ? help"
    } else {
        "  │  q quit · space pause · +/- interval · r refresh · e probe · s sort · S reverse · / filter · 1-6 view · x kill · ? help"
    };
    let line2 = Line::from(vec![
        // The profile name leads — the monitor is profile-fixed at
        // launch, so the name IS the target identity on screen.
        Span::styled(
            format!(
                "{} · {}s ",
                state.profile_name.as_deref().unwrap_or("—"),
                state.interval_secs
            ),
            theme::muted(palette),
        ),
        mode,
        age_part,
        Span::styled(
            format!(
                " · view {} · sort {}{sort_dir}{filter_part}",
                state.family.label(),
                state.sort_key.label(),
            ),
            theme::muted(palette),
        ),
        Span::styled(hints, theme::muted(palette)),
    ]);
    frame.render_widget(line2, keys_line);
}

/// All per-section errors from one sample, joined — the honest
/// multi-failure readout.
fn section_errors(sample: &crate::itop::worker::TopSample) -> String {
    let pairs = [
        ("status", &sample.status_error),
        ("metrics", &sample.metrics_error),
        ("modules", &sample.modules_error),
        ("sessions", &sample.sessions_error),
        ("connections", &sample.connections_error),
        ("providers", &sample.providers_error),
    ];
    pairs
        .into_iter()
        .filter_map(|(name, err)| err.as_ref().map(|err| format!("{name}: {err}")))
        .collect::<Vec<String>>()
        .join("; ")
}

/// The help overlay — centered, the full keymap (the `?`/F1 modal).
fn render_help(state: &TopState, frame: &mut Frame) {
    let palette = &state.palette;
    let lines = vec![
        Line::from(Span::styled(
            "itop — htop for Ignition",
            theme::emphasis(palette),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("q           ", theme::success(palette)),
            Span::styled("quit", theme::text(palette)),
        ]),
        Line::from(vec![
            Span::styled("space / p   ", theme::success(palette)),
            Span::styled("pause / resume sampling", theme::text(palette)),
        ]),
        Line::from(vec![
            Span::styled("+ / -       ", theme::success(palette)),
            Span::styled("sample interval up / down (1–60s)", theme::text(palette)),
        ]),
        Line::from(vec![
            Span::styled("r           ", theme::success(palette)),
            Span::styled("refresh now", theme::text(palette)),
        ]),
        Line::from(vec![
            Span::styled("e           ", theme::success(palette)),
            Span::styled(
                "scriptExec probe — gateway-side JMX snapshot",
                theme::text(palette),
            ),
        ]),
        Line::from(vec![
            Span::styled("s           ", theme::success(palette)),
            Span::styled(
                "cycle sort key (kind/name/state/age/detail)",
                theme::text(palette),
            ),
        ]),
        Line::from(vec![
            Span::styled("S           ", theme::success(palette)),
            Span::styled("reverse sort direction", theme::text(palette)),
        ]),
        Line::from(vec![
            Span::styled("/           ", theme::success(palette)),
            Span::styled(
                "filter (substring across all columns)",
                theme::text(palette),
            ),
        ]),
        Line::from(vec![
            Span::styled("1-6         ", theme::success(palette)),
            Span::styled(
                "view all / modules / sessions / connections / providers / threads",
                theme::text(palette),
            ),
        ]),
        Line::from(vec![
            Span::styled("6 (threads) ", theme::success(palette)),
            Span::styled(
                "fetch the live thread dump again — Enter on a row opens its stack",
                theme::text(palette),
            ),
        ]),
        Line::from(vec![
            Span::styled("j k arrows  ", theme::success(palette)),
            Span::styled(
                "move selection (J/K/pgup/pgdn page, g/G home/end)",
                theme::text(palette),
            ),
        ]),
        Line::from(vec![
            Span::styled("x / F9      ", theme::success(palette)),
            Span::styled(
                "terminate the selected session (confirm with y)",
                theme::text(palette),
            ),
        ]),
        Line::from(vec![
            Span::styled("Esc         ", theme::success(palette)),
            Span::styled("clear filter / cancel modal", theme::text(palette)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "only session rows terminate (designer prune / perspective / vision close)",
            theme::muted(palette),
        )),
    ];
    let height = lines.len() as u16 + 2;
    render_centered(state, frame, "itop help", lines, 60, height);
}

/// The thread stack viewer — the web UI's per-thread stack pane.
/// SCROLLABLE (a real stack outgrows any modal): the offset moves the
/// window over the frozen snapshot lines; the scroll position and the
/// line count ride the title (where you are, honestly).
fn render_thread_stack(
    state: &TopState,
    frame: &mut Frame,
    name: &str,
    lines: &[String],
    scroll: usize,
) {
    let palette = &state.palette;
    let area = frame.area();
    let width = 100u16.min(area.width.saturating_sub(2));
    let height = (area.height.saturating_sub(2))
        .min(lines.len() as u16 + 4)
        .max(6);
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let popup = Rect::new(x, y, width, height);

    let scroll = scroll.min(lines.len().saturating_sub(1));
    let body: Vec<Line> = lines
        .iter()
        .map(|line| Line::from(Span::styled(line.clone(), theme::text(palette))))
        .collect();
    let block = Block::bordered()
        .title(format!("thread — {name}"))
        .title_bottom(Line::from(Span::styled(
            format!(
                "lines {}/{} · j/k scroll · esc closes",
                scroll + 1,
                lines.len()
            ),
            theme::muted(palette),
        )))
        .border_style(theme::border(palette))
        .title_style(theme::title(palette));
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(body)
            .block(block)
            .scroll((scroll as u16, 0))
            .wrap(Wrap { trim: false }),
        popup,
    );
}

/// The filter prompt — bottom-centered one-line input.
fn render_filter(state: &TopState, frame: &mut Frame, buffer: &str) {
    let palette = &state.palette;
    let line = Line::from(vec![
        Span::styled("/", theme::accent(palette)),
        Span::styled(buffer.to_string(), theme::text(palette)),
        Span::styled("█", theme::text(palette)),
    ]);
    render_centered(state, frame, "filter", vec![line], 50, 3);
}

/// The kill confirmation — the TUI-side `--yes` guard.
fn render_confirm_kill(
    state: &TopState,
    frame: &mut Frame,
    target: &crate::itop::state::KillTarget,
) {
    let palette = &state.palette;
    let lines = vec![
        Line::from(Span::styled(
            format!("Terminate {} ({})?", target.label, target.kind),
            theme::text(palette),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "y terminate · any other key cancels",
            theme::warning(palette),
        )),
    ];
    render_centered(state, frame, "confirm kill", lines, 60, 5);
}

/// The scriptExec probe's result — a DOCUMENT (twelve JMX rows), so it
/// renders as a modal the height of its content (the testing-run
/// exclusion's rule: a result document never shrinks to one line).
/// Ok shows the pretty JSON (stdout + route-measured elapsed ride the
/// title); Err shows the honest refusal.
fn render_script_probe(state: &TopState, frame: &mut Frame, result: &Result<String, String>) {
    let palette = &state.palette;
    match result {
        Ok(document) => {
            let lines: Vec<Line> = document
                .lines()
                .map(|line| Line::from(Span::styled(line.to_string(), theme::text(palette))))
                .collect();
            let height = lines.len() as u16 + 4; // borders + title + elapsed note
            let mut all = lines;
            all.push(Line::from(""));
            all.push(Line::from(Span::styled(
                "esc closes · e re-runs the probe",
                theme::muted(palette),
            )));
            render_centered(
                state,
                frame,
                "scriptExec probe — JMX snapshot",
                all,
                64,
                height + 2,
            );
        }
        Err(refusal) => {
            let lines = vec![
                Line::from(Span::styled(refusal.clone(), theme::warning(palette))),
                Line::from(""),
                Line::from(Span::styled(
                    "the probe rides the scriptExec WebDev route — a gateway-side \
                     Jython snapshot (JMX internals no REST endpoint exposes)",
                    theme::muted(palette),
                )),
            ];
            render_centered(state, frame, "scriptExec probe unavailable", lines, 78, 6);
        }
    }
}

/// The shared centered-modal chrome (the cockpit's modal geometry).
fn render_centered(
    state: &TopState,
    frame: &mut Frame,
    title: &str,
    lines: Vec<Line<'static>>,
    width: u16,
    height: u16,
) {
    let palette = &state.palette;
    let area = frame.area();
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let popup = Rect::new(x, y, width, height);
    let block = Block::bordered()
        .title(title)
        .border_style(theme::border(palette))
        .title_style(theme::title(palette));
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        popup,
    );
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::itop::state::KillTarget;
    use crate::itop::worker::TopSample;
    use ignition_core::actions::inspect::{MetricsResult, ModulesResult, StatusResult};
    use ignition_core::actions::sessions::SessionsResult;
    use ignition_core::actions::tags::TagProvidersResult;
    use ignition_core::client::sessions::PerspectiveSession;
    use ignition_core::client::status::{ModuleInfo, Overview};

    /// Render one frame on a fixed 100×30 TestBackend and return the
    /// full text grid (the dashboard render-proof idiom).
    fn rendered(state: &TopState) -> String {
        rendered_at(state, 100, 30)
    }

    /// The grid at an explicit size — the threads table needs ~150
    /// columns to show a full thread name, its tid, and lock strings
    /// (the live captures run at 190).
    fn rendered_at(state: &TopState, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
        terminal.draw(|frame| render(state, frame)).expect("draw");
        let buffer = terminal.backend().buffer();
        let mut text = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                text.push_str(buffer[(x, y)].symbol());
            }
            text.push('\n');
        }
        text
    }

    /// A fixture sample carrying one row per family — handcrafted (no
    /// wiremock): the render proof cares about shapes, not endpoints.
    fn fixture_sample() -> TopSample {
        TopSample {
            status: Some(StatusResult {
                gateway: ignition_core::actions::inspect::StatusGateway {
                    name: Some("whiskeyhouse".into()),
                    ignition_version: "8.3.6 (b2026042713)".into(),
                    edition: Some("standard".into()),
                    license: None,
                },
                state: "RUNNING".into(),
                overview: ignition_core::actions::inspect::StatusOverview {
                    java: None,
                    os: None,
                    uptime_ms: 2 * 86_400_000 + 3 * 3_600_000,
                    memory: vec![338_137_088, 1_073_741_824],
                    cpu_fraction: 0.0031,
                    disk: Some(ignition_core::client::status::DiskInfo {
                        total: 62_661_259_264,
                        used: 12_272_824_320,
                    }),
                    license: None,
                },
            }),
            metrics: Some(MetricsResult {
                current: ignition_core::client::metrics::CurrentGauges {
                    cpu: 4.88,
                    heap_memory: 240_000_000.0,
                    max_memory: 1_073_741_824.0,
                    extra: Default::default(),
                },
                threads: ignition_core::client::metrics::ThreadCounts {
                    running: 32,
                    waiting: 39,
                    timed_waiting: 51,
                    blocked: 0,
                    extra: Default::default(),
                },
                history: None,
            }),
            modules: Some(ModulesResult {
                items: vec![ModuleInfo {
                    id: "com.inductiveautomation.perspective".into(),
                    name: "Perspective".into(),
                    version: "8.3.6".into(),
                    state: Some("ACTIVE".into()),
                    license_state: Some("Active".into()),
                    vendor_name: None,
                    startup_time: None,
                    extra: Default::default(),
                }],
                quarantined: false,
            }),
            sessions: Some(SessionsResult {
                designers: vec![],
                perspective: vec![PerspectiveSession {
                    id: "ps-1".into(),
                    username: "admin".into(),
                    authorized: true,
                    project: "whiskeyhouse".into(),
                    client_address: "10.0.0.9:55321".into(),
                    last_comm: 0,
                    active_pages: 2,
                    user_agent: String::new(),
                    extra: Default::default(),
                }],
                vision: vec![],
            }),
            connections: None,
            providers: Some(TagProvidersResult {
                providers: vec![ignition_core::actions::tags::TagProviderRow {
                    name: "default".into(),
                    enabled: true,
                    tag_count: Some(12_341),
                    health: None,
                    managed: false,
                }],
            }),
            ..TopSample::default()
        }
    }

    fn fixture_state() -> TopState {
        TopState {
            last: Some(Box::new(fixture_sample())),
            last_at: Some(std::time::Instant::now()),
            ..TopState::default()
        }
    }

    /// The loading state renders honestly — never blank (the
    /// must-have-truth rule at itop's surface).
    #[test]
    fn loading_renders_sampling_never_blank() {
        let state = TopState::default();
        let text = rendered(&state);
        assert!(text.contains("sampling"), "loading state names itself");
        assert!(text.contains("cpu"), "the gauge titles always render");
    }

    /// The loaded fixture renders every family's rows, the identity
    /// line, and the footer hints.
    #[test]
    fn loaded_fixture_renders_rows_identity_and_hints() {
        let state = fixture_state();
        let text = rendered(&state);
        for token in [
            "Perspective",    // module NAME
            "ACTIVE",         // module STATE
            "admin",          // perspective session user
            "default",        // provider name
            "12341 tags",     // provider DETAIL
            "whiskeyhouse",   // gateway name on the identity line
            "RUNNING",        // running state
            "2d 03h",         // uptime condensed
            "R32 W39 T51 B0", // thread counts
            "pause",          // the key hints
        ] {
            assert!(text.contains(token), "expected {token:?} in the frame");
        }
    }

    /// Section errors render verbatim in the footer (the honest
    /// degradation readout), and the paused mode is visible.
    #[test]
    fn errors_render_verbatim_and_pause_is_visible() {
        let sample = TopSample {
            metrics_error: Some("connection refused".into()),
            ..TopSample::default()
        };
        let state = TopState {
            last: Some(Box::new(sample)),
            last_at: Some(std::time::Instant::now()),
            paused: true,
            ..TopState::default()
        };
        let text = rendered(&state);
        assert!(
            text.contains("metrics: connection refused"),
            "the section error renders verbatim"
        );
        assert!(text.contains("PAUSED"), "paused mode is visible");
        assert!(!text.contains("all sections healthy"));
    }

    /// The help modal renders over the frame and lists the keymap.
    #[test]
    fn help_modal_renders_the_keymap() {
        let mut state = fixture_state();
        state.modal = Some(Modal::Help);
        let text = rendered(&state);
        assert!(text.contains("itop help"));
        assert!(text.contains("terminate the selected session"));
        assert!(text.contains("pause / resume sampling"));
    }

    /// The kill confirmation renders the staged target.
    #[test]
    fn confirm_kill_renders_the_target() {
        let mut state = fixture_state();
        state.modal = Some(Modal::ConfirmKill {
            target: KillTarget {
                kind: ignition_core::actions::sessions::SessionType::Perspective,
                id: "ps-1".into(),
                label: "perspective admin".into(),
            },
        });
        let text = rendered(&state);
        assert!(text.contains("confirm kill"));
        assert!(text.contains("Terminate perspective admin (perspective)?"));
    }

    /// The filter modal renders the prompt with the buffer.
    #[test]
    fn filter_modal_renders_the_buffer() {
        let mut state = fixture_state();
        state.modal = Some(Modal::Filter {
            buffer: "persp".into(),
        });
        let text = rendered(&state);
        assert!(text.contains("filter"));
        assert!(text.contains("persp"));
    }

    /// An Overview model exists for the identity path — the smoke
    /// constructor compiles against the real fields (guards against
    /// silent model drift).
    #[test]
    fn overview_model_smoke() {
        let overview: Overview = serde_json::from_value(serde_json::json!({
            "version": "8.3.6",
            "uptime": 1i64,
            "memory": [1i64, 2i64],
            "cpu": 0.5
        }))
        .expect("the minimal overview shape parses");
        assert_eq!(overview.uptime, 1);
    }

    use crate::itop::state::{SortKey, ThreadSnapshot, thread_stack_lines};

    /// A fixture dump — two threads, one at 3.75% cpu holding a monitor,
    /// one blocked waiting on it, id 7 deadlocked.
    fn fixture_dump() -> ignition_core::client::threads::FormattedThreadDump {
        serde_json::from_value(serde_json::json!({
        "version": "dump-version-1",
        "threads": [
            {"name": "Perspective-Worker-3", "id": 42, "state": "RUNNABLE",
             "daemon": true, "cpuUsage": 3.75, "stacktrace": [
                "Worker.lock - line 88",
                "java.lang.Thread.sleep(Native Method)"
             ],
             "lockedMonitors": [{"lock": "<0x1a2b> (a Worker)", "frame": "Worker.lock - line 88"}]},
            {"name": "pool-2-thread-1", "id": 7, "state": "BLOCKED", "cpuUsage": 0.0,
             "waitingFor": {"lock": "<0x1a2b> (a Worker)"}}
        ]
    }))
    .expect("the fixture dump parses")
    }

    /// The threads view (family 6) renders the thread table sorted cpu
    /// desc — name, tid, cpu, state, web/daemon/system columns — plus
    /// the deadlock alarm in the title and the `⏎ stack` hint.
    #[test]
    fn threads_view_renders_rows_and_the_deadlock_alarm() {
        let dump = fixture_dump();
        let state = TopState {
            family: Family::Threads,
            sort_key: SortKey::Cpu,
            thread_dump: Some(Box::new(ThreadSnapshot {
                at: std::time::Instant::now(),
                dump: Some(Box::new(dump)),
                dump_error: None,
                deadlocked: Some(vec![7]),
                deadlocks_error: None,
            })),
            ..fixture_state()
        };
        let text = rendered_at(&state, 150, 30);
        assert!(text.contains("Perspective-Worker-3"), "{text}");
        assert!(text.contains("42"), "the tid column renders");
        assert!(text.contains("3.8"), "the cpu column renders");
        assert!(text.contains("RUNNABLE"), "the state column renders");
        assert!(text.contains("⏎ stack"), "the enter hint renders");
        assert!(
            text.contains("⚠ 1 DEADLOCKED"),
            "the deadlock alarm renders in the title"
        );
        assert!(
            text.contains("pool-2-thread-1"),
            "all rows render (deadlocked included)"
        );
    }

    /// The thread stack modal renders the selected thread's name and
    /// its stack frame verbatim, with the scroll label.
    #[test]
    fn thread_stack_modal_renders_the_selected_stack() {
        let dump = fixture_dump();
        let state = TopState {
            modal: Some(Modal::ThreadStack {
                name: "Perspective-Worker-3".into(),
                lines: thread_stack_lines(&dump.threads[0], false),
                scroll: 0,
            }),
            ..fixture_state()
        };
        let text = rendered(&state);
        assert!(text.contains("Perspective-Worker-3"), "{text}");
        assert!(text.contains("Worker.lock - line 88"), "{text}");
        assert!(text.contains("lines 1/"), "the scroll label renders");
    }
}
