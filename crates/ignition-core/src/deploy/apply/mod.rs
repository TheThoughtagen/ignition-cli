//! The guarded write half of `deploy` (Phase 23): gates, journal, snapshots,
//! the apply engine and rollback.
//!
//! This module and `deploy/transport/` are the only places that name the
//! write half. Every decision made here is pure or sits behind a seam, so a
//! test can drive it without a gateway. The vocabulary (error slugs and
//! exits, the gate order, the on-box journal, record and staging layouts, the
//! run-directory rule, the outcome, cause and preflight check ids) is the one
//! the user confirmed at the 23-02 checkpoint; see `23-02-SUMMARY.md`
//! `## Checkpoint outcome`.
//!
//! # The engine (23-05): the green path
//!
//! [`run_apply`] takes a plan the gates have passed and drives it, in this
//! order, over the seams in [`ApplyContext`]:
//!
//! 1. the run directory (the git-ignore guard first) and the events log;
//! 2. the [`WriteSet`];
//! 3. the preflights ([`preflight::run_preflights`], 23-07): every failure
//!    in one `deploy_preflight_failed` refusal, nothing staged or locked;
//!    then `run.json` (once, pass or fail) and `plan.json`; then the `.gwbk`
//!    (23-09, D-13), downloaded into the run directory before anything is
//!    staged or locked so that it never spends hold budget: a failed, empty
//!    or truncated one is `deploy_preflight_failed` check `gwbk`, and only
//!    `--skip-gwbk` (recorded in `run.json`) bypasses it;
//! 4. the new ledger, validated with `Ledger::parse`;
//! 5. `ApplyHost::prepare`, outside the lock;
//! 6. [`hold::HeldLock::acquire`];
//! 7. exactly one `ApplyHost::commit`;
//! 8. [`hold::HeldLock::check`], which must say `Held`;
//! 9. `scan_and_wait` on the config scan, once (C11: a tag provider and its
//!    definitions load in one scan);
//! 10. `deploy_verify`, which must be green;
//! 11. the post fingerprint, then `ApplyHost::finalize`;
//! 12. the snapshot download into the run directory (a failure is a warning;
//!     each copied file's SHA-256 is checked against the record's pre-image,
//!     and a difference is a warning naming the path).
//!
//! The pending-restart list is read before staging and again after the
//! run (23-09, DAPL-10): [`ApplyOutcome::restart_pending`] is the list
//! after, [`ApplyOutcome::restart_pending_new`] what was not there before.
//! Reading is all it does: no restart or failover route is called under any
//! outcome (SF-3), and an unreadable list is the warning
//! `restart_pending_unavailable`, never a failure.
//!
//! Only a green verify makes the outcome `applied`. Every other way the
//! under-lock window can end (23-17):
//!
//! | Branch | Ends as |
//! |---|---|
//! | the under-lock pre-image check finds a changed file (nothing moved) | the run's staging is dropped, one no-op scan releases the lock (D12), `plan_stale` |
//! | the lock is refused (for example `scan_lock_held`) | the run's own staging is dropped, the refusal is returned, nothing was written |
//! | the commit fails after it moved files | [`restore_snapshot`], outcome `rolled_back`, cause `write_failed` |
//! | the commit runs past its time budget (the hold less [`hold::LOCK_MARGIN_S`]; the host stops before the next move, delete or retry and reports `deadline`) | [`restore_snapshot`], cause `lock_expired`, no scan before it (review WR-04) |
//! | the lock is not ours or has no margin left after the commit (either clock) | [`restore_snapshot`], cause `lock_expired`, no scan before it |
//! | the scan does not complete | [`restore_snapshot`], cause `scan_incomplete` |
//! | the post-apply verify is red | [`restore_snapshot`], cause `verify_failed` |
//! | a step that cannot be answered before the verdict (the lock read, the scan or the verify fails with an `Err`) | the same restore as a red verify, with the closest cause (`lock_expired`, `scan_incomplete`, `verify_failed`); the step and the error's code are the warning `post_commit_error (<step>: <code>)` (review CR-01) |
//! | a host call fails after a green verify (the fingerprint, the finalize) | the interrupted run: `deploy_journal_present` with `ign deploy rollback --run <id> --env <env>`, the tree left as written; a finalize whose answer was lost but whose journal is gone is `applied` with the warning `finalize_reported_error (<code>)` (review CR-01) |
//! | a restore that cannot finish | `rollback_failed`, the journal stays on the box (D-12) |
//!
//! A host that refuses a commit before it wrote anything (a journal of
//! another run, an invalid spec) is returned as its own typed error and the
//! lock is left to lapse: a scan there could load another run's partial
//! state. Added by later plans: the journal refusal (23-12).
//!
//! Once the run directory exists, `run_apply` leaves through one exit point
//! that writes `result.json` once.
//!
//! # Manual rollback (23-11)
//!
//! [`run_rollback`] undoes a past run from its on-box record alone, through
//! the same [`restore_snapshot`]; see the [`rollback`] module doc for the
//! checks and the run it makes. It needs no declared repo, which is why
//! [`restore_snapshot`] takes a [`RestoreContext`] and not an
//! [`ApplyContext`].

pub mod fingerprint;
pub mod gates;
pub mod hold;
pub mod host;
pub mod journal;
pub mod preflight;
pub mod record;
pub mod rollback;
pub mod run_store;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Value, json};

use crate::actions::backup::backup_download;
use crate::actions::deploy::{
    Declared, DeployPlanResult, PlanOptions, VerifyOptions, deploy_plan, deploy_verify,
};
use crate::actions::workspace::rfc3339_now_utc;
use crate::client::backup::BackupType;
use crate::client::deploy_api::scan_and_wait;
use crate::client::scan::{LockVerdict, ScanKind, ScanOutcome};
use crate::client::{DeployApi, GatewayApi};
use crate::deploy::layout::RelPath;
use crate::deploy::ledger::{Ledger, LedgerRoot, journal_location, ledger_location};
use crate::deploy::pair::BackupNode;
use crate::deploy::plan::PlanStatus;
use crate::deploy::target::NodeTarget;
use crate::deploy::transport::TreeReader;
use crate::deploy::transport::batch::BUDGET_MAX_S;
use crate::deploy::tree::sha256_hex;
use crate::error::{CoreError, PreflightFailure, RollbackFailure};
use crate::poll::PollConfig;

use self::fingerprint::{LEDGER_KEY, fingerprint};
use self::gates::GateFlags;
use self::hold::{Clock, HeldLock, HoldModel, LOCK_MARGIN_S};
use self::host::{ApplyHost, BatchSpec, DeleteOp, WriteOp};
use self::journal::{Journal, JournalPhase, journal_on_box};
use self::preflight::{DesignersPolicy, PreflightInput, run_preflights};
use self::record::{RecordState, RunRecord, TouchedEntry};
use self::run_store::{EventLog, RunDir, RunId};

pub use self::rollback::{
    InterruptedRun, RollbackChecked, RollbackContext, RollbackKind, RollbackOptions,
    RollbackOutcome, check_rollback, restore_set, run_rollback,
};

/// The slug of the rolled-back apply verdict.
///
/// Decision A: `deploy_rolled_back` rides the verdict payload (`data.slug`,
/// exit 6) and is never a [`crate::error::CoreError`] variant, never in
/// [`crate::error::CoreError::DEPLOY_SLUGS`] and never a row of the README
/// exit table. It follows the `deploy_verify_failed` precedent
/// ([`crate::deploy::verify::VERIFY_FAILED_SLUG`]).
pub const ROLLED_BACK_SLUG: &str = "deploy_rolled_back";

/// The preflight check id of the `.gwbk` step (23-02 item 8).
const GWBK_CHECK: &str = "gwbk";

/// The warning for a pending-restart list that could not be read (23-02
/// item 9).
const RESTART_UNAVAILABLE: &str = "restart_pending_unavailable";

/// The warning for an event log that could not be appended to after the run
/// was recorded (the deploy is live and verified; the log is the casualty).
const EVENTS_UNWRITABLE: &str = "events_log_unwritable";

/// How a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeKind {
    /// The change is live and verified.
    Applied,
    /// The plan was empty; nothing was touched.
    Noop,
    /// The change was undone (23-17).
    RolledBack,
}

impl OutcomeKind {
    /// The `data.outcome` word.
    pub fn as_str(self) -> &'static str {
        match self {
            OutcomeKind::Applied => "applied",
            OutcomeKind::Noop => "noop",
            OutcomeKind::RolledBack => "rolled_back",
        }
    }
}

/// Why a run rolled back (23-17).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RollbackCause {
    /// The scan-lock lapsed before the scan.
    LockExpired,
    /// The commit failed after it moved files.
    WriteFailed,
    /// The scan did not complete.
    ScanIncomplete,
    /// The post-apply verify was red.
    VerifyFailed,
}

