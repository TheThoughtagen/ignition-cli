//! The backup node of a redundant pair, read-only (Phase 24, DPAIR-01 to
//! DPAIR-03).
//!
//! A redundant pair has one master and one backup. The master takes every
//! write; the backup receives the same configuration through the gateway's
//! own sync, and this module is how the rest of the deploy code looks at it.
//!
//! ## The backup is read, never written (D-01)
//!
//! [`BackupNode`] is the only way to reach the backup. Its fields are private
//! and its methods are reads: [`BackupNode::status`],
//! [`BackupNode::deployment_mode`], [`BackupNode::read_back`],
//! [`BackupNode::read_files`], [`BackupNode::secret_names`] and
//! [`BackupNode::sync_exhausted`]. It holds the backup's read half of a
//! transport (or nothing for a REST-only backup), never a type that can put a
//! file on the backup, so a caller cannot express a write to it. The test
//! `pair_module_never_writes` scans this file for every write the gateway or
//! the transport offers, and `tests/deploy_pair.rs` runs the handle over a
//! deploy surface that panics on a write.
//!
//! ## What the decisions implement
//!
//! - D-03: the backup's files are compared with the master's live files by
//!   hash ([`compare_files`]).
//! - D-06: the backup has converged when its own sync timestamp advanced past
//!   its own pre-apply sample, it is linked to its peer, and its mode, REST
//!   read-back and files match ([`converged`]).
//! - D-09: the report shape ([`BackupReport`]) and the reason a backup did
//!   not converge ([`classify`]).
//! - D-10: the gateway's sync-exhausted state ends the wait ([`wait_step`],
//!   [`is_sync_exhausted`]).
//! - D-13: the secret names a render references exist on each node
//!   ([`check_secret_names`]).
//!
//! Every decision is a pure function over value-free inputs: no clock, no
//! I/O, no gateway text. A status, a log line or a sync error never enters a
//! report; only codes, counts, paths and names do.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;

use super::layout::RelPath;
use super::lint::referenced_secret_names;
use super::node;
use super::transport::{FileBlob, ObservedFiles, Scope, TreeReader};
use super::tree::RenderedTree;
use super::verify::{self, CheckStatus, Expected, Finding, Remote};
use crate::client::deploy_api::SecretNameListing;
use crate::client::logs::{DEFAULT_LOG_LIMIT, LogQuery};
use crate::client::mode::normalize_deployment_mode;
use crate::client::redundancy::RedundancyStatusWire;
use crate::client::{DeployApi, GatewayApi};
use crate::error::{APPLY_LIST_SHOWN, CoreError};

/// Seconds between two reads of the backup while waiting for it to converge
/// (V5).
pub const BACKUP_POLL_S: u64 = 5;

/// Default `--backup-timeout`, in seconds (V5).
pub const BACKUP_TIMEOUT_DEFAULT_S: u64 = 300;

/// How long to wait for the backup, and how often to look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupPolicy {
    /// The most the wait may take.
    pub budget: Duration,
    /// Time between two reads.
    pub interval: Duration,
}

impl Default for BackupPolicy {
    fn default() -> Self {
        BackupPolicy {
            budget: Duration::from_secs(BACKUP_TIMEOUT_DEFAULT_S),
            interval: Duration::from_secs(BACKUP_POLL_S),
        }
    }
}

/// The read-only handle on a pair's backup node.
///
/// Built from the backup's own gateway surface, its own deploy surface and,
/// when the target gives the backup a file transport, the read half of that
/// transport. See the module doc for why no method here writes.
pub struct BackupNode<'a> {
    gateway: &'a dyn GatewayApi,
    deploy: &'a dyn DeployApi,
    reader: Option<&'a dyn TreeReader>,
}

impl<'a> BackupNode<'a> {
    /// A handle over the backup's surfaces. `reader` is `None` for a backup
    /// that declares no file transport (REST only).
    pub fn new(
        gateway: &'a dyn GatewayApi,
        deploy: &'a dyn DeployApi,
        reader: Option<&'a dyn TreeReader>,
    ) -> Self {
        BackupNode {
            gateway,
            deploy,
            reader,
        }
    }

    /// Whether the backup's files can be read (it declares a transport).
    pub fn has_files(&self) -> bool {
        self.reader.is_some()
    }

    /// The backup's own redundancy status.
    pub async fn status(&self) -> Result<RedundancyStatusWire, CoreError> {
        self.gateway.redundancy_status().await
    }

