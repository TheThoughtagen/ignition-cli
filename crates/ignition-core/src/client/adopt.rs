//! API-key adoption wire models + session-tier calls (ADOPT-01).
//!
//! Every shape here is LIVE-CAPTURED on the `ignition-devops` rig,
//! Ignition 8.3.6 (b2026042713), 2026-09-18 — browser-devtools capture
//! while clicking Config → Security → API Keys / General Settings as
//! `admin`, the same method as the trial route. Full bodies + the
//! capture narrative: `.planning/research/ADOPT-RESEARCH.md`.
//!
//! ## The wire (live-proven end-to-end, key minted + used same second)
//!
//! 8.3 API keys are NOT a bespoke route family — they are resource
//! type `ignition/api-token` on the resources API, plus ONE generate
//! endpoint for the key material:
//!
//! 1. `GET  /data/api/v1/resources/list/ignition/api-token` — the
//!    idempotent find-by-name (hash only; the plaintext key appears
//!    NOWHERE in this list).
//! 2. `POST /data/api/v1/api-token/generate` — `{key, hash}`; the
//!    plaintext key is returned HERE AND ONLY HERE.
//! 3. `POST /data/api/v1/resources/ignition/api-token` — array-of-one
//!    resource record carrying the hash (NOT the key).
//! 4. The security-properties singleton — the permissions wiring
//!    (doctor's part-2 diagnosis, made writable). The GET is
//!    `/data/api/v1/resources/singleton/ignition/security-properties`
//!    (carrying `?defaultIfUndefined=true`); the PUT is
//!    `/data/api/v1/resources/ignition/security-properties`.
//!
//! All four ride the native OIDC session cookie + `X-CSRF-Token` —
//! the [`crate::client::idp`] machinery verbatim (tier 1). This is
//! the bootstrap credential path: adopt runs BEFORE any API token
//! exists, so every call is session-tier.
//!
//! ## Live-discovered pitfalls (both documented in ADOPT-RESEARCH)
//!
//! - The security-properties PUT REPLACES the whole singleton config
//!   and carries the GET's `signature` (optimistic concurrency — it
//!   rotates on every write). Merge read-modify-write; never blind.
//! - `securityLevels: []` on an AnyOf permission means PUBLIC
//!   (everyone), not "nobody" — the UI serializes a checked Public
//!   as an empty list. [`merge_level_into`] only ever ADDS.
//! - Security levels are leaf-path TREES: granting
//!   `Authenticated/Roles/Administrator` serializes as ONE root
//!   `{name:"Authenticated", children:[{Roles, children:[{Administrator}]}]}`
//!   ([`level_tree`]).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json::json;

use crate::client::idp::GatewaySession;
use crate::client::idp::IdpLoginFlow;
use crate::error::CoreError;

/// The key-material generator (live-captured response below).
pub(crate) const GENERATE_PATH: &str = "/data/api/v1/api-token/generate";

/// The api-token resource list (idempotent find-by-name).
pub(crate) const API_TOKEN_LIST_PATH: &str = "/data/api/v1/resources/list/ignition/api-token";

/// The api-token resource CREATE (array-of-one body).
pub(crate) const API_TOKEN_CREATE_PATH: &str = "/data/api/v1/resources/ignition/api-token";

/// The security-properties singleton READ. NOTE: the NON-singleton
/// spelling `/resources/ignition/security-properties` is the PUT target
/// but 404s as a GET on live 8.3.6 (live-observed 2026-09-18; the
/// doctor capability reads the wrong path — see ADOPT-RESEARCH).
pub(crate) const SECURITY_SINGLETON_PATH: &str =
    "/data/api/v1/resources/singleton/ignition/security-properties";

/// The security-properties singleton WRITE (array-of-one + signature).
pub(crate) const SECURITY_PUT_PATH: &str = "/data/api/v1/resources/ignition/security-properties";

