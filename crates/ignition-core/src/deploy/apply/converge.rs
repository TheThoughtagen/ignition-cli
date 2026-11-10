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
//!
//! # After a restore (26-13, F1)
//!
//! [`converge_restore`] runs the same wait after [`super::restore_snapshot`]
//! (the automatic rollback of an apply and `ign deploy rollback`): the backup
//! must move past a sample taken just before the restore, be linked, run the
//! target's mode and hold the master's restored files. There is no render, so
//! the REST read-back is not compared (a `warn` check), the files compared are
//! the record's touched set (each path with a pre-image must be on the backup
//! with the master's bytes; each path the run created, which the restore
//! removed, must be gone), and the scope read is the touched set's types and
//! tag providers. It reports; it never restores again and never changes the
//! restore's outcome.

use std::collections::{BTreeMap, BTreeSet};

use super::fingerprint::LEDGER_KEY;
use super::hold::Clock;
use super::record::RunRecord;
use super::{ApplyContext, ApplyEvent, ApplyOptions, EVENTS_UNWRITABLE, Steps, WriteSet};
use crate::client::DeployApi;
use crate::deploy::layout::{RelPath, TAG_TYPES, split_key};
use crate::deploy::pair::{
    BackupCheck, BackupCheckId, BackupNode, BackupPolicy, BackupReport, BackupSignals,
    PairWaitStep, ResyncRecord, TickOutcome, UnreadableUnits, compare_files, effective_files,
    exempt_unreadable, files_check, files_skipped_check, mode_backup_check,
    read_back_backup_check_with, report, unreachable_check, unreadable_units, unreadable_warnings,
    wait_step,
};
use crate::deploy::transport::{FileBlob, Scope, TreeReader};
use crate::deploy::verify::{CheckStatus, Expected, ReadBackBaseline, ReadBackJudge};

/// What the backup phase compares the backup against, taken just before the
/// commit (WR-02, WR-01), and the backup's REST baseline, read before
/// anything was staged (quick 261031-bbs, R1).
#[derive(Debug, Clone)]
pub(crate) struct Baseline {
    /// The backup's `lastSyncTimestamp` once the lock was held; `None` is
    /// never synced.
    pub(crate) last_sync: Option<i64>,
    /// The host's epoch-ms at that same moment: a sync-exhausted log line
    /// counts only when it was logged at or after this.
    pub(crate) log_since_ms: i64,
    /// The backup's own REST effective view of every declared, compared
    /// resource before the master was written: its `read_back` is judged
    /// against it, by the rule the master's post-apply verify uses. `None`
    /// (never the case for a run with a backup) judges it strictly.
    pub(crate) read_back: Option<ReadBackBaseline>,
}

/// What the checks of one look at the backup found.
struct Content {
    checks: Vec<BackupCheck>,
    files_checked: usize,
}

/// How the backup's REST read-back is judged.
enum ReadBack {
    /// Against the expected render, judged on the backup's own baseline
    /// (R1); `None` judges it strictly.
    Judge(Option<ReadBackBaseline>),
    /// Not compared: after a restore there is no render to compare with.
    Skipped,
}

/// One run of the backup phase.
struct Phase<'a, 'c, 's> {
    /// The master's REST surface: the one resync goes here.
    deploy: &'a dyn DeployApi,
    /// The read half of the master's host: its live files.
    reader: &'a dyn TreeReader,
    clock: &'a dyn Clock,
    backup: &'a BackupNode<'c>,
    steps: &'a mut Steps<'s>,
    warnings: &'a mut Vec<String>,
    scope: Scope,
    /// Collection paths that must be on the backup with the master's bytes.
    present: BTreeSet<RelPath>,
    /// Collection paths the run deleted: absent from the backup when the
    /// master really lost them.
    deleted: Vec<RelPath>,
    /// The master's live files, read once on the first file comparison.
    master_files: Option<BTreeMap<RelPath, FileBlob>>,
    /// The master's unreadable units from that same read (quick-261029-ms3).
    master_unreadable: UnreadableUnits,
    expected: Expected,
    /// See [`Baseline::read_back`].
    read_back: ReadBack,
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
        read_back: read_back_baseline,
    } = baseline;
    let phase = Phase {
        deploy: ctx.deploy,
        reader: ctx.reader,
        clock: ctx.clock,
        backup,
        steps,
        warnings,
        scope: Scope::from_manifest(
            &declared.manifest,
            &declared.target.mode,
            &declared.excluded_tag_providers(),
        ),
        present: declared.outcome.tree.files.keys().cloned().collect(),
        deleted: set
            .deletes
            .iter()
            .filter_map(|op| RelPath::new(split_key(op.path.as_str()).1).ok())
            .collect(),
        master_files: None,
        master_unreadable: UnreadableUnits::new(),
        expected: Expected::from_render(
            &declared.target.mode,
            &declared.outcome,
            &declared.manifest,
        ),
        read_back: ReadBack::Judge(read_back_baseline),
        policy: opts.backup,
        before,
        log_since_ms,
        resync: None,
    };
    phase.run().await
}

/// The backup's sample taken just before a restore (26-13, F1).
#[derive(Debug, Clone, Copy)]
pub(crate) struct RestoreBaseline {
    /// The backup's `lastSyncTimestamp`; `None` is never synced or not read.
    pub(crate) last_sync: Option<i64>,
    /// The host's epoch-ms at that moment.
    pub(crate) log_since_ms: i64,
}