    /// The backup's active deployment mode, normalized (`None` is no mode).
    pub async fn deployment_mode(&self) -> Result<Option<String>, CoreError> {
        Ok(normalize_deployment_mode(
            self.deploy.deployment_mode().await?,
        ))
    }

    /// Each declared, comparable resource read from the backup's REST
    /// effective view (the same read the master's verify makes).
    pub async fn read_back(
        &self,
        expected: &Expected,
    ) -> Result<BTreeMap<String, Remote>, CoreError> {
        crate::actions::deploy::gather_read_back(self.deploy, expected).await
    }

    /// The backup's live files within `scope`; `None` when the backup
    /// declares no file transport (a REST-only backup).
    pub async fn read_files(&self, scope: &Scope) -> Option<Result<ObservedFiles, CoreError>> {
        match self.reader {
            Some(reader) => Some(reader.read(scope).await),
            None => None,
        }
    }

    /// The names a secret provider lists on the backup. Names only: the
    /// listing carries no value.
    pub async fn secret_names(&self, provider: &str) -> Result<SecretNameListing, CoreError> {
        self.deploy.secret_names(provider).await
    }

    /// Whether the backup's log says the gateway gave up syncing at or after
    /// `since_ms` (D-10, K5 d, WR-01). The lines are matched in memory and
    /// dropped; only the answer leaves.
    ///
    /// `since_ms` is the epoch-ms the engine took (see [`wall_clock_ms`]) just
    /// before this run's commit, so a line from a previous deploy or a past
    /// recovery never ends this run's wait. The bound is applied twice: as the
    /// log query's `startTime` and again on each entry's own timestamp, which
    /// is the gateway's clock.
    ///
    /// Clock assumption: `since_ms` comes from the CLI host's clock and the
    /// entries from the gateway's, so they must agree to within the age of the
    /// stale line (seconds to minutes for a real previous run). A gateway
    /// clock that runs behind the host by more than the time the sync takes
    /// to give up can hide a fresh line; that only costs the early exit, and
    /// the wait ends on its budget with the one resync as before.
    pub async fn sync_exhausted(&self, since_ms: i64) -> Result<bool, CoreError> {
        let query = LogQuery {
            start_time: Some(since_ms.max(0)),
            min_level: Some("ERROR".to_string()),
            limit: DEFAULT_LOG_LIMIT,
            ..LogQuery::default()
        };
        let envelope = self.gateway.logs(&query).await?;
        Ok(envelope
            .items
            .iter()
            .any(|entry| entry.timestamp >= since_ms && is_sync_exhausted(&entry.message)))
    }
}

/// The host's wall clock as epoch milliseconds: the anchor the engine hands to
/// [`BackupNode::sync_exhausted`]. `0` when the clock is before 1970.
pub fn wall_clock_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_millis()).ok())
        .unwrap_or(0)
}

/// The text the gateway logs when it stops retrying a sync (TEC-5105): `State
/// sync request failed. Sync operation has failed 5 time(s), which exceeds
/// the max attempt count. No further synchronization attempts will be made.`
/// The state was not reachable on the pair rig (24-LIVE-CAPTURES `## C-P8`),
/// so this matcher is tested against the spike's recorded line only.
const SYNC_EXHAUSTED_TEXT: &str = "exceeds the max attempt count";

/// Whether one log message is the sync-exhausted line. A bool, never text.
pub fn is_sync_exhausted(message: &str) -> bool {
    message.contains(SYNC_EXHAUSTED_TEXT)
}

/// What the backup's redundancy status says, against its own earlier sample.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupSignals {
    /// The backup sees its peer.
    pub peer_connected: bool,
    /// A sync is queued and has not run.
    pub sync_pending: bool,
    /// The backup's own `lastSyncTimestamp` before the apply; `None` is
    /// never synced (`-1`) or not sampled.
    pub last_sync_before: Option<i64>,
    /// The backup's `lastSyncTimestamp` now; `None` is never synced (`-1`).
    pub last_sync_now: Option<i64>,
    /// The backup's `projectState`, recorded, not a condition (V9).
    pub project_state: Option<String>,
}

impl BackupSignals {
    /// Signals from the backup's status now and its timestamp before. Both
    /// samples go through [`RedundancyStatusWire::last_sync_epoch_ms`]'s
    /// rule, so `-1` reads as never synced on either side (Pitfall 5). The
    /// timestamps are the same node's own clock: they are never compared with
    /// the master's.
    pub fn from_status(before: Option<i64>, now: &RedundancyStatusWire) -> Self {
        BackupSignals {
            peer_connected: now.peer_connected,
            sync_pending: now.sync_pending,
            last_sync_before: before.filter(|value| *value >= 0),
            last_sync_now: now.last_sync_epoch_ms(),
            project_state: now.project_state.as_deref().and_then(status_word),
        }
    }

