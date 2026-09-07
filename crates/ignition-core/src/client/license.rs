//! License capability model (09-04, EXT-02) — `GET /data/api/v1/licenses`,
//! field names + nesting exactly as **live-captured on both rigs**
//! (09-LIVE-CAPTURES §1: 8.3.6 rig A + 8.3.3 rig B, 2026-09-07). The
//! version-tolerance recipe is TrialWire's (client/trial.rs): every
//! `rename` + snake_case `alias` + `default`, unknown keys never
//! refuse the parse, and the remainder rides
//! `#[serde(flatten)]`.
//!
//! **The partial-curated split (capture-locked):** the capture shows a
//! flat top level — 5 arrays (`cloud`/`certificate`/`hardware`/
//! `embedded`/`leased`), one `effective` object, two scalars — with NO
//! deeper nesting to curate past. The arrays were EMPTY on both fresh
//! rigs (no activations exist), so array ELEMENT shapes are NOT
//! capture-proven: elements stay passthrough (`serde_json::Value`)
//! except `hardware`, whose `key`+`items` skeleton is the render's
//! per-key rows (modeled leniently, all-defaulted). `details` is
//! point-release-variable by the postman sample — never fully modeled,
//! `Option<serde_json::Value>` forever.
//!
//! The morning-check MODE does not live on this payload at all — the
//! trial companion (`/data/api/v1/trial`, [`TrialWire`]) owns
//! `licenseMode` (09-LIVE-CAPTURES §2: zero drift, both rigs). The
//! [`actions::license`][crate::actions::license] merge is where the two
//! reads become one command.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// GET path of the license capability (83-api collection + live capture).
pub const LICENSES_PATH: &str = "/data/api/v1/licenses";

/// GET `/data/api/v1/licenses` — the license inventory. Live-captured
/// 200 on BOTH rigs (09-LIVE-CAPTURES §1); the two bodies differ in
/// VALUES only, never shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LicenseStatusWire {
    /// Cloud-license entries — `[]` on both fresh-rig captures (no
    /// activations exist); element shape NOT capture-proven → elements
    /// ride as opaque values (the known-limitation passthrough).
    #[serde(default)]
    pub cloud: Vec<serde_json::Value>,
    /// Certificate-license entries — `[]` on both captures; passthrough
    /// elements for the same reason.
    #[serde(default)]
    pub certificate: Vec<serde_json::Value>,
    /// Per-key hardware activations — `[]` on both captures; the
    /// `key`+`items` skeleton below is modeled leniently (all-defaulted)
    /// so a populated shape can never fail the parse.
    #[serde(default)]
    pub hardware: Vec<LicenseHardware>,
    /// Embedded-license entries — `[]` on both captures; passthrough
    /// elements.
    #[serde(default)]
    pub embedded: Vec<serde_json::Value>,
    /// Leased-license entries — `[]` on both captures; passthrough
    /// elements (the Postman sample's deep lease objects are
    /// doc-generated placeholders, NOT captures — never modeled deeper).
    #[serde(default)]
    pub leased: Vec<serde_json::Value>,
    /// The effective-license stamp — present on both captures
    /// (`{"lastUpdated": 1788748551614i64}`); `default` so an absent key
    /// on a future shape still parses.
    #[serde(default)]
    pub effective: LicenseEffective,
    /// Activation-unactivate grace window — `7500` on both captures;
    /// name says MILLISECONDS (capital MS suffix), the doc comment is
    /// the capture citation. Option because only two point releases have
    /// ever been seen.
    #[serde(
        rename = "leasedUnactivateTimeoutMS",
        alias = "leased_unactivate_timeout_ms",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub leased_unactivate_timeout_ms: Option<i64>,
    /// Server mid-mutation flag — `false` on both captures (a read can
    /// race a license write; `true` tells the caller the inventory is
    /// in flux).
    #[serde(default)]
    pub busy: bool,
    /// Unknown keys round-trip (passthrough-shaped `--json`; the
    /// version-tolerance directive — point releases may add keys).
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl LicenseStatusWire {
    /// The morning-check count: total licensed ITEMS across every
    /// hardware key (`0` on a fresh trial rig — both captures answered
    /// empty arrays).
    pub fn item_count(&self) -> usize {
        self.hardware.iter().map(|entry| entry.items.len()).sum()
    }
}

/// One `hardware` entry (per license key). Element shapes were NOT
/// capturable on the fresh rigs — this skeleton is modeled LENIENTLY
/// (everything defaulted, everything flattened) exactly so a populated
/// shape parses without a second live capture. Doc comments cite the
/// lenient-modeling rule, not a capture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LicenseHardware {
    /// The license key identifier (postman sample shows a string; kept
    /// Option + defaulted — an element whose shape drifts must still
    /// parse, the TrialWire recipe).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Licensed items under this key — lenient modeling (see module
    /// doc): fresh-rig captures answered `[]`, so item shapes ride the
    /// postman-sample skeleton with every field defaulted.
    #[serde(default)]
    pub items: Vec<LicenseItem>,
    /// Unknown keys round-trip.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// One licensed item (`name`/`title`/`version` from the postman-sample