impl RollbackCause {
    /// The `data.cause` word.
    pub fn as_str(self) -> &'static str {
        match self {
            RollbackCause::LockExpired => "lock_expired",
            RollbackCause::WriteFailed => "write_failed",
            RollbackCause::ScanIncomplete => "scan_incomplete",
            RollbackCause::VerifyFailed => "verify_failed",
        }
    }
}

/// The files one run writes and deletes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteSet {
    /// Byte-sorted, each with its expected pre-image.
    pub writes: Vec<WriteOp>,
    /// Byte-sorted, each with its expected pre-image.
    pub deletes: Vec<DeleteOp>,
}

/// The write set of `plan` (research Pattern 3).
///
/// Every rendered file of every `added` or `changed` unit is written, whole
/// units and never a partial one in the mode collection; `same` and
/// `unmanaged` units are never written. The deletes are the plan's delete
/// set. Each touched path carries the hash the plan saw in the MODE
/// collection: `live_sha256` where the entry is live in `mode`, else none
/// (a copy that only `core` holds is not a file of the mode collection). A
/// delete without such a hash, or a rendered file whose bytes are not the
/// ones the plan hashed, is a defect, reported as
/// [`CoreError::Internal`].
pub fn write_set(plan: &DeployPlanResult, declared: &Declared) -> Result<WriteSet, CoreError> {
    let defect = |why: String| CoreError::Internal(format!("write set: {why}"));
    let mut writes = Vec::new();
    // Path to (live in the mode collection, effective live hash).
    let mut live: BTreeMap<&str, (bool, Option<&str>)> = BTreeMap::new();
    for entry in &plan.entries {
        let in_mode = entry.live_in.iter().any(|c| c == "mode");
        for file in &entry.files {
            live.insert(file.path.as_str(), (in_mode, file.live_sha256.as_deref()));
        }
        if !matches!(entry.status, PlanStatus::Added | PlanStatus::Changed) {
            continue;
        }
        for file in &entry.files {
            let Some(desired) = &file.desired_sha256 else {
                continue;
            };
            let path = RelPath::new(&file.path)
                .map_err(|why| defect(format!("the plan names an invalid path ({why})")))?;
            let bytes =
                declared.outcome.tree.files.get(&path).ok_or_else(|| {
                    defect(format!("{path} is in the plan but not in the render"))
                })?;
            if sha256_hex(bytes) != *desired {
                return Err(defect(format!(
                    "{path} was rendered differently from the plan"
                )));
            }
            writes.push(WriteOp {
                path,
                sha256: desired.clone(),
                pre: if in_mode {
                    file.live_sha256.clone()
                } else {
                    None
                },
            });
        }
    }
    let mut deletes = Vec::new();
    for path in &plan.delete_set.files {
        let Some((true, Some(pre))) = live.get(path.as_str()) else {
            return Err(defect(format!(
                "{path} is in the delete set without a pre-image in the plan"
            )));
        };
        deletes.push(DeleteOp {
            path: path.clone(),
            pre: (*pre).to_string(),
        });
    }
    writes.sort_by(|a, b| a.path.as_str().as_bytes().cmp(b.path.as_str().as_bytes()));
    deletes.sort_by(|a, b| a.path.as_str().as_bytes().cmp(b.path.as_str().as_bytes()));
    Ok(WriteSet { writes, deletes })
}

/// The ledger after this run: the old files minus the delete set, plus the
/// written files, byte-sorted (the ledger swap is the last move).
fn new_ledger(
    plan: &DeployPlanResult,
    old: Option<&Ledger>,
    set: &WriteSet,
    run_id: &RunId,
) -> Ledger {
    let mut files: BTreeSet<String> = old
        .map(|ledger| ledger.files.iter().cloned().collect())
        .unwrap_or_default();
    for op in &set.deletes {
        files.remove(op.path.as_str());
    }
    for op in &set.writes {
        files.insert(op.path.as_str().to_string());
    }
    Ledger {
        schema_version: 1,
        gateway: plan.gateway.clone(),
        mode: plan.mode.clone(),
        render_sha256: plan.render_sha256.clone(),
        run_id: run_id.as_str().to_string(),
        files: files.into_iter().collect(),
    }
}

/// The seconds a commit may spend moving and deleting: the scan-lock hold less
/// [`LOCK_MARGIN_S`], so that a scan can still be triggered inside the hold
/// when the host stops exactly at the budget. At least one second, at most
/// [`BUDGET_MAX_S`]. The host counts from the start of its commit and the hold
/// from the grant, so the ssh connect and the script start are what the margin
/// and the post-commit lock check absorb (WR-04).
fn commit_budget_s(hold: &HoldModel) -> u32 {
    let seconds = hold.hold_s.saturating_sub(LOCK_MARGIN_S as u64);
    u32::try_from(seconds.clamp(1, u64::from(BUDGET_MAX_S))).unwrap_or(BUDGET_MAX_S)
}

/// The batch the host will commit: the writes, the deletes, the new ledger
/// and the journal and record that describe exactly them.
fn batch_spec(
    plan: &DeployPlanResult,
    run_id: &RunId,
    set: &WriteSet,
    ledger_new: Vec<u8>,
    ledger_pre: Option<String>,
    commit_budget_s: u32,
) -> BatchSpec {
    let mut touched: Vec<TouchedEntry> = set
        .writes
        .iter()
        .map(|w| TouchedEntry {
            path: w.path.as_str().to_string(),
            pre_sha256: w.pre.clone(),
            post_sha256: Some(w.sha256.clone()),
        })
        .chain(set.deletes.iter().map(|d| TouchedEntry {
            path: d.path.as_str().to_string(),
            pre_sha256: Some(d.pre.clone()),
            post_sha256: None,
        }))
        .collect();
    touched.push(TouchedEntry {
        path: LEDGER_KEY.to_string(),
        pre_sha256: ledger_pre.clone(),
        post_sha256: Some(sha256_hex(&ledger_new)),
    });
    touched.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
    let pre_fingerprint = fingerprint(
        &touched
            .iter()
            .map(|e| (e.path.clone(), e.pre_sha256.clone()))
            .collect(),
    );
    let run = run_id.as_str().to_string();
    BatchSpec {
        run_id: run_id.clone(),
        gateway: plan.gateway.clone(),
        mode: plan.mode.clone(),
        writes: set.writes.clone(),
        deletes: set.deletes.clone(),
        ledger_new: Some(ledger_new),
        ledger_pre,
        commit_budget_s: Some(commit_budget_s),
        journal: Journal {
            schema_version: 1,
            run_id: run.clone(),
            gateway: plan.gateway.clone(),
            mode: plan.mode.clone(),
            phase: JournalPhase::Snapshotting,
            pre_fingerprint: pre_fingerprint.clone(),
            touched: touched.iter().map(|e| e.path.clone()).collect(),
        },
        record: RunRecord {
            schema_version: 1,
            run_id: run,
            gateway: plan.gateway.clone(),
            mode: plan.mode.clone(),
            render_sha256: plan.render_sha256.clone(),
            plan_sha256: plan.plan_sha256.clone(),
            state: RecordState::Applying,
            rolled_back_by: None,
            touched,
            pre_fingerprint,
            post_fingerprint: None,
        },
    }
}

