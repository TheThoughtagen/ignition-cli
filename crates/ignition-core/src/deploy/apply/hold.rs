//! The hold layer of the apply engine (Phase 23, D-02): the scan-lock held
//! for the length of one commit and its scan, judged by two clocks.
//!
//! # D-02's rule
//!
//! The hold is the scan-lock's `holdTimeout`. The user fixed its shape in
//! D-02, from the 22-14 measurement (22-LIVE-CAPTURES `## Decisions: OQ5
//! timings, hold budget and timeout defaults (22-14)`, OQ5-2):
//!
//! ```text
//! hold = ceil(2 x (p95 commit + p95 scan) + 1), floor 10 s, ceiling 300 s
//! ```
//!
//! [`HOLD_FLOOR_S`] and [`HOLD_CEILING_S`] are those two numbers. They are
//! user decisions and not measurements, and the ceiling changes only by the
//! user's D-03a choice (23-10 Task 3 option-a). [`hold_for`] returns the
//! formula UNCLAMPED so that D-03a can see the raw value; [`hold_clamped`]
//! is the separate step that applies floor and ceiling, so nothing clamps
//! silently.
//!
//! # No measured value here
//!
//! 505 s and 699 s, which the formula gives for the 22-14 timings, came from
//! the old per-file sftp commit that D-01 replaces. They appear in this
//! file's tests as formula regression values and are never a hold. The
//! measured p95 literals, the shipped hold and `HoldModel::MEASURED` are
//! added by 23-10 Task 2 from `## Decisions: OQ5b` and are absent until
//! then. Until then every caller passes an explicit [`HoldModel`], and the
//! apply verb (23-13) names `HoldModel::MEASURED`, so it cannot compile
//! before the re-measure (D-02, D-03a).
//!
//! # Two clocks
//!
//! [`HeldLock`] never compares the gateway's `timestamp` or
//! `lastScanTimestamp` with the runner's clock (Pitfall 15). It judges the
//! lock by the gateway's `remaining` and by its own monotonic [`Instant`],
//! taken before the acquire request; either one saying the lock is
//! expiring makes it expired.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::client::DeployApi;
use crate::client::scan::{LockVerdict, ScanKind, lock_verdict};
use crate::error::CoreError;

/// D-02's floor: the shortest hold the engine ever requests, in seconds.
/// A user decision, not a measurement.
pub const HOLD_FLOOR_S: u64 = 10;

/// D-02's ceiling: the longest hold the engine ever requests, in seconds.
/// A user decision, not a measurement. It changes only by the user's D-03a
/// choice (23-10 Task 3 option-a), which updates the clamp assertions in
/// the same commit.
pub const HOLD_CEILING_S: u64 = 300;

/// D-02's formula, unclamped: `ceil(2 x (p95_commit_s + p95_scan_s) + 1)`.
pub fn hold_for(_p95_commit_s: f64, _p95_scan_s: f64) -> u64 {
    0
}

/// `raw` held to D-02's floor and ceiling.
pub fn hold_clamped(raw: u64) -> u64 {
    raw
}

/// Whether a run whose commit is estimated at `est_commit_s` and whose scan
/// takes `p95_scan_s` fits in `hold_s`: the hold the formula asks for is at
/// most `hold_s`.
pub fn fits(_hold_s: u64, _est_commit_s: f64, _p95_scan_s: f64) -> bool {
    false
}

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

impl HoldModel {
    /// Seconds a commit of `files` files is estimated to take.
    pub fn estimate_commit_s(&self, _files: usize) -> f64 {
        0.0
    }

    /// Whether a run touching `files` files fits in this model's hold (D-03).
    pub fn fits(&self, _files: usize) -> bool {
        false
    }
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

/// A clock a test moves by hand.
#[doc(hidden)]
#[derive(Debug)]
pub struct ManualClock {
    base: Instant,
    offset: Mutex<Duration>,
}

impl Default for ManualClock {
    fn default() -> Self {
        Self::new()
    }
}

impl ManualClock {
    /// A clock at an arbitrary starting instant.
    pub fn new() -> Self {
        ManualClock {
            base: Instant::now(),
            offset: Mutex::new(Duration::ZERO),
        }
    }

