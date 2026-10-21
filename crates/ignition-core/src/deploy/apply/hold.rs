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
//! # The measured value
//!
//! 505 s and 699 s, which the formula gives for the 22-14 timings, came from
//! the old per-file sftp commit that D-01 replaces. They appear in this
//! file's tests as formula regression values and are never a hold. The hold
//! the engine ships with is [`HOLD_S`], computed from the batched commit
//! measured on the Windows capture host (23-10, `23-LIVE-CAPTURES.md`
//! `## OQ5b` and `## Decisions: OQ5b`): the p95 literals below, the formula
//! on the largest measured scale, and D-02's clamp. [`HoldModel::MEASURED`]
//! is the model the apply verb (23-13) names. The literals are written here
//! and nowhere else; `hold_s_is_derived_from_recorded_measurements`
//! recomputes the hold and the fit from the recorded numbers and fails if
//! either drifts.
//!
//! # Two clocks
//!
//! [`HeldLock`] never compares the gateway's `timestamp` or
//! `lastScanTimestamp` with the runner's clock (Pitfall 15). It judges the
//! lock by the gateway's `remaining` and by its own monotonic [`Instant`],
//! taken before the acquire request; either one saying the lock is
//! expiring makes it expired.
//!
//! # Whose lock
//!
//! The gateway frees a lapsed lock and grants it to the next actor, so a
//! held lock after our hold lapsed is not proof that it is still ours
//! (23-17, DAPL-07). [`HeldLock::acquire`] reads the lock once right after
//! the grant and remembers its `timestamp` and `actor`; C1c and D9 show the
//! `timestamp` is unchanged across reads of one hold and different for each
//! new hold. A later reading with another `timestamp` or `actor` is another
//! actor's lock and reads as [`LockVerdict::NotHeld`]. The two readings come
//! from the same gateway and are only compared with each other, never with a
//! clock.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::client::DeployApi;
use crate::client::scan::{LockObservation, LockVerdict, ScanKind, ScanLockInfo, lock_verdict};
use crate::error::CoreError;

/// Seconds the engine asks the gateway to wait for a lock held by someone
/// else (`acquireTimeout`). The gateway refuses 0 (and any non-positive
/// timeout) with a 400 "Timeouts must be positive" (23-16 live gate), and it
/// never waits out a positive one: a held lock is an immediate 409 (D1,
/// `20-LIVE-CAPTURES.md`), so the smallest positive value is "no wait".
pub const ACQUIRE_WAIT_S: u64 = 1;

/// D-02's floor: the shortest hold the engine ever requests, in seconds.
/// A user decision, not a measurement.
pub const HOLD_FLOOR_S: u64 = 10;

/// D-02's ceiling: the longest hold the engine ever requests, in seconds.
/// A user decision, not a measurement. It changes only by the user's D-03a
/// choice (23-10 Task 3 option-a), which updates the clamp assertions in
/// the same commit.
pub const HOLD_CEILING_S: u64 = 300;

/// D-02's formula, unclamped: `ceil(2 x (p95_commit_s + p95_scan_s) + 1)`.
pub fn hold_for(p95_commit_s: f64, p95_scan_s: f64) -> u64 {
    // `as` saturates: a negative or NaN input reads as 0.
    (2.0 * (p95_commit_s + p95_scan_s) + 1.0).ceil() as u64
}

/// `raw` held to D-02's floor and ceiling.
pub fn hold_clamped(raw: u64) -> u64 {
    raw.clamp(HOLD_FLOOR_S, HOLD_CEILING_S)
}

/// Whether a run whose commit is estimated at `est_commit_s` and whose scan
/// takes `p95_scan_s` fits in `hold_s`: the hold the formula asks for is at
/// most `hold_s`.
pub fn fits(hold_s: u64, est_commit_s: f64, p95_scan_s: f64) -> bool {
    hold_for(est_commit_s, p95_scan_s) <= hold_s
}