/// skeleton). `details` stays `Option<serde_json::Value>` FOREVER — it
/// is the point-release-variable deep tree the version-tolerance
/// directive exists for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LicenseItem {
    /// Module/item name (sample: e.g. `"ignition"`); Option so drift
    /// never fails the parse (render shows `-`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Display title (sample: `"Ignition"`); same lenient Option.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Version string (sample: `"8.3.6"`); same lenient Option.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The deep, point-release-variable remainder — NEVER fully
    /// modeled (the roadmap's version-tolerance directive; the
    /// module-doc known-limitation note). Passthrough verbatim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    /// Unknown keys round-trip.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// `effective` — the license-state stamp. Capture: a one-key object on
/// both rigs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LicenseEffective {
    /// Epoch **MILLISECONDS** — capture-proven by magnitude cross-check
    /// on BOTH rigs: `1788748551614` = ×1000 of the capture moment's
    /// epoch seconds (09-LIVE-CAPTURES §1 shape notes). Option +
    /// default: only two point releases observed.
    #[serde(
        rename = "lastUpdated",
        alias = "last_updated_ms",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub last_updated_ms: Option<i64>,
    /// Unknown keys round-trip.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::{LicenseHardware, LicenseStatusWire};

    /// THE 8.3.6 live capture (rig A, 09-LIVE-CAPTURES §1) — verbatim
    /// body; the fresh-trial shape every model field is doc-cited to.
    #[test]
    fn license_parses_the_live_8_3_6_capture() {
        let wire: LicenseStatusWire = serde_json::from_value(serde_json::json!({
            "cloud": [],
            "certificate": [],
            "hardware": [],
            "embedded": [],
            "leased": [],
            "effective": { "lastUpdated": 1788748551614i64 },
            "leasedUnactivateTimeoutMS": 7500,
            "busy": false
        }))
        .expect("the live 8.3.6 shape must parse");
        assert!(wire.cloud.is_empty());
        assert!(wire.hardware.is_empty());
        assert_eq!(
            wire.effective.last_updated_ms,
            Some(1_788_748_551_614),
            "epoch MILLISECONDS (magnitude cross-check)"
        );
        assert_eq!(wire.leased_unactivate_timeout_ms, Some(7500));
        assert!(!wire.busy);
        assert_eq!(wire.item_count(), 0, "fresh trial rig: zero items");
    }

    /// THE 8.3.3 live capture (rig B, 09-LIVE-CAPTURES §1) — same
    /// shape, different values: no point-release drift.
    #[test]
    fn license_parses_the_live_8_3_3_capture() {
        let wire: LicenseStatusWire = serde_json::from_value(serde_json::json!({
            "cloud": [],
            "certificate": [],
            "hardware": [],
            "embedded": [],
            "leased": [],
            "effective": { "lastUpdated": 1788748731641i64 },
            "leasedUnactivateTimeoutMS": 7500,
            "busy": false
        }))
        .expect("the live 8.3.3 shape must parse");
        assert_eq!(wire.effective.last_updated_ms, Some(1_788_748_731_641));
    }

    /// A POPULATED hardware entry parses through the lenient skeleton
    /// (postman-sample shape, all fields defaulted, details passthrough)
    /// — the array-element shapes were NOT capture-proven, so populated
    /// drift must ride, never refuse.
    #[test]
    fn license_parses_a_populated_hardware_entry_leniently() {
        let wire: LicenseStatusWire = serde_json::from_value(serde_json::json!({
            "hardware": [{
                "key": "0123456789",
                "items": [{
                    "name": "ignition",
                    "title": "Ignition",
                    "version": "8.3.6",
                    "details": { "state": "Active", "expirationDate": null,
                                 "whateverFuturePoint": {"nested": true} }
                }]
            }]
        }))
        .expect("a populated hardware entry must parse leniently");
        assert_eq!(wire.item_count(), 1);
        let entry: &LicenseHardware = &wire.hardware[0];
        assert_eq!(entry.key.as_deref(), Some("0123456789"));
        let item = &entry.items[0];
        assert_eq!(item.name.as_deref(), Some("ignition"));
        assert_eq!(item.title.as_deref(), Some("Ignition"));
        assert_eq!(item.version.as_deref(), Some("8.3.6"));
        assert!(
            item.details.is_some(),
            "details rides passthrough, never modeled"
        );
    }

    /// The unknown-key round-trip: a fake future point-release key
    /// appended to the live capture still parses AND lands in `extra`
    /// (the passthrough proof).
    #[test]
    fn license_unknown_keys_ride_extra() {
        let wire: LicenseStatusWire = serde_json::from_value(serde_json::json!({
            "cloud": [],
            "certificate": [],
            "hardware": [],
            "embedded": [],
            "leased": [],
            "effective": { "lastUpdated": 1788748551614i64 },
            "leasedUnactivateTimeoutMS": 7500,
            "busy": false,
            "brandNewPointReleaseKey": {"future": true}
        }))
        .expect("unknown keys must never refuse the parse");
        assert_eq!(
            wire.extra.get("brandNewPointReleaseKey"),
            Some(&serde_json::json!({"future": true})),
            "the unknown key rides flatten passthrough"
        );
    }
}
