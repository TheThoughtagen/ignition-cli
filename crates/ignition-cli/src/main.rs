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

use crate::cli::{
    Cli, Commands, LogLevel, LoggersCmd, LogsArgs, LogsCmd, ProfileArgs, ProfileCmd, ProjectArgs,
    ProjectCommand, SessionsArgs, SessionsCmd, WaitArgs, WaitCmd,
};
use crate::render::{RenderMode, render_error, render_log_entry_line, render_ok};

/// What a dispatched subcommand produced. One variant per command; grows in
/// later plans. The payload serializes as the envelope's `data` (see
/// [`Self::render_json`]); human-mode rendering lives in `render.rs`.
enum ActionOutput {
    /// `ign version` — CLI version + optional gateway report + warnings.
    Version(actions::version::VersionResult),
    /// `ign status` — merged gateway_info + overview + status_ping.
    Status(actions::inspect::StatusResult),
    /// `ign modules` — healthy (default) or quarantined module rows.
    Modules(actions::inspect::ModulesResult),
    /// `ign metrics` — current gauges + threads (+ optional history).
    Metrics(actions::inspect::MetricsResult),
    /// `ign sessions` — the three session families merged (filtered keys
    /// stay present-but-empty).
    Sessions(actions::sessions::SessionsResult),
    /// `ign sessions terminate` — the FIRST destructive command.
    SessionsTerminate(actions::sessions::TerminateResult),
    /// `ign connections` — DB/OPC connection status.
    Connections(actions::connections::ConnectionsResult),
    /// `ign logs` — the queried page (newest first).
    LogsList(actions::logs::LogPage),
    /// `ign logs -f` — the tail ALREADY streamed to stdout line-by-line
    /// (human) or as NDJSON (the second sanctioned stdout exception,
    /// README-documented); render prints nothing further.
    LogsTail(actions::logs::TailResult),
    /// `ign logs download` — archive written, path + byte count.
    LogsDownload(actions::logs::DownloadResult),
    /// `ign logs loggers` — the logger registry.
    LoggersList(actions::logs::LoggersEnvelope),
    /// `ign logs loggers set` — one logger level changed.
    LoggerSet(actions::logs::SetLevelResult),
    /// `ign logs loggers reset` — all custom levels reset.
    LoggerReset(actions::logs::ResetResult),
    /// `ign restart` — the POST fired (no wait).
    Restart(actions::restart::RestartResult),
    /// `ign restart --wait` — POST + floor + poll to RUNNING.
    RestartWait(actions::restart::RestartWaitResult),
    /// `ign wait <target>` — the target reached its terminal state.
    Wait(actions::restart::WaitResult),
    /// `ign doctor` — the structured checks[] report.
    Doctor(actions::doctor::DoctorResult),
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
    /// `ign project list` — every runnable project with inheritance
    /// info (PROJ-01).
    ProjectsList(actions::projects::ProjectsResult),
    /// `ign project new` — the create + read-back record.
    ProjectNew(actions::projects::ProjectSummary),
    /// `ign project copy` — source + destination read-back.
    ProjectCopy(actions::projects::ProjectCopyResult),
    /// `ign project rename` — previous name + renamed read-back.
    ProjectRename(actions::projects::ProjectRenameResult),
    /// `ign project set` — fields-touched (display-only) + read-back.
    ProjectSet(actions::projects::ProjectSetResult),
    /// `ign project delete` — the family's destructive verb.
    ProjectDelete(actions::projects::ProjectDeleteResult),
}

