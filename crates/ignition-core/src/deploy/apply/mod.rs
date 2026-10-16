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
//! 1. the run directory (the git-ignore guard first), `run.json`, `plan.json`
//!    and the events log;
//! 2. the [`WriteSet`] and the new ledger, validated with `Ledger::parse`;
//! 3. `ApplyHost::prepare`, outside the lock;
//! 4. [`hold::HeldLock::acquire`];
//! 5. exactly one `ApplyHost::commit`;
//! 6. [`hold::HeldLock::check`], which must say `Held`;
//! 7. `scan_and_wait` on the config scan, once (C11: a tag provider and its
//!    definitions load in one scan);
//! 8. `deploy_verify`, which must be green;
//! 9. the post fingerprint, then `ApplyHost::finalize`;
//! 10. the snapshot download into the run directory (a failure is a warning).
//!
//! Only a green verify makes the outcome `applied`. Every branch that would
//! end elsewhere (a stale pre-image, a commit that failed after it moved
//! files, a lapsed lock, an incomplete scan, a red verify, a failed lock
//! acquire) returns [`crate::error::CoreError::Internal`] naming its cause in
//! this plan; 23-17 replaces each with `restore_snapshot` or a precise
//! refusal. Added by later plans: the preflights (23-07), `.gwbk` and
//! restart-pending (23-09), the journal refusal (23-12).
//!
//! Once the run directory exists, `run_apply` leaves through one exit point
//! that writes `result.json` once.

pub mod fingerprint;
pub mod gates;
pub mod hold;
pub mod host;
pub mod journal;
pub mod record;
pub mod run_store;

use std::path::PathBuf;

use serde::Serialize;

use crate::actions::deploy::{Declared, DeployPlanResult, VerifyOptions};
use crate::client::{DeployApi, GatewayApi};
use crate::deploy::transport::TreeReader;
use crate::error::CoreError;
use crate::poll::PollConfig;

use self::gates::GateFlags;
use self::hold::{Clock, HoldModel};
use self::host::{ApplyHost, DeleteOp, WriteOp};

/// The slug of the rolled-back apply verdict.
///
/// Decision A: `deploy_rolled_back` rides the verdict payload (`data.slug`,
/// exit 6) and is never a [`crate::error::CoreError`] variant, never in
/// [`crate::error::CoreError::DEPLOY_SLUGS`] and never a row of the README
/// exit table. It follows the `deploy_verify_failed` precedent
/// ([`crate::deploy::verify::VERIFY_FAILED_SLUG`]).
pub const ROLLED_BACK_SLUG: &str = "deploy_rolled_back";

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

/// The write set of `plan`.
pub fn write_set(_plan: &DeployPlanResult, _declared: &Declared) -> Result<WriteSet, CoreError> {
    Err(CoreError::Internal("write_set is not built".into()))
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
    /// A restore began (23-17).
    RestoreStarted {
        /// Why.
        cause: RollbackCause,
    },
    /// The restore finished (23-17).
    Restored,
    /// The run failed.
    Failed {
        /// The error code slug.
        code: String,
    },
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
    /// The monotonic clock the lock is judged by.
    pub clock: &'a dyn Clock,
    /// Hears each step.
    pub observer: &'a dyn ApplyObserver,
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
}

/// Apply `plan` (see the module doc for the order).
pub async fn run_apply(
    _ctx: &ApplyContext<'_>,
    _plan: &DeployPlanResult,
    _opts: &ApplyOptions,
) -> Result<ApplyOutcome, CoreError> {
    Err(CoreError::Internal("run_apply is not built".into()))
}
