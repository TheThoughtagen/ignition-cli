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

use std::time::Duration;

use serde::Serialize;

use super::verify::{CheckStatus, Finding};
use crate::client::mode::normalize_deployment_mode;
use crate::client::redundancy::RedundancyStatusWire;
use crate::client::{DeployApi, GatewayApi};
use crate::deploy::transport::TreeReader;
use crate::error::CoreError;

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
            project_state: now.project_state.clone(),
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
