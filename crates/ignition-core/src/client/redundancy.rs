//! Redundancy capability model (09-04, EXT-02) — `GET
//! /data/api/v1/redundancy`, the FLAT 11-field body live-captured on
//! both rigs (09-LIVE-CAPTURES §3: 8.3.6 rig A + 8.3.3 rig B). The
//! TrialWire recipe throughout: `rename` + snake_case `alias` +
//! `default` on every camelCase key, unknown keys never refuse the
//! parse, flatten remainder.
//!
//! **The units are capture-locked (09-LIVE-CAPTURES §3 + Decisions 4):**
//! - `uptime` = **milliseconds since gateway (JVM) start** — wall-clock
//!   proven TWICE: rig A 460 959 ms + gateway start 02:29:30Z = exactly
//!   the 02:37:11Z capture moment; rig B 617 422 ms same proof. A
//!   minutes-old-restarted rig rules out epoch units AND
//!   seconds-since-start.
//! - `lastSyncTimestamp` = `-1` sentinel on BOTH fresh Independent rigs
//!   ("never synced"). Its unit is therefore NOT capture-proven — no
//!   sync ever happened. The sibling timestamp (`licenses.effective.
//!   lastUpdated`) is epoch-ms, so non-negative values are interpreted
//!   as epoch-ms flagged as INFERENCE, and `-1`/absent normalizes to
//!   `None` via [`RedundancyStatusWire::last_sync_epoch_ms`].
//!
//! `role` is a String, NOT an enum: only `Independent` was captured
//! (fresh single rig); `Primary`/`Backup` come from `GatewayInfo.
//! redundancy_role` — an unknown future role must RIDE, never refuse.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// GET path of the redundancy capability (83-api collection + live
/// capture; the /config, /events, /providers and /gwaction routes are
/// out of this phase's scope — `api call` covers them).
pub const REDUNDANCY_PATH: &str = "/data/api/v1/redundancy";

/// GET `/data/api/v1/redundancy` — the redundancy status. Live-captured
/// 200 on BOTH rigs (09-LIVE-CAPTURES §3); byte-shape identical, values
/// differ only in `localId` + `uptime`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RedundancyStatusWire {
    /// `"Independent"` (captured on both fresh rigs) / `"Primary"` /
    /// `"Backup"` (GatewayInfo.redundancy_role vocabulary) — a String
    /// on purpose: an unknown future role must ride, not refuse.
    #[serde(default)]
    pub role: String,
    /// `"Unknown"` on both captures (a fresh Independent rig has no
    /// project state to report).
    #[serde(
        rename = "projectState",
        alias = "project_state",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub project_state: Option<String>,
    /// `"Active"` on both captures.
    #[serde(
        rename = "activityLevel",
        alias = "activity_level",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub activity_level: Option<String>,
    /// This gateway's id — the container IP on both captures
    /// (`192.168.215.2` / `192.168.171.2`).
    #[serde(
        rename = "localId",
        alias = "local_id",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub local_id: Option<String>,
    /// The peer's id — ABSENT on both fresh-rig captures (no peer
    /// exists); the flat research shape documents it, so it rides
    /// Option. Peer-connected shapes were NOT capturable on a single
    /// fresh rig — everything peer-shaped stays Option/lenient.
    #[serde(
        rename = "peerId",
        alias = "peer_id",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub peer_id: Option<String>,
    /// `false` on both captures (no peer on a fresh rig).
    #[serde(rename = "peerConnected", alias = "peer_connected", default)]
    pub peer_connected: bool,
    /// `true` on both captures (an Independent gateway owns its config).
    #[serde(rename = "hasConfigAccess", alias = "has_config_access", default)]
    pub has_config_access: bool,
    /// `false` on both captures.
    #[serde(rename = "syncPending", alias = "sync_pending", default)]
    pub sync_pending: bool,
    /// `false` on both captures.
    #[serde(rename = "failoverPending", alias = "failover_pending", default)]
    pub failover_pending: bool,
    /// **Milliseconds since gateway (JVM) start** — capture-proven by
    /// wall-clock cross-check on BOTH rigs (module doc; 09-LIVE-CAPTURES
    /// §3). Option: present on both captures, defaulted for tolerance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uptime: Option<i64>,
    /// `-1` on both captures = the NEVER-SYNCED sentinel. Unit NOT
    /// capture-proven (no sync ever happened on a fresh rig); the ms
    /// interpretation is INFERENCE from the sibling
    /// `licenses.effective.lastUpdated` epoch-ms. Use
    /// [`Self::last_sync_epoch_ms`] — raw `-1` rides here, never
    /// misread as a real timestamp.
    #[serde(
        rename = "lastSyncTimestamp",
        alias = "last_sync_timestamp",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub last_sync_timestamp: Option<i64>,
    /// Unknown keys round-trip (version tolerance).
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl RedundancyStatusWire {
    /// `lastSyncTimestamp` normalized: the `-1` sentinel ("never
    /// synced" — both fresh rigs) and absent keys → `None`; a
    /// non-negative value is epoch-ms PER THE SIBLING PATTERN — an
    /// INFERENCE (flagged at the field), never a capture-proven fact.
    pub fn last_sync_epoch_ms(&self) -> Option<i64> {
        self.last_sync_timestamp.filter(|value| *value >= 0)
    }
}

