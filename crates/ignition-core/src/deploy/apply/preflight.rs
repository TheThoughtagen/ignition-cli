//! The apply preflights (Phase 23, DAPL-04, D-03, D-14, D-15).
//!
//! [`run_preflights`] runs after the run directory exists and before any
//! `.gwbk` download, staging, lock or write. It runs every check, so one
//! refusal lists every problem, and it reports them in one fixed order
//! ([`PreflightCheck::ALL`]) whichever check finished first.
//!
//! | Check | Fails when |
//! |---|---|
//! | `mode_master` | the master's active deployment mode is not the target's `mode` |
//! | `mode_backup` | the same through the backup's own profile; `skipped` when the target has no `nodes.backup` |
//! | `wrong_target` | the REST `gateway-info` name is not the host's `systemName` (the `ignition/system-properties` singleton), or not `nodes.master.gateway_name` when that is set |
//! | `write_rest` | the project-scan write probe answers 403 (a read-only token) |
//! | `write_host` | [`ApplyHost::probe`] says the mode collection or the `ign-deploy` root is not writable |
//! | `designers` | a Designer session is open (see [`DesignersMode`]) |
//! | `hold_budget` | `ceil(2 x (estimated commit + p95 scan) + 1)` for this run's writes and deletes is more than the hold (D-03); never clamped |
//! | `master_active` | the master is not the only node whose `activityLevel` is `Active` (a failover moved activity, split-brain, or neither is active); `skipped` when the target has no `nodes.backup`. Runs for `rollback` too (D-11, K3) |
//! | `backup_read` | one read of the backup's mode collection through the backup's own file transport fails (D-02); `skipped` for a REST-only backup (the file-hash check will not run), without a backup node and in `rollback` |
//! | `secrets` | a secret name the rendered tree references is missing on the master or on the backup (D-14), by exact name (K9); a provider the render declares that does not exist yet is skipped by name (K8) and a provider that cannot list is named unverifiable (K7), neither fails; `skipped` when the tree references no secret and in `rollback` |
//!
//! A check that cannot get its answer (an unreachable gateway or host, a
//! refused credential) is not a failed check: it aborts with its own error
//! (`network_error`, `transport_failed`, `auth_rejected`) and nothing else is
//! claimed about the checks that did not run.
//!
//! Details name checks, counts, seconds and the Designer sessions D-15 asks
//! for (user, project, host). They never carry file content or a secret.
//!
//! A Designer session or a foreign edit that starts after the preflights is
//! not seen by them. The under-lock pre-image check is the guard for file
//! changes; C2 in `23-LIVE-CAPTURES.md` records whether the lock blocks
//! Designer saves.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use super::{ApplyEvent, Steps};
use serde_json::Value;

use super::hold::{Clock, HoldModel, hold_for};
use super::host::ApplyHost;
use crate::actions::deploy::Declared;
use crate::client::mode::normalize_deployment_mode;
use crate::client::query::ListQuery;
use crate::client::redundancy::{ActiveNode, RedundancyStatusWire, active_node};
use crate::client::{DeployApi, GatewayApi};
use crate::deploy::layout::RelPath;
use crate::deploy::pair::{
    BackupNode, SecretCheckMode, check_secret_names, collect_referenced_secrets,
    declared_secret_providers, secrets_defined_by_render,
};
use crate::deploy::target::Target;
use crate::deploy::transport::{Scope, TreeReader};
use crate::error::{APPLY_LIST_SHOWN, CoreError, PreflightFailure};

/// Seconds between two reads of the Designer list under `--designers wait`
/// (D-14).
pub const DESIGNERS_POLL_S: u64 = 10;

/// Default `--designers-timeout`, in seconds (D-14).
pub const DESIGNERS_TIMEOUT_DEFAULT_S: u64 = 600;

/// The longest a name from the gateway is shown in a detail.
const SHOWN_NAME_CHARS: usize = 64;

/// The singleton whose `systemName` names the gateway on its host.
const SYSTEM_PROPERTIES_UNIT: &str = "ignition/system-properties";

/// One preflight check, by the id it is reported under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreflightCheck {
    /// The master runs the target's mode.
    ModeMaster,
    /// The backup runs the target's mode.
    ModeBackup,
    /// The REST identity matches the host's.
    WrongTarget,
    /// The API token may write.
    WriteRest,
    /// The host may be written.
    WriteHost,
    /// No Designer session is open.
    Designers,
    /// The run fits in the hold.
    HoldBudget,
    /// The master is the active node of its pair.
    MasterActive,
    /// The backup's files can be read by the account the target names.
    BackupRead,
    /// Every referenced secret name exists on each node.
    Secrets,
}