/// What a restore's backup wait reads and judges with.
pub(crate) struct RestoreSurfaces<'a, 'c> {
    /// The master's REST surface (the one resync).
    pub(crate) deploy: &'a dyn DeployApi,
    /// The read half of the master's host.
    pub(crate) reader: &'a dyn TreeReader,
    /// The clock the wait is timed by.
    pub(crate) clock: &'a dyn Clock,
    /// The backup, read only.
    pub(crate) backup: &'a BackupNode<'c>,
}

/// Wait for the backup to hold what the master holds after a restore of
/// `record` (module doc, F1). `mode` is the target's mode. Never an error,
/// and never a second restore.
pub(crate) async fn converge_restore(
    surfaces: RestoreSurfaces<'_, '_>,
    record: &RunRecord,
    mode: &str,
    policy: BackupPolicy,
    baseline: RestoreBaseline,
    steps: &mut Steps<'_>,
    warnings: &mut Vec<String>,
) -> BackupReport {
    let mut present = BTreeSet::new();
    let mut deleted = Vec::new();
    let mut types = BTreeSet::new();
    let mut providers = BTreeSet::new();
    for entry in &record.touched {
        if entry.path == LEDGER_KEY {
            continue;
        }
        let Ok(path) = RelPath::new(split_key(&entry.path).1) else {
            continue;
        };
        let mut parts = path.as_str().split('/');
        if let (Some(module), Some(ty)) = (parts.next(), parts.next()) {
            if module == "ignition" && TAG_TYPES.contains(&ty) {
                if let Some(provider) = parts.next() {
                    providers.insert(provider.to_string());
                }
            } else {
                types.insert((module.to_string(), ty.to_string()));
            }
        }
        if entry.pre_sha256.is_some() {
            present.insert(path);
        } else {
            deleted.push(path);
        }
    }
    let phase = Phase {
        deploy: surfaces.deploy,
        reader: surfaces.reader,
        clock: surfaces.clock,
        backup: surfaces.backup,
        steps,
        warnings,
        scope: Scope {
            mode: mode.to_string(),
            types: types.into_iter().collect(),
            tag_providers: providers.into_iter().collect(),
        },
        present,
        deleted,
        master_files: None,
        master_unreadable: UnreadableUnits::new(),
        expected: Expected {
            mode: mode.to_string(),
            resources: Vec::new(),
            not_checked: Vec::new(),
            declared_ids: Vec::new(),
            tag_units: Vec::new(),
            probes: Vec::new(),
        },
        read_back: ReadBack::Skipped,
        policy,
        before: baseline.last_sync,
        log_since_ms: baseline.log_since_ms,
        resync: None,
    };
    phase.run().await
}

/// The `read_back` check after a restore: not compared, a `warn`.
fn read_back_skipped_check() -> BackupCheck {
    BackupCheck {
        id: BackupCheckId::ReadBack,
        status: CheckStatus::Warn,
        detail: "a restore puts back the run's snapshot, not a render, so the backup's REST \
                 read-back was not compared"
            .to_string(),
        findings: Vec::new(),
    }
}

impl Phase<'_, '_, '_> {
    /// The wait, its one resync and its verdict (module doc).
    async fn run(mut self) -> BackupReport {
        let phase = &mut self;
        let before = phase.before;
        let budget_s = phase.policy.budget.as_secs();
        let started = phase.clock.now();
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

        let waited = phase.clock.now().saturating_duration_since(started);
        let resync = phase.resync.take();
        let report = report(&last, before, budget_s, waited.as_secs(), resync);
        phase.emit(ApplyEvent::BackupVerdict {
            converged: report.converged,
            reason: report.reason.map(|reason| reason.as_str()),
        });
        report
    }

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
        let record = match self.deploy.redundancy_resync().await {
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
        let started = self.clock.now();
        loop {
            let tick = self.tick(false).await;
            let elapsed = self.clock.now().saturating_duration_since(started);
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
        checks.push(match &self.read_back {
            ReadBack::Skipped => read_back_skipped_check(),
            ReadBack::Judge(baseline) => match self.backup.read_back(&self.expected).await {
                Ok(read_back) => {
                    // The warnings of a read-back are the master verify's to
                    // report; the backup report has no field for them. A
                    // difference the backup already had before the write is a
                    // warn finding, judged on the backup's own baseline (R1).
                    let judge = match baseline {
                        Some(baseline) => ReadBackJudge::Baseline(baseline),
                        None => ReadBackJudge::Strict,
                    };
                    read_back_backup_check_with(&self.expected, &read_back, &judge, &mut Vec::new())
                        .0
                }
                Err(error) => unreachable_check(BackupCheckId::ReadBack, error.code()),
            },
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
            match self.reader.read(&self.scope).await {
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
        let present = &self.present;
        // A path the apply deleted that the master really lost: a copy in
        // another collection keeps it live. Both sides are the effective
        // view, so a `.core/` delete is compared by its collection path, and
        // a migrated mode path stays live from `core` (quick 261102-cef, P8).
        let absent: BTreeSet<RelPath> = self
            .deleted
            .iter()
            .filter(|path| !master_files.contains_key(*path))
            .cloned()
            .collect();
        let mut comparison = compare_files(master_files, &backup_files, present, &absent);
        exempt_unreadable(
            &mut comparison,
            present,
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
