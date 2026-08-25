//! Tag capability models (05-04, TAGS-01..04) — TWO seams, one file
//! per the family's wire split (05-RESEARCH):
//!
//! 1. **NATIVE config-resource REST** for provider CRUD
//!    (TAGS-01): the `ignition/tag-provider` resource family —
//!    live-proven including create (the ARRAY body) and
//!    delete-by-signature. Providers are the native seam: healthier
//!    data (`metrics.tagCount`, `healthchecks.status`) with no
//!    deployed route needed.
//! 2. **The deployed `tags` WebDev route** for browse/read/write
//!    (TAGS-02/03/04): those calls ride the 05-03 generic
//!    [`super::GatewayApi::webdev_route_call`] (no new trait
//!    methods); THIS file owns only the wire model
//!    ([`BrowseEntry`]) the route's `browse` results deserialize
//!    into.
//!
//! 05-05's config CRUD / UDTs / export+import ride the deployed
//! `tagConfig` route through the SAME generic seam — no new client
//! models (its `getConfig`/`getUDTDefinition` payloads are
//! free-form dicts the actions layer post-processes; `exportTags`
//! is a JSON string parsed at the action layer), so this file's
//! models are unchanged.
//!
//! Two-layer naming (the LOCKED convention): client models stay
//! wire-faithful (the browse route answers gateway-native camelCase
//! `fullPath`/`tagType`/`hasChildren`/`dataType` — renames here,
//! unit-explicit keys at the ACTIONS layer); provider records are
//! raw-passthrough shapes like [`super::connections::GatewayConnection`]
//! (extra keys round-trip through `#[serde(flatten)]`).
//!
//! Path discipline: every `{name}`/`{signature}` path segment rides
//! the ONE locked per-segment encoder
//! ([`super::projects::encode_segment`]) — over-encoding is safe,
//! the server decodes before matching.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::client::projects::encode_segment;

/// GET path — the tag-provider resource list (web-UI poll family).
pub(crate) const TAG_PROVIDERS_LIST_PATH: &str =
    "/data/api/v1/resources/list/ignition/tag-provider";

/// POST path — create tag providers (the body is a JSON ARRAY of
/// create records; live-proven shape).
pub(crate) const TAG_PROVIDERS_CREATE_PATH: &str = "/data/api/v1/resources/ignition/tag-provider";

/// GET path — one provider's full record (`/find/{enc}`), incl. the
/// `signature` the chained DELETE needs.
pub(crate) fn tag_provider_find_path(name: &str) -> String {
    format!(
        "/data/api/v1/resources/find/ignition/tag-provider/{}",
        encode_segment(name)
    )
}

/// DELETE path — delete-by-signature
/// (`/{name}/{signature}`; the signature comes from find —
/// live-proven chain).
pub(crate) fn tag_provider_delete_path(name: &str, signature: &str) -> String {
    format!(
        "/data/api/v1/resources/ignition/tag-provider/{}/{}",
        encode_segment(name),
        encode_segment(signature)
    )
}

/// One item of the tag-provider resource lists — wire-faithful
/// passthrough: `metrics.tagCount` and `healthchecks.status` ride as
/// raw [`serde_json::Value`]s the actions layer pointers into (the
/// GatewayConnection pattern; renderers never see this model).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TagProviderRecord {
    /// Resource name, e.g. `"default"`.
    #[serde(default)]
    pub name: String,
    /// Whether the provider resource is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Provider config (`config.profile.type` distinguishes
    /// STANDARD/DB-backed/MANAGED) — raw passthrough.
    #[serde(default)]
    pub config: serde_json::Value,
    /// `metrics.tagCount` — raw passthrough (the healthy native seam).
    #[serde(default)]
    pub metrics: serde_json::Value,
    /// `healthchecks.status` — raw passthrough.
    #[serde(default)]
    pub healthchecks: serde_json::Value,
    /// The record's mutation signature (find records carry it; the
    /// chained DELETE path embeds it).
    #[serde(default)]
    pub signature: Option<String>,
    /// `collection`, `type`, … resource keys round-trip.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// One element of the create ARRAY body — field order is
/// declaration order, so the recorded-request pin is deterministic
/// (the serde discipline note in [`super::projects`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TagProviderCreate {
    /// Provider name to create.
    pub name: String,
    /// The resource type token — always `"ignition/tag-provider"`.
    #[serde(rename = "type")]
    pub resource_type: String,
    /// The config-module collection — always `"core"`.
    pub collection: String,
    /// Whether the new provider starts enabled.
    pub enabled: bool,
    /// Provider config — MVP ships the fixed STANDARD shape
    /// (`{profile: {type: "STANDARD"}, settings: {}}`); DB-backed
    /// providers are out of scope (README documents).
    pub config: serde_json::Value,
}

