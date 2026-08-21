//! Session-family capability models (02-03, HLTH-08) — designer
//! sessions, Perspective sessions, and Vision clients, field names
//! matching the live 8.3.6 captures (02-RESEARCH §Sessions + terminate)
//! and the gateway's openapi schema.
//!
//! Every model carries `#[serde(flatten)] extra` passthrough so `--json`
//! stays complete as gateway responses evolve.
//!
//! Two path subtleties are pinned here (Pitfall 8 + spec):
//! - the Perspective LIST is `/data/perspective/api/v1/sessions/` WITH a
//!   trailing slash (the module-scoped prefix differs from core
//!   `/data/api/v1` — base-URL joining must not collapse it; pinned by
//!   an exact-path wiremock matcher in tests/sessions_contract.rs);
//! - the Perspective TERMINATE is a DELETE against the same path WITHOUT
//!   the trailing slash, carrying `sessionId` (+ optional `message`) as
//!   QUERY params, never a body.
//!
//! All three terminations are audit-logged server-side (official docs) —
//! nothing to do client-side beyond the call itself.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// GET path of the designer-sessions list.
pub(crate) const DESIGNERS_PATH: &str = "/data/api/v1/designers";

/// GET path of the Perspective sessions list — the EXACT trailing slash
/// is part of the contract (spec; Pitfall 8).
pub(crate) const PERSPECTIVE_SESSIONS_LIST_PATH: &str = "/data/perspective/api/v1/sessions/";

/// DELETE path of the Perspective terminate route — NO trailing slash
/// (spec), with `sessionId` (+ optional `message`) as query params.
pub(crate) const PERSPECTIVE_SESSIONS_TERMINATE_PATH: &str = "/data/perspective/api/v1/sessions";

/// GET path of the Vision clients list.
pub(crate) const VISION_CLIENTS_PATH: &str = "/data/vision/api/v1/clients";

/// DELETE path of the designer prune route (singular `designer`).
pub(crate) fn designer_prune_path(id: &str) -> String {
    format!("/data/api/v1/designer/{id}")
}

/// DELETE path of the Vision client terminate route (singular `client`).
pub(crate) fn vision_client_terminate_path(id: &str) -> String {
    format!("/data/vision/api/v1/client/{id}")
}

/// One item of `GET /data/api/v1/designers` (openapi: `id`, `user`,
/// `uptime`, `lastcomm`, `timeout`, `memory` (an OBJECT — heap
/// breakdown), `project`, `address`, `timezone`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesignerInfo {
    /// Session id.
    pub id: String,
    /// Remote designer address, e.g. `"192.168.1.50:52526"`.
    #[serde(default)]
    pub address: String,
    /// Authenticated designer user.
    #[serde(default)]
    pub user: String,
    /// Open project name.
    #[serde(default)]
    pub project: String,
    /// Heap breakdown as the gateway reports it (openapi: object) —
    /// raw passthrough, unit semantics are the gateway's.
    #[serde(default)]
    pub memory: serde_json::Value,
    /// Session uptime in epoch **MILLISECONDS** (same scale as
    /// `overview.uptime`).
    #[serde(default)]
    pub uptime: i64,
    /// Last communication timestamp, epoch **MILLISECONDS**.
    #[serde(default)]
    pub lastcomm: i64,
    /// Session timeout, epoch **MILLISECONDS**.
    #[serde(default)]
    pub timeout: i64,
    /// Designer-side timezone id.
    #[serde(default)]
    pub timezone: String,
    /// Unknown keys round-trip (passthrough-shaped `--json`).
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// One item of `GET /data/perspective/api/v1/sessions/` (openapi:
/// camelCase keys — serde-renamed; `sessionScope`, `pageIds`,
/// `recentBytesSent`, `totalBytesSent` ride the passthrough).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerspectiveSession {
    /// Session id (the `sessionId` the terminate route takes).
    pub id: String,
    /// Authenticated Perspective user (`"anonymous"` when public).
    #[serde(default)]
    pub username: String,
    /// Whether the session passed login.
    #[serde(default)]
    pub authorized: bool,
    /// Open project name.
    #[serde(default)]
    pub project: String,
    /// Browser address (`clientAddress` on the wire).
    #[serde(
        rename = "clientAddress",
        alias = "client_address",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub client_address: String,
    /// Last communication timestamp, epoch **MILLISECONDS**
    /// (`lastComm` on the wire).
    #[serde(rename = "lastComm", alias = "last_comm", default)]
    pub last_comm: i64,
    /// Number of pages the session holds open (`activePages`).
    #[serde(rename = "activePages", alias = "active_pages", default)]
    pub active_pages: i64,
    /// Browser user agent (`userAgent`).
    #[serde(
        rename = "userAgent",
        alias = "user_agent",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub user_agent: String,
    /// `sessionScope`, `pageIds`, `recentBytesSent`, `totalBytesSent`,
    /// … round-trip.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// One item of `GET /data/vision/api/v1/clients` — the same shape as