/// One step of a run, value-free: ids, counts and flags only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyEvent {
    /// The run directory exists.
    RunStarted {
        /// The run.
        run_id: String,
        /// Files to write.
        writes: usize,
        /// Files to delete.
        deletes: usize,
    },
    /// `prepare` staged the batch.
    Staged {
        /// Files staged.
        files: usize,
    },
    /// The scan-lock was taken.
    LockAcquired {
        /// Seconds requested.
        hold_s: u64,
    },
    /// `commit` returned.
    Committed {
        /// Writes moved.
        moved: usize,
        /// Deletes done.
        deleted: usize,
    },
    /// The lock was judged after the commit.
    LockVerdict {
        /// Whether it was held with the margin left.
        held: bool,
    },
    /// The config scan completed.
    ScanDone,
    /// `deploy_verify` answered.
    VerifyDone {
        /// Whether it was green.
        ok: bool,
    },
    /// The run was finalized.
    Finalized,
    /// `commit` returned an error and this run's journal is still on the box:
    /// what is on the host is unknown and the run is recovered with
    /// `rollback --run` (23-12). Carries the code of the original failure.
    CommitInterrupted {
        /// The error code `commit` failed with.
        code: String,
    },
    /// The checks of a red verdict, by id and status.
    VerifyChecks {
        /// `(check id, status)` in report order.
        checks: Vec<(String, String)>,
    },
    /// A restore began (23-17). `None` is a manual rollback (23-11): the
    /// closed `data.cause` vocabulary of an apply is unchanged.
    RestoreStarted {
        /// Why an apply is being undone; `None` for a manual rollback.
        cause: Option<RollbackCause>,
    },
    /// A rollback went ahead over live changes made after the run
    /// (`--discard-live-changes`, 23-11): the first paths and the count.
    LiveChangesDiscarded {
        /// How many touched paths had changed.
        count: usize,
        /// The first [`crate::error::APPLY_LIST_SHOWN`], byte-sorted.
        paths: Vec<String>,
    },
    /// The restore finished (23-17).
    Restored,
    /// A recovery removed an interrupted run that had changed nothing
    /// (23-12, D-10).
    Cleared,
    /// A rollback found that the run it recovers was interrupted (23-12,
    /// D-09, D-11): the journal's phase, what recovery will do and the live
    /// hash of each path of the journal's touched list (`None` when absent).
    InterruptedRun {
        /// The journal's phase.
        phase: &'static str,
        /// `restore` or `clear`.
        action: &'static str,
        /// Path to live hash.
        hashes: BTreeMap<String, Option<String>>,
    },
    /// The preflights found a failed check; every verdict, by id and status.
    PreflightChecks {
        /// `(check id, status)` in report order.
        checks: Vec<(String, String)>,
    },
    /// The Designer list was read under `--designers wait`.
    DesignersPoll {
        /// Sessions open at this read.
        open: usize,
    },
    /// `--designers proceed` went ahead over these open sessions (D-15).
    DesignersProceed {
        /// `(user, project, host)` of each open session.
        sessions: Vec<preflight::DesignerSeen>,
    },
    /// The `.gwbk` was downloaded and read as a ZIP (23-09).
    GwbkDownloaded {
        /// Its size in bytes.
        bytes: u64,
    },
    /// `--skip-gwbk`: no `.gwbk` was taken (23-09).
    GwbkSkipped,
    /// The run failed.
    Failed {
        /// The error code slug.
        code: String,
    },
}

impl ApplyEvent {
    /// The event's name in `events.ndjson`.
    pub fn name(&self) -> &'static str {
        match self {
            ApplyEvent::RunStarted { .. } => "run_started",
            ApplyEvent::Staged { .. } => "staged",
            ApplyEvent::LockAcquired { .. } => "lock_acquired",
            ApplyEvent::Committed { .. } => "committed",
            ApplyEvent::LockVerdict { .. } => "lock_verdict",
            ApplyEvent::ScanDone => "scan_done",
            ApplyEvent::VerifyDone { .. } => "verify_done",
            ApplyEvent::VerifyChecks { .. } => "verify_checks",
            ApplyEvent::Finalized => "finalized",
            ApplyEvent::CommitInterrupted { .. } => "commit_interrupted",
            ApplyEvent::RestoreStarted { .. } => "restore_started",
            ApplyEvent::Restored => "restored",
            ApplyEvent::Cleared => "cleared",
            ApplyEvent::InterruptedRun { .. } => "interrupted_run",
            ApplyEvent::LiveChangesDiscarded { .. } => "live_changes_discarded",
            ApplyEvent::PreflightChecks { .. } => "preflight_checks",
            ApplyEvent::DesignersPoll { .. } => "designers_poll",
            ApplyEvent::DesignersProceed { .. } => "designers_proceed",
            ApplyEvent::GwbkDownloaded { .. } => "gwbk_downloaded",
            ApplyEvent::GwbkSkipped => "gwbk_skipped",
            ApplyEvent::Failed { .. } => "failed",
        }
    }

    /// The event's fields: ids, counts and flags, never a value.
    pub fn fields(&self) -> Value {
        match self {
            ApplyEvent::RunStarted {
                run_id,
                writes,
                deletes,
            } => json!({"run_id": run_id, "writes": writes, "deletes": deletes}),
            ApplyEvent::Staged { files } => json!({"files": files}),
            ApplyEvent::LockAcquired { hold_s } => json!({"hold_s": hold_s}),
            ApplyEvent::Committed { moved, deleted } => {
                json!({"moved": moved, "deleted": deleted})
            }
            ApplyEvent::LockVerdict { held } => json!({"held": held}),
            ApplyEvent::VerifyDone { ok } => json!({"ok": ok}),
            ApplyEvent::VerifyChecks { checks } => json!({
                "checks": checks
                    .iter()
                    .map(|(id, status)| json!({"id": id, "status": status}))
                    .collect::<Vec<_>>(),
            }),
            ApplyEvent::PreflightChecks { checks } => json!({
                "checks": checks
                    .iter()
                    .map(|(id, status)| json!({"id": id, "status": status}))
                    .collect::<Vec<_>>(),
            }),
            ApplyEvent::DesignersPoll { open } => json!({"open": open}),
            ApplyEvent::DesignersProceed { sessions } => json!({
                "sessions": sessions
                    .iter()
                    .map(|(user, project, host)| json!({"user": user, "project": project, "host": host}))
                    .collect::<Vec<_>>(),
            }),
            ApplyEvent::RestoreStarted { cause } => {
                json!({"cause": cause.map(RollbackCause::as_str)})
            }
            ApplyEvent::LiveChangesDiscarded { count, paths } => {
                json!({"count": count, "paths": paths})
            }
            ApplyEvent::InterruptedRun {
                phase,
                action,
                hashes,
            } => json!({
                "phase": phase,
                "action": action,
                "touched": hashes.len(),
                "hashes": hashes,
            }),
            ApplyEvent::GwbkDownloaded { bytes } => json!({"bytes": bytes}),
            ApplyEvent::CommitInterrupted { code } | ApplyEvent::Failed { code } => {
                json!({"code": code})
            }
            ApplyEvent::ScanDone
            | ApplyEvent::Finalized
            | ApplyEvent::Restored
            | ApplyEvent::Cleared
            | ApplyEvent::GwbkSkipped => Value::Null,
        }
    }
}

/// Hears each [`ApplyEvent`] as it happens.
pub trait ApplyObserver: Send + Sync {
    /// One step happened.
    fn event(&self, e: &ApplyEvent);
}

/// An observer that ignores everything.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoObserver;

impl ApplyObserver for NoObserver {
    fn event(&self, _e: &ApplyEvent) {}
}

/// What the engine drives.
pub struct ApplyContext<'a> {
    /// The declared repo, rendered.
    pub declared: &'a Declared,
    /// The repo root (the run directory lives below it).
    pub repo: &'a std::path::Path,
    /// The read half of the master's host.
    pub reader: &'a dyn TreeReader,
    /// The write seam of the master's host.
    pub host: &'a dyn ApplyHost,
    /// The gateway REST surface (lock, scan, mode, read-back).
    pub deploy: &'a dyn DeployApi,
    /// The gateway log and WebDev surface (verify).
    pub gateway: &'a dyn GatewayApi,
    /// The backup node (a read-only handle), when the target has a backup
    /// node (the `mode_backup` preflight).
    pub backup: Option<&'a BackupNode<'a>>,
    /// The monotonic clock the lock is judged by.
    pub clock: &'a dyn Clock,
    /// Hears each step.
    pub observer: &'a dyn ApplyObserver,
}

impl<'a> ApplyContext<'a> {
    /// What [`restore_snapshot`] needs from this context.
    fn restore_context(&'a self, opts: &'a ApplyOptions) -> RestoreContext<'a> {
        RestoreContext {
            env: &self.declared.env,
            master: &self.declared.target.nodes.master,
            host: self.host,
            deploy: self.deploy,
            clock: self.clock,
            hold: &opts.hold,
            poll: &opts.poll,
        }
    }
}

/// What a run needs besides its context.
#[derive(Debug, Clone)]
pub struct ApplyOptions {
    /// The `--base` the plan was drawn against.
    pub base: Option<PathBuf>,
    /// The gate flags the plan passed with.
    pub gates: GateFlags,
    /// The WebDev project the verify probes read through.
    pub project: String,
    /// The hold the lock is requested with.
    pub hold: HoldModel,
    /// How the scan wait polls.
    pub poll: PollConfig,
    /// How the post-apply verify waits.
    pub verify: VerifyOptions,
    /// What to do about open Designer sessions (D-14, D-15).
    pub designers: DesignersPolicy,
    /// `--skip-gwbk`: take no `.gwbk` (D-13). Recorded in `run.json`.
    pub skip_gwbk: bool,
}