/// Files in the scale 1 corpus the commit was timed with (22-14, 23-10).
pub const FILES_SCALE1: usize = 152;

/// Files in the scale 10 corpus, the largest tree measured.
pub const FILES_SCALE10: usize = 1520;

/// p95 seconds of the slowest commit variant at 152 files: the replace
/// commit, every file replaced and every pre-image copied (`## Decisions:
/// OQ5b`, OQ5b-1 and OQ5b-2; n = 5, so the nearest-rank p95 is the maximum).
pub const P95_COMMIT_S_SCALE1: f64 = 10.123;

/// p95 seconds of the slowest commit variant at 1,520 files, the largest
/// measured scale (FA-23-07): the replace commit (`## Decisions: OQ5b`,
/// OQ5b-1 and OQ5b-2; FA-23-08). The new-file commit measured 62.040 s.
pub const P95_COMMIT_S_SCALE10: f64 = 85.496;

/// p95 seconds of the slowest config scan at 1,520 files, wall clock through
/// `ign api call` (`## Decisions: OQ5b`, OQ5b-1): the scan after the replace
/// commit. The gateway's own figure is under 1.2 s.
pub const P95_SCAN_S: f64 = 1.579;

/// The hold the apply engine requests, in seconds: D-02's formula on the
/// largest measured scale, clamped. `ceil(2 x (85.496 + 1.579) + 1)` is 176,
/// inside the ceiling, so D-03a did not trigger. A literal so that it reads in
/// `grep`; `hold_s_is_derived_from_recorded_measurements` recomputes it from
/// the three literals above and fails if any of the four drifts. One constant:
/// no per-run scaling and no per-target override (D-02).
pub const HOLD_S: u64 = 176;

/// The two-scale fit of the p95 commit time against the file count: seconds
/// per file, `(p95 at 1,520 - p95 at 152) / (1520 - 152)`.
const FIT_PER_FILE_S: f64 =
    (P95_COMMIT_S_SCALE10 - P95_COMMIT_S_SCALE1) / (FILES_SCALE10 as f64 - FILES_SCALE1 as f64);