    /// The backup's timestamp is a real one and is later than its sample
    /// (or it had never synced before).
    pub fn advanced(&self) -> bool {
        match (self.last_sync_before, self.last_sync_now) {
            (_, None) => false,
            (None, Some(_)) => true,
            (Some(before), Some(now)) => now > before,
        }
    }

    /// The backup sees its peer and has no sync queued.
    pub fn linked(&self) -> bool {
        self.peer_connected && !self.sync_pending
    }
}

/// `text` when it is a short status word (`Good`, `Out of Date`), else
/// `None`: the gateway's own text never enters a report beyond that.
fn status_word(text: &str) -> Option<String> {
    let plain = !text.is_empty()
        && text.chars().count() <= 32
        && text
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'));
    plain.then(|| text.to_string())
}

/// Why a backup did not converge (V2). Precedence, first that holds:
/// `unreachable`, `sync_exhausted`, `not_syncing`, `content_differs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsyncedReason {
    /// The backup's status or a content read could not be answered.
    Unreachable,
    /// The backup is not linked or its sync has not run.
    NotSyncing,
    /// The backup syncs but its content differs from the master's.
    ContentDiffers,
    /// The gateway gave up syncing (TEC-5105).
    SyncExhausted,
}

impl UnsyncedReason {
    /// The word the reason serializes as.
    pub fn as_str(self) -> &'static str {
        match self {
            UnsyncedReason::Unreachable => "unreachable",
            UnsyncedReason::NotSyncing => "not_syncing",
            UnsyncedReason::ContentDiffers => "content_differs",
            UnsyncedReason::SyncExhausted => "sync_exhausted",
        }
    }
}

/// The four checks of a backup report, in report order (V2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupCheckId {
    /// The backup is linked to its peer and its sync advanced.
    Redundancy,
    /// The backup runs the target's mode.
    Mode,
    /// Declared resources read back equal through the backup's REST.
    ReadBack,
    /// The backup's files equal the master's live files.
    Files,
}

/// One check of a backup report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackupCheck {
    /// Which check.
    pub id: BackupCheckId,
    /// Its outcome.
    pub status: CheckStatus,
    /// One line of what was evaluated.
    pub detail: String,
    /// Value-free observations.
    pub findings: Vec<Finding>,
}

/// The one resync request an apply may send (V2): the status of the POST,
/// never proof that the backup caught up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResyncRecord {
    /// The POST was accepted.
    pub ok: bool,
    /// The error code when it was not.
    pub code: Option<String>,
}

/// `data.backup` of an apply and of `deploy verify` (V2, D-09). It has no
/// field for gateway text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackupReport {
    /// The backup holds what the master holds.
    pub converged: bool,
    /// Why not, when it does not.
    pub reason: Option<UnsyncedReason>,
    /// The wait budget in seconds (0 for a verify).
    pub budget_s: u64,
    /// Seconds waited.
    pub waited_s: u64,
    /// The resync sent, if any.
    pub resync: Option<ResyncRecord>,
    /// The backup's timestamp before the apply, `None` when never synced.
    pub last_sync_before: Option<i64>,
    /// The backup's timestamp at the last read.
    pub last_sync_after: Option<i64>,
    /// The backup's `projectState` at the last read.
    pub project_state: Option<String>,
    /// Files compared with the master's.
    pub files_checked: usize,
    /// The checks, in [`BackupCheckId`] order.
    pub checks: Vec<BackupCheck>,
}

fn finding(code: &'static str, subject: &str, detail: &str) -> Finding {
    Finding {
        code,
        subject: subject.to_string(),
        detail: detail.to_string(),
    }
}