impl PreflightCheck {
    /// Every check, in the fixed report order.
    pub const ALL: [PreflightCheck; 10] = [
        PreflightCheck::ModeMaster,
        PreflightCheck::ModeBackup,
        PreflightCheck::WrongTarget,
        PreflightCheck::WriteRest,
        PreflightCheck::WriteHost,
        PreflightCheck::Designers,
        PreflightCheck::HoldBudget,
        PreflightCheck::MasterActive,
        PreflightCheck::BackupRead,
        PreflightCheck::Secrets,
    ];

    /// The id of the check in `deploy_preflight_failed` and `run.json`.
    pub fn as_str(self) -> &'static str {
        match self {
            PreflightCheck::ModeMaster => "mode_master",
            PreflightCheck::ModeBackup => "mode_backup",
            PreflightCheck::WrongTarget => "wrong_target",
            PreflightCheck::WriteRest => "write_rest",
            PreflightCheck::WriteHost => "write_host",
            PreflightCheck::Designers => "designers",
            PreflightCheck::HoldBudget => "hold_budget",
            PreflightCheck::MasterActive => "master_active",
            PreflightCheck::BackupRead => "backup_read",
            PreflightCheck::Secrets => "secrets",
        }
    }
}

/// What one check found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingStatus {
    /// The check ran and passed.
    Pass,
    /// The check ran and failed.
    Fail,
    /// The check does not apply to this target (a missing backup node is
    /// skipped, never passed).
    Skipped,
}

impl FindingStatus {
    /// The word in `run.json` and the events.
    pub fn as_str(self) -> &'static str {
        match self {
            FindingStatus::Pass => "pass",
            FindingStatus::Fail => "fail",
            FindingStatus::Skipped => "skipped",
        }
    }
}

/// One check's verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightFinding {
    /// Which check.
    pub check: PreflightCheck,
    /// What it found.
    pub status: FindingStatus,
    /// Why, without any content or secret.
    pub detail: String,
}

/// What to do about open Designer sessions (D-14, D-15).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesignersMode {
    /// Refuse while any is open (the default).
    Fail,
    /// Poll until none is open or the timeout passes, then refuse.
    Wait,
    /// Go ahead, and record that and the open sessions.
    Proceed,
}

impl DesignersMode {
    /// The `--designers` word.
    pub fn as_str(self) -> &'static str {
        match self {
            DesignersMode::Fail => "fail",
            DesignersMode::Wait => "wait",
            DesignersMode::Proceed => "proceed",
        }
    }
}

/// `--designers`, `--designers-timeout` and the poll interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesignersPolicy {
    /// What to do about open sessions.
    pub mode: DesignersMode,
    /// How long `wait` polls before it gives up.
    pub timeout: Duration,
    /// How long `wait` sleeps between two reads.
    pub interval: Duration,
}

impl Default for DesignersPolicy {
    /// `fail`, with the production timeout and interval.
    fn default() -> Self {
        DesignersPolicy {
            mode: DesignersMode::Fail,
            timeout: Duration::from_secs(DESIGNERS_TIMEOUT_DEFAULT_S),
            interval: Duration::from_secs(DESIGNERS_POLL_S),
        }
    }
}

/// What the preflights read.
pub struct PreflightInput<'a> {
    /// The target of this run.
    pub target: &'a Target,
    /// The declared render of this run: `None` for a rollback, which renders
    /// nothing, so the checks that need the rendered tree are `skipped`.
    pub declared: Option<&'a Declared>,
    /// The master's deploy surface (mode).
    pub master: &'a dyn DeployApi,
    /// The backup node (a read-only handle), when the target has one.
    pub backup: Option<&'a BackupNode<'a>>,
    /// The master's gateway surface (identity, write probe, Designers).
    pub gateway: &'a dyn GatewayApi,
    /// The read half of the master's host.
    pub reader: &'a dyn TreeReader,
    /// The write seam of the master's host.
    pub host: &'a dyn ApplyHost,
    /// Files this run writes plus files it deletes.
    pub touched_files: usize,
    /// The paths this run touches (collection paths, and the ledger key when
    /// the ledger is swapped), for the limits a host puts on a path.
    pub touched_paths: &'a [String],
    /// The hold the lock will be requested with and its estimates.
    pub hold: &'a HoldModel,
    /// What to do about open Designer sessions.
    pub designers: &'a DesignersPolicy,
    /// The monotonic clock the Designer wait is timed by.
    pub clock: &'a dyn Clock,
}

