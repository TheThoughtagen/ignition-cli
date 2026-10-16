//! `ign deploy apply` as library verbs (Phase 23, DAPL-01, DAPL-03,
//! DAPL-06): [`deploy_apply_prepare`] re-plans and evaluates the gates,
//! [`deploy_apply_execute`] drives the engine. Neither prints.

use std::path::PathBuf;

use serde::Serialize;

use crate::actions::deploy::{Declared, DeployPlanResult};
use crate::deploy::apply::gates::GateFlags;
use crate::deploy::apply::{ApplyContext, ApplyOptions, OutcomeKind, RollbackCause};
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

/// Validate the flags, re-plan against the host and evaluate the gates.
pub async fn deploy_apply_prepare(
    _declared: &Declared,
    _reader: &dyn TreeReader,
    _opts: &PrepareOptions,
) -> Result<ApplyPrepared, CoreError> {
    Err(CoreError::Internal(
        "deploy_apply_prepare is not built".into(),
    ))
}

/// Run a prepared apply.
pub async fn deploy_apply_execute(
    _prepared: ApplyPrepared,
    _ctx: &ApplyContext<'_>,
    _opts: &ApplyOptions,
) -> Result<DeployApplyResult, CoreError> {
    Err(CoreError::Internal(
        "deploy_apply_execute is not built".into(),
    ))
}
