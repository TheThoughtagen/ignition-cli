//! Manual rollback of a past run (Phase 23, DAPL-08, D-07).
//!
//! `rollback --run <id>` needs the target file and the run id and nothing
//! else: no manifest, no render, no local run directory. The run's pre-images
//! and its record live on the gateway host (`snapshots/<id>/`, 23-03), so an
//! ephemeral runner can undo a run it did not perform. The restore itself is
//! [`restore_snapshot`], the same path an abort and an auto-rollback take.
//!
//! # What is checked, in this order, before anything is written
//!
//! [`check_rollback`] (the same function answers the early
//! `deploy_rollback_prepare` and, again, the run itself, so a change between
//! the two is seen):
//!
//! 1. the run id is a valid [`RunId`] (checked by the caller before any host
//!    contact; the id selects a path on the box);
//! 2. no journal of another run is on the box: such a journal is
//!    `deploy_journal_present` naming its run and the recovery command. The
//!    run's own journal is not a refusal: this is its recovery (23-12, D-09),
//!    which skips the staleness check of step 5 (D-11);
//! 3. the on-box record exists and parses (`RunRecord::parse`, strict) for
//!    this target's gateway and mode; it is at state `applied` (a record at
//!    `applying` without a journal and a record already `rolled_back` are
//!    `deploy_run_invalid`), and its fingerprints agree with its touched set;
//! 4. when the run's local run directory exists, its `run.json` and
//!    `plan.json` agree with the record on run id, gateway, mode,
//!    `plan_sha256` and `render_sha256`; a difference names the field;
//! 5. the live hashes of the touched set equal the record's post-run hashes.
//!    A difference is `deploy_live_changed` naming the paths, unless the
//!    caller discards them (`--discard-live-changes`), in which case they are
//!    recorded in the rollback run's `run.json` and events and the restore
//!    goes ahead. Files outside the touched set never count.
//!
//! # The run
//!
//! A rollback is a run of its own: a fresh run id and run directory
//! (`run.json` with `kind: "rollback"` and the target run id, `plan.json`
//! holding the restore set, `events.ndjson`, `result.json`), the preflights
//! of an apply (`mode_master`, `mode_backup`, `wrong_target`, `write_rest`,
//! `write_host`, `designers`, `hold_budget`), the scan-lock (refused as
//! `scan_lock_held` before any write), [`restore_snapshot`] with the held
//! lock, which scans once and checks the restore by hashes and mode (never
//! `deploy_verify`), and the record marked `rolled_back` with `rolled_back_by`
//! this run's id. The outcome is `restored`.
//!
//! The pending-restart list is read before the lock and after the restore, as
//! an apply does, and reported as `restart_pending` and
//! `restart_pending_new`. Reading is all it does (SF-3).

use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::Path;

use serde::Serialize;
use serde_json::{Value, json};

use super::fingerprint::{LEDGER_KEY, fingerprint};
use super::hold::{Clock, HeldLock, HoldModel};
use super::host::ApplyHost;
use super::journal::{
    Journal, RecoveryAction, journal_location, journal_on_box, journal_present, recovery_action,
};
use super::preflight::{DesignersPolicy, PreflightInput, run_preflights};
use super::record::{RECORD_FILE, RecordState, RunRecord, TouchedEntry};
use super::run_store::{EventLog, RunDir, RunId, run_dir_rel};
use super::{
    ApplyEvent, ApplyObserver, RestoreContext, RestoreInput, Steps, restart_report,
    restore_snapshot,
};
use crate::actions::workspace::rfc3339_now_utc;
use crate::client::scan::ScanKind;
use crate::client::{DeployApi, GatewayApi};
use crate::deploy::component_problem;
use crate::deploy::ledger::LedgerRoot;
use crate::deploy::target::Target;
use crate::deploy::transport::TreeReader;
use crate::error::{APPLY_LIST_SHOWN, CoreError, RollbackFailure};
use crate::poll::PollConfig;

/// The largest local `run.json` or `plan.json` read for the cross-check.
const LOCAL_FILE_MAX_BYTES: u64 = 8_388_608;