/// A Designer session as D-15 records it: user, project, host.
pub type DesignerSeen = (String, String, String);

/// Every check's verdict, in the fixed order, and the sessions seen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightReport {
    /// One finding per check, in [`PreflightCheck::ALL`] order.
    pub findings: Vec<PreflightFinding>,
    /// The Designer sessions open at the last read, whatever the mode.
    pub designers_seen: Vec<DesignerSeen>,
}

impl PreflightReport {
    /// Whether no check failed.
    pub fn passed(&self) -> bool {
        self.findings
            .iter()
            .all(|f| f.status != FindingStatus::Fail)
    }

    /// The failed checks as the refusal carries them, in check order.
    pub fn failures(&self) -> Vec<PreflightFailure> {
        self.findings
            .iter()
            .filter(|f| f.status == FindingStatus::Fail)
            .map(|f| PreflightFailure {
                check: f.check.as_str().to_string(),
                detail: f.detail.clone(),
            })
            .collect()
    }
}

/// Run every preflight (see the module doc) and report every verdict.
///
/// Reads the gateway, the host and the Designer list; writes nothing but the
/// harmless project-scan probe and the host's own probe file, and emits
/// `designers_poll` and `designers_proceed` events through `steps`. An
/// unreachable gateway or host is that call's own error, not a finding.
pub async fn run_preflights(
    input: &PreflightInput<'_>,
    steps: &mut Steps<'_>,
) -> Result<PreflightReport, CoreError> {
    let mut findings = Vec::with_capacity(PreflightCheck::ALL.len());
    let mode = input.target.mode.as_str();

    let master_seen = normalize_deployment_mode(input.master.deployment_mode().await?);
    findings.push(mode_finding(
        PreflightCheck::ModeMaster,
        "master",
        Some(master_seen),
        mode,
    ));
    findings.push(if input.target.nodes.backup.is_some() {
        let backup_seen = match input.backup {
            Some(backup) => Some(backup.deployment_mode().await?),
            None => None,
        };
        mode_finding(PreflightCheck::ModeBackup, "backup", backup_seen, mode)
    } else {
        PreflightFinding {
            check: PreflightCheck::ModeBackup,
            status: FindingStatus::Skipped,
            detail: "the target has no backup node".to_string(),
        }
    });
    findings.push(wrong_target_finding(input).await?);
    findings.push(write_rest_finding(input.gateway).await?);
    findings.push(write_host_finding(input.host, input.touched_paths).await?);
    let (designers, designers_seen) = designers_finding(input, steps).await?;
    findings.push(designers);
    findings.push(hold_budget_finding(input.hold, input.touched_files));
    findings.push(master_active_finding(input).await?);
    findings.push(backup_read_finding(input).await);
    findings.push(secrets_finding(input).await?);

    Ok(PreflightReport {
        findings,
        designers_seen,
    })
}

fn finding(check: PreflightCheck, ok: bool, detail: String) -> PreflightFinding {
    PreflightFinding {
        check,
        status: if ok {
            FindingStatus::Pass
        } else {
            FindingStatus::Fail
        },
        detail,
    }
}

/// `text` without control characters, at most [`SHOWN_NAME_CHARS`] long.
fn printable(text: &str) -> String {
    let mut out: String = text
        .chars()
        .filter(|c| !c.is_control())
        .take(SHOWN_NAME_CHARS)
        .collect();
    if text.chars().count() > SHOWN_NAME_CHARS {
        out.push_str("...");
    }
    out
}

// ---- mode_master, mode_backup ----------------------------------------------

/// `seen` is the node's normalized mode as already read, or `None` when the
/// caller holds no connection to the node.
fn mode_finding(
    check: PreflightCheck,
    node: &str,
    seen: Option<Option<String>>,
    target_mode: &str,
) -> PreflightFinding {
    let Some(seen) = seen else {
        // The target names a node the caller holds no connection to: the
        // mode cannot be confirmed, so the run does not go ahead.
        return finding(
            check,
            false,
            format!("the {node} node has no gateway connection, so its mode cannot be confirmed"),
        );
    };
    let ok = seen.as_deref() == Some(target_mode);
    let detail = match (&seen, ok) {
        (_, true) => format!("the {node} runs mode `{}`", printable(target_mode)),
        (Some(seen), false) => format!(
            "the {node} runs mode `{}` but the target's mode is `{}`",
            printable(seen),
            printable(target_mode)
        ),
        (None, false) => format!(
            "the {node} runs no deployment mode but the target's mode is `{}`",
            printable(target_mode)
        ),
    };
    finding(check, ok, detail)
}

