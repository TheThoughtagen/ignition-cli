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

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::Serialize;
use serde_json::{Value, json};

use crate::actions::deploy::{Declared, DeployPlanResult, VerifyOptions, deploy_verify};
use crate::actions::workspace::rfc3339_now_utc;
use crate::client::deploy_api::scan_and_wait;
use crate::client::scan::{LockVerdict, ScanKind, ScanOutcome};
use crate::client::{DeployApi, GatewayApi};
use crate::deploy::layout::RelPath;
use crate::deploy::ledger::{Ledger, LedgerRoot, journal_location, ledger_location};
use crate::deploy::plan::PlanStatus;
use crate::deploy::transport::TreeReader;
use crate::deploy::tree::sha256_hex;
use crate::deploy::verify::CheckStatus;
use crate::error::{CoreError, RollbackFailure};
use crate::poll::PollConfig;

use self::fingerprint::{LEDGER_KEY, fingerprint};
use self::gates::GateFlags;
use self::hold::{Clock, HeldLock, HoldModel, LOCK_MARGIN_S};
use self::host::{ApplyHost, BatchSpec, DeleteOp, WriteOp};
use self::journal::{Journal, JournalPhase};
use self::record::{RecordState, RunRecord, TouchedEntry};
use self::run_store::{EventLog, RunDir, RunId};

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

/// The batch the host will commit: the writes, the deletes, the new ledger
/// and the journal and record that describe exactly them.
fn batch_spec(
    plan: &DeployPlanResult,
    run_id: &RunId,
    set: &WriteSet,
    ledger_new: Vec<u8>,
    ledger_pre: Option<String>,
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
            ApplyEvent::Finalized => "finalized",
            ApplyEvent::RestoreStarted { .. } => "restore_started",
            ApplyEvent::Restored => "restored",
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
            ApplyEvent::RestoreStarted { cause } => json!({"cause": cause.as_str()}),
            ApplyEvent::Failed { code } => json!({"code": code}),
            ApplyEvent::ScanDone | ApplyEvent::Finalized | ApplyEvent::Restored => Value::Null,
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
            "started_at": rfc3339_now_utc(),
            "run_dir_git": run_dir.git_status().as_record(),
            "render_sha256": plan.render_sha256,
            "plan_sha256": plan.plan_sha256,
            "hold_s": opts.hold.hold_s,
            "allow_destructive": opts.gates.allow_destructive,
            "allow_drift": opts.gates.allow_drift,
        }),
    )?;
    run_dir.write_json("plan.json", plan)?;
    steps.log = Some(EventLog::open(run_dir)?);

    let set = write_set(plan, declared)?;
    steps.emit(ApplyEvent::RunStarted {
        run_id: run_id.as_str().to_string(),
        writes: set.writes.len(),
        deletes: set.deletes.len(),
    })?;

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
    let lock = HeldLock::acquire(ctx.deploy, ScanKind::Config, opts.hold.hold_s, ctx.clock).await?;
    steps.emit(ApplyEvent::LockAcquired {
        hold_s: opts.hold.hold_s,
    })?;
    let report = ctx
        .host
        .commit(&prepared)
        .await
        .map_err(|e| with_env(e, env))?;
    steps.emit(ApplyEvent::Committed {
        moved: report.moved.len(),
        deleted: report.deleted.len(),
    })?;
    if report.stale {
        return Err(CoreError::Internal(
            "apply stopped (stale_pre_image): a live file differs from the plan's pre-image; \
             nothing was moved"
                .to_string(),
        ));
    }
    if report.failure.is_some() {
        // The failure names an entry by index and never its content; the
        // restore puts back every touched path from the snapshot.
        return roll_back(
            ctx,
            opts,
            &scope,
            Some(&lock),
            RollbackCause::WriteFailed,
            steps,
            Vec::new(),
        )
        .await;
    }

    // Only a lock still held, with the margin left, may be followed by a scan.
    let verdict = lock.check(ctx.deploy, LOCK_MARGIN_S, ctx.clock).await?;
    let held = matches!(verdict, LockVerdict::Held { .. });
    steps.emit(ApplyEvent::LockVerdict { held })?;
    if !held {
        return Err(CoreError::Internal(
            "apply stopped (lock_expired): the scan-lock was not held with the margin left after \
             the commit; the files moved, no scan ran and they were not restored"
                .to_string(),
        ));
    }
    // C11: a tag provider and its definitions load in one scan.
    match scan_and_wait(ctx.deploy, ScanKind::Config, opts.poll.clone()).await? {
        ScanOutcome::Complete(_) => steps.emit(ApplyEvent::ScanDone)?,
        ScanOutcome::NotComplete { .. } => {
            return Err(CoreError::Internal(
                "apply stopped (scan_incomplete): the config scan did not complete in time; the \
                 files moved and were not restored"
                    .to_string(),
            ));
        }
    }

    let verify = deploy_verify(
        declared,
        &opts.project,
        ctx.deploy,
        ctx.gateway,
        ctx.reader,
        &opts.verify,
    )
    .await?;
    steps.emit(ApplyEvent::VerifyDone { ok: verify.ok })?;
    if !verify.ok {
        let failed: Vec<String> = verify
            .checks
            .iter()
            .filter(|c| c.status == CheckStatus::Fail)
            .map(|c| format!("{:?}", c.id))
            .collect();
        return Err(CoreError::Internal(format!(
            "apply stopped (verify_failed): the post-apply verify failed ({}); the files moved \
             and were not restored",
            failed.join(", ")
        )));
    }

    // Green: record what is live now, finalize, keep the snapshot locally.
    let post = ctx.host.fingerprint(&spec.touched_paths()).await?;
    let post_fingerprint = fingerprint(&post);
    let mut record = spec.record.clone();
    record.state = RecordState::Applied;
    record.post_fingerprint = Some(post_fingerprint.clone());
    ctx.host.finalize(&record).await?;
    steps.emit(ApplyEvent::Finalized)?;

    let mut warnings = verify.warnings;
    if let Err(e) = ctx.host.fetch_snapshot(run_id, run_dir.path()).await {
        warnings.push(format!("snapshot_download_failed ({})", e.code()));
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
    })
}

