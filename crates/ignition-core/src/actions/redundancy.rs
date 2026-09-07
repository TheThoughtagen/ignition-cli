//! `ign redundancy status` action (09-04, EXT-02) — a thin wrapper
//! over the redundancy read. The wrapper indirection keeps the render
//! arms symmetrical with the license merge (`{status: …}` envelope
//! shape for the single-read families).

use serde::Serialize;

use crate::client::GatewayApi;
use crate::client::redundancy::RedundancyStatusWire;
use crate::error::CoreError;

/// `ign redundancy status` output model.
#[derive(Debug, Serialize)]
pub struct RedundancyStatusResult {
    /// The flat wire status, capture-shaped (units documented at the
    /// model: uptime ms-since-start; lastSyncTimestamp -1 sentinel).
    pub status: RedundancyStatusWire,
}

/// Read the redundancy status in one command.
pub async fn redundancy_status(api: &dyn GatewayApi) -> Result<RedundancyStatusResult, CoreError> {
    Ok(RedundancyStatusResult {
        status: api.redundancy_status().await?,
    })
}
