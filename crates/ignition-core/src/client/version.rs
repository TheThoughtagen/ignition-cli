//! Gateway-info model + the minimum-version gate (CORE-08).
//!
//! Field names match the **live 8.3.6 gateway** exactly (captured
//! 2026-08-21, 02-RESEARCH §Status/info + the gateway's own openapi
//! schema): the version field is `ignitionVersion` — Phase 1's `version`
//! name failed deserialization against every real gateway, fixed here
//! with an alias that tolerates any 8.3.x still shipping the old name.
//! `state`/`uptime` do NOT exist on this payload (running state and
//! uptime come from `/overview` + `/StatusPing` in 02-02) — the model
//! stays truthful to what the endpoint returns.

use serde::{Deserialize, Serialize};

/// Minimum gateway version `ign` supports (CORE-08). Appears in the
/// `gateway_too_old` envelope and its hint.
pub const MIN_GATEWAY: &str = "8.3.1";

/// `/data/api/v1/gateway-info` response — field names match the gateway.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GatewayInfo {
    /// Gateway display name, when reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Redundancy role (`"Independent"` / `"Backup"` / `"Primary"`), when
    /// reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redundancy_role: Option<String>,
    /// Edition (`"standard"` / `"maker"` / …), when reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edition: Option<String>,
    /// Version + build revision in one string, e.g.
    /// `"8.3.6 (b2026042713)"`. Serialized under the gateway-native key
    /// (`ignitionVersion`, passthrough-shaped `--json` data); the alias
    /// tolerates any 8.3.x still shipping the Phase-1-era `version` name.
    #[serde(rename = "ignitionVersion", alias = "version")]
    pub ignition_version: String,
    /// JVM version passthrough, when reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jvm_version: Option<String>,
    /// License summary, when reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<LicenseInfo>,
    /// The request URL (populated by the client, never serialized) so
    /// error variants built from this info can carry `endpoint` (CORE-05).
    #[serde(skip, default)]
    pub endpoint: Option<String>,
}

/// License block of gateway-info (`license: {mode, expirationDate, …}`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LicenseInfo {
    /// `"Trial"` / `"Licensed"` / …
    pub mode: String,
    /// Trial/license expiration, when reported. Serialized under the
    /// gateway-native key (camelCase, like `ignitionVersion`); the
    /// snake_case alias tolerates non-gateway shapers. (02-02 found the
    /// missing rename silently DROPPED the gateway's `expirationDate`
    /// on parse — the 02-01 fix covered `ignitionVersion` only.)
    #[serde(
        rename = "expirationDate",
        alias = "expiration_date",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub expiration_date: Option<String>,
}

/// Is `raw` below [`MIN_GATEWAY`]? Suffix- and short-form tolerant;
/// unparseable → `true` — refuse safely rather than guess (CORE-08).
///
/// Note: the comparison target is the plain three-component semver
/// `8.3.1` — `semver::Version::parse` is strict, so a four-component
/// literal like `"8.3.1.0"` would NOT parse (the research sketch's
/// `.0`-appended constant would have made EVERY comparison fail).
pub fn below_minimum(raw: &str) -> bool {
    // Gateway versions are dotted triples, sometimes with a "-SNAPSHOT…"
    // or space suffix — compare the leading numeric part only.
    let clean = raw.trim().split(['-', ' ']).next().unwrap_or(raw);
    // Tolerate the short "8.3" form by appending a patch component.
    let normalized = if clean.matches('.').count() == 1 {
        format!("{clean}.0")
    } else {
        clean.to_string()
    };
    match semver::Version::parse(&normalized) {
        Ok(version) => {
            version < semver::Version::parse(MIN_GATEWAY).expect("MIN_GATEWAY is valid semver")
        }
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::{GatewayInfo, LicenseInfo, MIN_GATEWAY, below_minimum};

    /// CORE-08 boundary table — every row of the locked comparison matrix,
    /// including the EXACT live strings captured from 8.3.6 (02-RESEARCH).
    #[test]
    fn below_minimum_matrix() {
        // (raw, expected below minimum?)
        let cases = [
            ("8.3.1", false),                   // exactly the minimum
            ("8.3.2", false),                   // above
            ("8.3.0", true),                    // one patch below
            ("8.1.10", true),                   // older minor line
            ("8.3", true),                      // short form → 8.3.0
            ("8.3.1-SNAPSHOT.20260801", false), // suffix stripped → equal
            (" 8.3.2 ", false),                 // surrounding space tolerated
            ("garbage", true),                  // unparseable → refuse
            ("", true),                         // empty → refuse
            ("9.0.0", false),                   // future major
            // Live-capture rows (8.3.6, b-build suffix in parens).
            ("8.3.6 (b2026042713)", false), // the exact live string
            ("8.3.6", false),               // its bare prefix
        ];
        for (raw, expected) in cases {
            assert_eq!(
                below_minimum(raw),
                expected,
                "below_minimum({raw:?}) must be {expected}"
            );
        }
        assert_eq!(MIN_GATEWAY, "8.3.1");
    }

    /// The live-shape regression at the model level: the field is
    /// `ignitionVersion` (camelCase from the gateway) and the Phase-1-era
    /// `version` name still parses via the alias — both must deserialize.
    #[test]
    fn gateway_info_parses_live_and_legacy_field_names() {
        let live = serde_json::json!({
            "name": "ign-live",
            "redundancyRole": "Independent",
            "edition": "standard",
            "ignitionVersion": "8.3.6 (b2026042713)",
            "jvmVersion": "17.0.11",
            "license": {"mode": "Trial", "expirationDate": "2026-08-21"}
        });
        let info: GatewayInfo =
            serde_json::from_value(live).expect("live ignitionVersion shape parses");
        assert_eq!(info.ignition_version, "8.3.6 (b2026042713)");
        assert_eq!(info.name.as_deref(), Some("ign-live"));
        assert_eq!(info.license.as_ref().expect("license").mode, "Trial");

        let legacy = serde_json::json!({"version": "8.3.2"});
        let info: GatewayInfo =
            serde_json::from_value(legacy).expect("legacy `version` name still parses (alias)");
        assert_eq!(info.ignition_version, "8.3.2");
    }

    /// The `--json` data key is the gateway-native `ignitionVersion`
    /// (passthrough shape; LOCKED additive field).
    #[test]
    fn gateway_info_serializes_under_the_gateway_native_key() {
        let info = GatewayInfo {
            name: Some("ign-live-rig".into()),
            redundancy_role: Some("Independent".into()),
            edition: Some("standard".into()),
            ignition_version: "8.3.6 (b2026042713)".into(),
            jvm_version: None,
            license: Some(LicenseInfo {
                mode: "Trial".into(),
                expiration_date: None,
            }),
            endpoint: None,
        };
        let json = serde_json::to_value(&info).expect("serialize");
        assert_eq!(json["ignitionVersion"], "8.3.6 (b2026042713)");
        assert_eq!(json["name"], "ign-live-rig");
        assert_eq!(json["license"]["mode"], "Trial");
        assert!(
            json.get("endpoint").is_none(),
            "endpoint is never serialized (CORE-05 skip)"
        );
    }
}
