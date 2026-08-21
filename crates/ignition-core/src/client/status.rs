//! Status/info capability models (02-02, HLTH-01/02) — field names match
//! the **live 8.3.6 gateway** captures (02-RESEARCH §Status/info,
//! §Modules) and the gateway's own openapi schema.
//!
//! Every model carries `#[serde(flatten)] extra` passthrough so `--json`
//! stays complete as gateway responses evolve (unknown keys round-trip
//! instead of being dropped).
//!
//! Two unit gotchas are pinned by naming/comments, never silently
//! converted (02-RESEARCH §Metrics: "normalize in the model, not in
//! users' eyes" — the normalization is HONEST NAMING):
//! - [`Overview::uptime`] is epoch **milliseconds**;
//! - [`Overview::cpu`] is a 0–1 **fraction** — the
//!   `systemPerformance/currentGauges` cpu is **percent**. Same concept,
//!   two scales: the field comments say so at both homes.
//!
//! `/data/api/v1/overview` is live-verified (02-RESEARCH §Status/info)
//! and present in the openapi extract — the single best status call.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// GET path of the overview capability (live-verified, 02-RESEARCH
/// §Status/info).
pub(crate) const OVERVIEW_PATH: &str = "/data/api/v1/overview";

/// GET path of the unauthenticated readiness probe (NOTE: root-level, not
/// under `/data` — it answers even while the gateway restarts).
pub(crate) const STATUS_PING_PATH: &str = "/StatusPing";

/// GET paths of the two module lists (healthy = fully loaded modules;
/// quarantined = modules withheld at startup).
pub(crate) const MODULES_HEALTHY_PATH: &str = "/data/api/v1/modules/healthy";
pub(crate) const MODULES_QUARANTINED_PATH: &str = "/data/api/v1/modules/quarantined";

/// GET `/data/api/v1/overview` — the status call (platform + runtime).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Overview {
    /// Version + build revision in one string, e.g.
    /// `"8.3.6 (b2026042713)"`.
    pub version: String,
    /// Redundancy block, when reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redundancy: Option<RedundancyInfo>,
    /// JVM block `{version, vendor, name}`, when reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java: Option<JavaInfo>,
    /// OS block `{name, arch, version}`, when reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<OsInfo>,
    /// Gateway uptime in epoch **MILLISECONDS** (live capture: `338137`
    /// ≈ 5½ minutes — a seconds interpretation would be off by 1000×).
    pub uptime: i64,
    /// `[used, max]` heap bytes (live capture: `[338137088i64, 1073741824i64]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub memory: Vec<i64>,
    /// CPU utilization as a 0–1 **FRACTION** (live capture: `0.0031`).
    /// NOT percent — `systemPerformance/currentGauges` reports percent
    /// (4.88). No silent conversion between the two, ever.
    pub cpu: f64,
    /// Disk block `{total, used}` bytes, when reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk: Option<DiskInfo>,
    /// License state block, when reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<OverviewLicense>,
    /// Unknown keys round-trip (passthrough-shaped `--json`).
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// `overview.redundancy` — `{role, activityLevel, projectState, …}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RedundancyInfo {
    /// `"Independent"` / `"Backup"` / `"Primary"`, when reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(
        rename = "activityLevel",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub activity_level: Option<String>,
    #[serde(
        rename = "projectState",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub project_state: Option<String>,
    /// Unknown keys round-trip.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// `overview.java` — `{version, vendor, name}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JavaInfo {
    /// e.g. `"17.0.11"`.
    #[serde(default)]
    pub version: String,
    /// e.g. `"Azul Systems, Inc."`.
    #[serde(default)]
    pub vendor: String,
    /// e.g. `"OpenJDK 64-Bit Server VM"`.
    #[serde(default)]
    pub name: String,
}

/// `overview.os` — `{name, arch, version}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OsInfo {
    /// e.g. `"Linux"`.
    #[serde(default)]
    pub name: String,
    /// e.g. `"amd64"`.
    #[serde(default)]
    pub arch: String,
    /// e.g. `"5.15.0-91-generic"`, when reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// `overview.disk` — `{total, used}` bytes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiskInfo {
    /// Total bytes.
    #[serde(default)]
    pub total: i64,
    /// Used bytes.
    #[serde(default)]
    pub used: i64,
}

/// `overview.license` — `{state, trialRemaining}` (suffixed `_s` in Rust
/// to make the unit explicit; serialized under the gateway-native key).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverviewLicense {
    /// `"trial"` / `"licensed"` / …
    #[serde(default)]
    pub state: String,
    /// Trial countdown in epoch **SECONDS** (live capture: `7017` ≈
    /// 1h57m), when reported (absent on licensed gateways).
    #[serde(
        rename = "trialRemaining",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub trial_remaining_s: Option<i64>,
    /// Unknown keys round-trip.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// GET `/StatusPing` — the UNAUTHENTICATED readiness anchor (works
/// mid-restart and with broken credentials; 02-02 fetches it header-less
/// via `auth = false`).
///
/// States observed live: `RUNNING`, `STARTING`. Commissioning-era states
/// are unenumerated — unknown states surface as-is and are treated as
/// not-ready by 02-05's wait loops (02-RESEARCH Open Question 3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusPing {
    /// `RUNNING` / `STARTING` / …, verbatim from the gateway.
    pub state: String,
}