/// The api-token resource type string (the create answer's
/// `changes[].type`; asserted against the live answer in the mint).
pub(crate) const API_TOKEN_TYPE: &str = "ignition/api-token";

/// The basic-token profile type string.
pub(crate) const BASIC_TOKEN_TYPE: &str = "basic-token";

/// `POST /data/api/v1/api-token/generate` — the mint. The `key` is the
/// plaintext token, returned here and nowhere else; `hash` is what the
/// resource record stores. Live-captured 8.3.6:
///
/// ```json
/// { "key":  "<43-char urlsafe plaintext — REDACTED, same shape>",
///   "hash": "<43-char urlsafe stored hash — REDACTED, same shape>" }
/// ```
///
/// Redaction discipline: this struct carries the plaintext between the
/// client parse and the action's ONE exposure site (the result model +
/// keyring write). It deliberately implements `Debug` with the fields
/// visible — it lives core-internal for one hop; the ACTION result is
/// the documented exposure boundary.
#[derive(Debug, Clone, Deserialize)]
pub struct GeneratedKeyWire {
    /// The plaintext token (43-char urlsafe) — the only place the
    /// gateway ever returns it.
    #[serde(default)]
    pub key: String,
    /// The stored hash — rides the resource record's
    /// `config.settings.tokenHash`.
    #[serde(default)]
    pub hash: String,
}

/// One `ignition/api-token` resource record — the typed subset adopt
/// reads (find-by-name + config inspection); unknown keys round-trip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiTokenRecord {
    /// The key's name — the find-by-name key AND half of the auth
    /// header value (`<name>:<key>`).
    #[serde(default)]
    pub name: String,
    /// Free-text description, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Disabled keys authenticate as nothing.
    #[serde(default)]
    pub enabled: bool,
    /// The token profile + hash.
    #[serde(default)]
    pub config: ApiTokenConfig,
    /// Unknown keys round-trip (version, signature, attributes, …).
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// The record's `config` — `profile` (level + flags) + `settings`
/// (the hash).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ApiTokenConfig {
    /// The token profile.
    #[serde(default)]
    pub profile: ApiTokenProfile,
    /// The hash store.
    #[serde(default)]
    pub settings: ApiTokenSettings,
}

/// The record's `config.profile` — everything doctor's three-part
/// diagnosis needs.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ApiTokenProfile {
    /// `"basic-token"` (the only extension point today).
    #[serde(rename = "type", default)]
    pub kind: String,
    /// The granted leaf-path trees (see [`level_tree`]).
    #[serde(rename = "securityLevels", default)]
    pub security_levels: Vec<Value>,
    /// The "Require secure connections" flag — MUST be false for
    /// http gateways or every request 403s (three-part cause 3).
    #[serde(rename = "secureChannelRequired", default)]
    pub secure_channel_required: bool,
    /// Creation time, epoch milliseconds.
    #[serde(default)]
    pub timestamp: i64,
}

/// The record's `config.settings` — the hash only, never the key.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ApiTokenSettings {
    /// The stored hash (`config.settings.tokenHash`).
    #[serde(rename = "tokenHash", default)]
    pub token_hash: String,
}

/// The shared resources-API mutation envelope (create + singleton PUT;
/// live-captured on the create — `{success, changes[], problem}`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceMutationWire {
    /// The gateway's verdict — `false` or a non-2xx both refuse.
    #[serde(default)]
    pub success: bool,
    /// What changed (name/type/collection/newSignature).
    #[serde(default)]
    pub changes: Vec<ResourceChangeWire>,
    /// Populated (non-null) when the gateway reports a partial
    /// problem — surfaced verbatim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<Value>,
}

/// One `changes[]` row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceChangeWire {
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub collection: String,
    #[serde(rename = "newSignature", default)]
    pub new_signature: String,
}

