//! The backup phase of an apply (Phase 24, DPAIR-01, DPAIR-02).
//!
//! When the target has a backup node, an apply ends by proving that the
//! backup holds what the master now holds. The proof is the backup's own
//! answers: its `lastSyncTimestamp` moved past a sample taken before the
//! first write, it sees its peer, and its mode, REST view and files equal the
//! render. A write to the master's REST API that was accepted proves nothing
//! about the backup (Pitfall 9), and neither does the resync's 200.
//!
//! The phase runs after the master is finalized, the restart report is read
//! and the snapshot is downloaded, outside the scan-lock (the scan already
//! released it) and without spending hold budget (D-05). It reads the backup
//! through [`BackupNode`], which cannot write. Its only write is the single
//! resync request to the master (D-07): when the first budget ends without
//! convergence the request is sent once, whatever its answer, and a second
//! budget runs; the sync-exhausted signal (D-10, K6 a) ends the wait at once,
//! sends the same one request and goes straight to the final evaluation.
//!
//! Each look at the backup reads only its redundancy status (and its log for
//! the sync-exhausted line). The content checks run when the signals pass and
//! once more as the final evaluation that names the reason (research Pattern
//! 2, D-06). The phase never returns an error: a read that cannot be answered
//! is an `unreachable` check, and an event log that cannot be written is the
//! `events_log_unwritable` warning.
//!
//! The phase never restores the master: the deploy is live and verified, and
//! only the backup is unproven (D-08, DPAIR-02).

use std::collections::{BTreeMap, BTreeSet};

use super::{ApplyContext, ApplyEvent, ApplyOptions, EVENTS_UNWRITABLE, Steps, WriteSet};
use crate::deploy::layout::RelPath;
use crate::deploy::pair::{
    BackupCheck, BackupCheckId, BackupNode, BackupPolicy, BackupReport, BackupSignals,
    PairWaitStep, ResyncRecord, TickOutcome, UnreadableUnits, compare_files, effective_files,
    exempt_unreadable, files_check, files_skipped_check, mode_backup_check, read_back_backup_check,
    report, unreachable_check, unreadable_units, unreadable_warnings, wait_step,
};
use crate::deploy::transport::{FileBlob, Scope};
use crate::deploy::verify::Expected;

/// What the backup phase compares the backup against, taken just before the
/// commit (WR-02, WR-01).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Baseline {
    /// The backup's `lastSyncTimestamp` once the lock was held; `None` is
    /// never synced.
    pub(crate) last_sync: Option<i64>,
    /// The host's epoch-ms at that same moment: a sync-exhausted log line
    /// counts only when it was logged at or after this.
    pub(crate) log_since_ms: i64,
}

/// What the checks of one look at the backup found.
struct Content {
    checks: Vec<BackupCheck>,
    files_checked: usize,
}

/// One run of the backup phase.
struct Phase<'a, 'c, 's> {
    ctx: &'a ApplyContext<'c>,
    backup: &'a BackupNode<'c>,
    steps: &'a mut Steps<'s>,
    warnings: &'a mut Vec<String>,
    set: &'a WriteSet,
    scope: Scope,
    /// The master's live files, read once on the first file comparison.
    master_files: Option<BTreeMap<RelPath, FileBlob>>,
    /// The master's unreadable units from that same read (quick-261029-ms3).
    master_unreadable: UnreadableUnits,
    expected: Expected,
    policy: BackupPolicy,
    before: Option<i64>,
    /// See [`Baseline::log_since_ms`].
    log_since_ms: i64,
    /// The one resync of this run, once it was sent. Set once and never
    /// replaced: it is also what keeps a second request from being sent.
    resync: Option<ResyncRecord>,
}