/// The `redundancy` check: the backup is linked to its peer, has no sync
/// queued and, when `require_advance`, its own timestamp advanced past its
/// own earlier sample. An `Err(code)` is a status that could not be read.
pub fn redundancy_check(
    signals: &Result<BackupSignals, String>,
    require_advance: bool,
) -> BackupCheck {
    let signals = match signals {
        Ok(signals) => signals,
        Err(code) => {
            let detail = format!("unreachable: {code}");
            return BackupCheck {
                id: BackupCheckId::Redundancy,
                status: CheckStatus::Fail,
                findings: vec![finding("backup_unreachable", "redundancy", &detail)],
                detail,
            };
        }
    };
    let mut findings = Vec::new();
    if !signals.peer_connected {
        findings.push(finding(
            "peer_not_connected",
            "peerConnected",
            "peerConnected is false",
        ));
    }
    if signals.sync_pending {
        findings.push(finding(
            "sync_pending",
            "syncPending",
            "syncPending is true",
        ));
    }
    if require_advance && !signals.advanced() {
        findings.push(finding(
            "sync_not_advanced",
            "lastSyncTimestamp",
            "lastSyncTimestamp has not advanced past the pre-apply sample",
        ));
    }
    if findings.is_empty() {
        let detail = if require_advance {
            "peerConnected, no sync pending, lastSyncTimestamp advanced"
        } else {
            "peerConnected, no sync pending"
        };
        return BackupCheck {
            id: BackupCheckId::Redundancy,
            status: CheckStatus::Pass,
            detail: detail.to_string(),
            findings,
        };
    }
    let names: Vec<&str> = findings.iter().map(|f| f.subject.as_str()).collect();
    BackupCheck {
        id: BackupCheckId::Redundancy,
        status: CheckStatus::Fail,
        detail: format!("the backup is not settled: {}", names.join(", ")),
        findings,
    }
}

/// A check for a read the backup could not answer. The detail carries the
/// error code only, never the gateway's words.
pub fn unreachable_check(id: BackupCheckId, code: &str) -> BackupCheck {
    let detail = format!("unreachable: {code}");
    BackupCheck {
        id,
        status: CheckStatus::Fail,
        findings: vec![finding("backup_unreachable", id.word(), &detail)],
        detail,
    }
}

impl BackupCheckId {
    /// The word the id serializes as.
    pub fn word(self) -> &'static str {
        match self {
            BackupCheckId::Redundancy => "redundancy",
            BackupCheckId::Mode => "mode",
            BackupCheckId::ReadBack => "read_back",
            BackupCheckId::Files => "files",
        }
    }
}

fn backup_check_from(id: BackupCheckId, check: verify::Check) -> BackupCheck {
    BackupCheck {
        id,
        status: check.status,
        detail: check.detail,
        findings: check.findings,
    }
}

/// The `mode` check of the backup: the master's `mode` evaluation over the
/// backup's normalized mode.
pub fn mode_backup_check(expected: &Expected, mode: Option<&str>) -> BackupCheck {
    backup_check_from(BackupCheckId::Mode, verify::mode_check(expected, mode))
}

/// The `read_back` check of the backup and the number of resources compared:
/// the master's `read_back` evaluation over the backup's REST view.
pub fn read_back_backup_check(
    expected: &Expected,
    read_back: &BTreeMap<String, Remote>,
    warnings: &mut Vec<String>,
) -> (BackupCheck, usize) {
    let (check, compared) = verify::read_back_check(expected, read_back, warnings);
    (backup_check_from(BackupCheckId::ReadBack, check), compared)
}

// ---- convergence ---------------------------------------------------------------

/// Which signal says the backup has synced (K1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncPredicate {
    /// The backup's own `lastSyncTimestamp` advanced past its own pre-apply
    /// sample (D-06 as written).
    TimestampAdvanced,
    /// The backup was linked and its content equal on this many ticks in a
    /// row.
    StableTicks(u8),
}

/// The predicate in force: K1 a, D-06 as written (24-02).
pub const SYNC_PREDICATE: SyncPredicate = SyncPredicate::TimestampAdvanced;

/// What one look at the backup found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TickOutcome {
    /// The status signals, or the error code of a status that could not be
    /// read.
    pub signals: Result<BackupSignals, String>,
    /// The log says the gateway gave up syncing (D-10).
    pub exhausted: bool,
    /// The mode, read-back and files checks; `None` when not evaluated yet.
    pub content: Option<Vec<BackupCheck>>,
    /// Files compared with the master's.
    pub files_checked: usize,
    /// Consecutive ticks on which the backup was linked and its content
    /// equal (only [`SyncPredicate::StableTicks`] reads it).
    pub stable_ticks: u8,
}

/// Whether the backup has converged (D-06): it is linked, the sync predicate
/// holds and no content check failed. A 200 or a timestamp alone never
/// converges (Pitfall 9). A `warn` (the file check a REST-only backup skips)
/// does not block.
pub fn converged(t: &TickOutcome, predicate: SyncPredicate) -> bool {
    let Ok(signals) = &t.signals else {
        return false;
    };
    if !signals.linked() || !sync_clause_met(t, signals, predicate) {
        return false;
    }
    match &t.content {
        Some(checks) => checks.iter().all(|c| c.status != CheckStatus::Fail),
        None => false,
    }
}