impl ActionOutput {
    /// The JSON envelope for this output (pretty or compact). Matched per
    /// variant — `Serialize` is not dyn-compatible, and a monomorphic match
    /// preserves each payload's declaration (golden) order.
    pub(crate) fn render_json(&self, profile: Option<&str>, compact: bool) -> String {
        use ignition_core::output::render_success;

        match self {
            ActionOutput::Version(result) => render_success(profile, result, compact),
            ActionOutput::Status(result) => render_success(profile, result, compact),
            ActionOutput::Modules(result) => render_success(profile, result, compact),
            ActionOutput::Metrics(result) => render_success(profile, result, compact),
            ActionOutput::Sessions(result) => render_success(profile, result, compact),
            ActionOutput::SessionsTerminate(result) => render_success(profile, result, compact),
            ActionOutput::Connections(result) => render_success(profile, result, compact),
            ActionOutput::LogsList(result) => render_success(profile, result, compact),
            // Unreachable in practice (render_ok intercepts LogsTail
            // before mode dispatch — the entries already streamed).
            ActionOutput::LogsTail(result) => render_success(profile, result, compact),
            ActionOutput::LogsDownload(result) => render_success(profile, result, compact),
            ActionOutput::LoggersList(result) => render_success(profile, result, compact),
            ActionOutput::LoggerSet(result) => render_success(profile, result, compact),
            ActionOutput::LoggerReset(result) => render_success(profile, result, compact),
            ActionOutput::Restart(result) => render_success(profile, result, compact),
            ActionOutput::RestartWait(result) => render_success(profile, result, compact),
            ActionOutput::Wait(result) => render_success(profile, result, compact),
            ActionOutput::Doctor(result) => render_success(profile, result, compact),
            // Unreachable in practice (render_ok intercepts Completions
            // before mode dispatch) — but degrades to the correct raw
            // script rather than panicking if that bypass ever moves.
            ActionOutput::Completions { shell } => crate::completions::completions(*shell),
            ActionOutput::ProfileAdd(result) => render_success(profile, result, compact),
            ActionOutput::ProfileList(result) => render_success(profile, result, compact),
            ActionOutput::ProfileUse(result) => render_success(profile, result, compact),
            ActionOutput::ProjectsList(result) => render_success(profile, result, compact),
            ActionOutput::ProjectNew(result) => render_success(profile, result, compact),
            ActionOutput::ProjectCopy(result) => render_success(profile, result, compact),
            ActionOutput::ProjectRename(result) => render_success(profile, result, compact),
            ActionOutput::ProjectSet(result) => render_success(profile, result, compact),
            ActionOutput::ProjectDelete(result) => render_success(profile, result, compact),
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
    let (profile, result) = runtime.block_on(dispatch(cli, mode));
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
async fn dispatch(cli: Cli, mode: RenderMode) -> (Option<String>, Result<ActionOutput, CoreError>) {
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
        // The inspection commands (02-02): authed reads of a healthy
        // gateway. Credential REQUIRED — resolve_secret, not the
        // header-less degradation `version` uses: these commands cannot
        // work unauthenticated, so a missing secret is SecretUnavailable
        // (exit 3), the correct taxonomy, not a doomed 401.
        Commands::Status => {
            run_inspection(&mut config, cli.profile.as_deref(), Inspection::Status).await
        }
        Commands::Modules { quarantined } => {
            run_inspection(
                &mut config,
                cli.profile.as_deref(),
                Inspection::Modules(quarantined),
            )
            .await
        }
        Commands::Metrics { history } => {
            run_inspection(
                &mut config,
                cli.profile.as_deref(),
                Inspection::Metrics(history),
            )
            .await
        }
        // Sessions (02-03): the merged list is a plain authed read; the
        // terminate half is the CLI's FIRST DESTRUCTIVE COMMAND — the
        // guard refuses (exit 2, confirmation_required) BEFORE any API
        // construction, so a refusal costs nothing and never depends on
        // config/profile state (usage-class errors lead, like clap's).
        Commands::Sessions(SessionsArgs { r#type, command }) => match command {
            None => {
                let (name, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::sessions::sessions(&api, r#type.map(Into::into))
                        .await
                        .map(ActionOutput::Sessions),
                    Err(err) => Err(err),
                };
                (name, result)
            }
            Some(SessionsCmd::Terminate {
                r#type,
                id,
                message,
            }) => {
                if let Err(err) = require_confirmation(cli.yes, "sessions terminate") {
                    return (None, Err(err));
                }
                let (name, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::sessions::terminate_session(
                        &api,
                        r#type.into(),
                        &id,
                        message.as_deref(),
                    )
                    .await
                    .map(ActionOutput::SessionsTerminate),
                    Err(err) => Err(err),
                };
                (name, result)
            }
        },
        // Connections (02-03): authed read of the resource lists.
        Commands::Connections { r#type } => {
            let (name, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
            let result = match api {
                Ok(api) => actions::connections::connections(&api, r#type.map(Into::into))
                    .await
                    .map(ActionOutput::Connections),
                Err(err) => Err(err),
            };
            (name, result)
        }
        // Logs (02-04, HLTH-03/04). The list/tail arm STREAMS: tail
        // entries print to stdout as they arrive — human lines or
        // NDJSON, one compact entry per line with NO envelope (the
        // streaming exception, README-documented; render_ok prints
        // nothing further for LogsTail). The set/reset mutations are
        // --yes-guarded BEFORE any API construction (the sessions
        // terminate precedent: a refusal costs nothing).
        Commands::Logs(LogsArgs {
            logger,
            min_level,
            since,
            limit,
            follow,
            interval,
            timeout,
            command,
        }) => match command {
            None => {
                let (name, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
                let result = match api {
                    Ok(api) if follow => {
                        let min_level = min_level.map(LogLevel::wire);
                        let header = name.clone();
                        let mut first = true;
                        let sink: &mut dyn FnMut(&ignition_core::client::logs::LogEntry) =
                            &mut |entry| match mode {
                                RenderMode::Human => {
                                    if first {
                                        if let Some(name) = &header {
                                            println!("[profile: {name}]");
                                        }
                                        first = false;
                                    }
                                    println!("{}", render_log_entry_line(entry));
                                }
                                // NDJSON — the streaming exception: one
                                // compact entry object per line, no
                                // envelope (README §Streaming).
                                RenderMode::PrettyJson | RenderMode::CompactJson => {
                                    println!(
                                        "{}",
                                        serde_json::to_string(entry)
                                            .expect("a log entry serializes")
                                    );
                                }
                            };
                        actions::logs::tail(
                            &api,
                            logger.as_deref(),
                            min_level,
                            since,
                            std::time::Duration::from_secs(interval),
                            timeout.map(std::time::Duration::from_secs),
                            sink,
                        )
                        .await
                        .map(ActionOutput::LogsTail)
                    }
                    Ok(api) => {
                        let min_level = min_level.map(LogLevel::wire);
                        actions::logs::list_logs(&api, logger.as_deref(), min_level, since, limit)
                            .await
                            .map(ActionOutput::LogsList)
                    }
                    Err(err) => Err(err),
                };
                (name, result)
            }
            Some(LogsCmd::Download { output }) => {
                let (name, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
                let result = match api {
                    Ok(api) => {
                        let stem = name.as_deref().unwrap_or("gateway");
                        actions::logs::download(&api, output.as_deref(), stem)
                            .await
                            .map(ActionOutput::LogsDownload)
                    }
                    Err(err) => Err(err),
                };
                (name, result)
            }
            Some(LogsCmd::Loggers(loggers_args)) => match loggers_args.command {
                None => {
                    let (name, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
                    let result = match api {
                        Ok(api) => actions::logs::loggers(&api, loggers_args.search.as_deref())
                            .await
                            .map(ActionOutput::LoggersList),
                        Err(err) => Err(err),
                    };
                    (name, result)
                }
                Some(LoggersCmd::Set {
                    name: logger,
                    level,
                }) => {
                    if let Err(err) = require_confirmation(cli.yes, "logs loggers set") {
                        return (None, Err(err));
                    }
                    let (name, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
                    let result = match api {
                        Ok(api) => actions::logs::set_logger_level(&api, &logger, level.wire())
                            .await
                            .map(ActionOutput::LoggerSet),
                        Err(err) => Err(err),
                    };
                    (name, result)
                }
                Some(LoggersCmd::Reset) => {
                    if let Err(err) = require_confirmation(cli.yes, "logs loggers reset") {
                        return (None, Err(err));
                    }
                    let (name, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
                    let result = match api {
                        Ok(api) => actions::logs::reset_logger_levels(&api)
                            .await
                            .map(ActionOutput::LoggerReset),
                        Err(err) => Err(err),
                    };
                    (name, result)
                }
            },
        },
        // Restart (02-05, HLTH-09): the phase's one big red button —
        // --yes-guarded ALWAYS (research Pitfall 10: it takes the
        // gateway down; agents pass --yes). The guard fires BEFORE any
        // API construction (the sessions-terminate precedent: a
        // refusal costs nothing and never touches the gateway).
        Commands::Restart {
            wait,
            timeout,
            interval,
        } => {
            if let Err(err) = require_confirmation(cli.yes, "restart") {
                return (None, Err(err));
            }
            let (name, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
            let result = match api {
                Ok(api) => {
                    let interval = interval.map_or(
                        actions::restart::DEFAULT_INTERVAL,
                        std::time::Duration::from_secs,
                    );
                    let timeout = timeout.map_or(
                        actions::restart::RESTART_TIMEOUT,
                        std::time::Duration::from_secs,
                    );
                    if wait {
                        actions::restart::restart_and_wait(
                            &api,
                            interval,
                            timeout,
                            actions::restart::RESTART_FLOOR,
                        )
                        .await
                        .map(ActionOutput::RestartWait)
                    } else {
                        actions::restart::restart(&api)
                            .await
                            .map(ActionOutput::Restart)
                    }
                }
                Err(err) => Err(err),
            };
            (name, result)
        }
        // Wait (02-05, HLTH-11). `gateway` and `restart` dispatch with
        // a HEADER-LESS client (credential resolution degrades to None
        // — the whole point is these work when auth is broken;
        // StatusPing answers unauthenticated, even mid-restart).
        // `wait module` is an authed read (modules needs a token).
        Commands::Wait(WaitArgs { command }) => match command {
            WaitCmd::Gateway { interval, timeout } => {
                let (name, api) = resolve_headerless_api(&mut config, cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::restart::wait_gateway(
                        &api,
                        std::time::Duration::from_secs(interval),
                        std::time::Duration::from_secs(timeout),
                    )
                    .await
                    .map(ActionOutput::Wait),
                    Err(err) => Err(err),
                };
                (name, result)
            }
            WaitCmd::Restart { interval, timeout } => {
                // Restart-aware (research line 94 + Open Question 4):
                // observing non-RUNNING once → RUNNING completes with
                // NO floor wait; an all-RUNNING sequence is accepted
                // only past the SAME 5 s floor as `restart --wait` —
                // running right after `ign restart` cannot
                // false-positive on the ~5 s grace window.
                let (name, api) = resolve_headerless_api(&mut config, cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::restart::wait_restart(
                        &api,
                        std::time::Duration::from_secs(interval),
                        std::time::Duration::from_secs(timeout),
                        actions::restart::RESTART_FLOOR,
                    )
                    .await
                    .map(ActionOutput::Wait),
                    Err(err) => Err(err),
                };
                (name, result)
            }
            WaitCmd::Module {
                id,
                interval,
                timeout,
            } => {
                let (name, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::restart::wait_module(
                        &api,
                        &id,
                        std::time::Duration::from_secs(interval),
                        std::time::Duration::from_secs(timeout),
                    )
                    .await
                    .map(ActionOutput::Wait),
                    Err(err) => Err(err),
                };
                (name, result)
            }
        },
        // Doctor (02-05, HLTH-10): the self-service preflight. NEVER
        // errors on failing CHECKS — the diagnosis completing IS the
        // success (exit 0; agents parse checks[], humans read the
        // table — README-documented). Only config-class problems (no
        // profile) exit through the normal path. The credential
        // DEGRADES to header-less: doctor diagnoses broken/absent auth
        // for a living, so it must run without one (a 401 is then
        // honestly reported as "no credential resolved").
        Commands::Doctor {
            check_write,
            webdev_route,
        } => match resolve_profile_context(&mut config, cli.profile.as_deref()) {
            Ok(None) => (None, Err(CoreError::NoActiveProfile)),
            Ok(Some((name, profile))) => {
                let credential = resolve_secret_opt(&name, &profile.auth);
                match credential {
                    Ok(credential) => {
                        let credential_present = credential.is_some();
                        match ReqwestGatewayApi::new(&profile, credential) {
                            Ok(api) => {
                                let opts = actions::doctor::DoctorOptions {
                                    check_write,
                                    webdev_route,
                                };
                                let result = actions::doctor::doctor(
                                    &api,
                                    profile.url.as_str(),
                                    credential_present,
                                    &opts,
                                )
                                .await;
                                (Some(name), Ok(ActionOutput::Doctor(result)))
                            }
                            Err(err) => (Some(name), Err(err)),
                        }
                    }
                    Err(err) => (Some(name), Err(err)),
                }
            }
            Err(err) => (None, Err(err)),
        },
        // Projects (03-01, PROJ-01/02): the first project-family
        // commands. All arms are authed (inspection-command rule: exit
        // 3 without a credential). Delete is the family's ONE
        // destructive verb — the sessions-terminate shape VERBATIM
        // (LOCKED 02-03): the guard fires BEFORE resolve_gateway_api,
        // so a refusal exits 2 with profile null and does ZERO
        // config/secret/network work. Copy/rename/set create or
        // relabel, never destroy — NO --yes (planner decision).
        Commands::Project(ProjectArgs { command }) => match command {
            ProjectCommand::List => {
                let (profile, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::projects::projects(&api)
                        .await
                        .map(ActionOutput::ProjectsList),
                    Err(err) => Err(err),
                };
                (profile, result)
            }
            ProjectCommand::New {
                name: project_name,
                title,
                description,
                parent,
                inheritable,
                disabled,
            } => {
                let (profile, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
                let result = match api {
                    Ok(api) => {
                        let opts = actions::projects::NewOptions {
                            enabled: !disabled,
                            title,
                            description,
                            parent,
                            inheritable: inheritable.then_some(true),
                        };
                        actions::projects::project_new(&api, &project_name, &opts)
                            .await
                            .map(ActionOutput::ProjectNew)
                    }
                    Err(err) => Err(err),
                };
                (profile, result)
            }
            ProjectCommand::Copy { src, dst } => {
                let (profile, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::projects::project_copy(&api, &src, &dst)
                        .await
                        .map(ActionOutput::ProjectCopy),
                    Err(err) => Err(err),
                };
                (profile, result)
            }
            ProjectCommand::Rename { old_name, new_name } => {
                let (profile, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::projects::project_rename(&api, &old_name, &new_name)
                        .await
                        .map(ActionOutput::ProjectRename),
                    Err(err) => Err(err),
                };
                (profile, result)
            }
            ProjectCommand::Set {
                name: project_name,
                title,
                description,
                parent,
                set_enabled,
                disabled,
                inheritable,
            } => {
                let (profile, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
                let result = match api {
                    Ok(api) => {
                        // --set-enabled → Some(true), --disabled →
                        // Some(false), neither → None (don't touch).
                        let enabled = if set_enabled {
                            Some(true)
                        } else if disabled {
                            Some(false)
                        } else {
                            None
                        };
                        let opts = actions::projects::SetOptions {
                            title,
                            description,
                            parent,
                            enabled,
                            inheritable,
                        };
                        actions::projects::project_set(&api, &project_name, &opts)
                            .await
                            .map(ActionOutput::ProjectSet)
                    }
                    Err(err) => Err(err),
                };
                (profile, result)
            }
            ProjectCommand::Delete { name: project_name } => {
                // The sessions-terminate shape VERBATIM: the guard
                // refuses (exit 2, confirmation_required, profile null)
                // BEFORE any profile/secret/client resolution — a
                // refusal costs nothing and never touches the gateway.
                if let Err(err) = require_confirmation(cli.yes, "project delete") {
                    return (None, Err(err));
                }
                let (profile, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::projects::project_delete(&api, &project_name)
                        .await
                        .map(ActionOutput::ProjectDelete),
                    Err(err) => Err(err),
                };
                (profile, result)
            }
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

/// Which inspection action a dispatch arm runs.
enum Inspection {
    Status,
    Modules(bool),
    Metrics(bool),
}

/// Shared tail of the authed inspection commands: resolve profile +
/// REQUIRED credential + client (post-overlay profile at the
/// construction site), then run the action. The profile name travels
/// out even on failure so the error envelope echoes it (CORE-01).
async fn run_inspection(
    config: &mut Config,
    flag: Option<&str>,
    inspection: Inspection,
) -> (Option<String>, Result<ActionOutput, CoreError>) {
    let (name, api) = resolve_gateway_api(config, flag);
    let result = match api {
        Ok(api) => match inspection {
            Inspection::Status => actions::inspect::status(&api)
                .await
                .map(ActionOutput::Status),
            Inspection::Modules(quarantined) => actions::inspect::modules(&api, quarantined)
                .await
                .map(ActionOutput::Modules),
            Inspection::Metrics(history) => actions::inspect::metrics(&api, history)
                .await
                .map(ActionOutput::Metrics),
        },
        Err(err) => Err(err),
    };
    (name, result)
}

/// Profile + REQUIRED credential + client for gateway commands. `None`
/// profile → `NoActiveProfile` (these commands cannot run without a
/// target); the LOCKED secret chain with NO degradation — a missing
/// secret is `SecretUnavailable` (exit 3), correct for authed reads.
fn resolve_gateway_api(
    config: &mut Config,
    flag: Option<&str>,
) -> (Option<String>, Result<ReqwestGatewayApi, CoreError>) {
    match resolve_profile_context(config, flag) {
        Ok(None) => (None, Err(CoreError::NoActiveProfile)),
        Ok(Some((name, profile))) => {
            let credential = config::resolve_secret(&name, &profile.auth, &secret_chain());
            let result = credential
                .and_then(|credential| ReqwestGatewayApi::new(&profile, Some(credential)));
            (Some(name), result)
        }
        Err(err) => (None, Err(err)),
    }
}

/// Profile + HEADER-LESS-tolerant client for the unauthenticated wait
/// commands (`wait gateway`, `wait restart`): credential resolution
/// DEGRADES to None — StatusPing answers with no credential, so a
/// missing/broken secret must never block readiness polling (the whole
/// point: these waits work when auth is broken). Other credential
/// errors still propagate.
fn resolve_headerless_api(
    config: &mut Config,
    flag: Option<&str>,
) -> (Option<String>, Result<ReqwestGatewayApi, CoreError>) {
    match resolve_profile_context(config, flag) {
        Ok(None) => (None, Err(CoreError::NoActiveProfile)),
        Ok(Some((name, profile))) => {
            let credential = resolve_secret_opt(&name, &profile.auth);
            let result =
                credential.and_then(|credential| ReqwestGatewayApi::new(&profile, credential));
            (Some(name), result)
        }
        Err(err) => (None, Err(err)),
    }
}

/// The LOCKED secret chain (env tokens → keyring → basic pair), built in
/// exactly one place.
fn secret_chain() -> Vec<Box<dyn SecretStore>> {
    vec![
        Box::new(config::EnvStore),
        Box::new(config::KeyringStore),
        Box::new(config::BasicEnvStore),
    ]
}

/// Credential resolution degraded for non-authenticating commands: the
/// LOCKED chain (env tokens → keyring → basic pair) with
/// `SecretUnavailable` mapped to `Ok(None)` — version proceeds header-less
/// (gateway-info is `auth: none`) instead of demanding a secret. Every
/// OTHER credential error propagates.
fn resolve_secret_opt(profile: &str, auth: &AuthRef) -> Result<Option<Credential>, CoreError> {
    config::resolve_secret(profile, auth, &secret_chain())
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

/// CORE-06 pattern, PROVEN in production by `ign sessions terminate`
/// (02-03 — the first destructive caller; the Phase-1 `expect(dead_code)`
/// gate came off in the same commit it gained that caller). Later
/// destructive operations inherit this guard verbatim (`project delete`
/// in Phase 3, `rig reset` in Phase 4): destructive commands refuse
/// without `--yes` — which already merges `IGNITION_YES` via
/// [`apply_env_defaults`] — with a usage-class error (exit 2: it names
/// a flag the caller must add) whose hint says exactly that. Pinned here
/// in main.rs, no separate confirm.rs file.
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