/// How a rollback ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RollbackKind {
    /// The run's pre-apply files are back and the gateway was scanned.
    Restored,
    /// The recovered run had not changed anything (D-10): its journal, partial
    /// snapshot and staging were removed, nothing was restored and no scan
    /// was made.
    Cleared,
}

/// What a rollback drives. The target file names the gateway, the mode and
/// the master node; nothing else of the declared repo is read.
pub struct RollbackContext<'a> {
    /// The `--env` value.
    pub env: &'a str,
    /// The repo root (the run directories live below it).
    pub repo: &'a Path,
    /// Repo-relative path of the target file (`targets/<env>.yaml`).
    pub target_file: &'a str,
    /// The parsed target.
    pub target: &'a Target,
    /// The read half of the master's host.
    pub reader: &'a dyn TreeReader,
    /// The write seam of the master's host.
    pub host: &'a dyn ApplyHost,
    /// The gateway REST surface (lock, scan, mode, restart list).
    pub deploy: &'a dyn DeployApi,
    /// The gateway surface the preflights read (identity, Designers).
    pub gateway: &'a dyn GatewayApi,
    /// The backup node's REST surface, when the target has one.
    pub backup_deploy: Option<&'a dyn DeployApi>,
    /// The monotonic clock the lock is judged by.
    pub clock: &'a dyn Clock,
    /// Hears each step.
    pub observer: &'a dyn ApplyObserver,
}

/// What a rollback needs besides its context.
#[derive(Debug, Clone)]
pub struct RollbackOptions {
    /// The run to roll back (`--run`). Not this rollback's own id, which is
    /// generated.
    pub run_id: RunId,
    /// `--discard-live-changes`: restore over files changed since the run.
    pub discard_live_changes: bool,
    /// What to do about open Designer sessions (D-14, D-15).
    pub designers: DesignersPolicy,
    /// The hold the lock is requested with.
    pub hold: HoldModel,
    /// How the scan wait polls.
    pub poll: PollConfig,
}

/// What the checks found: the record to restore from and the touched paths
/// that changed since the run (empty unless discarding).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackChecked {
    /// The on-box record of the run: at state `applied` for a rollback, at
    /// any state for the recovery of an interrupted run, and `None` only for
    /// an interrupted run whose record was never written or is lost.
    pub record: Option<RunRecord>,
    /// Touched paths whose live hash is not the record's post-run hash,
    /// byte-sorted. Only ever non-empty when the caller discards them.
    pub live_changed: Vec<String>,
    /// Set when the run's own journal is on the box: this is a recovery, not
    /// a rollback of a completed run.
    pub interrupted: Option<InterruptedRun>,
}

/// What a recovery found on the box (23-12, D-09 to D-11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterruptedRun {
    /// The run's journal.
    pub journal: Journal,
    /// The live hash of each path in the journal's touched list when the
    /// recovery looked (`None` when absent): the torn state, recorded in the
    /// recovery run's events (D-11).
    pub live: BTreeMap<String, Option<String>>,
    /// What recovery will do with it (D-10).
    pub action: RecoveryAction,
}

/// How a manual rollback ended and what it did.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RollbackOutcome {
    /// `restored`.
    #[serde(rename = "outcome")]
    pub kind: RollbackKind,
    /// This rollback's own run id.
    pub run_id: String,
    /// The run that was rolled back.
    pub target_run_id: String,
    /// This rollback's run directory.
    pub run_dir: String,
    /// Collection paths put back from the snapshot.
    pub restored: Vec<String>,
    /// Collection paths the rolled-back run had created, now removed.
    pub deleted: Vec<String>,
    /// Touched paths that had changed since the run and were overwritten.
    pub discarded_live_changes: Vec<String>,
    /// Non-failing notes of the engine.
    pub warnings: Vec<String>,
    /// The pending restarts after the rollback (`[]` when none or unreadable).
    pub restart_pending: Vec<String>,
    /// The pending restarts the rollback added: those not listed before it.
    pub restart_pending_new: Vec<String>,
    /// SHA-256 of each file copied into the run directory's `snapshot/`.
    pub snapshot_sha256: BTreeMap<String, String>,
}