/// Render one granted level tree as its slash path
/// (`Authenticated/Roles/Administrator`) — the human form for report
/// rows. Walks the first-child chain (a granted tree is a single
/// path by construction). Pure function.
pub fn level_path_string(tree: &Value) -> String {
    let mut segments: Vec<&str> = Vec::new();
    let mut node = tree;
    while let Some(name) = node.get("name").and_then(Value::as_str) {
        segments.push(name);
        node = node
            .get("children")
            .and_then(Value::as_array)
            .and_then(|children| children.first())
            .unwrap_or(&Value::Null);
    }
    segments.join("/")
}

/// The security-properties singleton READ — typed shell (the merge
/// works at `Value` level inside `config`; the config's full shape is
/// passthrough by design, the doctor precedent).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SecuritySingletonWire {
    /// The optimistic-concurrency token — MUST ride the PUT verbatim
    /// or the write refuses (rotates on every write; live-observed).
    #[serde(default)]
    pub signature: String,
    /// `"core"` on every capture.
    #[serde(default = "default_collection")]
    pub collection: String,
    /// The whole config (`readPermissions`/`writePermissions` live
    /// HERE, nested — not at the record's top level).
    #[serde(default)]
    pub config: Value,
    /// Unknown keys round-trip.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

fn default_collection() -> String {
    "core".to_string()
}

/// Build the nested leaf-path tree for one granted level:
/// `["Authenticated","Roles","Administrator"]` →
/// `{name:"Authenticated", children:[{name:"Roles", children:[
/// {name:"Administrator", children:[]}]}]}` — the live-captured
/// encoding (the type descriptor's `defaultConfig` and every
/// permissions list agree). Pure function; the unit test pins the
/// captured shape verbatim.
pub fn level_tree(path: &[&str]) -> Value {
    match path.split_first() {
        // The LEAF: children is the empty array (not [[]] — the
        // recursion stops one level early by construction).
        Some((head, [])) => json!({ "name": head, "children": [] }),
        Some((head, tail)) => json!({
            "name": head,
            "children": [level_tree(tail)],
        }),
        // Degenerate defensive value — adopt always passes a path.
        None => json!([]),
    }
}

/// Build the api-token CREATE body (array-of-one) — pure function,
/// the unit test pins it against the live-captured request VERBATIM.
/// `timestamp_ms` is epoch milliseconds (std-only at call sites).
pub fn build_token_create_body(
    name: &str,
    level: &Value,
    token_hash: &str,
    timestamp_ms: i64,
) -> Value {
    json!([{
        "name": name,
        "collection": "core",
        "enabled": true,
        "description": "",
        "config": {
            "profile": {
                "securityLevels": [level],
                "secureChannelRequired": false,
                "type": BASIC_TOKEN_TYPE,
                "timestamp": timestamp_ms
            },
            "settings": { "tokenHash": token_hash }
        }
    }])
}

/// Build the security-properties PUT body (array-of-one, whole-config
/// replacement + the GET's signature) — pure function; the unit test
/// pins the captured shape (incl. `gatewayAuditProfile: null`, which
/// the UI sends and the GET omits).
pub fn build_security_put_body(singleton: &SecuritySingletonWire, config: &Value) -> Value {
    json!([{
        "collection": singleton.collection,
        "enabled": true,
        "description": "",
        "signature": singleton.signature,
        "config": config
    }])
}

