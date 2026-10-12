//! Config resources by module, type and name: path builders, segment
//! validation and the lenient wire models (20-04, DAPI-01 / DAPI-02).
//!
//! Every shape here rests on the live 8.3.6 captures in
//! `20-LIVE-CAPTURES.md`: the `C-list` blocks (find with and without a
//! collection, list, names, singleton), the `C3a` blocks (create, update,
//! the signature refusals) and Decision D7 (what a list can and cannot see).
//!
//! `config` is a [`serde_json::Value`], so the key order of a resource's
//! config is not preserved: the workspace keeps `serde_json`'s order
//! preservation off as a frozen-contract rule. Callers compare configs
//! semantically, never as text.
//!
//! Path safety (T-20-16): the module and type segments are validated
//! against `[A-Za-z0-9._-]` and never encoded, so a module id such as
//! `com.inductiveautomation.eam` keeps its dots. The name segment is free
//! text, so it is percent-encoded from its UTF-8 bytes through
//! [`crate::client::projects::encode_segment`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::client::projects::encode_segment;
use crate::client::query::ListQuery;
use crate::error::{CoreError, SignatureReason};

/// Base of every resource route.
const RESOURCES_BASE: &str = "/data/api/v1/resources";

/// Longest caller value quoted back in an `invalid_input` reason.
const QUOTE_CAP: usize = 64;

/// The filter operators the live OpenAPI allows (Decision D7: the
/// `propertyNames` pattern of the `filter` parameter).
const FILTER_OPS: [&str; 10] = [
    "eq", "ne", "cn", "sw", "ew", "gt", "gte", "lt", "lte", "rgx",
];

fn quoted(value: &str) -> String {
    let head: String = value.chars().take(QUOTE_CAP).collect();
    if head.len() < value.len() {
        format!("{head:?}...")
    } else {
        format!("{head:?}")
    }
}

/// Check a module or type path segment: non-empty ASCII `[A-Za-z0-9._-]`,
/// and not `.` or `..`. Refused before any request is built.
pub fn validate_segment(label: &str, value: &str) -> Result<(), CoreError> {
    let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-');
    if value.is_empty() {
        return Err(CoreError::InvalidInput {
            reason: format!("resource {label} must not be empty"),
        });
    }
    if value == "." || value == ".." || !value.chars().all(allowed) {
        return Err(CoreError::InvalidInput {
            reason: format!(
                "resource {label} {} is not allowed: use only letters, digits, '.', '_' and '-' \
                 (and not '.' or '..' alone)",
                quoted(value)
            ),
        });
    }
    Ok(())
}

fn validated(module: &str, ty: &str) -> Result<(), CoreError> {
    validate_segment("module", module)?;
    validate_segment("type", ty)
}

/// `GET .../resources/list/{module}/{type}` (capture C-list "GET list").
pub fn resource_list_path(module: &str, ty: &str) -> Result<String, CoreError> {
    validated(module, ty)?;
    Ok(format!("{RESOURCES_BASE}/list/{module}/{ty}"))
}

/// `GET .../resources/names/{module}/{type}` (capture C-list "GET names").
pub fn resource_names_path(module: &str, ty: &str) -> Result<String, CoreError> {
    validated(module, ty)?;
    Ok(format!("{RESOURCES_BASE}/names/{module}/{ty}"))
}

/// `GET .../resources/find/{module}/{type}/{name}` (captures C3a "GET find"
/// and C-list). The name is percent-encoded from its UTF-8 bytes. An empty
/// name, or one that is `.` or `..`, is refused: an encoded dot segment is
/// ambiguous to the servlet container, so it is never sent.
pub fn resource_find_path(module: &str, ty: &str, name: &str) -> Result<String, CoreError> {
    validated(module, ty)?;
    if name.is_empty() {
        return Err(CoreError::InvalidInput {
            reason: "resource name must not be empty".to_string(),
        });
    }
    if name == "." || name == ".." {
        return Err(CoreError::InvalidInput {
            reason: format!("resource name {} is not allowed", quoted(name)),
        });
    }
    Ok(format!(
        "{RESOURCES_BASE}/find/{module}/{ty}/{}",
        encode_segment(name)
    ))
}

