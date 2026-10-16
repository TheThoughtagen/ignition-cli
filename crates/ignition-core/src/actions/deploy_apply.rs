//! `ign deploy apply` and `ign deploy rollback` as library verbs (Phase 23,
//! DAPL-01, DAPL-03, DAPL-06, DAPL-08): [`deploy_apply_prepare`] re-plans and
//! evaluates the gates, [`deploy_apply_execute`] drives the engine;
//! [`deploy_rollback_prepare`] reads the on-box record of a past run and
//! refuses what cannot be rolled back, [`deploy_rollback_execute`] restores
//! it. None of them prints.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::actions::deploy::{Declared, DeployPlanResult, PlanOptions, deploy_plan, load_target};
use crate::deploy::apply::gates::{
    GateFlags, GateOutcome, evaluate_gates, validate_drift_reason, validate_plan_sha256,
};
use crate::deploy::apply::host::ApplyHost;
use crate::deploy::apply::record::RunRecord;
use crate::deploy::apply::run_store::RunId;
use crate::deploy::apply::{
    ApplyContext, ApplyOptions, InterruptedRun, OutcomeKind, ROLLED_BACK_SLUG, RollbackCause,
    RollbackContext, RollbackKind, RollbackOptions, check_rollback, restore_set, run_apply,
    run_rollback,
};
use crate::deploy::plan::PlanSummary;
use crate::deploy::target::Target;
use crate::deploy::transport::TreeReader;
use crate::error::CoreError;

/// What `prepare` needs.
#[derive(Debug, Clone, Default)]
pub struct PrepareOptions {
    /// The `--base` render for the drift check.
    pub base: Option<PathBuf>,
    /// The gate flags.
    pub gates: GateFlags,
}

/// What `prepare` found.
#[derive(Debug, Clone, PartialEq)]
pub enum ApplyPrepared {
    /// The plan is empty; the result is final.
    Noop(DeployApplyResult),
    /// Every gate passed.
    Ready {
        /// The plan to apply.
        plan: DeployPlanResult,
        /// The confirmation text.
        prose: String,
    },
}

/// The `ign deploy apply` result (the envelope's `data`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeployApplyResult {
    /// The `--env` value.
    pub env: String,
    /// Gateway directory name from the target.
    pub gateway: String,
    /// Deployment mode from the target.
    pub mode: String,
    /// The node: always `master`.
    pub node: String,
    /// How the host was reached: `local` or `ssh`.
    pub transport: String,
    /// The run; null for a no-op.
    pub run_id: Option<String>,
    /// The run directory; null for a no-op.
    pub run_dir: Option<String>,
    /// `applied`, `noop` or `rolled_back`.
    pub outcome: OutcomeKind,
    /// True unless the run rolled back.
    pub ok: bool,
    /// `deploy_rolled_back` when the run rolled back, else null.
    pub slug: Option<&'static str>,
    /// Why the run rolled back.
    pub cause: Option<RollbackCause>,
    /// `plan_sha256` of the plan.
    pub plan_sha256: String,
    /// `render_sha256` of the render.
    pub render_sha256: String,
    /// The plan's counts by status.
    pub summary: PlanSummary,
    /// Collection paths written.
    pub written: Vec<String>,
    /// Collection paths deleted.
    pub deleted: Vec<String>,
    /// Plan and engine warnings.
    pub warnings: Vec<String>,
    /// The `.gwbk` file name in the run directory; null for a no-op or
    /// `--skip-gwbk`.
    pub gwbk: Option<String>,
    /// Whether the run was started with `--skip-gwbk` (always false for a
    /// no-op, which takes no options).
    pub skip_gwbk: bool,
    /// The pending restarts after the run; `[]` when none (never null).
    pub restart_pending: Vec<String>,
    /// The pending restarts the run added; `[]` when none (never null).
    pub restart_pending_new: Vec<String>,
}

impl DeployApplyResult {
    /// `Some(6)` exactly when the run rolled back.
    pub fn failure_exit_code(&self) -> Option<u8> {
        (self.outcome == OutcomeKind::RolledBack).then_some(6)
    }
}