// ---- wrong_target -----------------------------------------------------------

/// The host's `systemName`, or why it cannot be read.
async fn host_system_name(
    reader: &dyn TreeReader,
    mode: &str,
) -> Result<Result<String, &'static str>, CoreError> {
    let (module, ty) = SYSTEM_PROPERTIES_UNIT
        .split_once('/')
        .unwrap_or((SYSTEM_PROPERTIES_UNIT, ""));
    let observed = reader
        .read(&Scope {
            mode: mode.to_string(),
            types: vec![(module.to_string(), ty.to_string())],
            tag_providers: Vec::new(),
        })
        .await?;
    // The gateway lets `local` override the mode collection and the mode
    // collection override `core`; only the bytes of the last two are read.
    if observed
        .local_units
        .iter()
        .any(|u| u == SYSTEM_PROPERTIES_UNIT)
    {
        return Ok(Err("it is overridden in the local collection"));
    }
    let path = RelPath::new(&format!("{SYSTEM_PROPERTIES_UNIT}/config.json"))
        .map_err(|why| CoreError::Internal(format!("system-properties path: {why}")))?;
    let Some(blob) = observed
        .mode
        .get(&path)
        .or_else(|| observed.core.get(&path))
    else {
        return Ok(Err("the host has no system-properties file"));
    };
    let Ok(doc) = serde_json::from_slice::<Value>(&blob.bytes) else {
        return Ok(Err("the system-properties file is not JSON"));
    };
    match doc.get("systemName").and_then(Value::as_str) {
        Some(name) if !name.trim().is_empty() => Ok(Ok(name.to_string())),
        _ => Ok(Err("the system-properties file has no systemName")),
    }
}

async fn wrong_target_finding(input: &PreflightInput<'_>) -> Result<PreflightFinding, CoreError> {
    let check = PreflightCheck::WrongTarget;
    let rest = input
        .gateway
        .gateway_info()
        .await?
        .name
        .filter(|n| !n.trim().is_empty());
    let host = host_system_name(input.reader, &input.target.mode).await?;
    let declared = input.target.nodes.master.gateway_name.as_deref();

    let mut problems: Vec<String> = Vec::new();
    match (&rest, &host) {
        (None, _) => problems.push("the gateway reports no name over REST".to_string()),
        (Some(_), Err(why)) => problems.push(format!("the host's systemName is unreadable: {why}")),
        (Some(rest), Ok(host)) if rest != host => {
            problems.push("the gateway's REST name is not the host's systemName".to_string())
        }
        (Some(_), Ok(_)) => {}
    }
    if let (Some(rest), Some(declared)) = (&rest, declared)
        && rest != declared
    {
        problems.push("the gateway's REST name is not nodes.master.gateway_name".to_string());
    }
    if problems.is_empty() {
        Ok(finding(
            check,
            true,
            "the REST name matches the host's systemName".to_string(),
        ))
    } else {
        Ok(finding(check, false, problems.join("; ")))
    }
}

// ---- write_rest ---------------------------------------------------------------

async fn write_rest_finding(gateway: &dyn GatewayApi) -> Result<PreflightFinding, CoreError> {
    let check = PreflightCheck::WriteRest;
    match gateway.scan_projects().await {
        Ok(()) => Ok(finding(
            check,
            true,
            "the project scan was accepted".to_string(),
        )),
        Err(CoreError::Auth { status: 403, .. }) => Ok(finding(
            check,
            false,
            "the token is read-only (HTTP 403 on the project scan)".to_string(),
        )),
        Err(other) => Err(other),
    }
}

// ---- write_host -------------------------------------------------------------

async fn write_host_finding(
    host: &dyn ApplyHost,
    touched: &[String],
) -> Result<PreflightFinding, CoreError> {
    let check = PreflightCheck::WriteHost;
    let report = host.probe_paths(touched).await?;
    if report.writable {
        return Ok(finding(
            check,
            true,
            "the mode collection and the ign-deploy root are writable".to_string(),
        ));
    }
    let detail = if report.problems.is_empty() {
        "the host is not writable".to_string()
    } else {
        report.problems.join("; ")
    };
    Ok(finding(check, false, detail))
}

// ---- designers --------------------------------------------------------------