/// The collection paths a restore of `record` puts back, and those it
/// removes (the run created them). The ledger is not listed.
pub fn restore_set(record: &RunRecord) -> (Vec<String>, Vec<String>) {
    let mut restored = Vec::new();
    let mut deleted = Vec::new();
    for entry in &record.touched {
        if entry.path == LEDGER_KEY {
            continue;
        }
        if entry.pre_sha256.is_some() {
            restored.push(entry.path.clone());
        } else {
            deleted.push(entry.path.clone());
        }
    }
    (restored, deleted)
}

fn invalid(run_id: &str, problem: impl Into<String>) -> CoreError {
    CoreError::DeployRunInvalid {
        run_id: run_id.to_string(),
        problem: problem.into(),
    }
}

/// Run the checks of the module doc against the host.
///
/// Reads the host and the local run directory and writes nothing. `env` is
/// named in a journal refusal (the recovery command needs it).
pub async fn check_rollback(
    env: &str,
    repo: &Path,
    target: &Target,
    reader: &dyn TreeReader,
    host: &dyn ApplyHost,
    run_id: &RunId,
    discard_live_changes: bool,
) -> Result<RollbackChecked, CoreError> {
    let (gateway, mode) = (target.gateway.as_str(), target.mode.as_str());
    let id = run_id.as_str();

    // 2. A journal on the box means a run is unfinished: only the rollback of
    // that very run goes on, as its recovery.
    let master = &target.nodes.master;
    if let Some(journal) = journal_on_box(reader, master, gateway, mode).await? {
        if journal.run_id != id {
            return Err(journal_present(&journal, env, master, gateway, mode));
        }
        return check_interrupted(repo, target, host, run_id, journal).await;
    }

    // 3. The record.
    let Some(record) = read_run_record(host, run_id).await? else {
        return Err(invalid(
            id,
            format!("the host has no {RECORD_FILE} for this run (snapshots/{id}/{RECORD_FILE})"),
        ));
    };
    validate_record(&record, target, run_id)?;

    // 4. The local run directory, when there is one.
    cross_check_local(repo, &record)?;

    // 5. Live state against the record's post-run hashes.
    let touched: Vec<String> = record.touched.iter().map(|e| e.path.clone()).collect();
    let live = host.fingerprint(&touched).await?;
    let live_changed: Vec<String> = record
        .post_map()
        .into_iter()
        .filter(|(path, hash)| live.get(path) != Some(hash))
        .map(|(path, _)| path)
        .collect();
    if !live_changed.is_empty() && !discard_live_changes {
        return Err(CoreError::DeployLiveChanged {
            run_id: id.to_string(),
            paths: live_changed
                .iter()
                .take(APPLY_LIST_SHOWN)
                .cloned()
                .collect(),
            count: live_changed.len(),
        });
    }
    Ok(RollbackChecked {
        record: Some(record),
        live_changed,
        interrupted: None,
    })
}

/// The on-box record of `run_id`; `None` when the host has none. A record too
/// damaged to name its run is still this run's problem.
async fn read_run_record(
    host: &dyn ApplyHost,
    run_id: &RunId,
) -> Result<Option<RunRecord>, CoreError> {
    host.read_record(run_id).await.map_err(|e| match e {
        CoreError::DeployRunInvalid { problem, .. } => invalid(run_id.as_str(), problem),
        other => other,
    })
}

