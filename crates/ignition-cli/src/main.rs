//! `ign` binary — single-exit-point dispatch chassis.
//!
//! Flow: `Cli::try_parse` → `apply_env_defaults` → `init_tracing` → tokio
//! runtime → `dispatch` → one of exactly two `ExitCode` return values (plus
//! clap's own `e.exit()` for usage errors, exit 2 by design).
//!
//! Contracts established here (Phase 1 research, Patterns 1 + 4):
//! - Env→flag precedence happens in exactly ONE place: [`apply_env_defaults`].
//! - Diagnostics go to stderr only ([`init_tracing`]); stdout is reserved for
//!   data output.
//! - No direct exit calls anywhere outside clap's `Error::exit` (the Phase-1
//!   single-exit-point discipline).

mod cli;

use std::process::ExitCode;

use clap::Parser;

use crate::cli::{Cli, Commands};

fn main() -> ExitCode {
    let mut cli = match Cli::try_parse() {
        Ok(c) => c,
        // clap renders usage errors itself (exit 2) and help/version (exit 0)
        // — by design, do NOT build a clap error hook.
        Err(e) => e.exit(),
    };
    apply_env_defaults(&mut cli);
    init_tracing(cli.verbose);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build async runtime");
    match runtime.block_on(dispatch(cli)) {
        Ok(()) => ExitCode::SUCCESS,
        // Simplified error surface: the typed CoreError envelope + exit-code
        // mapping lands in plan 01-02. Errors render to stderr, never stdout.
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(1)
        }
    }
}

/// Subcommand dispatch. The `Result<_, CoreError>` plumbing (single exit-code
/// mapping point) arrives in plan 01-02; a plain message suffices for now.
async fn dispatch(cli: Cli) -> Result<(), String> {
    match cli.command {
        Commands::Version => {
            // Plain text; envelope rendering (--json) arrives in 01-02.
            println!("ign {} (ignition-cli)", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        #[cfg(feature = "tui")]
        Commands::Tui => Err("TUI arrives in a later phase".to_string()),
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
