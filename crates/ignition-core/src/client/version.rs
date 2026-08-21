//! Gateway-info model + the minimum-version gate (CORE-08).

use serde::{Deserialize, Serialize};

/// Minimum gateway version `ign` supports (CORE-08). Appears in the
/// `gateway_too_old` envelope and its hint.
pub const MIN_GATEWAY: &str = "8.3.1";

/// `/data/api/v1/gateway-info` response (field names match the gateway).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GatewayInfo {
    /// Gateway version string, e.g. `"8.3.2"` or `"8.3.1-SNAPSHOT.20260801"`.
    pub version: String,
    /// Edition (Standard / Maker / …), when the gateway reports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edition: Option<String>,
    /// Gateway state (RUNNING / …), when reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// Uptime in whatever unit/shape the gateway reports — passed through.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uptime: Option<serde_json::Value>,
    /// The request URL (populated by the client, never serialized) so
    /// error variants built from this info can carry `endpoint` (CORE-05).
    #[serde(skip, default)]
    pub endpoint: Option<String>,
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
    use super::{MIN_GATEWAY, below_minimum};

    /// CORE-08 boundary table — every row of the locked comparison matrix.
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
}