/// The recovery of the run whose own journal is on the box (D-09 to D-11).
///
/// The tree may be partial, so the post-apply staleness check is skipped:
/// there is no post-apply fingerprint to compare with and nothing is refused
/// for live changes. The record may be at any state, or absent (Pitfall 14);
/// when it is there it must agree with the journal. Recovery never depends on
/// the record's post part.
async fn check_interrupted(
    repo: &Path,
    target: &Target,
    host: &dyn ApplyHost,
    run_id: &RunId,
    journal: Journal,
) -> Result<RollbackChecked, CoreError> {
    let id = run_id.as_str();
    let record = read_run_record(host, run_id).await?;
    if let Some(record) = &record {
        validate_identity(record, target, run_id)?;
        validate_pre(record)?;
        if journal.pre_fingerprint != record.pre_fingerprint
            || journal.touched
                != record
                    .touched
                    .iter()
                    .map(|e| e.path.clone())
                    .collect::<Vec<_>>()
        {
            return Err(invalid(
                id,
                "the journal and the record do not describe the same run",
            ));
        }
        cross_check_local(repo, record)?;
    }
    let live = host.fingerprint(&journal.touched).await?;
    let action = recovery_action(&journal, &live);
    Ok(RollbackChecked {
        record,
        live_changed: Vec::new(),
        interrupted: Some(InterruptedRun {
            journal,
            live,
            action,
        }),
    })
}

/// The record is for this run, this gateway and this mode.
fn validate_identity(record: &RunRecord, target: &Target, run_id: &RunId) -> Result<(), CoreError> {
    let id = run_id.as_str();
    if record.run_id != id {
        return Err(invalid(id, "the record names a different run"));
    }
    if record.gateway != target.gateway {
        return Err(invalid(
            id,
            "the record is for another gateway than the target's",
        ));
    }
    if record.mode != target.mode {
        return Err(invalid(
            id,
            "the record is for another mode than the target's",
        ));
    }
    Ok(())
}

/// The record's pre-run fingerprint is the fingerprint of its pre-images.
fn validate_pre(record: &RunRecord) -> Result<(), CoreError> {
    if record.pre_fingerprint != fingerprint(&record.pre_map()) {
        return Err(invalid(
            &record.run_id,
            "the record's pre_fingerprint is not the fingerprint of its touched set",
        ));
    }
    Ok(())
}

/// The record is for this run, this gateway and mode, is at state `applied`
/// and agrees with itself. Names the problem and never a content.
fn validate_record(record: &RunRecord, target: &Target, run_id: &RunId) -> Result<(), CoreError> {
    let id = run_id.as_str();
    validate_identity(record, target, run_id)?;
    match record.state {
        RecordState::Applied => {}
        RecordState::Applying => {
            return Err(invalid(
                id,
                "the run did not finish and no journal is on the box, so there is nothing \
                 consistent to restore from; restore from the .gwbk backup or the \
                 run-directory snapshot",
            ));
        }
        RecordState::RolledBack => {
            let by = record.rolled_back_by.as_deref().unwrap_or("an unknown run");
            return Err(invalid(id, format!("already rolled back by {by}")));
        }
    }
    validate_pre(record)?;
    if record.post_fingerprint.as_deref() != Some(fingerprint(&record.post_map()).as_str()) {
        return Err(invalid(
            id,
            "the record's post_fingerprint is missing or is not the fingerprint of its touched set",
        ));
    }
    Ok(())
}

