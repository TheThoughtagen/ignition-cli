//! Trial-license capability models (04-03, RIG-02/03) — field names
//! match the **live-captured 8.3.x bodies** (04-RESEARCH §Code Examples,
//! re-verified live on BOTH minor versions during this plan's spike:
//! expired shape on ign-research 8.3.6, active shape on the
//! ignition-devops rig 8.3.3).
//!
//! `GET /data/api/v1/trial` answers **unauthenticated** (live-verified
//! on both rigs, both trial states) — a fresh rig has no token yet, so
//! the trait methods apply auth ONLY when the client carries a
//! credential (the plan's "cred present → headers ride along
//! harmlessly" rule; a header-less client degrades cleanly, the
//! version-command precedent).
//!
//! `POST /data/api/v1/trial` is the RESET: session-cookie + CSRF (tier
//! 1, the browser-verified mechanism — live-proven end-to-end on
//! 8.3.3: `expired:true → false`, `trialSecondsLeft 0 → 7199`). The
//! 2xx response body IS the fresh [`TrialWire`] (live-observed), so
//! the reset parse reuses this model. **State gate (live-discovered):
//! the gateway answers 403 to reset attempts on a NON-expired trial**
//! — the action layer pre-checks expiry to keep that refusal honest.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// GET/POST path of the trial capability.
pub(crate) const TRIAL_PATH: &str = "/data/api/v1/trial";

/// GET path of the overview banners (the trial cross-check source).
pub(crate) const BANNERS_PATH: &str = "/data/api/v1/overview/banners";

/// GET `/data/api/v1/trial` — the trial state (also the POST-reset
/// response body). Live-captured on 8.3.6 (expired) and 8.3.3 (active):
///
/// ```jsonc
/// { "licenseMode": "Trial", "trialState": "AllInDemo",
///   "trialSecondsLeft": 0, "expired": true, "emergency": false,
///   "emergencySecondsLeft": 0, "development": false,
///   "developmentSecondsLeft": 0 }
/// ```
///
/// `trialState` domain (83-api postman, live-matched):
/// `AllInDemo` | `SomeInDemo` | `NoneInDemo`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrialWire {
    /// `"Trial"` / `"Licensed"` / …
    #[serde(rename = "licenseMode", default)]
    pub license_mode: String,
    /// `AllInDemo` / `SomeInDemo` / `NoneInDemo`.
    #[serde(rename = "trialState", default)]
    pub trial_state: String,
    /// Trial countdown in epoch **SECONDS** (0 once expired).
    #[serde(rename = "trialSecondsLeft", default)]
    pub trial_seconds_left: i64,
    /// The expiry flag — [`actions::rig::trial_status`]'s primary
    /// truth (never derive "active" from banners alone; Pitfall 7).
    #[serde(default)]
    pub expired: bool,
    /// Emergency-license mode flag.
    #[serde(default)]
    pub emergency: bool,
    /// Emergency countdown in epoch **SECONDS**.
    #[serde(rename = "emergencySecondsLeft", default)]
    pub emergency_seconds_left: i64,
    /// Development-license mode flag.
    #[serde(default)]
    pub development: bool,
    /// Development countdown in epoch **SECONDS**.
    #[serde(rename = "developmentSecondsLeft", default)]
    pub development_seconds_left: i64,
    /// Unknown keys round-trip (passthrough-shaped `--json`).
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// GET `/data/api/v1/overview/banners` — the banner set. The trial
/// banner (`type: "trial"`) is the status cross-check: expired shows
/// `severity:"warning"` + `expireTime:null`; active shows
/// `severity:"info"` + `expireTime` epoch-**milliseconds**
/// (live-captured both shapes, both rigs — Pitfall 7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BannerSet {
    /// The banners, order-field verbatim (8.3.6 serves `order: 0`,
    /// 8.3.3 serves `order: 5` for the trial banner — both parse).
    #[serde(default)]
    pub banners: Vec<Banner>,
}

