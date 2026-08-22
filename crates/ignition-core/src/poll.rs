//! The ONE wait/retry engine (02-04) — shared by the log tail here and
//! by 02-05's `wait` / `restart --wait`. Deliberately ~40 lines + tests
//! instead of a retry framework (STACK.md rejected reqwest-middleware;
//! 02-RESEARCH §Wait-loop pattern).
//!
//! Semantics (research-locked):
//! - adaptive interval: ×1.5 growth clamped to `[interval, 30 s]`
//!   (igw-cli's verified pattern);
//! - `Network` and `GatewayRestarting` are RETRIED (transient: the
//!   webserver answers 503 mid-restart and connections flap);
//! - `Auth` is NEVER retried — retrying a rejected token cannot
//!   succeed; fail fast (exit 5);
//! - any other error aborts;
//! - deadline expiry → `CoreError::Network`-class timeout (exit 4,
//!   `network_error` slug — NO new variant; the source is `None` and
//!   `url` carries the subject + last observation).
//!
//! `deadline = Duration::MAX` runs until the process is killed — the
//! documented Ctrl-C contract for `logs -f` (default kill, no envelope).

use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, Instant};

use crate::error::CoreError;

/// The research ceiling for the adaptive backoff (×1.5 clamped to
/// [interval, 30 s]) — even a user-provided `max` never exceeds it.
const BACKOFF_CEILING: Duration = Duration::from_secs(30);

/// What one probe reports.
#[derive(Debug, PartialEq, Eq)]
pub enum PollState<T> {
    /// Condition met — `poll` returns the value.
    Done(T),
    /// Not yet; the optional last observation rides the deadline error.
    Pending(Option<String>),
}

/// Poll tuning. `Default`: 2 s interval, 30 s clamp, 120 s deadline.
#[derive(Debug, Clone)]
pub struct PollConfig {
    /// What is being waited on — the deadline error names it (e.g.
    /// `"log tail (GET /data/api/v1/logs)"`, `"/StatusPing readiness"`).
    pub subject: String,
    /// Wait between polls; also the backoff FLOOR (never shrinks below).
    pub interval: Duration,
    /// Backoff clamp ceiling (additionally capped at 30 s).
    pub max: Duration,
    /// Total budget; `Duration::MAX` = until the process is killed.
    pub deadline: Duration,
}

impl Default for PollConfig {
    fn default() -> Self {
        Self {
            subject: "poll".to_string(),
            interval: Duration::from_secs(2),
            max: BACKOFF_CEILING,
            deadline: Duration::from_secs(120),
        }
    }
}

/// One probe call: a boxed future borrowing the probe closure, so the
/// closure can carry mutable state (the tail's cursor and sink) across
/// iterations without naming an unnameable future type.
pub type Probe<'a, T> = Pin<Box<dyn Future<Output = Result<PollState<T>, CoreError>> + 'a>>;

/// The adaptive-interval step: ×1.5 growth clamped to `[floor, ceiling]`.
/// Pure so the backoff sequence is unit-testable without sleeping.
fn next_interval(current: Duration, floor: Duration, ceiling: Duration) -> Duration {
    current.mul_f64(1.5).clamp(floor, ceiling)
}

/// Poll `probe` until `Done`, the deadline expires, or an unretryable
/// error fires (see module docs for the retry matrix). The FIRST probe
/// runs immediately — no initial sleep.
///
/// `state` is the probe's own mutable scratch (owned by the loop,
/// lent fresh to every call) — the pattern that lets a borrowing
/// async closure (`FnMut(&'a mut S) -> Probe<'a, T>`) carry a cursor
/// or sink across iterations without naming an unnameable future
/// type. `wait`-style callers pass `()`.
pub async fn poll<T, S, F>(cfg: PollConfig, state: S, mut probe: F) -> Result<T, CoreError>
where
    F: for<'a> FnMut(&'a mut S) -> Probe<'a, T>,
{
    let ceiling = cfg.max.min(BACKOFF_CEILING);
    let started = Instant::now();
    let mut interval = cfg.interval;
    let mut state = state;
    let mut last_observation: Option<String> = None;
    loop {
        match probe(&mut state).await {
            Ok(PollState::Done(value)) => return Ok(value),
            Ok(PollState::Pending(observation)) => last_observation = observation,
            // NEVER retried: a rejected token cannot succeed on retry.
            Err(err) if matches!(err, CoreError::Auth { .. }) => return Err(err),
            // Transient — retried until Done or deadline; a Network flap
            // keeps the last Pending observation for the deadline message.
            Err(CoreError::Network { .. } | CoreError::GatewayRestarting { .. }) => {}
            // Any other class aborts immediately.
            Err(other) => return Err(other),
        }
        let Some(remaining) = cfg.deadline.checked_sub(started.elapsed()) else {
            return Err(deadline_error(&cfg, started.elapsed(), &last_observation));
        };
        if remaining.is_zero() {
            return Err(deadline_error(&cfg, started.elapsed(), &last_observation));
        }
        tokio::time::sleep(interval.min(remaining)).await;
        interval = next_interval(interval, cfg.interval, ceiling);
    }
}