    /// Move the clock forward.
    pub fn advance(&self, _by: Duration) {}
}

impl Clock for ManualClock {
    fn now(&self) -> Instant {
        let _ = &self.offset;
        self.base
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::client::config_resources::{
        MutationOutcome, ResourceCreate, ResourceName, ResourceRecord, ResourceUpdate,
    };
    use crate::client::mode::{ModeCreate, ModeRecord};
    use crate::client::query::{ListEnvelope, ListQuery};
    use crate::client::scan::{LockObservation, ScanLockInfo, ScanStatus};

    // ---- the formula --------------------------------------------------------

    #[test]
    fn hold_formula_regression() {
        // The 22-14 per-file-path numbers, used only to pin the formula.
        assert_eq!(hold_for(251.063, 0.888), 505);
        assert_eq!(hold_for(348.225, 0.642), 699);
        // `hold_for` does not clamp; `hold_clamped` is a separate step.
        assert!(hold_for(251.063, 0.888) > HOLD_CEILING_S);
        assert_eq!(hold_clamped(505), HOLD_CEILING_S);
        assert_eq!(hold_clamped(699), HOLD_CEILING_S);
        assert_eq!(hold_clamped(HOLD_CEILING_S), HOLD_CEILING_S);
        assert_eq!(hold_clamped(120), 120);
    }

    #[test]
    fn hold_floor() {
        assert_eq!(hold_for(1.0, 0.5), 4);
        assert_eq!(hold_clamped(4), HOLD_FLOOR_S);
        assert_eq!(hold_clamped(0), HOLD_FLOOR_S);
        assert_eq!(hold_clamped(HOLD_FLOOR_S), HOLD_FLOOR_S);
    }

    #[test]
    fn d02_numbers_are_the_users() {
        assert_eq!((HOLD_FLOOR_S, HOLD_CEILING_S), (10, 300));
    }

    #[test]
    fn hold_fit_boundary() {
        let needed = hold_for(6.0, 0.5);
        assert_eq!(needed, 14);
        assert!(fits(needed, 6.0, 0.5), "equal fits");
        assert!(!fits(needed - 1, 6.0, 0.5), "one second over does not");

        let model = HoldModel {
            hold_s: 14,
            commit_fixed_s: 1.0,
            commit_per_file_s: 0.5,
            p95_scan_s: 0.5,
        };
        assert_eq!(model.estimate_commit_s(10), 6.0);
        assert!(model.fits(10), "needs exactly 14 s");
        assert!(!model.fits(11), "needs 16 s");
        let tight = HoldModel {
            hold_s: 13,
            ..model
        };
        assert!(!tight.fits(10), "one second short");
    }

    // ---- the held lock ------------------------------------------------------

    /// A gateway that only knows the scan-lock; the rest is not used.
    struct LockGateway {
        info: Mutex<LockObservation>,
        acquired: Mutex<Vec<(ScanKind, u64, u64)>>,
    }

    impl LockGateway {
        fn holding(remaining: i64, timestamp: Option<i64>) -> Self {
            LockGateway {
                info: Mutex::new(LockObservation::Held(ScanLockInfo {
                    timestamp,
                    remaining,
                    actor: Some("api-token:t:127.0.0.1".to_string()),
                    extra: BTreeMap::new(),
                })),
                acquired: Mutex::new(Vec::new()),
            }
        }

        fn free() -> Self {
            LockGateway {
                info: Mutex::new(LockObservation::Free),
                acquired: Mutex::new(Vec::new()),
            }
        }
    }

    fn unused<T>() -> Result<T, CoreError> {
        Err(CoreError::Internal(
            "not used by the hold tests".to_string(),
        ))
    }

    #[async_trait::async_trait]
    impl DeployApi for LockGateway {
        async fn resource_names(
            &self,
            _module: &str,
            _ty: &str,
        ) -> Result<ListEnvelope<ResourceName>, CoreError> {
            unused()
        }
        async fn resource_list(
            &self,
            _module: &str,
            _ty: &str,
            _q: &ListQuery,
        ) -> Result<ListEnvelope<ResourceRecord>, CoreError> {
            unused()
        }
        async fn resource_find(
            &self,
            _module: &str,
            _ty: &str,
            _name: &str,
            _collection: Option<&str>,
        ) -> Result<ResourceRecord, CoreError> {
            unused()
        }
        async fn resource_singleton(
            &self,
            _module: &str,
            _ty: &str,
            _collection: Option<&str>,
        ) -> Result<ResourceRecord, CoreError> {
            unused()
        }
        async fn resource_create(
            &self,
            _module: &str,
            _ty: &str,
            _items: &[ResourceCreate],
        ) -> Result<MutationOutcome, CoreError> {
            unused()
        }
        async fn resource_update(
            &self,
            _module: &str,
            _ty: &str,
            _items: &[ResourceUpdate],
        ) -> Result<MutationOutcome, CoreError> {
            unused()
        }
        async fn scan_lock_acquire(
            &self,
            kind: ScanKind,
            acquire_s: u64,
            hold_s: u64,
        ) -> Result<(), CoreError> {
            self.acquired
                .lock()
                .expect("acquired")
                .push((kind, acquire_s, hold_s));
            Ok(())
        }
        async fn scan_lock_info(&self, _kind: ScanKind) -> Result<LockObservation, CoreError> {
            Ok(self.info.lock().expect("info").clone())
        }
        async fn scan_status(&self, _kind: ScanKind) -> Result<ScanStatus, CoreError> {
            unused()
        }
        async fn scan_request(&self, _kind: ScanKind) -> Result<(), CoreError> {
            unused()
        }
        async fn modes(&self) -> Result<ListEnvelope<ModeRecord>, CoreError> {
            unused()
        }
        async fn mode_create(&self, _body: &ModeCreate) -> Result<ModeRecord, CoreError> {
            unused()
        }
        async fn deployment_mode(&self) -> Result<Option<String>, CoreError> {
            unused()
        }
    }

    const HOLD: u64 = 60;

    async fn lock_on(gateway: &LockGateway, clock: &ManualClock) -> HeldLock {
        HeldLock::acquire(gateway, ScanKind::Config, HOLD, clock)
            .await
            .expect("acquire")
    }

    #[tokio::test]
    async fn held_lock_acquires_the_config_lock_with_the_hold_and_no_wait() {
        let gateway = LockGateway::holding(HOLD as i64, None);
        let clock = ManualClock::new();
        let lock = lock_on(&gateway, &clock).await;
        assert_eq!(
            *gateway.acquired.lock().expect("acquired"),
            [(ScanKind::Config, 0, HOLD)]
        );
        assert_eq!((lock.kind(), lock.hold_s()), (ScanKind::Config, HOLD));
    }

    #[tokio::test]
    async fn held_lock_is_held_while_both_clocks_have_the_margin() {
        let gateway = LockGateway::holding(30, None);
        let clock = ManualClock::new();
        let lock = lock_on(&gateway, &clock).await;
        clock.advance(Duration::from_secs(5));
        let verdict = lock
            .check(&gateway, LOCK_MARGIN_S, &clock)
            .await
            .expect("check");
        assert_eq!(verdict, LockVerdict::Held { remaining_s: 30 });
    }

    #[tokio::test]
    async fn held_lock_local_clock_expiry() {
        // The gateway still says 30 s are left; the local clock says the hold
        // is nearly gone, and the local clock wins.
        let gateway = LockGateway::holding(30, None);
        let clock = ManualClock::new();
        let lock = lock_on(&gateway, &clock).await;

        // Exactly the margin left locally is still held (the gateway's rule).
        clock.advance(Duration::from_secs(HOLD - LOCK_MARGIN_S as u64));
        assert_eq!(lock.local_remaining_s(&clock), LOCK_MARGIN_S);
        assert!(matches!(
            lock.check(&gateway, LOCK_MARGIN_S, &clock)
                .await
                .expect("check"),
            LockVerdict::Held { .. }
        ));

        // One more second and it is expiring.
        clock.advance(Duration::from_secs(1));
        assert_eq!(lock.local_remaining_s(&clock), LOCK_MARGIN_S - 1);
        let verdict = lock
            .check(&gateway, LOCK_MARGIN_S, &clock)
            .await
            .expect("check");
        assert_eq!(
            verdict,
            LockVerdict::Expiring {
                remaining_s: LOCK_MARGIN_S - 1
            }
        );

        // Past the hold the local count is negative, never wrapped.
        clock.advance(Duration::from_secs(HOLD));
        assert!(lock.local_remaining_s(&clock) < 0);
    }

    #[tokio::test]
    async fn held_lock_gateway_expiring() {
        // A fresh local clock, but the gateway reports one second left.
        let gateway = LockGateway::holding(1, None);
        let clock = ManualClock::new();
        let lock = lock_on(&gateway, &clock).await;
        let verdict = lock
            .check(&gateway, LOCK_MARGIN_S, &clock)
            .await
            .expect("check");
        assert_eq!(verdict, LockVerdict::Expiring { remaining_s: 1 });
    }

    #[tokio::test]
    async fn held_lock_free_on_the_gateway_is_not_held() {
        let gateway = LockGateway::free();
        let clock = ManualClock::new();
        let lock = lock_on(&gateway, &clock).await;
        let verdict = lock
            .check(&gateway, LOCK_MARGIN_S, &clock)
            .await
            .expect("check");
        assert_eq!(verdict, LockVerdict::NotHeld);
    }

    #[tokio::test]
    async fn held_lock_never_reads_the_gateway_timestamp() {
        let clock = ManualClock::new();
        let mut verdicts = Vec::new();
        for timestamp in [None, Some(0), Some(i64::MAX), Some(i64::MIN)] {
            let gateway = LockGateway::holding(30, timestamp);
            let lock = lock_on(&gateway, &clock).await;
            verdicts.push(
                lock.check(&gateway, LOCK_MARGIN_S, &clock)
                    .await
                    .expect("check"),
            );
        }
        assert!(
            verdicts
                .iter()
                .all(|v| *v == LockVerdict::Held { remaining_s: 30 })
        );
    }

    #[test]
    fn manual_clock_moves_only_when_told() {
        let clock = ManualClock::new();
        let start = clock.now();
        assert_eq!(clock.now(), start);
        clock.advance(Duration::from_secs(3));
        assert_eq!(clock.now() - start, Duration::from_secs(3));
    }
}