/// One banner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Banner {
    /// Display order (not an index — values differ per version).
    #[serde(default)]
    pub order: i64,
    /// `"trial"` / … (the trial cross-check keys on this).
    #[serde(rename = "type", default)]
    pub r#type: String,
    /// The banner payload.
    #[serde(default)]
    pub data: BannerData,
}

/// `banner.data` — severity + expiry. `expireTime` is epoch
/// **MILLISECONDS** or `null` (an expired trial shows null — code
/// expecting a future timestamp misreads expired as active; Pitfall 7).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BannerData {
    /// `"info"` (active) / `"warning"` (expired) / …
    #[serde(default)]
    pub severity: String,
    /// Epoch **milliseconds**, or `null` when expired/unknown.
    #[serde(rename = "expireTime", default)]
    pub expire_time_ms: Option<i64>,
    /// Passthrough tooltip descriptors.
    #[serde(rename = "toolTips", default)]
    pub tool_tips: Vec<serde_json::Value>,
    /// Passthrough action descriptors.
    #[serde(default)]
    pub actions: Vec<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::{BannerSet, TrialWire};

    /// THE live-capture regression (ign-research 8.3.6, expired —
    /// fetched unauthenticated during this plan's spike).
    #[test]
    fn trial_parses_the_live_expired_capture() {
        let wire: TrialWire = serde_json::from_value(serde_json::json!({
            "licenseMode": "Trial",
            "trialState": "AllInDemo",
            "trialSecondsLeft": 0,
            "expired": true,
            "emergency": false,
            "emergencySecondsLeft": 0,
            "development": false,
            "developmentSecondsLeft": 0
        }))
        .expect("the live expired shape must parse");
        assert_eq!(wire.license_mode, "Trial");
        assert_eq!(wire.trial_state, "AllInDemo");
        assert_eq!(wire.trial_seconds_left, 0, "epoch seconds");
        assert!(wire.expired);
        assert!(!wire.emergency);
    }

    /// The active-state capture (ignition-devops 8.3.3, live during the
    /// spike): countdown non-zero, expired false.
    #[test]
    fn trial_parses_the_live_active_capture() {
        let wire: TrialWire = serde_json::from_value(serde_json::json!({
            "licenseMode": "Trial",
            "trialState": "AllInDemo",
            "trialSecondsLeft": 6727,
            "expired": false,
            "emergency": false,
            "emergencySecondsLeft": 0,
            "development": false,
            "developmentSecondsLeft": 0
        }))
        .expect("the live active shape must parse");
        assert!(!wire.expired);
        assert_eq!(wire.trial_seconds_left, 6727);
    }

    /// Both banner captures: expired = warning + null expireTime
    /// (8.3.6); active = info + epoch-ms expireTime (8.3.3, where the
    /// trial banner rides `order: 5` — order is not an index).
    #[test]
    fn banners_parse_both_live_states() {
        let expired: BannerSet = serde_json::from_value(serde_json::json!({
            "banners": [{
                "order": 0,
                "type": "trial",
                "data": { "severity": "warning", "expireTime": null,
                          "toolTips": [], "actions": [] }
            }]
        }))
        .expect("the live expired banner shape must parse");
        let trial = &expired.banners[0];
        assert_eq!(trial.r#type, "trial");
        assert_eq!(trial.data.severity, "warning");
        assert_eq!(trial.data.expire_time_ms, None, "expired = null, Pitfall 7");

        let active: BannerSet = serde_json::from_value(serde_json::json!({
            "banners": [{
                "order": 5,
                "type": "trial",
                "data": { "severity": "info",
                          "expireTime": 1787435662564i64,
                          "toolTips": [], "actions": [] }
            }]
        }))
        .expect("the live active banner shape must parse");
        let trial = &active.banners[0];
        assert_eq!(trial.data.severity, "info");
        assert_eq!(
            trial.data.expire_time_ms,
            Some(1_787_435_662_564),
            "epoch MILLISECONDS"
        );
    }
}