/// `GET .../resources/singleton/{module}/{type}` (capture C-list
/// "GET singleton").
pub fn resource_singleton_path(module: &str, ty: &str) -> Result<String, CoreError> {
    validated(module, ty)?;
    Ok(format!("{RESOURCES_BASE}/singleton/{module}/{ty}"))
}

/// `POST` (create) and `PUT` (update) `.../resources/{module}/{type}`
/// (captures C3a).
pub fn resource_write_path(module: &str, ty: &str) -> Result<String, CoreError> {
    validated(module, ty)?;
    Ok(format!("{RESOURCES_BASE}/{module}/{ty}"))
}

/// The query pairs of a resource list call.
///
/// `limit` and `offset` always go out. Per Decision D7 the gateway reads a
/// filter as its own query parameter `field[op]=value`, NOT as
/// `filter=field[op]=value` (that form matches nothing, capture C-list
/// "list with filter"). So a `ListQuery.filter` of `name[eq]=X` is sent as
/// the pair `name[eq]` = `X`, and one that is not of that form is refused
/// with `invalid_input` rather than sent to a route that would silently
/// answer an empty list. The list sees the EFFECTIVE view only (D7).
pub fn list_query_pairs(q: &ListQuery) -> Result<Vec<(String, String)>, CoreError> {
    let mut pairs: Vec<(String, String)> = q
        .to_query_pairs()
        .into_iter()
        .filter(|(key, _)| key != "filter")
        .collect();
    if let Some(filter) = &q.filter {
        let bad = || CoreError::InvalidInput {
            reason: format!(
                "resource list filter {} must look like field[op]=value with op one of {}",
                quoted(filter),
                FILTER_OPS.join(", ")
            ),
        };
        let (key, value) = filter.split_once('=').ok_or_else(bad)?;
        let (field, op) = key
            .strip_suffix(']')
            .and_then(|k| k.split_once('['))
            .ok_or_else(bad)?;
        let field_ok =
            !field.is_empty() && field.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !field_ok || !FILTER_OPS.contains(&op) {
            return Err(bad());
        }
        pairs.push((key.to_string(), value.to_string()));
    }
    Ok(pairs)
}

/// One config resource as the gateway reports it (captures C3a "GET find",
/// C-list "GET find" / "GET list" / "GET singleton"). Lenient: a key the
/// captures do not show stays in `extra` (live bodies also carry `data`,
/// `metrics` and `healthchecks`). A singleton has no `name`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceRecord {
    /// `type` — the `{module}/{type}` token, e.g. `ignition/database-connection`.
    #[serde(rename = "type", default)]
    pub resource_type: String,
    /// The resource name; absent on a singleton.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `description` (`null` on the captured create).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// `enabled`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// `version` (1 on every capture).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
    /// The collection this copy lives in (`core`, `production`, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    /// Every collection that holds a copy (C-list: `["core","production"]`).
    #[serde(default)]
    pub collections: Vec<String>,
    /// The signature of THIS copy. An update must carry the signature of
    /// the copy it targets (D7: the two copies differ).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// The resource config, order not preserved (see the module doc).
    #[serde(default)]
    pub config: serde_json::Value,
    /// `backupConfig`, when present.
    #[serde(
        rename = "backupConfig",
        alias = "backup_config",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub backup_config: Option<serde_json::Value>,
    /// `attributes` (`uuid`, `enabled` on the captures); shape not modelled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attributes: Option<serde_json::Value>,
    /// Every key above that the captures do not type.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// One item of the `names` list (capture C-list "GET names").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceName {
    /// The resource name.
    #[serde(default)]
    pub name: String,
    /// `enabled`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// The collections that hold the resource, in gateway order.
    #[serde(default)]
    pub modes: Vec<String>,
    /// Any other key.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// The mutation envelope of a create or update: `{success, changes[],