fn sync_clause_met(t: &TickOutcome, signals: &BackupSignals, predicate: SyncPredicate) -> bool {
    match predicate {
        SyncPredicate::TimestampAdvanced => signals.advanced(),
        SyncPredicate::StableTicks(ticks) => t.stable_ticks >= ticks,
    }
}

/// Why the backup is not converged, or `None` when it is (V2). First that
/// holds: `unreachable` (the status or a content read could not be answered),
/// `sync_exhausted`, `not_syncing` (not linked, still pending, or the sync
/// clause unmet), `content_differs`.
pub fn classify(t: &TickOutcome) -> Option<UnsyncedReason> {
    if converged(t, SYNC_PREDICATE) {
        return None;
    }
    let unanswered = t
        .content
        .iter()
        .flatten()
        .any(|c| c.status == CheckStatus::Fail && c.detail.starts_with(UNREACHABLE_PREFIX));
    if t.signals.is_err() || unanswered {
        return Some(UnsyncedReason::Unreachable);
    }
    if t.exhausted {
        return Some(UnsyncedReason::SyncExhausted);
    }
    if let Ok(signals) = &t.signals
        && (!signals.linked() || !sync_clause_met(t, signals, SYNC_PREDICATE))
    {
        return Some(UnsyncedReason::NotSyncing);
    }
    Some(UnsyncedReason::ContentDiffers)
}

/// The start of a check detail for a read that could not be answered.
const UNREACHABLE_PREFIX: &str = "unreachable:";

/// What the wait loop does next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairWaitStep {
    /// The backup has converged.
    Converged,
    /// The gateway gave up syncing: end the wait now (D-10).
    Exhausted,
    /// The budget is spent.
    TimedOut,
    /// Look again after this long.
    Sleep(Duration),
}

/// The decision of the wait loop, with the elapsed time injected. A budget
/// equal to the elapsed time has passed, and the last sleep never runs past
/// the budget.
pub fn wait_step(t: &TickOutcome, elapsed: Duration, policy: &BackupPolicy) -> PairWaitStep {
    if converged(t, SYNC_PREDICATE) {
        PairWaitStep::Converged
    } else if t.exhausted {
        PairWaitStep::Exhausted
    } else if elapsed >= policy.budget {
        PairWaitStep::TimedOut
    } else {
        PairWaitStep::Sleep(
            policy
                .interval
                .max(Duration::from_millis(1))
                .min(policy.budget - elapsed),
        )
    }
}

/// The report of a tick (V2): the redundancy check followed by the content
/// checks. `before` is the backup's pre-apply sample, kept when the status
/// could not be read.
pub fn report(
    t: &TickOutcome,
    before: Option<i64>,
    budget_s: u64,
    waited_s: u64,
    resync: Option<ResyncRecord>,
) -> BackupReport {
    let require_advance = matches!(SYNC_PREDICATE, SyncPredicate::TimestampAdvanced);
    let mut checks = vec![redundancy_check(&t.signals, require_advance)];
    checks.extend(t.content.iter().flatten().cloned());
    let (sampled, after, project_state) = match &t.signals {
        Ok(s) => (s.last_sync_before, s.last_sync_now, s.project_state.clone()),
        Err(_) => (None, None, None),
    };
    BackupReport {
        converged: converged(t, SYNC_PREDICATE),
        reason: classify(t),
        budget_s,
        waited_s,
        resync,
        last_sync_before: before.filter(|value| *value >= 0).or(sampled),
        last_sync_after: after,
        project_state,
        files_checked: t.files_checked,
        checks,
    }
}