/// How a run ended and what it did.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ApplyOutcome {
    /// `applied`, `noop` or `rolled_back`.
    #[serde(rename = "outcome")]
    pub kind: OutcomeKind,
    /// The run; absent for a no-op.
    pub run_id: Option<String>,
    /// The run directory; absent for a no-op.
    pub run_dir: Option<String>,
    /// Why the run rolled back.
    pub cause: Option<RollbackCause>,
    /// `render_sha256` of the render.
    pub render_sha256: String,
    /// `plan_sha256` of the plan.
    pub plan_sha256: String,
    /// Collection paths written.
    pub written: Vec<String>,
    /// Collection paths deleted.
    pub deleted: Vec<String>,
    /// The touched-set fingerprint after the run.
    pub post_fingerprint: Option<String>,
    /// Non-failing notes of the engine.
    pub warnings: Vec<String>,
    /// The `.gwbk` file name in the run directory; absent for a no-op, for
    /// `--skip-gwbk` and for a rolled-back run that never took one.
    pub gwbk: Option<String>,
    /// Whether the run was started with `--skip-gwbk`.
    pub skip_gwbk: bool,
    /// The pending restarts after the run (`[]` when none or unreadable).
    pub restart_pending: Vec<String>,
    /// The pending restarts the run added: those not listed before it.
    pub restart_pending_new: Vec<String>,
    /// SHA-256 of each file copied into the run directory's `snapshot/`,
    /// keyed by its path below it (`files/<path>`, `ledger.json`,
    /// `record.json`).
    pub snapshot_sha256: BTreeMap<String, String>,
}

/// Apply `plan` (see the module doc for the order).
///
/// An empty plan returns [`OutcomeKind::Noop`] before anything is touched:
/// no run directory, no host call, no lock, no scan. Otherwise the run
/// directory is created first (the git-ignore guard is part of that), and
/// from then on every way out passes through the one exit point at the end
/// of this function, which writes `result.json` once.
pub async fn run_apply(
    ctx: &ApplyContext<'_>,
    plan: &DeployPlanResult,
    opts: &ApplyOptions,
) -> Result<ApplyOutcome, CoreError> {
    if !plan.summary.has_changes {
        return Ok(ApplyOutcome {
            kind: OutcomeKind::Noop,
            run_id: None,
            run_dir: None,
            cause: None,
            render_sha256: plan.render_sha256.clone(),
            plan_sha256: plan.plan_sha256.clone(),
            written: Vec::new(),
            deleted: Vec::new(),
            post_fingerprint: None,
            warnings: Vec::new(),
            gwbk: None,
            skip_gwbk: false,
            restart_pending: Vec::new(),
            restart_pending_new: Vec::new(),
            snapshot_sha256: BTreeMap::new(),
        });
    }
    let (run_id, run_dir) = RunDir::create_fresh(ctx.repo, &plan.gateway)?;
    let mut steps = Steps::new(None, ctx.observer);
    let result = drive(ctx, plan, opts, &run_id, &run_dir, &mut steps).await;

    // The one exit point.
    if let Err(e) = &result {
        let _ = steps.emit(ApplyEvent::Failed {
            code: e.code().to_string(),
        });
    }
    let doc = match &result {
        Ok(outcome) => {
            let mut doc = serde_json::to_value(outcome).unwrap_or(Value::Null);
            doc["ok"] = Value::Bool(outcome.kind != OutcomeKind::RolledBack);
            if outcome.kind == OutcomeKind::RolledBack {
                doc["slug"] = Value::String(ROLLED_BACK_SLUG.to_string());
            }
            doc
        }
        Err(e) => json!({
            "ok": false,
            "run_id": run_id.as_str(),
            "error": {"code": e.code(), "message": e.to_string()},
        }),
    };
    let written = run_dir.write_json("result.json", &doc);
    match result {
        Ok(mut outcome) => {
            if written.is_err() {
                outcome.warnings.push("result_json_unwritable".to_string());
            }
            Ok(outcome)
        }
        Err(e) => Err(e),
    }
}

/// The run's events: appended to `events.ndjson` once it is open, and told
/// to the observer.
pub struct Steps<'a> {
    log: Option<EventLog>,
    observer: &'a dyn ApplyObserver,
}

impl<'a> Steps<'a> {
    /// Events go to `log` (when open) and to `observer`.
    pub fn new(log: Option<EventLog>, observer: &'a dyn ApplyObserver) -> Self {
        Steps { log, observer }
    }

    /// Report one step.
    pub fn emit(&mut self, event: ApplyEvent) -> Result<(), CoreError> {
        self.observer.event(&event);
        match &mut self.log {
            Some(log) => log.append(event.name(), event.fields()),
            None => Ok(()),
        }
    }
}

/// A host cannot know the `--env` name; the engine does.
fn with_env(err: CoreError, env: &str) -> CoreError {
    match err {
        CoreError::DeployJournalPresent {
            run_id, location, ..
        } => CoreError::DeployJournalPresent {
            run_id,
            env: env.to_string(),
            location,
        },
        other => other,
    }
}

/// What this run's journal says about the box after a failure whose effect on
/// the box is not known.
enum BoxState {
    /// This run's own journal is there.
    Mine(Journal),
    /// Another run's journal is there.
    Other,
    /// The box was read and holds no journal.
    Absent,
    /// The box (or the journal on it) could not be read.
    Unknown,
}

async fn box_state(ctx: &ApplyContext<'_>, plan: &DeployPlanResult, run_id: &RunId) -> BoxState {
    let master = &ctx.declared.target.nodes.master;
    match journal_on_box(ctx.reader, master, &plan.gateway, &plan.mode).await {
        Ok(Some(journal)) if journal.run_id == run_id.as_str() => BoxState::Mine(journal),
        Ok(Some(_)) => BoxState::Other,
        Ok(None) => BoxState::Absent,
        Err(_) => BoxState::Unknown,
    }
}

/// This run's journal is on the box: the run is interrupted. Reported as
/// `deploy_journal_present` with the recovery command, and the original
/// failure's code in a `commit_interrupted` event.
fn interrupted(
    ctx: &ApplyContext<'_>,
    plan: &DeployPlanResult,
    steps: &mut Steps<'_>,
    journal: &Journal,
    err: &CoreError,
) -> CoreError {
    let _ = steps.emit(ApplyEvent::CommitInterrupted {
        code: err.code().to_string(),
    });
    journal::journal_present(
        journal,
        &ctx.declared.env,
        &ctx.declared.target.nodes.master,
        &plan.gateway,
        &plan.mode,
    )
}

/// `commit` returned an `Err`. Over ssh a dropped connection is one, and what
/// is on the box is unknown: the remote script may have moved files or may
/// still be running (23-17 carry-over, DAPL-09).
///
/// Only the journal tells. When this run's own journal is on the box the run
/// is reported as interrupted: `deploy_journal_present` with the recovery
/// command (`ign deploy rollback --run <id> --env <env>`), and the original
/// failure's code in a `commit_interrupted` event. Nothing is restored here: a
/// restore on a guess could race a script that is still moving files, and the
/// recovery path reads the snapshot before it writes. When no journal of this
/// run is there, nothing is known to have been written and the original
/// error stands. When the box cannot be read at all, nothing is known either
/// way: the error keeps its code and names the recovery command (WR-05). A
/// refusal that is already
/// `deploy_journal_present` (another run's journal) is kept as it is.
async fn commit_failed(
    ctx: &ApplyContext<'_>,
    run_id: &RunId,
    plan: &DeployPlanResult,
    steps: &mut Steps<'_>,
    err: CoreError,
) -> CoreError {
    if matches!(err, CoreError::DeployJournalPresent { .. }) {
        return err;
    }
    match box_state(ctx, plan, run_id).await {
        BoxState::Mine(journal) => interrupted(ctx, plan, steps, &journal, &err),
        BoxState::Unknown => {
            // An unreadable box is not an empty one, and the usual reason it
            // cannot be read is the dropped connection that failed the commit
            // (WR-05): the journal may be there, so the recovery is named.
            let _ = steps.emit(ApplyEvent::CommitInterrupted {
                code: format!("{}; journal state unknown", err.code()),
            });
            recovery_pointer(ctx, plan, run_id, err)
        }
        BoxState::Other | BoxState::Absent => err,
    }
}

/// A host call failed after a green verify: the deploy is live and verified
/// but not recorded (CR-01). It is never a plain error. This run's own
/// journal on the box makes it the interrupted run (`deploy_journal_present`
/// with the recovery command: `rollback --run` restores the pre-apply files,
/// as for a kill past a green verify, A9). When the journal cannot be told
/// from the error alone the recovery command is named all the same.
async fn unfinished_after_green(
    ctx: &ApplyContext<'_>,
    run_id: &RunId,
    plan: &DeployPlanResult,
    steps: &mut Steps<'_>,
    err: CoreError,
) -> CoreError {
    match box_state(ctx, plan, run_id).await {
        BoxState::Mine(journal) => interrupted(ctx, plan, steps, &journal, &err),
        _ => recovery_pointer(ctx, plan, run_id, err),
    }
}

