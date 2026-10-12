//! Scan-lock and scan models plus the two pure verdicts built on them.
//!
//! Pure on purpose: no I/O and no clock. The wire calls live in
//! [`super::deploy_api`]; this file only decides what a reading means,
//! so every verdict is testable without sleeping.
//!
//! Wire facts (8.3.6, `20-LIVE-CAPTURES.md`, cited by block id):
//!
//! - `GET /data/api/v1/scan-lock/{kind}` answers 200 with
//!   `{timestamp, remaining, actor}` while a lock is held and 204 with no
//!   body when none is (C1a, C1c, D9).
//! - `timestamp` is an OPAQUE value, not epoch milliseconds (380308558 in
//!   C1c), constant for one hold and different for each new hold. It is
//!   never read here and never compared with a clock.
//! - `remaining` is integer seconds, floored: right after an acquire it
//!   reads `holdTimeout - 1` (C1b: 59 for 60).
//! - `actor` is the holder's opaque identity string
//!   (`api-token:<name>:<ip>`, C1c); it is echoed, never parsed.
//! - A scan releases the lock at trigger time, for the holder and for any
//!   other actor alike (C1j, C1i; D4, D12).
//! - `GET /data/api/v1/scan/{kind}` answers `{scanActive,
//!   lastScanTimestamp, lastScanDuration}`; `lastScanTimestamp` is
//!   epoch-ms of the gateway clock (C1i). The scan POST's own 200 body is
//!   a trigger-time snapshot (`scanActive: true`, OLD timestamp) and is
//!   never completion (D4).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Which scan surface a lock or scan call addresses. The live OpenAPI
/// declares both kinds (C1h); only `config` was exercised live, so
/// `projects` is wiremock-pinned only (Drift and limits).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanKind {
    /// Gateway configuration scan.
    Config,
    /// Project resource scan.
    Projects,
}

impl ScanKind {
    /// The path segment and error-payload spelling of this kind.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Config => "config",
            Self::Projects => "projects",
        }
    }
}

/// `/data/api/v1/scan-lock/{kind}` (GET info, POST acquire).
pub fn scan_lock_path(kind: ScanKind) -> String {
    format!("/data/api/v1/scan-lock/{}", kind.as_str())
}

/// POST body of a lock acquire. Field order is the wire order, so the
/// recorded request body is deterministic. The gateway answers a body-less
/// acquire with a 500 NullPointerException (C1b, D9), so the client always
/// sends this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct LockRequest {
    /// Seconds the gateway may wait for the lock. Never observed to wait:
    /// a competing acquire is an immediate 409 (D1).
    #[serde(rename = "acquireTimeout")]
    pub acquire_timeout: u64,
    /// Seconds the lock is held before it expires on its own.
    #[serde(rename = "holdTimeout")]
    pub hold_timeout: u64,
}

/// Body of a held lock (C1c). Lenient: unknown keys ride in `extra`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanLockInfo {
    /// Opaque per-hold value. Never read by any verdict (see module doc).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    /// Whole seconds until the hold expires, floored. An absent key reads
    /// as 0, which the verdict treats as expiring: the cautious reading.
    #[serde(default)]
    pub remaining: i64,
    /// The holder, as the gateway names it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    /// Keys this client does not model.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// What `GET scan-lock/{kind}` saw.
#[derive(Debug, Clone, PartialEq)]
pub enum LockObservation {
    /// 200 with a body: some actor holds the lock.
    Held(ScanLockInfo),
    /// 204: no lock is held.
    Free,
}

/// The decision a caller makes from one [`LockObservation`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockVerdict {
    /// Held with at least the requested margin left.
    Held {
        /// Seconds left, as the gateway reported them.
        remaining_s: i64,
    },
    /// Held, but with less than the margin left: it may lapse mid-use.
    Expiring {
        /// Seconds left, as the gateway reported them.
        remaining_s: i64,
    },
    /// No lock is held. Deliberately not named `Expired`: a 204 is also
    /// the NORMAL state after a successful scan trigger, because a scan
    /// releases the lock at trigger time for any actor (C1j, D4, D12).
    /// The client cannot tell "expired" from "released by a scan"; the
    /// caller's context decides which one it means.
    NotHeld,
}

/// Decide what `obs` means. `Held` when `remaining >= margin_s`
/// (inclusive), `Expiring` below it, `NotHeld` for a free lock.
///
/// Takes no clock and never reads `timestamp`, so a gateway timestamp at
/// any distance from local time changes nothing (proptest below).
/// `remaining` is floored whole seconds, so a margin under 2 s carries no
/// information.
pub fn lock_verdict(obs: &LockObservation, margin_s: i64) -> LockVerdict {
    match obs {
        LockObservation::Free => LockVerdict::NotHeld,
        LockObservation::Held(info) if info.remaining >= margin_s => LockVerdict::Held {
            remaining_s: info.remaining,
        },
        LockObservation::Held(info) => LockVerdict::Expiring {
            remaining_s: info.remaining,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn held(timestamp: Option<i64>, remaining: i64) -> LockObservation {
        LockObservation::Held(ScanLockInfo {
            timestamp,
            remaining,
            actor: Some("api-token:cap-a:192.168.97.1".to_string()),
            extra: BTreeMap::new(),
        })
    }

    #[test]
    fn lock_verdict_boundary_is_inclusive() {
        assert_eq!(
            lock_verdict(&held(None, 5), 5),
            LockVerdict::Held { remaining_s: 5 }
        );
        assert_eq!(
            lock_verdict(&held(None, 4), 5),
            LockVerdict::Expiring { remaining_s: 4 }
        );
        assert_eq!(
            lock_verdict(&LockObservation::Free, 5),
            LockVerdict::NotHeld
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        /// The skewed-clock test: the gateway `timestamp` may sit at any
        /// distance from local time and nothing changes.
        #[test]
        fn lock_verdict_ignores_timestamp(
            timestamp in any::<i64>(),
            remaining in -10i64..=10_000,
            margin in 0i64..=60,
        ) {
            prop_assert_eq!(
                lock_verdict(&held(Some(timestamp), remaining), margin),
                lock_verdict(&held(None, remaining), margin)
            );
        }
    }

    /// Structural pin: the non-test text of this file reads no clock. The
    /// forbidden list lives here, after the cut, so it cannot trip on
    /// itself.
    #[test]
    fn scan_rs_reads_no_clock() {
        let source = include_str!("scan.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("split yields a head");
        let code: String = production
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for forbidden in ["SystemTime", "Instant", "Utc", "chrono", "jiff", "now("] {
            assert!(
                !code.contains(forbidden),
                "scan.rs must not read a clock, found {forbidden:?}"
            );
        }
    }

    #[test]
    fn captured_bodies_parse() {
        // C1c held body, verbatim.
        let info: ScanLockInfo = serde_json::from_str(
            "{\n  \"timestamp\": 380308558,\n  \"remaining\": 59,\n  \"actor\": \"api-token:cap-a:192.168.97.1\"\n}",
        )
        .expect("C1c parses");
        assert_eq!(info.remaining, 59);
        assert_eq!(info.timestamp, Some(380_308_558));
    }

    #[test]
    fn paths_follow_the_kind() {
        assert_eq!(
            scan_lock_path(ScanKind::Projects),
            "/data/api/v1/scan-lock/projects"
        );
    }
}
