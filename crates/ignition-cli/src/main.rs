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
//! - The profile context is resolved exactly ONCE per command in
//!   [`dispatch`] and threaded into EVERY envelope (success and error) —
//!   CORE-01's "active profile visible in every output".
//! - Diagnostics go to stderr only ([`init_tracing`]); stdout is reserved for
//!   data output; errors render to stderr in every mode — no crossover.
//! - No direct exit calls anywhere outside clap's `Error::exit`.

mod cli;
mod completions;
mod render;

use std::process::ExitCode;

use clap::Parser;

use ignition_core::actions;
use ignition_core::client::ReqwestGatewayApi;
use ignition_core::config::{self, AuthRef, Config, Credential, SecretStore};
use ignition_core::error::CoreError;

use crate::cli::{Cli, Commands, ProfileArgs, ProfileCmd};
use crate::render::{RenderMode, render_error, render_ok};

/// What a dispatched subcommand produced. One variant per command; grows in
/// later plans. The payload serializes as the envelope's `data` (see
/// [`Self::render_json`]); human-mode rendering lives in `render.rs`.
enum ActionOutput {
    /// `ign version` — CLI version + optional gateway report + warnings.
    Version(actions::version::VersionResult),
    /// `ign completions <SHELL>` — raw script text on stdout, the ONE
    /// sanctioned exception: printed verbatim regardless of `--json`
    /// (shells source stdout; see `render_ok`).
    Completions {
        /// Target shell.
        shell: clap_complete::aot::Shell,
    },
    /// `ign profile add`.
    ProfileAdd(actions::profile::ProfileAddResult),
    /// `ign profile list`.
    ProfileList(actions::profile::ProfileListResult),
    /// `ign profile use`.
    ProfileUse(actions::profile::ProfileUseResult),
}