#[cfg(test)]
mod tests {
    use super::RedundancyStatusWire;

    /// THE 8.3.6 live capture (rig A, 09-LIVE-CAPTURES §3) — verbatim
    /// one-line body.
    #[test]
    fn redundancy_parses_the_live_8_3_6_capture() {
        let wire: RedundancyStatusWire = serde_json::from_value(serde_json::json!({
            "role": "Independent", "projectState": "Unknown",
            "activityLevel": "Active", "localId": "192.168.215.2",
            "peerConnected": false, "hasConfigAccess": true,
            "syncPending": false, "failoverPending": false,
            "uptime": 460959, "lastSyncTimestamp": -1
        }))
        .expect("the live 8.3.6 shape must parse");
        assert_eq!(wire.role, "Independent");
        assert_eq!(wire.project_state.as_deref(), Some("Unknown"));
        assert_eq!(wire.activity_level.as_deref(), Some("Active"));
        assert_eq!(wire.local_id.as_deref(), Some("192.168.215.2"));
        assert!(!wire.peer_connected);
        assert!(wire.has_config_access);
        assert!(!wire.sync_pending);
        assert!(!wire.failover_pending);
        assert_eq!(wire.uptime, Some(460_959), "ms since gateway start");
        assert_eq!(wire.last_sync_timestamp, Some(-1));
        assert_eq!(
            wire.last_sync_epoch_ms(),
            None,
            "the -1 sentinel never reads as a timestamp"
        );
        assert_eq!(wire.peer_id, None, "peer absent on a fresh rig");
    }

    /// THE 8.3.3 live capture (rig B, 09-LIVE-CAPTURES §3) — same
    /// shape, different localId + uptime (no point-release drift).
    #[test]
    fn redundancy_parses_the_live_8_3_3_capture() {
        let wire: RedundancyStatusWire = serde_json::from_value(serde_json::json!({
            "role": "Independent", "projectState": "Unknown",
            "activityLevel": "Active", "localId": "192.168.171.2",
            "peerConnected": false, "hasConfigAccess": true,
            "syncPending": false, "failoverPending": false,
            "uptime": 617422, "lastSyncTimestamp": -1
        }))
        .expect("the live 8.3.3 shape must parse");
        assert_eq!(wire.uptime, Some(617_422));
        assert_eq!(wire.last_sync_epoch_ms(), None);
    }

    /// A non-negative `lastSyncTimestamp` (never captured — the ms
    /// unit is the flagged INFERENCE) normalizes through, and unknown
    /// keys ride `extra` (the round-trip proof).
    #[test]
    fn redundancy_normalizes_and_passes_unknown_keys() {
        let wire: RedundancyStatusWire = serde_json::from_value(serde_json::json!({
            "role": "Primary", "projectState": "ReadyToGo",
            "activityLevel": "Active", "localId": "10.0.0.1",
            "peerId": "10.0.0.2",
            "peerConnected": true, "hasConfigAccess": true,
            "syncPending": false, "failoverPending": false,
            "uptime": 123456, "lastSyncTimestamp": 1788748551614i64,
            "futurePointReleaseField": [1, 2]
        }))
        .expect("peer-shaped + unknown-key variants must parse");
        assert_eq!(wire.role, "Primary", "a String role rides, never refuses");
        assert_eq!(wire.peer_id.as_deref(), Some("10.0.0.2"));
        assert_eq!(
            wire.last_sync_epoch_ms(),
            Some(1_788_748_551_614),
            "non-negative values read epoch-ms (flagged inference)"
        );
        assert_eq!(
            wire.extra.get("futurePointReleaseField"),
            Some(&serde_json::json!([1, 2])),
            "unknown keys ride flatten passthrough"
        );
    }
}