/// The result of a run that did nothing: counts and hashes of `plan`.
fn base_result(plan: &DeployPlanResult, outcome: OutcomeKind) -> DeployApplyResult {
    DeployApplyResult {
        env: plan.env.clone(),
        gateway: plan.gateway.clone(),
        mode: plan.mode.clone(),
        node: plan.node.clone(),
        transport: plan.transport.clone(),
        run_id: None,
        run_dir: None,
        outcome,
        ok: outcome != OutcomeKind::RolledBack,
        slug: (outcome == OutcomeKind::RolledBack).then_some(ROLLED_BACK_SLUG),
        cause: None,
        plan_sha256: plan.plan_sha256.clone(),
        render_sha256: plan.render_sha256.clone(),
        summary: plan.summary.clone(),
        written: Vec::new(),
        deleted: Vec::new(),
        warnings: plan.warnings.clone(),
        gwbk: None,
        skip_gwbk: false,
        restart_pending: Vec::new(),
        restart_pending_new: Vec::new(),
    }
}

/// Validate the flags, re-plan against the host and evaluate the gates.
///
/// Reads the host (through `reader`) and nothing else: no gateway session,
/// no lock, no scan, no run directory. An empty plan is
/// [`ApplyPrepared::Noop`] with outcome `noop` (DAPL-03); a refused gate is
/// its error.
pub async fn deploy_apply_prepare(
    declared: &Declared,
    reader: &dyn TreeReader,
    opts: &PrepareOptions,
) -> Result<ApplyPrepared, CoreError> {
    // A malformed flag is invalid_input before any host contact.
    if let Some(hash) = &opts.gates.plan_sha256 {
        validate_plan_sha256(hash)?;
    }
    if let Some(reason) = &opts.gates.allow_drift {
        validate_drift_reason(reason)?;
    }
    let plan = deploy_plan(
        declared,
        reader,
        &PlanOptions {
            base: opts.base.clone(),
            diff: false,
        },
    )
    .await?;
    match evaluate_gates(&plan, &opts.gates)? {
        GateOutcome::Noop => Ok(ApplyPrepared::Noop(base_result(&plan, OutcomeKind::Noop))),
        GateOutcome::Ready { prose } => Ok(ApplyPrepared::Ready { plan, prose }),
    }
}

/// Run a prepared apply: a no-op is returned as it is, a ready plan goes
/// through [`run_apply`].
pub async fn deploy_apply_execute(
    prepared: ApplyPrepared,
    ctx: &ApplyContext<'_>,
    opts: &ApplyOptions,
) -> Result<DeployApplyResult, CoreError> {
    let plan = match prepared {
        ApplyPrepared::Noop(result) => return Ok(result),
        ApplyPrepared::Ready { plan, .. } => plan,
    };
    let outcome = run_apply(ctx, &plan, opts).await?;
    let mut result = base_result(&plan, outcome.kind);
    result.run_id = outcome.run_id;
    result.run_dir = outcome.run_dir;
    result.cause = outcome.cause;
    result.written = outcome.written;
    result.deleted = outcome.deleted;
    result.warnings.extend(outcome.warnings);
    result.gwbk = outcome.gwbk;
    result.skip_gwbk = outcome.skip_gwbk;
    result.restart_pending = outcome.restart_pending;
    result.restart_pending_new = outcome.restart_pending_new;
    Ok(result)
}

// ============================================================================
// Rollback (23-11)
// ============================================================================

/// What `deploy_rollback_prepare` takes besides the run id.
#[derive(Debug, Clone, Copy, Default)]
pub struct RollbackPrepareOptions {
    /// `--discard-live-changes`: carry changed files instead of refusing.
    pub discard_live_changes: bool,
}

/// What `deploy_rollback_prepare` found: the rollback may go ahead.
#[derive(Debug, Clone)]
pub struct RollbackPrepared {
    /// The on-box record of the run, at state `applied`.
    pub record: RunRecord,
    /// The confirmation text: run, gateway, mode, counts and `plan_sha256`.
    pub prose: String,
    /// Touched paths changed since the run (only with
    /// `discard_live_changes`).
    pub live_changed: Vec<String>,
    /// The target the file names (the caller builds the rollback context
    /// from it).
    pub target: Target,
    /// Repo-relative path of the target file.
    pub target_file: String,
}

/// The `ign deploy rollback` result (the envelope's `data`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeployRollbackResult {
    /// The `--env` value.
    pub env: String,
    /// Gateway directory name from the target.
    pub gateway: String,
    /// Deployment mode from the target.
    pub mode: String,
    /// This rollback's own run id.
    pub run_id: String,
    /// The run that was rolled back (`--run`).
    pub target_run_id: String,
    /// This rollback's run directory.
    pub run_dir: String,
    /// `restored`.
    pub outcome: RollbackKind,
    /// Always true: a rollback that cannot restore is an error, not a result.
    pub ok: bool,
    /// Collection paths put back from the snapshot.
    pub restored: Vec<String>,
    /// Collection paths the rolled-back run had created, now removed.
    pub deleted: Vec<String>,
    /// Touched paths that had changed since the run and were overwritten;
    /// `[]` when none (never null).
    pub discarded_live_changes: Vec<String>,
    /// Engine warnings.
    pub warnings: Vec<String>,
    /// The pending restarts after the rollback; `[]` when none (never null).
    pub restart_pending: Vec<String>,
    /// The pending restarts the rollback added; `[]` when none (never null).
    pub restart_pending_new: Vec<String>,
}

