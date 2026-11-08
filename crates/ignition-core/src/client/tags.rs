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
//! 05-06's alarms active/history/ack (TAGS-07) and tag-history
//! query (TAGS-08) ride the deployed `alarms` + `tagHistory` routes
//! the same way — free-form payloads at the action layer — with ONE
//! client-seam addition: the alarms route's structured
//! `no_alarm_journal` denial maps to the actionable
//! [`CoreError::AlarmJournalMissing`] in
//! [`super::webdev::denial_to_error`] (the single body-denial
//! mapping site).
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

/// Per-request ceiling for the Phase-11 bulk `exportTags` transfer —
/// the 09-05 [`BUNDLE_DOWNLOAD_TIMEOUT`] pattern (research Pitfall 6:
/// the 30 s client default would truncate large exports). The bulk
/// `importTagsFile` call shares it (the same one-request envelope,
/// opposite direction). Pinned by unit test at birth.
pub const TAGS_EXPORT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// GET path — the native tag export of one provider
/// (`?provider=<P>&type=json`), the read `ign deploy projects` uses to
/// learn which tags a gateway has. Needs no deployed WebDev route.
pub(crate) const TAGS_EXPORT_PATH: &str = "/data/api/v1/tags/export";

/// POST path — the native tag import of one body into one provider
/// (`?provider=<P>&type=json&collisionPolicy=<policy>[&path=<parent>]`),
/// the only tag write `ign deploy projects` makes (D-08: the gateway writes
/// its own files). Needs no deployed WebDev route.
pub(crate) const TAGS_IMPORT_PATH: &str = "/data/api/v1/tags/import";

/// Per-request ceiling for one `tags/import` call: the export ceiling
/// (research Pitfall 11, a provider-size body). Pinned by unit test.
pub const TAGS_IMPORT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// How `tags/import` treats a node whose name already exists at the import
/// level. The wire spellings are the OpenAPI document's. `Rename` is
/// deliberately not offered: it creates a second node under another name,
/// which contradicts repo authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionPolicy {
    /// Refuse the colliding node (create only).
    Abort,
    /// Replace the existing node's configuration.
    Overwrite,
    /// Change only the keys the body defines.
    MergeOverwrite,
    /// Skip the colliding node.
    Ignore,
}

impl CollisionPolicy {
    /// The `collisionPolicy` query value, exactly as the gateway spells it.
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Abort => "Abort",
            Self::Overwrite => "Overwrite",
            Self::MergeOverwrite => "MergeOverwrite",
            Self::Ignore => "Ignore",
        }
    }
}

/// What a `tags/import` answer says, read fail-closed (research Pitfall 5):
/// a partial import is never read as success.
///
/// Two shapes are accepted. The OpenAPI document describes a JSON array,
/// "List of QualityCodes from import that aren't Good"; the live 8.3.6
/// gateway answers an object, `{"successCount":0,"failureCount":1,
/// "failures":[{"quality":"Bad","qualitySubCode":527,"diagnosticMessage":..}]}`
/// (26-LIVE-CAPTURES.md C1, a name collision under `Abort`). An empty body is
/// zero results. Anything else is an error naming the shape. The fields
/// carry counts and quality names only, never tag values or diagnostic
/// messages.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TagImportAnswer {
    /// `successCount`, when the answer carries it.
    pub success_count: Option<u64>,
    /// `failureCount`, when the answer carries it.
    pub failure_count: Option<u64>,
    /// Every quality the answer reports that is not `Good`, rendered short
    /// (a quality name or code, never a tag value).
    pub bad_quality: Vec<String>,
}