async fn read_designers(gateway: &dyn GatewayApi) -> Result<Vec<DesignerSeen>, CoreError> {
    let page = gateway.designers(&ListQuery::default()).await?;
    Ok(page
        .items
        .into_iter()
        .map(|d| {
            (
                printable(&d.user),
                printable(&d.project),
                printable(&d.address),
            )
        })
        .collect())
}

/// `n Designer session(s) open: user on project P from host H, ...`.
fn designers_detail(open: &[DesignerSeen]) -> String {
    let each: Vec<String> = open
        .iter()
        .map(|(user, project, host)| {
            let project = if project.is_empty() {
                "no project"
            } else {
                project
            };
            format!("{user} on {project} from {host}")
        })
        .collect();
    let take = each.len().min(APPLY_LIST_SHOWN);
    let mut text = format!(
        "{} Designer session(s) open: {}",
        open.len(),
        each[..take].join(", ")
    );
    if each.len() > take {
        text.push_str(&format!(" and {} more", each.len() - take));
    }
    text
}

/// What `wait` does next, given the sessions still open and the time spent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WaitStep {
    /// No session is open.
    Clear,
    /// Still open and the timeout has passed.
    TimedOut,
    /// Still open: sleep this long, then read again.
    Sleep(Duration),
}

/// The decision of the `wait` loop. A timeout equal to the elapsed time has
/// passed, and the last sleep never runs past the timeout.
pub(crate) fn wait_step(open: usize, elapsed: Duration, policy: &DesignersPolicy) -> WaitStep {
    if open == 0 {
        WaitStep::Clear
    } else if elapsed >= policy.timeout {
        WaitStep::TimedOut
    } else {
        WaitStep::Sleep(
            policy
                .interval
                .max(Duration::from_millis(1))
                .min(policy.timeout - elapsed),
        )
    }
}

async fn designers_finding(
    input: &PreflightInput<'_>,
    steps: &mut Steps<'_>,
) -> Result<(PreflightFinding, Vec<DesignerSeen>), CoreError> {
    let check = PreflightCheck::Designers;
    let policy = input.designers;
    let started = input.clock.now();
    let mut open = read_designers(input.gateway).await?;
    if policy.mode == DesignersMode::Wait {
        loop {
            steps.emit(ApplyEvent::DesignersPoll { open: open.len() })?;
            let elapsed = input.clock.now().saturating_duration_since(started);
            match wait_step(open.len(), elapsed, policy) {
                WaitStep::Clear | WaitStep::TimedOut => break,
                WaitStep::Sleep(pause) => {
                    tokio::time::sleep(pause).await;
                    open = read_designers(input.gateway).await?;
                }
            }
        }
    }
    let found = if open.is_empty() {
        finding(check, true, "no Designer session is open".to_string())
    } else if policy.mode == DesignersMode::Proceed {
        // D-15: going ahead is allowed only with the choice and the sessions
        // on record (run.json and the events).
        steps.emit(ApplyEvent::DesignersProceed {
            sessions: open.clone(),
        })?;
        finding(
            check,
            true,
            format!(
                "{}; going ahead because --designers proceed was given",
                designers_detail(&open)
            ),
        )
    } else {
        finding(check, false, designers_detail(&open))
    };
    Ok((found, open))
}

// ---- hold_budget ------------------------------------------------------------

pub(super) fn hold_budget_finding(hold: &HoldModel, files: usize) -> PreflightFinding {
    let estimate = hold.estimate_commit_s(files);
    let needed = hold_for(estimate, hold.p95_scan_s);
    let detail = format!(
        "{files} file(s): estimated commit {estimate:.1} s and scan {:.1} s need a hold of {needed} s; \
         the budget is {} s",
        hold.p95_scan_s, hold.hold_s
    );
    finding(PreflightCheck::HoldBudget, needed <= hold.hold_s, detail)
}

// ---- master_active ------------------------------------------------------------

