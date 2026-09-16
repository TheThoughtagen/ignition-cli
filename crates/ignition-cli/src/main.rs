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
mod mcp;
mod render;

#[cfg(feature = "tui")]
use std::io::IsTerminal;
use std::process::ExitCode;

use clap::Parser;

use ignition_core::actions;
use ignition_core::client::GatewayApi as _;
use ignition_core::config::{self, Config, Credential};
use ignition_core::error::CoreError;
use ignition_core::session::Session;

// The command tree lives in the crate's lib target (shared with the
// integration tests — the TUI-coverage walk needs `Cli::command()`).
use crate::render::{RenderMode, render_error, render_log_entry_line, render_ok};
use ignition_cli::cli;
use ignition_cli::cli::{
    ApiArgs, ApiCommand, BackupArgs, BackupCommand, BundleCommand, Cli, Commands, DiagnosticsArgs,
    DiagnosticsCommand, EamArgs, EamCommand, EamTaskCommand, EditArgs, GanArgs, GanCommand,
    LicenseArgs, LicenseCommand, LintArgs, LogLevel, LoggersCmd, LogsArgs, LogsCmd, ProfileArgs,
    ProfileCmd, ProjectArgs, ProjectCommand, RedundancyArgs, RedundancyCommand, ResourceArgs,
    ResourceCommand, RigArgs, RigCommand, ScheduleMode, ScriptArgs, ScriptCommand, SessionsArgs,
    SessionsCmd, TagsAlarmsCommand, TagsArgs, TagsCommand, TagsConfigCommand, TagsHistoryCommand,
    TagsProviderCommand, TagsUdtCommand, WaitArgs, WaitCmd, WebdevArgs, WebdevCommand,
    WorkspaceArgs, WorkspaceCommand,
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
    /// `ign eam task suspend|resume|cancel` — ONE variant keyed by
    /// the result's own `action` string ("suspended"/"resumed"/
    /// "cancelled"; 10-04): the lifecycle verbs share the all-keys
    /// EamLifecycleResult shape.
    EamTaskLifecycle(actions::eam::EamLifecycleResult),
    /// `ign eam task modify` — the full-record RMW read-back (10-04).
    EamTaskModify(actions::eam::EamModifyResult),
    /// `ign eam task delete` — the signature-keyed delete outcome
    /// (10-04).
    EamTaskDelete(actions::eam::EamDeleteResult),
    /// `ign script run` — the scriptExec answer under unit-explicit
    /// keys {stdout, result, elapsedMs} (ALL keys always; the
    /// secret never rides any output path).
    ScriptRun(actions::script::ScriptRunResult),
    /// `ign lint` — the doctor-posture delegation result; exit 0
    /// whenever the child ran, findings + child_exit_code + the
    /// parsed report as data (`--strict`'s passthrough is decided
    /// in `main` AFTER the envelope renders).
    Lint(actions::lint::LintResult),
    /// `ign api call` — the raw passthrough's outcome: method/path
    /// echo plus the gateway's verbatim answer (status + RawValue
    /// body; the README's documented contract exception).
    ApiCall(actions::apicall::ApiCallOutcome),
    /// `ign license status` — the inventory + trial-companion merge
    /// (09-04, EXT-02; the morning-check read).
    LicenseStatus(actions::license::LicenseStatusResult),
    /// `ign redundancy status` — the flat capture-shaped read (09-04).
    RedundancyStatus(actions::redundancy::RedundancyStatusResult),
    /// `ign gan status` — the 5-field GAN overview (09-04).
    GanStatus(actions::gan::GanStatusResult),
    /// `ign diagnostics bundle generate` — the fresh status wire
    /// (09-05, EXT-02; the 200 body IS the status).
    BundleGenerate(actions::diagnostics::BundleStatusWire),
    /// `ign diagnostics bundle status` — the captured-vocabulary
    /// status read.
    BundleStatus(actions::diagnostics::BundleStatusWire),
    /// `ign diagnostics bundle wait` — the terminal status wire
    /// (poll engine semantics).
    BundleWait(actions::diagnostics::BundleStatusWire),
    /// `ign diagnostics bundle download` — file + bytes + content
    /// type.
    BundleDownload(actions::diagnostics::BundleDownloadResult),
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
    /// `ign workspace checkout` — the mapped tree + recorded
    /// manifest outcome (13-07): {project, target, member_count,
    /// scripts_decoded}.
    WorkspaceCheckout(actions::workspace::CheckoutOutcome),
    /// `ign workspace status` — the PUSH-RELATIVE three-way compare
    /// (13-06): {project, clean, rows}.
    WorkspaceStatus(actions::workspace::WorkspaceStatus),
    /// `ign workspace push` — the splice bookkeeping (13-06):
    /// {project, wrote, deleted, skipped}.
    WorkspacePush(actions::workspace::PushOutcome),
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
            ActionOutput::EamTaskLifecycle(result) => render_success(profile, result, compact),
            ActionOutput::EamTaskModify(result) => render_success(profile, result, compact),
            ActionOutput::EamTaskDelete(result) => render_success(profile, result, compact),
            ActionOutput::ScriptRun(result) => render_success(profile, result, compact),
            ActionOutput::Lint(result) => render_success(profile, result, compact),
            ActionOutput::ApiCall(result) => render_success(profile, result, compact),
            ActionOutput::LicenseStatus(result) => render_success(profile, result, compact),
            ActionOutput::RedundancyStatus(result) => render_success(profile, result, compact),
            ActionOutput::GanStatus(result) => render_success(profile, result, compact),
            ActionOutput::BundleGenerate(result) => render_success(profile, result, compact),
            ActionOutput::BundleStatus(result) => render_success(profile, result, compact),
            ActionOutput::BundleWait(result) => render_success(profile, result, compact),
            ActionOutput::BundleDownload(result) => render_success(profile, result, compact),
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
            ActionOutput::WorkspaceCheckout(result) => render_success(profile, result, compact),
            ActionOutput::WorkspaceStatus(result) => render_success(profile, result, compact),
            ActionOutput::WorkspacePush(result) => render_success(profile, result, compact),
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
    // 13-08: `ign edit` is the OutOfBand verb — the child $EDITOR owns
    // the terminal, so edit writes ZERO bytes to stdout in every mode
    // (byte-scan pinned by contract_edit.rs) and its prose renders to
    // stderr. No ActionOutput variant exists for it BY DESIGN (a
    // success variant would route through render_ok's stdout —
    // exactly what the OutOfBand declaration forbids — and render.rs
    // stays untouched), so edit dispatches on its own seam HERE:
    // after the ONE mode decision, still one ExitCode decision point
    // in main; its refusals ride the SAME render_error envelope
    // (stderr in every mode).
    if let Commands::Edit(args) = cli.command {
        let profile_flag = cli.profile.clone();
        let yes = cli.yes;
        return runtime.block_on(dispatch_edit(args, profile_flag.as_deref(), yes, mode));
    }
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
        Commands::Version => match Session::resolve_degraded(cli.profile.as_deref()) {
            Ok(session) => {
                let name = session.profile_name().to_string();
                let result =
                    actions::version::version(Some(session.api()), env!("CARGO_PKG_VERSION")).await;
                (Some(name), result.map(ActionOutput::Version))
            }
            Err(CoreError::NoActiveProfile) => {
                // Fresh install / nothing resolved: CLI version only.
                let result = actions::version::version(None, env!("CARGO_PKG_VERSION")).await;
                (None, result.map(ActionOutput::Version))
            }
            Err(err) => (error_profile(&err), Err(err)),
        },
        // The inspection commands (02-02): authed reads of a healthy
        // gateway. Credential REQUIRED — resolve_secret, not the
        // header-less degradation `version` uses: these commands cannot
        // work unauthenticated, so a missing secret is SecretUnavailable
        // (exit 3), the correct taxonomy, not a doomed 401.
        Commands::Status => run_inspection(cli.profile.as_deref(), Inspection::Status).await,
        Commands::Modules { quarantined } => {
            run_inspection(cli.profile.as_deref(), Inspection::Modules(quarantined)).await
        }
        Commands::Metrics { history } => {
            run_inspection(cli.profile.as_deref(), Inspection::Metrics(history)).await
        }
        // Sessions (02-03): the merged list is a plain authed read; the
        // terminate half is the CLI's FIRST DESTRUCTIVE COMMAND — the
        // guard refuses (exit 2, confirmation_required) BEFORE any API
        // construction, so a refusal costs nothing and never depends on
        // config/profile state (usage-class errors lead, like clap's).
        Commands::Sessions(SessionsArgs { r#type, command }) => match command {
            None => {
                let (name, api) = resolve_gateway_api(cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::sessions::sessions(&*api, r#type.map(Into::into))
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
                // guarded:sessions terminate
                if let Err(err) = require_confirmation(cli.yes, "sessions terminate") {
                    return (None, Err(err));
                }
                let (name, api) = resolve_gateway_api(cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::sessions::terminate_session(
                        &*api,
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
            let (name, api) = resolve_gateway_api(cli.profile.as_deref());
            let result = match api {
                Ok(api) => actions::connections::connections(&*api, r#type.map(Into::into))
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
                let (name, api) = resolve_gateway_api(cli.profile.as_deref());
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
                            &*api,
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
                        actions::logs::list_logs(&*api, logger.as_deref(), min_level, since, limit)
                            .await
                            .map(ActionOutput::LogsList)
                    }
                    Err(err) => Err(err),
                };
                (name, result)
            }
            Some(LogsCmd::Download { output }) => {
                let (name, api) = resolve_gateway_api(cli.profile.as_deref());
                let result = match api {
                    Ok(api) => {
                        let stem = name.as_deref().unwrap_or("gateway");
                        actions::logs::download(&*api, output.as_deref(), stem)
                            .await
                            .map(ActionOutput::LogsDownload)
                    }
                    Err(err) => Err(err),
                };
                (name, result)
            }
            Some(LogsCmd::Loggers(loggers_args)) => match loggers_args.command {
                None => {
                    let (name, api) = resolve_gateway_api(cli.profile.as_deref());
                    let result = match api {
                        Ok(api) => actions::logs::loggers(&*api, loggers_args.search.as_deref())
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
                    // guarded:logs loggers set
                    if let Err(err) = require_confirmation(cli.yes, "logs loggers set") {
                        return (None, Err(err));
                    }
                    let (name, api) = resolve_gateway_api(cli.profile.as_deref());
                    let result = match api {
                        Ok(api) => actions::logs::set_logger_level(&*api, &logger, level.wire())
                            .await
                            .map(ActionOutput::LoggerSet),
                        Err(err) => Err(err),
                    };
                    (name, result)
                }
                Some(LoggersCmd::Reset) => {
                    // guarded:logs loggers reset
                    if let Err(err) = require_confirmation(cli.yes, "logs loggers reset") {
                        return (None, Err(err));
                    }
                    let (name, api) = resolve_gateway_api(cli.profile.as_deref());
                    let result = match api {
                        Ok(api) => actions::logs::reset_logger_levels(&*api)
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
            // guarded:restart
            if let Err(err) = require_confirmation(cli.yes, "restart") {
                return (None, Err(err));
            }
            let (name, api) = resolve_gateway_api(cli.profile.as_deref());
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
                            &*api,
                            interval,
                            timeout,
                            actions::restart::RESTART_FLOOR,
                        )
                        .await
                        .map(ActionOutput::RestartWait)
                    } else {
                        actions::restart::restart(&*api)
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
                let (name, api) = resolve_headerless_api(cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::restart::wait_gateway(
                        &*api,
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
                let (name, api) = resolve_headerless_api(cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::restart::wait_restart(
                        &*api,
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
                let (name, api) = resolve_gateway_api(cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::restart::wait_module(
                        &*api,
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
        } => match Session::resolve_degraded(cli.profile.as_deref()) {
            Ok(session) => {
                let name = session.profile_name().to_string();
                let opts = actions::doctor::DoctorOptions {
                    check_write,
                    webdev_route,
                };
                let result = actions::doctor::doctor(
                    &*session,
                    session.profile_url().as_str(),
                    session.credential_present(),
                    &opts,
                )
                .await;
                (Some(name), Ok(ActionOutput::Doctor(result)))
            }
            Err(err) => (error_profile(&err), Err(err)),
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
                let (profile, api) = resolve_gateway_api(cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::projects::projects(&*api)
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
                let (profile, api) = resolve_gateway_api(cli.profile.as_deref());
                let result = match api {
                    Ok(api) => {
                        let opts = actions::projects::NewOptions {
                            enabled: !disabled,
                            title,
                            description,
                            parent,
                            inheritable: inheritable.then_some(true),
                        };
                        actions::projects::project_new(&*api, &project_name, &opts)
                            .await
                            .map(ActionOutput::ProjectNew)
                    }
                    Err(err) => Err(err),
                };
                (profile, result)
            }
            ProjectCommand::Copy { src, dst } => {
                let (profile, api) = resolve_gateway_api(cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::projects::project_copy(&*api, &src, &dst)
                        .await
                        .map(ActionOutput::ProjectCopy),
                    Err(err) => Err(err),
                };
                (profile, result)
            }
            ProjectCommand::Rename { old_name, new_name } => {
                let (profile, api) = resolve_gateway_api(cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::projects::project_rename(&*api, &old_name, &new_name)
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
                let (profile, api) = resolve_gateway_api(cli.profile.as_deref());
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
                        actions::projects::project_set(&*api, &project_name, &opts)
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
                // guarded:project delete
                if let Err(err) = require_confirmation(cli.yes, "project delete") {
                    return (None, Err(err));
                }
                let (profile, api) = resolve_gateway_api(cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::projects::project_delete(&*api, &project_name)
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
                let (profile, api) = resolve_gateway_api(cli.profile.as_deref());
                let result = match api {
                    Ok(api) => {
                        if decode_scripts {
                            actions::projects::project_export_decoded(
                                &*api,
                                &project_name,
                                output.as_deref(),
                            )
                            .await
                            .map(ActionOutput::ProjectExportDecoded)
                        } else {
                            actions::projects::project_export(
                                &*api,
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
                // guarded:project import
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
                let (profile, api) = resolve_gateway_api(cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::projects::project_import(
                        &*api,
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
                        &*api_a,
                        &*api_b,
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
                // guarded:project sync
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
                            &*api_a,
                            &*api_b,
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
        // Workspace (13-07): the local edit loop over the
        // project-export interchange. `status`/`push` derive the
        // project from the RECORDED manifest — the user never
        // re-types it — and the manifest read needs NO gateway, so
        // its refusals (13-03's stable prefixes: missing / corrupt /
        // foreign-schema) fire PRE-resolution: exit 2, envelope
        // profile null, ZERO requests (the api-call usage-guard
        // convention). `checkout` takes the project as an explicit
        // arg and rides the action's own clobber/refusal ladder
        // post-resolution (still exit 2; the profile echoes because
        // resolution already succeeded). Everything dispatches
        // through `Session::resolve` — the Phase-8 seam, NO second
        // construction path.
        Commands::Workspace(WorkspaceArgs { command }) => match command {
            WorkspaceCommand::Checkout {
                project,
                target,
                decode_scripts,
            } => match Session::resolve(cli.profile.as_deref()) {
                Ok(session) => {
                    let name = session.profile_name().to_string();
                    let result = actions::workspace::workspace_checkout(
                        &*session,
                        &project,
                        &target,
                        &name,
                        decode_scripts,
                    )
                    .await
                    .map(ActionOutput::WorkspaceCheckout);
                    (Some(name), result)
                }
                Err(err) => (error_profile(&err), Err(err)),
            },
            WorkspaceCommand::Status { path } => {
                let manifest = match actions::workspace::read_manifest(&path) {
                    Ok(manifest) => manifest,
                    Err(err) => return (None, Err(err)),
                };
                match Session::resolve(cli.profile.as_deref()) {
                    Ok(session) => {
                        let name = session.profile_name().to_string();
                        let result = actions::workspace::workspace_status(
                            &path,
                            &*session,
                            &manifest.project,
                        )
                        .await
                        .map(ActionOutput::WorkspaceStatus);
                        (Some(name), result)
                    }
                    Err(err) => (error_profile(&err), Err(err)),
                }
            }
            WorkspaceCommand::Push { path, delete } => {
                let manifest = match actions::workspace::read_manifest(&path) {
                    Ok(manifest) => manifest,
                    Err(err) => return (None, Err(err)),
                };
                match Session::resolve(cli.profile.as_deref()) {
                    Ok(session) => {
                        let name = session.profile_name().to_string();
                        let result = actions::workspace::workspace_push(
                            &path,
                            &*session,
                            &manifest.project,
                            cli.yes,
                            delete,
                        )
                        .await
                        .map(ActionOutput::WorkspacePush);
                        (Some(name), result)
                    }
                    Err(err) => (error_profile(&err), Err(err)),
                }
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
                let (profile, api) = resolve_gateway_api(cli.profile.as_deref());
                let result = match api {
                    Ok(api) => {
                        actions::resources::resources_list(&*api, &project, prefix.as_deref())
                            .await
                            .map(ActionOutput::ResourcesList)
                    }
                    Err(err) => Err(err),
                };
                (profile, result)
            }
            ResourceCommand::Get { project, path } => {
                let (profile, api) = resolve_gateway_api(cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::resources::resource_get(&*api, &project, &path)
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
                // guarded:resource put
                if let Err(err) = require_confirmation(
                    cli.yes,
                    "resource put (re-imports the project; concurrent Designer edits are replaced)",
                ) {
                    return (None, Err(err));
                }
                let (profile, api) = resolve_gateway_api(cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::resources::resource_put(&*api, &project, &path, input)
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
                // guarded:resource delete
                if let Err(err) = require_confirmation(
                    cli.yes,
                    "resource delete (re-imports the project; concurrent Designer edits are replaced)",
                ) {
                    return (None, Err(err));
                }
                let (profile, api) = resolve_gateway_api(cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::resources::resource_delete(&*api, &project, &path)
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
            match Session::resolve(cli.profile.as_deref()) {
                Ok(session) => {
                    let name = session.profile_name().to_string();
                    // `webdev status` reads the profile's STORED scriptExec
                    // secret — a config fact the URL overlay never touches,
                    // answered by the already-loaded dispatch config now
                    // that the profile struct is core-owned (the seam
                    // resolves clients; the stored secret is not one).
                    let stored_secret = config
                        .profiles
                        .get(&name)
                        .and_then(|profile| profile.webdev_secret.clone());
                    let result = match command {
                        WebdevCommand::Deploy {
                            project,
                            with_script_exec,
                            rotate_secret,
                        } => {
                            if mode == RenderMode::Human {
                                eprintln!("deploying webdev routes to {project} …");
                            }
                            actions::webdev::webdev_deploy(
                                &*session,
                                &project,
                                with_script_exec,
                                rotate_secret,
                                &path,
                                &name,
                            )
                            .await
                            .map(ActionOutput::WebdevDeploy)
                        }
                        WebdevCommand::Status { project } => actions::webdev::webdev_status(
                            &*session,
                            &project,
                            stored_secret.as_deref(),
                        )
                        .await
                        .map(ActionOutput::WebdevStatus),
                    };
                    (Some(name), result)
                }
                Err(err) => (error_profile(&err), Err(err)),
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
                // guarded:tags provider delete
                TagsCommand::Provider(TagsProviderCommand::Delete { .. }) => {
                    Some("tags provider delete")
                }
                // guarded:tags config delete
                TagsCommand::Config(TagsConfigCommand::Delete { .. }) => Some("tags config delete"),
                // guarded:tags import
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
            // definition (`--file PATH` / `--file -`, parsed —
            // InvalidInput pre-resolution). Import's payload moved to
            // the format-aware block below (11-05).
            let json_input = match &command {
                TagsCommand::Config(TagsConfigCommand::Create { file, .. })
                | TagsCommand::Config(TagsConfigCommand::Edit { file, .. }) => {
                    match read_json_input(file).await {
                        Ok(parsed) => Some(parsed),
                        Err(err) => return (None, Err(err)),
                    }
                }
                _ => None,
            };
            // Import's payload input, FORMAT-AWARE (11-05): json
            // parses + validates pre-resolution exactly as before
            // (the dispatch re-serializes value-identically — the
            // wire body stays byte-identical); xml/csv read RAW BYTES
            // and run THE TAGS-12 loss gate — one function,
            // pre-resolution, so a refusal exits 2 `invalid_input`
            // with profile null and ZERO wire work (the guard and
            // read_json_input precedent).
            let (import_payload, import_loss): (
                Option<Vec<u8>>,
                Option<actions::tags::LossReport>,
            ) = match &command {
                TagsCommand::Import { file, format, .. } => {
                    if matches!(format, cli::TransferFormat::Json) {
                        match read_json_input(file).await {
                            Ok(parsed) => (
                                Some(serde_json::to_vec(&parsed).expect("parsed Value serializes")),
                                None,
                            ),
                            Err(err) => return (None, Err(err)),
                        }
                    } else {
                        let bytes = match read_input_bytes(file).await {
                            Ok(bytes) => bytes,
                            Err(err) => return (None, Err(err)),
                        };
                        match loss_gate(&bytes, *format, cli.yes) {
                            Ok(loss) => (Some(bytes), loss),
                            Err(err) => return (None, Err(err)),
                        }
                    }
                }
                _ => (None, None),
            };
            // Export's output resolution: `-o -` = stdout (the
            // payload rides the result; render prints it raw — the
            // sanctioned stdout exception), `-o FILE` = that file,
            // none = the default `<last-segment>.<ext>` (the
            // export-streaming convention; the extension rides the
            // format, 11-05 — json keeps `.json` byte-identical).
            let export_out = match &command {
                TagsCommand::Export {
                    paths,
                    output,
                    format,
                    ..
                } => Some(match output {
                    Some(path) if path == std::path::Path::new("-") => None,
                    Some(path) => Some(path.clone()),
                    None => Some(default_export_path(paths, *format)),
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
            let (name, api) = resolve_gateway_api(cli.profile.as_deref());
            let result = match (&command, api) {
                (TagsCommand::Provider(TagsProviderCommand::List), Ok(api)) => {
                    actions::tags::tag_provider_list(&*api)
                        .await
                        .map(ActionOutput::TagProviders)
                }
                (
                    TagsCommand::Provider(TagsProviderCommand::Create { name: provider }),
                    Ok(api),
                ) => actions::tags::tag_provider_create(&*api, provider)
                    .await
                    .map(ActionOutput::TagProviderCreate),
                (
                    TagsCommand::Provider(TagsProviderCommand::Delete { name: provider }),
                    Ok(api),
                ) => actions::tags::tag_provider_delete(&*api, provider)
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
                    &*api,
                    project,
                    path.as_deref().unwrap_or(""),
                    filter.as_deref(),
                    *include_properties,
                )
                .await
                .map(ActionOutput::TagsBrowse),
                (TagsCommand::Read { paths, project }, Ok(api)) => {
                    actions::tags::tags_read(&*api, project, paths)
                        .await
                        .map(ActionOutput::TagsRead)
                }
                (TagsCommand::Write { path, project, .. }, Ok(api)) => {
                    actions::tags::tags_write(&*api, project, path, write_value.expect("parsed"))
                        .await
                        .map(ActionOutput::TagsWrite)
                }
                (TagsCommand::Config(TagsConfigCommand::Get { path, project }), Ok(api)) => {
                    actions::tags::tags_config_get(&*api, project, path)
                        .await
                        .map(ActionOutput::TagsConfigGet)
                }
                (TagsCommand::Config(TagsConfigCommand::Create { path, project, .. }), Ok(api)) => {
                    actions::tags::tags_config_create(
                        &*api,
                        project,
                        path,
                        json_input.as_ref().expect("parsed pre-resolution"),
                    )
                    .await
                    .map(ActionOutput::TagsConfigCreate)
                }
                (TagsCommand::Config(TagsConfigCommand::Edit { path, project, .. }), Ok(api)) => {
                    actions::tags::tags_config_edit(
                        &*api,
                        project,
                        path,
                        json_input.as_ref().expect("parsed pre-resolution"),
                    )
                    .await
                    .map(ActionOutput::TagsConfigEdit)
                }
                (TagsCommand::Config(TagsConfigCommand::Delete { paths, project }), Ok(api)) => {
                    actions::tags::tags_config_delete(&*api, project, paths)
                        .await
                        .map(ActionOutput::TagsConfigDelete)
                }
                (TagsCommand::Udt(TagsUdtCommand::Types { provider, project }), Ok(api)) => {
                    actions::tags::tags_udt_types(&*api, project, provider)
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
                ) => actions::tags::tags_udt_def(&*api, project, provider, udt_name)
                    .await
                    .map(ActionOutput::TagsUdtDef),
                (
                    TagsCommand::Export {
                        paths,
                        project,
                        format,
                        ..
                    },
                    Ok(api),
                ) => match format {
                    // json: today verbatim — byte-identical wire
                    // body, file write, and payload (the JSON lock).
                    cli::TransferFormat::Json => actions::tags::tags_export(
                        &*api,
                        project,
                        paths,
                        export_out
                            .as_ref()
                            .expect("resolved pre-resolution")
                            .as_deref(),
                        actions::tags::ExportFormat::Json,
                    )
                    .await
                    .map(ActionOutput::TagsExport),
                    // xml: RAW gateway bytes — file mode writes them
                    // with NO trailing newline, stdout mode carries
                    // them for the render layer's raw write (the
                    // fourth-exception pattern extended).
                    cli::TransferFormat::Xml => actions::tags::tags_export(
                        &*api,
                        project,
                        paths,
                        export_out
                            .as_ref()
                            .expect("resolved pre-resolution")
                            .as_deref(),
                        actions::tags::ExportFormat::Xml,
                    )
                    .await
                    .map(ActionOutput::TagsExport),
                    // csv: the gateway cannot export CSV — the JSON
                    // interchange is fetched exactly as today (same
                    // wire body) and generate_legacy_csv (the ONE
                    // sanctioned CLI-side conversion) runs over the
                    // subtrees; every drop/coercion rides the
                    // envelope as data.loss_report (warn-and-continue,
                    // exit 0 — NEVER gated). Mixed-parent note (11-01
                    // Probe 4): mixed-parent export silently corrupts
                    // (type="Unknown" skeletons) — PRE-EXISTING in the
                    // JSON interchange (probe 4c), NOT a hard failure —
                    // so per the planner lock the constraint is not
                    // new-format and no pre-resolution gate fires here.
                    cli::TransferFormat::Csv => export_csv_arm(
                        &*api,
                        project,
                        paths,
                        export_out
                            .as_ref()
                            .expect("resolved pre-resolution")
                            .as_deref(),
                    )
                    .await
                    .map(ActionOutput::TagsExport),
                },
                (
                    TagsCommand::Import {
                        provider,
                        project,
                        collision_policy,
                        format,
                        ..
                    },
                    Ok(api),
                ) => {
                    let bytes = import_payload.as_ref().expect("parsed pre-resolution");
                    // json: byte-identical body (the parsed Value
                    // re-serializes); xml/csv: the raw bytes ride
                    // base64 to importTagsFile verbatim (11-04's
                    // base64-only seam). The loss gate already ran
                    // pre-resolution; a confirmed import attaches the
                    // structured scan summary as data.loss_report.
                    let import_format = match format {
                        cli::TransferFormat::Json => actions::tags::ImportFormat::Json,
                        cli::TransferFormat::Xml => actions::tags::ImportFormat::Xml,
                        cli::TransferFormat::Csv => actions::tags::ImportFormat::Csv,
                    };
                    actions::tags::tags_import(
                        &*api,
                        project,
                        provider,
                        bytes,
                        (*collision_policy).into(),
                        import_format,
                    )
                    .await
                    .map(|mut result| {
                        result.loss_report = import_loss.clone();
                        ActionOutput::TagsImport(result)
                    })
                }
                (
                    TagsCommand::Alarms(TagsAlarmsCommand::Active {
                        source,
                        priority,
                        state,
                        project,
                    }),
                    Ok(api),
                ) => actions::tags::tags_alarms_active(
                    &*api,
                    project,
                    source.as_deref(),
                    priority.as_deref(),
                    state.as_deref(),
                )
                .await
                .map(ActionOutput::TagsAlarmsActive),
                (TagsCommand::Alarms(TagsAlarmsCommand::History { project, .. }), Ok(api)) => {
                    let (start_ms, end_ms) = time_args.expect("parsed pre-resolution");
                    actions::tags::tags_alarms_history(&*api, project, start_ms, end_ms)
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
                    &*api,
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
                        &*api,
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
                // guarded:rig reset
                RigCommand::Reset { .. } => Some("rig reset"),
                // guarded:rig restore
                RigCommand::Restore { .. } => Some("rig restore"),
                // guarded:rig trial reset
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
                            .map(|session| session.api() as &dyn ignition_core::client::GatewayApi);
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
                            Some(api) => actions::rig::trial_status(&*api)
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
                                        &*api,
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
                                actions::rig::rig_snapshot(&*api, &plan.name, output.as_deref())
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
                                actions::rig::rig_restore(&*api, &rig_url, &file, timeout)
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
                let (name, api) = resolve_gateway_api(cli.profile.as_deref());
                let result = match api {
                    Ok(api) => {
                        let stem = name.as_deref().unwrap_or("gateway");
                        actions::backup::backup_download(
                            &*api,
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
                // guarded:backup restore
                if let Err(err) = require_confirmation(
                    cli.yes,
                    "backup restore (overwrites this gateway's state from the gwbk — \
                     gateway restarts and blocks ~minutes)",
                ) {
                    return (None, Err(err));
                }
                let (name, api) = resolve_gateway_api(cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::backup::backup_restore(&*api, &file)
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
                let (name, api) = resolve_gateway_api(cli.profile.as_deref());
                let result = match api {
                    Ok(api) => actions::eam::eam_history(&*api, limit, search.as_deref())
                        .await
                        .map(ActionOutput::EamHistory),
                    Err(err) => Err(err),
                };
                (name, result)
            }
            EamCommand::Tasks { name: task_name } => {
                let (name, api) = resolve_gateway_api(cli.profile.as_deref());
                let result = match api {
                    Ok(api) => match task_name {
                        Some(task_name) => actions::eam::eam_task_detail(&*api, &task_name)
                            .await
                            .map(ActionOutput::EamTaskDetail),
                        None => actions::eam::eam_tasks(&*api)
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
                            // guarded:eam task new
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
                    let (name, api) = resolve_gateway_api(cli.profile.as_deref());
                    let result = match api {
                        Ok(api) => actions::eam::eam_task_create(
                            &*api,
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
                // The guarded lifecycle/mutation verbs (10-04): the
                // TWO-TIER flow — (a) the PURE precheck first (exit
                // 2, zero network); (b) resolution; (c) the
                // blast-radius preview fetch (a bad name refuses
                // `not_found` BEFORE any prompt — the preview IS the
                // pre-flight); (d) `require_confirmation` whose
                // operation string IS the preview line (the refusal
                // message names task + agents + impact); (e) the
                // action (the authoritative re-checks inside core).
                // The guard stays PRE-WRITE: the preview fetch may
                // read, but zero writes fire without --yes.
                EamTaskCommand::Force { name: task_name } => {
                    if let Err(err) = actions::eam::lifecycle_precheck("force", &task_name) {
                        return (None, Err(err));
                    }
                    let (name, api) = resolve_gateway_api(cli.profile.as_deref());
                    let result = match api {
                        Ok(api) => {
                            match preview_then_confirm(&*api, "force", &task_name, cli.yes).await {
                                Ok(()) => actions::eam::eam_task_force(&*api, &task_name)
                                    .await
                                    .map(ActionOutput::EamTaskForce),
                                Err(err) => Err(err),
                            }
                        }
                        Err(err) => Err(err),
                    };
                    (name, result)
                }
                EamTaskCommand::Suspend { name: task_name } => {
                    if let Err(err) = actions::eam::lifecycle_precheck("suspend", &task_name) {
                        return (None, Err(err));
                    }
                    let (name, api) = resolve_gateway_api(cli.profile.as_deref());
                    let result = match api {
                        Ok(api) => {
                            match preview_then_confirm(&*api, "suspend", &task_name, cli.yes).await
                            {
                                Ok(()) => actions::eam::eam_task_suspend(&*api, &task_name)
                                    .await
                                    .map(ActionOutput::EamTaskLifecycle),
                                Err(err) => Err(err),
                            }
                        }
                        Err(err) => Err(err),
                    };
                    (name, result)
                }
                EamTaskCommand::Resume { name: task_name } => {
                    if let Err(err) = actions::eam::lifecycle_precheck("resume", &task_name) {
                        return (None, Err(err));
                    }
                    let (name, api) = resolve_gateway_api(cli.profile.as_deref());
                    let result = match api {
                        Ok(api) => {
                            match preview_then_confirm(&*api, "resume", &task_name, cli.yes).await {
                                Ok(()) => actions::eam::eam_task_resume(&*api, &task_name)
                                    .await
                                    .map(ActionOutput::EamTaskLifecycle),
                                Err(err) => Err(err),
                            }
                        }
                        Err(err) => Err(err),
                    };
                    (name, result)
                }
                EamTaskCommand::Cancel { name: task_name } => {
                    if let Err(err) = actions::eam::lifecycle_precheck("cancel", &task_name) {
                        return (None, Err(err));
                    }
                    let (name, api) = resolve_gateway_api(cli.profile.as_deref());
                    let result = match api {
                        Ok(api) => {
                            match preview_then_confirm(&*api, "cancel", &task_name, cli.yes).await {
                                Ok(()) => actions::eam::eam_task_cancel(&*api, &task_name)
                                    .await
                                    .map(ActionOutput::EamTaskLifecycle),
                                Err(err) => Err(err),
                            }
                        }
                        Err(err) => Err(err),
                    };
                    (name, result)
                }
                // Modify's Tier-0 pure stage validates flag sanity
                // BEFORE resolution: --setting K=Vs parse via
                // parse_setting (malformed K=V is exit 2 with zero
                // network — the tags-write byte-source precedent) and
                // an all-nothing change refuses pre-network (a no-op
                // PUT would still rotate the server-side signature).
                // enable+disable conflicts are clap's job.
                EamTaskCommand::Modify {
                    name: task_name,
                    enable,
                    disable,
                    schedule_mode,
                    setting,
                    description,
                } => {
                    let mut overlay = serde_json::Map::new();
                    for raw in &setting {
                        match actions::eam::parse_setting(raw) {
                            Ok((key, value)) => {
                                overlay.insert(key, value);
                            }
                            Err(err) => return (None, Err(err)),
                        }
                    }
                    let settings_overlay = if overlay.is_empty() {
                        None
                    } else {
                        Some(serde_json::Value::Object(overlay))
                    };
                    let change = actions::eam::TaskChange {
                        enabled: if enable {
                            Some(true)
                        } else if disable {
                            Some(false)
                        } else {
                            None
                        },
                        description,
                        schedule_mode,
                        settings_overlay,
                    };
                    if change.enabled.is_none()
                        && change.description.is_none()
                        && change.schedule_mode.is_none()
                        && change.settings_overlay.is_none()
                    {
                        return (
                            None,
                            Err(CoreError::InvalidInput {
                                reason: format!(
                                    "eam task modify {task_name:?}: no targeted keys — a modify \
                                     must change something (enabled / description / schedule-mode \
                                     / settings overlay)"
                                ),
                            }),
                        );
                    }
                    if let Err(err) = actions::eam::lifecycle_precheck("modify", &task_name) {
                        return (None, Err(err));
                    }
                    let (name, api) = resolve_gateway_api(cli.profile.as_deref());
                    let result = match api {
                        Ok(api) => {
                            match preview_then_confirm(&*api, "modify", &task_name, cli.yes).await {
                                Ok(()) => actions::eam::eam_task_modify(&*api, &task_name, change)
                                    .await
                                    .map(ActionOutput::EamTaskModify),
                                Err(err) => Err(err),
                            }
                        }
                        Err(err) => Err(err),
                    };
                    (name, result)
                }
                EamTaskCommand::Delete { name: task_name } => {
                    if let Err(err) = actions::eam::lifecycle_precheck("delete", &task_name) {
                        return (None, Err(err));
                    }
                    let (name, api) = resolve_gateway_api(cli.profile.as_deref());
                    let result = match api {
                        Ok(api) => {
                            match preview_then_confirm(&*api, "delete", &task_name, cli.yes).await {
                                Ok(()) => actions::eam::eam_task_delete(&*api, &task_name)
                                    .await
                                    .map(ActionOutput::EamTaskDelete),
                                Err(err) => Err(err),
                            }
                        }
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
                match Session::resolve(cli.profile.as_deref()) {
                    Ok(session) => {
                        let name = session.profile_name().to_string();
                        let result = actions::script::script_run(
                            &*session, &config, &name, &project, &script,
                        )
                        .await
                        .map(ActionOutput::ScriptRun);
                        (Some(name), result)
                    }
                    Err(err) => (error_profile(&err), Err(err)),
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
        // `ign api call` (09-03, EXT-01): the raw passthrough escape
        // hatch. Usage-class guards LEAD — header/query string parsing,
        // the auth-pattern refusal, and path validation all run
        // PRE-resolve (exit 2, envelope profile null, ZERO client
        // construction or gateway requests — the sync-guard
        // convention), then `Session::resolve` (the Phase-8 seam — NO
        // second construction path), then the action (whose own
        // re-check keeps in-process callers honest). NEVER hook clap
        // for the refusal — it renders as a contract error envelope
        // (the frozen rule).
        Commands::Api(ApiArgs {
            command: ApiCommand::Call(args),
        }) => {
            let call = match build_api_call_request(&args) {
                Ok(call) => call,
                Err(err) => return (None, Err(err)),
            };
            if let Err(err) = ignition_core::client::apicall::refuse_auth_headers(&call.headers) {
                return (None, Err(err));
            }
            if let Err(err) = ignition_core::client::apicall::validate_path(&call.path) {
                return (None, Err(err));
            }
            match Session::resolve(cli.profile.as_deref()) {
                Ok(session) => {
                    let name = session.profile_name().to_string();
                    let result = actions::apicall::api_call(&*session, call)
                        .await
                        .map(ActionOutput::ApiCall);
                    (Some(name), result)
                }
                Err(err) => (error_profile(&err), Err(err)),
            }
        }
        // The three curated morning-check reads (09-04, EXT-02):
        // `Session::resolve` (the Phase-8 seam — NO second
        // construction path) → the action, one command per read. All
        // authed (/data routes under 8.3 default security — exit 3
        // without a credential, the inspection-command rule).
        Commands::License(LicenseArgs {
            command: LicenseCommand::Status,
        }) => match Session::resolve(cli.profile.as_deref()) {
            Ok(session) => {
                let name = session.profile_name().to_string();
                let result = actions::license::license_status(&*session)
                    .await
                    .map(ActionOutput::LicenseStatus);
                (Some(name), result)
            }
            Err(err) => (error_profile(&err), Err(err)),
        },
        Commands::Redundancy(RedundancyArgs {
            command: RedundancyCommand::Status,
        }) => match Session::resolve(cli.profile.as_deref()) {
            Ok(session) => {
                let name = session.profile_name().to_string();
                let result = actions::redundancy::redundancy_status(&*session)
                    .await
                    .map(ActionOutput::RedundancyStatus);
                (Some(name), result)
            }
            Err(err) => (error_profile(&err), Err(err)),
        },
        Commands::Gan(GanArgs {
            command: GanCommand::Status,
        }) => match Session::resolve(cli.profile.as_deref()) {
            Ok(session) => {
                let name = session.profile_name().to_string();
                let result = actions::gan::gan_status(&*session)
                    .await
                    .map(ActionOutput::GanStatus);
                (Some(name), result)
            }
            Err(err) => (error_profile(&err), Err(err)),
        },
        // The diagnostics-bundle family (09-05, EXT-02): the same
        // Session::resolve shape as the morning-check reads (all
        // authed /data routes — exit 3 without a credential).
        // generate/status/download are one-shot; wait rides the poll
        // engine (deadline → exit 4 network_error, no new slug).
        // Nothing here is destructive — no --yes guard (the
        // backup-download posture).
        Commands::Diagnostics(DiagnosticsArgs {
            command: DiagnosticsCommand::Bundle(command),
        }) => match command {
            BundleCommand::Generate => match Session::resolve(cli.profile.as_deref()) {
                Ok(session) => {
                    let name = session.profile_name().to_string();
                    let result = actions::diagnostics::bundle_generate(&*session)
                        .await
                        .map(ActionOutput::BundleGenerate);
                    (Some(name), result)
                }
                Err(err) => (error_profile(&err), Err(err)),
            },
            BundleCommand::Status => match Session::resolve(cli.profile.as_deref()) {
                Ok(session) => {
                    let name = session.profile_name().to_string();
                    let result = actions::diagnostics::bundle_status(&*session)
                        .await
                        .map(ActionOutput::BundleStatus);
                    (Some(name), result)
                }
                Err(err) => (error_profile(&err), Err(err)),
            },
            BundleCommand::Download { output } => match Session::resolve(cli.profile.as_deref()) {
                Ok(session) => {
                    let name = session.profile_name().to_string();
                    let result =
                        actions::diagnostics::bundle_download(&*session, output.as_deref())
                            .await
                            .map(ActionOutput::BundleDownload);
                    (Some(name), result)
                }
                Err(err) => (error_profile(&err), Err(err)),
            },
            BundleCommand::Wait { interval, timeout } => {
                match Session::resolve(cli.profile.as_deref()) {
                    Ok(session) => {
                        let name = session.profile_name().to_string();
                        let result = actions::diagnostics::bundle_wait(
                            &*session,
                            std::time::Duration::from_secs(interval),
                            std::time::Duration::from_secs(timeout),
                        )
                        .await
                        .map(ActionOutput::BundleWait);
                        (Some(name), result)
                    }
                    Err(err) => (error_profile(&err), Err(err)),
                }
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
        // Runtime-unreachable: main returns early for Edit (13-08's
        // OutOfBand seam dispatches on its own BEFORE the normal
        // chassis — a success variant would route through render_ok's
        // stdout, which the zero-stdout contract forbids); the arm
        // exists only for match exhaustiveness.
        Commands::Edit(_) => {
            unreachable!("edit handled by dispatch_edit before the chassis")
        }
        // Runtime-unreachable: main returns early for Mcp (14-01's
        // OutOfBand seam — the protocol stream IS stdout, so no
        // ActionOutput variant may exist for it); the arm exists only
        // for match exhaustiveness.
        Commands::Mcp(_) => {
            unreachable!("mcp handled by mcp::serve before the chassis")
        }
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

/// `ign edit`'s OutOfBand dispatch (13-08): the kubectl-edit loop with
/// the guard composed at ONE site. Order is the contract:
///
/// 1. **editor env FIRST** — `VISUAL`/`EDITOR` resolution refuses
///    PRE-resolve (usage-class: exit 2, envelope profile null, ZERO
///    requests — the api-call usage-guard convention);
/// 2. `Session::resolve` — the Phase-8 seam, no second construction
///    path;
/// 3. 13-05's `edit_pipeline` — fetch → decode → $EDITOR →
///    content-decided no-op → fail-closed encode → staleness gate →
///    staged payload. Its refusals — the stable `no $EDITOR set`,
///    codec-verbatim, `changed on gateway since fetch`, and
///    `preserved at <path>` prefixes — propagate VERBATIM through the
///    standard error envelope. The staleness gate stays NOT
///    --yes-able: dispatch adds no override.
/// 4. **THE ONE GATE** — the staged diff summary is composed once:
///    without `--yes` it IS the refusal (`require_confirmation`'s
///    operation string, the 10-04 preview_then_confirm shape); with
///    it, the same text renders as the blast-radius prose ahead of
///    the push.
/// 5. the existing project-import client call pushes the staged zip
///    (`overwrite=true` — replace, the resource-put precedent;
///    ImportDenied honesty rides the client seam).
///
/// Zero stdout in every mode: success prose is stderr `eprintln!`s
/// and NOTHING routes through render_ok (no ActionOutput::Edit
/// variant exists BY DESIGN).
async fn dispatch_edit(
    args: EditArgs,
    profile_flag: Option<&str>,
    yes: bool,
    mode: RenderMode,
) -> ExitCode {
    use actions::edit::{EditStatus, TokioEditor};

    // 1. The editor env resolves BEFORE any profile/secret/request
    //    work: a missing VISUAL/EDITOR is a usage-class refusal (the
    //    profile never resolves, so the envelope echoes null).
    if let Err(err) = TokioEditor::resolve_command() {
        render_error(&err, None, mode);
        return ExitCode::from(err.exit_code());
    }
    // 2. The one session seam.
    let session = match Session::resolve(profile_flag) {
        Ok(session) => session,
        Err(err) => {
            render_error(&err, error_profile(&err).as_deref(), mode);
            return ExitCode::from(err.exit_code());
        }
    };
    let profile_name = session.profile_name().to_string();
    // 3. The pipeline; every refusal rides the standard envelope
    //    verbatim (stable prefixes intact).
    let staged = match actions::edit::edit_pipeline(
        &*session,
        &TokioEditor,
        &args.project,
        Some(&args.resource_path),
    )
    .await
    {
        Ok(staged) => staged,
        Err(err) => {
            render_error(&err, Some(&profile_name), mode);
            return ExitCode::from(err.exit_code());
        }
    };
    match staged.status {
        // Content-decided no-op: no push, no prompt, clean exit 0.
        EditStatus::NoOp => {
            eprintln!("edit: no changes — nothing pushed");
            ExitCode::SUCCESS
        }
        EditStatus::Ready { changed } => {
            let zip = staged
                .import_zip
                .expect("Ready always carries the staged import zip");
            // 4. THE ONE GATE (the 10-04 preview_then_confirm shape):
            //    the summary text is composed once — it IS the refusal
            //    without --yes, the blast-radius prose with it.
            let summary = render_edit_summary(&args.project, &changed);
            // guarded:edit
            if let Err(err) = require_confirmation(yes, &summary) {
                render_error(&err, Some(&profile_name), mode);
                return ExitCode::from(err.exit_code());
            }
            eprintln!("{summary}");
            // 5. The existing project-import client call with the
            //    staged zip — overwrite replace (the resource-put
            //    precedent); the client seam carries the ImportDenied
            //    honesty.
            match session.project_import(&args.project, zip, true).await {
                Ok(_) => {
                    eprintln!(
                        "edit: pushed {} member(s) to {}",
                        changed.len(),
                        args.project
                    );
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    render_error(&err, Some(&profile_name), mode);
                    ExitCode::from(err.exit_code())
                }
            }
        }
    }
}

/// The staged edit's changed-members summary — the blast radius. The
/// SAME text is the refusal message (without `--yes`) and the
/// pre-push prose (with it); it mirrors the workspace push preview's
/// shape (write lines under a count header, 13-06's
/// render_push_preview genre). Lives here, NOT in render.rs — edit's
/// prose is deliberately dispatch-arm stderr output (OutOfBand: no
/// envelope involvement).
fn render_edit_summary(project: &str, changed: &[String]) -> String {
    let mut summary = format!("edit would write {} member(s) to {project}", changed.len());
    for path in changed {
        summary.push_str(&format!("\n  write: {path}"));
    }
    summary
}

/// Selection-only profile resolution for the TWO consumers that need the
/// resolved NAME/config VIEW rather than a gateway client (`profile list`
/// and the two-client envelope echo): mirror the selection precedence
/// (flag > config.active) to scope the `IGNITION_URL` env overlay, then
/// resolve and validate the selection. NO secret is walked and NO client
/// is built here — all client construction is core-owned
/// ([`Session::resolve`] family); this survives because those consumers
/// must not demand the active profile's credential (a secret-less active
/// profile can still diff/sync two named sides, and `profile list` runs
/// on a fresh install).
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
    flag: Option<&str>,
    inspection: Inspection,
) -> (Option<String>, Result<ActionOutput, CoreError>) {
    let (name, api) = resolve_gateway_api(flag);
    let result = match api {
        Ok(api) => match inspection {
            Inspection::Status => actions::inspect::status(&*api)
                .await
                .map(ActionOutput::Status),
            Inspection::Modules(quarantined) => actions::inspect::modules(&*api, quarantined)
                .await
                .map(ActionOutput::Modules),
            Inspection::Metrics(history) => actions::inspect::metrics(&*api, history)
                .await
                .map(ActionOutput::Metrics),
        },
        Err(err) => Err(err),
    };
    (name, result)
}

/// Profile + REQUIRED credential + client for gateway commands — a thin
/// delegate onto [`Session::resolve`] (the seam owns load → overlay →
/// selection → LOCKED chain → construction; CORE-09 dissolved this file's
/// duplicated choreography). `None` profile → `NoActiveProfile`; a missing
/// secret is `SecretUnavailable` (exit 3), correct for authed reads. The
/// profile name travels out even on failure so the error envelope echoes
/// it (CORE-01).
fn resolve_gateway_api(flag: Option<&str>) -> (Option<String>, Result<Session, CoreError>) {
    match Session::resolve(flag) {
        Ok(session) => (Some(session.profile_name().to_string()), Ok(session)),
        Err(err) => (error_profile(&err), Err(err)),
    }
}

/// Resolve ONE named profile side to a session — a thin delegate onto
/// [`Session::resolve_side`] (per-side resolution, the seam's locked
/// shape: each side walks the one chain independently, and the env
/// overlay does NOT re-target the sides — only the envelope's active
/// selection is overlaid, which the caller's already-loaded config
/// carries).
fn named_profile_client(config: &mut Config, name: &str) -> Result<Session, CoreError> {
    Session::resolve_side(config, name)
}

/// THE two-client resolution shape (07-01, 07-RESEARCH Pattern): the
/// ENVELOPE's active profile resolves for its NAME exactly as every
/// other command (flag > active, env overlay scoped to that selection —
/// no secret demanded: a credential-less active profile can still drive
/// diff/sync between two named sides), then each positional side builds
/// its own session through the seam. Each side's secret chain resolves
/// INDEPENDENTLY: `IGNITION_TOKEN` (and the basic env pair) applies to
/// BOTH sides unless per-profile keyring entries exist — the README's
/// two-sided-secret caveat.
fn resolve_two_clients(
    config: &mut Config,
    flag: Option<&str>,
    name_a: &str,
    name_b: &str,
) -> (Option<String>, Result<(Session, Session), CoreError>) {
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

/// Profile + HEADER-LESS-tolerant session for the unauthenticated wait
/// commands (`wait gateway`, `wait restart`) — a thin delegate onto
/// [`Session::resolve_degraded`]: credential resolution DEGRADES to None
/// — StatusPing answers with no credential, so a missing/broken secret
/// must never block readiness polling (the whole point: these waits work
/// when auth is broken). Other credential errors still propagate.
fn resolve_headerless_api(flag: Option<&str>) -> (Option<String>, Result<Session, CoreError>) {
    match Session::resolve_degraded(flag) {
        Ok(session) => (Some(session.profile_name().to_string()), Ok(session)),
        Err(err) => (error_profile(&err), Err(err)),
    }
}

/// The rig family's commissioned-wait probe: a HEADER-LESS client
/// pointed at the rig's OWN derived gateway URL (never the profile's
/// gateway) — `/StatusPing` answers unauthenticated, so the up/reset
/// waits work even with no credential at all. `ssl_verify=false`:
/// localhost probes against self-signed rig https are the norm.
/// Shared by `rig up` and `rig reset` (both end in the wait).
fn commissioned_probe(plan: &ignition_core::rig::RigPlan) -> Option<Session> {
    rig_gateway_client(plan, None)
}

/// A client pointed at the rig's OWN derived gateway URL (the
/// `commissioned_probe` generalized for the trial verbs: an optional
/// credential rides along — tier 0's token when `IGNITION_TOKEN` is
/// set; the trial endpoints tolerate headers either way,
/// live-verified). `None` when no gateway port is derivable. The URL
/// DERIVATION stays here; the construction is [`Session::for_url`] —
/// headerless-by-construction, no config read.
fn rig_gateway_client(
    plan: &ignition_core::rig::RigPlan,
    credential: Option<Credential>,
) -> Option<Session> {
    actions::rig::gateway_url_from(plan)
        .and_then(|url| Session::for_url(url.parse().ok()?, credential, false).ok())
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

/// The profile NAME an error carries, when it does — `SecretUnavailable`
/// names the profile whose chain exhausted, so the error envelope still
/// echoes `[profile: NAME]` (CORE-01 threading) even though the seam no
/// longer hands the resolved name across its error path. Selection-class
/// errors (`profile_not_found`, `no_active_profile`) carry no name — the
/// envelope stays `null`, as today.
fn error_profile(err: &CoreError) -> Option<String> {
    match err {
        CoreError::SecretUnavailable { profile } => Some(profile.clone()),
        _ => None,
    }
}

/// A non-empty env var, when set.
fn env_non_empty(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
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

/// The guarded-verb registry (14-01): ONE const enumerating every
/// `--yes`-guarded clap leaf as `(clap leaf path, refusal operation
/// prose)` pairs. `--yes` is a GLOBAL arg, so guardedness is invisible
/// to the clap walk — the guards live as `require_confirmation` call
/// sites in the dispatch arms. This const is the MCP catalog's single
/// source for the synthetic required `confirm` boolean property
/// (mcp.rs reads ONLY this — SC-2), and the drift test in `mod tests`
/// pins it against the live dispatch sites (the 12-03 grep-CI genre:
/// a new guarded verb landing unregistered goes red — the safety
/// direction).
///
/// Prose column: for STATIC-literal sites, the exact
/// `require_confirmation` literal (the drift test byte-matches it);
/// for DYNAMIC-prose sites (`&format!`/variable second args), a
/// representative rendering of the refusal text. Every site also
/// carries an adjacent marker comment keyed to the path column — the
/// drift test walks both directions.
pub(crate) const GUARDED_OPS: &[(&str, &str)] = &[
    // 02-03: the first destructive verb (sessions terminate).
    ("sessions terminate", "sessions terminate"),
    // 05-02: logger-level writes rewrite the registry.
    ("logs loggers set", "logs loggers set"),
    ("logs loggers reset", "logs loggers reset"),
    // 02-04: gateway restart.
    ("restart", "restart"),
    // 02-04: gateway restart.
    // 03-01: the project family's ONE destructive verb.
    ("project delete", "project delete"),
    // 03-02: overwrite-import REPLACES the whole project.
    (
        "project import",
        "project import --collision-policy overwrite",
    ),
    // 07-01: the guarded cross-gateway promotion (dynamic prose
    // names the overwritten target profile B).
    (
        "project sync",
        "project sync (overwrite-import the whole project on <profile-b> — \
         replaces concurrent Designer edits)",
    ),
    // 05-02: resource put/delete re-import the project wholesale.
    (
        "resource put",
        "resource put (re-imports the project; concurrent Designer edits are replaced)",
    ),
    (
        "resource delete",
        "resource delete (re-imports the project; concurrent Designer edits are replaced)",
    ),
    // 05-04: the tags guards (one shared match table above the
    // family's single require_confirmation call).
    ("tags provider delete", "tags provider delete"),
    ("tags config delete", "tags config delete"),
    ("tags import", "tags import --collision-policy overwrite"),
    // 04-01/04-03: the rig guards (one shared match table).
    ("rig reset", "rig reset"),
    ("rig restore", "rig restore"),
    ("rig trial reset", "rig trial reset"),
    // 07-02: standalone restore overwrites THIS gateway's state.
    (
        "backup restore",
        "backup restore (overwrites this gateway's state from the gwbk — gateway \
         restarts and blocks ~minutes)",
    ),
    // 07-02 Task 3: the eam guard ladder (dynamic prose names WHICH
    // rung fired — the schedule mode or the mutating task type).
    (
        "eam task new",
        "eam task new (scheduleMode <mode> arms autonomous gateway actions)",
    ),
    // 10-04: the two-tier blast-radius verbs — the refusal prose IS
    // the render_preview_line output (task + agent targets + impact),
    // so these carry representative prose.
    (
        "eam task force",
        "eam task force <task> <blast-radius preview line>",
    ),
    (
        "eam task suspend",
        "eam task suspend <task> <blast-radius preview line>",
    ),
    (
        "eam task resume",
        "eam task resume <task> <blast-radius preview line>",
    ),
    (
        "eam task cancel",
        "eam task cancel <task> <blast-radius preview line>",
    ),
    (
        "eam task modify",
        "eam task modify <task> <blast-radius preview line>",
    ),
    (
        "eam task delete",
        "eam task delete <task> <blast-radius preview line>",
    ),
    // 13-08: the kubectl-edit loop's push gate (dynamic prose: the
    // staged-changed summary IS the refusal message).
    ("edit", "edit would write <N> member(s) to <project>"),
];

/// The guarded lifecycle verbs' Tier-2 gate (10-04): the blast-radius
/// preview fetch (read-only — a bad task name refuses `not_found`
/// HERE, before any prompt) followed by `require_confirmation` whose
/// operation string IS the rendered preview line — so the refusal
/// message itself names the task, its agent targets, and the factual
/// impact. Post-resolution refusals carry the resolved profile; the
/// guard stays PRE-WRITE (zero mutations without --yes).
async fn preview_then_confirm(
    api: &dyn ignition_core::client::GatewayApi,
    verb: &str,
    task_name: &str,
    yes: bool,
) -> Result<(), CoreError> {
    let preview = actions::eam::build_blast_radius(api, verb, task_name).await?;
    let operation = actions::eam::render_preview_line(&preview);
    // guarded:eam task force
    // guarded:eam task suspend
    // guarded:eam task resume
    // guarded:eam task cancel
    // guarded:eam task modify
    // guarded:eam task delete
    require_confirmation(yes, &operation)
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

/// Read a document from `--file PATH` (std::fs) or `--file -`
/// (tokio stdin) as RAW BYTES — the xml/csv import source (the
/// transfer path never parses: bytes ride `file_b64` verbatim,
/// 11-04's base64-only seam). InvalidInput class, pre-resolution:
/// exit 2 with zero network work on an unreadable file (the
/// read_json_input precedent).
async fn read_input_bytes(file: &std::path::Path) -> Result<Vec<u8>, CoreError> {
    let label = if file == std::path::Path::new("-") {
        "stdin".to_string()
    } else {
        file.display().to_string()
    };
    if file == std::path::Path::new("-") {
        use tokio::io::AsyncReadExt;
        let mut buffer = Vec::new();
        tokio::io::stdin()
            .read_to_end(&mut buffer)
            .await
            .map_err(|err| CoreError::InvalidInput {
                reason: format!("cannot read stdin: {err}"),
            })?;
        Ok(buffer)
    } else {
        std::fs::read(file).map_err(|err| CoreError::InvalidInput {
            reason: format!("cannot read {label}: {err}"),
        })
    }
}

/// Read a JSON document from `--file PATH` (std::fs) or `--file -`
/// (tokio stdin) and PARSE it — the resource-put byte-source
/// precedent (InvalidInput class, pre-resolution: exit 2 with zero
/// network work on an unreadable file or malformed JSON). Used by
/// `tags config create|edit` (the definition) and `tags import`
/// (the json payload — the dispatch re-serializes value-identically,
/// so the wire body stays byte-identical).
async fn read_json_input(file: &std::path::Path) -> Result<serde_json::Value, CoreError> {
    let bytes = read_input_bytes(file).await?;
    let label = if file == std::path::Path::new("-") {
        "stdin".to_string()
    } else {
        file.display().to_string()
    };
    serde_json::from_slice(&bytes).map_err(|err| CoreError::InvalidInput {
        reason: format!("{label} is not valid JSON: {err}"),
    })
}

/// THE TAGS-12 loss gate (11-05) — ONE function (the 10-04
/// preview_then_confirm lesson: one gate means the refusal shape
/// cannot drift). Scans the RAW xml/csv input bytes (the 11-03
/// advisory scans — pure, no wire) and, when facts are reported and
/// `--yes` is absent, refuses exit-2 `invalid_input` with the prose
/// report AS the message (rendered to stderr in every mode via the
/// established error path). PRE-RESOLUTION: this runs before any
/// profile resolution or request, so a refusal does zero wire work.
/// Imports ONLY — csv GENERATION warns-and-continues and is never
/// gated. With `--yes` the structured scan summary returns for the
/// success envelope (`data.loss_report`); json skips the gate
/// entirely (the byte-identical path).
fn loss_gate(
    bytes: &[u8],
    format: cli::TransferFormat,
    yes: bool,
) -> Result<Option<actions::tags::LossReport>, CoreError> {
    let (label, scan_names, facts) = match format {
        cli::TransferFormat::Json => return Ok(None),
        cli::TransferFormat::Xml => {
            let scan = actions::tag_loss::scan_xml(bytes);
            ("xml", scan.top_level_names, scan.facts)
        }
        cli::TransferFormat::Csv => {
            let scan = actions::tag_loss::scan_csv(bytes);
            ("csv", scan.top_level_names, scan.facts)
        }
    };
    if facts.is_empty() {
        return Ok(None);
    }
    let prose = render_loss_prose(label, &facts, &scan_names);
    if !yes {
        // The prose header "loss report (" is LOSS_GATE_REFUSAL_REASON_PREFIX
        // (ignition-core::error) — hint() content-addresses the --yes hint off
        // it. Changing render_loss_prose's header changes the hint; the
        // contract_tags loss-gate pins are the drift guard.
        return Err(CoreError::InvalidInput {
            reason: format!("{prose}re-run with --yes to import anyway"),
        });
    }
    Ok(Some(actions::tags::LossReport {
        format: if matches!(format, cli::TransferFormat::Xml) {
            "xml"
        } else {
            "csv"
        },
        facts,
        top_level_names: scan_names,
        dropped_keys: Vec::new(),
        coerced: Vec::new(),
        rows: 0,
    }))
}

/// The loss report's prose form — the refusal message's content (and
/// the shape agents read from the error envelope's `error.message`):
/// the stable scan codes in brackets, one fact per line, then the
/// top-level names the import would land. Advisory by contract — the
/// gateway remains the parsing authority; the scan never refuses on
/// its own (a clean scan imports without --yes).
fn render_loss_prose(
    label: &str,
    facts: &[actions::tag_loss::LossFact],
    names: &[String],
) -> String {
    let mut out = format!(
        "loss report ({label}): the scan reports {} finding(s) before the import:\n",
        facts.len()
    );
    for fact in facts {
        out.push_str(&format!("  - [{}] {}\n", fact.code, fact.detail));
    }
    if !names.is_empty() {
        out.push_str(&format!("top-level tag(s): {}\n", names.join(", ")));
    }
    out
}

/// Export's default destination: `<last-path-segment>.<ext>` with
/// the extension per format (11-05) — json keeps today's
/// `<last-segment>.json` byte-identical (the core helper owns the
/// stem sanitization; only the suffix swaps).
fn default_export_path(paths: &[String], format: cli::TransferFormat) -> std::path::PathBuf {
    let json_name = actions::tags::default_export_file_name(paths);
    let stem = json_name.strip_suffix(".json").unwrap_or(&json_name);
    let ext = match format {
        cli::TransferFormat::Json => "json",
        cli::TransferFormat::Xml => "xml",
        cli::TransferFormat::Csv => "csv",
    };
    std::path::PathBuf::from(format!("{stem}.{ext}"))
}

/// `tags export --format csv` (11-05) — the ONE sanctioned CLI-side
/// conversion. The JSON interchange is fetched exactly as today
/// (same wire body), the subtrees re-parsed from the pretty payload
/// (our own serialization — value-identical), and
/// `generate_legacy_csv` produces the legacy bytes plus the full
/// drop/coercion report. File mode writes the CSV bytes with NO
/// trailing newline beyond what the csv crate emits; stdout mode
/// carries them verbatim for the render layer's raw write.
/// Warn-and-continue: exit 0 even when the report is heavy — the
/// drops ride `data.loss_report` and human mode warns on stderr.
async fn export_csv_arm(
    api: &dyn ignition_core::client::GatewayApi,
    project: &str,
    paths: &[String],
    out: Option<&std::path::Path>,
) -> Result<actions::tags::TagsExportResult, CoreError> {
    let json_result =
        actions::tags::tags_export(api, project, paths, None, actions::tags::ExportFormat::Json)
            .await?;
    let pretty = json_result
        .payload
        .as_deref()
        .expect("stdout-mode export carries the pretty payload");
    let subtrees: Vec<serde_json::Value> = serde_json::from_str(pretty).map_err(|err| {
        CoreError::Internal(format!("csv export lost the parsed interchange: {err}"))
    })?;
    let report = actions::tags::generate_legacy_csv(&subtrees)?;
    let loss_report = actions::tags::LossReport {
        format: "csv",
        facts: Vec::new(),
        top_level_names: Vec::new(),
        dropped_keys: report.dropped_keys.clone(),
        coerced: report.coerced.clone(),
        rows: report.rows,
    };
    match out {
        Some(path) => {
            std::fs::write(path, &report.csv).map_err(|err| {
                CoreError::Internal(format!("cannot write {}: {err}", path.display()))
            })?;
            Ok(actions::tags::TagsExportResult {
                project: project.to_string(),
                paths: paths.to_vec(),
                file: Some(path.display().to_string()),
                stdout: false,
                tag_count: report.rows,
                format: "csv",
                payload: None,
                raw: None,
                loss_report: Some(loss_report),
            })
        }
        None => {
            // The csv crate writes UTF-8 by construction.
            let text = String::from_utf8(report.csv).map_err(|_| {
                CoreError::Internal("generated csv bytes are not UTF-8".to_string())
            })?;
            Ok(actions::tags::TagsExportResult {
                project: project.to_string(),
                paths: paths.to_vec(),
                file: None,
                stdout: true,
                tag_count: report.rows,
                format: "csv",
                payload: None,
                raw: Some(text),
                loss_report: Some(loss_report),
            })
        }
    }
}

/// `ign api call --header`/`--query` string parsing (09-03): headers
/// split on the FIRST `:` (values may contain colons — URLs, tokens),
/// query pairs on the FIRST `=`; both trimmed; a malformed entry is a
/// usage-class refusal (exit 2, pre-resolution — zero work). The
/// method is normalized to uppercase so the wire, the outcome echo,
/// and human rendering agree.
fn build_api_call_request(
    args: &ignition_cli::cli::ApiCallArgs,
) -> Result<ignition_core::client::apicall::ApiCallRequest, CoreError> {
    let mut headers = Vec::with_capacity(args.header.len());
    for raw in &args.header {
        let Some((name, value)) = raw.split_once(':') else {
            return Err(CoreError::InvalidInput {
                reason: format!(
                    "--header expects \"Name: Value\" (split on the first colon): {raw:?}"
                ),
            });
        };
        if name.trim().is_empty() {
            return Err(CoreError::InvalidInput {
                reason: format!("--header needs a non-empty header name: {raw:?}"),
            });
        }
        headers.push((name.trim().to_string(), value.trim().to_string()));
    }
    let mut query = Vec::with_capacity(args.query.len());
    for raw in &args.query {
        let Some((key, value)) = raw.split_once('=') else {
            return Err(CoreError::InvalidInput {
                reason: format!("--query expects \"k=v\" (split on the first '='): {raw:?}"),
            });
        };
        if key.trim().is_empty() {
            return Err(CoreError::InvalidInput {
                reason: format!("--query needs a non-empty key: {raw:?}"),
            });
        }
        query.push((key.trim().to_string(), value.trim().to_string()));
    }
    Ok(ignition_core::client::apicall::ApiCallRequest {
        method: args.method.trim().to_uppercase(),
        path: args.path.clone(),
        body: args.data.clone(),
        headers,
        query,
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
    use std::collections::BTreeSet;

    use super::{GUARDED_OPS, require_confirmation};

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

    /// Extract the SECOND top-level argument text of every
    /// `require_confirmation` call site in `src` — string-literal
    /// sites only (a `&format!`/variable second arg yields `None`:
    /// those are the dynamic-prose sites, covered by the markers).
    /// Balanced-paren walk with in-string awareness (refusal prose
    /// carries literal parens), no regex dependency. NOTE: the
    /// needle this scans for must never appear in a comment here —
    /// the scan reads the whole file including its own source.
    fn second_call_argument(call_inner: &str) -> Option<&str> {
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        for (i, b) in call_inner.bytes().enumerate() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if b == b'\\' {
                    escaped = true;
                } else if b == b'"' {
                    in_string = false;
                }
                continue;
            }
            match b {
                b'"' => in_string = true,
                b'(' => depth += 1,
                b')' => return None,
                b',' if depth == 0 => return Some(call_inner[i + 1..].trim()),
                _ => {}
            }
        }
        None
    }

    /// Unquote a Rust string-literal token: `\"` `\\` `\n` `\t` plus
    /// the line-continuation form (backslash + newline + following
    /// whitespace elided) that multi-line refusal literals use.
    fn unquote_rust_string(token: &str) -> String {
        let inner = &token[1..token.len() - 1];
        let mut out = String::new();
        let mut chars = inner.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\\' => match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('"') => out.push('"'),
                    Some('\\') => out.push('\\'),
                    Some('\n') => {
                        while chars.peek().is_some_and(|next| next.is_whitespace()) {
                            chars.next();
                        }
                    }
                    Some(other) => {
                        out.push('\\');
                        out.push(other);
                    }
                    None => {}
                },
                other => out.push(other),
            }
        }
        out
    }

    /// Scan `src` for static-literal `require_confirmation` sites and
    /// return their unquoted literals.
    fn static_guard_literals(src: &str) -> Vec<String> {
        const NEEDLE: &str = "require_confirmation(";
        let mut literals = Vec::new();
        let mut from = 0usize;
        while let Some(found) = src[from..].find(NEEDLE) {
            let start = from + found + NEEDLE.len();
            // Walk to the matching close paren (string-aware).
            let mut depth = 1usize;
            let mut in_string = false;
            let mut escaped = false;
            let mut end = None;
            for (i, b) in src[start..].bytes().enumerate() {
                if in_string {
                    if escaped {
                        escaped = false;
                    } else if b == b'\\' {
                        escaped = true;
                    } else if b == b'"' {
                        in_string = false;
                    }
                    continue;
                }
                match b {
                    b'"' => in_string = true,
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            end = Some(i);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let Some(end) = end else { break };
            let inner = &src[start..start + end];
            if let Some(second) = second_call_argument(inner)
                && second.starts_with('"')
                && second.ends_with('"')
            {
                literals.push(unquote_rust_string(second));
            }
            from = start + end;
        }
        literals
    }

    /// Extract the leaf paths from the dispatch-site marker comments
    /// (lines of the form: two slashes, a space, "guarded:", a colon,
    /// then the clap leaf path — lowercase words separated by single
    /// spaces). The validator shape keeps stray prose out of the pin.
    fn guarded_site_markers(src: &str) -> Vec<String> {
        let mut out = Vec::new();
        for line in src.lines() {
            let trimmed = line.trim_start();
            let Some(rest) = trimmed.strip_prefix("// guarded:") else {
                continue;
            };
            let path = rest.trim();
            if !path.is_empty() && path.bytes().all(|b| b.is_ascii_lowercase() || b == b' ') {
                out.push(path.to_string());
            }
        }
        out
    }

    /// The 14-01 guarded-verb drift pin (the 12-03 grep-CI genre):
    /// the scan goes red the moment a new guarded dispatch site lands
    /// UNREGISTERED — the safety direction, so the MCP catalog's
    /// synthetic `confirm` field can never silently miss a guarded
    /// verb (nor advertise one that does not exist).
    ///
    /// (a) Every STATIC guarded dispatch site — one whose second
    ///     argument is a plain string literal — contributes that
    ///     literal, and it must be the prose column of exactly one
    ///     GUARDED_OPS entry (dynamic-prose sites — second args
    ///     starting with a `&`/variable — carry representative prose
    ///     and are pinned by the markers instead).
    /// (b) Every guarded dispatch site carries an adjacent marker
    ///     comment keyed to its clap leaf path, and the marker set
    ///     pins GUARDED_OPS paths in BOTH directions.
    #[test]
    fn guarded_ops_registry_tracks_every_dispatch_site() {
        let src = include_str!("main.rs");

        // (a) static literals → exactly one prose-column entry each.
        let literals = static_guard_literals(src);
        assert!(
            !literals.is_empty(),
            "the literal scan found the guarded sites"
        );
        for literal in &literals {
            let hits: Vec<_> = GUARDED_OPS
                .iter()
                .filter(|(_, prose)| *prose == literal)
                .collect();
            assert_eq!(
                hits.len(),
                1,
                "static refusal literal {literal:?} must be the prose column of \
                 exactly one GUARDED_OPS entry"
            );
        }

        // (b) markers ↔ GUARDED_OPS paths, both directions.
        let markers = guarded_site_markers(src);
        let marker_set: BTreeSet<&str> = markers.iter().map(String::as_str).collect();
        let path_set: BTreeSet<&str> = GUARDED_OPS.iter().map(|(path, _)| *path).collect();
        let unregistered: Vec<_> = marker_set.difference(&path_set).collect();
        assert!(
            unregistered.is_empty(),
            "dispatch-site marker with no GUARDED_OPS entry (register it): {unregistered:?}"
        );
        let unmarked: Vec<_> = path_set.difference(&marker_set).collect();
        assert!(
            unmarked.is_empty(),
            "GUARDED_OPS entry with no dispatch-site marker (mark the site): {unmarked:?}"
        );
        assert_eq!(
            marker_set.len(),
            markers.len(),
            "duplicate markers at dispatch sites are forbidden"
        );
        assert_eq!(
            path_set.len(),
            GUARDED_OPS.len(),
            "duplicate GUARDED_OPS paths are forbidden"
        );
    }
}
