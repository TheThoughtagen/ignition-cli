//! `ign deploy apply` as library verbs (Phase 23, DAPL-01, DAPL-03,
//! DAPL-06): [`deploy_apply_prepare`] re-plans and evaluates the gates,
//! [`deploy_apply_execute`] drives the engine. Neither prints.

use std::path::PathBuf;

use serde::Serialize;

use crate::actions::deploy::{Declared, DeployPlanResult, PlanOptions, deploy_plan};
use crate::deploy::apply::gates::{
    GateFlags, GateOutcome, evaluate_gates, validate_drift_reason, validate_plan_sha256,
};
use crate::deploy::apply::{
    ApplyContext, ApplyOptions, OutcomeKind, ROLLED_BACK_SLUG, RollbackCause, run_apply,
};
use crate::deploy::plan::PlanSummary;
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
    Ok(result)
}
