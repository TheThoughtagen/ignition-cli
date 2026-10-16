//! The hold layer of the apply engine (Phase 23, D-02): the scan-lock held
//! for the length of one commit and its scan, judged by two clocks.
//!
//! Task 2 of 23-05 completes this module with D-02's formula, floor and
//! ceiling. It carries no measured value: the measured hold lands only in
//! 23-10.

use std::time::Instant;

use crate::client::DeployApi;
use crate::client::scan::{LockVerdict, ScanKind};
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
    /// Take the lock for `hold_s` seconds.
    pub async fn acquire(
        _deploy: &dyn DeployApi,
        _kind: ScanKind,
        _hold_s: u64,
        _clock: &dyn Clock,
    ) -> Result<HeldLock, CoreError> {
        Err(CoreError::Internal("HeldLock::acquire is not built".into()))
    }

    /// The lock's verdict now.
    pub async fn check(
        &self,
        _deploy: &dyn DeployApi,
        _margin_s: i64,
        _clock: &dyn Clock,
    ) -> Result<LockVerdict, CoreError> {
        let _ = (self.kind, self.hold_s, self.acquired);
        Err(CoreError::Internal("HeldLock::check is not built".into()))
    }
}
