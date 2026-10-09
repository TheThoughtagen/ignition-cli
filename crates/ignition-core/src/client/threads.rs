//! Thread-diagnostics capability models — the verified
//! `/data/api/v1/diagnostics/threads/` endpoints (the Gateway web UI's
//! Status → Diagnostics → Threads page, wire-shaped from the live
//! 8.3.3 gateway's own openapi spec, `/tmp/itop-live/spec.json`).
//!
//! **Tolerance is the contract here**: the spec's formatted-dump schema
//! carries the caveat "Not all fields in response shape are
//! guaranteed", so EVERY field defaults or rides an `Option` — a
//! future/trimmed entry must parse, never refuse (the Pitfall-2 rule
//! applied to a diagnostic snapshot). Partial data renders honestly;
//! a parse refusal is the only unrecoverable shape.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// GET path of the formatted thread dump (the legacy-compat shape the
/// web UI renders — one entry per JVM thread with cpu usage, stack
/// trace, and monitor locks).
pub(crate) const THREAD_DUMP_FORMATTED_PATH: &str =
    "/data/api/v1/diagnostics/threads/dump/formatted";

/// GET path of the deadlocked-thread id list (the JVM's own deadlock
/// detection, ids matching [`ThreadInfo::id`]).
pub(crate) const THREAD_DEADLOCKS_PATH: &str = "/data/api/v1/diagnostics/threads/deadlocks";

/// GET `/data/api/v1/diagnostics/threads/dump/formatted` — the whole
/// dump: a VM version string and one entry per thread.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormattedThreadDump {
    /// The dump format / VM version string (verbatim passthrough).
    #[serde(default)]
    pub version: String,
    /// One entry per JVM thread (order: the gateway's — the itop view
    /// applies its own sort).
    #[serde(default)]
    pub threads: Vec<ThreadInfo>,
    /// Unknown keys round-trip (version tolerance).
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// One JVM thread from the formatted dump. Every field tolerates
/// absence (spec caveat: "not all fields guaranteed").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadInfo {
    /// The thread's Java name (e.g. `Perspective-Worker-3`).
    #[serde(default)]
    pub name: String,
    /// The JVM thread id (matches the deadlocks list's ids).
    #[serde(default)]
    pub id: Option<i64>,
    /// `java.lang.Thread.State` verbatim (`RUNNABLE`, `WAITING`, …).
    #[serde(default)]
    pub state: String,
    /// Daemon flag (`false` when absent).
    #[serde(default)]
    pub daemon: bool,
    /// System-thread marker string, verbatim (empty when absent).
    #[serde(default)]
    pub system: String,
    /// Scope string, verbatim (empty when absent).
    #[serde(default)]
    pub scope: String,
    /// CPU usage — PERCENT scale per the web UI's column (absent when
    /// the gateway doesn't report it).
    #[serde(rename = "cpuUsage", default)]
    pub cpu_usage: Option<f64>,
    /// The monitor the thread is waiting to acquire, when blocked
    /// (`waitingFor` — the gateway's verbatim camel key).
    #[serde(rename = "waitingFor", default)]
    pub waiting_for: Option<WaitingFor>,
    /// The thread's stack trace, top frame first.
    #[serde(default)]
    pub stacktrace: Vec<String>,
    /// Monitors this thread HOLDS with the frame that locked them —
    /// the contention evidence the web UI shows as "locks X"
    /// (`lockedMonitors` — the gateway's verbatim camel key).
    #[serde(rename = "lockedMonitors", default)]
    pub locked_monitors: Vec<LockedMonitor>,
    /// Unknown keys round-trip (version tolerance).
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// The monitor a blocked thread is waiting to acquire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaitingFor {
    /// The lock's identifying string (verbatim).
    #[serde(default)]
    pub lock: Option<String>,
    /// Unknown keys round-trip.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// A monitor the thread HOLDS, with the frame that locked it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LockedMonitor {
    /// The lock's identifying string (verbatim).
    #[serde(default)]
    pub lock: Option<String>,
    /// The stack frame that acquired the lock.
    #[serde(default)]
    pub frame: Option<String>,
    /// Unknown keys round-trip.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// GET `/data/api/v1/diagnostics/threads/deadlocks` — the wire body is