/// Ensure a BARE-ROOT entry for the granted level's top segment
/// exists in an AnyOf permission list inside a singleton `config` —
/// add-if-missing, never remove, never mutate existing entries.
/// Returns true when the config changed. Missing permissions objects
/// are CREATED as `{"type":"AnyOf","securityLevels":[bare-root]}`.
///
/// WHY THE BARE ROOT and not the granted leaf-path tree
/// (live-pinned 2026-09-18, ADOPT-RESEARCH pitfall 3): a permission
/// entry serialized as the NESTED tree
/// (`Authenticated>Roles>Administrator`, the UI's own spelling, with
/// or without descriptions) does NOT admit an API token granted that
/// exact path — 403, live-observed both ways — while a BARE-root
/// entry (`{name:"Authenticated",children:[]}`) admits every
/// descendant-granted token (200, live-observed). The bare root is
/// the ANCESTOR form: strictly broader than the nested grant, so
/// adding it can only widen admission, never narrow.
///
/// The `[]` case (the PUBLIC encoding — pitfall 2) gets the bare
/// root ADDED, tightening Public down to root-admitted.
pub fn merge_level_into(config: &mut Value, field: &str, level: &Value) -> bool {
    let Some(incoming_root) = level.get("name").and_then(Value::as_str) else {
        return false;
    };
    let Some(config_object) = config.as_object_mut() else {
        return false;
    };
    let permissions = config_object
        .entry(field)
        .or_insert_with(|| json!({ "type": "AnyOf", "securityLevels": [] }));
    let Some(permissions_object) = permissions.as_object_mut() else {
        return false;
    };
    let levels = permissions_object
        .entry("securityLevels")
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(levels_array) = levels.as_array_mut() else {
        return false;
    };
    // Present = a BARE-root entry exists (children empty). A nested
    // same-root entry does NOT count — it does not admit the token
    // (pitfall 3), so the bare form must land beside it.
    let has_bare_root = levels_array.iter().any(|existing| {
        existing.get("name").and_then(Value::as_str) == Some(incoming_root)
            && existing
                .get("children")
                .and_then(Value::as_array)
                .is_some_and(|children| children.is_empty())
    });
    if has_bare_root {
        return false;
    }
    levels_array.push(json!({ "name": incoming_root, "children": [] }));
    true
}

