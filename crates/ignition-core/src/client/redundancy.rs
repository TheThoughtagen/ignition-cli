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
//! - `lastSyncTimestamp` = **epoch-ms**, proven by the live 8.3.6
//!   Master/Backup pair (`20-LIVE-CAPTURES.md` block C-S3, spike file
//!   `/Users/pmannion/data/worktrees/whk-environment-orchestration/
//!   gateway-config-spec/spikes/gateway-modes/results/S3.md` lines 6-7
//!   and 40-41: `1791586091827` and `1791586358959`). `-1` stays the
//!   "never synced" sentinel seen on both fresh Independent rigs, and
//!   `-1`/absent normalizes to `None` via
//!   [`RedundancyStatusWire::last_sync_epoch_ms`].
//!
//! **Role versus activity (block C-S3).** `role` is configuration:
//! `Independent` (fresh single rig, 09 captures), `Master` and `Backup`
//! (the C-S3 pair). It is a String, NOT an enum: an unknown future role
//! must RIDE, never refuse. `role` does NOT say which node serves.
//! `activityLevel` does: `Active` on the serving node, `Cold` on the
//! standby, and it MOVES on failover while `role` stays fixed (S3.md
//! 6-7 pre-failover: a Master Active, b Backup Cold; S3.md 40-41 after
//! failover: a Master Cold, b Backup Active). A cold node reports
//! `uptime: -1`. Decide the active node with [`active_node`], never from
//! `role`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// GET path of the redundancy capability (83-api collection + live
/// capture; the /config, /events, /providers and /gwaction routes are
/// out of this phase's scope — `api call` covers them).
pub const REDUNDANCY_PATH: &str = "/data/api/v1/redundancy";

/// `activityLevel` of the serving node (block C-S3: S3.md 6-7 node a,
/// S3.md 40-41 node b; also every fresh Independent rig).
pub const ACTIVITY_ACTIVE: &str = "Active";

/// `activityLevel` of the standby node (block C-S3: S3.md 6-7 node b,
/// S3.md 40-41 node a). A cold node also reports `uptime: -1`.
pub const ACTIVITY_COLD: &str = "Cold";

/// GET `/data/api/v1/redundancy` — the redundancy status. Live-captured
/// 200 on BOTH rigs (09-LIVE-CAPTURES §3); byte-shape identical, values
/// differ only in `localId` + `uptime`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RedundancyStatusWire {
    /// `"Independent"` (both fresh rigs) / `"Master"` / `"Backup"` (the
    /// live pair, block C-S3). Configuration only: it does not say
    /// which node is active (see [`active_node`]). A String on purpose:
    /// an unknown future role must ride, not refuse.
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
    /// `"Active"` on the serving node, `"Cold"` on the standby (block
    /// C-S3); `"Active"` on both fresh Independent rigs. Moves on
    /// failover while `role` stays fixed.
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
    /// Epoch-ms (block C-S3: `1791586091827` on a live 8.3.6 pair);
    /// `-1` is the NEVER-SYNCED sentinel (both fresh Independent
    /// rigs). Use [`Self::last_sync_epoch_ms`] — raw `-1` rides here,
    /// never misread as a real timestamp.
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
    /// non-negative value is epoch-ms (block C-S3).
    pub fn last_sync_epoch_ms(&self) -> Option<i64> {
        self.last_sync_timestamp.filter(|value| *value >= 0)
    }
}

/// Which node of a redundant pair is serving, decided by `activityLevel`
/// alone (block C-S3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveNode {
    /// Only the first node is `Active`.
    OnlyA,
    /// Only the second node is `Active`.
    OnlyB,
    /// Both nodes report `Active`: a split-brain signal. Reported, never
    /// resolved; the caller must refuse on it.
    Both,
    /// Neither node reports `Active`.
    Neither,
}

/// Decide which node of a pair is active. Pure: no I/O, no clock.
///
/// Compares `activity_level` against [`ACTIVITY_ACTIVE`] on each side and
/// NEVER reads `role`: on failover `role` stays fixed (a stays Master)
/// while activity moves (block C-S3, S3.md 40-41), so a role-based
/// decision would pick the wrong node. An absent or unrecognised level
/// (anything but `Active`) is not active.
pub fn active_node(a: &RedundancyStatusWire, b: &RedundancyStatusWire) -> ActiveNode {
    let is_active = |w: &RedundancyStatusWire| w.activity_level.as_deref() == Some(ACTIVITY_ACTIVE);
    match (is_active(a), is_active(b)) {
        (true, false) => ActiveNode::OnlyA,
        (false, true) => ActiveNode::OnlyB,
        (true, true) => ActiveNode::Both,
        (false, false) => ActiveNode::Neither,
    }
}

#[cfg(test)]
mod tests {
    use super::{ACTIVITY_COLD, ActiveNode, RedundancyStatusWire, active_node};

    fn wire(value: serde_json::Value) -> RedundancyStatusWire {
        serde_json::from_value(value).expect("the pair body must parse")
    }