/// `{"deadlocks": [id, …]}`; the ids match [`ThreadInfo::id`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeadlocksWire {
    /// Deadlocked thread ids (empty when the JVM reports none — the
    /// healthy answer, not an error).
    #[serde(default)]
    pub deadlocks: Vec<i64>,
    /// Unknown keys round-trip (version tolerance).
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::{DeadlocksWire, FormattedThreadDump};

    /// The spec's schema shape, exercised field by field: camelCase
    /// `cpuUsage`, the nested `waitingFor.lock`, the `stacktrace` and
    /// `lockedMonitors` arrays.
    #[test]
    fn formatted_dump_parses_the_spec_shape() {
        let body = serde_json::json!({
            "version": "dump-version-1",
            "threads": [
                {
                    "name": "Perspective-Worker-3",
                    "id": 42,
                    "state": "RUNNABLE",
                    "daemon": true,
                    "system": "gateway",
                    "scope": "web",
                    "cpuUsage": 3.75,
                    "waitingFor": null,
                    "stacktrace": [
                        "java.base@21.0.5/java.lang.Thread.sleepNanos",
                        "com.inductiveautomation.perspective.gateway.Worker.run"
                    ],
                    "lockedMonitors": [
                        {"lock": "<0x1a2b> (a com.inductiveautomation...) ", "frame": "Worker.lock - line 88"}
                    ]
                },
                {
                    "name": "pool-2-thread-1",
                    "id": 7,
                    "state": "BLOCKED",
                    "daemon": false,
                    "cpuUsage": 0.0,
                    "waitingFor": {"lock": "<0x1a2b> (a com.inductiveautomation...)"}
                }
            ]
        });
        let dump: FormattedThreadDump =
            serde_json::from_value(body).expect("the spec shape must parse");
        assert_eq!(dump.version, "dump-version-1");
        assert_eq!(dump.threads.len(), 2);

        let runnable = &dump.threads[0];
        assert_eq!(runnable.name, "Perspective-Worker-3");
        assert_eq!(runnable.id, Some(42));
        assert_eq!(runnable.state, "RUNNABLE");
        assert!(runnable.daemon);
        assert_eq!(runnable.system, "gateway");
        assert_eq!(runnable.scope, "web");
        assert!((runnable.cpu_usage.unwrap() - 3.75).abs() < f64::EPSILON);
        assert_eq!(runnable.stacktrace.len(), 2);
        assert_eq!(
            runnable.locked_monitors[0].lock.as_deref(),
            Some("<0x1a2b> (a com.inductiveautomation...) ")
        );
        assert_eq!(
            runnable.locked_monitors[0].frame.as_deref(),
            Some("Worker.lock - line 88")
        );

        let blocked = &dump.threads[1];
        assert_eq!(blocked.state, "BLOCKED");
        assert!(
            !blocked.daemon,
            "absent daemon defaults false"
        );
        assert_eq!(
            blocked.waiting_for.as_ref().and_then(|w| w.lock.clone()).as_deref(),
            Some("<0x1a2b> (a com.inductiveautomation...)"),
            "the waiting-for lock parses through the nested object"
        );
        assert!(blocked.locked_monitors.is_empty());
        assert!(blocked.stacktrace.is_empty(), "absent arrays default empty");
    }

    /// The spec caveat ("not all fields are guaranteed") is honored:
    /// an entry of NOTHING but a name parses, every absent field takes
    /// its honest default, and unknown keys ride the flatten
    /// passthrough.
    #[test]
    fn thread_fields_are_all_tolerant() {
        let dump: FormattedThreadDump = serde_json::from_value(serde_json::json!({
            "threads": [{"name": "minimal"}],
            "zzFutureKey": {"future": true}
        }))
        .expect("a name-only entry must parse");
        let thread = &dump.threads[0];
        assert_eq!(thread.name, "minimal");
        assert_eq!(thread.id, None);
        assert_eq!(thread.state, "");
        assert!(!thread.daemon);
        assert_eq!(thread.cpu_usage, None);
        assert!(thread.waiting_for.is_none());
        assert!(thread.stacktrace.is_empty());
        assert!(thread.locked_monitors.is_empty());
        assert!(
            dump.extra.contains_key("zzFutureKey"),
            "unknown keys round-trip"
        );

        let round = serde_json::to_value(thread).expect("serialize");
        assert_eq!(round["name"], "minimal", "gateway-native key on the way out");
        assert_eq!(round["cpuUsage"], serde_json::Value::Null, "camelCase rename");
    }

    /// The deadlocks body: `{"deadlocks": [id, …]}` — ids match the
    /// dump's `id` field, and an EMPTY list is the healthy answer.
    #[test]
    fn deadlocks_wire_parses_the_id_list() {
        let wire: DeadlocksWire =
            serde_json::from_value(serde_json::json!({"deadlocks": [7, 42]}))
                .expect("the deadlocks shape must parse");
        assert_eq!(wire.deadlocks, vec![7, 42]);

        let healthy: DeadlocksWire = serde_json::from_value(serde_json::json!({}))
            .expect("an absent list is the healthy answer");
        assert!(healthy.deadlocks.is_empty());
    }
}
