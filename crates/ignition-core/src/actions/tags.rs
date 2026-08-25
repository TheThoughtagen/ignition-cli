//! Tag actions (05-04..05-05) — serde models OUT, no printing.
//!
//! THREE seams, one family (05-RESEARCH's crisp split):
//!
//! - **Providers** (TAGS-01) ride the NATIVE config-resource REST
//!   (`ignition/tag-provider` — healthier data: tagCount metrics,
//!   healthchecks, no deployed route needed). Delete is the
//!   find→signature→delete chain; a find miss refuses with the
//!   family-specific `provider_not_found` (exit 6) instead of a
//!   bare 404. The CLI layer guards the destructive verb
//!   (`--yes`, pre-resolution — the LOCKED shape).
//! - **browse/read/write** (TAGS-02/03/04) ride the deployed
//!   `tags` WebDev route through the 05-03 generic
//!   [`GatewayApi::webdev_route_call`] — every one runs
//!   [`webdev_precondition`] first (the 05-03 shared helper, this
//!   plan's `require_routes` verbatim: probe the tags route's
//!   version handshake; absent → `routes_not_deployed`, unlicensed
//!   → `webdev_unlicensed`, mismatch → `route_version_mismatch`,
//!   all exit 6 with hints naming `ign webdev deploy`). One extra
//!   round trip per command, correctness over latency — no caching
//!   this phase (documented).
//! - **config CRUD / UDTs / bulk export+import** (05-05,
//!   TAGS-05/06/09) ride the deployed `tagConfig` route — the same
//!   precondition, the same generic seam. Configs carry STRINGIFIED
//!   JSON (`value`/`defaultValue`) that gets re-parsed for agents;
//!   import maps the LOCKED Phase-3 collision matrix onto
//!   configure's `'a'`/`'o'` (abort = browse pre-check refusing
//!   `tag_collision` BEFORE any write; overwrite = `--yes`-guarded,
//!   NO pre-check — server authority).
//!
//! Two-layer naming: the client models stay wire-faithful; the
//! action results re-expose selected fields under unit-explicit
//! keys (`tag_count`, …) — the LOCKED convention.

use serde::Serialize;

use crate::actions::webdev::webdev_precondition;
use crate::client::GatewayApi;
use crate::client::query::ListQuery;
use crate::client::tags::{BrowseEntry, TagProviderCreate, TagProviderRecord};
use crate::error::CoreError;

/// `ign tags provider list` row — unit-explicit keys, ALL keys
/// always.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TagProviderRow {
    /// Provider name.
    pub name: String,
    /// Whether the provider resource is enabled.
    pub enabled: bool,
    /// `metrics.tagCount` when the gateway reported one.
    pub tag_count: Option<i64>,
    /// `healthchecks.status` when the gateway reported one.
    pub health: Option<String>,
    /// Gateway-managed providers (the built-in `System` provider —
    /// MANAGED-type; not user-deletable surface).
    pub managed: bool,
}

/// `ign tags provider list` result.
#[derive(Debug, Serialize)]
pub struct TagProvidersResult {
    /// One row per provider, gateway order.
    pub providers: Vec<TagProviderRow>,
}

/// `ign tags provider create` result — the Ok classification IS the
/// success contract (the project-create precedent: create response
/// bodies are opaque).
#[derive(Debug, Serialize)]
pub struct TagProviderCreateResult {
    /// The provider name created.
    pub name: String,
}

/// `ign tags provider delete` result.
#[derive(Debug, Serialize)]
pub struct TagProviderDeleteResult {
    /// The provider name deleted.
    pub deleted: String,
}

/// Map one wire record onto the unit-explicit row (two-layer
/// naming: raw pointers into the passthrough `metrics`/
/// `healthchecks`/`config` values, never interpreted).
fn provider_row(record: &TagProviderRecord) -> TagProviderRow {
    let tag_count = record.metrics.pointer("/tagCount").and_then(|v| v.as_i64());
    let health = record
        .healthchecks
        .pointer("/status")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    // The built-in System provider is MANAGED-type (research); a
    // MANAGED profile type flags it regardless of name.
    let managed = record.name == "System"
        || record
            .config
            .pointer("/profile/type")
            .and_then(|v| v.as_str())
            == Some("MANAGED");
    TagProviderRow {
        name: record.name.clone(),
        enabled: record.enabled,
        tag_count,
        health,
        managed,
    }
}

/// `ign tags provider list` — the native resource list (no deployed
/// routes involved).
pub async fn tag_provider_list(api: &dyn GatewayApi) -> Result<TagProvidersResult, CoreError> {
    let page = api.tag_provider_list(&ListQuery::default()).await?;
    Ok(TagProvidersResult {
        providers: page.items.iter().map(provider_row).collect(),
    })
}

/// `ign tags provider create NAME` — STANDARD profile only at MVP
/// (the create body is the fixed live-proven shape; DB-backed
/// providers are out of scope, README documents).
pub async fn tag_provider_create(
    api: &dyn GatewayApi,
    name: &str,
) -> Result<TagProviderCreateResult, CoreError> {
    api.tag_provider_create(&[TagProviderCreate::standard(name)])
        .await?;
    Ok(TagProviderCreateResult {
        name: name.to_string(),
    })
}

/// `ign tags provider delete NAME` — the find→signature→delete
/// chain: find carries the record (and its mutation signature);
/// delete embeds both on the path. A find miss refuses with the
/// family-specific `provider_not_found` (exit 6) over the bare 404.
pub async fn tag_provider_delete(
    api: &dyn GatewayApi,
    name: &str,
) -> Result<TagProviderDeleteResult, CoreError> {
    let record = api.tag_provider_find(name).await.map_err(|err| match err {
        CoreError::NotFound { .. } => CoreError::ProviderNotFound {
            name: name.to_string(),
            endpoint: err.endpoint(),
        },
        other => other,
    })?;
    let signature = record.signature.clone().ok_or_else(|| {
        CoreError::Internal(format!(
            "tag provider {name:?} find record carried no signature — \
             the delete chain needs one (unexpected wire shape)"
        ))
    })?;
    api.tag_provider_delete(name, &signature).await?;
    Ok(TagProviderDeleteResult {
        deleted: name.to_string(),
    })
}

