//! Metrics capability models (02-02, HLTH-07) — the verified
//! `/data/api/v1/systemPerformance/` endpoints (02-RESEARCH §Metrics).
//!
//! PATH WARNING pinned by wiremock tests: the real endpoints are
//! `systemPerformance/currentGauges|charts|threads`. The ignition-mcp
//! client's `/data/api/v1/system/metrics` path is an invention — 404 on
//! a real gateway — do not copy it.
//!
//! Scale honesty (the research rule "normalize in the model, not in
//! users' eyes"): [`CurrentGauges::cpu`] is PERCENT (live: `4.88`);
//! [`crate::client::status::Overview::cpu`] is a 0–1 FRACTION (live:
//! `0.0031`). Same concept, two endpoints, two scales — documented at
//! both fields, never silently converted.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// GET path of the current-gauges capability.
pub(crate) const CURRENT_GAUGES_PATH: &str = "/data/api/v1/systemPerformance/currentGauges";
/// GET path of the historic charts capability.
pub(crate) const CHARTS_PATH: &str = "/data/api/v1/systemPerformance/charts";
/// GET path of the thread-execution counts capability.
pub(crate) const THREADS_PATH: &str = "/data/api/v1/systemPerformance/threads";

/// GET `/data/api/v1/systemPerformance/currentGauges` — live capture:
/// `{cpu: 4.88, heapMemory: 240000000, maxMemory: 1073741824}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurrentGauges {
    /// CPU utilization in PERCENT (0–100). NOT the 0–1 fraction
    /// `/overview` reports — two endpoints, two scales, both documented.
    pub cpu: f64,
    /// Heap memory in use, bytes.
    #[serde(rename = "heapMemory")]
    pub heap_memory: i64,
    /// Max heap (`-Xmx`), bytes.
    #[serde(rename = "maxMemory")]
    pub max_memory: i64,
    /// Unknown keys (`nonHeapMemory`, …) round-trip.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// GET `/data/api/v1/systemPerformance/threads` — live capture:
/// `{running: 32, waiting: 39, timedWaiting: 51, blocked: 0}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadCounts {
    /// Threads currently executing.
    #[serde(default)]
    pub running: i64,
    /// Threads waiting to acquire a monitor.
    #[serde(default)]
    pub waiting: i64,
    /// Threads in `Thread.sleep`/park-style waits.
    #[serde(rename = "timedWaiting", default)]
    pub timed_waiting: i64,
    /// Threads blocked on monitor entry.
    #[serde(default)]
    pub blocked: i64,
    /// Unknown keys round-trip.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// One historic datapoint of the charts capability.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Datapoint {
    /// History series id.
    #[serde(rename = "histId", default)]
    pub hist_id: i64,
    /// Epoch **MILLISECONDS** (live: `1787346747022`).
    pub timestamp: i64,
    /// cpu series: PERCENT; memory series: bytes.
    pub value: f64,
}

/// GET `/data/api/v1/systemPerformance/charts` — historic datapoints.
///
/// The WIRE shape nests the memory series:
/// `{cpuChartDatapoints: […],
///   memoryChartDatapoints: {heapMemoryDatapoints: […],
///                            nonHeapMemoryDatapoints: […]}}`
/// (openapi + live capture). The model is FLAT with serde renames onto
/// the gateway-native series names, so deserialization accepts the
/// nested wire body (a manual impl walks `memoryChartDatapoints`) while
/// serialization stays a flat, agent-friendly camelCase shape.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PerformanceCharts {
    /// CPU percent per sample.
    #[serde(rename = "cpuChartDatapoints")]
    pub cpu_datapoints: Vec<Datapoint>,
    /// Heap memory bytes per sample.
    #[serde(rename = "heapMemoryDatapoints")]
    pub heap_memory_datapoints: Vec<Datapoint>,
    /// Non-heap memory bytes per sample.
    #[serde(rename = "nonHeapMemoryDatapoints")]
    pub non_heap_memory_datapoints: Vec<Datapoint>,
}