/// [`DesignerInfo`] plus `tagCount` (openapi).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisionClient {
    /// Client id.
    pub id: String,
    /// Remote client address.
    #[serde(default)]
    pub address: String,
    /// Authenticated client user.
    #[serde(default)]
    pub user: String,
    /// Open project name.
    #[serde(default)]
    pub project: String,
    /// Heap breakdown as the gateway reports it (openapi: object).
    #[serde(default)]
    pub memory: serde_json::Value,
    /// Client uptime in epoch **MILLISECONDS**.
    #[serde(default)]
    pub uptime: i64,
    /// Last communication timestamp, epoch **MILLISECONDS**.
    #[serde(default)]
    pub lastcomm: i64,
    /// Client timeout, epoch **MILLISECONDS**.
    #[serde(default)]
    pub timeout: i64,
    /// Client-side timezone id.
    #[serde(default)]
    pub timezone: String,
    /// Subscribed tag count (`tagCount` on the wire).
    #[serde(rename = "tagCount", alias = "tag_count", default)]
    pub tag_count: i64,
    /// Unknown keys round-trip.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::{DesignerInfo, PerspectiveSession, VisionClient};

    /// The openapi item shapes parse: designers carry an OBJECT `memory`
    /// and ms-scale numerics; unknown keys round-trip.
    #[test]
    fn designer_info_parses_the_openapi_shape() {
        let designer: DesignerInfo = serde_json::from_value(serde_json::json!({
            "id": "d-1",
            "user": "admin",
            "uptime": 600000,
            "lastcomm": 1787346747022i64,
            "timeout": 3600000,
            "memory": {"used": 268435456i64, "max": 1073741824i64},
            "project": "MyProject",
            "address": "192.168.1.50:52526",
            "timezone": "America/New_York"
        }))
        .expect("openapi designer shape must parse");
        assert_eq!(designer.id, "d-1");
        assert_eq!(designer.user, "admin");
        assert_eq!(designer.uptime, 600000, "epoch ms");
        assert_eq!(
            designer.memory["used"], 268435456i64,
            "memory is an OBJECT (openapi) — passthrough"
        );

        // Wire-faithful round-trip: keys unchanged, extras preserved.
        let round = serde_json::to_value(&designer).expect("serialize");
        assert_eq!(round["memory"]["max"], 1073741824i64);
        assert_eq!(round["timezone"], "America/New_York");
    }

    /// Perspective items parse with the camelCase renames; the not-modeled
    /// known keys (`sessionScope`, `pageIds`, byte counters) ride extras.
    #[test]
    fn perspective_session_parses_the_camel_case_shape() {
        let session: PerspectiveSession = serde_json::from_value(serde_json::json!({
            "id": "psess-1",
            "username": "admin",
            "authorized": true,
            "project": "MyProject",
            "clientAddress": "10.0.0.5",
            "lastComm": 1787346747022i64,
            "sessionScope": "G",
            "activePages": 2,
            "pageIds": ["viewA", "viewB"],
            "recentBytesSent": 1024,
            "totalBytesSent": 4096,
            "userAgent": "Mozilla/5.0"
        }))
        .expect("perspective capture shape must parse");
        assert_eq!(session.id, "psess-1");
        assert_eq!(session.client_address, "10.0.0.5", "clientAddress rename");
        assert_eq!(session.last_comm, 1787346747022, "lastComm rename");
        assert_eq!(session.active_pages, 2, "activePages rename");
        assert_eq!(session.user_agent, "Mozilla/5.0", "userAgent rename");
        assert_eq!(
            session.extra.get("sessionScope"),
            Some(&serde_json::json!("G")),
            "unmodeled known keys round-trip"
        );

        let round = serde_json::to_value(&session).expect("serialize");
        assert_eq!(round["clientAddress"], "10.0.0.5");
        assert_eq!(round["userAgent"], "Mozilla/5.0");
    }

    /// Vision clients = designer shape + `tagCount`.
    #[test]
    fn vision_client_parses_designer_shape_plus_tag_count() {
        let client: VisionClient = serde_json::from_value(serde_json::json!({
            "id": "v-1",
            "user": "operator",
            "uptime": 120000,
            "lastcomm": 1787346747022i64,
            "timeout": 3600000,
            "memory": {"used": 134217728i64, "max": 536870912i64},
            "project": "PlantFloor",
            "address": "10.0.0.9:443",
            "timezone": "UTC",
            "tagCount": 1523
        }))
        .expect("vision capture shape must parse");
        assert_eq!(client.id, "v-1");
        assert_eq!(client.tag_count, 1523, "tagCount rename");

        let round = serde_json::to_value(&client).expect("serialize");
        assert_eq!(round["tagCount"], 1523, "gateway-native key on the way out");
    }
}