    // C-S3, S3.md:6-7 (paired, pre-failover), copied verbatim.
    fn pre_failover() -> (RedundancyStatusWire, RedundancyStatusWire) {
        (
            wire(
                serde_json::json!({"role":"Master","projectState":"Good","activityLevel":"Active","localId":"192.168.97.3","peerId":"spike-a-Backup","peerConnected":true,"hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":29279,"lastSyncTimestamp":1791586091827i64}),
            ),
            wire(
                serde_json::json!({"role":"Backup","projectState":"Good","activityLevel":"Cold","localId":"192.168.97.2","peerId":"spike-a-Master","peerConnected":true,"hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":-1,"lastSyncTimestamp":1791586091875i64}),
            ),
        )
    }

    // C-S3, S3.md:40-41 (after failover), copied verbatim.
    fn post_failover() -> (RedundancyStatusWire, RedundancyStatusWire) {
        (
            wire(
                serde_json::json!({"role":"Master","projectState":"Good","activityLevel":"Cold","localId":"192.168.97.3","peerId":"spike-a-Backup","peerConnected":true,"hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":-1,"lastSyncTimestamp":1791586358959i64}),
            ),
            wire(
                serde_json::json!({"role":"Backup","projectState":"Good","activityLevel":"Active","localId":"192.168.97.2","peerId":"spike-a-Master","peerConnected":true,"hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":30539,"lastSyncTimestamp":1791586359045i64}),
            ),
        )
    }

    /// C-S3 S3.md:40-41: role stays Master on a while activity moves to
    /// b; deciding by role would pick the wrong node.
    #[test]
    fn active_node_follows_activity_not_role() {
        let (a, b) = post_failover();
        assert_eq!(a.role, "Master");
        assert_eq!(b.role, "Backup");
        assert_eq!(a.activity_level.as_deref(), Some(ACTIVITY_COLD));
        assert_eq!(a.uptime, Some(-1), "a cold node reports uptime -1");
        assert_eq!(active_node(&a, &b), ActiveNode::OnlyB);
    }

    /// C-S3 S3.md:6-7: before failover the Master is the active node.
    #[test]
    fn active_node_pre_failover_is_master() {
        let (a, b) = pre_failover();
        assert_eq!(a.role, "Master");
        assert_eq!(b.role, "Backup");
        assert_eq!(active_node(&a, &b), ActiveNode::OnlyA);
    }

    /// Split-brain is reported, no active node is `Neither`, and an
    /// absent or unrecognised level never counts as active (A-07a/b).
    #[test]
    fn active_node_split_brain_and_none() {
        let with = |level: Option<&str>| {
            let mut v = serde_json::json!({"role":"Independent"});
            if let Some(level) = level {
                v["activityLevel"] = serde_json::json!(level);
            }
            wire(v)
        };
        assert_eq!(
            active_node(&with(Some("Active")), &with(Some("Active"))),
            ActiveNode::Both
        );
        assert_eq!(
            active_node(&with(Some("Cold")), &with(Some("Cold"))),
            ActiveNode::Neither
        );
        assert_eq!(
            active_node(&with(None), &with(Some("Cold"))),
            ActiveNode::Neither
        );
        assert_eq!(
            active_node(&with(Some("Warm")), &with(Some("Cold"))),
            ActiveNode::Neither
        );
        assert_eq!(
            active_node(&with(Some("Warm")), &with(Some("Active"))),
            ActiveNode::OnlyB
        );
    }

    /// C-S3 S3.md:6-7: a live pair's lastSyncTimestamp is epoch-ms.
    #[test]
    fn last_sync_is_epoch_ms_on_the_live_pair() {
        let (a, _) = pre_failover();
        assert_eq!(a.last_sync_epoch_ms(), Some(1_791_586_091_827));
    }

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

    /// A non-negative `lastSyncTimestamp` (epoch-ms, block C-S3)
    /// normalizes through, a peer-shaped Master body parses, and unknown
    /// keys ride `extra` (the round-trip proof).
    #[test]
    fn redundancy_normalizes_and_passes_unknown_keys() {
        let wire: RedundancyStatusWire = serde_json::from_value(serde_json::json!({
            "role": "Master", "projectState": "Good",
            "activityLevel": "Active", "localId": "192.168.97.3",
            "peerId": "spike-a-Backup",
            "peerConnected": true, "hasConfigAccess": true,
            "syncPending": false, "failoverPending": false,
            "uptime": 29279, "lastSyncTimestamp": 1791586091827i64,
            "futurePointReleaseField": [1, 2]
        }))
        .expect("peer-shaped + unknown-key variants must parse");
        assert_eq!(wire.role, "Master", "a String role rides, never refuses");
        assert_eq!(wire.peer_id.as_deref(), Some("spike-a-Backup"));
        assert_eq!(
            wire.last_sync_epoch_ms(),
            Some(1_791_586_091_827),
            "non-negative values read epoch-ms (block C-S3)"
        );
        assert_eq!(
            wire.extra.get("futurePointReleaseField"),
            Some(&serde_json::json!([1, 2])),
            "unknown keys ride flatten passthrough"
        );
    }
}