/// The master is the only active node of its pair (D-11, K3).
///
/// Which node is active is decided by `activityLevel`, never by `role`: after
/// a switch-over the former master keeps `role: Master` while its level drops to
/// `Cold` (C-S3). The target file still names that node as the master, so an
/// apply or a rollback would write to the standby, whose collection the
/// active node's sync then overwrites. Both nodes are read here and the check
/// fails on a moved, doubled or missing activity. Rollback runs the same
/// check. The activity can still move after this read; the lock-time guards
/// and the backup phase are what catch a failover during the run.
async fn master_active_finding(input: &PreflightInput<'_>) -> Result<PreflightFinding, CoreError> {
    let check = PreflightCheck::MasterActive;
    if input.target.nodes.backup.is_none() {
        return Ok(PreflightFinding {
            check,
            status: FindingStatus::Skipped,
            detail: "the target has no backup node".to_string(),
        });
    }
    let Some(backup) = input.backup else {
        return Ok(finding(
            check,
            false,
            "the backup node has no gateway connection, so which node is active cannot be confirmed"
                .to_string(),
        ));
    };
    let master = input.gateway.redundancy_status().await?;
    let standby = backup.status().await?;
    let level = |wire: &RedundancyStatusWire| {
        wire.activity_level
            .as_deref()
            .map_or_else(|| "none".to_string(), printable)
    };
    let (master_level, backup_level) = (level(&master), level(&standby));
    let both = format!("master {master_level}, backup {backup_level}");
    Ok(match active_node(&master, &standby) {
        ActiveNode::OnlyA => finding(
            check,
            true,
            format!("the master is the active node ({both})"),
        ),
        ActiveNode::OnlyB => finding(
            check,
            false,
            format!(
                "the backup reports activityLevel Active and the master {master_level}: \
                 activity moved to the backup (the pair switched over); apply and rollback write only to the active master ({both})"
            ),
        ),
        ActiveNode::Both => finding(
            check,
            false,
            format!("both nodes report activityLevel Active (split-brain) ({both})"),
        ),
        ActiveNode::Neither => finding(
            check,
            false,
            format!("neither node reports activityLevel Active ({both})"),
        ),
    })
}

// ---- backup_read ----------------------------------------------------------------

/// The backup's mode collection can be read (D-02).
///
/// This is the one check that turns a read error into a finding instead of an
/// abort: the read is the check. The file-hash comparison after the apply
/// reads the same collection through the same transport, and an account that
/// cannot list the data directory (the Linux gap Phase 23 recorded for list
/// rights on `data` and `data/config`) would only be found after the master
/// was written. Reading it here refuses before the lock. The scope is the
/// manifest's, the same one the comparison reads. `skipped` when there is
/// nothing to read: no backup node, a REST-only backup (the detail says the
/// file-hash check will not run, D-04), or a rollback, which renders nothing.
async fn backup_read_finding(input: &PreflightInput<'_>) -> PreflightFinding {
    let check = PreflightCheck::BackupRead;
    let skipped = |detail: &str| PreflightFinding {
        check,
        status: FindingStatus::Skipped,
        detail: detail.to_string(),
    };
    if input.target.nodes.backup.is_none() {
        return skipped("the target has no backup node");
    }
    let Some(declared) = input.declared else {
        return skipped("a rollback renders no tree, so the backup's files are not compared");
    };
    let Some(backup) = input.backup else {
        return skipped("the backup node has no gateway connection, so its files are not read");
    };
    if !backup.has_files() {
        return skipped(
            "nodes.backup declares no ssh or local transport, so the file-hash check did not run",
        );
    }
    let scope = Scope::from_manifest(&declared.manifest, &declared.target.mode);
    match backup.read_files(&scope).await {
        Some(Ok(observed)) => finding(
            check,
            true,
            format!(
                "read {} files of the backup's mode collection",
                observed.core.len() + observed.mode.len()
            ),
        ),
        Some(Err(err)) => finding(
            check,
            false,
            format!(
                "the backup's mode collection cannot be read: {}",
                printable(err.code())
            ),
        ),
        None => skipped(
            "nodes.backup declares no ssh or local transport, so the file-hash check did not run",
        ),
    }
}

// ---- secrets ------------------------------------------------------------------------