/// The report of an on-demand look at the backup (`deploy verify --node`, V6).
///
/// Verify never waits, so there is no pre-apply sample to compare the sync
/// timestamp with: the `redundancy` check is `peerConnected` and not
/// `syncPending` ([`redundancy_check`] with `require_advance` false), and
/// the backup is converged when no check failed. The reason follows the V2
/// precedence without the sync clause: `unreachable`, then `not_syncing`
/// (not linked or still pending), then `content_differs`. Verify does not
/// read the log, so `sync_exhausted` is never its reason. The budget and the
/// time waited are 0 and no resync was sent.
pub fn verify_report(t: &TickOutcome) -> BackupReport {
    let mut checks = vec![redundancy_check(&t.signals, false)];
    checks.extend(t.content.iter().flatten().cloned());
    let converged = t.content.is_some() && checks.iter().all(|c| c.status != CheckStatus::Fail);
    let reason = if converged {
        None
    } else {
        let unanswered = checks
            .iter()
            .any(|c| c.status == CheckStatus::Fail && c.detail.starts_with(UNREACHABLE_PREFIX));
        Some(if unanswered {
            UnsyncedReason::Unreachable
        } else if !t.signals.as_ref().is_ok_and(BackupSignals::linked) {
            UnsyncedReason::NotSyncing
        } else {
            UnsyncedReason::ContentDiffers
        })
    };
    let (after, project_state) = match &t.signals {
        Ok(s) => (s.last_sync_now, s.project_state.clone()),
        Err(_) => (None, None),
    };
    BackupReport {
        converged,
        reason,
        budget_s: 0,
        waited_s: 0,
        resync: None,
        last_sync_before: None,
        last_sync_after: after,
        project_state,
        files_checked: t.files_checked,
        checks,
    }
}

/// The files the gateway serves from what a read found: the mode collection
/// over `core`.
pub fn effective_files(observed: ObservedFiles) -> BTreeMap<RelPath, FileBlob> {
    let mut files = observed.core;
    files.extend(observed.mode);
    files
}

// ---- files -----------------------------------------------------------------------

/// The one file that is compared as a parsed JSON document instead of byte
/// by byte, as `(file name, members left out of the comparison)` (D-03, K2 b
/// as revised after 24-10, `## C-P2`). The backup rewrites `resource.json`
/// with its members in another order or pretty-printed while every value
/// stays equal. Each excluded member is a dotted path from the document root;
/// exactly two are left out, `attributes.lastModification` and
/// `attributes.lastModificationSignature`, because a node that re-saves a
/// unit whose stamp has no valid signature rewrites both while the other node
/// keeps the text as written (24-10 finding). Every other member still
/// counts: a changed value, a missing member or a file that is not JSON
/// differs. Every other file is byte-equal.
pub const FILE_NORMALIZATION: &[(&str, &[&str])] = &[(
    "resource.json",
    &[
        "attributes.lastModification",
        "attributes.lastModificationSignature",
    ],
)];

/// How the backup's files compare with the master's live files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileComparison {
    /// Rendered paths compared.
    pub checked: usize,
    /// Paths whose bytes differ, byte-sorted.
    pub differing: Vec<String>,
    /// Rendered paths the backup does not have, byte-sorted.
    pub missing: Vec<String>,
    /// Paths an apply deleted that the backup still has, byte-sorted.
    pub unexpected: Vec<String>,
}

/// Compare the master's live files with the backup's (D-03): every rendered
/// path in `present` must be on the backup with the same bytes (sha256), and
/// every path in `absent` (the apply's deletes) must not be. The only
/// exception is a [`FILE_NORMALIZATION`] row (`resource.json`, compared as
/// parsed JSON without its two modification-stamp members). Bytes are compared as they are:
/// no newline or Unicode normalization.
pub fn compare_files(
    master: &BTreeMap<RelPath, FileBlob>,
    backup: &BTreeMap<RelPath, FileBlob>,
    present: &BTreeSet<RelPath>,
    absent: &BTreeSet<RelPath>,
) -> FileComparison {
    let mut out = FileComparison {
        checked: present.len(),
        ..FileComparison::default()
    };
    for path in present {
        let Some(theirs) = backup.get(path) else {
            out.missing.push(path.as_str().to_string());
            continue;
        };
        let equal = match master.get(path) {
            Some(ours) => ours.sha256 == theirs.sha256 || same_document(path, ours, theirs),
            None => false,
        };
        if !equal {
            out.differing.push(path.as_str().to_string());
        }
    }
    for path in absent {
        if backup.contains_key(path) {
            out.unexpected.push(path.as_str().to_string());
        }
    }
    // `present` and `absent` iterate in `RelPath` order, which is byte order.
    out
}

/// Whether `path` is a normalized file and the two copies are the same JSON
/// document.
fn same_document(path: &RelPath, ours: &FileBlob, theirs: &FileBlob) -> bool {
    let Some(name) = path.components().next_back() else {
        return false;
    };
    let Some((_, ignored)) = FILE_NORMALIZATION.iter().find(|(file, _)| *file == name) else {
        return false;
    };
    let (Ok(mut a), Ok(mut b)) = (
        serde_json::from_slice::<Value>(&ours.bytes),
        serde_json::from_slice::<Value>(&theirs.bytes),
    ) else {
        return false;
    };
    for doc in [&mut a, &mut b] {
        for dotted in *ignored {
            remove_member(doc, dotted);
        }
    }
    a == b
}