/// One browse row — unit-explicit keys (two-layer naming over the
/// wire-faithful [`BrowseEntry`]). `path` carries the bracketed
/// `fullPath` (`[default]P5/T1`) so tree NESTING is derivable at the
/// render layer without another round trip.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BrowseRow {
    /// Bracket-qualified fullPath (nesting-derivable).
    pub path: String,
    /// Leaf name.
    pub name: String,
    /// Wire `tagType` token verbatim (Provider/Folder/AtomicTag/
    /// UdtType/UdtInstance/Property).
    pub tag_type: String,
    /// Whether the entry has children (browse-deeper hint).
    pub has_children: bool,
    /// `dataType` for entries that carry one, else null.
    pub data_type: Option<String>,
}

/// `ign tags browse` result — the flat ordered list (JSON mode's
/// stable agent shape; tree RENDERING from `path` nesting is
/// render.rs's job).
#[derive(Debug, Serialize)]
pub struct TagsBrowseResult {
    /// The project the route answered from.
    pub project: String,
    /// The browse path sent (root = `""`).
    pub path: String,
    /// The substring filter applied, when one was.
    pub filter: Option<String>,
    /// Whether Property children were included (display default:
    /// filtered out).
    pub include_properties: bool,
    /// Filtered, gateway-ordered entries.
    pub entries: Vec<BrowseRow>,
}

/// One read row — VERBATIM from the route envelope: quality strings
/// carry embedded detail (`Good`, `Bad_NotFound`, …) and are never
/// parsed further (quality IS data).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TagReadRow {
    /// The tag path read.
    pub path: String,
    /// The value, raw JSON passthrough.
    pub value: serde_json::Value,
    /// Quality string verbatim (never parsed further).
    pub quality: String,
    /// Timestamp string verbatim.
    pub timestamp: String,
}

/// `ign tags read` result — single path = one-element vec (the route
/// is always batch).
#[derive(Debug, Serialize)]
pub struct TagsReadResult {
    /// The project the route answered from.
    pub project: String,
    /// Per-path rows, request order.
    pub results: Vec<TagReadRow>,
}

/// `ign tags write` result.
#[derive(Debug, Serialize)]
pub struct TagsWriteResult {
    /// The project the route answered from.
    pub project: String,
    /// The tag path written.
    pub path: String,
    /// Post-write quality string verbatim (`Good` on success —
    /// quality IS data, the e2e gate's honest oracle).
    pub quality: String,
}

/// The route's tags folder name (the precondition's canonical
/// probe target too — one constant, never drift).
const TAGS_ROUTE: &str = "tags";

/// The display filter: Property children dropped UNLESS included
/// (research display default), then the case-insensitive substring
/// on name+fullPath when one was provided. Pure — unit-pinned.
fn filter_entries(
    entries: Vec<BrowseEntry>,
    filter: Option<&str>,
    include_properties: bool,
) -> Vec<BrowseRow> {
    entries
        .into_iter()
        .filter(|entry| include_properties || entry.tag_type != "Property")
        .filter(|entry| {
            let Some(needle) = filter else {
                return true;
            };
            let needle = needle.to_lowercase();
            entry.name.to_lowercase().contains(&needle)
                || entry.full_path.to_lowercase().contains(&needle)
        })
        .map(|entry| BrowseRow {
            path: entry.full_path,
            name: entry.name,
            tag_type: entry.tag_type,
            has_children: entry.has_children,
            data_type: entry.data_type,
        })
        .collect()
}

/// Deserialize the route's `{results: [...]}` payload rows as
/// [`BrowseEntry`]s (the wire-faithful half of two-layer naming).
fn parse_results<T: serde::de::DeserializeOwned>(
    data: &serde_json::Value,
    context: &str,
) -> Result<Vec<T>, CoreError> {
    serde_json::from_value(data["results"].clone()).map_err(|err| {
        CoreError::Internal(format!(
            "tags route {context} returned an unexpected shape \
             (missing/invalid `results`: {err})"
        ))
    })
}

/// `ign tags browse [PATH]` — route action `browse` → the filtered
/// flat list. Runs the version precondition first (every
/// webdev-dependent command's LOCKED refusal matrix).
pub async fn tags_browse(
    api: &dyn GatewayApi,
    project: &str,
    path: &str,
    filter: Option<&str>,
    include_properties: bool,
) -> Result<TagsBrowseResult, CoreError> {
    webdev_precondition(api, project).await?;
    let data = api
        .webdev_route_call(
            project,
            TAGS_ROUTE,
            &serde_json::json!({"action": "browse", "path": path}),
            &[],
        )
        .await?;
    let entries: Vec<BrowseEntry> = parse_results(&data, "browse")?;
    Ok(TagsBrowseResult {
        project: project.to_string(),
        path: path.to_string(),
        filter: filter.map(str::to_string),
        include_properties,
        entries: filter_entries(entries, filter, include_properties),
    })
}

/// `ign tags read PATH...` — route action `read` (the route is
/// always batch; a single path is a one-element vec). Rows ride
/// VERBATIM from the envelope.
pub async fn tags_read(
    api: &dyn GatewayApi,
    project: &str,
    paths: &[String],
) -> Result<TagsReadResult, CoreError> {
    webdev_precondition(api, project).await?;
    let data = api
        .webdev_route_call(
            project,
            TAGS_ROUTE,
            &serde_json::json!({"action": "read", "paths": paths}),
            &[],
        )
        .await?;
    let wire_rows: Vec<serde_json::Value> = parse_results(&data, "read")?;
    let results = wire_rows
        .into_iter()
        .map(|row| TagReadRow {
            path: row["path"].as_str().unwrap_or_default().to_string(),
            value: row["value"].clone(),
            quality: row["quality"].as_str().unwrap_or_default().to_string(),
            timestamp: row["timestamp"].as_str().unwrap_or_default().to_string(),
        })
        .collect();
    Ok(TagsReadResult {
        project: project.to_string(),
        results,
    })
}

