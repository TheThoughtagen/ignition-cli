//! Gateway Area Network overview model (09-04, EXT-02) — `GET
//! /data/api/v1/overview/gan`, the ZERO-CONNECTION body live-captured
//! identically on both rigs (09-LIVE-CAPTURES §4: 8.3.6 rig A + 8.3.3
//! rig B). A fresh non-GAN gateway's zero-connection shape IS the
//! canonical capture — 5 flat scalars, no arrays, no nesting.
//!
//! **Number shapes (capture-locked):** the connection counts and
//! `remoteGateways` serialize as JSON **ints** (`0`); the byte rates
//! serialize as JSON **floats** (`0.0`) — so counts are integer-typed
//! and rates are `f64` (which parses BOTH forms; a point release that
//! switches a rate to whole-number JSON can never fail the parse).
//!
//! Decision 4 of the phase: `overview/gan` ONLY — no
//! `/gateway-network/gateways` companion this phase; `ign api call`
//! covers that list.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// GET path of the GAN overview capability (83-api collection + live
/// capture).
pub const GAN_OVERVIEW_PATH: &str = "/data/api/v1/overview/gan";

/// GET `/data/api/v1/overview/gan` — the 5-field GAN summary.
/// Live-captured 200 on BOTH rigs; zero drift.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GanStatusWire {
    /// Total GAN connections — `0` on both captures (JSON int); a
    /// non-GAN gateway's steady state IS zero (the capture IS the
    /// meaning: zeros are healthy data, not absence of feature).
    #[serde(rename = "totalConnections", alias = "total_connections", default)]
    pub total_connections: i64,
    /// Currently-running connections — `0` on both captures (JSON int).
    #[serde(rename = "runningConnections", alias = "running_connections", default)]
    pub running_connections: i64,
    /// Outbound byte rate — `0.0` on both captures (JSON FLOAT; f64
    /// parses both the float and a hypothetical whole-number form).
    #[serde(rename = "outgoingByteRate", alias = "outgoing_byte_rate", default)]
    pub outgoing_byte_rate: f64,
    /// Inbound byte rate — `0.0` on both captures (JSON float).
    #[serde(rename = "incomingByteRate", alias = "incoming_byte_rate", default)]
    pub incoming_byte_rate: f64,
    /// Remote gateways known to this one — `0` on both captures (JSON
    /// int).
    #[serde(rename = "remoteGateways", alias = "remote_gateways", default)]
    pub remote_gateways: i64,
    /// Unknown keys round-trip (version tolerance — the passthrough
    /// proof fixture appends a fake key and rides it here).
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::GanStatusWire;

    /// THE live capture (IDENTICAL on both rigs, 09-LIVE-CAPTURES §4)
    /// — the zero-connection body, floats spelled `0.0`.
    #[test]
    fn gan_parses_the_live_zero_connection_capture() {
        let wire: GanStatusWire = serde_json::from_value(serde_json::json!({
            "totalConnections": 0,
            "runningConnections": 0,
            "outgoingByteRate": 0.0,
            "incomingByteRate": 0.0,
            "remoteGateways": 0
        }))
        .expect("the live zero-connection shape must parse");
        assert_eq!(wire.total_connections, 0);
        assert_eq!(wire.running_connections, 0);
        assert_eq!(wire.outgoing_byte_rate, 0.0);
        assert_eq!(wire.incoming_byte_rate, 0.0);
        assert_eq!(wire.remote_gateways, 0);
    }

    /// Whole-number rate forms AND integer/float mixes still parse
    /// (f64 tolerance), a populated shape carries real values, and
    /// unknown keys ride `extra` (the round-trip proof).
    #[test]
    fn gan_tolerant_over_populated_and_unknown_shapes() {
        let wire: GanStatusWire = serde_json::from_value(serde_json::json!({
            "totalConnections": 4,
            "runningConnections": 2,
            "outgoingByteRate": 15360,
            "incomingByteRate": 2048.5,
            "remoteGateways": 1,
            "futurePointReleaseField": {"rate": 1.25}
        }))
        .expect("whole-number + unknown-key forms must parse");
        assert_eq!(wire.total_connections, 4);
        assert_eq!(wire.running_connections, 2);
        assert_eq!(wire.outgoing_byte_rate, 15360.0, "f64 parses ints too");
        assert_eq!(wire.incoming_byte_rate, 2048.5);
        assert_eq!(wire.remote_gateways, 1);
        assert_eq!(
            wire.extra.get("futurePointReleaseField"),
            Some(&serde_json::json!({"rate": 1.25})),
            "unknown keys ride flatten passthrough"
        );
    }
}