/// Wait for the backup to hold what the master holds, and say how it went.
///
/// `baseline` is the backup's `lastSyncTimestamp` sampled after the lock was
/// taken and before the first write (WR-02; `None` is never synced) and the
/// host's epoch-ms at that same moment (WR-01). The returned report is the V2
/// `data.backup`.
pub(crate) async fn converge_backup(
    ctx: &ApplyContext<'_>,
    backup: &BackupNode<'_>,
    opts: &ApplyOptions,
    set: &WriteSet,
    baseline: Baseline,
    steps: &mut Steps<'_>,
    warnings: &mut Vec<String>,
) -> BackupReport {
    let declared = ctx.declared;
    let Baseline {
        last_sync: before,
        log_since_ms,
    } = baseline;
    let mut phase = Phase {
        ctx,
        backup,
        steps,
        warnings,
        set,
        scope: Scope::from_manifest(
            &declared.manifest,
            &declared.target.mode,
            &declared.excluded_tag_providers(),
        ),
        master_files: None,
        master_unreadable: UnreadableUnits::new(),
        expected: Expected::from_render(
            &declared.target.mode,
            &declared.outcome,
            &declared.manifest,
        ),
        policy: opts.backup,
        before,
        log_since_ms,
        resync: None,
    };
    let budget_s = phase.policy.budget.as_secs();
    let started = ctx.clock.now();
    phase.emit(ApplyEvent::BackupWaitStarted { budget_s });

    let (mut last, mut step) = phase.wait_budget().await;
    if step != PairWaitStep::Converged {
        // The first budget passed, or the gateway gave up syncing: the one
        // resync goes to the master (D-07), whatever the answer.
        phase.send_resync_once().await;
        if step == PairWaitStep::TimedOut {
            // A timeout waits a second budget. The sync-exhausted signal does
            // not (K6 a): nothing in the log changes by waiting.
            (last, step) = phase.wait_budget().await;
        }
        if step != PairWaitStep::Converged {
            // One final evaluation reads everything it can and names the
            // reason.
            last = phase.tick(true).await;
        }
    }

    let waited = ctx.clock.now().saturating_duration_since(started);
    let resync = phase.resync.take();
    let report = report(&last, before, budget_s, waited.as_secs(), resync);
    phase.emit(ApplyEvent::BackupVerdict {
        converged: report.converged,
        reason: report.reason.map(|reason| reason.as_str()),
    });
    report
}