/// The fixed part of the fit: `p95 at 152 - 152 x per file`, floored at 0.
const FIT_FIXED_S: f64 = {
    let fixed = P95_COMMIT_S_SCALE1 - FILES_SCALE1 as f64 * FIT_PER_FILE_S;
    if fixed > 0.0 { fixed } else { 0.0 }
};

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
    /// The model measured on the Windows capture host (23-10): the shipped
    /// hold, the p95 scan and the two-scale fit of the commit time. The D-03
    /// refusal (23-07) and the apply verb (23-13) use this and no other.
    pub const MEASURED: HoldModel = HoldModel {
        hold_s: HOLD_S,
        commit_fixed_s: FIT_FIXED_S,
        commit_per_file_s: FIT_PER_FILE_S,
        p95_scan_s: P95_SCAN_S,
    };

    /// Seconds a commit of `files` files is estimated to take.
    pub fn estimate_commit_s(&self, files: usize) -> f64 {
        self.commit_fixed_s + files as f64 * self.commit_per_file_s
    }

    /// Whether a run touching `files` files fits in this model's hold (D-03).
    pub fn fits(&self, files: usize) -> bool {
        fits(self.hold_s, self.estimate_commit_s(files), self.p95_scan_s)
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

/// What tells one hold from another on the gateway (C1c, D9).
#[derive(Debug, Clone, PartialEq, Eq)]
struct LockIdentity {
    timestamp: Option<i64>,
    actor: Option<String>,
}

impl LockIdentity {
    fn of(info: &ScanLockInfo) -> Self {
        LockIdentity {
            timestamp: info.timestamp,
            actor: info.actor.clone(),
        }
    }
}

/// A scan-lock this run took.
#[derive(Debug, Clone)]
pub struct HeldLock {
    kind: ScanKind,
    hold_s: u64,
    acquired: Instant,
    /// The hold as the gateway showed it right after the grant; `None` when
    /// the lock read free then, in which case no later lock is ours.
    identity: Option<LockIdentity>,
}

impl HeldLock {
    /// Take the lock for `hold_s` seconds. The local instant is taken
    /// before the request, so the local clock never reads later than the
    /// gateway's own count of the hold (research Pattern 6). One read right
    /// after the grant remembers which hold is ours (see the module doc).
    pub async fn acquire(
        deploy: &dyn DeployApi,
        kind: ScanKind,
        hold_s: u64,
        clock: &dyn Clock,
    ) -> Result<HeldLock, CoreError> {
        let acquired = clock.now();
        deploy
            .scan_lock_acquire(kind, ACQUIRE_WAIT_S, hold_s)
            .await?;
        let identity = match deploy.scan_lock_info(kind).await? {
            LockObservation::Held(info) => Some(LockIdentity::of(&info)),
            LockObservation::Free => None,
        };
        Ok(HeldLock {
            kind,
            hold_s,
            acquired,
            identity,
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

    /// Whether `info` is the hold this value took.
    fn is_ours(&self, info: &ScanLockInfo) -> bool {
        self.identity.as_ref() == Some(&LockIdentity::of(info))
    }

    /// The gateway's side of the judgement alone: [`lock_verdict`] on its
    /// `remaining`, where a lock that is not ours reads as
    /// [`LockVerdict::NotHeld`]. This is the only call that asks the gateway.
    pub async fn check_gateway(
        &self,
        deploy: &dyn DeployApi,
        margin_s: i64,
    ) -> Result<LockVerdict, CoreError> {
        let observed = deploy.scan_lock_info(self.kind).await?;
        Ok(match &observed {
            LockObservation::Held(info) if !self.is_ours(info) => LockVerdict::NotHeld,
            _ => lock_verdict(&observed, margin_s),
        })
    }

    /// Judge the lock now, by both clocks: the gateway's `remaining`
    /// through [`HeldLock::check_gateway`], and the local count of the hold.
    /// Either one saying the lock has less than `margin_s` left makes the
    /// answer [`LockVerdict::Expiring`]; a free lock, or one that is another
    /// actor's, is [`LockVerdict::NotHeld`]. `timestamp` and
    /// `lastScanTimestamp` are never compared with the runner's clock
    /// (Pitfall 15).
    pub async fn check(
        &self,
        deploy: &dyn DeployApi,
        margin_s: i64,
        clock: &dyn Clock,
    ) -> Result<LockVerdict, CoreError> {
        let local_s = self.local_remaining_s(clock);
        Ok(match self.check_gateway(deploy, margin_s).await? {
            LockVerdict::Held { remaining_s } if local_s < margin_s => LockVerdict::Expiring {
                remaining_s: remaining_s.min(local_s),
            },
            verdict => verdict,
        })
    }

    /// Wait until the gateway no longer shows this hold: it lapsed, a scan
    /// released it, or another actor holds the lock now. A scan is never
    /// the way to end it, because a scan loads whatever is on disk. Polls
    /// every `interval` and gives up after the hold plus a margin of real
    /// time.
    pub async fn wait_for_lapse(
        &self,
        deploy: &dyn DeployApi,
        interval: Duration,
    ) -> Result<(), CoreError> {
        let limit = Duration::from_secs(self.hold_s.saturating_add(LOCK_MARGIN_S as u64 + 3));
        let started = tokio::time::Instant::now();
        loop {
            if self.check_gateway(deploy, 0).await? == LockVerdict::NotHeld {
                return Ok(());
            }
            if started.elapsed() >= limit {
                return Err(CoreError::Internal(
                    "the scan-lock did not lapse within its hold".to_string(),
                ));
            }
            tokio::time::sleep(interval).await;
        }
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
    pub fn advance(&self, by: Duration) {
        *self.offset.lock().expect("manual clock") += by;
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Instant {
        self.base + *self.offset.lock().expect("manual clock")
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

    // ---- the measured hold (23-10, `## Decisions: OQ5b`) ---------------------

    /// The numbers recorded in `23-LIVE-CAPTURES.md` `## Decisions: OQ5b`,
    /// copied here so a change to a constant in the source fails this test
    /// until the record and the test are changed with it.
    const RECORDED_COMMIT_S_152: f64 = 10.123;
    const RECORDED_COMMIT_S_1520: f64 = 85.496;
    const RECORDED_SCAN_S: f64 = 1.579;
    const RECORDED_HOLD_S: u64 = 176;

    #[test]
    fn hold_s_is_derived_from_recorded_measurements() {
        assert_eq!(P95_COMMIT_S_SCALE1, RECORDED_COMMIT_S_152);
        assert_eq!(P95_COMMIT_S_SCALE10, RECORDED_COMMIT_S_1520);
        assert_eq!(P95_SCAN_S, RECORDED_SCAN_S);
        assert_eq!((FILES_SCALE1, FILES_SCALE10), (152, 1520));

        // D-02's formula on the largest measured scale, then its clamp.
        let unclamped = hold_for(RECORDED_COMMIT_S_1520, RECORDED_SCAN_S);
        assert_eq!(
            unclamped, 176,
            "ceil(2 x (85.496 + 1.579) + 1) = ceil(175.15)"
        );
        assert!(
            unclamped <= HOLD_CEILING_S,
            "D-03a is not triggered: the unclamped hold is inside the ceiling"
        );
        assert_eq!(HOLD_S, hold_clamped(unclamped));
        assert_eq!(HOLD_S, RECORDED_HOLD_S);

        // The fit through the two scales, recomputed from the literals.
        let per_file = (RECORDED_COMMIT_S_1520 - RECORDED_COMMIT_S_152) / (1520.0 - 152.0);
        let fixed = (RECORDED_COMMIT_S_152 - 152.0 * per_file).max(0.0);
        assert!(fixed > 0.0, "the floor at 0 did not apply");
        assert_eq!(HoldModel::MEASURED.commit_per_file_s, per_file);
        assert_eq!(HoldModel::MEASURED.commit_fixed_s, fixed);
        assert_eq!(HoldModel::MEASURED.p95_scan_s, RECORDED_SCAN_S);
        assert_eq!(HoldModel::MEASURED.hold_s, HOLD_S);
        // The fit goes through both measured points.
        assert!((HoldModel::MEASURED.estimate_commit_s(152) - RECORDED_COMMIT_S_152).abs() < 1e-9);
        assert!(
            (HoldModel::MEASURED.estimate_commit_s(1520) - RECORDED_COMMIT_S_1520).abs() < 1e-9
        );
    }

    #[test]
    fn hold_model_fits_the_measured_largest_tree() {
        let model = HoldModel::MEASURED;
        assert!(model.fits(1520), "the largest measured tree fits");
        assert!(model.fits(0));
        // The smallest change that does not fit; recorded in OQ5b-3.
        let first_over = (1520..4000)
            .find(|files| !model.fits(*files))
            .expect("some size does not fit");
        assert_eq!(
            first_over,
            1528,
            "1,527 files is the largest change that fits {} s; 1,528 needs {} s",
            model.hold_s,
            hold_for(model.estimate_commit_s(1528), model.p95_scan_s)
        );
        assert!(model.fits(first_over - 1));
        assert!(!model.fits(first_over));
        // The refusal reads the same numbers the hold was built from.
        assert_eq!(
            hold_for(model.estimate_commit_s(1520), model.p95_scan_s),
            HOLD_S
        );
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
        async fn restart_pending(&self) -> Result<Vec<String>, CoreError> {
            unused()
        }
        async fn redundancy_resync(&self) -> Result<(), CoreError> {
            unused()
        }
        async fn secret_names(
            &self,
            _provider: &str,
        ) -> Result<crate::client::deploy_api::SecretNameListing, CoreError> {
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
    async fn held_lock_acquires_the_config_lock_with_the_hold_and_the_smallest_wait() {
        let gateway = LockGateway::holding(HOLD as i64, None);
        let clock = ManualClock::new();
        let lock = lock_on(&gateway, &clock).await;
        assert_eq!(
            *gateway.acquired.lock().expect("acquired"),
            [(ScanKind::Config, ACQUIRE_WAIT_S, HOLD)]
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

    fn held_by(timestamp: i64, actor: &str, remaining: i64) -> LockObservation {
        LockObservation::Held(ScanLockInfo {
            timestamp: Some(timestamp),
            remaining,
            actor: Some(actor.to_string()),
            extra: BTreeMap::new(),
        })
    }

    #[tokio::test]
    async fn held_lock_another_actors_hold_is_not_ours() {
        let gateway = LockGateway::holding(30, Some(100));
        let clock = ManualClock::new();
        let lock = lock_on(&gateway, &clock).await;
        // The same actor name, a new hold: the timestamp tells them apart.
        *gateway.info.lock().expect("info") = held_by(200, "api-token:t:127.0.0.1", 59);
        assert_eq!(
            lock.check(&gateway, LOCK_MARGIN_S, &clock)
                .await
                .expect("check"),
            LockVerdict::NotHeld
        );
        // Another actor with the very same timestamp is not ours either.
        *gateway.info.lock().expect("info") = held_by(100, "api-token:other:127.0.0.1", 59);
        assert_eq!(
            lock.check_gateway(&gateway, 0).await.expect("check"),
            LockVerdict::NotHeld
        );
        // And our own hold reads as held again.
        *gateway.info.lock().expect("info") = held_by(100, "api-token:t:127.0.0.1", 29);
        assert_eq!(
            lock.check_gateway(&gateway, 0).await.expect("check"),
            LockVerdict::Held { remaining_s: 29 }
        );
    }

    #[tokio::test]
    async fn held_lock_unseen_after_the_grant_owns_no_later_lock() {
        // The lock read free right after the grant, so a lock seen later was
        // taken by someone else.
        let gateway = LockGateway::free();
        let clock = ManualClock::new();
        let lock = lock_on(&gateway, &clock).await;
        *gateway.info.lock().expect("info") = held_by(5, "api-token:t:127.0.0.1", 59);
        assert_eq!(
            lock.check(&gateway, LOCK_MARGIN_S, &clock)
                .await
                .expect("check"),
            LockVerdict::NotHeld
        );
    }

    #[tokio::test]
    async fn held_lock_check_gateway_ignores_the_local_clock() {
        let gateway = LockGateway::holding(30, Some(100));
        let clock = ManualClock::new();
        let lock = lock_on(&gateway, &clock).await;
        clock.advance(Duration::from_secs(HOLD * 2));
        assert!(lock.local_remaining_s(&clock) < 0);
        assert_eq!(
            lock.check_gateway(&gateway, LOCK_MARGIN_S)
                .await
                .expect("check"),
            LockVerdict::Held { remaining_s: 30 }
        );
    }

    #[tokio::test]
    async fn held_lock_wait_for_lapse_returns_when_the_hold_is_gone() {
        let gateway = LockGateway::holding(30, Some(100));
        let clock = ManualClock::new();
        let lock = lock_on(&gateway, &clock).await;
        // Another actor's lock ends the wait at once: it is not ours to wait for.
        *gateway.info.lock().expect("info") = held_by(7, "api-token:other:127.0.0.1", 59);
        lock.wait_for_lapse(&gateway, Duration::from_millis(1))
            .await
            .expect("not ours");
        *gateway.info.lock().expect("info") = LockObservation::Free;
        lock.wait_for_lapse(&gateway, Duration::from_millis(1))
            .await
            .expect("free");
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
