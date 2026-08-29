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

mod completions;
mod render;

#[cfg(feature = "tui")]
use std::io::IsTerminal;
use std::process::ExitCode;

use clap::Parser;

use ignition_core::actions;
use ignition_core::client::ReqwestGatewayApi;
use ignition_core::config::{self, AuthRef, Config, Credential, SecretStore};
use ignition_core::error::CoreError;

// The command tree lives in the crate's lib target (shared with the
// integration tests — the TUI-coverage walk needs `Cli::command()`).
use crate::render::{RenderMode, render_error, render_log_entry_line, render_ok};
use ignition_cli::cli;
use ignition_cli::cli::{
    BackupArgs, BackupCommand, Cli, Commands, EamArgs, EamCommand, EamTaskCommand, LintArgs,
    LogLevel, LoggersCmd, LogsArgs, LogsCmd, ProfileArgs, ProfileCmd, ProjectArgs, ProjectCommand,
    ResourceArgs, ResourceCommand, RigArgs, RigCommand, ScheduleMode, ScriptArgs, ScriptCommand,
    SessionsArgs, SessionsCmd, TagsAlarmsCommand, TagsArgs, TagsCommand, TagsConfigCommand,
    TagsHistoryCommand, TagsProviderCommand, TagsUdtCommand, WaitArgs, WaitCmd, WebdevArgs,
    WebdevCommand,
};

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
    /// `ign project export` — ZIP streamed to disk; data carries
    /// {project, file, bytes, scope}.
    ProjectExport(actions::projects::ExportResult),
    /// `ign project export --decode-scripts` — the member tree +
    /// sidecars + manifest directory; data carries {project, dir,
    /// members, scripts_decoded, bytes, scope}.
    ProjectExportDecoded(actions::projects::ExportDecodedResult),
    /// `ign project import` — buffered upload with collision policy;
    /// data carries {name, collision_policy, bytes, scope, outcome}.
    ProjectImport(actions::projects::ImportResult),
    /// `ign project diff` — the cross-gateway normalized member
    /// compare; data carries {scope, profile_a, profile_b, project,
    /// project_meta, summary, entries}.
    ProjectDiff(actions::projects::ProjectDiffResult),
    /// `ign project sync` — the guarded cross-gateway promotion; data
    /// carries {scope, profile_a, profile_b, project, synced,
    /// removed}.
    ProjectSync(actions::projects::ProjectSyncResult),
    /// `ign resource list` — a project's resources (passthrough
    /// entries).
    ResourcesList(actions::resources::ResourcesResult),
    /// `ign resource get` — one resource's sniffed content
    /// ({project, path, content_kind, content}).
    ResourceGet(actions::resources::ResourceGetResult),
    /// `ign resource put` — the surgical upsert.
    ResourcePut(actions::resources::ResourcePutResult),
    /// `ign resource delete` — the surgical loop's destructive verb.
    ResourceDelete(actions::resources::ResourceDeleteResult),
    /// `ign rig up` — compose up + commissioned wait; uncommissioned
    /// arrives as DATA (exit 0, wizard hint in warnings).
    RigUp(actions::rig::RigUpResult),
    /// `ign rig down` — compose down (volumes kept).
    RigDown(actions::rig::RigDownResult),
    /// `ign rig reset` — guarded volume teardown + fresh bring-up.
    RigReset(actions::rig::RigResetResult),
    /// `ign rig status` — the allowlist status (docker-only family).
    RigStatus(actions::rig::RigStatusResult),
    /// `ign rig logs` — the lines ALREADY streamed to stdout raw (the
    /// third sanctioned stdout exception, README-documented); render
    /// prints nothing further.
    RigLogs(actions::rig::RigLogsResult),
    RigSnapshot(actions::rig::SnapshotResult),
    RigRestore(actions::rig::RestoreResult),
    /// `ign backup download` — the standalone gwbk streamed to disk
    /// (07-02, BKUP-01); data carries {file, type}.
    BackupDownload(actions::backup::BackupDownloadResult),
    /// `ign backup restore` — the guarded standalone restore; data
    /// carries the flat {restored: true}.
    BackupRestore(actions::backup::BackupRestoreResult),
    /// `ign eam history` — task run history (items passthrough +
    /// count).
    EamHistory(actions::eam::EamHistoryResult),
    /// `ign eam tasks` — the definition summaries.
    EamTasks(actions::eam::EamTasksResult),
    /// `ign eam tasks <NAME>` — one definition + its state.
    EamTaskDetail(actions::eam::EamTaskDetailResult),
    /// `ign eam task new` — the guarded create; data carries the
    /// composed definition verbatim.
    EamTaskCreate(actions::eam::EamTaskCreateResult),
    /// `ign eam task force` — the guarded dispatch + the honest
    /// history read-back.
    EamTaskForce(actions::eam::EamTaskForceResult),
    /// `ign script run` — the scriptExec answer under unit-explicit
    /// keys {stdout, result, elapsedMs} (ALL keys always; the
    /// secret never rides any output path).
    ScriptRun(actions::script::ScriptRunResult),
    /// `ign lint` — the doctor-posture delegation result; exit 0
    /// whenever the child ran, findings + child_exit_code + the
    /// parsed report as data (`--strict`'s passthrough is decided
    /// in `main` AFTER the envelope renders).
    Lint(actions::lint::LintResult),
    /// `ign rig trial status` — the credential-free trial truth +
    /// banners cross-check (04-03).
    RigTrialStatus(actions::rig::TrialStatusResult),
    /// `ign rig trial reset` — the ladder's outcome: mechanism +
    /// before/after flip (04-03).
    RigTrialReset(actions::rig::TrialResetResult),
    /// `ign webdev deploy` — the embedded bundle installed (routes +
    /// import outcome; the scriptExec secret NEVER rides any output).
    WebdevDeploy(actions::webdev::WebdevDeployResult),
    /// `ign webdev status` — the per-route version-handshake sweep
    /// (degradation is data; exit 0 whenever the sweep completes).
    WebdevStatus(actions::webdev::WebdevStatusResult),
    /// `ign tags provider list` — the native provider rows (tag
    /// counts + health, System flagged managed).
    TagProviders(actions::tags::TagProvidersResult),
    /// `ign tags provider create` — a STANDARD provider created.
    TagProviderCreate(actions::tags::TagProviderCreateResult),
    /// `ign tags provider delete` — the signature-chained delete.
    TagProviderDelete(actions::tags::TagProviderDeleteResult),
    /// `ign tags browse` — the filtered flat entry list (JSON mode;
    /// human renders the tree).
    TagsBrowse(actions::tags::TagsBrowseResult),
    /// `ign tags browse --from-export` — the OFFLINE rows (the same
    /// BrowseRow shape; profile null, no gateway).
    TagsBrowseFromExport(actions::tags::TagBrowseFromExportResult),
    /// `ign tags read` — verbatim per-path rows.
    TagsRead(actions::tags::TagsReadResult),
    /// `ign tags write` — the post-write quality.
    TagsWrite(actions::tags::TagsWriteResult),
    /// `ign tags config get` — the re-parsed config dict.
    TagsConfigGet(actions::tags::TagsConfigGetResult),
    /// `ign tags config create|edit` — the configure quality.
    TagsConfigCreate(actions::tags::TagsConfigWriteResult),
    /// `ign tags config edit` — the configure quality.
    TagsConfigEdit(actions::tags::TagsConfigWriteResult),
    /// `ign tags config delete` — the echoed count.
    TagsConfigDelete(actions::tags::TagsConfigDeleteResult),
    /// `ign tags udt types` — the provider's type rows.
    TagsUdtTypes(actions::tags::TagsUdtTypesResult),
    /// `ign tags udt def` — the recursive definition.
    TagsUdtDef(actions::tags::TagsUdtDefResult),
    /// `ign tags export` — the artifact line's data (stdout mode is
    /// intercepted in render_ok — the payload already printed).
    TagsExport(actions::tags::TagsExportResult),
    /// `ign tags import` — counts + provider.
    TagsImport(actions::tags::TagsImportResult),
    /// `ign tags alarms active` — the active alarm rows.
    TagsAlarmsActive(actions::tags::TagsAlarmsActiveResult),
    /// `ign tags alarms history` — the journal rows (journal-shape
    /// dependent, verbatim).
    TagsAlarmsHistory(actions::tags::TagsAlarmsHistoryResult),
    /// `ign tags alarms ack` — the honest count + remainder.
    TagsAlarmsAck(actions::tags::TagsAlarmsAckResult),
    /// `ign tags history query` — the dataset with t_stamp
    /// preserved exactly.
    TagsHistoryQuery(actions::tags::TagsHistoryQueryResult),
    /// `ign tui` — the cockpit ran and exited. Renders NOTHING in every
    /// mode (LOCKED stdout decision: the TUI owns the alternate screen
    /// and prints nothing on success; errors after restore flow the
    /// normal stderr envelope + exit taxonomy).
    #[cfg(feature = "tui")]
    TuiExited,
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
            ActionOutput::ProjectExport(result) => render_success(profile, result, compact),
            ActionOutput::ProjectExportDecoded(result) => render_success(profile, result, compact),
            ActionOutput::ProjectImport(result) => render_success(profile, result, compact),
            ActionOutput::ProjectDiff(result) => render_success(profile, result, compact),
            ActionOutput::ProjectSync(result) => render_success(profile, result, compact),
            ActionOutput::ResourcesList(result) => render_success(profile, result, compact),
            ActionOutput::ResourceGet(result) => render_success(profile, result, compact),
            ActionOutput::ResourcePut(result) => render_success(profile, result, compact),
            ActionOutput::ResourceDelete(result) => render_success(profile, result, compact),
            ActionOutput::RigUp(result) => render_success(profile, result, compact),
            ActionOutput::RigDown(result) => render_success(profile, result, compact),
            ActionOutput::RigReset(result) => render_success(profile, result, compact),
            ActionOutput::RigStatus(result) => render_success(profile, result, compact),
            // Unreachable in practice (render_ok intercepts RigLogs
            // before mode dispatch — the lines already streamed).
            ActionOutput::RigLogs(result) => render_success(profile, result, compact),
            ActionOutput::RigSnapshot(result) => render_success(profile, result, compact),
            ActionOutput::RigRestore(result) => render_success(profile, result, compact),
            ActionOutput::BackupDownload(result) => render_success(profile, result, compact),
            ActionOutput::BackupRestore(result) => render_success(profile, result, compact),
            ActionOutput::EamHistory(result) => render_success(profile, result, compact),
            ActionOutput::EamTasks(result) => render_success(profile, result, compact),
            ActionOutput::EamTaskDetail(result) => render_success(profile, result, compact),
            ActionOutput::EamTaskCreate(result) => render_success(profile, result, compact),
            ActionOutput::EamTaskForce(result) => render_success(profile, result, compact),
            ActionOutput::ScriptRun(result) => render_success(profile, result, compact),
            ActionOutput::Lint(result) => render_success(profile, result, compact),
            ActionOutput::RigTrialStatus(result) => render_success(profile, result, compact),
            ActionOutput::RigTrialReset(result) => render_success(profile, result, compact),
            ActionOutput::WebdevDeploy(result) => render_success(profile, result, compact),
            ActionOutput::WebdevStatus(result) => render_success(profile, result, compact),
            ActionOutput::TagProviders(result) => render_success(profile, result, compact),
            ActionOutput::TagProviderCreate(result) => render_success(profile, result, compact),
            ActionOutput::TagProviderDelete(result) => render_success(profile, result, compact),
            ActionOutput::TagsBrowse(result) => render_success(profile, result, compact),
            ActionOutput::TagsBrowseFromExport(result) => render_success(profile, result, compact),
            ActionOutput::TagsRead(result) => render_success(profile, result, compact),
            ActionOutput::TagsWrite(result) => render_success(profile, result, compact),
            ActionOutput::TagsConfigGet(result) => render_success(profile, result, compact),
            ActionOutput::TagsConfigCreate(result) => render_success(profile, result, compact),
            ActionOutput::TagsConfigEdit(result) => render_success(profile, result, compact),
            ActionOutput::TagsConfigDelete(result) => render_success(profile, result, compact),
            ActionOutput::TagsUdtTypes(result) => render_success(profile, result, compact),
            ActionOutput::TagsUdtDef(result) => render_success(profile, result, compact),
            // Unreachable in practice (render_ok intercepts the
            // stdout-mode export before mode dispatch — the payload
            // already printed raw).
            ActionOutput::TagsExport(result) => render_success(profile, result, compact),
            ActionOutput::TagsImport(result) => render_success(profile, result, compact),
            ActionOutput::TagsAlarmsActive(result) => render_success(profile, result, compact),
            ActionOutput::TagsAlarmsHistory(result) => render_success(profile, result, compact),
            ActionOutput::TagsAlarmsAck(result) => render_success(profile, result, compact),
            ActionOutput::TagsHistoryQuery(result) => render_success(profile, result, compact),
            // Unreachable in practice (render_ok intercepts TuiExited
            // before mode dispatch — the cockpit prints nothing).
            #[cfg(feature = "tui")]
            ActionOutput::TuiExited => String::new(),
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
            // The ONE sanctioned success-path EXIT exception (07-04,
            // INTR-02): `ign lint --strict` exits with the child's
            // code LITERALLY for CI pipelines — the envelope rendered
            // above first (documented with the lint section; exit 1 =
            // findings at/above the tool's --fail-on threshold).
            if let ActionOutput::Lint(lint) = &out
                && let Some(code) = lint.strict_exit_code()
            {
                return ExitCode::from(code);
            }
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
                        let sink: &mut (dyn FnMut(&ignition_core::client::logs::LogEntry) + Send) =
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
            // Export (03-02, PROJ-03): non-destructive — it writes a
            // LOCAL file — so no --yes. The file is the artifact and
            // stdout stays data-only (no stdout exception); human mode
            // gets a one-line progress note on STDERR while the ZIP
            // streams (stdout is reserved for the envelope).
            // 07-04: `--decode-scripts` routes to the decoded-tree
            // action (same streaming seam, then the PURE codec owns
            // the directory write).
            ProjectCommand::Export {
                name: project_name,
                output,
                decode_scripts,
            } => {
                if mode == RenderMode::Human {
                    eprintln!("exporting {project_name} …");
                }
                let (profile, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
                let result = match api {
                    Ok(api) => {
                        if decode_scripts {
                            actions::projects::project_export_decoded(
                                &api,
                                &project_name,
                                output.as_deref(),
                            )
                            .await
                            .map(ActionOutput::ProjectExportDecoded)
                        } else {
                            actions::projects::project_export(
                                &api,
                                &project_name,
                                output.as_deref(),
                            )
                            .await
                            .map(ActionOutput::ProjectExport)
                        }
                    }
                    Err(err) => Err(err),
                };
                (profile, result)
            }
            // Import (03-02, PROJ-04). Overwrite is DESTRUCTIVE (it
            // replaces the entire project — Pitfall 4): the guard
            // fires BEFORE resolve_gateway_api (exit 2, profile null,
            // zero work — the LOCKED shape). Abort-policy imports skip
            // the guard: they fail safely server-side. The dispatch
            // layer owns the byte source — `--file PATH` via std::fs,
            // `--file -` via tokio stdin — then the action owns the
            // magic/size guards and the collision pre-check.
            ProjectCommand::Import {
                name: project_name,
                file,
                collision_policy,
                encode_scripts,
            } => {
                if matches!(collision_policy, cli::CollisionPolicy::Overwrite)
                    && let Err(err) =
                        require_confirmation(cli.yes, "project import --collision-policy overwrite")
                {
                    return (None, Err(err));
                }
                // 07-04: `--encode-scripts` re-zips the decoded
                // DIRECTORY first (the byte source is a tree, not an
                // archive) — the re-encoded zip then rides the
                // standard import path verbatim, so validate_import's
                // full-structure walk applies free. Stdin cannot
                // carry a directory (usage-class, pre-resolution).
                let zip = if encode_scripts {
                    if file == "-" {
                        return (
                            None,
                            Err(CoreError::InvalidInput {
                                reason: "--encode-scripts needs the decoded export \
                                         DIRECTORY via --file (stdin cannot carry one)"
                                    .to_string(),
                            }),
                        );
                    }
                    match ignition_core::client::scripts_codec::encode_export_tree(
                        std::path::Path::new(&file),
                    ) {
                        Ok(zip) => zip,
                        Err(err) => return (None, Err(err)),
                    }
                } else if file == "-" {
                    use tokio::io::AsyncReadExt;
                    let mut buffer = Vec::new();
                    match tokio::io::stdin().read_to_end(&mut buffer).await {
                        Ok(_) => buffer,
                        Err(err) => {
                            return (
                                None,
                                Err(CoreError::InvalidImportFile {
                                    reason: format!("cannot read stdin: {err}"),
                                }),
                            );
                        }
                    }
                } else {
                    match std::fs::read(&file) {
                        Ok(bytes) => bytes,
                        Err(err) => {
                            return (
                                None,
                                Err(CoreError::InvalidImportFile {
                                    reason: format!("cannot read {file}: {err}"),
                                }),
                            );
                        }
                    }
                };
                let (profile, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::projects::project_import(
                        &api,
                        &project_name,
                        zip,
                        collision_policy.into(),
                    )
                    .await
                    .map(ActionOutput::ProjectImport),
                    Err(err) => Err(err),
                };
                (profile, result)
            }
            // Diff (07-01, SYNC-01): cross-gateway compare — a READ,
            // no guard. The envelope's active profile resolves
            // UNCHANGED (flag > active, env overlay scoped to it);
            // both positional sides then resolve through the
            // two-client shape (each side its own client + secret
            // chain — see resolve_two_clients).
            ProjectCommand::Diff {
                profile_a,
                profile_b,
                project: project_name,
            } => {
                let (active, sides) = resolve_two_clients(
                    &mut config,
                    cli.profile.as_deref(),
                    &profile_a,
                    &profile_b,
                );
                let result = match sides {
                    Ok((api_a, api_b)) => actions::projects::project_diff(
                        &api_a,
                        &api_b,
                        &project_name,
                        &profile_a,
                        &profile_b,
                    )
                    .await
                    .map(ActionOutput::ProjectDiff),
                    Err(err) => Err(err),
                };
                (active, result)
            }
            // Sync (07-01, SYNC-02): the guarded promotion — usage
            // errors lead (a selection-less sync refuses exit 2 with
            // profile null BEFORE the guard, the resource-put
            // precedent), then the --yes guard fires BEFORE
            // resolve_two_clients: a refusal is exit 2 with profile
            // null and does ZERO config/secret/network work, its
            // operation string naming the whole-project
            // overwrite-import consequence on B (the shared
            // ConfirmationRequired hint stays frozen).
            ProjectCommand::Sync {
                profile_a,
                profile_b,
                project: project_name,
                resource,
                all_changed,
                delete,
            } => {
                if resource.is_empty() && !all_changed {
                    return (
                        None,
                        Err(CoreError::InvalidInput {
                            reason: "sync needs a selection — pass --resource PATH \
                                     (repeatable) and/or --all-changed"
                                .to_string(),
                        }),
                    );
                }
                if let Err(err) = require_confirmation(
                    cli.yes,
                    &format!(
                        "project sync (overwrite-import the whole project on {profile_b} — \
                         replaces concurrent Designer edits)"
                    ),
                ) {
                    return (None, Err(err));
                }
                let (active, sides) = resolve_two_clients(
                    &mut config,
                    cli.profile.as_deref(),
                    &profile_a,
                    &profile_b,
                );
                let result = match sides {
                    Ok((api_a, api_b)) => {
                        let selection = actions::projects::SyncSelection {
                            resources: resource,
                            all_changed,
                        };
                        actions::projects::project_sync(
                            &api_a,
                            &api_b,
                            &project_name,
                            &selection,
                            delete,
                            &profile_a,
                            &profile_b,
                        )
                        .await
                        .map(ActionOutput::ProjectSync)
                    }
                    Err(err) => Err(err),
                };
                (active, result)
            }
        },
        // Resources (05-02 re-point): the surgical edit loop riding
        // project-export ZIP surgery. All arms are authed
        // (inspection-command rule: exit 3 without a credential).
        // Delete AND put are the family's guarded verbs: every
        // mutation now implicitly OVERWRITE-IMPORTS the whole project
        // (replace-not-merge wipes concurrent Designer edits), so the
        // LOCKED guard shape fires BEFORE resolve_gateway_api (a
        // refusal exits 2 with profile null and does ZERO work). The
        // dispatch layer owns the byte source (`--file PATH` via
        // std::fs, `--file -` via tokio stdin — the unchanged
        // InvalidInput path), the action owns the sniff + surgery.
        Commands::Resource(ResourceArgs { command }) => match command {
            ResourceCommand::List { project, prefix } => {
                let (profile, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
                let result = match api {
                    Ok(api) => {
                        actions::resources::resources_list(&api, &project, prefix.as_deref())
                            .await
                            .map(ActionOutput::ResourcesList)
                    }
                    Err(err) => Err(err),
                };
                (profile, result)
            }
            ResourceCommand::Get { project, path } => {
                let (profile, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::resources::resource_get(&api, &project, &path)
                        .await
                        .map(ActionOutput::ResourceGet),
                    Err(err) => Err(err),
                };
                (profile, result)
            }
            ResourceCommand::Put {
                project,
                path,
                file,
            } => {
                let input = if file == "-" {
                    use tokio::io::AsyncReadExt;
                    let mut buffer = Vec::new();
                    match tokio::io::stdin().read_to_end(&mut buffer).await {
                        Ok(_) => buffer,
                        Err(err) => {
                            return (
                                None,
                                Err(CoreError::InvalidInput {
                                    reason: format!("cannot read stdin: {err}"),
                                }),
                            );
                        }
                    }
                } else {
                    match std::fs::read(&file) {
                        Ok(bytes) => bytes,
                        Err(err) => {
                            return (
                                None,
                                Err(CoreError::InvalidInput {
                                    reason: format!("cannot read {file}: {err}"),
                                }),
                            );
                        }
                    }
                };
                // The 05-02 guard: put overwrite-imports the WHOLE
                // project — a refusal costs nothing and never touches
                // the gateway (the resource-delete shape verbatim).
                // The operation string names the consequence: the
                // replace-not-merge import wipes concurrent Designer
                // edits (research's accepted-tradeoff language).
                if let Err(err) = require_confirmation(
                    cli.yes,
                    "resource put (re-imports the project; concurrent Designer edits are replaced)",
                ) {
                    return (None, Err(err));
                }
                let (profile, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::resources::resource_put(&api, &project, &path, input)
                        .await
                        .map(ActionOutput::ResourcePut),
                    Err(err) => Err(err),
                };
                (profile, result)
            }
            ResourceCommand::Delete { project, path } => {
                // The LOCKED destructive shape: the guard refuses
                // (exit 2, confirmation_required, profile null)
                // BEFORE any profile/secret/client resolution — a
                // refusal costs nothing and never touches the gateway.
                // 05-02: the operation string names the consequence —
                // delete re-imports the project without the member.
                if let Err(err) = require_confirmation(
                    cli.yes,
                    "resource delete (re-imports the project; concurrent Designer edits are replaced)",
                ) {
                    return (None, Err(err));
                }
                let (profile, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::resources::resource_delete(&api, &project, &path)
                        .await
                        .map(ActionOutput::ResourceDelete),
                    Err(err) => Err(err),
                };
                (profile, result)
            }
        },
        // Webdev (05-03, WEB-01/02): the CLI's own gateway-side
        // surface. Deploy is deliberately NOT --yes-guarded — the
        // dedicated ign-cli project is CLI-OWNED (born from the first
        // deploy zip; overwrite-replace is the contract, README
        // documents; user projects are never touched). This arm
        // resolves the profile MANUALLY (the Doctor precedent):
        // deploy needs the config PATH + profile NAME for the
        // scriptExec secret lifecycle, and status needs the stored
        // secret from the resolved profile. Both are authed commands
        // (required credential, exit 3 without — the
        // inspection-command rule).
        Commands::Webdev(WebdevArgs { command }) => {
            match resolve_profile_context(&mut config, cli.profile.as_deref()) {
                Ok(None) => (None, Err(CoreError::NoActiveProfile)),
                Ok(Some((name, profile))) => {
                    let client = config::resolve_secret(&name, &profile.auth, &secret_chain())
                        .and_then(|credential| ReqwestGatewayApi::new(&profile, Some(credential)));
                    match command {
                        WebdevCommand::Deploy {
                            project,
                            with_script_exec,
                            rotate_secret,
                        } => {
                            let result = match client {
                                Ok(api) => {
                                    if mode == RenderMode::Human {
                                        eprintln!("deploying webdev routes to {project} …");
                                    }
                                    actions::webdev::webdev_deploy(
                                        &api,
                                        &project,
                                        with_script_exec,
                                        rotate_secret,
                                        &path,
                                        &name,
                                    )
                                    .await
                                    .map(ActionOutput::WebdevDeploy)
                                }
                                Err(err) => Err(err),
                            };
                            (Some(name), result)
                        }
                        WebdevCommand::Status { project } => {
                            let result = match client {
                                Ok(api) => actions::webdev::webdev_status(
                                    &api,
                                    &project,
                                    profile.webdev_secret.as_deref(),
                                )
                                .await
                                .map(ActionOutput::WebdevStatus),
                                Err(err) => Err(err),
                            };
                            (Some(name), result)
                        }
                    }
                }
                Err(err) => (None, Err(err)),
            }
        }
        // `rig reset`, `rig trial reset`, and `rig restore` are the
        // family's destructive verbs: their guards fire BEFORE the
        // runner/discovery even exist (the sessions-terminate
        // precedent) — a refusal is exit 2 with profile null and does
        // ZERO discovery work (binary-pinned: exit 2 in a cwd with no
        // rig discoverable at all).
        // Tags (05-04/05-05): provider verbs ride the NATIVE
        // config-resource REST; browse/read/write/config/udt/export/
        // import ride the deployed routes (every one refuses exit 6
        // pre-deploy via the precondition). The destructive verbs —
        // provider delete, config delete, and import-under-overwrite
        // — guard BEFORE resolution (exit 2, profile null, zero
        // work); write's --value parses PRE-resolution; the config
        // create/edit + import byte sources (definition/payload JSON)
        // read PRE-resolution (the resource-put precedent).
        // 05-06 adds alarms active/history/ack + history query:
        // --start/--end parse PRE-resolution (usage errors lead);
        // ack is deliberately NOT guarded (acknowledging never
        // un-acknowledges — a state-advancing read-adjacent verb).
        Commands::Tags(TagsArgs { command }) => {
            // `--from-export` short-circuits BEFORE profile/secret/
            // client/route resolution ENTIRELY (07-04, INTR-03) —
            // offline: no gateway, envelope profile null (the
            // docker-verb precedent for non-gateway commands). The
            // render rides the existing browse paths.
            if let TagsCommand::Browse {
                from_export: Some(export_path),
                filter,
                include_properties,
                ..
            } = &command
            {
                let result = actions::tags::browse_rows_from_export(
                    export_path,
                    *include_properties,
                    filter.as_deref(),
                )
                .map(ActionOutput::TagsBrowseFromExport);
                return (None, result);
            }
            let guard_operation = match &command {
                TagsCommand::Provider(TagsProviderCommand::Delete { .. }) => {
                    Some("tags provider delete")
                }
                TagsCommand::Config(TagsConfigCommand::Delete { .. }) => Some("tags config delete"),
                TagsCommand::Import {
                    collision_policy: cli::CollisionPolicy::Overwrite,
                    ..
                } => Some("tags import --collision-policy overwrite"),
                _ => None,
            };
            if let Some(operation) = guard_operation
                && let Err(err) = require_confirmation(cli.yes, operation)
            {
                return (None, Err(err));
            }
            let write_value = match &command {
                TagsCommand::Write { value, .. } => match parse_write_scalar(value) {
                    Ok(parsed) => Some(parsed),
                    Err(err) => return (None, Err(err)),
                },
                _ => None,
            };
            // The JSON document inputs: config create/edit's
            // definition and import's payload (`--file PATH` /
            // `--file -`, parsed — InvalidInput pre-resolution).
            let json_input = match &command {
                TagsCommand::Config(TagsConfigCommand::Create { file, .. })
                | TagsCommand::Config(TagsConfigCommand::Edit { file, .. })
                | TagsCommand::Import { file, .. } => match read_json_input(file).await {
                    Ok(parsed) => Some(parsed),
                    Err(err) => return (None, Err(err)),
                },
                _ => None,
            };
            // Export's output resolution: `-o -` = stdout (the
            // payload rides the result; render prints it raw — the
            // sanctioned stdout exception), `-o FILE` = that file,
            // none = the default `<last-segment>.json` (the
            // export-streaming convention).
            let export_out = match &command {
                TagsCommand::Export { paths, output, .. } => Some(match output {
                    Some(path) if path == std::path::Path::new("-") => None,
                    Some(path) => Some(path.clone()),
                    None => Some(std::path::PathBuf::from(
                        actions::tags::default_export_file_name(paths),
                    )),
                }),
                _ => None,
            };
            // The time-bearing verbs (alarms history, history
            // query): --start/--end parse to epoch-ms PRE-resolution
            // (RFC3339 or raw digits; the parse_write_scalar
            // precedent — usage errors lead, zero wire work).
            let time_args = match &command {
                TagsCommand::Alarms(TagsAlarmsCommand::History { start, end, .. })
                | TagsCommand::History(TagsHistoryCommand::Query { start, end, .. }) => {
                    match (
                        actions::tags::parse_time_ms(start),
                        actions::tags::parse_time_ms(end),
                    ) {
                        (Ok(start_ms), Ok(end_ms)) => Some((start_ms, end_ms)),
                        (Err(err), _) | (_, Err(err)) => return (None, Err(err)),
                    }
                }
                _ => None,
            };
            let (name, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
            let result = match (&command, api) {
                (TagsCommand::Provider(TagsProviderCommand::List), Ok(api)) => {
                    actions::tags::tag_provider_list(&api)
                        .await
                        .map(ActionOutput::TagProviders)
                }
                (
                    TagsCommand::Provider(TagsProviderCommand::Create { name: provider }),
                    Ok(api),
                ) => actions::tags::tag_provider_create(&api, provider)
                    .await
                    .map(ActionOutput::TagProviderCreate),
                (
                    TagsCommand::Provider(TagsProviderCommand::Delete { name: provider }),
                    Ok(api),
                ) => actions::tags::tag_provider_delete(&api, provider)
                    .await
                    .map(ActionOutput::TagProviderDelete),
                (
                    TagsCommand::Browse {
                        path,
                        filter,
                        include_properties,
                        project,
                        ..
                    },
                    Ok(api),
                ) => actions::tags::tags_browse(
                    &api,
                    project,
                    path.as_deref().unwrap_or(""),
                    filter.as_deref(),
                    *include_properties,
                )
                .await
                .map(ActionOutput::TagsBrowse),
                (TagsCommand::Read { paths, project }, Ok(api)) => {
                    actions::tags::tags_read(&api, project, paths)
                        .await
                        .map(ActionOutput::TagsRead)
                }
                (TagsCommand::Write { path, project, .. }, Ok(api)) => {
                    actions::tags::tags_write(&api, project, path, write_value.expect("parsed"))
                        .await
                        .map(ActionOutput::TagsWrite)
                }
                (TagsCommand::Config(TagsConfigCommand::Get { path, project }), Ok(api)) => {
                    actions::tags::tags_config_get(&api, project, path)
                        .await
                        .map(ActionOutput::TagsConfigGet)
                }
                (TagsCommand::Config(TagsConfigCommand::Create { path, project, .. }), Ok(api)) => {
                    actions::tags::tags_config_create(
                        &api,
                        project,
                        path,
                        json_input.as_ref().expect("parsed pre-resolution"),
                    )
                    .await
                    .map(ActionOutput::TagsConfigCreate)
                }
                (TagsCommand::Config(TagsConfigCommand::Edit { path, project, .. }), Ok(api)) => {
                    actions::tags::tags_config_edit(
                        &api,
                        project,
                        path,
                        json_input.as_ref().expect("parsed pre-resolution"),
                    )
                    .await
                    .map(ActionOutput::TagsConfigEdit)
                }
                (TagsCommand::Config(TagsConfigCommand::Delete { paths, project }), Ok(api)) => {
                    actions::tags::tags_config_delete(&api, project, paths)
                        .await
                        .map(ActionOutput::TagsConfigDelete)
                }
                (TagsCommand::Udt(TagsUdtCommand::Types { provider, project }), Ok(api)) => {
                    actions::tags::tags_udt_types(&api, project, provider)
                        .await
                        .map(ActionOutput::TagsUdtTypes)
                }
                (
                    TagsCommand::Udt(TagsUdtCommand::Def {
                        name: udt_name,
                        provider,
                        project,
                    }),
                    Ok(api),
                ) => actions::tags::tags_udt_def(&api, project, provider, udt_name)
                    .await
                    .map(ActionOutput::TagsUdtDef),
                (TagsCommand::Export { paths, project, .. }, Ok(api)) => {
                    actions::tags::tags_export(
                        &api,
                        project,
                        paths,
                        export_out
                            .as_ref()
                            .expect("resolved pre-resolution")
                            .as_deref(),
                    )
                    .await
                    .map(ActionOutput::TagsExport)
                }
                (
                    TagsCommand::Import {
                        provider,
                        project,
                        collision_policy,
                        ..
                    },
                    Ok(api),
                ) => actions::tags::tags_import(
                    &api,
                    project,
                    provider,
                    json_input.expect("parsed pre-resolution"),
                    (*collision_policy).into(),
                )
                .await
                .map(ActionOutput::TagsImport),
                (
                    TagsCommand::Alarms(TagsAlarmsCommand::Active {
                        source,
                        priority,
                        state,
                        project,
                    }),
                    Ok(api),
                ) => actions::tags::tags_alarms_active(
                    &api,
                    project,
                    source.as_deref(),
                    priority.as_deref(),
                    state.as_deref(),
                )
                .await
                .map(ActionOutput::TagsAlarmsActive),
                (TagsCommand::Alarms(TagsAlarmsCommand::History { project, .. }), Ok(api)) => {
                    let (start_ms, end_ms) = time_args.expect("parsed pre-resolution");
                    actions::tags::tags_alarms_history(&api, project, start_ms, end_ms)
                        .await
                        .map(ActionOutput::TagsAlarmsHistory)
                }
                (
                    TagsCommand::Alarms(TagsAlarmsCommand::Ack {
                        ids,
                        note,
                        username,
                        project,
                    }),
                    Ok(api),
                ) => actions::tags::tags_alarms_ack(
                    &api,
                    project,
                    ids,
                    note.as_deref().unwrap_or(""),
                    username,
                )
                .await
                .map(ActionOutput::TagsAlarmsAck),
                (
                    TagsCommand::History(TagsHistoryCommand::Query {
                        paths,
                        return_size,
                        aggregation,
                        project,
                        ..
                    }),
                    Ok(api),
                ) => {
                    let (start_ms, end_ms) = time_args.expect("parsed pre-resolution");
                    actions::tags::tags_history_query(
                        &api,
                        project,
                        paths,
                        start_ms,
                        end_ms,
                        *return_size,
                        aggregation.as_deref(),
                    )
                    .await
                    .map(ActionOutput::TagsHistoryQuery)
                }
                (_, Err(err)) => Err(err),
            };
            (name, result)
        }
        Commands::Rig(RigArgs { rig, command }) => {
            // Guard BEFORE the runner/discovery even exist (the
            // sessions-terminate precedent) — a refusal is exit 2
            // with profile null and does ZERO discovery work
            // (binary-pinned: exit 2 in a cwd with no rig
            // discoverable at all). The message names the ACTUAL verb.
            let guarded_operation = match &command {
                RigCommand::Reset { .. } => Some("rig reset"),
                RigCommand::Restore { .. } => Some("rig restore"),
                RigCommand::Trial(trial_args) => match trial_args.command {
                    cli::TrialCommand::Reset { .. } => Some("rig trial reset"),
                    cli::TrialCommand::Status => None,
                },
                _ => None,
            };
            if let Some(operation) = guarded_operation
                && let Err(err) = require_confirmation(cli.yes, operation)
            {
                return (None, Err(err));
            }
            let runner = ignition_core::rig::DockerCompose;
            let selection = match rig {
                Some(name) => ignition_core::rig::RigSelection::Named(name),
                None => ignition_core::rig::RigSelection::Auto,
            };
            // The GATEWAY verbs (trial, snapshot, restore — they
            // address the rig's gateway, not the profile's) echo the
            // CONFIG's active profile name as context when one
            // exists; docker verbs stay profile:null (documented).
            let gateway_verb_echo = config.active.clone();
            let is_gateway_verb = matches!(
                command,
                RigCommand::Trial(_) | RigCommand::Snapshot { .. } | RigCommand::Restore { .. }
            );
            let result = match ignition_core::rig::resolve_plan(&runner, selection, &config).await {
                Ok(plan) => match command {
                    RigCommand::Up { .. } | RigCommand::Reset { .. } => {
                        // The commissioned-wait probe: a HEADER-LESS
                        // client pointed at the rig's OWN derived
                        // gateway URL (never the profile's gateway) —
                        // StatusPing answers unauthenticated, so the
                        // wait works even with no credential at all.
                        // ssl_verify=false: localhost probes against
                        // self-signed rig https are the norm. Shared
                        // by up and reset (both end in the wait).
                        let probe = commissioned_probe(&plan);
                        let probe_dyn: Option<&dyn ignition_core::client::GatewayApi> = probe
                            .as_ref()
                            .map(|api| api as &dyn ignition_core::client::GatewayApi);
                        match command {
                            RigCommand::Up { timeout } => {
                                actions::rig::rig_up(&runner, &plan, timeout, probe_dyn)
                                    .await
                                    .map(ActionOutput::RigUp)
                            }
                            RigCommand::Reset { timeout } => {
                                actions::rig::rig_reset(&runner, &plan, timeout, probe_dyn)
                                    .await
                                    .map(ActionOutput::RigReset)
                            }
                            _ => unreachable!("guarded by the outer match arm"),
                        }
                    }
                    RigCommand::Down => actions::rig::rig_down(&runner, &plan)
                        .await
                        .map(ActionOutput::RigDown),
                    RigCommand::Status => actions::rig::rig_status(&runner, &plan)
                        .await
                        .map(ActionOutput::RigStatus),
                    // The THIRD sanctioned stdout exception (after
                    // completions and `logs -f`): raw passthrough in
                    // EVERY render mode — compose log lines are not
                    // gateway JSON objects, and wrapping would corrupt
                    // them (`rig logs --json` = same passthrough). The
                    // dispatch owns the printing during execution; the
                    // returned result only carries the count.
                    RigCommand::Logs {
                        tail,
                        follow,
                        service,
                    } => {
                        let mut sink = |line: String| println!("{line}");
                        actions::rig::rig_logs(
                            &runner,
                            &plan,
                            tail,
                            follow,
                            service.as_deref(),
                            &mut sink,
                        )
                        .await
                        .map(ActionOutput::RigLogs)
                    }
                    // Trial (04-03, RIG-02/03): BOTH verbs address the
                    // RIG's derived gateway URL (never the profile's)
                    // — status header-less (the endpoints answer
                    // unauthenticated; fresh-rig friendly), reset with
                    // the tier-0 token when IGNITION_TOKEN is set (the
                    // client carries it; the action's ladder decides
                    // which rung lands) and/or the tier-1 pair
                    // (--user / IGNITION_USER + IGNITION_PASSWORD).
                    RigCommand::Trial(ref trial_args) => match trial_args.command.clone() {
                        cli::TrialCommand::Status => match rig_gateway_client(&plan, None) {
                            Some(api) => actions::rig::trial_status(&api)
                                .await
                                .map(ActionOutput::RigTrialStatus),
                            None => Err(trial_no_gateway(&plan)),
                        },
                        cli::TrialCommand::Reset { user } => {
                            // Cred sourcing (rig family — no profile
                            // chain): tier-0 token = IGNITION_TOKEN;
                            // tier-1 pair = --user flag or IGNITION_USER
                            // + IGNITION_PASSWORD (password env-only,
                            // NEVER a flag).
                            let token = std::env::var("IGNITION_TOKEN")
                                .ok()
                                .filter(|value| !value.is_empty());
                            let username = user.clone().or_else(|| env_non_empty("IGNITION_USER"));
                            let password = env_non_empty("IGNITION_PASSWORD");
                            let basic = username.zip(password).map(|(user, password)| {
                                (user, ignition_core::config::Secret::new(password))
                            });
                            if token.is_none() && basic.is_none() {
                                // The both-absent refusal: exit 3, the
                                // hint names both credential paths.
                                return (
                                    gateway_verb_echo,
                                    Err(CoreError::SecretUnavailable {
                                        profile: plan.name.clone(),
                                    }),
                                );
                            }
                            let credential =
                                token.map(|token| Credential::Token(config::Secret::new(token)));
                            let token_available = credential.is_some();
                            match rig_gateway_client(&plan, credential) {
                                Some(api) => {
                                    let rig_url = actions::rig::gateway_url_from(&plan)
                                        .expect("rig_gateway_client derived it or returned None");
                                    let basic_ref = basic
                                        .as_ref()
                                        .map(|(user, password)| (user.as_str(), password));
                                    actions::rig::trial_reset(
                                        &api,
                                        &rig_url,
                                        token_available,
                                        basic_ref,
                                    )
                                    .await
                                    .map(ActionOutput::RigTrialReset)
                                }
                                None => Err(trial_no_gateway(&plan)),
                            }
                        }
                    },
                    // Snapshot (04-04, RIG-04): the backup endpoints
                    // REQUIRE a token (401 HTML unauth — live-verified
                    // shape), so the rig-family cred sourcing (no
                    // profile chain) has exactly one rung: IGNITION_TOKEN.
                    RigCommand::Snapshot { output } => {
                        let Some(token) = env_non_empty("IGNITION_TOKEN") else {
                            return (
                                gateway_verb_echo,
                                Err(CoreError::SecretUnavailable {
                                    profile: plan.name.clone(),
                                }),
                            );
                        };
                        let credential = Some(Credential::Token(config::Secret::new(token)));
                        match rig_gateway_client(&plan, credential) {
                            Some(api) => {
                                actions::rig::rig_snapshot(&api, &plan.name, output.as_deref())
                                    .await
                                    .map(ActionOutput::RigSnapshot)
                            }
                            None => Err(trial_no_gateway(&plan)),
                        }
                    }
                    // Restore (04-04, RIG-04): guarded above (BEFORE
                    // discovery — the binary pin), token-sourced like
                    // snapshot, and the action owns the witnessed
                    // post-restore RUNNING wait.
                    RigCommand::Restore { file, timeout } => {
                        let Some(token) = env_non_empty("IGNITION_TOKEN") else {
                            return (
                                gateway_verb_echo,
                                Err(CoreError::SecretUnavailable {
                                    profile: plan.name.clone(),
                                }),
                            );
                        };
                        let credential = Some(Credential::Token(config::Secret::new(token)));
                        match rig_gateway_client(&plan, credential) {
                            Some(api) => {
                                let rig_url = actions::rig::gateway_url_from(&plan)
                                    .expect("rig_gateway_client derived it or returned None");
                                actions::rig::rig_restore(&api, &rig_url, &file, timeout)
                                    .await
                                    .map(ActionOutput::RigRestore)
                            }
                            None => Err(trial_no_gateway(&plan)),
                        }
                    }
                },
                Err(err) => Err(err),
            };
            // The gateway verbs (trial, snapshot, restore) echo the
            // active profile as context; the docker verbs keep the
            // family's profile:null contract.
            let echo = if is_gateway_verb {
                gateway_verb_echo
            } else {
                None
            };
            (echo, result)
        }
        // Backups (07-02, BKUP-01): the Phase 4 gwbk wire on any
        // profiled gateway. Download is a streamed read (unguarded);
        // restore is the 8th --yes-guarded destructive verb — the
        // guard fires BEFORE resolution (the sessions-terminate
        // shape: exit 2, profile null, zero network on refusal),
        // naming the whole-gateway consequence + restart block.
        Commands::Backup(BackupArgs { command }) => match command {
            BackupCommand::Download { output, r#type } => {
                let (name, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
                let result = match api {
                    Ok(api) => {
                        let stem = name.as_deref().unwrap_or("gateway");
                        actions::backup::backup_download(
                            &api,
                            output.as_deref(),
                            stem,
                            r#type.into(),
                        )
                        .await
                        .map(ActionOutput::BackupDownload)
                    }
                    Err(err) => Err(err),
                };
                (name, result)
            }
            BackupCommand::Restore { file } => {
                if let Err(err) = require_confirmation(
                    cli.yes,
                    "backup restore (overwrites this gateway's state from the gwbk — \
                     gateway restarts and blocks ~minutes)",
                ) {
                    return (None, Err(err));
                }
                let (name, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::backup::backup_restore(&api, &file)
                        .await
                        .map(ActionOutput::BackupRestore),
                    Err(err) => Err(err),
                };
                (name, result)
            }
        },
        // EAM (07-02, BKUP-02): the read-heavy family. history rides
        // the RUNTIME seam (the controller 403 classifies
        // eam_not_controller — never auth_rejected); tasks ride the
        // config-resource seam (definitions answer on stock
        // gateways). Both are reads, unguarded.
        Commands::Eam(EamArgs { command }) => match command {
            EamCommand::History { limit, search } => {
                let (name, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::eam::eam_history(&api, limit, search.as_deref())
                        .await
                        .map(ActionOutput::EamHistory),
                    Err(err) => Err(err),
                };
                (name, result)
            }
            EamCommand::Tasks { name: task_name } => {
                let (name, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
                let result = match api {
                    Ok(api) => match task_name {
                        Some(task_name) => actions::eam::eam_task_detail(&api, &task_name)
                            .await
                            .map(ActionOutput::EamTaskDetail),
                        None => actions::eam::eam_tasks(&api)
                            .await
                            .map(ActionOutput::EamTasks),
                    },
                    Err(err) => Err(err),
                };
                (name, result)
            }
            // Task writes (07-02 Task 3): the typed guard ladder.
            // `new` computes the PURE verdict from the parsed args
            // PRE-RESOLUTION — zero network on refusal; `Refused`
            // never reaches a client either way (the action
            // re-checks and errors — the double-check keeps the
            // ladder authoritative in core). `force` is ALWAYS
            // guarded (it dispatches NOW).
            EamCommand::Task(task_command) => match task_command {
                EamTaskCommand::New {
                    name: task_name,
                    r#type,
                    target,
                    setting,
                    definition,
                    schedule_mode,
                } => {
                    use actions::eam::TaskCreateVerdict;
                    match actions::eam::task_create_guard(&r#type, schedule_mode.wire()) {
                        TaskCreateVerdict::Refused => {
                            return (
                                None,
                                Err(CoreError::EamTaskTypeRefused {
                                    task_type: r#type.clone(),
                                }),
                            );
                        }
                        TaskCreateVerdict::NeedsYes => {
                            // The operation string names WHICH rung
                            // fired (the consequence, not a generic
                            // hint — the resource-put message
                            // pattern).
                            let operation = if schedule_mode != ScheduleMode::OnDemand {
                                format!(
                                    "eam task new (scheduleMode {} arms autonomous \
                                     gateway actions)",
                                    schedule_mode.wire()
                                )
                            } else {
                                format!(
                                    "eam task new ({} mutates the agent \
                                     targets it dispatches to)",
                                    r#type
                                )
                            };
                            if let Err(err) = require_confirmation(cli.yes, &operation) {
                                return (None, Err(err));
                            }
                        }
                        TaskCreateVerdict::Unguarded => {}
                    }
                    // The --definition file reads BEFORE resolution
                    // (the tags-config create byte-source
                    // precedent: malformed JSON is exit 2 with zero
                    // network work).
                    let definition_value = match definition.as_deref() {
                        Some(path) => match read_json_input(path).await {
                            Ok(value) => Some(value),
                            Err(err) => return (None, Err(err)),
                        },
                        None => None,
                    };
                    let (name, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
                    let result = match api {
                        Ok(api) => actions::eam::eam_task_create(
                            &api,
                            &task_name,
                            &r#type,
                            &target,
                            &setting,
                            definition_value.as_ref(),
                            schedule_mode.wire(),
                        )
                        .await
                        .map(ActionOutput::EamTaskCreate),
                        Err(err) => Err(err),
                    };
                    (name, result)
                }
                EamTaskCommand::Force { name: task_name } => {
                    if let Err(err) = require_confirmation(
                        cli.yes,
                        "eam task force (dispatches the task to agent targets NOW)",
                    ) {
                        return (None, Err(err));
                    }
                    let (name, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
                    let result = match api {
                        Ok(api) => actions::eam::eam_task_force(&api, &task_name)
                            .await
                            .map(ActionOutput::EamTaskForce),
                        Err(err) => Err(err),
                    };
                    (name, result)
                }
            },
        },
        // `ign script run` (07-03, SCRPT-01): usage errors LEAD —
        // the three-form input reader runs BEFORE any resolution
        // (both --code and --file, or neither, refuse invalid_input
        // exit 2 with profile null, zero work — the 03-03 put
        // convention). NO --yes guard exists: the opt-in is
        // STRUCTURAL (the scriptExec route deploys only via
        // `ign webdev deploy --with-script-exec`); the action's own
        // secret gate refuses `script_exec_not_configured` (exit 6)
        // when the route was never deployed.
        Commands::Script(ScriptArgs { command }) => match command {
            ScriptCommand::Run {
                code,
                file,
                project,
            } => {
                let script =
                    match actions::script::read_script_input(code.as_deref(), file.as_deref()) {
                        Ok(script) => script,
                        Err(err) => return (None, Err(err)),
                    };
                match resolve_profile_context(&mut config, cli.profile.as_deref()) {
                    Ok(None) => (None, Err(CoreError::NoActiveProfile)),
                    Ok(Some((name, profile))) => {
                        let client = config::resolve_secret(&name, &profile.auth, &secret_chain())
                            .and_then(|credential| {
                                ReqwestGatewayApi::new(&profile, Some(credential))
                            });
                        let result = match client {
                            Ok(api) => {
                                actions::script::script_run(&api, &config, &name, &project, &script)
                                    .await
                                    .map(ActionOutput::ScriptRun)
                            }
                            Err(err) => Err(err),
                        };
                        (Some(name), result)
                    }
                    Err(err) => (None, Err(err)),
                }
            }
        },
        // `ign lint` (07-04, INTR-02): LOCAL delegation — no gateway,
        // no credential, no profile resolution at all (the docker-verb
        // precedent for non-gateway commands; envelope profile null).
        // The strict-mode exit passthrough is decided in `main` AFTER
        // the envelope renders — the one sanctioned success-path EXIT
        // exception (README "Linting").
        Commands::Lint(LintArgs {
            paths,
            strict,
            passthrough,
        }) => {
            let result = actions::lint::lint_run(&paths, strict, &passthrough)
                .await
                .map(ActionOutput::Lint);
            (None, result)
        }
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
        // TTY guard BEFORE anything: ratatui::init panics on non-terminal
        // stdout (Pitfall 10) — refuse usage-class instead (06-07: the
        // constructor pairs the reason with its terminal-contextual hint,
        // not the --file/stdin resource-put default). The cockpit
        // itself (loop, lifecycle, restore) lives in ignition-tui; this
        // arm stays thin (choke-file discipline).
        Commands::Tui => {
            if !std::io::stdout().is_terminal() {
                return (None, Err(CoreError::tui_tty_refusal()));
            }
            match ignition_tui::run(cli.profile.clone()).await {
                Ok(()) => (None, Ok(ActionOutput::TuiExited)),
                Err(err) => (None, Err(err)),
            }
        }
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

/// Resolve ONE named profile side to a built client — the per-profile
/// construction path shared with [`resolve_gateway_api`]: REQUIRED
/// credential through the ONE locked secret chain (no degradation, no
/// fork), then `ReqwestGatewayApi::new` from the resolved profile.
fn named_profile_client(config: &mut Config, name: &str) -> Result<ReqwestGatewayApi, CoreError> {
    let Some((_resolved, profile)) = config::resolve_selection(config, Some(name))? else {
        return Err(CoreError::Internal(
            "a named profile selection resolved to nothing".to_string(),
        ));
    };
    let credential = config::resolve_secret(name, &profile.auth, &secret_chain())?;
    ReqwestGatewayApi::new(&profile, Some(credential))
}

/// THE two-client resolution shape (07-01, 07-RESEARCH Pattern): the
/// ENVELOPE's active profile resolves exactly as every other command
/// (flag > active, `IGNITION_URL` env overlay scoped to that
/// selection — resolution UNCHANGED), then each positional side
/// resolves through the same `resolve_selection` machinery and builds
/// its own client. Each side's secret chain resolves INDEPENDENTLY
/// through the one locked chain: `IGNITION_TOKEN` (and the basic env
/// pair) applies to BOTH sides unless per-profile keyring entries
/// exist — the README's two-sided-secret caveat.
fn resolve_two_clients(
    config: &mut Config,
    flag: Option<&str>,
    name_a: &str,
    name_b: &str,
) -> (
    Option<String>,
    Result<(ReqwestGatewayApi, ReqwestGatewayApi), CoreError>,
) {
    match resolve_profile_context(config, flag) {
        Ok(None) => (None, Err(CoreError::NoActiveProfile)),
        Ok(Some((active, _))) => {
            let sides = named_profile_client(config, name_a)
                .and_then(|api_a| named_profile_client(config, name_b).map(|api_b| (api_a, api_b)));
            (Some(active), sides)
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

/// The rig family's commissioned-wait probe: a HEADER-LESS client
/// pointed at the rig's OWN derived gateway URL (never the profile's
/// gateway) — `/StatusPing` answers unauthenticated, so the up/reset
/// waits work even with no credential at all. `ssl_verify=false`:
/// localhost probes against self-signed rig https are the norm.
/// Shared by `rig up` and `rig reset` (both end in the wait).
fn commissioned_probe(plan: &ignition_core::rig::RigPlan) -> Option<ReqwestGatewayApi> {
    rig_gateway_client(plan, None)
}

/// A client pointed at the rig's OWN derived gateway URL (the
/// `commissioned_probe` generalized for the trial verbs: an optional
/// credential rides along — tier 0's token when `IGNITION_TOKEN` is
/// set; the trial endpoints tolerate headers either way,
/// live-verified). `None` when no gateway port is derivable.
fn rig_gateway_client(
    plan: &ignition_core::rig::RigPlan,
    credential: Option<Credential>,
) -> Option<ReqwestGatewayApi> {
    actions::rig::gateway_url_from(plan).and_then(|url| {
        let profile = config::Profile {
            url: url.parse().ok()?,
            label: None,
            ssl_verify: false,
            auth: AuthRef::default(),
            webdev_secret: None,
        };
        ReqwestGatewayApi::new(&profile, credential).ok()
    })
}

/// The trial verbs' no-gateway refusal: a rig with no 8088/443 port
/// mapping has no gateway to ask.
fn trial_no_gateway(plan: &ignition_core::rig::RigPlan) -> CoreError {
    CoreError::Rig(format!(
        "rig {} publishes no gateway port (target 8088/443) — trial \
         commands address the rig's gateway",
        plan.name
    ))
}

/// A non-empty env var, when set.
fn env_non_empty(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
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
    // The rig family's env fold lives here too (one env→flag home):
    // IGNITION_RIG fills a missing --rig exactly like IGNITION_PROFILE
    // fills --profile — the nested arg is reachable via the command.
    if let Commands::Rig(rig_args) = &mut cli.command
        && rig_args.rig.is_none()
        && let Ok(rig) = std::env::var("IGNITION_RIG")
        && !rig.is_empty()
    {
        rig_args.rig = Some(rig);
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

/// `tags write --value`'s JSON-scalar rule (05-04, README-documented):
/// parse as JSON — a scalar (number/bool/null/string) rides untyped;
/// text that does NOT parse is a bare string (`--value hello` is the
/// string "hello"); a parsed ARRAY/OBJECT is a usage error
/// (`invalid_input`, exit 2, pre-resolution — the tag value wire
/// slot is a scalar).
fn parse_write_scalar(raw: &str) -> Result<serde_json::Value, CoreError> {
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(value) if !value.is_array() && !value.is_object() => Ok(value),
        Ok(_) => Err(CoreError::InvalidInput {
            reason: format!(
                "--value must be a JSON scalar (number, bool, null, or string) — \
                 arrays/objects cannot ride the tag write slot: {raw:?}"
            ),
        }),
        Err(_) => Ok(serde_json::Value::String(raw.to_string())),
    }
}

/// Read a JSON document from `--file PATH` (std::fs) or `--file -`
/// (tokio stdin) and PARSE it — the resource-put byte-source
/// precedent (InvalidInput class, pre-resolution: exit 2 with zero
/// network work on an unreadable file or malformed JSON). Used by
/// `tags config create|edit` (the definition) and `tags import`
/// (the payload).
async fn read_json_input(file: &std::path::Path) -> Result<serde_json::Value, CoreError> {
    let label = if file == std::path::Path::new("-") {
        "stdin".to_string()
    } else {
        file.display().to_string()
    };
    let bytes = if file == std::path::Path::new("-") {
        use tokio::io::AsyncReadExt;
        let mut buffer = Vec::new();
        tokio::io::stdin()
            .read_to_end(&mut buffer)
            .await
            .map_err(|err| CoreError::InvalidInput {
                reason: format!("cannot read stdin: {err}"),
            })?;
        buffer
    } else {
        std::fs::read(file).map_err(|err| CoreError::InvalidInput {
            reason: format!("cannot read {label}: {err}"),
        })?
    };
    serde_json::from_slice(&bytes).map_err(|err| CoreError::InvalidInput {
        reason: format!("{label} is not valid JSON: {err}"),
    })
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