/// `err`, saying that this run may have left a journal on the host and how to
/// recover it. A transport failure keeps its slug and exit code (4) and gains
/// the sentence; anything else becomes `deploy_journal_present` for this run,
/// whose message carries the same command.
fn recovery_pointer(
    ctx: &ApplyContext<'_>,
    plan: &DeployPlanResult,
    run_id: &RunId,
    err: CoreError,
) -> CoreError {
    let env = &ctx.declared.env;
    match err {
        CoreError::TransportFailed {
            operation,
            node,
            detail,
        } => CoreError::TransportFailed {
            operation,
            node,
            detail: format!(
                "{detail}; this run's journal may be on the host: recover it with: \
                 ign deploy rollback --run {run} --env {env}",
                run = run_id.as_str()
            ),
        },
        _ => CoreError::DeployJournalPresent {
            run_id: run_id.as_str().to_string(),
            env: env.clone(),
            location: journal_location(
                LedgerRoot::for_node(&ctx.declared.target.nodes.master),
                &plan.gateway,
                &plan.mode,
            ),
        },
    }
}

/// The steps of the module doc, in order. Returns `Ok` only for a verified,
/// finalized deploy.
async fn drive(
    ctx: &ApplyContext<'_>,
    plan: &DeployPlanResult,
    opts: &ApplyOptions,
    run_id: &RunId,
    run_dir: &RunDir,
    steps: &mut Steps<'_>,
) -> Result<ApplyOutcome, CoreError> {
    let declared = ctx.declared;
    let env = declared.env.as_str();
    let (gateway, mode) = (plan.gateway.as_str(), plan.mode.as_str());

    let started_at = rfc3339_now_utc();
    steps.log = Some(EventLog::open(run_dir)?);

    let set = write_set(plan, declared)?;
    steps.emit(ApplyEvent::RunStarted {
        run_id: run_id.as_str().to_string(),
        writes: set.writes.len(),
        deletes: set.deletes.len(),
    })?;

    // Every preflight, before anything is staged, locked or written. A check
    // that could not be answered at all (an unreachable gateway or host) is
    // its own error, after the run record has been written.
    let touched_paths: Vec<String> = set
        .writes
        .iter()
        .map(|w| w.path.as_str().to_string())
        .chain(set.deletes.iter().map(|d| d.path.as_str().to_string()))
        .collect();
    let preflights = run_preflights(
        &PreflightInput {
            target: &declared.target,
            master: ctx.deploy,
            backup: ctx.backup,
            gateway: ctx.gateway,
            reader: ctx.reader,
            host: ctx.host,
            touched_files: set.writes.len() + set.deletes.len(),
            touched_paths: &touched_paths,
            hold: &opts.hold,
            designers: &opts.designers,
            clock: ctx.clock,
        },
        steps,
    )
    .await;
    let seen = preflights.as_ref().ok();
    run_dir.write_json(
        "run.json",
        &json!({
            "schema_version": 1,
            "run_id": run_id.as_str(),
            "env": env,
            "gateway": gateway,
            "mode": mode,
            "node": plan.node,
            "transport": plan.transport,
            "started_at": started_at,
            "run_dir_git": run_dir.git_status().as_record(),
            "render_sha256": plan.render_sha256,
            "plan_sha256": plan.plan_sha256,
            "hold_s": opts.hold.hold_s,
            "allow_destructive": opts.gates.allow_destructive,
            "allow_drift": opts.gates.allow_drift,
            "skip_gwbk": opts.skip_gwbk,
            "designers": opts.designers.mode.as_str(),
            "designers_timeout_s": opts.designers.timeout.as_secs(),
            "designers_seen": seen.map(|r| r.designers_seen
                .iter()
                .map(|(user, project, host)| json!({"user": user, "project": project, "host": host}))
                .collect::<Vec<_>>()),
            "preflights": seen.map(|r| r.findings
                .iter()
                .map(|f| json!({"check": f.check.as_str(), "status": f.status.as_str()}))
                .collect::<Vec<_>>()),
        }),
    )?;
    run_dir.write_json("plan.json", plan)?;
    let preflights = preflights?;
    if !preflights.passed() {
        steps.emit(ApplyEvent::PreflightChecks {
            checks: preflights
                .findings
                .iter()
                .map(|f| (f.check.as_str().to_string(), f.status.as_str().to_string()))
                .collect(),
        })?;
        return Err(CoreError::DeployPreflightFailed {
            failures: preflights.failures().into(),
        });
    }

    // The `.gwbk`, before staging and the lock so that it never spends hold
    // budget (D-13). A refusal here leaves the run record and writes nothing.
    let gwbk = take_gwbk(ctx, opts, run_id, run_dir, steps).await?;

    // What is already waiting for a restart, so the run's own additions can
    // be told apart afterwards (an entry persists until the gateway restarts,
    // C-restart). Unreadable is `None`.
    let restart_before = ctx.deploy.restart_pending().await.ok();

    // The ledger: what it lists now, and what it will list.
    let old_bytes = ctx.reader.read_ledger(gateway, mode).await?;
    let old_ledger = match &old_bytes {
        Some(bytes) => Some(Ledger::parse(
            &ledger_location(
                LedgerRoot::for_node(&declared.target.nodes.master),
                gateway,
                mode,
            ),
            bytes,
            gateway,
            mode,
        )?),
        None => None,
    };
    let ledger = new_ledger(plan, old_ledger.as_ref(), &set, run_id);
    let ledger_bytes = ledger.to_bytes();
    Ledger::parse(
        &ledger_location(
            LedgerRoot::for_node(&declared.target.nodes.master),
            gateway,
            mode,
        ),
        &ledger_bytes,
        gateway,
        mode,
    )?;
    let spec = batch_spec(
        plan,
        run_id,
        &set,
        ledger_bytes,
        old_bytes.as_deref().map(sha256_hex),
        commit_budget_s(&opts.hold),
    );
    if let Some(problem) = spec.problem() {
        return Err(CoreError::Internal(format!(
            "the batch the engine built is invalid: {problem}"
        )));
    }

    let scope = RunScope {
        plan,
        run_id,
        run_dir,
        gwbk: gwbk.as_deref(),
        restart_before: restart_before.as_deref(),
        record: &spec.record,
    };

    // Stage outside the lock, then hold the lock for the one commit.
    let prepared = ctx
        .host
        .prepare(&spec, &declared.outcome.tree)
        .await
        .map_err(|e| with_env(e, env))?;
    steps.emit(ApplyEvent::Staged {
        files: set.writes.len(),
    })?;
    let lock =
        match HeldLock::acquire(ctx.deploy, ScanKind::Config, opts.hold.hold_s, ctx.clock).await {
            Ok(lock) => lock,
            Err(refused) => {
                // Nothing was written into the collection. Drop this run's own
                // staging and nothing else; a failed cleanup must not hide why
                // the lock was refused.
                let _ = ctx.host.discard_interrupted(run_id).await;
                return Err(refused);
            }
        };
    steps.emit(ApplyEvent::LockAcquired {
        hold_s: opts.hold.hold_s,
    })?;
    let report = match ctx.host.commit(&prepared).await {
        Ok(report) => report,
        Err(e) => return Err(commit_failed(ctx, run_id, plan, steps, with_env(e, env)).await),
    };
    if let Err(e) = steps.emit(ApplyEvent::Committed {
        moved: report.moved.len(),
        deleted: report.deleted.len(),
    }) {
        // The event log failed with the journal possibly on the box: no
        // restore can be logged either, so the run is reported as it stands.
        return Err(commit_failed(ctx, run_id, plan, steps, e).await);
    }
    if report.stale {
        return Err(plan_stale(ctx, plan, opts, run_id).await);
    }
    if report.failure.is_some() {
        // The failure names an entry by index and never its content; the
        // restore puts back every touched path from the snapshot. A commit
        // that stopped on its time budget ran past the hold (WR-04): the
        // hold, not a file, is what failed.
        let cause = if report.deadline {
            RollbackCause::LockExpired
        } else {
            RollbackCause::WriteFailed
        };
        return roll_back(ctx, opts, &scope, Some(&lock), cause, steps, Vec::new()).await;
    }

    // From here the files have moved and the journal is on the box: every
    // way out is a restore, a typed refusal or the verified deploy (CR-01).
    let mut warnings = match confirm(ctx, opts, &lock, steps).await {
        Ok(Confirmed::Green { warnings }) => warnings,
        Ok(Confirmed::Abort { cause, warnings }) => {
            return roll_back(ctx, opts, &scope, Some(&lock), cause, steps, warnings).await;
        }
        Err(Unconfirmed::Restore { cause, step, error }) => {
            // The deploy could not be shown good; fail closed like a red
            // verify. The step and the error's code (never its text) are kept.
            let note = format!("post_commit_error ({step}: {})", error.code());
            return roll_back(ctx, opts, &scope, Some(&lock), cause, steps, vec![note]).await;
        }
        Err(Unconfirmed::Green(error)) => {
            return Err(unfinished_after_green(ctx, run_id, plan, steps, error).await);
        }
    };

    // Green: record what is live now, finalize, keep the snapshot locally.
    let post = match ctx.host.fingerprint(&spec.touched_paths()).await {
        Ok(post) => post,
        Err(e) => return Err(unfinished_after_green(ctx, run_id, plan, steps, e).await),
    };
    let post_fingerprint = fingerprint(&post);
    let mut record = spec.record.clone();
    record.state = RecordState::Applied;
    record.post_fingerprint = Some(post_fingerprint.clone());
    if let Err(e) = ctx.host.finalize(&record).await {
        // The record is installed before the journal is removed, so a journal
        // that is gone means the box finished and only the answer was lost.
        if !matches!(box_state(ctx, plan, run_id).await, BoxState::Absent) {
            return Err(unfinished_after_green(ctx, run_id, plan, steps, e).await);
        }
        warnings.push(format!("finalize_reported_error ({})", e.code()));
    }
    if steps.emit(ApplyEvent::Finalized).is_err() {
        warnings.push(EVENTS_UNWRITABLE.to_string());
    }

    let (restart_pending, restart_pending_new) =
        restart_report(ctx.deploy, restart_before.as_deref(), &mut warnings).await;
    let mut snapshot_sha256 = BTreeMap::new();
    match ctx.host.fetch_snapshot(run_id, run_dir.path()).await {
        Ok(hashes) => {
            warnings.extend(snapshot_warnings(&spec.record, &hashes));
            snapshot_sha256 = hashes;
        }
        Err(e) => warnings.push(format!("snapshot_download_failed ({})", e.code())),
    }
    Ok(ApplyOutcome {
        kind: OutcomeKind::Applied,
        run_id: Some(run_id.as_str().to_string()),
        run_dir: Some(run_dir.path().to_string_lossy().into_owned()),
        cause: None,
        render_sha256: plan.render_sha256.clone(),
        plan_sha256: plan.plan_sha256.clone(),
        written: set
            .writes
            .iter()
            .map(|w| w.path.as_str().to_string())
            .collect(),
        deleted: set
            .deletes
            .iter()
            .map(|d| d.path.as_str().to_string())
            .collect(),
        post_fingerprint: Some(post_fingerprint),
        warnings,
        gwbk,
        skip_gwbk: opts.skip_gwbk,
        restart_pending,
        restart_pending_new,
        snapshot_sha256,
    })
}