/// Every secret name the rendered tree references exists on each node (D-14).
///
/// Only the names a provider lists are read (`DeployApi::secret_names`); no
/// value is fetched and none can appear in a detail, which lists `node:
/// provider/name` pairs. A listing that cannot be answered at all (a network or
/// auth error) aborts like the other reads. Names compare exactly (K9). A
/// provider the render itself declares that answers 404 on a node is the first
/// deploy that creates it, so it is skipped by name (K8; `deploy verify`
/// still requires it); a provider that answers 501 or 500 cannot list and is
/// named unverifiable (K7). Neither fails the check. K8 extension (WR-04): a
/// name the render itself adds to a provider it declares is defined by the
/// deploy, so it is not asked of a node that already has the provider; only
/// names the tree references and does not provide are refused.
async fn secrets_finding(input: &PreflightInput<'_>) -> Result<PreflightFinding, CoreError> {
    let check = PreflightCheck::Secrets;
    let skipped = |detail: &str| PreflightFinding {
        check,
        status: FindingStatus::Skipped,
        detail: detail.to_string(),
    };
    let Some(declared) = input.declared else {
        return Ok(skipped(
            "a rollback renders no tree, so no secret reference is checked",
        ));
    };
    let all_referenced = collect_referenced_secrets(&declared.outcome.tree);
    if all_referenced.is_empty() {
        return Ok(skipped("the rendered tree references no secret"));
    }
    // A name this render adds to a provider it declares is defined by the
    // deploy itself (K8 extension, WR-04): a node cannot list it before the
    // apply, so it is not asked for. Only names the tree references and does
    // not provide are checked.
    let defined = secrets_defined_by_render(&declared.outcome.tree);
    let referenced: BTreeSet<(String, String)> =
        all_referenced.difference(&defined).cloned().collect();
    if referenced.is_empty() {
        return Ok(skipped(
            "every secret name the rendered tree references is defined by the tree itself",
        ));
    }
    let own_names = all_referenced.len() - referenced.len();
    let declared_providers = declared_secret_providers(&declared.outcome.tree);
    let providers: BTreeSet<&str> = referenced.iter().map(|(p, _)| p.as_str()).collect();

    let mut reports = Vec::with_capacity(2);
    let mut on_master = BTreeMap::new();
    for provider in &providers {
        on_master.insert(
            (*provider).to_string(),
            input.master.secret_names(provider).await?,
        );
    }
    reports.push(check_secret_names(
        "master",
        &referenced,
        &on_master,
        &declared_providers,
        SecretCheckMode::Preflight,
    ));
    if input.target.nodes.backup.is_some() {
        let Some(backup) = input.backup else {
            return Ok(finding(
                check,
                false,
                "the backup node has no gateway connection, so its secret names cannot be checked"
                    .to_string(),
            ));
        };
        let mut on_backup = BTreeMap::new();
        for provider in &providers {
            on_backup.insert(
                (*provider).to_string(),
                backup.secret_names(provider).await?,
            );
        }
        reports.push(check_secret_names(
            "backup",
            &referenced,
            &on_backup,
            &declared_providers,
            SecretCheckMode::Preflight,
        ));
    }

    let missing: Vec<String> = reports
        .iter()
        .flat_map(|r| {
            r.missing.iter().map(|m| {
                format!(
                    "{}: {}/{}",
                    r.node,
                    printable(&m.provider),
                    printable(&m.name)
                )
            })
        })
        .collect();
    let mut notes: Vec<String> = Vec::new();
    for r in &reports {
        if !r.skipped_providers.is_empty() {
            notes.push(format!(
                "{}: skipped {} (declared by this render, not on the node yet)",
                r.node,
                provider_list(&r.skipped_providers)
            ));
        }
        if !r.unverifiable.is_empty() {
            notes.push(format!(
                "{}: unverifiable {} (the provider cannot list its names)",
                r.node,
                provider_list(&r.unverifiable)
            ));
        }
    }
    if missing.is_empty() {
        let mut detail = format!(
            "{} referenced secret name(s) in {} provider(s) checked on {}",
            referenced.len(),
            providers.len(),
            reports
                .iter()
                .map(|r| r.node.as_str())
                .collect::<Vec<_>>()
                .join(" and ")
        );
        if own_names > 0 {
            detail.push_str(&format!(
                "; {own_names} name(s) defined by this render were not required on the node"
            ));
        }
        for note in notes {
            detail.push_str("; ");
            detail.push_str(&note);
        }
        return Ok(finding(check, true, detail));
    }
    let take = missing.len().min(APPLY_LIST_SHOWN);
    let mut detail = format!(
        "{} referenced secret name(s) missing: {}",
        missing.len(),
        missing[..take].join(", ")
    );
    if missing.len() > take {
        detail.push_str(&format!(" and {} more", missing.len() - take));
    }
    for note in notes {
        detail.push_str("; ");
        detail.push_str(&note);
    }
    Ok(finding(check, false, detail))
}

