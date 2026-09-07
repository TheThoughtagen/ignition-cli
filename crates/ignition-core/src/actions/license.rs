//! `ign license status` action (09-04, EXT-02) — the morning-check
//! MERGE: the `/data/api/v1/licenses` inventory read plus the trial
//! companion (`/data/api/v1/trial` — [`TrialWire`] + its capability
//! method, already live-proven in Phase 4; zero new trial code).
//!
//! The merge exists because the license MODE is not on the licenses
//! payload at all (09-LIVE-CAPTURES §1: the captured top level is 5
//! arrays + effective + two scalars — no mode/edition keys); the
//! capture wins over the plan's field sketch. `license_mode` +
//! `trial_seconds_left` ride the trial companion; the typed
//! hardware items carry the count.

use serde::Serialize;

use crate::client::GatewayApi;
use crate::client::license::LicenseStatusWire;
use crate::client::trial::TrialWire;
use crate::error::CoreError;

/// `ign license status` output model (all keys always present).
#[derive(Debug, Serialize)]
pub struct LicenseStatusResult {
    /// The `/licenses` inventory (partial-curated wire passthrough).
    pub license: LicenseStatusWire,
    /// The trial companion — mode + countdown (see the module doc:
    /// the mode lives HERE, not on the licenses payload).
    pub trial: TrialWire,
}

/// Read the license inventory + the trial companion in one command.
pub async fn license_status(api: &dyn GatewayApi) -> Result<LicenseStatusResult, CoreError> {
    let license = api.license_status().await?;
    let trial = api.trial_status_wire().await?;
    Ok(LicenseStatusResult { license, trial })
}
