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

use ignition_core::actions::connections::ConnectionsResult;
use ignition_core::actions::inspect::{MetricsResult, ModulesResult, StatusResult};
use ignition_core::actions::logs::{
    DownloadResult, LogPage, ResetResult, SetLevelResult, TailResult,
};
use ignition_core::actions::projects::{
    ExportResult, ImportResult, ProjectCopyResult, ProjectDeleteResult, ProjectRenameResult,
    ProjectSetResult, ProjectsResult,
};
use ignition_core::actions::resources::{
    ResourceDeleteResult, ResourceGetResult, ResourcePutResult, ResourcesResult,
};
use ignition_core::actions::rig::{
    RigDownResult, RigLogsResult, RigResetResult, RigStatusResult, RigUpResult,
};
use ignition_core::actions::sessions::{SessionsResult, TerminateResult};
use ignition_core::client::logs::LogEntry;
use ignition_core::client::query::ListEnvelope;
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
    // The SECOND sanctioned stdout exception: a completed tail already
    // streamed every entry to stdout as it arrived (human lines or
    // NDJSON — README §Streaming); there is nothing left to render.
    // The THIRD is its sibling: `rig logs` already streamed its raw
    // compose lines in EVERY mode (passthrough — same exception).
    if matches!(out, ActionOutput::LogsTail(_) | ActionOutput::RigLogs(_)) {
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
        ActionOutput::Sessions(result) => render_sessions_human(result),
        ActionOutput::SessionsTerminate(result) => render_terminate_human(result),
        ActionOutput::Connections(result) => render_connections_human(result),
        ActionOutput::LogsList(result) => render_logs_list_human(result),
        // Unreachable: render_ok intercepts LogsTail before mode
        // dispatch (the streaming exception — entries already printed).
        ActionOutput::LogsTail(result) => render_tail_human(result),
        ActionOutput::LogsDownload(result) => render_download_human(result),
        ActionOutput::LoggersList(result) => render_loggers_human(result),
        ActionOutput::LoggerSet(result) => render_set_level_human(result),
        ActionOutput::LoggerReset(result) => render_reset_human(result),
        ActionOutput::Restart(result) => render_restart_human(result),
        ActionOutput::RestartWait(result) => render_restart_wait_human(result),
        ActionOutput::Wait(result) => render_wait_human(result),
        ActionOutput::Doctor(result) => render_doctor_human(result),
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
        ActionOutput::ProjectsList(result) => render_projects_list_human(result),
        ActionOutput::ProjectNew(result) => render_project_new_human(result),
        ActionOutput::ProjectCopy(result) => render_project_copy_human(result),
        ActionOutput::ProjectRename(result) => render_project_rename_human(result),
        ActionOutput::ProjectSet(result) => render_project_set_human(result),
        ActionOutput::ProjectDelete(result) => render_project_delete_human(result),
        ActionOutput::ProjectExport(result) => render_project_export_human(result),
        ActionOutput::ProjectImport(result) => render_project_import_human(result),
        ActionOutput::ResourcesList(result) => render_resources_list_human(result),
        ActionOutput::ResourceGet(result) => render_resource_get_human(result),
        ActionOutput::ResourcePut(result) => render_resource_put_human(result),
        ActionOutput::ResourceDelete(result) => render_resource_delete_human(result),
        ActionOutput::RigUp(result) => render_rig_up_human(result),
        ActionOutput::RigDown(result) => render_rig_down_human(result),
        ActionOutput::RigReset(result) => render_rig_reset_human(result),
        ActionOutput::RigStatus(result) => render_rig_status_human(result),
        // Unreachable: render_ok intercepts RigLogs before mode
        // dispatch (the third streaming exception — lines already
        // printed during execution).
        ActionOutput::RigLogs(result) => render_rig_logs_human(result),
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

/// `ign sessions` human lines: one section per family
/// (`designers (1)` / `perspective (2)` / `vision (0)`), a row per item
/// (`id  user/username  project  address  lastcomm`) — the webpage's
/// Sessions pages as terminal sections.
fn render_sessions_human(result: &SessionsResult) {
    println!("designers ({})", result.designers.len());
    for designer in &result.designers {
        println!(
            "{}  {}  {}  {}  {}",
            designer.id, designer.user, designer.project, designer.address, designer.lastcomm
        );
    }
    println!("perspective ({})", result.perspective.len());
    for session in &result.perspective {
        println!(
            "{}  {}  {}  {}  {}",
            session.id,
            session.username,
            session.project,
            session.client_address,
            session.last_comm
        );
    }
    println!("vision ({})", result.vision.len());
    for client in &result.vision {
        println!(
            "{}  {}  {}  {}  {}",
            client.id, client.user, client.project, client.address, client.lastcomm
        );
    }
}

/// `ign sessions terminate` human line.
fn render_terminate_human(result: &TerminateResult) {
    println!("terminated {} session {}", result.kind, result.id);
}

/// `ign connections` human lines: `database (N)` / `opc (N)` sections
/// with `name  enabled  healthchecks-as-reported` rows — healthchecks
/// render as compact JSON, verbatim (the shape is passthrough by
/// design; LOW-confidence populated detail).
fn render_connections_human(result: &ConnectionsResult) {
    println!("database ({})", result.database.len());
    for connection in &result.database {
        println!(
            "{}  {}  {}",
            connection.name,
            connection.enabled,
            serde_json::to_string(&connection.healthchecks).unwrap_or_default()
        );
    }
    println!("opc ({})", result.opc.len());
    for connection in &result.opc {
        println!(
            "{}  {}  {}",
            connection.name,
            connection.enabled,
            serde_json::to_string(&connection.healthchecks).unwrap_or_default()
        );
    }
}

/// One log entry as a human line — the SAME format the list and the
/// live tail print: `ISO-UTC  LEVEL  logger  message`. Timestamps are
/// UTC (rendered from epoch ms; the raw epoch-ms value is always in
/// --json) — no timezone machinery, deterministic everywhere.
pub fn render_log_entry_line(entry: &LogEntry) -> String {
    format!(
        "{}  {:>5}  {}  {}",
        iso_utc(entry.timestamp),
        entry.level,
        entry.logger_name,
        entry.message
    )
}

/// `ign logs` human lines: one row per entry, newest first (the query
/// sorts desc); the total from metadata grounds the limit note.
fn render_logs_list_human(result: &LogPage) {
    for entry in &result.items {
        println!("{}", render_log_entry_line(entry));
    }
    if result.items.is_empty() {
        println!("(no matching log entries)");
    }
}

/// Unreachable-in-practice tail summary (render_ok intercepts the
/// streaming output; kept for match totality and future reuse).
fn render_tail_human(result: &TailResult) {
    println!("({} entries streamed)", result.streamed);
}

/// `ign logs download` human line.
fn render_download_human(result: &DownloadResult) {
    println!("wrote {} ({} bytes)", result.file, result.bytes);
}

/// `ign logs loggers` human rows: `name  level  context`.
fn render_loggers_human(result: &ListEnvelope<ignition_core::client::logs::LoggerInfo>) {
    for logger in &result.items {
        let level = logger.level.as_deref().unwrap_or("-");
        let context = if logger.context.is_null() {
            "-".to_string()
        } else {
            serde_json::to_string(&logger.context).unwrap_or_default()
        };
        println!("{}  {}  {}", logger.name, level, context);
    }
    if result.items.is_empty() {
        println!("(no matching loggers)");
    }
}

/// `ign logs loggers set` human line.
fn render_set_level_human(result: &SetLevelResult) {
    println!("set logger {} to {}", result.logger, result.level);
}

/// `ign logs loggers reset` human line.
fn render_reset_human(result: &ResetResult) {
    if result.reset {
        println!("reset all logger levels to defaults");
    }
}

/// `ign restart` human line (no --wait): the research-mandated
/// advisory — the gateway answers the POST immediately, then goes down
/// for ~1 min; steer toward `--wait`.
fn render_restart_human(_result: &ignition_core::actions::restart::RestartResult) {
    println!("restarting; gateway READY in ~1 min; consider `ign restart --wait`");
}

/// `ign restart --wait` human line: progress-free success (the wait
/// printed nothing while polling — StatusPing is polled, not
/// streamed).
fn render_restart_wait_human(result: &ignition_core::actions::restart::RestartWaitResult) {
    println!("gateway {} after {}s", result.state, result.elapsed_secs);
}

/// `ign wait <target>` human line: final state + elapsed.
fn render_wait_human(result: &ignition_core::actions::restart::WaitResult) {
    println!(
        "{} {} after {}s",
        result.target, result.state, result.elapsed_secs
    );
}

/// `ign doctor` human table: one row per check
/// (`name  STATUS  detail`, hint line under failures), then the count
/// summary. The doctor exits 0 whenever the table prints — failing
/// checks are the product (README-documented).
fn render_doctor_human(result: &ignition_core::actions::doctor::DoctorResult) {
    use ignition_core::actions::doctor::CheckStatus;
    let mut ok = 0;
    let mut warn = 0;
    let mut fail = 0;
    let mut skip = 0;
    for check in &result.checks {
        match check.status {
            CheckStatus::Ok => ok += 1,
            CheckStatus::Warn => warn += 1,
            CheckStatus::Fail => fail += 1,
            CheckStatus::Skip => skip += 1,
        }
        let status = match check.status {
            CheckStatus::Ok => "OK",
            CheckStatus::Warn => "WARN",
            CheckStatus::Fail => "FAIL",
            CheckStatus::Skip => "SKIP",
        };
        println!("{:<12}  {:<4}  {}", check.name, status, check.detail);
        if check.status == CheckStatus::Fail
            && let Some(hint) = &check.hint
        {
            println!("  hint: {hint}");
        }
    }
    let mut summary = format!(
        "{} checks: {ok} ok, {warn} warn, {fail} fail",
        result.checks.len()
    );
    if skip > 0 {
        summary.push_str(&format!(", {skip} skip"));
    }
    println!("{summary}");
}

/// `ign project list` human rows: `name  title  enabled  parent
/// inheritable` (absent title/parent/inheritable render as `-`) — the
/// webpage's project list as terminal rows.
fn render_projects_list_human(result: &ProjectsResult) {
    for project in &result.projects {
        let title = project.title.as_deref().unwrap_or("-");
        let parent = project.parent.as_deref().unwrap_or("-");
        let inheritable = match project.inheritable {
            Some(value) => value.to_string(),
            None => "-".to_string(),
        };
        println!(
            "{}  {}  {}  {}  {}",
            project.name, title, project.enabled, parent, inheritable
        );
    }
    if result.projects.is_empty() {
        println!("(no projects)");
    }
}

/// `ign project new` human line: the confirmation + the parent when
/// one was set.
fn render_project_new_human(result: &ignition_core::actions::projects::ProjectSummary) {
    match &result.parent {
        Some(parent) => println!("created {} (parent {})", result.name, parent),
        None => println!("created {}", result.name),
    }
}

/// `ign project copy` human line.
fn render_project_copy_human(result: &ProjectCopyResult) {
    println!("copied {} \u{2192} {}", result.from, result.project.name);
}

/// `ign project rename` human line.
fn render_project_rename_human(result: &ProjectRenameResult) {
    println!(
        "renamed {} \u{2192} {}",
        result.previous_name, result.project.name
    );
}

/// `ign project set` human line: which fields changed on which
/// project (the fields-touched list rides the result display-only).
fn render_project_set_human(result: &ProjectSetResult) {
    println!(
        "set {} on {}",
        result.fields.join(", "),
        result.project.name
    );
}

/// `ign project delete` human line.
fn render_project_delete_human(result: &ProjectDeleteResult) {
    println!("deleted {}", result.deleted);
}

/// `ign project export` human lines: the artifact line + the scope
/// summary — what the ZIP does and does not contain (roadmap
/// criterion 4, in prose form for humans; agents read the arrays).
fn render_project_export_human(result: &ExportResult) {
    println!(
        "exported {} \u{2192} {} ({} bytes)",
        result.project, result.file, result.bytes
    );
    println!(
        "scope: includes {} \u{b7} excludes {}",
        result.scope.includes.join("/"),
        result.scope.excludes.join("/")
    );
}

/// `ign project import` human line: the byte count + the policy that
/// ran (scope rides the JSON data; humans saw it at export time).
fn render_project_import_human(result: &ImportResult) {
    println!(
        "imported {} ({} bytes, policy {})",
        result.name, result.bytes, result.collision_policy
    );
}

/// `ign resource list` human rows: one resource path per line — the
/// surgical loop's inventory (pathless entries render as `-`).
fn render_resources_list_human(result: &ResourcesResult) {
    for entry in &result.resources {
        println!("{}", entry.path.as_deref().unwrap_or("-"));
    }
    if result.resources.is_empty() {
        println!("(no resources)");
    }
}

/// `ign resource get` human output: pretty JSON when the sniff said
/// json, the raw text otherwise — ready to redirect into a file and
/// put back (the surgical edit loop).
fn render_resource_get_human(result: &ResourceGetResult) {
    if result.content_kind == "json" {
        println!(
            "{}",
            serde_json::to_string_pretty(&result.content).unwrap_or_default()
        );
    } else {
        println!("{}", result.content.as_str().unwrap_or_default());
    }
}

/// `ign resource put` human line: the path + the sniffed kind that
/// rode the wire.
fn render_resource_put_human(result: &ResourcePutResult) {
    println!("put {} ({})", result.path, result.content_kind);
}

/// `ign resource delete` human line.
fn render_resource_delete_human(result: &ResourceDeleteResult) {
    println!("deleted {}", result.deleted);
}

/// `ign rig up` human line: the state-forward confirmation (RUNNING,
/// uncommissioned-with-wizard, or compose-wait-satisfied when no
/// gateway port was derivable); warnings follow as their own lines
/// (data-level, never stderr).
fn render_rig_up_human(result: &RigUpResult) {
    match result.state.as_str() {
        "uncommissioned" => {
            let url = result.gateway_url.as_deref().unwrap_or("<gateway>");
            println!("rig {} up — uncommissioned (open {url}/welcome)", result.rig);
        }
        _ => match result.gateway_url.as_deref() {
            Some(url) => println!("rig {} up — gateway RUNNING ({url})", result.rig),
            None => println!("rig {} up — compose --wait satisfied", result.rig),
        },
    }
    for warning in &result.warnings {
        println!("warning: {warning}");
    }
}

/// `ign rig down` human line.
fn render_rig_down_human(result: &RigDownResult) {
    println!("rig {} down", result.rig);
}

/// `ign rig reset` human lines: what the teardown removed (the volume
/// preview, reported as it acted), then the state-forward confirmation
/// — the fresh volume usually boots into the wizard (warnings carry
/// the URL).
fn render_rig_reset_human(result: &RigResetResult) {
    if result.removed_volumes.is_empty() {
        println!("rig {} reset — no named volumes found to remove", result.rig);
    } else {
        println!("rig {} reset — removed volumes:", result.rig);
        for volume in &result.removed_volumes {
            println!("  - {volume}");
        }
    }
    match result.state.as_str() {
        "uncommissioned" => println!("gateway up — uncommissioned (fresh volume)"),
        _ => println!("gateway RUNNING"),
    }
    for warning in &result.warnings {
        println!("warning: {warning}");
    }
}

/// `ign rig logs` human tail — unreachable (the lines already streamed
/// raw during execution; kept for match totality, the render_tail
/// precedent).
fn render_rig_logs_human(result: &RigLogsResult) {
    println!("({} lines streamed)", result.streamed);
}

/// `ign rig status` human table: identity header, one row per service
/// (`service  state  health  published->target/proto`), volumes, and
/// the ports occupancy line — a down rig prints its emptiness as data
/// (exit 0).
fn render_rig_status_human(result: &RigStatusResult) {
    println!("rig {} ({})", result.rig, result.compose_file);
    for service in &result.services {
        let health = service.health.as_deref().unwrap_or("-");
        let ports = if service.publishers.is_empty() {
            "-".to_string()
        } else {
            service
                .publishers
                .iter()
                .map(|publisher| {
                    let protocol = publisher.protocol.as_deref().unwrap_or("tcp");
                    match (publisher.published_port, publisher.target_port) {
                        (Some(published), Some(target)) => {
                            format!("{published}\u{2192}{target}/{protocol}")
                        }
                        (Some(published), None) => format!("{published}/{protocol}"),
                        _ => "-".to_string(),
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        println!("{:<16} {:<8} {:<9} {}", service.name, service.state, health, ports);
    }
    if result.services.is_empty() {
        println!("(no running services — rig is down)");
    }
    let volumes = if result.volumes.is_empty() {
        "-".to_string()
    } else {
        result.volumes.join(", ")
    };
    println!("volumes: {volumes}");
    println!(
        "ports {}",
        if result.ports_free {
            "free"
        } else {
            "in use"
        }
    );
}

/// Epoch milliseconds → an ISO-8601 UTC string
/// (`2026-08-22T03:07:40.123Z`), zero timezone machinery: the
/// civil-from-days algorithm (Howard Hinnant) over the well-known
/// shift; deterministic across machines.
fn iso_utc(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let millis = ms.rem_euclid(1000);
    let days = secs.div_euclid(86_400);
    let time_of_day = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let second = time_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

/// Days since 1970-01-01 → (year, month, day) — Howard Hinnant's
/// `civil_from_days`, the standard compact conversion.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // year of era
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year [0, 365]
    let mp = (5 * doy + 2) / 153; // month index [0, 11] from March
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // day of month [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // month [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
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

#[cfg(test)]
mod tests {
    use super::{civil_from_days, iso_utc};

    /// Known instants: the epoch, a recent date, a pre-epoch value
    /// (negative ms must still render, via euclidean division), and
    /// leap-day handling.
    #[test]
    fn iso_utc_known_instants() {
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso_utc(1_787_346_747_022), "2026-08-21T21:12:27.022Z");
        assert_eq!(iso_utc(1_709_164_800_000), "2024-02-29T00:00:00.000Z");
        assert_eq!(iso_utc(-1), "1969-12-31T23:59:59.999Z");
        assert_eq!(iso_utc(951_782_400_000), "2000-02-29T00:00:00.000Z");
    }

    /// The epoch and a handful of dates round-trip through the civil
    /// conversion.
    #[test]
    fn civil_from_days_matches_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }
}