/// Read one local run-directory file as JSON. `None` when it is absent; a
/// symlink, a non-file, a file over the cap or invalid JSON is
/// `deploy_run_invalid`. A symlink is never followed.
fn read_local_json(
    dir: &Path,
    name: &str,
    rel: &str,
    id: &str,
) -> Result<Option<Value>, CoreError> {
    let path = dir.join(name);
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(invalid(id, format!("{rel}/{name} cannot be inspected"))),
    };
    if !meta.file_type().is_file() {
        return Err(invalid(
            id,
            format!("{rel}/{name} is not a regular file (a link is not followed)"),
        ));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(&path)
        .and_then(|file| file.take(LOCAL_FILE_MAX_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|_| invalid(id, format!("{rel}/{name} cannot be read")))?;
    if bytes.len() as u64 > LOCAL_FILE_MAX_BYTES {
        return Err(invalid(id, format!("{rel}/{name} is over the size cap")));
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| invalid(id, format!("{rel}/{name} is not valid JSON")))
}

/// When the local run directory of the run exists, its `run.json` and
/// `plan.json` must agree with the on-box record. Neither side wins; a
/// difference names the field.
fn cross_check_local(repo: &Path, record: &RunRecord) -> Result<(), CoreError> {
    let id = record.run_id.as_str();
    let run_id = RunId::parse(id)?;
    if component_problem(&record.gateway).is_some() || record.gateway.contains(':') {
        return Err(invalid(
            id,
            "the record's gateway is not a plain directory name",
        ));
    }
    let rel = run_dir_rel(&record.gateway, &run_id);
    let dir = rel
        .split('/')
        .fold(repo.to_path_buf(), |acc, c| acc.join(c));
    match std::fs::symlink_metadata(&dir) {
        Ok(meta) if meta.file_type().is_dir() => {}
        Ok(_) => {
            return Err(invalid(
                id,
                format!("{rel} is not a plain directory (a link is not followed)"),
            ));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(invalid(id, format!("{rel} cannot be inspected"))),
    }

    let on_box: [(&str, &str); 4] = [
        ("gateway", &record.gateway),
        ("mode", &record.mode),
        ("plan_sha256", &record.plan_sha256),
        ("render_sha256", &record.render_sha256),
    ];
    let compare = |name: &str, with_run_id: bool| -> Result<(), CoreError> {
        let Some(doc) = read_local_json(&dir, name, &rel, id)? else {
            return Ok(());
        };
        if name == "run.json" && doc.get("kind").and_then(Value::as_str) == Some("rollback") {
            return Err(invalid(
                id,
                format!("{rel}/{name} is the run directory of a rollback, not of a deployment"),
            ));
        }
        let mut fields: Vec<(&str, &str)> = on_box.to_vec();
        if with_run_id {
            fields.insert(0, ("run_id", id));
        }
        for (field, expected) in fields {
            if doc.get(field).and_then(Value::as_str) != Some(expected) {
                return Err(invalid(
                    id,
                    format!("{rel}/{name} field `{field}` differs from the on-box record"),
                ));
            }
        }
        Ok(())
    };
    compare("run.json", true)?;
    compare("plan.json", false)
}

/// Roll `opts.run_id` back (see the module doc). Re-runs [`check_rollback`]
/// first, so a journal, a changed file or a second rollback that appeared
/// after `deploy_rollback_prepare` is still refused.
///
/// Once the rollback's run directory exists, every way out passes through
/// the one exit point that writes `result.json` once.
pub async fn run_rollback(
    ctx: &RollbackContext<'_>,
    opts: &RollbackOptions,
) -> Result<RollbackOutcome, CoreError> {
    let checked = check_rollback(
        ctx.env,
        ctx.repo,
        ctx.target,
        ctx.reader,
        ctx.host,
        &opts.run_id,
        opts.discard_live_changes,
    )
    .await?;
    let (run_id, run_dir) = RunDir::create_fresh(ctx.repo, &ctx.target.gateway)?;
    let mut steps = Steps::new(None, ctx.observer);
    let result = drive(ctx, opts, &checked, &run_id, &run_dir, &mut steps).await;

    // The one exit point.
    if let Err(e) = &result {
        let _ = steps.emit(ApplyEvent::Failed {
            code: e.code().to_string(),
        });
    }
    let doc = match &result {
        Ok(outcome) => {
            let mut doc = serde_json::to_value(outcome).unwrap_or(Value::Null);
            doc["ok"] = Value::Bool(true);
            doc["kind"] = Value::String("rollback".to_string());
            doc
        }
        Err(e) => json!({
            "ok": false,
            "kind": "rollback",
            "run_id": run_id.as_str(),
            "target_run_id": opts.run_id.as_str(),
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

async fn drive(
    ctx: &RollbackContext<'_>,
    opts: &RollbackOptions,
    checked: &RollbackChecked,
    run_id: &RunId,
    run_dir: &RunDir,
    steps: &mut Steps<'_>,
) -> Result<RollbackOutcome, CoreError> {
    let target = ctx.target;

    let started_at = rfc3339_now_utc();
    steps.log = Some(EventLog::open(run_dir)?);
    // The counts of the record; a recovery whose record is lost does not know
    // them yet and reports zero until the snapshot has been read.
    let (writes, deletes) = checked.record.as_ref().map(restore_set).unwrap_or_default();
    steps.emit(ApplyEvent::RunStarted {
        run_id: run_id.as_str().to_string(),
        writes: writes.len(),
        deletes: deletes.len(),
    })?;
    let interrupted = checked.interrupted.as_ref();
    if let Some(found) = interrupted {
        // The torn state the recovery found, by path and hash (D-11).
        steps.emit(ApplyEvent::InterruptedRun {
            phase: found.journal.phase.as_str(),
            action: found.action.as_str(),
            hashes: found.live.clone(),
        })?;
        if found.action == RecoveryAction::ClearWithoutRestore {
            return clear_run(ctx, opts, checked, run_id, run_dir, steps, &started_at).await;
        }
    }

    // What the restore puts back: the record when there is one; otherwise (a
    // recovery whose record is lost) the snapshot itself, checked against the
    // journal's pre-run fingerprint. A recovery reads the snapshot before it
    // restores anything, so a lost or damaged one is `rollback_failed` with
    // nothing half restored (D-12).
    let record = match (interrupted, &checked.record) {
        (Some(found), record) => {
            snapshot_record(ctx, opts, run_dir, found, record.as_ref()).await?
        }
        (None, Some(record)) => record.clone(),
        (None, None) => {
            return Err(CoreError::Internal(
                "a rollback of a finished run has no record".to_string(),
            ));
        }
    };
    let record = &record;
    let (restored, deleted) = restore_set(record);

    let touched_paths: Vec<String> = record.touched.iter().map(|e| e.path.clone()).collect();
    let preflights = run_preflights(
        &PreflightInput {
            target,
            master: ctx.deploy,
            backup: ctx.backup_deploy,
            gateway: ctx.gateway,
            reader: ctx.reader,
            host: ctx.host,
            touched_files: record.touched.len(),
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
            "kind": "rollback",
            "run_id": run_id.as_str(),
            "target_run_id": opts.run_id.as_str(),
            "env": ctx.env,
            "target_file": ctx.target_file,
            "gateway": target.gateway,
            "mode": target.mode,
            "node": "master",
            "started_at": started_at,
            "run_dir_git": run_dir.git_status().as_record(),
            "render_sha256": record.render_sha256,
            "plan_sha256": record.plan_sha256,
            "hold_s": opts.hold.hold_s,
            "discard_live_changes": opts.discard_live_changes,
            "discarded_live_changes": checked.live_changed,
            "interrupted_phase": interrupted.map(|i| i.journal.phase.as_str()),
            "recovery": interrupted.map(|i| i.action.as_str()),
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
    run_dir.write_json(
        "plan.json",
        &json!({
            "schema_version": 1,
            "kind": "rollback",
            "target_run_id": opts.run_id.as_str(),
            "gateway": target.gateway,
            "mode": target.mode,
            "plan_sha256": record.plan_sha256,
            "render_sha256": record.render_sha256,
            "restore": restored,
            "delete": deleted,
            "ledger": record
                .touched
                .iter()
                .find(|e| e.path == LEDGER_KEY)
                .map(|e| if e.pre_sha256.is_some() { "restore" } else { "delete" }),
        }),
    )?;
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
    if !checked.live_changed.is_empty() {
        steps.emit(ApplyEvent::LiveChangesDiscarded {
            count: checked.live_changed.len(),
            paths: checked
                .live_changed
                .iter()
                .take(APPLY_LIST_SHOWN)
                .cloned()
                .collect(),
        })?;
    }

    // What is already waiting for a restart, so the rollback's own additions
    // can be told apart afterwards. Unreadable is `None`.
    let restart_before = ctx.deploy.restart_pending().await.ok();

    // The lock is taken here, so that another actor's hold is the typed
    // `scan_lock_held` before anything is written; `restore_snapshot` then
    // reuses it.
    let lock = HeldLock::acquire(ctx.deploy, ScanKind::Config, opts.hold.hold_s, ctx.clock).await?;
    steps.emit(ApplyEvent::LockAcquired {
        hold_s: opts.hold.hold_s,
    })?;
    let report = restore_snapshot(
        &RestoreContext {
            master: &target.nodes.master,
            host: ctx.host,
            deploy: ctx.deploy,
            clock: ctx.clock,
            hold: &opts.hold,
            poll: &opts.poll,
        },
        &RestoreInput {
            run_id,
            run_dir,
            record,
            held: Some(&lock),
            cause: None,
        },
        steps,
    )
    .await?;

    let mut warnings = report.warnings;
    let (restart_pending, restart_pending_new) =
        restart_report(ctx.deploy, restart_before.as_deref(), &mut warnings).await;
    Ok(RollbackOutcome {
        kind: RollbackKind::Restored,
        run_id: run_id.as_str().to_string(),
        target_run_id: opts.run_id.as_str().to_string(),
        run_dir: run_dir.path().to_string_lossy().into_owned(),
        restored,
        deleted,
        discarded_live_changes: checked.live_changed.clone(),
        warnings,
        restart_pending,
        restart_pending_new,
        snapshot_sha256: report.snapshot_sha256,
    })
}

/// The zero hash: what a recovery writes for the hashes of a plan and
/// a render it never saw (the record was lost).
const UNKNOWN_SHA256: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// How the snapshot spells `path`: the ledger is `ledger.json`, a collection
/// file is `files/<path>`.
fn snapshot_key(path: &str) -> String {
    if path == LEDGER_KEY {
        LEDGER_KEY.to_string()
    } else {
        format!("files/{path}")
    }
}

/// `a, b and N more`, at most [`APPLY_LIST_SHOWN`] named.
fn named(paths: &[String]) -> String {
    let shown = paths.len().min(APPLY_LIST_SHOWN);
    let mut out = paths[..shown].join(", ");
    if paths.len() > shown {
        out.push_str(&format!(" and {} more", paths.len() - shown));
    }
    out
}

/// Read the interrupted run's snapshot from the host and return the record to
/// restore from.
///
/// The snapshot is copied into a scratch directory below this run's directory
/// (private, removed straight away) and every file hashed. With a record, each
/// pre-image the record lists must be there with its hash. Without one, the
/// pre-run state is what the snapshot holds, and it must hash to the journal's
/// `pre_fingerprint`, which is how an incomplete snapshot is told from a
/// created file. A snapshot that is missing, incomplete or fails its hashes is
/// `rollback_failed` (D-12): the journal stays and nothing is restored.
async fn snapshot_record(
    ctx: &RollbackContext<'_>,
    opts: &RollbackOptions,
    run_dir: &RunDir,
    found: &InterruptedRun,
    record: Option<&RunRecord>,
) -> Result<RunRecord, CoreError> {
    let target = ctx.target;
    let (gateway, mode) = (target.gateway.as_str(), target.mode.as_str());
    let fail = |cause: String| {
        CoreError::RollbackFailed(Box::new(RollbackFailure {
            run_id: opts.run_id.as_str().to_string(),
            run_dir: Some(run_dir.path().to_string_lossy().into_owned()),
            journal: journal_location(LedgerRoot::for_node(&target.nodes.master), gateway, mode),
            cause,
        }))
    };

    let scratch = run_dir.path().join("inspect");
    std::fs::create_dir(&scratch)
        .map_err(|e| fail(format!("cannot read the snapshot ({:?})", e.kind())))?;
    let fetched = ctx.host.fetch_snapshot(&opts.run_id, &scratch).await;
    // The copies were only for hashing; the run directory keeps the one the
    // restore itself downloads.
    let _ = std::fs::remove_dir_all(&scratch);
    let hashes = fetched.map_err(|e| fail(format!("the snapshot cannot be read: {e}")))?;

    if let Some(record) = record {
        let bad: Vec<String> = record
            .touched
            .iter()
            .filter_map(|entry| {
                let pre = entry.pre_sha256.as_deref()?;
                (hashes.get(&snapshot_key(&entry.path)).map(String::as_str) != Some(pre))
                    .then(|| entry.path.clone())
            })
            .collect();
        if !bad.is_empty() {
            return Err(fail(format!(
                "the snapshot is missing or damaged for {} path(s): {}",
                bad.len(),
                named(&bad)
            )));
        }
        return Ok(record.clone());
    }

    let pre: BTreeMap<String, Option<String>> = found
        .journal
        .touched
        .iter()
        .map(|path| (path.clone(), hashes.get(&snapshot_key(path)).cloned()))
        .collect();
    if fingerprint(&pre) != found.journal.pre_fingerprint {
        return Err(fail(
            "the run's record is missing and the snapshot is incomplete: its files do not \
             match the journal's pre-run fingerprint"
                .to_string(),
        ));
    }
    Ok(RunRecord {
        schema_version: 1,
        run_id: found.journal.run_id.clone(),
        gateway: found.journal.gateway.clone(),
        mode: found.journal.mode.clone(),
        render_sha256: UNKNOWN_SHA256.to_string(),
        plan_sha256: UNKNOWN_SHA256.to_string(),
        state: RecordState::Applying,
        rolled_back_by: None,
        touched: found
            .journal
            .touched
            .iter()
            .map(|path| TouchedEntry {
                path: path.clone(),
                pre_sha256: pre.get(path).cloned().flatten(),
                post_sha256: None,
            })
            .collect(),
        pre_fingerprint: found.journal.pre_fingerprint.clone(),
        post_fingerprint: None,
    })
}

/// D-10: the run died before any move and the touched set is as it was. Drop
/// the journal, the partial snapshot and the staging; no lock, no restore, no
/// scan. The run directory still gets its `run.json` and `plan.json`.
async fn clear_run(
    ctx: &RollbackContext<'_>,
    opts: &RollbackOptions,
    checked: &RollbackChecked,
    run_id: &RunId,
    run_dir: &RunDir,
    steps: &mut Steps<'_>,
    started_at: &str,
) -> Result<RollbackOutcome, CoreError> {
    let target = ctx.target;
    let phase = checked
        .interrupted
        .as_ref()
        .map(|i| i.journal.phase.as_str());
    run_dir.write_json(
        "run.json",
        &json!({
            "schema_version": 1,
            "kind": "rollback",
            "run_id": run_id.as_str(),
            "target_run_id": opts.run_id.as_str(),
            "env": ctx.env,
            "target_file": ctx.target_file,
            "gateway": target.gateway,
            "mode": target.mode,
            "node": "master",
            "started_at": started_at,
            "run_dir_git": run_dir.git_status().as_record(),
            "discard_live_changes": opts.discard_live_changes,
            "discarded_live_changes": checked.live_changed,
            "interrupted_phase": phase,
            "recovery": RecoveryAction::ClearWithoutRestore.as_str(),
        }),
    )?;
    run_dir.write_json(
        "plan.json",
        &json!({
            "schema_version": 1,
            "kind": "rollback",
            "target_run_id": opts.run_id.as_str(),
            "gateway": target.gateway,
            "mode": target.mode,
            "restore": Vec::<String>::new(),
            "delete": Vec::<String>::new(),
            "ledger": Value::Null,
            "recovery": RecoveryAction::ClearWithoutRestore.as_str(),
        }),
    )?;
    ctx.host.discard_interrupted(&opts.run_id).await?;
    steps.emit(ApplyEvent::Cleared)?;
    Ok(RollbackOutcome {
        kind: RollbackKind::Cleared,
        run_id: run_id.as_str().to_string(),
        target_run_id: opts.run_id.as_str().to_string(),
        run_dir: run_dir.path().to_string_lossy().into_owned(),
        restored: Vec::new(),
        deleted: Vec::new(),
        discarded_live_changes: Vec::new(),
        warnings: Vec::new(),
        restart_pending: Vec::new(),
        restart_pending_new: Vec::new(),
        snapshot_sha256: BTreeMap::new(),
    })
}