/// Remove the member at the dotted `path` below `doc`. A step that is not an
/// object, or a member that is absent, leaves the document as it was.
fn remove_member(doc: &mut Value, path: &str) {
    let mut node = doc;
    let mut steps = path.split('.').peekable();
    while let Some(step) = steps.next() {
        let Value::Object(map) = node else {
            return;
        };
        if steps.peek().is_none() {
            map.remove(step);
            return;
        }
        let Some(next) = map.get_mut(step) else {
            return;
        };
        node = next;
    }
}

/// The `files` check of a comparison. Lists at most [`APPLY_LIST_SHOWN`]
/// paths and the counts.
pub fn files_check(c: &FileComparison) -> BackupCheck {
    let kinds: [(&str, &str, &Vec<String>); 3] = [
        (
            "file_differs",
            "differs from the master's live copy",
            &c.differing,
        ),
        ("file_missing_on_backup", "is not on the backup", &c.missing),
        (
            "file_unexpected",
            "was deleted by the apply but is still on the backup",
            &c.unexpected,
        ),
    ];
    let findings: Vec<Finding> = kinds
        .iter()
        .flat_map(|(code, what, paths)| {
            paths.iter().map(move |path| Finding {
                code,
                subject: path.clone(),
                detail: (*what).to_string(),
            })
        })
        .take(APPLY_LIST_SHOWN)
        .collect();
    let bad = c.differing.len() + c.missing.len() + c.unexpected.len();
    let (status, detail) = if bad == 0 {
        (
            CheckStatus::Pass,
            format!("{} file(s) equal the master's live copies", c.checked),
        )
    } else {
        (
            CheckStatus::Fail,
            format!(
                "{} file(s) compared with the master's live copies: {} differ, {} missing on the backup, {} still present after being deleted",
                c.checked,
                c.differing.len(),
                c.missing.len(),
                c.unexpected.len()
            ),
        )
    };
    BackupCheck {
        id: BackupCheckId::Files,
        status,
        detail,
        findings,
    }
}

/// The `files` check of a backup that declares no file transport: it cannot
/// be run, which is a warning and not a failure.
pub fn files_skipped_check() -> BackupCheck {
    BackupCheck {
        id: BackupCheckId::Files,
        status: CheckStatus::Warn,
        detail:
            "nodes.backup declares no ssh or local transport, so the file-hash check did not run"
                .to_string(),
        findings: Vec::new(),
    }
}

// ---- secret names ----------------------------------------------------------------

/// A secret a render references, by provider and name. Names only.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct SecretRef {
    /// The secret provider.
    pub provider: String,
    /// The secret's name in it.
    pub name: String,
}

/// Where a secret-name check runs (K8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretCheckMode {
    /// Before an apply: a provider the render itself declares may not exist
    /// yet on a node, so it is skipped with its name.
    Preflight,
    /// After an apply: every referenced provider must exist.
    Verify,
}

/// The secret-name check of one node (V7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SecretNodeReport {
    /// `master` or `backup`.
    pub node: String,
    /// `fail` when a name is missing, `warn` when a provider cannot list its
    /// names, else `pass`.
    pub status: CheckStatus,
    /// Referenced providers.
    pub providers_checked: usize,
    /// Referenced names the node does not have, by provider then name.
    pub missing: Vec<SecretRef>,
    /// Providers that cannot list their names (K7), byte-sorted.
    pub unverifiable: Vec<String>,
    /// Providers the render declares that do not exist yet (K8, preflight
    /// only), byte-sorted.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skipped_providers: Vec<String>,
}

/// The secret-name check of the nodes a command ran it on, master first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SecretNamesReport {
    /// One report per node.
    pub nodes: Vec<SecretNodeReport>,
}

impl SecretNamesReport {
    /// The reports with `master` before `backup`.
    pub fn ordered(mut nodes: Vec<SecretNodeReport>) -> Self {
        nodes.sort_by_key(|n| match n.node.as_str() {
            "master" => 0u8,
            "backup" => 1,
            _ => 2,
        });
        SecretNamesReport { nodes }
    }
}

