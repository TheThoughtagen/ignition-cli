//! `ign` binary — single-exit-point dispatch chassis.
//!
//! Flow: `Cli::try_parse` → `apply_env_defaults` → `init_tracing` → tokio
//! runtime → `dispatch` → exactly one of: success (render to stdout, exit 0)
//! or a [`CoreError`] (render to stderr, exit via the LOCKED
//! `CoreError::exit_code` mapping — the only place exit codes are decided).
//!
//! Contracts established here (Phase 1 research, Patterns 1 + 4):
//! - Env→flag precedence happens in exactly ONE place: [`apply_env_defaults`].
//! - Render mode is decided exactly ONCE: [`RenderMode::resolve`]
//!   (`--compact` implies `--json`).
//! - Diagnostics go to stderr only ([`init_tracing`]); stdout is reserved for
//!   data output; errors render to stderr in every mode — no crossover.
//! - No direct exit calls anywhere outside clap's `Error::exit`.

mod cli;
mod render;

use std::process::ExitCode;

use clap::Parser;

use ignition_core::error::CoreError;

use crate::cli::{Cli, Commands};
use crate::render::{RenderMode, render_error, render_ok};

/// What a dispatched subcommand produced. One variant per command; grows in
/// later plans. The payload serializes as the envelope's `data` (see
/// [`Self::data`]); human-mode rendering lives in `render.rs`.
enum ActionOutput {
    /// `ign version` — gateway fields arrive in 01-04.
    Version { cli_version: &'static str },
}

/// `data` payload for [`ActionOutput::Version`] (declaration order = golden
/// field order).
#[derive(serde::Serialize)]
struct VersionData<'a> {
    cli_version: &'a str,
}

impl ActionOutput {
    /// The JSON `data` payload for the envelope.
    fn data(&self) -> impl serde::Serialize + '_ {
        match self {
            ActionOutput::Version { cli_version } => VersionData { cli_version },
        }
    }
}

fn main() -> ExitCode {
    let mut cli = match Cli::try_parse() {
        Ok(c) => c,
        // clap renders usage errors itself (its exit 2) and help/version
        // (exit 0) — by design, do NOT build a clap error hook.
        Err(e) => e.exit(),
    };
    apply_env_defaults(&mut cli);
    init_tracing(cli.verbose);
    // The ONE render-mode decision (LOCKED): --compact implies --json.
    let mode = RenderMode::resolve(cli.json, cli.compact);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build async runtime");
    // The envelope's profile echo stays None until config resolution lands
    // (01-03 threads the resolved profile name through); the FIELD exists
    // from day one so goldens change value, never shape.
    let profile: Option<&str> = None;
    match runtime.block_on(dispatch(cli)) {
        Ok(out) => {
            render_ok(&out, profile, mode);
            ExitCode::SUCCESS
        }
        Err(err) => {
            render_error(&err, profile, mode);
            // The single exit-code mapping point (LOCKED taxonomy).
            ExitCode::from(err.exit_code())
        }
    }
}

/// Subcommand dispatch: typed `Result<ActionOutput, CoreError>`; rendering
/// and exit mapping happen once, in `main`.
async fn dispatch(cli: Cli) -> Result<ActionOutput, CoreError> {
    match cli.command {
        Commands::Version => Ok(ActionOutput::Version {
            cli_version: env!("CARGO_PKG_VERSION"),
        }),
        #[cfg(feature = "tui")]
        Commands::Tui => Err(CoreError::Internal(
            "the TUI cockpit arrives in a later phase".into(),
        )),
    }
}

/// The ONLY place env→flag precedence happens. Flags win; env fills the gaps:
/// `IGNITION_PROFILE` (when --profile absent), `IGNITION_JSON=1`, `IGNITION_YES=1`.
/// Subcommand code only ever reads the struct — single precedence point.
fn apply_env_defaults(cli: &mut Cli) {
    if cli.profile.is_none()
        && let Ok(profile) = std::env::var("IGNITION_PROFILE")
        && !profile.is_empty()
    {
        cli.profile = Some(profile);
    }
    if !cli.json && std::env::var("IGNITION_JSON").is_ok_and(|v| v == "1") {
        cli.json = true;
    }
    if !cli.yes && std::env::var("IGNITION_YES").is_ok_and(|v| v == "1") {
        cli.yes = true;
    }
}

/// stderr-only tracing init. Filter levels: 0=warn (default), 1=info, 2=debug,
/// 3+=trace. `IGNITION_LOG`, when set, overrides the verbosity-derived filter
/// (pass-through to EnvFilter, RUST_LOG-style directives).
fn init_tracing(verbosity: u8) {
    use tracing_subscriber::EnvFilter;

    let filter = match std::env::var("IGNITION_LOG") {
        Ok(spec) if !spec.is_empty() => EnvFilter::new(spec),
        _ => EnvFilter::new(match verbosity {
            0 => "warn",
            1 => "info",
            2 => "debug",
            _ => "trace",
        }),
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}
