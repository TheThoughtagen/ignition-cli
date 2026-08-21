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
                let state = gateway.state.as_deref().unwrap_or("unknown state");
                println!("gateway {} ({edition}, {state})", gateway.version);
            }
            for warning in &result.warnings {
                println!("warning: {warning}");
            }
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
