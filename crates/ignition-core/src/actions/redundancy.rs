//! `ign redundancy status` action (09-04, EXT-02) — a thin wrapper
//! over the redundancy read. The wrapper indirection keeps the render
//! arms symmetrical with the license merge (`{status: …}` envelope
//! shape for the single-read families).
//!
//! `redundancy_status_pair` (24-04, D-11) reads a second node through
//! its own profile and names the serving node. **Activity level decides,
//! never role** (block C-S3): on failover `role` stays fixed while
//! `activityLevel` moves. A Cold backup with `uptime: -1` is a healthy
//! standby, not a fault, and nothing here warns about it.

use serde::Serialize;

use crate::client::GatewayApi;
use crate::client::redundancy::{ActiveNode, RedundancyStatusWire, active_node};
use crate::error::CoreError;

/// Which node of a pair is serving, from the second-node read.
#[derive(Debug, Serialize)]
pub struct ActiveNodeReport {
    /// `this`, `peer`, `both` (a split-brain signal, reported never
    /// resolved) or `neither`; decided by `activityLevel` alone.
    pub active: &'static str,
    /// The `role` the active profile's node reports (display only).
    pub this_role: String,
    /// The `role` the peer profile's node reports (display only).
    pub peer_role: String,
    /// The peer profile's name.
    pub peer_profile: String,
    /// The peer node's flat wire status, capture-shaped.
    pub peer: RedundancyStatusWire,
}

/// `ign redundancy status` output model.
#[derive(Debug, Serialize)]
pub struct RedundancyStatusResult {
    /// The flat wire status, capture-shaped (units documented at the
    /// model: uptime ms-since-start; lastSyncTimestamp -1 sentinel).
    pub status: RedundancyStatusWire,
    /// Present only with `--peer`: absent (not null) otherwise so the
    /// single-node output stays byte-identical.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_node: Option<ActiveNodeReport>,
}

/// Read the redundancy status in one command.
pub async fn redundancy_status(api: &dyn GatewayApi) -> Result<RedundancyStatusResult, CoreError> {
    Ok(RedundancyStatusResult {
        status: api.redundancy_status().await?,
        active_node: None,
    })
}

/// Read both nodes of a pair (each through its own client) and name the
/// active one. `this` is the active profile's node, `peer` the second.
pub async fn redundancy_status_pair(
    this: &dyn GatewayApi,
    peer: &dyn GatewayApi,
    peer_profile: &str,
) -> Result<RedundancyStatusResult, CoreError> {
    let this_status = this.redundancy_status().await?;
    let peer_status = peer.redundancy_status().await?;
    let active = match active_node(&this_status, &peer_status) {
        ActiveNode::OnlyA => "this",
        ActiveNode::OnlyB => "peer",
        ActiveNode::Both => "both",
        ActiveNode::Neither => "neither",
    };
    let report = ActiveNodeReport {
        active,
        this_role: this_status.role.clone(),
        peer_role: peer_status.role.clone(),
        peer_profile: peer_profile.to_string(),
        peer: peer_status,
    };
    Ok(RedundancyStatusResult {
        status: this_status,
        active_node: Some(report),
    })
}