/// Every `(provider, name)` a rendered `.json` file references, byte-ordered.
/// A file that does not parse is skipped (lint reports it elsewhere).
pub fn collect_referenced_secrets(tree: &RenderedTree) -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    for (path, bytes) in &tree.files {
        if !path.as_str().ends_with(".json") {
            continue;
        }
        if let Ok((_, parsed)) = node::classify(bytes) {
            referenced_secret_names(&parsed, &mut out);
        }
    }
    out
}

/// The secret providers the render itself declares:
/// `ignition/secret-provider/<P>/...`.
pub fn declared_secret_providers(tree: &RenderedTree) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for path in tree.files.keys() {
        let parts: Vec<&str> = path.components().collect();
        if parts.len() >= 4 && parts[0] == "ignition" && parts[1] == "secret-provider" {
            out.insert(parts[2].to_string());
        }
    }
    out
}

/// The `(provider, name)` pairs the render itself defines (K8 extension,
/// WR-04): for each provider the render declares, the entries in its own
/// `ignition/secret-provider/<P>/config.json`, which are the keys of
/// `settings.files` (a file provider) and of `settings.secrets` (an internal
/// provider). A deploy that adds such an entry and references it in the same
/// render cannot find the name on a node before it is applied, so the
/// preflight does not ask the node for it.
///
/// Names only: the entries' values are never read. A config that does not
/// parse, or has no such member, defines nothing.
pub fn secrets_defined_by_render(tree: &RenderedTree) -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    for (path, bytes) in &tree.files {
        let parts: Vec<&str> = path.components().collect();
        let [first, second, provider, file] = parts.as_slice() else {
            continue;
        };
        if *first != "ignition" || *second != "secret-provider" || *file != "config.json" {
            continue;
        }
        let Ok(config) = serde_json::from_slice::<Value>(bytes) else {
            continue;
        };
        for member in ["files", "secrets"] {
            if let Some(entries) = config
                .get("settings")
                .and_then(|settings| settings.get(member))
                .and_then(Value::as_object)
            {
                out.extend(
                    entries
                        .keys()
                        .map(|name| ((*provider).to_string(), name.clone())),
                );
            }
        }
    }
    out
}

/// Whether `node` has every secret name the render references (D-13).
///
/// Names compare as exact, case-sensitive strings (K9). A provider that
/// cannot list its names is `unverifiable` (K7); a provider that does not
/// exist on the node is `skipped` when the render declares it and the mode is
/// a preflight (K8), else its names are missing. A name the render defines
/// itself is filtered out of `referenced` by the caller before a preflight
/// ([`secrets_defined_by_render`]); verify keeps it, since after an apply the
/// name must exist. A provider with no entry in
/// `listings` reads as not existing.
pub fn check_secret_names(
    node: &str,
    referenced: &BTreeSet<(String, String)>,
    listings: &BTreeMap<String, SecretNameListing>,
    declared: &BTreeSet<String>,
    mode: SecretCheckMode,
) -> SecretNodeReport {
    let mut by_provider: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (provider, name) in referenced {
        by_provider
            .entry(provider.as_str())
            .or_default()
            .push(name.as_str());
    }
    let mut missing = Vec::new();
    let mut unverifiable = Vec::new();
    let mut skipped = Vec::new();
    for (provider, names) in &by_provider {
        let all_missing = |missing: &mut Vec<SecretRef>| {
            missing.extend(names.iter().map(|name| SecretRef {
                provider: (*provider).to_string(),
                name: (*name).to_string(),
            }));
        };
        match listings.get(*provider) {
            Some(SecretNameListing::Listed(listed)) => {
                missing.extend(
                    names
                        .iter()
                        .filter(|name| !listed.contains(**name))
                        .map(|name| SecretRef {
                            provider: (*provider).to_string(),
                            name: (*name).to_string(),
                        }),
                );
            }
            Some(SecretNameListing::CannotList) => unverifiable.push((*provider).to_string()),
            Some(SecretNameListing::NoProvider) | None => {
                if mode == SecretCheckMode::Preflight && declared.contains(*provider) {
                    skipped.push((*provider).to_string());
                } else {
                    all_missing(&mut missing);
                }
            }
        }
    }
    let status = if !missing.is_empty() {
        CheckStatus::Fail
    } else if !unverifiable.is_empty() {
        CheckStatus::Warn
    } else {
        CheckStatus::Pass
    };
    SecretNodeReport {
        node: node.to_string(),
        status,
        providers_checked: by_provider.len(),
        missing,
        unverifiable,
        skipped_providers: skipped,
    }
}

#[cfg(test)]
mod tests;