/// problem}` (captures C3a "POST create" and "PUT with the right
/// signature"). `changes[].new_signature` is the POST-write signature: each
/// write rotates it, so it is what the next update must carry.
/// `problem.message` is never surfaced verbatim by this crate (8.3.3
/// messages carry the live signature; the 8.3.6 stacktrace is about 6 KB).
pub type MutationOutcome = crate::client::eam::ModifyOutcome;

/// One resource to create, `POST .../resources/{module}/{type}` (capture
/// C3a "POST create"; Decision D10: a create needs only `name` and
/// `config`, and assumption A9 holds, so there is no `attributes` field).
/// Absent options are not sent, never sent as `null`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResourceCreate {
    /// The resource name.
    pub name: String,
    /// Target collection; the gateway picks one when absent (`core` on the
    /// capture). C-list created a `production` copy by sending it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    /// `enabled`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// `description`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The resource config.
    pub config: serde_json::Value,
    /// `backupConfig`.
    #[serde(rename = "backupConfig", skip_serializing_if = "Option::is_none")]
    pub backup_config: Option<serde_json::Value>,
}

impl ResourceCreate {
    /// A create of `name` with `config` and nothing else.
    pub fn new(name: impl Into<String>, config: serde_json::Value) -> Self {
        Self {
            name: name.into(),
            collection: None,
            enabled: None,
            description: None,
            config,
            backup_config: None,
        }
    }

    /// Target a collection.
    #[must_use]
    pub fn with_collection(mut self, collection: impl Into<String>) -> Self {
        self.collection = Some(collection.into());
        self
    }

    /// Set `enabled`.
    #[must_use]
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = Some(enabled);
        self
    }

    /// Set `description`.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set `backupConfig`.
    #[must_use]
    pub fn with_backup_config(mut self, backup_config: serde_json::Value) -> Self {
        self.backup_config = Some(backup_config);
        self
    }
}

/// One resource to update, an item of the signature-checked array PUT
/// (capture C3a, Decision D3). The signature is PRIVATE and always
/// serialized, so an update without one cannot be built: construction
/// goes through [`ResourceUpdate::new`] or [`ResourceUpdate::from_record`],
/// both of which refuse an empty signature as
/// `resource_signature_rejected` (missing) before any request exists.
///
/// The signature must be the one of the copy being updated (D7: copies in
/// different collections carry different signatures), read just before the
/// write; every successful write rotates it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResourceUpdate {
    /// The resource name; an update of an unknown name is a bare 404 (D10).
    pub name: String,
    /// Collection of the copy being updated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    /// `enabled`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// `description`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The new resource config.
    pub config: serde_json::Value,
    /// `backupConfig`.
    #[serde(rename = "backupConfig", skip_serializing_if = "Option::is_none")]
    pub backup_config: Option<serde_json::Value>,
    signature: String,
}

impl ResourceUpdate {
    /// An update of `name` carrying `signature` and the new `config`.
    ///
    /// An empty or whitespace-only signature is
    /// `resource_signature_rejected` (missing); an empty name is
    /// `invalid_input`. Both are refused before any request.
    pub fn new(
        name: impl Into<String>,
        signature: impl Into<String>,
        config: serde_json::Value,
    ) -> Result<Self, CoreError> {
        let name = name.into();
        let signature = signature.into();
        if name.is_empty() {
            return Err(CoreError::InvalidInput {
                reason: "resource name must not be empty".to_string(),
            });
        }
        if signature.trim().is_empty() {
            return Err(CoreError::ResourceSignatureRejected {
                name,
                reason: SignatureReason::Missing,
                endpoint: None,
            });
        }
        Ok(Self {
            name,
            collection: None,
            enabled: None,
            description: None,
            config,
            backup_config: None,
            signature,
        })
    }