impl TagProviderCreate {
    /// The MVP create body: a STANDARD provider, enabled, empty
    /// settings — the live-proven fixed shape (05-RESEARCH provider
    /// table verbatim).
    pub fn standard(name: &str) -> Self {
        Self {
            name: name.to_string(),
            resource_type: "ignition/tag-provider".to_string(),
            collection: "core".to_string(),
            enabled: true,
            config: serde_json::json!({"profile": {"type": "STANDARD"}, "settings": {}}),
        }
    }
}

/// One `browse` result entry from the deployed `tags` route —
/// wire-faithful camelCase renames (the route passes the
/// `system.tag.browse` dicts through with `tagType` as the
/// discriminator: Provider/Folder/AtomicTag/UdtType/UdtInstance/
/// Property). Property children ARE included in the payload;
/// filtering is the action layer's display decision.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BrowseEntry {
    /// `fullPath` — bracket-qualified (`[default]`, `[default]P5/T1`).
    #[serde(rename = "fullPath", default)]
    pub full_path: String,
    /// Leaf name.
    #[serde(default)]
    pub name: String,
    /// `tagType` — the discriminator (wire token verbatim).
    #[serde(rename = "tagType", default)]
    pub tag_type: String,
    /// `hasChildren`.
    #[serde(rename = "hasChildren", default)]
    pub has_children: bool,
    /// `dataType` for entries that carry one (AtomicTag/Property),
    /// else null.
    #[serde(rename = "dataType", default)]
    pub data_type: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{BrowseEntry, TagProviderCreate, TagProviderRecord};

    /// A plausible provider list item parses with tagCount/health
    /// passthrough and the signature absent (list records) vs
    /// present (find records).
    #[test]
    fn provider_record_parses_with_passthrough_metrics() {
        let listed: TagProviderRecord = serde_json::from_value(serde_json::json!({
            "name": "default",
            "enabled": true,
            "config": {"profile": {"type": "STANDARD"}},
            "metrics": {"tagCount": 12},
            "healthchecks": {"status": "OK"},
            "collection": "core"
        }))
        .expect("plausible list item must parse");
        assert_eq!(listed.name, "default");
        assert_eq!(listed.metrics["tagCount"], 12);
        assert_eq!(listed.healthchecks["status"], "OK");
        assert_eq!(listed.signature, None, "list records carry no signature");
        assert_eq!(
            listed.extra.get("collection"),
            Some(&serde_json::json!("core")),
            "resource keys round-trip"
        );

        let found: TagProviderRecord = serde_json::from_value(serde_json::json!({
            "name": "System",
            "enabled": true,
            "signature": "abc123"
        }))
        .expect("find record parses");
        assert_eq!(found.signature.as_deref(), Some("abc123"));
    }

    /// The MVP create body serializes to the live-proven array
    /// ELEMENT shape, field order included (the wire pin's oracle).
    #[test]
    fn create_body_is_the_live_proven_shape() {
        let body = TagProviderCreate::standard("p5e2e");
        let json = serde_json::to_string(&body).expect("serializes");
        assert_eq!(
            json,
            concat!(
                r#"{"name":"p5e2e","type":"ignition/tag-provider","collection":"core","#,
                r#""enabled":true,"config":{"profile":{"type":"STANDARD"},"settings":{}}}"#
            )
        );
    }

    /// Browse entries parse under the gateway-native camelCase keys
    /// and tolerate a missing dataType.
    #[test]
    fn browse_entry_parses_camel_case_wire_keys() {
        let tag: BrowseEntry = serde_json::from_value(serde_json::json!({
            "fullPath": "[default]P5/T1",
            "name": "T1",
            "tagType": "AtomicTag",
            "hasChildren": false,
            "dataType": "Int4"
        }))
        .expect("atomic entry parses");
        assert_eq!(tag.full_path, "[default]P5/T1");
        assert_eq!(tag.tag_type, "AtomicTag");
        assert_eq!(tag.data_type.as_deref(), Some("Int4"));

        let property: BrowseEntry = serde_json::from_value(serde_json::json!({
            "fullPath": "[default]P5/T1.valueSource",
            "name": "valueSource",
            "tagType": "Property",
            "hasChildren": false
        }))
        .expect("property entry parses without dataType");
        assert_eq!(property.tag_type, "Property");
        assert_eq!(property.data_type, None);
    }
}