/// `ign tags write PATH --value V` — route action `write`; the value
/// is a JSON scalar the CLI passes through untyped (the
/// write-scalar-is-JSON rule, README-documented).
pub async fn tags_write(
    api: &dyn GatewayApi,
    project: &str,
    path: &str,
    value: serde_json::Value,
) -> Result<TagsWriteResult, CoreError> {
    webdev_precondition(api, project).await?;
    let data = api
        .webdev_route_call(
            project,
            TAGS_ROUTE,
            &serde_json::json!({"action": "write", "path": path, "value": value}),
            &[],
        )
        .await?;
    let mut rows: Vec<serde_json::Value> = parse_results(&data, "write")?;
    let row = if rows.len() == 1 {
        rows.remove(0)
    } else {
        return Err(CoreError::Internal(format!(
            "tags route write returned {} result rows (expected exactly 1)",
            rows.len()
        )));
    };
    Ok(TagsWriteResult {
        project: project.to_string(),
        path: row["path"].as_str().unwrap_or_default().to_string(),
        quality: row["quality"].as_str().unwrap_or_default().to_string(),
    })
}

// ---- config get/create/edit/delete (05-05, TAGS-05) ----
//
// The tagConfig route's action set rides the same 05-03 generic
// seam (no new trait methods): `getConfig` (STRING tagPath — the
// route owns that trap), `configure` (basePath + per-tag names,
// collisionPolicy 'a'/'o'), `deleteTags` (batch paths).

/// The tagConfig route folder name (05-01's config-CRUD/UDT/export
/// route — everything in this plan's second half dispatches here).
const TAG_CONFIG_ROUTE: &str = "tagConfig";

/// `ign tags config get PATH` result — the re-parsed config under
/// unit-explicit keys.
#[derive(Debug, Serialize)]
pub struct TagsConfigGetResult {
    /// The project the route answered from.
    pub project: String,
    /// The tag path requested.
    pub path: String,
    /// The config's `tagType` discriminator when present.
    pub tag_type: Option<String>,
    /// The full config dict with stringified `value`/`defaultValue`
    /// re-parsed into real JSON.
    pub config: serde_json::Value,
}

/// `ign tags config create|edit PATH` result — the configure
/// quality IS the success contract (quality is data).
#[derive(Debug, Serialize)]
pub struct TagsConfigWriteResult {
    /// The project the route answered from.
    pub project: String,
    /// The tag path created/edited.
    pub path: String,
    /// Post-configure quality string verbatim (`Good` on success).
    pub quality: String,
    /// The verb for the human line (`created`/`edited`) — never
    /// serialized (the ProjectSetResult fields-touched precedent).
    #[serde(skip)]
    pub operation: &'static str,
}

/// `ign tags config delete PATH...` result.
#[derive(Debug, Serialize)]
pub struct TagsConfigDeleteResult {
    /// The project the route answered from.
    pub project: String,
    /// The count the route echoed (the request length).
    pub deleted: i64,
}

/// Recursively re-parse STRINGIFIED JSON inside a tag config: the
/// gateway's `getConfiguration` hands `value`/`defaultValue` back as
/// STRINGS containing JSON objects/arrays (05-RESEARCH's
/// serialization hazard — configs carry JSON-in-a-string) — agents
/// must see real JSON. Only OBJECT/ARRAY parses are re-parsed (a
/// string value that parses as a scalar is still semantically a
/// string); unparseable strings pass through verbatim. Pure —
/// unit-pinned.
fn reparse_stringified(config: &mut serde_json::Value) {
    match config {
        serde_json::Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                if (key == "value" || key == "defaultValue")
                    && let serde_json::Value::String(text) = value
                    && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(text)
                    && (parsed.is_object() || parsed.is_array())
                {
                    *value = parsed;
                    continue;
                }
                reparse_stringified(value);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items.iter_mut() {
                reparse_stringified(item);
            }
        }
        _ => {}
    }
}

/// Split a bracket-qualified tag path into `(basePath, name)` for
/// the configure call (05-RESEARCH Pitfall 3: configure takes a
/// basePath — NEVER a provider name — plus per-tag `name`s). The
/// last segment after the provider bracket is the tag name; a BARE
/// path (no brackets) rides under `[default]`. Pure — unit-pinned.
fn split_base_path(path: &str) -> (String, String) {
    let after_bracket = match path.find(']') {
        Some(close) => &path[close + 1..],
        // Bare path: the whole thing is the name under [default].
        None => return ("[default]".to_string(), path.to_string()),
    };
    let bracket_end = path.len() - after_bracket.len();
    match after_bracket.rfind('/') {
        Some(idx) => {
            let split = bracket_end + idx;
            (path[..split].to_string(), path[split + 1..].to_string())
        }
        None => (path[..bracket_end].to_string(), after_bracket.to_string()),
    }
}

/// The shared configure half of create/edit: derive basePath + name
/// from the path, merge the caller's definition (the path-derived
/// name WINS — the path is the argument of record; the CLI never
/// re-shapes the definition — the route owns the four configure
/// traps), then one configure call with the collision policy char.
async fn configure_single(
    api: &dyn GatewayApi,
    project: &str,
    path: &str,
    definition: &serde_json::Value,
    collision_policy: &str,
    operation: &'static str,
) -> Result<TagsConfigWriteResult, CoreError> {
    let (base_path, name) = split_base_path(path);
    if name.is_empty() {
        return Err(CoreError::InvalidInput {
            reason: format!(
                "tag path {path:?} names no tag — use a full path like \
                 [provider]Folder/TagName or [provider]TagName"
            ),
        });
    }
    let serde_json::Value::Object(map) = definition else {
        return Err(CoreError::InvalidInput {
            reason: "tag definition must be a JSON object (tagType/value/alarms/…)".to_string(),
        });
    };
    let mut tag = map.clone();
    tag.insert("name".to_string(), serde_json::Value::String(name.clone()));
    webdev_precondition(api, project).await?;
    let data = api
        .webdev_route_call(
            project,
            TAG_CONFIG_ROUTE,
            &serde_json::json!({
                "action": "configure",
                "basePath": base_path,
                "tags": [tag],
                "collisionPolicy": collision_policy,
            }),
            &[],
        )
        .await?;
    let mut rows: Vec<String> = parse_results(&data, "configure")?;
    let quality = if rows.len() == 1 {
        rows.remove(0)
    } else {
        return Err(CoreError::Internal(format!(
            "tagConfig route configure returned {} result rows (expected exactly 1)",
            rows.len()
        )));
    };
    Ok(TagsConfigWriteResult {
        project: project.to_string(),
        path: path.to_string(),
        quality,
        operation,
    })
}