/// How the under-lock window after a clean commit ended when every step could
/// be answered.
enum Confirmed {
    /// The lock held, the scan completed and the verify is green.
    Green { warnings: Vec<String> },
    /// The deploy must not stay: restore with `cause`.
    Abort {
        cause: RollbackCause,
        warnings: Vec<String>,
    },
}

/// A step after the commit that could not be answered at all.
enum Unconfirmed {
    /// Before a green verdict: the deploy is unproven, so it is restored
    /// like a red verify. `step` names the step, `cause` is the closest
    /// option-a cause.
    Restore {
        cause: RollbackCause,
        step: &'static str,
        error: CoreError,
    },
    /// After a green verdict (the event log): the deploy is verified.
    Green(CoreError),
}

/// Judge the files the commit moved: the lock must still be ours with its
/// margin, the config scan must complete, the verify must be green. A step
/// that fails with an `Err` (a gateway or log error, an event that cannot be
/// written) is [`Unconfirmed::Restore`] with the closest cause: the lock read
/// is `lock_expired`, the scan `scan_incomplete`, the verify and its events
/// `verify_failed`.
async fn confirm(
    ctx: &ApplyContext<'_>,
    opts: &ApplyOptions,
    lock: &HeldLock,
    steps: &mut Steps<'_>,
) -> Result<Confirmed, Unconfirmed> {
    fn undone(cause: RollbackCause, step: &'static str) -> impl FnOnce(CoreError) -> Unconfirmed {
        move |error| Unconfirmed::Restore { cause, step, error }
    }
    let abort = |cause, warnings| Ok(Confirmed::Abort { cause, warnings });

    // Only a lock still held, with the margin left, may be followed by a scan.
    let verdict = lock
        .check(ctx.deploy, LOCK_MARGIN_S, ctx.clock)
        .await
        .map_err(undone(RollbackCause::LockExpired, "lock_check"))?;
    let held = matches!(verdict, LockVerdict::Held { .. });
    steps
        .emit(ApplyEvent::LockVerdict { held })
        .map_err(undone(RollbackCause::LockExpired, "events"))?;
    if !held {
        // No scan before the restore: a scan would load the partial tree.
        return abort(RollbackCause::LockExpired, Vec::new());
    }
    // C11: a tag provider and its definitions load in one scan.
    match scan_and_wait(ctx.deploy, ScanKind::Config, opts.poll.clone())
        .await
        .map_err(undone(RollbackCause::ScanIncomplete, "scan"))?
    {
        ScanOutcome::Complete(_) => steps
            .emit(ApplyEvent::ScanDone)
            .map_err(undone(RollbackCause::ScanIncomplete, "events"))?,
        ScanOutcome::NotComplete { .. } => {
            return abort(RollbackCause::ScanIncomplete, Vec::new());
        }
    }

    let verify = deploy_verify(
        ctx.declared,
        &opts.project,
        ctx.deploy,
        ctx.gateway,
        ctx.reader,
        &opts.verify,
    )
    .await
    .map_err(undone(RollbackCause::VerifyFailed, "verify"))?;
    if !verify.ok {
        steps
            .emit(ApplyEvent::VerifyDone { ok: false })
            .map_err(undone(RollbackCause::VerifyFailed, "events"))?;
        // The verdict is kept in the log by check id and status only.
        steps
            .emit(ApplyEvent::VerifyChecks {
                checks: verify
                    .checks
                    .iter()
                    .map(|c| (c.id.word().to_string(), c.status.word().to_string()))
                    .collect(),
            })
            .map_err(undone(RollbackCause::VerifyFailed, "events"))?;
        return abort(RollbackCause::VerifyFailed, verify.warnings);
    }
    steps
        .emit(ApplyEvent::VerifyDone { ok: true })
        .map_err(Unconfirmed::Green)?;
    Ok(Confirmed::Green {
        warnings: verify.warnings,
    })
}

/// Read the pending restarts again and compare with what was pending before
/// the run. Returns `(pending now, pending that were not there before)`, both
/// `[]` when there are none. A list that could not be read, before or now, is
/// the one warning `restart_pending_unavailable`: an unreadable "before" is
/// treated as empty, so everything now pending is reported as new rather than
/// hidden. Read only (SF-3).
async fn restart_report(
    deploy: &dyn DeployApi,
    before: Option<&[String]>,
    warnings: &mut Vec<String>,
) -> (Vec<String>, Vec<String>) {
    let after = deploy.restart_pending().await.ok();
    if before.is_none() || after.is_none() {
        warnings.push(RESTART_UNAVAILABLE.to_string());
    }
    let pending = after.unwrap_or_default();
    let known = before.unwrap_or_default();
    let added = pending
        .iter()
        .filter(|entry| !known.contains(entry))
        .cloned()
        .collect();
    (pending, added)
}

/// Warnings for the paths whose copy in the run directory is not the
/// pre-image the record holds (the on-box snapshot stays authoritative,
/// D-04). A created path has no pre-image and no copy, so it never counts.
fn snapshot_warnings(record: &RunRecord, copied: &BTreeMap<String, String>) -> Vec<String> {
    record
        .pre_map()
        .into_iter()
        .filter_map(|(path, pre)| {
            let pre = pre?;
            let key = if path == LEDGER_KEY {
                "ledger.json".to_string()
            } else {
                format!("files/{path}")
            };
            (copied.get(&key) != Some(&pre)).then(|| format!("snapshot_copy_differs ({path})"))
        })
        .collect()
}