/// The confirmation prose of a rollback: byte-stable, no file content.
fn rollback_prose(
    record: &RunRecord,
    live_changed: &[String],
    interrupted: Option<&InterruptedRun>,
) -> String {
    let (restored, deleted) = restore_set(record);
    let ledger = record
        .touched
        .iter()
        .find(|e| e.path == crate::deploy::apply::fingerprint::LEDGER_KEY)
        .map(|e| {
            if e.pre_sha256.is_some() {
                ", and restore the ledger"
            } else {
                ", and remove the ledger"
            }
        })
        .unwrap_or("");
    let mut prose = match interrupted {
        Some(found) => format!(
            "Recover run {} on gateway {} (mode {}), which was interrupted at phase {}: \
             restore {} file(s) from the on-box snapshot and delete {} file(s) the run \
             created{}, then remove the journal. The run was applying plan_sha256 {}.",
            record.run_id,
            record.gateway,
            record.mode,
            found.journal.phase.as_str(),
            restored.len(),
            deleted.len(),
            ledger,
            record.plan_sha256,
        ),
        None => format!(
            "Roll back run {} on gateway {} (mode {}): restore {} file(s) from the on-box snapshot \
             and delete {} file(s) the run created{}. The run applied plan_sha256 {}.",
            record.run_id,
            record.gateway,
            record.mode,
            restored.len(),
            deleted.len(),
            ledger,
            record.plan_sha256,
        ),
    };
    if !live_changed.is_empty() {
        prose.push_str(&format!(
            " {} file(s) changed after the run and will be overwritten: {}.",
            live_changed.len(),
            live_changed
                .iter()
                .take(crate::error::APPLY_LIST_SHOWN)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    prose
}

/// Validate the run id, then read the host and decide whether the run can be
/// rolled back.
///
/// The id is checked before any host contact (`invalid_input`). After that
/// this reads the target file, the host's journal, the on-box record, the
/// run's local run directory when it exists, and the live hashes of the
/// touched set; it writes nothing and takes no lock. See
/// [`check_rollback`] for the refusals.
pub async fn deploy_rollback_prepare(
    repo: &Path,
    env: &str,
    run_id_text: &str,
    reader: &dyn TreeReader,
    host: &dyn ApplyHost,
    opts: &RollbackPrepareOptions,
) -> Result<RollbackPrepared, CoreError> {
    let run_id = RunId::parse(run_id_text)?;
    let (target_file, target) = load_target(repo, env)?;
    let checked = check_rollback(
        env,
        repo,
        &target,
        reader,
        host,
        &run_id,
        opts.discard_live_changes,
    )
    .await?;
    Ok(RollbackPrepared {
        prose: rollback_prose(
            &checked.record,
            &checked.live_changed,
            checked.interrupted.as_ref(),
        ),
        record: checked.record,
        live_changed: checked.live_changed,
        target,
        target_file,
    })
}

/// Run a prepared rollback through [`run_rollback`].
pub async fn deploy_rollback_execute(
    prepared: RollbackPrepared,
    ctx: &RollbackContext<'_>,
    opts: &RollbackOptions,
) -> Result<DeployRollbackResult, CoreError> {
    if prepared.record.run_id != opts.run_id.as_str() {
        return Err(CoreError::Internal(
            "the prepared rollback is for another run than the options name".to_string(),
        ));
    }
    let outcome = run_rollback(ctx, opts).await?;
    Ok(DeployRollbackResult {
        env: ctx.env.to_string(),
        gateway: ctx.target.gateway.clone(),
        mode: ctx.target.mode.clone(),
        run_id: outcome.run_id,
        target_run_id: outcome.target_run_id,
        run_dir: outcome.run_dir,
        outcome: outcome.kind,
        ok: true,
        restored: outcome.restored,
        deleted: outcome.deleted,
        discarded_live_changes: outcome.discarded_live_changes,
        warnings: outcome.warnings,
        restart_pending: outcome.restart_pending,
        restart_pending_new: outcome.restart_pending_new,
    })
}