impl ActionOutput {
    /// The JSON envelope for this output (pretty or compact). Matched per
    /// variant — `Serialize` is not dyn-compatible, and a monomorphic match
    /// preserves each payload's declaration (golden) order.
    pub(crate) fn render_json(&self, profile: Option<&str>, compact: bool) -> String {
        use ignition_core::output::render_success;

        match self {
            ActionOutput::Version(result) => render_success(profile, result, compact),
            // Unreachable in practice (render_ok intercepts Completions
            // before mode dispatch) — but degrades to the correct raw
            // script rather than panicking if that bypass ever moves.
            ActionOutput::Completions { shell } => crate::completions::completions(*shell),
            ActionOutput::ProfileAdd(result) => render_success(profile, result, compact),
            ActionOutput::ProfileList(result) => render_success(profile, result, compact),
            ActionOutput::ProfileUse(result) => render_success(profile, result, compact),
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
    // dispatch resolves the profile context and returns it alongside the
    // result so BOTH the success and the error envelope echo it (CORE-01).
    let (profile, result) = runtime.block_on(dispatch(cli));
    match result {
        Ok(out) => {
            render_ok(&out, profile.as_deref(), mode);
            ExitCode::SUCCESS
        }
        Err(err) => {
            render_error(&err, profile.as_deref(), mode);
            // The single exit-code mapping point (LOCKED taxonomy).
            ExitCode::from(err.exit_code())
        }
    }
}

/// Subcommand dispatch: typed `Result<ActionOutput, CoreError>` plus the
/// resolved profile name for the envelope echo. Rendering and exit mapping
/// happen once, in `main`.
///
/// The profile context is resolved exactly ONCE (flag — which already
/// contains `IGNITION_PROFILE` via [`apply_env_defaults`] — > config
/// active), with the `IGNITION_URL` env overlay applied to the selected
/// profile first. `version` and `profile list` tolerate no-selection (fresh
/// install, exit 0, envelope `profile: null`); an unknown name is
/// `profile_not_found` (exit 3) everywhere.
async fn dispatch(cli: Cli) -> (Option<String>, Result<ActionOutput, CoreError>) {
    // Completions never touch config — shells source them at install
    // time, and a broken config.toml must not break `completions`.
    if let Commands::Completions { shell } = cli.command {
        return (None, Ok(ActionOutput::Completions { shell }));
    }
    let path = config::config_path();
    let mut config = match config::load(&path) {
        Ok(config) => config,
        Err(err) => return (None, Err(err)),
    };

    match cli.command {
        Commands::Version => match resolve_profile_context(&mut config, cli.profile.as_deref()) {
            Ok(None) => {
                // Fresh install / nothing resolved: CLI version only.
                let result = actions::version::version(None, env!("CARGO_PKG_VERSION")).await;
                (None, result.map(ActionOutput::Version))
            }
            Ok(Some((name, profile))) => {
                // Credential for the CHECK only: exhaustion degrades to
                // header-less (version must not demand a secret; gateway-info
                // is `auth: none`); every other credential error propagates.
                let credential = match resolve_secret_opt(&name, &profile.auth) {
                    Ok(credential) => credential,
                    Err(err) => return (Some(name), Err(err)),
                };
                // The client is built from the POST-OVERLAY profile — the
                // research-locked precedence (flag > IGNITION_URL env >
                // profile value) must hold at the construction site, not
                // just in the config unit tests.
                let result = match ReqwestGatewayApi::new(&profile, credential) {
                    Ok(api) => {
                        actions::version::version(Some(&api), env!("CARGO_PKG_VERSION")).await
                    }
                    Err(err) => Err(err),
                };
                (Some(name), result.map(ActionOutput::Version))
            }
            Err(err) => (None, Err(err)),
        },
        Commands::Profile(ProfileArgs { command }) => match command {
            ProfileCmd::List => {
                match resolve_profile_context(&mut config, cli.profile.as_deref()) {
                    Ok(selection) => (
                        selection.as_ref().map(|(name, _)| name.clone()),
                        Ok(ActionOutput::ProfileList(actions::profile::list(&config))),
                    ),
                    Err(err) => (None, Err(err)),
                }
            }
            ProfileCmd::Add {
                name,
                url,
                label,
                token_env,
                keyring,
                user_env,
                password_env,
                active,
            } => {
                // `add` defines the active profile itself — no pre-resolution
                // (a --profile flag naming the NEW profile must not fail).
                let prior_active = config.active.clone();
                let result = actions::profile::add(
                    &path,
                    &name,
                    &url,
                    label.as_deref(),
                    auth_ref_from_flags(token_env, keyring, user_env, password_env),
                    active,
                );
                match result {
                    Ok(result) => {
                        let envelope_profile = if result.active {
                            Some(result.name.clone())
                        } else {
                            prior_active
                        };
                        (envelope_profile, Ok(ActionOutput::ProfileAdd(result)))
                    }
                    Err(err) => (None, Err(err)),
                }
            }
            ProfileCmd::Use { name } => match actions::profile::use_profile(&path, &name) {
                Ok(result) => (
                    Some(result.active.clone()),
                    Ok(ActionOutput::ProfileUse(result)),
                ),
                Err(err) => (None, Err(err)),
            },
        },
        // Runtime-unreachable: dispatch returns early for Completions
        // before config load (a broken config must not break `completions`);
        // the arm exists only for match exhaustiveness.
        Commands::Completions { .. } => {
            unreachable!("completions handled before config load")
        }
        #[cfg(feature = "tui")]
        Commands::Tui => (
            None,
            Err(CoreError::Internal(
                "the TUI cockpit arrives in a later phase".into(),
            )),
        ),
    }
}

/// Resolve the profile context once: mirror the selection precedence
/// (flag > config.active) to scope the `IGNITION_URL` env overlay, then
/// resolve and validate the selection.
fn resolve_profile_context(
    config: &mut Config,
    flag: Option<&str>,
) -> Result<Option<(String, config::Profile)>, CoreError> {
    let overlay_target = flag.map(str::to_string).or_else(|| config.active.clone());
    config::apply_env_overlay(config, overlay_target.as_deref());
    config::resolve_selection(config, flag)
}

/// Credential resolution degraded for non-authenticating commands: the
/// LOCKED chain (env tokens → keyring → basic pair) with
/// `SecretUnavailable` mapped to `Ok(None)` — version proceeds header-less
/// (gateway-info is `auth: none`) instead of demanding a secret. Every
/// OTHER credential error propagates.
fn resolve_secret_opt(profile: &str, auth: &AuthRef) -> Result<Option<Credential>, CoreError> {
    let chain: Vec<Box<dyn SecretStore>> = vec![
        Box::new(config::EnvStore),
        Box::new(config::KeyringStore),
        Box::new(config::BasicEnvStore),
    ];
    config::resolve_secret(profile, auth, &chain)
        .map(Some)
        .or_else(|err| match err {
            CoreError::SecretUnavailable { .. } => Ok(None),
            other => Err(other),
        })
}

/// Deterministic auth-ref construction from `profile add` flags:
/// `--token-env` > `--keyring` > `--user-env`+`--password-env`; none given
/// falls back to the generic `IGNITION_TOKEN` reference.
fn auth_ref_from_flags(
    token_env: Option<String>,
    keyring: Option<String>,
    user_env: Option<String>,
    password_env: Option<String>,
) -> config::AuthRef {
    if let Some(token_env) = token_env {
        config::AuthRef::TokenEnv { token_env }
    } else if let Some(keyring) = keyring {
        config::AuthRef::Keyring { keyring }
    } else if let (Some(user_env), Some(password_env)) = (user_env, password_env) {
        config::AuthRef::Basic {
            user_env,
            password_env,
        }
    } else {
        config::AuthRef::default()
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

/// CORE-06 pattern, proven now so later phases inherit it verbatim
/// (`project delete` in Phase 3, `rig reset` in Phase 4): destructive
/// operations refuse without `--yes` — which already merges `IGNITION_YES`
/// via [`apply_env_defaults`] — with a usage-class error (exit 2: it names
/// a flag the caller must add) whose hint says exactly that. Pinned here
/// in main.rs, no separate confirm.rs file.
#[expect(
    dead_code,
    reason = "no destructive command exists in Phase 1; first caller is Phase 3's project delete — the expectation flags removal when it gains a caller"
)]
fn require_confirmation(yes: bool, operation: &str) -> Result<(), CoreError> {
    if yes {
        Ok(())
    } else {
        Err(CoreError::ConfirmationRequired {
            operation: operation.to_string(),
        })
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

#[cfg(test)]
mod tests {
    use super::require_confirmation;

    /// CORE-06 guard proof: without `--yes` → usage-class error (exit 2,
    /// `confirmation_required` slug) with a hint naming BOTH the flag and
    /// the env escape hatch; with `--yes` → Ok. (`IGNITION_YES=1` reaches
    /// the guard as `yes == true` via `apply_env_defaults`, binary-tested
    /// by `cli_chassis::env_yes_flag_is_accepted`.) Phase 3+ (`project
    /// delete`, `rig reset`) inherits this helper verbatim.
    #[test]
    fn confirmation_guard_refuses_without_yes() {
        let err = require_confirmation(false, "project delete").expect_err("refuses without --yes");
        assert_eq!(err.exit_code(), 2, "usage class — it names a missing flag");
        assert_eq!(err.code(), "confirmation_required");
        let hint = err.hint().expect("hint required");
        assert!(
            hint.contains("--yes") && hint.contains("IGNITION_YES"),
            "hint names the flag and the env escape hatch: {hint}"
        );
        assert!(
            err.to_string().contains("project delete"),
            "message names the operation: {err}"
        );

        require_confirmation(true, "project delete").expect("--yes confirms");
    }
}
