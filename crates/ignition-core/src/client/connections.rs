//! DB/OPC connection and OPC UA device capability models (02-03,
//! HLTH-05/06).
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

/// GET path of the OPC UA device resource list (same resource-list
/// mechanism, module-scoped type `com.inductiveautomation.opcua/device`).
pub(crate) const DEVICE_CONNECTIONS_PATH: &str =
    "/data/api/v1/resources/list/com.inductiveautomation.opcua/device";

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

/// One OPC UA device as `ign connections --type device` reports it.
///
/// Deliberately NOT a passthrough like [`GatewayConnection`]: the wire
/// item's `config.settings` carries hostnames/IPs and can carry
/// credentials, so [`GatewayDeviceWire`] reads only the driver name out
/// of `config` and drops the rest at the deserialization boundary — the
/// settings never reach a value that can be serialized, rendered or
/// logged. Serialize-only for the same reason: there is no path back to
/// the wire shape.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GatewayDevice {
    /// Device name, e.g. `"Device-A"`.
    pub name: String,
    /// The resource description (`null` when unset).
    pub description: Option<String>,
    /// Whether the device resource is enabled.
    pub enabled: bool,
    /// Driver / device type from `config.profile.type` (`LogixDriver`,
    /// `ModbusTcp`, …); `null` when the gateway reports none.
    pub driver: Option<String>,
    /// The `healthchecks` map EXACTLY as the gateway reports it
    /// (`{"status": {"name": …, "result": {"healthy", "time",
    /// "message"}}}`); `null` when the item carries none. Never
    /// interpreted in the JSON output — see [`GatewayDevice::health`]
    /// for the human renderer's reading.
    pub healthchecks: serde_json::Value,
}

/// The `status` healthcheck result of one device, read out of the raw
/// [`GatewayDevice::healthchecks`] map. A device with no (or an
/// unreadable) `status` healthcheck is UNKNOWN (`healthy: None`) —
/// never healthy by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceHealth<'a> {
    /// `result.healthy` — `None` when the gateway reported no verdict.
    pub healthy: Option<bool>,
    /// `result.message` (`Connected`, `Connecting`, `1/1 Connected`, …).
    pub message: Option<&'a str>,
}

impl GatewayDevice {
    /// The device's `status` healthcheck verdict and message.
    pub fn health(&self) -> DeviceHealth<'_> {
        let result = self.healthchecks.pointer("/status/result");
        DeviceHealth {
            healthy: result
                .and_then(|r| r.get("healthy"))
                .and_then(serde_json::Value::as_bool),
            message: result
                .and_then(|r| r.get("message"))
                .and_then(serde_json::Value::as_str),
        }
    }
}

/// The device resource-list item as the gateway sends it. `config` is
/// held as a raw [`serde_json::Value`] only long enough to lift the
/// driver name out; nothing else in it is ever read (see
/// [`GatewayDevice`]).
///
/// No `Debug` derive on purpose: `config` holds the settings that must
/// never be printed.
#[derive(Deserialize)]
pub(crate) struct GatewayDeviceWire {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    config: serde_json::Value,
    #[serde(default)]
    healthchecks: serde_json::Value,
}

impl From<GatewayDeviceWire> for GatewayDevice {
    fn from(wire: GatewayDeviceWire) -> Self {
        let driver = wire
            .config
            .pointer("/profile/type")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        Self {
            name: wire.name,
            description: wire.description,
            enabled: wire.enabled,
            driver,
            healthchecks: wire.healthchecks,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DeviceHealth, GatewayConnection, GatewayDevice, GatewayDeviceWire};

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

    fn device(wire: serde_json::Value) -> GatewayDevice {
        serde_json::from_value::<GatewayDeviceWire>(wire)
            .expect("device item must parse")
            .into()
    }

    /// A populated device item keeps name/description/enabled/driver and
    /// the healthchecks map verbatim, and DROPS `config.settings` (host,
    /// credentials) so it can never serialize.
    #[test]
    fn gateway_device_keeps_driver_and_healthchecks_and_drops_settings() {
        let item = device(serde_json::json!({
            "name": "Device-A",
            "description": "Line 1 PLC",
            "enabled": true,
            "config": {
                "profile": {"type": "LogixDriver"},
                "settings": {
                    "connectivity": {"hostname": "192.0.2.10"},
                    "advanced": {"password": "hunter2"}
                }
            },
            "healthchecks": {"status": {
                "name": "devices.device-Device-A.status",
                "result": {"healthy": true, "time": 1_700_000_000_000_i64, "message": "Connected"}
            }},
            "collection": "core",
            "signature": "abc"
        }));
        assert_eq!(item.name, "Device-A");
        assert_eq!(item.description.as_deref(), Some("Line 1 PLC"));
        assert!(item.enabled);
        assert_eq!(item.driver.as_deref(), Some("LogixDriver"));
        assert_eq!(
            item.health(),
            DeviceHealth {
                healthy: Some(true),
                message: Some("Connected")
            }
        );

        let json = serde_json::to_string(&item).expect("serialize");
        for leaked in ["192.0.2.10", "hunter2", "settings", "hostname", "signature"] {
            assert!(!json.contains(leaked), "{leaked} leaked into {json}");
        }
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&json).unwrap()["healthchecks"]["status"]["result"]
                ["message"],
            "Connected",
            "healthchecks ride through verbatim"
        );
    }

    /// No healthchecks / no profile / null description: parses, and the
    /// device is UNKNOWN health — never healthy.
    #[test]
    fn gateway_device_without_healthchecks_is_unknown_never_healthy() {
        let item = device(serde_json::json!({"name": "Bare", "description": null}));
        assert_eq!(item.driver, None);
        assert_eq!(item.description, None);
        assert!(!item.enabled);
        assert_eq!(item.healthchecks, serde_json::Value::Null);
        assert_eq!(
            item.health(),
            DeviceHealth {
                healthy: None,
                message: None
            }
        );

        // A status block the reader cannot interpret is unknown too.
        let odd = device(serde_json::json!({
            "name": "Odd",
            "healthchecks": {"status": {"result": {"healthy": "yes"}}}
        }));
        assert_eq!(odd.health().healthy, None);
    }
}
