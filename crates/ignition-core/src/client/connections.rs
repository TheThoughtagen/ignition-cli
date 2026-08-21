//! DB/OPC connection capability models (02-03, HLTH-05/06).
//!
//! The REAL mechanism is the web UI's Connections-page poll — the
//! resource-list endpoints (network-captured, 02-RESEARCH §DB / OPC
//! connection status). The ignition-mcp `/data/api/v1/connections/*`
//! paths are inventions (404) and appear nowhere here;
//! `/data/api/v1/overview/connections` (web-UI presentation objects) is
//! deliberately NOT used for the list either.
//!
//! ⚠ LOW-CONFIDENCE SHAPE (02-RESEARCH Open Question 1): the research
//! gateway had ZERO connections configured — the envelope + mechanism
//! are verified, but the POPULATED `healthchecks` detail is not.
//! [`GatewayConnection::healthchecks`] is therefore a RAW passthrough
//! [`serde_json::Value`], rendered as-is; the live check
//! (`live_connections` in tests/live_gateway.rs) exists to capture the
//! populated shape against a gateway that HAS a connection (recorded as
//! an open question for UAT until then).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// GET path of the database-connection resource list (web-UI poll,
/// network-captured).
pub(crate) const DATABASE_CONNECTIONS_PATH: &str =
    "/data/api/v1/resources/list/ignition/database-connection";

/// GET path of the OPC-connection resource list (same family).
pub(crate) const OPC_CONNECTIONS_PATH: &str = "/data/api/v1/resources/list/ignition/opc-connection";

/// One item of the connection resource lists — name/enabled plus the
/// `healthchecks` map EXACTLY as the gateway reports it (raw
/// passthrough; see the module docs for the LOW-confidence flag).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GatewayConnection {
    /// Resource name, e.g. `"MyPostgres"`.
    #[serde(default)]
    pub name: String,
    /// Whether the connection resource is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Healthcheck status map — RAW passthrough (`Value`): the populated
    /// shape is LOW-confidence until captured against a gateway with a
    /// configured connection (research Open Question 1). Rendered
    /// as-is, never interpreted.
    #[serde(default)]
    pub healthchecks: serde_json::Value,
    /// `collection`, `signature`, `config`, … resource keys round-trip.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::GatewayConnection;

    /// A plausible resource-list item parses with the passthrough
    /// healthchecks map and unknown config keys preserved.
    #[test]
    fn gateway_connection_parses_with_passthrough_healthchecks() {
        let connection: GatewayConnection = serde_json::from_value(serde_json::json!({
            "name": "MyPostgres",
            "enabled": true,
            "healthchecks": {"jdbc": "FAIR"},
            "collection": "database-connections",
            "config": {"driver": "postgresql"}
        }))
        .expect("plausible resource item must parse");
        assert_eq!(connection.name, "MyPostgres");
        assert!(connection.enabled);
        assert_eq!(
            connection.healthchecks["jdbc"], "FAIR",
            "healthchecks ride through UNINTERPRETED"
        );
        assert_eq!(
            connection.extra.get("collection"),
            Some(&serde_json::json!("database-connections")),
            "resource keys round-trip"
        );

        // An item WITHOUT a healthchecks map still parses (the research
        // gateway's empty list could not confirm its presence).
        let bare: GatewayConnection = serde_json::from_value(serde_json::json!({
            "name": "Bare",
            "enabled": false
        }))
        .expect("healthchecks may be absent — default Value::Null");
        assert_eq!(bare.healthchecks, serde_json::Value::Null);
    }
}
