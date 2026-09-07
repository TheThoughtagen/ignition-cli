//! `ign gan status` action (09-04, EXT-02) — a thin wrapper over the
//! GAN overview read (the redundancy wrapper pattern; `{status: …}`
//! keeps the render arms symmetrical).

use serde::Serialize;

use crate::client::GatewayApi;
use crate::client::gan::GanStatusWire;
use crate::error::CoreError;

/// `ign gan status` output model.
#[derive(Debug, Serialize)]
pub struct GanStatusResult {
    /// The 5-field wire status — zeros on a non-GAN gateway are
    /// healthy data (the capture IS the meaning).
    pub status: GanStatusWire,
}

/// Read the GAN overview in one command.
pub async fn gan_status(api: &dyn GatewayApi) -> Result<GanStatusResult, CoreError> {
    Ok(GanStatusResult {
        status: api.gan_status().await?,
    })
}