impl Phase<'_, '_, '_> {
    /// Tell the run's events; a log that cannot be written is a warning.
    fn emit(&mut self, event: ApplyEvent) {
        if self.steps.emit(event).is_err()
            && !self
                .warnings
                .iter()
                .any(|warning| warning == EVENTS_UNWRITABLE)
        {
            self.warnings.push(EVENTS_UNWRITABLE.to_string());
        }
    }

    /// Ask the master to resynchronise its backup, once per run (D-07). The
    /// record holds the status of the POST and nothing more: a 200 is a
    /// request accepted, never proof that the backup caught up. A refusal or
    /// a transport error is recorded as it is and never sent again. The
    /// request goes to the MASTER's session; the backup is only read.
    async fn send_resync_once(&mut self) {
        if self.resync.is_some() {
            return;
        }
        let record = match self.ctx.deploy.redundancy_resync().await {
            Ok(()) => ResyncRecord {
                ok: true,
                code: None,
            },
            Err(error) => ResyncRecord {
                ok: false,
                code: Some(error.code().to_string()),
            },
        };
        self.resync = Some(record.clone());
        self.emit(ApplyEvent::BackupResync {
            ok: record.ok,
            code: record.code,
        });
    }

    /// Look at the backup until it converges, the gateway gives up syncing or
    /// the budget passes.
    async fn wait_budget(&mut self) -> (TickOutcome, PairWaitStep) {
        let started = self.ctx.clock.now();
        loop {
            let tick = self.tick(false).await;
            let elapsed = self.ctx.clock.now().saturating_duration_since(started);
            match wait_step(&tick, elapsed, &self.policy) {
                PairWaitStep::Sleep(pause) => tokio::time::sleep(pause).await,
                step => return (tick, step),
            }
        }
    }

    /// One look at the backup. The content checks run when its signals pass,
    /// or whenever its status is readable when `force` is set (the final
    /// evaluation).
    async fn tick(&mut self, force: bool) -> TickOutcome {
        let signals = match self.backup.status().await {
            Ok(status) => Ok(BackupSignals::from_status(self.before, &status)),
            Err(error) => Err(error.code().to_string()),
        };
        let poll = match &signals {
            Ok(s) => ApplyEvent::BackupPoll {
                peer_connected: s.peer_connected,
                sync_pending: s.sync_pending,
                advanced: s.advanced(),
            },
            Err(_) => ApplyEvent::BackupPoll {
                peer_connected: false,
                sync_pending: false,
                advanced: false,
            },
        };
        self.emit(poll);
        // The log read only feeds the early exit; one that cannot be answered
        // is not the gateway giving up.
        let exhausted = match &signals {
            Ok(_) => self
                .backup
                .sync_exhausted(self.log_since_ms)
                .await
                .unwrap_or(false),
            Err(_) => false,
        };
        let content = match &signals {
            Ok(s) if force || (s.linked() && s.advanced()) => Some(self.content().await),
            _ => None,
        };
        TickOutcome {
            signals,
            exhausted,
            files_checked: content.as_ref().map_or(0, |c| c.files_checked),
            content: content.map(|c| c.checks),
            stable_ticks: 0,
        }
    }

    /// The mode, REST read-back and file checks of the backup.
    async fn content(&mut self) -> Content {
        let mut checks = Vec::with_capacity(3);
        checks.push(match self.backup.deployment_mode().await {
            Ok(mode) => mode_backup_check(&self.expected, mode.as_deref()),
            Err(error) => unreachable_check(BackupCheckId::Mode, error.code()),
        });
        checks.push(match self.backup.read_back(&self.expected).await {
            Ok(read_back) => {
                // The warnings of a read-back are the master verify's to
                // report; the backup report has no field for them.
                read_back_backup_check(&self.expected, &read_back, &mut Vec::new()).0
            }
            Err(error) => unreachable_check(BackupCheckId::ReadBack, error.code()),
        });
        let (files, files_checked) = if self.backup.has_files() {
            self.files().await
        } else {
            (files_skipped_check(), 0)
        };
        checks.push(files);
        Content {
            checks,
            files_checked,
        }
    }

    /// The backup's files against the master's live copies (D-03): every
    /// rendered path must be on the backup with the master's bytes, and every
    /// path the apply deleted must be gone from it. Both sides are read as
    /// the gateway reads them (the mode collection over `core`). A read that
    /// cannot be answered fails the check with the node and the error code.
    /// A rendered or deleted unit either node cannot read fails it too, as
    /// `unreadable` (quick-261029-ms3); any other unreadable unit is a
    /// warning, given once per run.
    async fn files(&mut self) -> (BackupCheck, usize) {
        let unreachable = |side: &str, code: &str| {
            (
                unreachable_check(BackupCheckId::Files, &format!("{side} {code}")),
                0,
            )
        };
        if self.master_files.is_none() {
            match self.ctx.reader.read(&self.scope).await {
                Ok(observed) => {
                    self.master_unreadable = unreadable_units(&observed);
                    self.master_files = Some(effective_files(observed));
                }
                Err(error) => return unreachable("master", error.code()),
            }
        }
        let (backup_files, backup_unreadable) = match self.backup.read_files(&self.scope).await {
            Some(Ok(observed)) => {
                let unreadable = unreadable_units(&observed);
                (effective_files(observed), unreadable)
            }
            Some(Err(error)) => return unreachable("backup", error.code()),
            // `has_files` was checked by the caller.
            None => return (files_skipped_check(), 0),
        };
        let Some(master_files) = self.master_files.as_ref() else {
            return (files_skipped_check(), 0);
        };
        let present: BTreeSet<RelPath> = self
            .ctx
            .declared
            .outcome
            .tree
            .files
            .keys()
            .cloned()
            .collect();
        // A path the apply deleted that the master really lost: a copy in
        // another collection keeps it live.
        let absent: BTreeSet<RelPath> = self
            .set
            .deletes
            .iter()
            .map(|op| op.path.clone())
            .filter(|path| !master_files.contains_key(path))
            .collect();
        let mut comparison = compare_files(master_files, &backup_files, &present, &absent);
        exempt_unreadable(
            &mut comparison,
            &present,
            &absent,
            &[
                ("master", &self.master_unreadable),
                ("backup", &backup_unreadable),
            ],
        );
        for warning in unreadable_warnings(&comparison) {
            if !self.warnings.contains(&warning) {
                self.warnings.push(warning);
            }
        }
        (files_check(&comparison), comparison.checked)
    }
}