/// What an abort needs to know about the run that is being undone.
struct RunScope<'a> {
    plan: &'a DeployPlanResult,
    run_id: &'a RunId,
    run_dir: &'a RunDir,
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
        ctx,
        &RestoreInput {
            run_id: scope.run_id,
            run_dir: scope.run_dir,
            record: scope.record,
            held,
            cause,
        },
        opts,
        steps,
    )
    .await?;
    warnings.extend(restored.warnings);
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
    /// Why the restore runs.
    pub cause: RollbackCause,
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
    ctx: &ApplyContext<'_>,
    input: &RestoreInput<'_>,
    opts: &ApplyOptions,
    steps: &mut Steps<'_>,
) -> Result<RestoreReport, CoreError> {
    let record = input.record;
    match restore_steps(ctx, input, opts, steps).await {
        Ok(report) => Ok(report),
        Err(cause) => Err(CoreError::RollbackFailed(Box::new(RollbackFailure {
            run_id: record.run_id.clone(),
            run_dir: Some(input.run_dir.path().to_string_lossy().into_owned()),
            journal: journal_location(
                LedgerRoot::for_node(&ctx.declared.target.nodes.master),
                &record.gateway,
                &record.mode,
            ),
            cause,
        }))),
    }
}

/// The steps of [`restore_snapshot`]; an `Err` is a value-free reason.
async fn restore_steps(
    ctx: &ApplyContext<'_>,
    input: &RestoreInput<'_>,
    opts: &ApplyOptions,
    steps: &mut Steps<'_>,
) -> Result<RestoreReport, String> {
    // A gateway error is named by its code only; a host error already names
    // entries by index and never their content.
    let gw = |step: &'static str| move |e: CoreError| format!("{step}: {}", e.code());
    let host = |step: &'static str| move |e: CoreError| format!("{step}: {e}");
    let event = |e: CoreError| format!("events: {}", e.code());

    let record = input.record;
    let deploy = ctx.deploy;
    steps
        .emit(ApplyEvent::RestoreStarted { cause: input.cause })
        .map_err(event)?;

    // The lock.
    let need_s = opts.hold.estimate_commit_s(record.touched.len()).ceil() as i64 + LOCK_MARGIN_S;
    let reuse = match input.held {
        Some(lock) => match lock
            .check_gateway(deploy, need_s)
            .await
            .map_err(gw("checking the scan-lock"))?
        {
            LockVerdict::Held { .. } => true,
            LockVerdict::Expiring { .. } => {
                lock.wait_for_lapse(deploy, opts.poll.interval)
                    .await
                    .map_err(gw("waiting for the scan-lock to lapse"))?;
                false
            }
            LockVerdict::NotHeld => false,
        },
        None => false,
    };
    if !reuse {
        HeldLock::acquire(deploy, ScanKind::Config, opts.hold.hold_s, ctx.clock)
            .await
            .map_err(gw("taking the scan-lock"))?;
        steps
            .emit(ApplyEvent::LockAcquired {
                hold_s: opts.hold.hold_s,
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
    match scan_and_wait(deploy, ScanKind::Config, opts.poll.clone())
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
    match RunId::parse(&record.run_id) {
        Ok(restored_run) => {
            if let Err(e) = ctx
                .host
                .fetch_snapshot(&restored_run, input.run_dir.path())
                .await
            {
                warnings.push(format!("snapshot_download_failed ({})", e.code()));
            }
        }
        Err(e) => warnings.push(format!("snapshot_download_failed ({})", e.code())),
    }
    Ok(RestoreReport {
        restored: report.moved.len(),
        removed: report.deleted.len(),
        fresh_lock: !reuse,
        warnings,
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
