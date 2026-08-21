//! Output rendering for the `ign` bin — the ONLY home of human-mode output
//! (never in core; ARCHITECTURE.md layering invariant).
//!
//! Stream discipline: success renders to stdout in every mode; errors render
//! to stderr in every mode (human-readable by default; the JSON envelope
//! under `--json`/`--compact`) — no crossover. JSON strings come from
//! `ignition_core::output` (pretty or compact per the LOCKED precedence:
//! `--compact` implies `--json`).
//!
//! CORE-01 human path (research Pattern 4): EVERY human-mode render —
//! success AND error — begins with an active-profile header line
//! `[profile: NAME]` when a profile resolved; the header is omitted when
//! none did (a fresh install keeps the bare version line). JSON/compact
//! modes are untouched — the envelope's top-level `profile` field is their
//! mechanism.

use ignition_core::actions::inspect::{MetricsResult, ModulesResult, StatusResult};
use ignition_core::error::CoreError;
use ignition_core::output::render_failure;

use crate::ActionOutput;

/// The three render modes. Resolved exactly once, in `main`, by
/// [`RenderMode::resolve`] — the single precedence decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderMode {
    /// Default: human-readable lines/tables, rendered here (bin-only).
    Human,
    /// `--json`: pretty-printed envelope on stdout.
    PrettyJson,
    /// `--compact`: one-line envelope on stdout. Implies `--json`.
    CompactJson,
}

impl RenderMode {
    /// The LOCKED precedence (Pitfall 6): `--compact` implies `--json`.
    pub fn resolve(json: bool, compact: bool) -> Self {
        if compact {
            Self::CompactJson
        } else if json {
            Self::PrettyJson
        } else {
            Self::Human
        }
    }

    fn is_json(self) -> bool {
        matches!(self, Self::PrettyJson | Self::CompactJson)
    }
}

/// Success path — ALWAYS stdout.
pub fn render_ok(out: &ActionOutput, profile: Option<&str>, mode: RenderMode) {
    // The ONE sanctioned stdout exception: completions print the raw
    // script regardless of --json (shells source stdout; see completions.rs
    // and the README contract note). No profile header either — the script
    // must stay clean for sourcing.
    if let ActionOutput::Completions { shell } = out {
        print!("{}", crate::completions::completions(*shell));
        return;
    }
    match mode {
        RenderMode::Human => render_human(out, profile),
        RenderMode::PrettyJson => {
            let rendered = out.render_json(profile, false);
            println!("{rendered}");
        }
        RenderMode::CompactJson => {
            let rendered = out.render_json(profile, true);
            println!("{rendered}");
        }
    }
}

/// Error path — ALWAYS stderr. Human-readable message + hint by default; the
/// JSON envelope under `--json`/`--compact`. The human form leads with the
/// active-profile header when one resolved (CORE-01).
pub fn render_error(err: &CoreError, profile: Option<&str>, mode: RenderMode) {
    if mode.is_json() {
        let envelope = err.envelope(profile);
        let rendered = render_failure(&envelope, mode == RenderMode::CompactJson);
        eprintln!("{rendered}");
    } else {
        if let Some(name) = profile {
            eprintln!("[profile: {name}]");
        }
        eprintln!("error: {err}");
        if let Some(hint) = err.hint() {
            eprintln!("hint: {hint}");
        }
    }
}

/// Human mode: active-profile header line first (CORE-01, Pattern 4), then
/// plain lines per command — always here, never in core.
fn render_human(out: &ActionOutput, profile: Option<&str>) {
    if let Some(name) = profile {
        println!("[profile: {name}]");
    }
    match out {
        ActionOutput::Version(result) => {
            println!("ign {} (ignition-cli)", result.cli_version);
            if let Some(gateway) = &result.gateway {
                let edition = gateway.edition.as_deref().unwrap_or("unknown edition");
                println!("gateway {} ({edition})", gateway.ignition_version);
            }
            for warning in &result.warnings {
                println!("warning: {warning}");
            }
        }
        ActionOutput::Status(result) => render_status_human(result),
        ActionOutput::Modules(result) => render_modules_human(result),
        ActionOutput::Metrics(result) => render_metrics_human(result),
        // Unreachable: render_ok intercepts Completions before mode
        // dispatch (the sanctioned stdout exception).
        ActionOutput::Completions { shell } => {
            print!("{}", crate::completions::completions(*shell));
        }
        ActionOutput::ProfileAdd(result) => {
            println!("added profile {} ({})", result.name, result.url);
            if result.active {
                println!("active profile set to {}", result.name);
            }
        }
        ActionOutput::ProfileList(result) => {
            for summary in &result.profiles {
                let label = summary.label.as_deref().unwrap_or(&summary.name);
                println!(
                    "{}  {}  {}  {}",
                    summary.name, label, summary.url, summary.auth_kind
                );
            }
        }
        ActionOutput::ProfileUse(result) => println!("active profile set to {}", result.active),
    }
}

