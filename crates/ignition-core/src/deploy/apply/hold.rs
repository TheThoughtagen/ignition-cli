//! The hold layer of the apply engine (Phase 23, D-02): the scan-lock held
//! for the length of one commit and its scan, judged by two clocks.
//!
//! Task 2 of 23-05 completes this module with D-02's formula, floor and
//! ceiling. It carries no measured value: the measured hold lands only in
//! 23-10.

use std::time::Instant;

use crate::client::DeployApi;
use crate::client::scan::{LockVerdict, ScanKind, lock_verdict};
use crate::error::CoreError;

/// Seconds of margin a lock needs left at a check: one REST round trip from
/// the check to the scan request, which releases the lock (D12).
pub const LOCK_MARGIN_S: i64 = 2;

/// What the engine knows about the length of a run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HoldModel {
    /// Seconds the scan-lock is requested for.
    pub hold_s: u64,
    /// Fixed seconds a commit costs, whatever its size.
    pub commit_fixed_s: f64,
    /// Seconds a commit costs per touched file.
    pub commit_per_file_s: f64,
    /// 95th percentile seconds a config scan takes.
    pub p95_scan_s: f64,
}

/// A monotonic clock, so a test can move time.
pub trait Clock: Send + Sync {
    /// The current instant.
    fn now(&self) -> Instant;
}

/// The real monotonic clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// A scan-lock this run took.
#[derive(Debug, Clone, Copy)]
pub struct HeldLock {
    kind: ScanKind,
    hold_s: u64,
    acquired: Instant,
}

impl HeldLock {
    /// Take the lock for `hold_s` seconds. The local instant is taken
    /// before the request, so the local clock never reads later than the
    /// gateway's own count of the hold (research Pattern 6).
    pub async fn acquire(
        deploy: &dyn DeployApi,
        kind: ScanKind,
        hold_s: u64,
        clock: &dyn Clock,
    ) -> Result<HeldLock, CoreError> {
        let acquired = clock.now();
        deploy.scan_lock_acquire(kind, 0, hold_s).await?;
        Ok(HeldLock {
            kind,
            hold_s,
            acquired,
        })
    }

    /// The scan kind this lock covers.
    pub fn kind(&self) -> ScanKind {
        self.kind
    }

    /// The hold that was requested, in seconds.
    pub fn hold_s(&self) -> u64 {
        self.hold_s
    }

    /// Seconds left by the local monotonic clock: the hold minus the whole
    /// seconds since the request, rounded up so the answer errs short.
    pub fn local_remaining_s(&self, clock: &dyn Clock) -> i64 {
        let elapsed = clock.now().saturating_duration_since(self.acquired);
        let elapsed_s = elapsed.as_secs() + u64::from(elapsed.subsec_nanos() > 0);
        i64::try_from(self.hold_s)
            .unwrap_or(i64::MAX)
            .saturating_sub(i64::try_from(elapsed_s).unwrap_or(i64::MAX))
    }

    /// Judge the lock now, by both clocks: the gateway's `remaining`
    /// through [`lock_verdict`], and the local count of the hold. Either one
    /// saying the lock has less than `margin_s` left makes the answer
    /// [`LockVerdict::Expiring`]; a free lock is [`LockVerdict::NotHeld`].
    /// `timestamp` and `lastScanTimestamp` are never compared with the
    /// runner's clock (Pitfall 15).
    pub async fn check(
        &self,
        deploy: &dyn DeployApi,
        margin_s: i64,
        clock: &dyn Clock,
    ) -> Result<LockVerdict, CoreError> {
        let observed = deploy.scan_lock_info(self.kind).await?;
        let local_s = self.local_remaining_s(clock);
        Ok(match lock_verdict(&observed, margin_s) {
            LockVerdict::Held { remaining_s } if local_s < margin_s => LockVerdict::Expiring {
                remaining_s: remaining_s.min(local_s),
            },
            verdict => verdict,
        })
    }
}