/// The `.gwbk` step (23-09, D-13, SF-6): unless `--skip-gwbk`, download the
/// gateway backup into `<run id>.gwbk` in the run directory and judge that it
/// is whole. Returns the file name, or `None` when skipped.
///
/// The file is created exclusively at 0600 before a byte is written, so it is
/// never readable by anyone else even for a moment (the download opens an
/// existing file and keeps its mode). `BackupType::All` because the archive is
/// a restore point for this same gateway (FA-23-06). It goes from the gateway
/// to the run directory and nowhere else.
///
/// Whole means: the ZIP central directory reads and has an entry (C-gwbk: the
/// gateway sends no Content-Length, and a cut copy has no central
/// directory). Where the gateway does send a length, the HTTP stack already
/// fails a body that stops short, which surfaces here as a failed download.
/// Any failure removes the file (it is useless and holds secrets) and is
/// `deploy_preflight_failed` with check `gwbk`; the detail names no content.
async fn take_gwbk(
    ctx: &ApplyContext<'_>,
    opts: &ApplyOptions,
    run_id: &RunId,
    run_dir: &RunDir,
    steps: &mut Steps<'_>,
) -> Result<Option<String>, CoreError> {
    if opts.skip_gwbk {
        steps.emit(ApplyEvent::GwbkSkipped)?;
        return Ok(None);
    }
    let refuse = |detail: String| CoreError::DeployPreflightFailed {
        failures: vec![PreflightFailure {
            check: GWBK_CHECK.to_string(),
            detail,
        }]
        .into(),
    };
    let name = format!("{}.gwbk", run_id.as_str());
    drop(run_dir.create_private_file(&name)?);
    let path = run_dir.path().join(&name);
    let verdict = match backup_download(ctx.gateway, Some(&path), "", BackupType::All).await {
        Ok(_) => validate_gwbk(&path),
        Err(e) => Err(format!("the .gwbk was not downloaded ({})", e.code())),
    };
    match verdict {
        Ok(bytes) => {
            steps.emit(ApplyEvent::GwbkDownloaded { bytes })?;
            Ok(Some(name))
        }
        Err(detail) => {
            let _ = std::fs::remove_file(&path);
            Err(refuse(detail))
        }
    }
}

/// Judge a downloaded `.gwbk` by the file itself (C-gwbk): not empty, a ZIP
/// whose central directory reads, at least one entry. Returns its size.
fn validate_gwbk(path: &Path) -> Result<u64, String> {
    let bytes = std::fs::metadata(path)
        .map_err(|_| "the downloaded .gwbk cannot be read back".to_string())?
        .len();
    if bytes == 0 {
        return Err("the downloaded .gwbk is empty".to_string());
    }
    let file = std::fs::File::open(path)
        .map_err(|_| "the downloaded .gwbk cannot be read back".to_string())?;
    let archive = zip::ZipArchive::new(file).map_err(|_| {
        "the downloaded .gwbk is not a readable ZIP (no central directory), so the download is incomplete"
            .to_string()
    })?;
    if archive.is_empty() {
        return Err("the downloaded .gwbk is a ZIP with no entries".to_string());
    }
    Ok(bytes)
}

/// What an abort needs to know about the run that is being undone.
struct RunScope<'a> {
    plan: &'a DeployPlanResult,
    run_id: &'a RunId,
    run_dir: &'a RunDir,
    /// The `.gwbk` taken before staging, if any.
    gwbk: Option<&'a str>,
    /// The pending restarts read before staging; `None` when unreadable.
    restart_before: Option<&'a [String]>,
    /// The record at state `applying`: its pre-images are what the restore
    /// puts back.
    record: &'a RunRecord,
}

/// End a run that must not stay applied: restore the snapshot through
/// [`restore_snapshot`] and report the cause. Never `applied`.
async fn roll_back(
    ctx: &ApplyContext<'_>,
    opts: &ApplyOptions,
    scope: &RunScope<'_>,
    held: Option<&HeldLock>,
    cause: RollbackCause,
    steps: &mut Steps<'_>,
    mut warnings: Vec<String>,
) -> Result<ApplyOutcome, CoreError> {
    let restored = restore_snapshot(
        &ctx.restore_context(opts),
        &RestoreInput {
            run_id: scope.run_id,
            run_dir: scope.run_dir,
            record: scope.record,
            held,
            cause: Some(cause),
        },
        steps,
    )
    .await?;
    warnings.extend(restored.warnings);
    let (restart_pending, restart_pending_new) =
        restart_report(ctx.deploy, scope.restart_before, &mut warnings).await;
    Ok(ApplyOutcome {
        kind: OutcomeKind::RolledBack,
        run_id: Some(scope.run_id.as_str().to_string()),
        run_dir: Some(scope.run_dir.path().to_string_lossy().into_owned()),
        cause: Some(cause),
        render_sha256: scope.plan.render_sha256.clone(),
        plan_sha256: scope.plan.plan_sha256.clone(),
        written: Vec::new(),
        deleted: Vec::new(),
        post_fingerprint: Some(scope.record.pre_fingerprint.clone()),
        warnings,
        gwbk: scope.gwbk.map(str::to_string),
        skip_gwbk: opts.skip_gwbk,
        restart_pending,
        restart_pending_new,
        snapshot_sha256: restored.snapshot_sha256,
    })
}

/// What [`restore_snapshot`] is asked to undo.
pub struct RestoreInput<'a> {
    /// The run doing the restoring: for an abort or an auto-rollback the run
    /// itself, for a manual rollback the rollback run (it becomes the
    /// record's `rolled_back_by`).
    pub run_id: &'a RunId,
    /// That run's directory (named in `rollback_failed`; the snapshot copy
    /// is downloaded into it).
    pub run_dir: &'a RunDir,
    /// The record of the run being undone: its touched set and pre-images.
    pub record: &'a RunRecord,
    /// The scan-lock this engine still holds, when it does.
    pub held: Option<&'a HeldLock>,
    /// Why an apply is being undone; `None` for a manual rollback (23-11),
    /// which keeps the closed `data.cause` vocabulary of an apply as it is.
    pub cause: Option<RollbackCause>,
}

/// What [`restore_snapshot`] drives. It names no declared repo: a manual
/// rollback (23-11) has only the target file and a run id.
pub struct RestoreContext<'a> {
    /// The `--env` value, named in the retry command of a `rollback_failed`.
    pub env: &'a str,
    /// The target's master node (where the journal lives, for the error).
    pub master: &'a NodeTarget,
    /// The write seam of the master's host.
    pub host: &'a dyn ApplyHost,
    /// The gateway REST surface (lock, scan, mode).
    pub deploy: &'a dyn DeployApi,
    /// The monotonic clock the lock is judged by.
    pub clock: &'a dyn Clock,
    /// The hold a fresh lock is requested with, and its estimates.
    pub hold: &'a HoldModel,
    /// How the scan wait polls.
    pub poll: &'a PollConfig,
}

/// What a finished restore did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreReport {
    /// Paths put back from the snapshot.
    pub restored: usize,
    /// Files this run created that the restore removed.
    pub removed: usize,
    /// Whether the restore took a fresh scan-lock instead of reusing the
    /// held one.
    pub fresh_lock: bool,
    /// Non-failing notes.
    pub warnings: Vec<String>,
    /// SHA-256 of each file copied into the run directory's `snapshot/`,
    /// keyed by its path below it.
    pub snapshot_sha256: BTreeMap<String, String>,
}

/// The one restore path (abort, auto-rollback and, from 23-11, manual
/// rollback). In this order:
///
/// 1. `restore_started`;
/// 2. the lock: the held one is reused when the gateway says it is still
///    ours with the restore's estimated time plus [`LOCK_MARGIN_S`] left (the
///    local clock is not consulted: it can only read shorter); if it is
///    ours but too short the restore waits for it to lapse; then, or when
///    there is no held lock, a fresh hold is taken (a holder's own
///    re-acquire is a 409, C1d, so it is never tried while the hold is still
///    ours);
/// 3. exactly one `ApplyHost::restore`;
/// 4. exactly one config scan, only now (research Pitfall 1);
/// 5. the restore check: the touched set's live hashes equal the record's
///    pre-images, the scan completed, `deploymentMode` is the target's.
///    Never `deploy_verify`: after a restore the declared tree is no longer
///    the expected state (research Pitfall 9);
/// 6. the record to `rolled_back`, `ApplyHost::finalize` (installs it, drops
///    the journal and staging), `restored`, and the snapshot copy into the
///    run directory (a failure is a warning).
///
/// Any failure is [`CoreError::RollbackFailed`] naming the run, its
/// directory and the on-box journal, which stays (D-12).
pub async fn restore_snapshot(
    ctx: &RestoreContext<'_>,
    input: &RestoreInput<'_>,
    steps: &mut Steps<'_>,
) -> Result<RestoreReport, CoreError> {
    let record = input.record;
    match restore_steps(ctx, input, steps).await {
        Ok(report) => Ok(report),
        Err(cause) => Err(CoreError::RollbackFailed(Box::new(RollbackFailure {
            run_id: record.run_id.clone(),
            env: ctx.env.to_string(),
            run_dir: Some(input.run_dir.path().to_string_lossy().into_owned()),
            journal: journal_location(
                LedgerRoot::for_node(ctx.master),
                &record.gateway,
                &record.mode,
            ),
            cause,
        }))),
    }
}