/// Provider names, capped like every other list in a detail.
fn provider_list(providers: &[String]) -> String {
    let take = providers.len().min(APPLY_LIST_SHOWN);
    let mut text = providers[..take]
        .iter()
        .map(|p| printable(p))
        .collect::<Vec<_>>()
        .join(", ");
    if providers.len() > take {
        text.push_str(&format!(" and {} more", providers.len() - take));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_the_contract_words_in_report_order() {
        let ids: Vec<&str> = PreflightCheck::ALL.iter().map(|c| c.as_str()).collect();
        assert_eq!(
            ids,
            [
                "mode_master",
                "mode_backup",
                "wrong_target",
                "write_rest",
                "write_host",
                "designers",
                "hold_budget",
                "master_active",
                "backup_read",
                "secrets"
            ]
        );
    }

    #[test]
    fn production_defaults_are_the_decided_numbers() {
        assert_eq!((DESIGNERS_POLL_S, DESIGNERS_TIMEOUT_DEFAULT_S), (10, 600));
        let d = DesignersPolicy::default();
        assert_eq!(d.mode, DesignersMode::Fail);
        assert_eq!(d.timeout, Duration::from_secs(600));
        assert_eq!(d.interval, Duration::from_secs(10));
    }

    #[test]
    fn the_session_list_is_capped_and_counts_the_rest() {
        let open: Vec<DesignerSeen> = (0..13)
            .map(|i| (format!("u{i}"), "P".to_string(), "h".to_string()))
            .collect();
        let text = designers_detail(&open);
        assert!(text.starts_with("13 Designer session(s) open: u0 on P from h"));
        assert!(text.ends_with("and 3 more"), "{text}");
        assert!(!text.contains("u12"), "{text}");
    }

    #[test]
    fn a_designer_without_a_project_is_named_so() {
        let text = designers_detail(&[("bob".into(), String::new(), "h".into())]);
        assert!(text.contains("bob on no project from h"), "{text}");
    }

    #[test]
    fn printable_drops_controls_and_caps_length() {
        assert_eq!(printable("a\nb\u{1b}c"), "abc");
        let long = "x".repeat(SHOWN_NAME_CHARS + 5);
        assert_eq!(printable(&long).len(), SHOWN_NAME_CHARS + 3);
    }

    #[test]
    fn hold_budget_boundary_is_inclusive() {
        // need(n) = ceil(2 x (1 + 0.5 n + 1) + 1) = 5 + n.
        let hold = |hold_s| HoldModel {
            hold_s,
            commit_fixed_s: 1.0,
            commit_per_file_s: 0.5,
            p95_scan_s: 1.0,
        };
        assert_eq!(
            hold_budget_finding(&hold(14), 9).status,
            FindingStatus::Pass
        );
        let over = hold_budget_finding(&hold(14), 10);
        assert_eq!(over.status, FindingStatus::Fail);
        assert!(over.detail.contains("hold of 15 s"), "{}", over.detail);
        assert!(over.detail.contains("budget is 14 s"), "{}", over.detail);
        assert!(
            over.detail.contains("estimated commit 6.0 s"),
            "{}",
            over.detail
        );
    }

    fn policy(timeout_ms: u64, interval_ms: u64) -> DesignersPolicy {
        DesignersPolicy {
            mode: DesignersMode::Wait,
            timeout: Duration::from_millis(timeout_ms),
            interval: Duration::from_millis(interval_ms),
        }
    }

    #[test]
    fn wait_with_nothing_open_is_clear_at_any_time() {
        let p = policy(1000, 100);
        assert_eq!(wait_step(0, Duration::ZERO, &p), WaitStep::Clear);
        assert_eq!(wait_step(0, Duration::from_secs(99), &p), WaitStep::Clear);
    }

    #[test]
    fn wait_timeout_equal_to_the_elapsed_time_has_passed() {
        // Edge DAPL-04 adjacency.
        let p = policy(1000, 100);
        assert_eq!(
            wait_step(1, Duration::from_millis(999), &p),
            WaitStep::Sleep(Duration::from_millis(1))
        );
        assert_eq!(
            wait_step(1, Duration::from_millis(1000), &p),
            WaitStep::TimedOut
        );
        assert_eq!(
            wait_step(1, Duration::from_millis(1001), &p),
            WaitStep::TimedOut
        );
    }

    #[test]
    fn wait_sleeps_one_interval_unless_the_timeout_is_nearer() {
        let p = policy(1000, 100);
        assert_eq!(
            wait_step(2, Duration::ZERO, &p),
            WaitStep::Sleep(Duration::from_millis(100))
        );
        assert_eq!(
            wait_step(2, Duration::from_millis(950), &p),
            WaitStep::Sleep(Duration::from_millis(50))
        );
    }

    #[test]
    fn a_zero_interval_never_spins() {
        let p = policy(1000, 0);
        assert_eq!(
            wait_step(1, Duration::ZERO, &p),
            WaitStep::Sleep(Duration::from_millis(1))
        );
    }
}
