//! Deployment modes and the gateway's active `deploymentMode` (20-05,
//! DAPI-05).
//!
//! Wire facts (8.3.6, `20-LIVE-CAPTURES.md`, cited by block id):
//!
//! - `GET /data/api/v1/mode` answers the `{items, metadata}` envelope; an
//!   item is `{name, title, description, resourceCount}` (C-mode "GET /mode
//!   (before)" and "(after)").
//! - `POST /data/api/v1/mode` with `{name, title, description}` answers 200
//!   with the created `ModeRecord` (D6). A duplicate is a 409 and an illegal
//!   name a 400, both Jetty HTML (D6). Creating a mode only DEFINES it: the
//!   gateway's active mode is set by a JVM argument at start, which this
//!   library never touches.
//! - `GET /data/api/v1/gateway-info` carries `deploymentMode`: `""` on a
//!   gateway with no mode and `"production"` on one started with
//!   `-Dignition.config.mode=production` (C-null (no mode), C-null (mode
//!   production), D5). `null` and an absent key were not observed on 8.3.6.
//!
//! `deployment_mode()` returns `Option<String>` (research A6, held by A6 in
//! the capture table) rather than the `""` sentinel milestone SUMMARY
//! Conflict 10 names. Both carry the same intent: no mode is the absence of
//! a name. [`normalize_deployment_mode`] reads `null`, `""`, an absent key
//! and a whitespace-only string all as no mode, so a gateway version that
//! sends any of them behaves the same.
//!
//! The v1.0 `gateway_info_contract` golden body carries
//! `"deploymentMode": "STANDARD"`. That value was never confirmed by a
//! capture (02-RESEARCH records the key without a value); D5 showed `""`
//! with no mode. `STANDARD` is therefore treated as a mode name like any
//! other string: nothing here special-cases it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// `GET` (list) and `POST` (create) path of the mode collection (C-mode).
pub const MODE_PATH: &str = "/data/api/v1/mode";

/// One deployment mode as the gateway lists it (C-mode). Lenient: a key
/// the captures do not show rides in `extra`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModeRecord {
    /// The mode name, e.g. `production`.
    #[serde(default)]
    pub name: String,
    /// Display title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Free-text description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Number of resources the mode's collection holds (0 on the capture).
    #[serde(
        rename = "resourceCount",
        alias = "resource_count",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub resource_count: Option<i64>,
    /// Keys this client does not model.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// POST body of a mode create (C-mode "POST /mode production"). Field order
/// is the wire order, so the recorded request body is deterministic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModeCreate {
    /// The mode name; validated before any request.
    pub name: String,
    /// Display title.
    pub title: String,
    /// Free-text description.
    pub description: String,
}

/// The one gateway-info key `deployment_mode()` reads. A private probe
/// struct, so [`crate::client::version::GatewayInfo`] and its `--json`
/// output stay frozen. Every other gateway-info key is ignored.
#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct GatewayModeProbe {
    /// `deploymentMode`: `""` with no mode, the mode name otherwise (D5).
    #[serde(rename = "deploymentMode", default)]
    pub(crate) deployment_mode: Option<String>,
}

/// Read a raw `deploymentMode` as an optional mode name.
///
/// `None` for an absent key, `null`, an empty string and a whitespace-only
/// string (D5: the 8.3.6 no-mode value is `""`). Any other string is
/// `Some(value)` verbatim, never trimmed or case-folded: a mode name is
/// compared exactly. D5 recorded no distinct no-mode sentinel string.
pub fn normalize_deployment_mode(raw: Option<String>) -> Option<String> {
    raw.filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_table() {
        assert_eq!(normalize_deployment_mode(None), None);
        assert_eq!(normalize_deployment_mode(Some(String::new())), None);
        assert_eq!(normalize_deployment_mode(Some("   ".to_string())), None);
        assert_eq!(normalize_deployment_mode(Some("\t\n".to_string())), None);
        assert_eq!(
            normalize_deployment_mode(Some("production".to_string())),
            Some("production".to_string())
        );
        // Verbatim: no trimming or case folding of a real name.
        assert_eq!(
            normalize_deployment_mode(Some(" Production ".to_string())),
            Some(" Production ".to_string())
        );
    }

    #[test]
    fn probe_reads_null_absent_and_string() {
        let null: GatewayModeProbe =
            serde_json::from_str(r#"{"deploymentMode":null,"name":"x"}"#).expect("null");
        assert_eq!(null.deployment_mode, None);
        let absent: GatewayModeProbe = serde_json::from_str(r#"{"name":"x"}"#).expect("absent");
        assert_eq!(absent.deployment_mode, None);
        let empty: GatewayModeProbe =
            serde_json::from_str(r#"{"deploymentMode":""}"#).expect("empty");
        assert_eq!(empty.deployment_mode.as_deref(), Some(""));
    }

    #[test]
    fn captured_mode_body_parses() {
        // C-mode "POST /mode production", verbatim.
        let record: ModeRecord = serde_json::from_str(
            "{\n  \"name\": \"production\",\n  \"title\": \"Production\",\n  \"description\": \"ign-deploy-rig\",\n  \"resourceCount\": 0\n}",
        )
        .expect("C-mode parses");
        assert_eq!(record.name, "production");
        assert_eq!(record.title.as_deref(), Some("Production"));
        assert_eq!(record.resource_count, Some(0));
        assert!(record.extra.is_empty());
    }

    #[test]
    fn create_body_field_order_is_the_wire_order() {
        let body = serde_json::to_string(&ModeCreate {
            name: "production".to_string(),
            title: "Production".to_string(),
            description: "x".to_string(),
        })
        .expect("serializes");
        assert_eq!(
            body,
            r#"{"name":"production","title":"Production","description":"x"}"#
        );
    }
}