/// `ign status` human lines: identity, state, platform, uptime,
/// cpu/mem/disk, license (incl. the trial countdown — the
/// research-recommended banner).
fn render_status_human(result: &StatusResult) {
    let gateway = &result.gateway;
    let name = gateway.name.as_deref().unwrap_or("gateway");
    let edition = gateway.edition.as_deref().unwrap_or("unknown edition");
    println!("{}  {}  {}", name, gateway.ignition_version, edition);
    println!("state: {}", result.state);

    let overview = &result.overview;
    // Platform: "Java <version> (<vendor>) on <os name> (<arch>)" from
    // whichever halves the gateway reported.
    let java = overview
        .java
        .as_ref()
        .map(|java| format!("Java {} ({})", java.version, java.vendor));
    let os = overview
        .os
        .as_ref()
        .map(|os| format!("on {} ({})", os.name, os.arch));
    match (java, os) {
        (Some(java), Some(os)) => println!("platform: {java} {os}"),
        (Some(java), None) => println!("platform: {java}"),
        (None, Some(os)) => println!("platform: {os}"),
        (None, None) => {}
    }

    println!("uptime: {}", humanize_duration_ms(overview.uptime_ms));

    // Overview cpu is a 0–1 FRACTION (documented at the model) — the
    // human line is percent; metrics' gauges row is already percent.
    let cpu = overview.cpu_fraction * 100.0;
    let memory = match overview.memory.as_slice() {
        [used, max] => format!("{}/{}", human_bytes(*used), human_bytes(*max)),
        _ => "-".to_string(),
    };
    let disk = overview
        .disk
        .as_ref()
        .map(|disk| format!("{}/{}", human_bytes(disk.used), human_bytes(disk.total)));
    match disk {
        Some(disk) => println!("cpu {cpu:.1}%  memory {memory}  disk {disk}"),
        None => println!("cpu {cpu:.1}%  memory {memory}"),
    }

    // License banner incl. the trial countdown; falls back to the
    // gateway-info license mode when overview carries no block.
    match &overview.license {
        Some(license) => match license.trial_remaining_s {
            Some(remaining_s) => println!(
                "license: {}, {} remaining",
                license.state,
                humanize_duration_ms(remaining_s * 1000)
            ),
            None => println!("license: {}", license.state),
        },
        None => {
            if let Some(license) = &gateway.license {
                println!("license: {}", license.mode);
            }
        }
    }
}

/// `ign modules` human rows: `id  name  version  state  licenseState`.
fn render_modules_human(result: &ModulesResult) {
    for module in &result.items {
        let state = module.state.as_deref().unwrap_or("-");
        let license = module.license_state.as_deref().unwrap_or("-");
        println!(
            "{}  {}  {}  {}  {}",
            module.id, module.name, module.version, state, license
        );
    }
    if result.items.is_empty() {
        let kind = if result.quarantined {
            "quarantined"
        } else {
            "healthy"
        };
        println!("(no {kind} modules)");
    }
}

/// `ign metrics` human lines: gauges row, threads row; `--history` adds
/// a first/last summary line per non-empty series.
fn render_metrics_human(result: &MetricsResult) {
    let gauges = &result.current;
    println!(
        "cpu {:.1}%  heap {}/{}",
        gauges.cpu,
        human_bytes(gauges.heap_memory),
        human_bytes(gauges.max_memory)
    );
    let threads = &result.threads;
    println!(
        "threads: {} running, {} waiting, {} timed-waiting, {} blocked",
        threads.running, threads.waiting, threads.timed_waiting, threads.blocked
    );
    if let Some(charts) = &result.history {
        // cpu datapoints are PERCENT; memory series are bytes.
        for (label, series, is_percent) in [
            ("cpu", &charts.cpu_datapoints, true),
            ("heap", &charts.heap_memory_datapoints, false),
            ("non-heap", &charts.non_heap_memory_datapoints, false),
        ] {
            if let Some(first) = series.first()
                && let Some(last) = series.last()
            {
                let fmt = |datapoint: &ignition_core::client::metrics::Datapoint| {
                    if is_percent {
                        format!("{:.1}% @ {}", datapoint.value, datapoint.timestamp)
                    } else {
                        format!(
                            "{} @ {}",
                            human_bytes(datapoint.value as i64),
                            datapoint.timestamp
                        )
                    }
                };
                println!("history {label}: first {}, last {}", fmt(first), fmt(last));
            }
        }
    }
}

/// Milliseconds → compact human duration with the two most significant
/// non-zero units ("3d 4h", "1h 56m", "5m 38s", "12s"); "0s" when empty.
fn humanize_duration_ms(ms: i64) -> String {
    let total_s = ms / 1000;
    let units = [
        (total_s / 86400, "d"),
        ((total_s % 86400) / 3600, "h"),
        ((total_s % 3600) / 60, "m"),
        (total_s % 60, "s"),
    ];
    let mut parts = units
        .iter()
        .filter(|(value, _)| *value > 0)
        .map(|(value, unit)| format!("{value}{unit}"));
    match (parts.next(), parts.next()) {
        (Some(first), Some(second)) => format!("{first} {second}"),
        (Some(first), None) => first,
        (None, _) => "0s".to_string(),
    }
}

/// Bytes → compact human size (binary units, at most one decimal, none
/// when whole: "338MB", "322.5MB", "1GB", "11.4GB").
fn human_bytes(bytes: i64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if value.fract() == 0.0 {
        format!("{value:.0}{}", UNITS[unit])
    } else {
        format!("{value:.1}{}", UNITS[unit])
    }
}