/// `GET …/resources/list/ignition/api-token` on the session tier —
/// the pre-token idempotent find. One page at `limit=100`; a gateway
/// holding MORE tokens than that REFUSES rather than guessing (the
/// review round's truncation trap: a key beyond the page would read
/// as absent and the mint would collide — an honest error beats an
/// ambiguous duplicate).
pub async fn api_tokens_via_session(
    flow: &IdpLoginFlow,
    session: &GatewaySession,
) -> Result<Vec<ApiTokenRecord>, CoreError> {
    let value = flow
        .session_get_json(session, API_TOKEN_LIST_PATH, &[("limit", "100")])
        .await?;
    let total = value
        .get("metadata")
        .and_then(|metadata| metadata.get("total"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let items = value
        .get("items")
        .cloned()
        .ok_or_else(|| CoreError::Internal("api-token list carried no items array".into()))?;
    let count = items.as_array().map_or(0, Vec::len) as i64;
    if total > count {
        return Err(CoreError::Internal(format!(
            "this gateway holds {total} API keys — more than adopt's one-page \
             lookup ({count} returned); delete unused keys and re-run"
        )));
    }
    let records: Vec<ApiTokenRecord> = serde_json::from_value(items).map_err(|err| {
        CoreError::Internal(format!(
            "api-token list item did not match the record shape: {err}"
        ))
    })?;
    Ok(records)
}

/// `POST /data/api/v1/api-token/generate` on the session tier —
/// the mint (see [`GeneratedKeyWire`] for the plaintext discipline).
pub async fn generate_api_key_via_session(
    flow: &IdpLoginFlow,
    session: &GatewaySession,
) -> Result<GeneratedKeyWire, CoreError> {
    let value = flow
        .session_post_json(session, GENERATE_PATH, &json!({}))
        .await?;
    serde_json::from_value(value)
        .map_err(|err| CoreError::Internal(format!("api-token generate answer shape: {err}")))
}

/// `POST …/resources/ignition/api-token` on the session tier — the
/// create (array-of-one body from [`build_token_create_body`]).
pub async fn create_api_token_via_session(
    flow: &IdpLoginFlow,
    session: &GatewaySession,
    body: &Value,
) -> Result<ResourceMutationWire, CoreError> {
    let value = flow
        .session_post_json(session, API_TOKEN_CREATE_PATH, body)
        .await?;
    serde_json::from_value(value)
        .map_err(|err| CoreError::Internal(format!("api-token create answer shape: {err}")))
}

/// `GET …/resources/singleton/ignition/security-properties` on the
/// session tier — the pre-token read for the merge.
pub async fn security_properties_via_session(
    flow: &IdpLoginFlow,
    session: &GatewaySession,
) -> Result<SecuritySingletonWire, CoreError> {
    let value = flow
        .session_get_json(
            session,
            SECURITY_SINGLETON_PATH,
            &[("defaultIfUndefined", "true")],
        )
        .await?;
    serde_json::from_value(value)
        .map_err(|err| CoreError::Internal(format!("security-properties singleton shape: {err}")))
}

/// `PUT …/resources/ignition/security-properties` on the session
/// tier — the whole-config replacement (body from
/// [`build_security_put_body`]; the signature is the concurrency
/// gate).
pub async fn put_security_properties_via_session(
    flow: &IdpLoginFlow,
    session: &GatewaySession,
    body: &Value,
) -> Result<ResourceMutationWire, CoreError> {
    let value = flow
        .session_put_json(session, SECURITY_PUT_PATH, body)
        .await?;
    serde_json::from_value(value)
        .map_err(|err| CoreError::Internal(format!("security-properties put answer shape: {err}")))
}

#[cfg(test)]
mod tests {
    use serde_json::Value;
    use serde_json::json;

    use super::{
        ApiTokenRecord, GeneratedKeyWire, ResourceMutationWire, SecuritySingletonWire,
        build_security_put_body, build_token_create_body, level_tree, merge_level_into,
    };

    /// THE live-capture regression — the generate answer's SHAPE,
    /// from the 8.3.6 rig capture (ADOPT-RESEARCH §1a). The VALUES
    /// are redacted same-shape synthetics: real key material never
    /// lands in source, even disposable-rig keys (review round).
    #[test]
    fn generate_parses_the_live_capture() {
        let wire: GeneratedKeyWire = serde_json::from_value(json!({
            "key": "AAAAexample-redacted-key-43-chars-urlsafe-0",
            "hash": "BBBBexample_redacted_hash_43_chars_urlsafe0"
        }))
        .expect("the live generate shape must parse");
        assert_eq!(wire.key.len(), 43, "urlsafe plaintext, 43 chars");
        assert_eq!(wire.hash.len(), 43, "stored hash, 43 chars");
    }

    /// The live-captured list item (trimmed to the typed fields +
    /// one passthrough key) must parse with the level + secure flag
    /// + hash visible (ADOPT-RESEARCH §1c).
    #[test]
    fn list_item_parses_the_live_capture() {
        let record: ApiTokenRecord = serde_json::from_value(json!({
            "type": "ignition/api-token",
            "name": "ign-adopt-capture",
            "description": "",
            "enabled": true,
            "version": 1,
            "collection": "core",
            "signature": "87717b87",
            "config": {
                "profile": {
                    "type": "basic-token",
                    "secureChannelRequired": false,
                    "securityLevels": [
                        { "name": "Authenticated", "children": [] }
                    ],
                    "timestamp": 1789760514446i64
                },
                "settings": { "tokenHash": "BBBBexample_redacted_hash_43_chars_urlsafe0" }
            },
            "attributes": { "uuid": "…", "enabled": true }
        }))
        .expect("the live list item shape must parse");
        assert_eq!(record.name, "ign-adopt-capture");
        assert!(record.enabled);
        assert!(!record.config.profile.secure_channel_required);
        assert_eq!(record.config.profile.kind, "basic-token");
        assert_eq!(
            record.config.settings.token_hash,
            "BBBBexample_redacted_hash_43_chars_urlsafe0"
        );
        assert!(record.extra.contains_key("signature"), "passthrough rides");
    }

    /// The create body builder must reproduce the live-captured
    /// request VERBATIM (modulo the varying timestamp) —
    /// ADOPT-RESEARCH §1b.
    #[test]
    fn create_body_matches_the_live_capture() {
        let level = level_tree(&["Authenticated"]);
        let body = build_token_create_body(
            "ign-adopt-capture",
            &level,
            "BBBBexample_redacted_hash_43_chars_urlsafe0",
            1789760514446,
        );
        let captured: Value = json!([{
            "name": "ign-adopt-capture",
            "collection": "core",
            "enabled": true,
            "description": "",
            "config": {
                "profile": {
                    "securityLevels": [
                        { "name": "Authenticated",
                          "description": "Represents a user who has been authenticated by the system.",
                          "children": [] }
                    ],
                    "secureChannelRequired": false,
                    "type": "basic-token",
                    "timestamp": 1789760514446i64
                },
                "settings": { "tokenHash": "BBBBexample_redacted_hash_43_chars_urlsafe0" }
            }
        }]);
        // descriptions are optional-in (the gateway echoes them out);
        // the builder omits them by design — compare descriptionless.
        let mut expected = captured;
        for record in expected.as_array_mut().expect("array") {
            record["config"]["profile"]["securityLevels"][0]
                .as_object_mut()
                .expect("level object")
                .remove("description");
        }
        assert_eq!(body, expected, "byte-faithful modulo descriptions");
    }

    /// The create answer — `{success, changes[], problem}` — must
    /// parse with the new signature visible (ADOPT-RESEARCH §1b).
    #[test]
    fn mutation_parses_the_live_create_answer() {
        let wire: ResourceMutationWire = serde_json::from_value(json!({
            "success": true,
            "changes": [{
                "name": "ign-adopt-capture",
                "type": "ignition/api-token",
                "collection": "core",
                "newSignature": "87717b874ec57a83e676c7e757e5264efc20e4cdb571525470ef6c93d5736f45"
            }],
            "problem": null
        }))
        .expect("the live create answer must parse");
        assert!(wire.success);
        assert_eq!(wire.changes.len(), 1);
        assert_eq!(wire.changes[0].kind, "ignition/api-token");
        assert!(wire.problem.is_none());
    }

    /// The singleton GET shape — signature + config-nested
    /// permissions + attributes passthrough (ADOPT-RESEARCH §2).
    #[test]
    fn singleton_parses_the_live_capture() {
        let wire: SecuritySingletonWire = serde_json::from_value(json!({
            "type": "ignition/security-properties",
            "signature": "dee8c94600032841",
            "collection": "core",
            "enabled": true,
            "config": {
                "readPermissions": {
                    "type": "AnyOf",
                    "securityLevels": [ { "name": "Authenticated", "children": [] } ]
                },
                "writePermissions": {
                    "type": "AnyOf",
                    "securityLevels": [ { "name": "Authenticated", "children": [] } ]
                }
            },
            "attributes": { "lastModification": { "actor": "admin" } }
        }))
        .expect("the live singleton shape must parse");
        assert_eq!(wire.signature, "dee8c94600032841");
        assert!(wire.config.get("writePermissions").is_some());
        assert!(wire.extra.contains_key("attributes"));
    }

    /// The Administrator leaf-path tree — the defaultConfig encoding
    /// (ADOPT-RESEARCH §1b/§2).
    #[test]
    fn level_tree_nests_the_administrator_path() {
        assert_eq!(
            level_tree(&["Authenticated", "Roles", "Administrator"]),
            json!({
                "name": "Authenticated",
                "children": [{
                    "name": "Roles",
                    "children": [{
                        "name": "Administrator",
                        "children": []
                    }]
                }]
            })
        );
        assert_eq!(
            level_tree(&["Authenticated"]),
            json!({ "name": "Authenticated", "children": [] })
        );
    }

    /// The slash-path render — report rows print the full leaf path.
    #[test]
    fn level_path_string_walks_the_chain() {
        use super::level_path_string;
        assert_eq!(
            level_path_string(&level_tree(&["Authenticated", "Roles", "Administrator"])),
            "Authenticated/Roles/Administrator"
        );
        assert_eq!(
            level_path_string(&level_tree(&["Authenticated"])),
            "Authenticated"
        );
    }

    /// merge: ensures a BARE-ROOT entry for the granted level's top
    /// segment — add-if-missing, idempotent; a NESTED same-root entry
    /// does NOT count (pitfall 3: nested entries do not admit
    /// equal-path tokens — the bare root lands BESIDE it, the
    /// fresh-gateway-default fix); `[]` (the PUBLIC encoding —
    /// pitfall 2) gets the root ADDED; a missing field is created.
    #[test]
    fn merge_adds_missing_and_is_idempotent() {
        let authenticated = level_tree(&["Authenticated"]);
        let administrator = level_tree(&["Authenticated", "Roles", "Administrator"]);
        let bare = |root: &str| json!({ "name": root, "children": [] });

        // Different root → the bare root added; re-run changes nothing.
        let mut config = json!({
            "writePermissions": {
                "type": "AnyOf",
                "securityLevels": [ bare("SecurityZones") ]
            }
        });
        assert!(merge_level_into(
            &mut config,
            "writePermissions",
            &administrator
        ));
        assert!(
            !merge_level_into(&mut config, "writePermissions", &administrator),
            "idempotent re-run changes nothing"
        );
        assert_eq!(
            config["writePermissions"]["securityLevels"][1],
            bare("Authenticated"),
            "the BARE root lands (pitfall 3), not the nested tree"
        );

        // Bare-root entry already present → skip.
        let mut config = json!({
            "writePermissions": { "type": "AnyOf", "securityLevels": [authenticated.clone()] }
        });
        assert!(!merge_level_into(
            &mut config,
            "writePermissions",
            &administrator
        ));

        // NESTED same-root entry (the fresh-gateway default) does not
        // admit the token — the bare root lands BESIDE it.
        let mut config = json!({
            "writePermissions": { "type": "AnyOf", "securityLevels": [administrator.clone()] }
        });
        assert!(merge_level_into(
            &mut config,
            "writePermissions",
            &administrator
        ));
        let levels = config["writePermissions"]["securityLevels"]
            .as_array()
            .expect("levels");
        assert_eq!(levels.len(), 2, "bare root added beside the nested entry");
        assert_eq!(levels[1], bare("Authenticated"));

        // Empty list (Public) → the bare root lands, tightening it.
        let mut config = json!({
            "writePermissions": { "type": "AnyOf", "securityLevels": [] }
        });
        assert!(merge_level_into(
            &mut config,
            "writePermissions",
            &authenticated
        ));
        let levels = config["writePermissions"]["securityLevels"]
            .as_array()
            .expect("levels");
        assert_eq!(levels.len(), 1);
        assert_eq!(levels[0], bare("Authenticated"));

        // Missing field entirely → created as AnyOf.
        let mut config = json!({});
        assert!(merge_level_into(
            &mut config,
            "readPermissions",
            &authenticated
        ));
        assert_eq!(config["readPermissions"]["type"], "AnyOf");
    }

    /// The PUT body — array-of-one, whole config, the GET's
    /// signature riding verbatim (ADOPT-RESEARCH §2).
    #[test]
    fn put_body_carries_the_signature() {
        let singleton: SecuritySingletonWire = serde_json::from_value(json!({
            "signature": "dee8c94600032841",
            "collection": "core",
            "config": { "forceIdpAuth": true }
        }))
        .expect("parses");
        let mut config = singleton.config.clone();
        merge_level_into(
            &mut config,
            "writePermissions",
            &level_tree(&["Authenticated"]),
        );
        let body = build_security_put_body(&singleton, &config);
        assert_eq!(body[0]["signature"], "dee8c94600032841");
        assert_eq!(body[0]["collection"], "core");
        assert_eq!(body[0]["config"]["forceIdpAuth"], true);
        assert!(
            body[0]["config"]["writePermissions"]["securityLevels"]
                .as_array()
                .expect("levels")
                .len()
                == 1
        );
    }
}