    /// An update seeded from a record just read: its name, signature,
    /// collection, `enabled`, `description`, `config` and `backupConfig`.
    /// A record without a signature is `resource_signature_rejected`
    /// (missing); one without a name (a singleton) is `invalid_input`.
    /// Every capture of a PUT body (C3a) carries only `name`, `signature`,
    /// `description` and `config`. Whether the gateway accepts `collection`,
    /// `enabled` and `backupConfig` on a PUT, and which copy a collection-less
    /// PUT updates, is NOT settled by any capture: the 20-06 live gate
    /// updates with [`ResourceUpdate::new`] only. The live section that
    /// exercises this constructor (two copies present, one update per copy)
    /// is written but has not been run against a gateway yet (20-REVIEW
    /// WR-03), so until it has, treat the extra keys as unproven and prefer
    /// [`ResourceUpdate::new`] with [`ResourceUpdate::with_collection`] only
    /// when a copy must be targeted.
    pub fn from_record(record: &ResourceRecord) -> Result<Self, CoreError> {
        let name = record.name.clone().ok_or_else(|| CoreError::InvalidInput {
            reason: "the record has no name, so it cannot be updated by name".to_string(),
        })?;
        let mut update = Self::new(
            name,
            record.signature.clone().unwrap_or_default(),
            record.config.clone(),
        )?;
        update.collection = record.collection.clone();
        update.enabled = record.enabled;
        update.description = record.description.clone();
        update.backup_config = record.backup_config.clone();
        Ok(update)
    }

    /// The signature this update carries.
    pub fn signature(&self) -> &str {
        &self.signature
    }

    /// Target a collection.
    #[must_use]
    pub fn with_collection(mut self, collection: impl Into<String>) -> Self {
        self.collection = Some(collection.into());
        self
    }

    /// Set `enabled`.
    #[must_use]
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = Some(enabled);
        self
    }

    /// Set `description`.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set `backupConfig`.
    #[must_use]
    pub fn with_backup_config(mut self, backup_config: serde_json::Value) -> Self {
        self.backup_config = Some(backup_config);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_accept_module_ids_and_types() {
        for ok in [
            "ignition",
            "com.inductiveautomation.eam",
            "eam-tasks",
            "a_b",
        ] {
            assert!(validate_segment("module", ok).is_ok(), "{ok}");
        }
        for bad in ["", ".", "..", "../etc", "a/b", "db conn", "é", "a%2Fb"] {
            let err = validate_segment("module", bad).expect_err(bad);
            assert_eq!(err.code(), "invalid_input", "{bad}");
        }
    }

    #[test]
    fn quoted_value_is_capped() {
        let long = "x/".repeat(100);
        let err = validate_segment("type", &long).expect_err("refused");
        assert!(err.to_string().len() < 250, "{err}");
    }

    #[test]
    fn name_segment_is_encoded_and_dot_names_refused() {
        assert_eq!(
            resource_find_path("com.inductiveautomation.eam", "eam-tasks", "a/b c é").unwrap(),
            "/data/api/v1/resources/find/com.inductiveautomation.eam/eam-tasks/a%2Fb%20c%20%C3%A9"
        );
        assert!(resource_find_path("ignition", "t", "").is_err());
        assert!(resource_find_path("ignition", "t", "..").is_err());
    }

    #[test]
    fn filter_goes_out_object_style() {
        let q = ListQuery {
            filter: Some("name[eq]=A b".to_string()),
            ..ListQuery::default()
        };
        let pairs = list_query_pairs(&q).unwrap();
        assert!(pairs.contains(&("name[eq]".to_string(), "A b".to_string())));
        assert!(!pairs.iter().any(|(k, _)| k == "filter"));
        for bad in ["name=A", "name[zz]=A", "[eq]=A", "na me[eq]=A", "name[eq]"] {
            let q = ListQuery {
                filter: Some(bad.to_string()),
                ..ListQuery::default()
            };
            assert!(list_query_pairs(&q).is_err(), "{bad}");
        }
    }
}