/// `ign tags config get PATH` — route action `getConfig` (STRING
/// tagPath — the route owns the list-form trap) with the
/// stringified-JSON re-parse applied so agents see real JSON, not
/// JSON-in-a-string.
pub async fn tags_config_get(
    api: &dyn GatewayApi,
    project: &str,
    path: &str,
) -> Result<TagsConfigGetResult, CoreError> {
    webdev_precondition(api, project).await?;
    let data = api
        .webdev_route_call(
            project,
            TAG_CONFIG_ROUTE,
            &serde_json::json!({"action": "getConfig", "tagPath": path}),
            &[],
        )
        .await?;
    let mut config = data
        .get("config")
        .filter(|value| value.is_object())
        .cloned()
        .ok_or_else(|| {
            CoreError::Internal(
                "tagConfig route getConfig returned an unexpected shape (missing `config`)"
                    .to_string(),
            )
        })?;
    let tag_type = config
        .get("tagType")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    reparse_stringified(&mut config);
    Ok(TagsConfigGetResult {
        project: project.to_string(),
        path: path.to_string(),
        tag_type,
        config,
    })
}

/// `ign tags config create PATH --file -` — one configure call with
/// collisionPolicy `'a'` (abort): creating over an existing node
/// refuses server-side rather than silently clobbering. The CLI does
/// NOT re-shape the definition dict (tagType discriminator, nested
/// children, alarms-as-LIST — README documents the required shape;
/// the route owns the four configure traps).
pub async fn tags_config_create(
    api: &dyn GatewayApi,
    project: &str,
    path: &str,
    definition: &serde_json::Value,
) -> Result<TagsConfigWriteResult, CoreError> {
    configure_single(api, project, path, definition, "a", "created").await
}

/// `ign tags config edit PATH --file -` — the same configure call
/// with collisionPolicy `'o'` scoped to the single named node (edit
/// = overwrite that node). NOT `--yes`-guarded: a single-node edit
/// is not a project-wide destructive (the guard set's line).
pub async fn tags_config_edit(
    api: &dyn GatewayApi,
    project: &str,
    path: &str,
    definition: &serde_json::Value,
) -> Result<TagsConfigWriteResult, CoreError> {
    configure_single(api, project, path, definition, "o", "edited").await
}