/// One item of `/data/api/v1/modules/healthy` (or `/modules/quarantined`).
///
/// Quarantined items answer a REDUCED shape (openapi + live): only
/// `id`/`name`/version-family fields are guaranteed — `state`,
/// `licenseState`, `vendorName`, `startupTime` exist only on fully
/// loaded modules, so they are `Option` here or the `--quarantined`
/// list would fail to parse.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleInfo {
    /// Module id, e.g. `"com.inductiveautomation.perspective"`.
    pub id: String,
    /// Human-readable module name.
    #[serde(default)]
    pub name: String,
    /// Module version string.
    #[serde(default)]
    pub version: String,
    /// `"ACTIVE"` / … — fully-loaded modules only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(
        rename = "licenseState",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub license_state: Option<String>,
    #[serde(
        rename = "vendorName",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub vendor_name: Option<String>,
    /// The time the module started — a STRING on the wire (openapi),
    /// not epoch ms.
    #[serde(
        rename = "startupTime",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub startup_time: Option<String>,
    /// Unknown keys (`onStartup`, `shouldUpgrade`, `description`,
    /// `vendorId`, `selfSigned`, `certAccepted`, `reason`, …) round-trip.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::{ModuleInfo, Overview, StatusPing};

    /// THE live-capture regression (02-RESEARCH §Status/info): the exact
    /// overview body a commissioned 8.3.6 gateway answers with — uptime
    /// in ms, cpu a 0–1 fraction, trialRemaining in seconds, unknown
    /// keys (`cloudEnv`, `timezone`, …) preserved in `extra`.
    #[test]
    fn overview_parses_the_live_capture() {
        let body = serde_json::json!({
            "version": "8.3.6 (b2026042713)",
            "redundancy": {"role": "Independent", "activityLevel": "ACTIVE", "projectState": "RUNNING"},
            "java": {"version": "17.0.11", "vendor": "Azul Systems, Inc.", "name": "OpenJDK 64-Bit Server VM"},
            "os": {"name": "Linux", "arch": "amd64", "version": "5.15.0"},
            "cloudEnv": "unknown",
            "uptime": 338137,
            "timezone": "America/New_York",
            "locale": "en-US",
            "time": 1787346747022i64,
            "memory": [338137088i64, 1073741824i64],
            "cpu": 0.0031,
            "disk": {"total": 62661259264i64, "used": 12272824320i64},
            "license": {"state": "trial", "trialRemaining": 7017}
        });
        let overview: Overview =
            serde_json::from_value(body).expect("the live overview shape must parse");
        assert_eq!(overview.version, "8.3.6 (b2026042713)");
        assert_eq!(overview.uptime, 338137, "epoch ms, verbatim");
        assert!((overview.cpu - 0.0031).abs() < f64::EPSILON, "0–1 fraction");
        assert_eq!(overview.memory, vec![338137088i64, 1073741824i64]);
        assert_eq!(
            overview
                .license
                .as_ref()
                .expect("license block")
                .trial_remaining_s,
            Some(7017),
            "trial countdown in seconds"
        );
        assert_eq!(
            overview.java.as_ref().expect("java").vendor,
            "Azul Systems, Inc."
        );
        assert_eq!(
            overview.extra.get("cloudEnv"),
            Some(&serde_json::json!("unknown")),
            "unknown keys round-trip into extra"
        );

        // Fraction vs percent honesty: serializing keeps the wire value.
        let round = serde_json::to_value(&overview).expect("serialize");
        assert_eq!(round["cpu"], 0.0031);
        assert_eq!(round["license"]["trialRemaining"], 7017);
    }

    /// StatusPing parses the two observed states verbatim.
    #[test]
    fn status_ping_parses_observed_states() {
        for state in ["RUNNING", "STARTING"] {
            let ping: StatusPing = serde_json::from_value(serde_json::json!({ "state": state }))
                .expect("observed state parses");
            assert_eq!(ping.state, state);
        }
    }

    /// A healthy-module item parses with the fully-loaded fields AND a
    /// quarantined item (reduced shape, `reason` passthrough) parses too.
    #[test]
    fn module_info_parses_healthy_and_quarantined_shapes() {
        let healthy: ModuleInfo = serde_json::from_value(serde_json::json!({
            "id": "com.inductiveautomation.perspective",
            "name": "Perspective",
            "version": "8.3.6",
            "state": "ACTIVE",
            "licenseState": "ACTIVATED",
            "vendorName": "Inductive Automation",
            "startupTime": "2026-08-21T22:03:29Z",
            "onStartup": "ENABLE",
            "shouldUpgrade": false
        }))
        .expect("healthy item parses");
        assert_eq!(healthy.state.as_deref(), Some("ACTIVE"));
        assert_eq!(healthy.license_state.as_deref(), Some("ACTIVATED"));
        assert_eq!(
            healthy.startup_time.as_deref(),
            Some("2026-08-21T22:03:29Z")
        );
        assert_eq!(
            healthy.extra.get("onStartup"),
            Some(&serde_json::json!("ENABLE")),
            "onStartup round-trips via extra"
        );

        let quarantined: ModuleInfo = serde_json::from_value(serde_json::json!({
            "id": "com.example.broken",
            "name": "Broken Module",
            "version": "1.0.0",
            "certAccepted": false,
            "licenseAccepted": false,
            "reason": "Certificate rejected"
        }))
        .expect("quarantined items carry a REDUCED shape (openapi) — must parse");
        assert_eq!(quarantined.state, None);
        assert_eq!(quarantined.license_state, None);
        assert_eq!(
            quarantined.extra.get("reason"),
            Some(&serde_json::json!("Certificate rejected"))
        );
    }
}