impl TagImportAnswer {
    /// Read a `tags/import` answer body. An unknown shape is
    /// `CoreError::Internal` naming the shape.
    pub fn parse(bytes: &[u8]) -> Result<Self, crate::error::CoreError> {
        if bytes.iter().all(u8::is_ascii_whitespace) {
            return Ok(Self::default());
        }
        let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|err| {
            crate::error::CoreError::Internal(format!(
                "the tags/import answer is not JSON ({} bytes): {err}",
                bytes.len()
            ))
        })?;
        match value {
            serde_json::Value::Array(entries) => Ok(Self {
                success_count: None,
                failure_count: None,
                bad_quality: not_good(&entries),
            }),
            serde_json::Value::Object(map) => {
                let count = |key: &str| -> Result<Option<u64>, crate::error::CoreError> {
                    match map.get(key) {
                        None | Some(serde_json::Value::Null) => Ok(None),
                        Some(serde_json::Value::Number(n)) if n.as_u64().is_some() => {
                            Ok(n.as_u64())
                        }
                        Some(other) => Err(crate::error::CoreError::Internal(format!(
                            "the tags/import answer's {key} is a {}, not a count",
                            json_kind(other)
                        ))),
                    }
                };
                let success_count = count("successCount")?;
                let failure_count = count("failureCount")?;
                if success_count.is_none() && failure_count.is_none() {
                    let keys: Vec<&str> = map.keys().map(String::as_str).collect();
                    return Err(crate::error::CoreError::Internal(format!(
                        "the tags/import answer is an object without successCount or \
                         failureCount (keys: {})",
                        keys.join(", ")
                    )));
                }
                let mut bad_quality = Vec::new();
                for entries in map.values().filter_map(serde_json::Value::as_array) {
                    bad_quality.extend(not_good(entries));
                }
                Ok(Self {
                    success_count,
                    failure_count,
                    bad_quality,
                })
            }
            other => Err(crate::error::CoreError::Internal(format!(
                "the tags/import answer is a JSON {}, not an array or an object",
                json_kind(&other)
            ))),
        }
    }

    /// True only when no failure is counted and no quality is other than
    /// `Good`.
    pub fn is_clean(&self) -> bool {
        self.failure_count.unwrap_or(0) == 0 && self.bad_quality.is_empty()
    }
}

