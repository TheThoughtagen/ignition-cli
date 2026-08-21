//! Output rendering for the `ign` bin — the ONLY home of human-mode output
//! (never in core; ARCHITECTURE.md layering invariant).
//!
//! Stream discipline: success renders to stdout in every mode; errors render
//! to stderr in every mode (human-readable by default; the JSON envelope
//! under `--json`/`--compact`) — no crossover. JSON strings come from
//! `ignition_core::output` (pretty or compact per the LOCKED precedence:
//! `--compact` implies `--json`).

use ignition_core::error::CoreError;
use ignition_core::output::{render_failure, render_success};

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
        RenderMode::Human => render_human(out),
        RenderMode::PrettyJson => {
            let rendered = render_success(profile, &out.data(), false);
            println!("{rendered}");
        }
        RenderMode::CompactJson => {
            let rendered = render_success(profile, &out.data(), true);
            println!("{rendered}");
        }
    }
}

/// Error path — ALWAYS stderr. Human-readable message + hint by default; the
/// JSON envelope under `--json`/`--compact`.
pub fn render_error(err: &CoreError, profile: Option<&str>, mode: RenderMode) {
    if mode.is_json() {
        let envelope = err.envelope(profile);
        let rendered = render_failure(&envelope, mode == RenderMode::CompactJson);
        eprintln!("{rendered}");
    } else {
        eprintln!("error: {err}");
        if let Some(hint) = err.hint() {
            eprintln!("hint: {hint}");
        }
    }
}

/// Human mode: plain lines for now; grows into tables/lines per command in
/// later phases — always here, never in core.
fn render_human(out: &ActionOutput) {
    match out {
        ActionOutput::Version { cli_version } => println!("ign {cli_version} (ignition-cli)"),
    }
}