/// `ign tags config delete PATH...` — route action `deleteTags`
/// (batch paths; the route echoes the request length). Guarded at
/// the CLI layer (destructive) — the action is the wire half.
pub async fn tags_config_delete(
    api: &dyn GatewayApi,
    project: &str,
    paths: &[String],
) -> Result<TagsConfigDeleteResult, CoreError> {
    webdev_precondition(api, project).await?;
    let data = api
        .webdev_route_call(
            project,
            TAG_CONFIG_ROUTE,
            &serde_json::json!({"action": "deleteTags", "paths": paths}),
            &[],
        )
        .await?;
    let deleted = data
        .get("deleted")
        .and_then(|value| value.as_i64())
        .ok_or_else(|| {
            CoreError::Internal(
                "tagConfig route deleteTags returned an unexpected shape (missing `deleted`)"
                    .to_string(),
            )
        })?;
    Ok(TagsConfigDeleteResult {
        project: project.to_string(),
        deleted,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        TagProviderRecord, TagProviderRow, TagProvidersResult, provider_row, tag_provider_delete,
        tag_provider_list,
    };
    use crate::client::GatewayApi;
    use crate::client::query::{ListEnvelope, ListMetadata, ListQuery};
    use crate::client::tags::TagProviderCreate;
    use crate::error::CoreError;

    use std::sync::Mutex;

    /// A scripted double: the provider methods AND the webdev seam
    /// answer from fixtures (recorded so the chain can be
    /// asserted). Everything else is unreachable (the established
    /// action-double shape).
    struct TagsRig {
        providers: Vec<TagProviderRecord>,
        found: Mutex<Vec<String>>,
        deleted: Mutex<Vec<(String, String)>>,
        created: Mutex<Vec<serde_json::Value>>,
        /// The scripted probe answer (default: a matching Present —
        /// the precondition passes).
        probe: crate::client::webdev::RouteProbe,
        /// Recorded route-call bodies (the write body pin's oracle).
        calls: Mutex<Vec<serde_json::Value>>,
        /// The scripted route-call `data` payload.
        route_data: serde_json::Value,
    }

    impl TagsRig {
        fn with(providers: Vec<TagProviderRecord>) -> Self {
            Self {
                providers,
                found: Mutex::new(Vec::new()),
                deleted: Mutex::new(Vec::new()),
                created: Mutex::new(Vec::new()),
                probe: crate::client::webdev::RouteProbe::Present {
                    route_version: crate::webdev::ROUTE_BUNDLE_VERSION.to_string(),
                },
                calls: Mutex::new(Vec::new()),
                route_data: serde_json::json!({"results": []}),
            }
        }

        /// Script the probe answer + the route-call payload.
        fn route(
            mut self,
            probe: crate::client::webdev::RouteProbe,
            route_data: serde_json::Value,
        ) -> Self {
            self.probe = probe;
            self.route_data = route_data;
            self
        }
    }

    fn record(name: &str, tag_count: i64, signature: Option<&str>) -> TagProviderRecord {
        TagProviderRecord {
            name: name.to_string(),
            enabled: true,
            config: serde_json::json!({"profile": {"type": "STANDARD"}}),
            metrics: serde_json::json!({"tagCount": tag_count}),
            healthchecks: serde_json::json!({"status": "OK"}),
            signature: signature.map(str::to_string),
            extra: Default::default(),
        }
    }

    fn page(items: Vec<TagProviderRecord>) -> ListEnvelope<TagProviderRecord> {
        let total = items.len() as i64;
        ListEnvelope {
            items,
            metadata: ListMetadata {
                total,
                matching: total,
                limit: -1,
                offset: 0,
            },
        }
    }

    #[async_trait::async_trait]
    impl GatewayApi for TagsRig {
        async fn tag_provider_list(
            &self,
            _query: &ListQuery,
        ) -> Result<ListEnvelope<TagProviderRecord>, CoreError> {
            Ok(page(self.providers.clone()))
        }
        async fn tag_provider_find(&self, name: &str) -> Result<TagProviderRecord, CoreError> {
            self.found.lock().unwrap().push(name.to_string());
            self.providers
                .iter()
                .find(|record| record.name == name)
                .cloned()
                .ok_or(CoreError::NotFound { endpoint: None })
        }
        async fn tag_provider_create(&self, body: &[TagProviderCreate]) -> Result<(), CoreError> {
            let mut created = self.created.lock().unwrap();
            for record in body {
                created.push(serde_json::to_value(record).expect("serializes"));
            }
            Ok(())
        }
        async fn tag_provider_delete(&self, name: &str, signature: &str) -> Result<(), CoreError> {
            self.deleted
                .lock()
                .unwrap()
                .push((name.to_string(), signature.to_string()));
            Ok(())
        }
        async fn gateway_info(&self) -> Result<crate::client::version::GatewayInfo, CoreError> {
            unreachable!("not part of this action")
        }
        async fn overview(&self) -> Result<crate::client::status::Overview, CoreError> {
            unreachable!("not part of this action")
        }
        async fn status_ping(&self) -> Result<crate::client::status::StatusPing, CoreError> {
            unreachable!("not part of this action")
        }
        async fn modules(
            &self,
            _quarantined: bool,
            _query: &ListQuery,
        ) -> Result<ListEnvelope<crate::client::status::ModuleInfo>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn metrics_current(
            &self,
        ) -> Result<crate::client::metrics::CurrentGauges, CoreError> {
            unreachable!("not part of this action")
        }
        async fn metrics_historic(
            &self,
        ) -> Result<crate::client::metrics::PerformanceCharts, CoreError> {
            unreachable!("not part of this action")
        }
        async fn metrics_threads(&self) -> Result<crate::client::metrics::ThreadCounts, CoreError> {
            unreachable!("not part of this action")
        }
        async fn designers(
            &self,
            _query: &ListQuery,
        ) -> Result<ListEnvelope<crate::client::sessions::DesignerInfo>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn perspective_sessions(
            &self,
            _query: &ListQuery,
        ) -> Result<ListEnvelope<crate::client::sessions::PerspectiveSession>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn vision_clients(
            &self,
            _query: &ListQuery,
        ) -> Result<ListEnvelope<crate::client::sessions::VisionClient>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn terminate_perspective_session(
            &self,
            _id: &str,
            _message: Option<&str>,
        ) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn terminate_vision_client(&self, _id: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn prune_designer(&self, _id: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn database_connections(
            &self,
        ) -> Result<ListEnvelope<crate::client::connections::GatewayConnection>, CoreError>
        {
            unreachable!("not part of this action")
        }
        async fn opc_connections(
            &self,
        ) -> Result<ListEnvelope<crate::client::connections::GatewayConnection>, CoreError>
        {
            unreachable!("not part of this action")
        }
        async fn logs(
            &self,
            _filter: &crate::client::logs::LogQuery,
        ) -> Result<ListEnvelope<crate::client::logs::LogEntry>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn logs_download(&self) -> Result<crate::client::logs::LogDownload, CoreError> {
            unreachable!("not part of this action")
        }
        async fn loggers(
            &self,
            _query: &ListQuery,
        ) -> Result<ListEnvelope<crate::client::logs::LoggerInfo>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn set_logger_level(&self, _logger: &str, _level: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn reset_logger_levels(&self) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn restart(&self) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn scan_projects(&self) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn security_properties(
            &self,
        ) -> Result<crate::client::restart::SecurityProperties, CoreError> {
            unreachable!("not part of this action")
        }
        async fn webdev_route_status(&self, _route: &str) -> Result<u16, CoreError> {
            unreachable!("not part of this action")
        }
        async fn webdev_route_call(
            &self,
            _project: &str,
            _route: &str,
            body: &serde_json::Value,
            _extra_headers: &[(&str, &str)],
        ) -> Result<serde_json::Value, CoreError> {
            self.calls.lock().unwrap().push(body.clone());
            Ok(self.route_data.clone())
        }
        async fn webdev_route_probe(
            &self,
            _project: &str,
            _route: &str,
            _extra_headers: &[(&str, &str)],
        ) -> Result<crate::client::webdev::RouteProbe, CoreError> {
            Ok(self.probe.clone())
        }
        async fn projects(
            &self,
            _query: &ListQuery,
        ) -> Result<ListEnvelope<crate::client::projects::ProjectRecord>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_find(
            &self,
            _name: &str,
        ) -> Result<crate::client::projects::ProjectRecord, CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_create(
            &self,
            _body: &crate::client::projects::ProjectCreate,
        ) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_copy(&self, _from: &str, _to: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_rename(&self, _name: &str, _new_name: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_modify(
            &self,
            _name: &str,
            _body: &crate::client::projects::ProjectModify,
        ) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_delete(&self, _name: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_export_to_file(
            &self,
            _name: &str,
            _out: &std::path::Path,
        ) -> Result<crate::client::projects::ExportMeta, CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_import(
            &self,
            _name: &str,
            _zip: Vec<u8>,
            _overwrite: bool,
        ) -> Result<crate::client::projects::ImportOutcome, CoreError> {
            unreachable!("not part of this action")
        }
        async fn trial_status_wire(&self) -> Result<crate::client::trial::TrialWire, CoreError> {
            unreachable!("not part of this action")
        }
        async fn banners(&self) -> Result<crate::client::trial::BannerSet, CoreError> {
            unreachable!("not part of this action")
        }
        async fn trial_reset_wire(&self) -> Result<crate::client::trial::TrialWire, CoreError> {
            unreachable!("not part of this action")
        }
        async fn backup_download(
            &self,
            _out: &std::path::Path,
        ) -> Result<crate::client::projects::ExportMeta, CoreError> {
            unreachable!("not part of this action")
        }
        async fn backup_restore(&self, _gwbk: &std::path::Path) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
    }

    /// The unit-explicit row mapping: tagCount/health pointered out
    /// of the passthrough values; System (and MANAGED-type
    /// providers) flagged managed.
    #[test]
    fn provider_rows_map_unit_explicit_keys() {
        let mut system = record("System", 3, None);
        system.config = serde_json::json!({"profile": {"type": "MANAGED"}});
        let rows: Vec<TagProviderRow> = [record("default", 12, None), system]
            .iter()
            .map(provider_row)
            .collect();
        assert_eq!(
            rows[0],
            TagProviderRow {
                name: "default".into(),
                enabled: true,
                tag_count: Some(12),
                health: Some("OK".into()),
                managed: false,
            }
        );
        assert!(rows[1].managed, "System is MANAGED-type");
        // A MANAGED profile type flags managed regardless of name.
        let mut managed = record("weird", 0, None);
        managed.config = serde_json::json!({"profile": {"type": "MANAGED"}});
        assert!(provider_row(&managed).managed);
        // Missing metrics/health degrade to None — all keys still
        // present.
        let bare = TagProviderRecord {
            name: "bare".into(),
            enabled: false,
            config: serde_json::Value::Null,
            metrics: serde_json::Value::Null,
            healthchecks: serde_json::Value::Null,
            signature: None,
            extra: Default::default(),
        };
        let row = provider_row(&bare);
        assert_eq!(row.tag_count, None);
        assert_eq!(row.health, None);
    }

    /// The delete chain: find → signature → delete, with the
    /// family-specific refusal when find misses.
    #[tokio::test]
    async fn provider_delete_chains_find_signature_delete() {
        let rig = TagsRig::with(vec![record("default", 12, Some("sig-42"))]);
        let result = tag_provider_delete(&rig, "default")
            .await
            .expect("chain deletes");
        assert_eq!(result.deleted, "default");
        assert_eq!(
            *rig.deleted.lock().unwrap(),
            vec![("default".into(), "sig-42".into())]
        );

        let rig = TagsRig::with(Vec::new());
        let err = tag_provider_delete(&rig, "ghost")
            .await
            .expect_err("find miss refuses");
        assert_eq!(err.code(), "provider_not_found");
        assert_eq!(err.exit_code(), 6);
        assert!(
            err.hint().unwrap().contains("ign tags provider list"),
            "hint names the fix: {err}"
        );
        assert!(rig.deleted.lock().unwrap().is_empty(), "zero deletes ran");
    }

    /// `tags provider list` maps every record into the agent shape.
    #[tokio::test]
    async fn provider_list_maps_rows() {
        let rig = TagsRig::with(vec![record("default", 12, None), record("System", 0, None)]);
        let result: TagProvidersResult = tag_provider_list(&rig).await.expect("lists");
        assert_eq!(result.providers.len(), 2);
        assert_eq!(result.providers[1].name, "System");
        assert!(result.providers[1].managed);
    }

    // ---- browse/read/write (TAGS-02/03/04) ----

    use super::{BrowseRow, filter_entries, tags_browse, tags_read, tags_write};
    use crate::client::tags::BrowseEntry;
    use crate::client::webdev::RouteProbe;
    use crate::webdev::ROUTE_BUNDLE_VERSION as BUNDLE_VERSION;

    fn entry(full_path: &str, name: &str, tag_type: &str) -> BrowseEntry {
        BrowseEntry {
            full_path: full_path.to_string(),
            name: name.to_string(),
            tag_type: tag_type.to_string(),
            has_children: false,
            data_type: None,
        }
    }

    /// THE display default: Property children are dropped UNLESS
    /// explicitly included (research display default).
    #[test]
    fn browse_filter_drops_properties_unless_included() {
        let entries = vec![
            entry("[default]", "default", "Provider"),
            entry("[default]T1", "T1", "AtomicTag"),
            entry("[default]T1.valueSource", "valueSource", "Property"),
        ];
        let rows = filter_entries(entries.clone(), None, false);
        assert_eq!(rows.len(), 2, "Property dropped by default");
        assert!(rows.iter().all(|row| row.tag_type != "Property"));

        let rows = filter_entries(entries, None, true);
        assert_eq!(rows.len(), 3, "--include-properties keeps them");
    }

    /// The substring filter is case-insensitive and matches EITHER
    /// the leaf name OR the full path.
    #[test]
    fn browse_filter_substring_matches_name_or_path_case_insensitively() {
        let entries = vec![
            entry("[default]Pump1", "Pump1", "AtomicTag"),
            entry("[default]PUMP2", "PUMP2", "AtomicTag"),
            entry("[default]Motor1", "Motor1", "AtomicTag"),
            entry("[default]Area/Pump3", "Pump3", "AtomicTag"),
        ];
        // Name match, case-insensitive both directions.
        let rows = filter_entries(entries.clone(), Some("pump"), false);
        assert_eq!(
            rows.iter().map(|row| row.path.as_str()).collect::<Vec<_>>(),
            vec!["[default]Pump1", "[default]PUMP2", "[default]Area/Pump3"]
        );
        // Path-only match (needle hits the folder, not the leaf).
        let rows = filter_entries(entries, Some("area/"), false);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, "[default]Area/Pump3");
    }

    /// Rows carry the unit-explicit keys with fullPath as `path`
    /// (nesting-derivable).
    #[test]
    fn browse_rows_map_unit_explicit_keys() {
        let mut tag = entry("[default]T1", "T1", "AtomicTag");
        tag.has_children = true;
        tag.data_type = Some("Int4".into());
        let rows = filter_entries(vec![tag], None, false);
        assert_eq!(
            rows[0],
            BrowseRow {
                path: "[default]T1".into(),
                name: "T1".into(),
                tag_type: "AtomicTag".into(),
                has_children: true,
                data_type: Some("Int4".into()),
            }
        );
    }

    /// browse rides the precondition + the route action and filters
    /// the payload's entries.
    #[tokio::test]
    async fn browse_probes_then_calls_and_filters() {
        let rig = TagsRig::with(Vec::new()).route(
            RouteProbe::Present {
                route_version: BUNDLE_VERSION.to_string(),
            },
            serde_json::json!({"results": [
                {"fullPath": "[default]", "name": "default", "tagType": "Provider", "hasChildren": true, "dataType": null},
                {"fullPath": "[default]T1.value", "name": "value", "tagType": "Property", "hasChildren": false, "dataType": "Float8"}
            ]}),
        );
        let result = tags_browse(&rig, "ign-cli", "", None, false)
            .await
            .expect("browse filters");
        assert_eq!(result.entries.len(), 1, "Property filtered by default");
        assert_eq!(result.entries[0].tag_type, "Provider");
        assert_eq!(result.project, "ign-cli");
        // The recorded call: precondition passed, browse dispatched.
        let calls = rig.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["action"], "browse");
        assert_eq!(calls[0]["path"], "");
    }

    /// THE refusal inheritance: an absent route (405 probe) refuses
    /// `routes_not_deployed` (exit 6) BEFORE any route call — the
    /// precondition every webdev-dependent command runs.
    #[tokio::test]
    async fn browse_refuses_when_routes_absent() {
        let rig = TagsRig::with(Vec::new()).route(RouteProbe::Absent, serde_json::json!({}));
        let err = tags_browse(&rig, "ign-cli", "", None, false)
            .await
            .expect_err("absent routes refuse");
        assert_eq!(err.code(), "routes_not_deployed");
        assert_eq!(err.exit_code(), 6);
        assert!(
            rig.calls.lock().unwrap().is_empty(),
            "zero route calls ran past the refusal"
        );
    }

    /// A version-mismatched route refuses `route_version_mismatch`
    /// (the redeploy-or-update hint is the error's own).
    #[tokio::test]
    async fn browse_refuses_on_version_mismatch() {
        let rig = TagsRig::with(Vec::new()).route(
            RouteProbe::Present {
                route_version: "0.9.0".to_string(),
            },
            serde_json::json!({}),
        );
        let err = tags_browse(&rig, "ign-cli", "", None, false)
            .await
            .expect_err("mismatched version refuses");
        assert_eq!(err.code(), "route_version_mismatch");
        assert_eq!(err.exit_code(), 6);
    }

    /// read passes rows through VERBATIM (value raw JSON, quality/
    /// timestamp strings never parsed further) and always rides the
    /// batch shape.
    #[tokio::test]
    async fn read_passes_rows_through_verbatim() {
        let rig = TagsRig::with(Vec::new()).route(
            RouteProbe::Present {
                route_version: BUNDLE_VERSION.to_string(),
            },
            serde_json::json!({"results": [
                {"path": "[default]T1", "value": 7, "quality": "Good", "timestamp": "Mon Aug 24 00:00:00 UTC 2026"},
                {"path": "[default]Ghost", "value": null, "quality": "Bad_NotFound", "timestamp": "Mon Aug 24 00:00:00 UTC 2026"}
            ]}),
        );
        let result = tags_read(
            &rig,
            "ign-cli",
            &["[default]T1".into(), "[default]Ghost".into()],
        )
        .await
        .expect("read parses");
        assert_eq!(result.results.len(), 2);
        assert_eq!(result.results[0].value, 7);
        assert_eq!(result.results[1].quality, "Bad_NotFound");
        // The wire body pinned: batch paths array, request order.
        let calls = rig.calls.lock().unwrap();
        assert_eq!(
            calls[0]["paths"],
            serde_json::json!(["[default]T1", "[default]Ghost"])
        );
    }

    /// THE write body pin: `{action, path, value}` — value riding
    /// EXACTLY as passed (a JSON scalar, untyped at this layer).
    #[tokio::test]
    async fn write_body_pins_path_and_value_exactly() {
        let rig = TagsRig::with(Vec::new()).route(
            RouteProbe::Present {
                route_version: BUNDLE_VERSION.to_string(),
            },
            serde_json::json!({"results": [{"path": "[default]T1", "quality": "Good"}]}),
        );
        let result = tags_write(&rig, "ign-cli", "[default]T1", serde_json::json!(42))
            .await
            .expect("write parses");
        assert_eq!(result.quality, "Good");
        assert_eq!(result.path, "[default]T1");
        let calls = rig.calls.lock().unwrap();
        assert_eq!(
            calls[0],
            serde_json::json!({"action": "write", "path": "[default]T1", "value": 42}),
            "the write body is exactly action+path+value"
        );
    }

    /// Write inherits the precondition too (the refusal matrix is
    /// every webdev-dependent verb's, not just browse's).
    #[tokio::test]
    async fn write_refuses_when_routes_absent() {
        let rig = TagsRig::with(Vec::new()).route(RouteProbe::Absent, serde_json::json!({}));
        let err = tags_write(&rig, "ign-cli", "[default]T1", serde_json::json!(1))
            .await
            .expect_err("absent routes refuse");
        assert_eq!(err.code(), "routes_not_deployed");
        assert!(rig.calls.lock().unwrap().is_empty());
    }

    // ---- config get/create/edit/delete (05-05, TAGS-05) ----

    use super::{
        TagsConfigGetResult, configure_single, reparse_stringified, split_base_path,
        tags_config_create, tags_config_delete, tags_config_edit, tags_config_get,
    };

    fn present() -> RouteProbe {
        RouteProbe::Present {
            route_version: BUNDLE_VERSION.to_string(),
        }
    }

    /// THE stringified re-parse: `value`/`defaultValue` strings
    /// containing JSON objects/arrays become real JSON; scalar-parse
    /// strings and unparseable strings stay STRINGS (semantics
    /// preserved); other keys are untouched; nested children are
    /// walked.
    #[test]
    fn reparse_stringified_rewrites_only_structured_value_strings() {
        let mut config = serde_json::json!({
            "name": "T1",
            "tagType": "AtomicTag",
            "value": "{\"dataType\": \"Int4\", \"value\": 123}",
            "defaultValue": "[1, 2, 3]",
            "scaleFactor": "{\"not\": \"a value key\"}",
            "notes": "plain text stays text",
            "numericString": "123",
            "children": [
                {"name": "C1", "value": "{\"a\": 1}", "junk": "not json {"}
            ]
        });
        reparse_stringified(&mut config);
        assert_eq!(
            config["value"],
            serde_json::json!({"dataType": "Int4", "value": 123}),
            "stringified object value re-parsed into real JSON"
        );
        assert_eq!(config["defaultValue"], serde_json::json!([1, 2, 3]));
        assert_eq!(
            config["scaleFactor"],
            serde_json::json!("{\"not\": \"a value key\"}"),
            "other string keys are NOT re-parsed"
        );
        assert_eq!(config["notes"], "plain text stays text");
        assert_eq!(config["numericString"], "123", "scalar parses stay strings");
        assert_eq!(
            config["children"][0]["value"],
            serde_json::json!({"a": 1}),
            "nested children are walked"
        );
        assert_eq!(config["children"][0]["junk"], "not json {");
    }

    /// The path split: last segment after the bracket is the name;
    /// everything before (bracket-qualified) is the basePath; a
    /// BARE path rides under `[default]`.
    #[test]
    fn split_base_path_derives_configure_operands() {
        assert_eq!(
            split_base_path("[default]P5/T1"),
            ("[default]P5".into(), "T1".into())
        );
        assert_eq!(
            split_base_path("[default]T1"),
            ("[default]".into(), "T1".into())
        );
        assert_eq!(
            split_base_path("[p5e2e]Area/Motor1"),
            ("[p5e2e]Area".into(), "Motor1".into())
        );
        assert_eq!(split_base_path("T1"), ("[default]".into(), "T1".into()));
        // A provider-only path names no tag — create/edit refuse it
        // (the empty-name guard's input).
        assert_eq!(
            split_base_path("[default]"),
            ("[default]".into(), "".into())
        );
    }

    /// config get rides the precondition + getConfig with the STRING
    /// tagPath, and the returned config is RE-PARSED (agents see
    /// real JSON — the must-have).
    #[tokio::test]
    async fn config_get_reparse_and_body_pin() {
        let rig = TagsRig::with(Vec::new()).route(
            present(),
            serde_json::json!({"config": {
                "name": "T1",
                "tagType": "AtomicTag",
                "value": "{\"dataType\": \"Int4\", \"value\": 123}"
            }}),
        );
        let result: TagsConfigGetResult = tags_config_get(&rig, "ign-cli", "[default]T1")
            .await
            .expect("config get parses");
        assert_eq!(result.tag_type.as_deref(), Some("AtomicTag"));
        assert_eq!(
            result.config["value"],
            serde_json::json!({"dataType": "Int4", "value": 123}),
            "the stringified value is re-parsed"
        );
        let calls = rig.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0],
            serde_json::json!({"action": "getConfig", "tagPath": "[default]T1"}),
            "the getConfig body is exactly action+tagPath (STRING arg)"
        );
    }

    /// THE create body pin: basePath split + the path-derived name +
    /// collisionPolicy 'a' — the definition rides verbatim beside
    /// the injected name.
    #[tokio::test]
    async fn config_create_body_pins_base_path_and_abort_policy() {
        let rig =
            TagsRig::with(Vec::new()).route(present(), serde_json::json!({"results": ["Good"]}));
        let result = tags_config_create(
            &rig,
            "ign-cli",
            "[p5e2e]Area/Motor1",
            &serde_json::json!({"tagType": "AtomicTag", "value": 42}),
        )
        .await
        .expect("create configures");
        assert_eq!(result.quality, "Good");
        assert_eq!(result.operation, "created");
        let calls = rig.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0],
            serde_json::json!({
                "action": "configure",
                "basePath": "[p5e2e]Area",
                "tags": [{"tagType": "AtomicTag", "value": 42, "name": "Motor1"}],
                "collisionPolicy": "a"
            }),
            "create = configure with the split basePath and abort policy"
        );
    }

    /// Edit is the same call with collisionPolicy 'o' (overwrite the
    /// single named node) — and the PATH-derived name wins over any
    /// name inside the definition (the path is the argument of
    /// record).
    #[tokio::test]
    async fn config_edit_pins_overwrite_policy_and_name_precedence() {
        let rig =
            TagsRig::with(Vec::new()).route(present(), serde_json::json!({"results": ["Good"]}));
        tags_config_edit(
            &rig,
            "ign-cli",
            "[default]T1",
            &serde_json::json!({"tagType": "AtomicTag", "name": "WrongName"}),
        )
        .await
        .expect("edit configures");
        let calls = rig.calls.lock().unwrap();
        assert_eq!(calls[0]["collisionPolicy"], "o");
        assert_eq!(calls[0]["basePath"], "[default]");
        assert_eq!(
            calls[0]["tags"][0]["name"], "T1",
            "path wins over the definition's name"
        );
    }

    /// The deleteTags pin: batch paths on the body, count echoed.
    #[tokio::test]
    async fn config_delete_pins_batch_paths() {
        let rig = TagsRig::with(Vec::new()).route(present(), serde_json::json!({"deleted": 2}));
        let result = tags_config_delete(
            &rig,
            "ign-cli",
            &["[default]T1".to_string(), "[default]T2".to_string()],
        )
        .await
        .expect("delete dispatches");
        assert_eq!(result.deleted, 2);
        let calls = rig.calls.lock().unwrap();
        assert_eq!(
            calls[0],
            serde_json::json!({"action": "deleteTags", "paths": ["[default]T1", "[default]T2"]})
        );
    }

    /// Precondition refusal inheritance regression pin: absent routes
    /// refuse BEFORE any tagConfig call (zero route calls).
    #[tokio::test]
    async fn config_get_refuses_when_routes_absent() {
        let rig = TagsRig::with(Vec::new()).route(RouteProbe::Absent, serde_json::json!({}));
        let err = tags_config_get(&rig, "ign-cli", "[default]T1")
            .await
            .expect_err("absent routes refuse");
        assert_eq!(err.code(), "routes_not_deployed");
        assert_eq!(err.exit_code(), 6);
        assert!(rig.calls.lock().unwrap().is_empty());
    }

    /// A provider-only path (no tag name) and a non-object definition
    /// refuse invalid_input PRE-resolution of the route (zero wire
    /// work) — the usage-error class.
    #[tokio::test]
    async fn config_create_refuses_path_and_shape_usage_errors() {
        let rig = TagsRig::with(Vec::new()).route(present(), serde_json::json!({}));
        let err = configure_single(
            &rig,
            "ign-cli",
            "[default]",
            &serde_json::json!({"tagType": "AtomicTag"}),
            "a",
            "created",
        )
        .await
        .expect_err("provider-only path refuses");
        assert_eq!(err.code(), "invalid_input");
        assert_eq!(err.exit_code(), 2);

        let err = tags_config_create(&rig, "ign-cli", "[default]T1", &serde_json::json!([1]))
            .await
            .expect_err("non-object definition refuses");
        assert_eq!(err.code(), "invalid_input");
        assert!(
            rig.calls.lock().unwrap().is_empty(),
            "zero route calls past the usage refusals"
        );
    }
}
