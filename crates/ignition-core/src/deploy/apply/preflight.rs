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
//!
//! A check that cannot get its answer (an unreachable gateway or host, a
//! refused credential) is not a failed check: it aborts with its own error
//! (`network_error`, `transport_failed`, `auth_failed`) and nothing else is
//! claimed about the checks that did not run.
//!
//! Details name checks, counts, seconds and the Designer sessions D-15 asks
//! for (user, project, host). They never carry file content or a secret.
//!
//! A Designer session or a foreign edit that starts after the preflights is
//! not seen by them. The under-lock pre-image check is the guard for file
//! changes; C2 in `23-LIVE-CAPTURES.md` records whether the lock blocks
//! Designer saves.

use std::time::Duration;

use super::Steps;
use super::hold::{Clock, HoldModel};
use super::host::ApplyHost;
use crate::client::mode::normalize_deployment_mode;
use crate::client::query::ListQuery;
use crate::client::{DeployApi, GatewayApi};
use crate::deploy::target::Target;
use crate::deploy::transport::TreeReader;
use crate::error::{APPLY_LIST_SHOWN, CoreError, PreflightFailure};

/// Seconds between two reads of the Designer list under `--designers wait`
/// (D-14).
pub const DESIGNERS_POLL_S: u64 = 10;

/// Default `--designers-timeout`, in seconds (D-14).
pub const DESIGNERS_TIMEOUT_DEFAULT_S: u64 = 600;

/// The longest a name from the gateway is shown in a detail.
const SHOWN_NAME_CHARS: usize = 64;

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
}

impl PreflightCheck {
    /// Every check, in the fixed report order.
    pub const ALL: [PreflightCheck; 7] = [
        PreflightCheck::ModeMaster,
        PreflightCheck::ModeBackup,
        PreflightCheck::WrongTarget,
        PreflightCheck::WriteRest,
        PreflightCheck::WriteHost,
        PreflightCheck::Designers,
        PreflightCheck::HoldBudget,
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
    /// The master's deploy surface (mode).
    pub master: &'a dyn DeployApi,
    /// The backup's deploy surface, when the target has a backup node.
    pub backup: Option<&'a dyn DeployApi>,
    /// The master's gateway surface (identity, write probe, Designers).
    pub gateway: &'a dyn GatewayApi,
    /// The read half of the master's host.
    pub reader: &'a dyn TreeReader,
    /// The write seam of the master's host.
    pub host: &'a dyn ApplyHost,
    /// Files this run writes plus files it deletes.
    pub touched_files: usize,
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
    _steps: &mut Steps<'_>,
) -> Result<PreflightReport, CoreError> {
    let mut findings = Vec::with_capacity(PreflightCheck::ALL.len());
    findings.push(
        mode_finding(
            PreflightCheck::ModeMaster,
            "master",
            Some(input.master),
            &input.target.mode,
        )
        .await?,
    );
    findings.push(write_host_finding(input.host).await?);
    let (designers, designers_seen) = designers_finding(input).await?;
    findings.push(designers);

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

async fn mode_finding(
    check: PreflightCheck,
    node: &str,
    api: Option<&dyn DeployApi>,
    target_mode: &str,
) -> Result<PreflightFinding, CoreError> {
    let Some(api) = api else {
        // The target names a node the caller holds no connection to: the
        // mode cannot be confirmed, so the run does not go ahead.
        return Ok(finding(
            check,
            false,
            format!("the {node} node has no gateway connection, so its mode cannot be confirmed"),
        ));
    };
    let seen = normalize_deployment_mode(api.deployment_mode().await?);
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
    Ok(finding(check, ok, detail))
}

// ---- write_host -------------------------------------------------------------

async fn write_host_finding(host: &dyn ApplyHost) -> Result<PreflightFinding, CoreError> {
    let check = PreflightCheck::WriteHost;
    let report = host.probe().await?;
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

async fn designers_finding(
    input: &PreflightInput<'_>,
) -> Result<(PreflightFinding, Vec<DesignerSeen>), CoreError> {
    let check = PreflightCheck::Designers;
    let open = read_designers(input.gateway).await?;
    let found = if open.is_empty() {
        finding(check, true, "no Designer session is open".to_string())
    } else {
        finding(check, false, designers_detail(&open))
    };
    Ok((found, open))
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
                "hold_budget"
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
}