impl<'de> Deserialize<'de> for PerformanceCharts {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        /// The literal wire shape (nesting under `memoryChartDatapoints`).
        #[derive(Deserialize)]
        struct Wire {
            #[serde(default, rename = "cpuChartDatapoints")]
            cpu: Vec<Datapoint>,
            #[serde(default, rename = "memoryChartDatapoints")]
            memory: WireMemory,
        }
        #[derive(Deserialize, Default)]
        struct WireMemory {
            #[serde(default, rename = "heapMemoryDatapoints")]
            heap: Vec<Datapoint>,
            #[serde(default, rename = "nonHeapMemoryDatapoints")]
            non_heap: Vec<Datapoint>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Ok(Self {
            cpu_datapoints: wire.cpu,
            heap_memory_datapoints: wire.memory.heap,
            non_heap_memory_datapoints: wire.memory.non_heap,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{CurrentGauges, Datapoint, PerformanceCharts, ThreadCounts};

    /// The exact live capture (02-RESEARCH §Metrics): cpu is PERCENT —
    /// and stays percent on round-trip (no fraction conversion).
    #[test]
    fn current_gauges_parses_the_live_capture() {
        let body = serde_json::json!({
            "cpu": 4.88,
            "heapMemory": 240000000i64,
            "maxMemory": 1073741824i64
        });
        let gauges: CurrentGauges =
            serde_json::from_value(body).expect("live gauges shape must parse");
        assert!((gauges.cpu - 4.88).abs() < f64::EPSILON, "percent");
        assert_eq!(gauges.heap_memory, 240000000);
        assert_eq!(gauges.max_memory, 1073741824);

        let round = serde_json::to_value(&gauges).expect("serialize");
        assert_eq!(round["cpu"], 4.88, "cpu stays percent");
        assert_eq!(round["heapMemory"], 240000000i64, "gateway-native key");
        assert_eq!(round["maxMemory"], 1073741824i64);
    }

    /// The exact live thread counts, including the camelCase
    /// `timedWaiting` rename.
    #[test]
    fn thread_counts_parses_the_live_capture() {
        let counts: ThreadCounts = serde_json::from_value(serde_json::json!({
            "running": 32, "waiting": 39, "timedWaiting": 51, "blocked": 0
        }))
        .expect("live threads shape must parse");
        assert_eq!(counts.running, 32);
        assert_eq!(counts.waiting, 39);
        assert_eq!(counts.timed_waiting, 51);
        assert_eq!(counts.blocked, 0);

        let round = serde_json::to_value(&counts).expect("serialize");
        assert_eq!(round["timedWaiting"], 51, "gateway-native key");
    }

    /// The charts body deserializes from the NESTED wire shape (one
    /// datapoint per series) and serializes FLAT under the
    /// gateway-native series names.
    #[test]
    fn charts_parse_nested_wire_and_serialize_flat() {
        let wire = serde_json::json!({
            "cpuChartDatapoints": [
                {"histId": 1, "timestamp": 1787346747022i64, "value": 4.88}
            ],
            "memoryChartDatapoints": {
                "heapMemoryDatapoints": [
                    {"histId": 2, "timestamp": 1787346747022i64, "value": 240000000.0}
                ],
                "nonHeapMemoryDatapoints": [
                    {"histId": 3, "timestamp": 1787346747022i64, "value": 52000000.0}
                ]
            }
        });
        let charts: PerformanceCharts =
            serde_json::from_value(wire).expect("nested wire shape must parse");
        assert_eq!(charts.cpu_datapoints.len(), 1);
        assert_eq!(charts.heap_memory_datapoints.len(), 1);
        assert_eq!(charts.non_heap_memory_datapoints.len(), 1);
        let Datapoint {
            hist_id,
            timestamp,
            value,
        } = &charts.cpu_datapoints[0];
        assert_eq!((*hist_id, *timestamp), (1, 1787346747022));
        assert!((*value - 4.88).abs() < f64::EPSILON);

        let flat = serde_json::to_value(&charts).expect("serialize");
        assert_eq!(flat["cpuChartDatapoints"][0]["histId"], 1);
        assert!(
            flat["heapMemoryDatapoints"].is_array() && flat["nonHeapMemoryDatapoints"].is_array(),
            "memory series serialize flat under their gateway-native names"
        );
    }
}