/// The steps of [`restore_snapshot`]; an `Err` is a value-free reason.
async fn restore_steps(
    ctx: &RestoreContext<'_>,
    input: &RestoreInput<'_>,
    steps: &mut Steps<'_>,
) -> Result<RestoreReport, String> {
    // A gateway error is named by its code only; a host error already names
    // entries by index and never their content.
    let gw = |step: &'static str| move |e: CoreError| format!("{step}: {}", e.code());
    let host = |step: &'static str| move |e: CoreError| format!("{step}: {}", with_env(e, ctx.env));
    let event = |e: CoreError| format!("events: {}", e.code());

    let record = input.record;
    let deploy = ctx.deploy;
    steps
        .emit(ApplyEvent::RestoreStarted { cause: input.cause })
        .map_err(event)?;

    // The lock.
    let need_s = ctx.hold.estimate_commit_s(record.touched.len()).ceil() as i64 + LOCK_MARGIN_S;
    let reuse = match input.held {
        Some(lock) => match lock
            .check_gateway(deploy, need_s)
            .await
            .map_err(gw("checking the scan-lock"))?
        {
            LockVerdict::Held { .. } => true,
            LockVerdict::Expiring { .. } => {
                lock.wait_for_lapse(deploy, ctx.poll.interval)
                    .await
                    .map_err(gw("waiting for the scan-lock to lapse"))?;
                false
            }
            LockVerdict::NotHeld => false,
        },
        None => false,
    };
    if !reuse {
        HeldLock::acquire(deploy, ScanKind::Config, ctx.hold.hold_s, ctx.clock)
            .await
            .map_err(gw("taking the scan-lock"))?;
        steps
            .emit(ApplyEvent::LockAcquired {
                hold_s: ctx.hold.hold_s,
            })
            .map_err(event)?;
    }

    // One restore call, then one scan.
    let report = ctx.host.restore(record).await.map_err(host("restore"))?;
    if let Some(failure) = &report.failure {
        return Err(format!("restore: {failure}"));
    }
    if report.stale {
        return Err("restore: the host reported a stale pre-image".to_string());
    }
    match scan_and_wait(deploy, ScanKind::Config, ctx.poll.clone())
        .await
        .map_err(gw("the scan after the restore"))?
    {
        ScanOutcome::Complete(_) => steps.emit(ApplyEvent::ScanDone).map_err(event)?,
        ScanOutcome::NotComplete { .. } => {
            return Err("the scan after the restore did not complete".to_string());
        }
    }

    // The restore check (Pitfall 9).
    let touched: Vec<String> = record.touched.iter().map(|e| e.path.clone()).collect();
    let live = ctx
        .host
        .fingerprint(&touched)
        .await
        .map_err(host("reading the restored files"))?;
    let pre = record.pre_map();
    let differing: Vec<String> = pre
        .iter()
        .filter(|(path, hash)| live.get(*path) != Some(*hash))
        .map(|(path, _)| path.clone())
        .collect();
    if !differing.is_empty() {
        return Err(format!(
            "{} restored path(s) differ from the snapshot: {}",
            differing.len(),
            listed(&differing)
        ));
    }
    let mode = deploy
        .deployment_mode()
        .await
        .map_err(gw("reading the deployment mode"))?;
    if mode.as_deref() != Some(record.mode.as_str()) {
        return Err("the gateway's deployment mode is not the target's".to_string());
    }

    // Done: install the final record, drop the journal and the staging.
    let mut rolled_back = record.clone();
    rolled_back.state = RecordState::RolledBack;
    rolled_back.rolled_back_by = Some(input.run_id.as_str().to_string());
    rolled_back.post_fingerprint = Some(record.pre_fingerprint.clone());
    ctx.host
        .finalize(&rolled_back)
        .await
        .map_err(host("finalize"))?;
    steps.emit(ApplyEvent::Restored).map_err(event)?;

    let mut warnings = Vec::new();
    let mut snapshot_sha256 = BTreeMap::new();
    match RunId::parse(&record.run_id) {
        Ok(restored_run) => {
            match ctx
                .host
                .fetch_snapshot(&restored_run, input.run_dir.path())
                .await
            {
                Ok(hashes) => {
                    warnings.extend(snapshot_warnings(record, &hashes));
                    snapshot_sha256 = hashes;
                }
                Err(e) => warnings.push(format!("snapshot_download_failed ({})", e.code())),
            }
        }
        Err(e) => warnings.push(format!("snapshot_download_failed ({})", e.code())),
    }
    Ok(RestoreReport {
        restored: report.moved.len(),
        removed: report.deleted.len(),
        fresh_lock: !reuse,
        warnings,
        snapshot_sha256,
    })
}

/// Up to [`crate::error::APPLY_LIST_SHOWN`] paths, then `and N more`.
fn listed(paths: &[String]) -> String {
    let shown = paths.len().min(crate::error::APPLY_LIST_SHOWN);
    let mut out = paths[..shown].join(", ");
    if paths.len() > shown {
        out.push_str(&format!(" and {} more", paths.len() - shown));
    }
    out
}

/// The pre-image check under the lock found a live file that is not what the
/// plan was made against. The host moved nothing and removed its journal and
/// partial snapshot; this drops the run's staging, issues one no-op scan to
/// release the lock (D12) and names the plan as it reads now.
async fn plan_stale(
    ctx: &ApplyContext<'_>,
    plan: &DeployPlanResult,
    opts: &ApplyOptions,
    run_id: &RunId,
) -> CoreError {
    // Best effort: the refusal below is the answer either way, and an
    // unreleased lock lapses by itself.
    let _ = ctx.host.discard_interrupted(run_id).await;
    let _ = ctx.deploy.scan_request(ScanKind::Config).await;
    let actual = match deploy_plan(
        ctx.declared,
        ctx.reader,
        &PlanOptions {
            base: opts.base.clone(),
            diff: false,
        },
    )
    .await
    {
        Ok(now) => now.plan_sha256,
        Err(_) => "unavailable".to_string(),
    };
    CoreError::PlanStale {
        expected: plan.plan_sha256.clone(),
        actual,
        detail: "a live file changed between the plan and the lock; nothing was written"
            .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(touched: &[(&str, Option<&str>)]) -> RunRecord {
        let touched: Vec<TouchedEntry> = touched
            .iter()
            .map(|(path, pre)| TouchedEntry {
                path: (*path).to_string(),
                pre_sha256: pre.map(str::to_string),
                post_sha256: Some("f".repeat(64)),
            })
            .collect();
        RunRecord {
            schema_version: 1,
            run_id: "20261016T000000Z-aaaaaa".to_string(),
            gateway: "gw".to_string(),
            mode: "dev".to_string(),
            render_sha256: "a".repeat(64),
            plan_sha256: "b".repeat(64),
            state: RecordState::Applying,
            rolled_back_by: None,
            touched,
            pre_fingerprint: "c".repeat(64),
            post_fingerprint: None,
        }
    }

    #[test]
    fn the_commit_budget_is_the_hold_less_its_margin_and_never_zero() {
        let hold = |hold_s| HoldModel {
            hold_s,
            ..HoldModel::MEASURED
        };
        // 176 s of hold leaves 174 s: the scan can still be triggered inside
        // the hold when the host stops at its budget.
        assert_eq!(commit_budget_s(&hold(176)), 174);
        assert_eq!(commit_budget_s(&HoldModel::MEASURED), 174);
        // A tiny hold still gives the commit one second, and a huge one is
        // capped at what a work list can carry.
        assert_eq!(commit_budget_s(&hold(2)), 1);
        assert_eq!(commit_budget_s(&hold(0)), 1);
        assert_eq!(commit_budget_s(&hold(1_000_000)), BUDGET_MAX_S);
    }

    #[test]
    fn snapshot_warnings_name_a_differing_or_missing_copy_and_skip_created_paths() {
        let (h1, h2) = ("1".repeat(64), "2".repeat(64));
        let rec = record(&[
            ("a/config.json", Some(&h1)),
            ("b/config.json", Some(&h1)),
            ("c/config.json", None),
            (LEDGER_KEY, Some(&h2)),
        ]);
        let mut copied = BTreeMap::new();
        copied.insert("files/a/config.json".to_string(), h1.clone());
        copied.insert("files/b/config.json".to_string(), h2.clone());
        copied.insert("ledger.json".to_string(), h2);
        // a matches, b differs, c was created (no copy needed), the ledger
        // matches.
        assert_eq!(
            snapshot_warnings(&rec, &copied),
            ["snapshot_copy_differs (b/config.json)"]
        );
        // A copy that is missing counts as different.
        copied.remove("files/a/config.json");
        assert_eq!(
            snapshot_warnings(&rec, &copied),
            [
                "snapshot_copy_differs (a/config.json)",
                "snapshot_copy_differs (b/config.json)"
            ]
        );
    }
}
