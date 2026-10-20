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
//! resync request to the master (D-07), added with the unsynced verdict.
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

use super::{ApplyContext, ApplyEvent, ApplyOptions, EVENTS_UNWRITABLE, Steps};
use crate::deploy::pair::{
    BackupCheck, BackupCheckId, BackupNode, BackupPolicy, BackupReport, BackupSignals,
    PairWaitStep, TickOutcome, files_skipped_check, mode_backup_check, read_back_backup_check,
    report, unreachable_check, wait_step,
};
use crate::deploy::verify::Expected;

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
    expected: Expected,
    policy: BackupPolicy,
    before: Option<i64>,
}

/// Wait for the backup to hold what the master holds, and say how it went.
///
/// `before` is the backup's `lastSyncTimestamp` sampled after the preflights
/// and before the first write; `None` is never synced. The returned report is
/// the V2 `data.backup`.
pub(crate) async fn converge_backup(
    ctx: &ApplyContext<'_>,
    backup: &BackupNode<'_>,
    opts: &ApplyOptions,
    before: Option<i64>,
    steps: &mut Steps<'_>,
    warnings: &mut Vec<String>,
) -> BackupReport {
    let declared = ctx.declared;
    let mut phase = Phase {
        ctx,
        backup,
        steps,
        warnings,
        expected: Expected::from_render(
            &declared.target.mode,
            &declared.outcome,
            &declared.manifest,
        ),
        policy: opts.backup,
        before,
    };
    let budget_s = phase.policy.budget.as_secs();
    let started = ctx.clock.now();
    phase.emit(ApplyEvent::BackupWaitStarted { budget_s });

    let (mut last, step) = phase.wait_budget().await;
    if step != PairWaitStep::Converged {
        // The budget passed (or the gateway gave up syncing): one final
        // evaluation reads everything it can and names the reason.
        last = phase.tick(true).await;
    }

    let waited = ctx.clock.now().saturating_duration_since(started);
    let report = report(&last, before, budget_s, waited.as_secs(), None);
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
            Ok(_) => self.backup.sync_exhausted().await.unwrap_or(false),
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
        // The comparison with the master's live files is added with the file
        // transport; until then the check is the skipped one.
        checks.push(files_skipped_check());
        Content {
            checks,
            files_checked: 0,
        }
    }
}