/// Deadline expiry: the `network_error` slug (exit 4) carrying the
/// subject and the last observation — reusing the Network variant with
/// `source: None` (a poll timeout has no transport error to show).
fn deadline_error(cfg: &PollConfig, waited: Duration, last: &Option<String>) -> CoreError {
    let observation = last
        .as_deref()
        .map(|last| format!("; last observation: {last}"))
        .unwrap_or_default();
    CoreError::Network {
        url: format!("{} — timed out after {waited:?}{observation}", cfg.subject),
        source: None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;
    use std::time::Duration;

    use super::{PollConfig, PollState, next_interval, poll};
    use crate::error::CoreError;

    /// A real transport error (instant loopback refusal) —
    /// `reqwest::Error` has no public constructor.
    async fn transport_error() -> reqwest::Error {
        reqwest::get("http://127.0.0.1:1")
            .await
            .expect_err("dead port refuses")
    }

    /// Scripted probe steps, served in order.
    struct FakeProbe {
        steps: Mutex<VecDeque<Step>>,
    }

    enum Step {
        Done(u32),
        Pending(Option<String>),
        Network,
        Restarting,
        Auth,
        NotFound,
    }

    impl FakeProbe {
        fn with(steps: Vec<Step>) -> Self {
            Self {
                steps: Mutex::new(steps.into()),
            }
        }

        async fn next(&self) -> Result<PollState<u32>, CoreError> {
            // Pop BEFORE matching: the guard must drop before any arm
            // awaits (clippy: await-holding-lock).
            let step = self.steps.lock().unwrap().pop_front();
            match step {
                Some(Step::Done(value)) => Ok(PollState::Done(value)),
                Some(Step::Pending(observation)) => Ok(PollState::Pending(observation)),
                Some(Step::Network) => Err(CoreError::Network {
                    url: "http://127.0.0.1:1".into(),
                    source: Some(transport_error().await),
                }),
                Some(Step::Restarting) => Err(CoreError::GatewayRestarting {
                    endpoint: Some("http://127.0.0.1:1/data/api/v1/overview".into()),
                }),
                Some(Step::Auth) => Err(CoreError::Auth {
                    status: 401,
                    endpoint: None,
                }),
                Some(Step::NotFound) => Err(CoreError::NotFound { endpoint: None }),
                None => panic!("scripted steps exhausted"),
            }
        }
    }

    /// The counting closure shape every scripted test shares: the
    /// counter is an owned `Arc` clone inside the future (no borrow to
    /// outlive the HRTB), the state is lent per iteration, the step
    /// serves in order.
    fn counting_probe(
        calls: std::sync::Arc<Mutex<usize>>,
    ) -> impl for<'a> FnMut(&'a mut FakeProbe) -> super::Probe<'a, u32> {
        move |rig| {
            let calls = std::sync::Arc::clone(&calls);
            Box::pin(async move {
                *calls.lock().unwrap() += 1;
                rig.next().await
            })
        }
    }

    fn counted_rig(steps: Vec<Step>) -> (FakeProbe, std::sync::Arc<Mutex<usize>>) {
        let calls = std::sync::Arc::new(Mutex::new(0usize));
        (FakeProbe::with(steps), std::sync::Arc::clone(&calls))
    }

    fn fast_cfg() -> PollConfig {
        PollConfig {
            subject: "test wait".into(),
            interval: Duration::from_millis(1),
            deadline: Duration::from_millis(500),
            ..PollConfig::default()
        }
    }

    /// First probe Done → value returned, exactly one call.
    #[tokio::test]
    async fn success_first_poll() {
        let (rig, calls) = counted_rig(vec![Step::Done(7)]);
        let value = poll(fast_cfg(), rig, counting_probe(calls.clone()))
            .await
            .expect("immediate Done");
        assert_eq!(value, 7);
        assert_eq!(*calls.lock().unwrap(), 1);
    }

    /// Network and GatewayRestarting are retried; Done eventually wins.
    #[tokio::test]
    async fn transient_errors_are_retried_then_done() {
        let (rig, calls) = counted_rig(vec![
            Step::Network,
            Step::Restarting,
            Step::Pending(Some("almost".into())),
            Step::Done(3),
        ]);
        let value = poll(fast_cfg(), rig, counting_probe(calls.clone()))
            .await
            .expect("transients retried to Done");
        assert_eq!(value, 3);
        assert_eq!(*calls.lock().unwrap(), 4);
    }

    /// Auth NEVER retries — exactly one call, the error propagates.
    #[tokio::test]
    async fn auth_fails_immediately() {
        let (rig, calls) = counted_rig(vec![Step::Auth, Step::Done(1)]);
        let err = poll(fast_cfg(), rig, counting_probe(calls.clone()))
            .await
            .expect_err("auth aborts");
        assert!(matches!(err, CoreError::Auth { status: 401, .. }));
        assert_eq!(*calls.lock().unwrap(), 1, "no retry on auth");
    }

    /// Any other error class aborts immediately (no retry).
    #[tokio::test]
    async fn other_errors_abort_immediately() {
        let (rig, calls) = counted_rig(vec![Step::NotFound, Step::Done(1)]);
        let err = poll(fast_cfg(), rig, counting_probe(calls.clone()))
            .await
            .expect_err("not-found aborts");
        assert!(matches!(err, CoreError::NotFound { .. }));
        assert_eq!(*calls.lock().unwrap(), 1);
    }

    /// Deadline expiry: Network class (exit 4, `network_error` slug) —
    /// NO new variant — with the subject AND the last observation in
    /// the message, and no transport source.
    #[tokio::test]
    async fn deadline_expiry_is_network_class_with_observation() {
        let calls = Mutex::new(0usize);
        let err = poll(
            PollConfig {
                subject: "test readiness".into(),
                interval: Duration::from_millis(1),
                deadline: Duration::from_millis(20),
                ..PollConfig::default()
            },
            &mut (),
            |()| {
                Box::pin(async {
                    *calls.lock().unwrap() += 1;
                    Ok(PollState::<()>::Pending(Some("obs-42".into())))
                })
            },
        )
        .await
        .expect_err("deadline must expire");
        assert!(
            matches!(&err, CoreError::Network { source: None, .. }),
            "deadline = Network with no transport source: {err}"
        );
        assert_eq!(err.exit_code(), 4);
        assert_eq!(err.code(), "network_error");
        let message = err.to_string();
        assert!(
            message.contains("test readiness"),
            "subject named: {message}"
        );
        assert!(
            message.contains("obs-42"),
            "last observation carried: {message}"
        );
        assert!(message.contains("timed out"), "timeout named: {message}");
        assert!(*calls.lock().unwrap() > 1, "multiple polls before expiry");
    }

    /// The backoff sequence: 2 s → 3 s → 4.5 s → … clamped at 30 s,
    /// never below the interval floor, and a custom smaller ceiling
    /// holds too (the 30 s research cap is an upper bound).
    #[test]
    fn backoff_sequence_math() {
        let floor = Duration::from_secs(2);
        let ceiling = Duration::from_secs(30);
        let mut current = floor;
        let mut sequence = Vec::new();
        for _ in 0..12 {
            sequence.push(current);
            current = next_interval(current, floor, ceiling);
        }
        assert_eq!(
            sequence,
            vec![
                Duration::from_secs(2),
                Duration::from_secs(3),
                Duration::from_secs_f64(4.5),
                Duration::from_secs_f64(6.75),
                Duration::from_secs_f64(10.125),
                Duration::from_secs_f64(15.1875),
                Duration::from_secs_f64(22.781_25),
                Duration::from_secs(30), // 34.17 s clamped
                Duration::from_secs(30),
                Duration::from_secs(30),
                Duration::from_secs(30),
                Duration::from_secs(30),
            ],
            "×1.5 growth clamped to [interval, 30 s]"
        );
        // Custom ceiling below 30 s also holds (and the floor never
        // lets the interval shrink).
        let tight = next_interval(
            Duration::from_secs(3),
            Duration::from_secs(2),
            Duration::from_secs(4),
        );
        assert_eq!(tight, Duration::from_secs(4));
        let floored = next_interval(
            Duration::from_secs(2),
            Duration::from_secs(2),
            Duration::from_secs(4),
        );
        assert_eq!(floored, Duration::from_secs(3), "3.0 s — floor unchanged");
    }
}