/// The JSON type name of `value`, for shape errors.
fn json_kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// The short quality text of one answer entry: a scalar's own text, an
/// object's `quality` (or that object's `name`/`code`) with its
/// `qualitySubCode` in brackets when present (`Bad (527)`), else its `name`,
/// else its `code`, else the entry's JSON type. Never a `value` and never a
/// `diagnosticMessage`.
fn quality_text(entry: &serde_json::Value) -> String {
    fn scalar(value: &serde_json::Value) -> Option<String> {
        match value {
            serde_json::Value::String(s) => Some(s.trim().to_string()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            serde_json::Value::Bool(b) => Some(b.to_string()),
            _ => None,
        }
    }
    if let Some(text) = scalar(entry) {
        return text;
    }
    if let serde_json::Value::Object(map) = entry {
        if let Some(quality) = map.get("quality") {
            let name = scalar(quality).or_else(|| {
                quality.as_object().and_then(|q| {
                    q.get("name")
                        .and_then(scalar)
                        .or_else(|| q.get("code").and_then(scalar))
                })
            });
            if let Some(name) = name {
                return match map.get("qualitySubCode").and_then(scalar) {
                    Some(sub) => format!("{name} ({sub})"),
                    None => name,
                };
            }
        }
        for key in ["name", "code"] {
            if let Some(text) = map.get(key).and_then(scalar) {
                return text;
            }
        }
    }
    format!("<{}>", json_kind(entry))
}

/// Every entry of `entries` whose quality is not `Good`, rendered short.
fn not_good(entries: &[serde_json::Value]) -> Vec<String> {
    entries
        .iter()
        .map(quality_text)
        .filter(|text| text != "Good")
        .collect()
}

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
    use std::time::Duration;

    use super::{
        BrowseEntry, CollisionPolicy, TAGS_EXPORT_TIMEOUT, TAGS_IMPORT_TIMEOUT, TagImportAnswer,
        TagProviderCreate, TagProviderRecord,
    };

    /// Birth pin (the 09-05 BUNDLE_DOWNLOAD_TIMEOUT pattern): the
    /// bulk-transfer override is exactly 300 s — the constant IS the
    /// contract, never a silent drift.
    #[test]
    fn tags_export_timeout_is_pinned_at_300s() {
        assert_eq!(TAGS_EXPORT_TIMEOUT, Duration::from_secs(300));
    }

    /// The import ceiling is the export ceiling (Pitfall 11).
    #[test]
    fn tags_import_timeout_is_pinned_at_300s() {
        assert_eq!(TAGS_IMPORT_TIMEOUT, Duration::from_secs(300));
    }

    /// The wire spellings are the OpenAPI document's, exactly.
    #[test]
    fn collision_policy_wire_spellings() {
        assert_eq!(CollisionPolicy::Abort.as_wire(), "Abort");
        assert_eq!(CollisionPolicy::Overwrite.as_wire(), "Overwrite");
        assert_eq!(CollisionPolicy::MergeOverwrite.as_wire(), "MergeOverwrite");
        assert_eq!(CollisionPolicy::Ignore.as_wire(), "Ignore");
    }

    /// The live object shape, the documented array shape and an empty body
    /// parse; anything else is an internal error.
    #[test]
    fn import_answer_shapes() {
        let live = TagImportAnswer::parse(br#"{"successCount":2,"failureCount":0}"#).unwrap();
        assert_eq!(live.success_count, Some(2));
        assert_eq!(live.failure_count, Some(0));
        assert!(live.is_clean());

        let failed = TagImportAnswer::parse(br#"{"successCount":0,"failureCount":1}"#).unwrap();
        assert!(!failed.is_clean());

        let array =
            TagImportAnswer::parse(br#"["Good",{"name":"Bad_NotFound"},{"code":12}]"#).unwrap();
        assert_eq!(array.bad_quality, vec!["Bad_NotFound", "12"]);
        assert!(!array.is_clean());

        let all_good = TagImportAnswer::parse(br#"["Good",{"quality":"Good"}]"#).unwrap();
        assert!(all_good.is_clean());

        let empty = TagImportAnswer::parse(b"  ").unwrap();
        assert_eq!(empty, TagImportAnswer::default());
        assert!(empty.is_clean());

        for bad in [
            &br#""ok""#[..],
            br#"{"ok":true}"#,
            b"not json",
            br#"{"successCount":"2"}"#,
        ] {
            let err = TagImportAnswer::parse(bad).unwrap_err();
            assert_eq!(err.code(), "internal", "{}", String::from_utf8_lossy(bad));
        }
    }

    /// The live 8.3.6 failure answer (26-LIVE-CAPTURES.md C1, a collision
    /// under Abort) is not clean, and its diagnostic message is not kept.
    #[test]
    fn import_answer_reads_the_live_failure_answer() {
        let answer = TagImportAnswer::parse(
            br#"{"failureCount":1,"failures":[{"diagnosticMessage":"Tag '[P]F/T1' already exists","quality":"Bad","qualitySubCode":527}],"successCount":0}"#,
        )
        .unwrap();
        assert_eq!(answer.failure_count, Some(1));
        assert_eq!(answer.bad_quality, vec!["Bad (527)"]);
        assert!(!answer.is_clean());
        assert!(!format!("{answer:?}").contains("already exists"));
        let ok = TagImportAnswer::parse(br#"{"failureCount":0,"failures":[],"successCount":2}"#)
            .unwrap();
        assert!(ok.is_clean());
    }

    /// A per-tag result array inside the object shape is read for non-Good
    /// qualities, and a value never reaches the rendered text.
    #[test]
    fn import_answer_reads_nested_results_without_values() {
        let answer = TagImportAnswer::parse(
            br#"{"successCount":1,"failureCount":1,"results":[{"quality":"Good","value":"s3cret"},{"quality":{"name":"Bad_Failure"},"value":"s3cret"}]}"#,
        )
        .unwrap();
        assert_eq!(answer.bad_quality, vec!["Bad_Failure"]);
        assert!(!format!("{answer:?}").contains("s3cret"));
    }

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
