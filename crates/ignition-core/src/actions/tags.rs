//! Tag actions (05-04..05-06) — serde models OUT, no printing.
//!
//! FOUR seams, one family (05-RESEARCH's crisp split):
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
//! - **alarms active/history/ack + tag history query** (05-06,
//!   TAGS-07/08) ride the deployed `alarms` and `tagHistory`
//!   routes — the same precondition, the same seam. Alarm history
//!   on a default rig refuses `alarm_journal_missing` (exit 6, the
//!   actionable journal-chain hint); history query passes
//!   `{columns, rows}` through with `t_stamp` preserved EXACTLY
//!   (never renamed — the prior-art defect the route corrects).
//!
//! Two-layer naming: the client models stay wire-faithful; the
//! action results re-expose selected fields under unit-explicit
//! keys (`tag_count`, …) — the LOCKED convention.

use serde::Serialize;

use crate::actions::projects::CollisionPolicy;
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

// ---- offline export browsing (07-04, INTR-03) ------------------------------
//
// `tags browse --from-export <PATH>` parses a tag export OFFLINE —
// no gateway, no route precondition, no credential (the docker-verb
// precedent for non-gateway commands). THREE layouts accepted
// (07-RESEARCH Focus 6, source-read from ignition-git-module's
// GitTagManager.java):
//
// 1. a DIRECTORY in the git-module layout — a `tags/` root (or the
//    dir itself when it directly holds provider folders/`.json`
//    files), one provider per subfolder in EITHER on-disk format:
//    individual files (one `.json` per leaf tag in a mirroring
//    hierarchy, folders = directories, `_types_/*.json` = UDT
//    definitions, the tag `name` field STRIPPED — derived from the
//    filename) or the legacy single-file `<provider>.json` (whole
//    tree). Dot-entries skip (the module's own rule);
//    `.tag-config.json` is config, not a provider; the `System`
//    provider is excluded (`.tag-config.json` semantics).
// 2. a FILE holding the CLI's own `tags export` interchange (the
//    normalized list-of-subtrees array).
// 3. a FILE holding a legacy `<provider>.json` whole tree — the
//    provider is the file stem.
//
// Rows emit the EXISTING [`BrowseRow`] shape (bracketed fullPath)
// so the CLI's tree renderer and flat JSON render ride verbatim —
// zero render changes.

/// `ign tags browse --from-export` result — the offline contract:
/// `source: "export"` + the origin path + the (filtered) rows.
#[derive(Debug, Serialize)]
pub struct TagBrowseFromExportResult {
    /// Always `"export"` — the offline provenance.
    pub source: &'static str,
    /// The path browsed.
    pub origin: String,
    /// The filtered rows (flat agent shape; the tree renders from
    /// path nesting).
    pub entries: Vec<BrowseRow>,
}

/// The row-level twin of [`filter_entries`]: Property children drop
/// unless included, then the case-insensitive substring on
/// name+path. Pure — the offline rows pass through the SAME display
/// rules the live browse applies.
fn filter_rows(
    rows: Vec<BrowseRow>,
    filter: Option<&str>,
    include_properties: bool,
) -> Vec<BrowseRow> {
    rows.into_iter()
        .filter(|row| include_properties || row.tag_type != "Property")
        .filter(|row| {
            let Some(needle) = filter else {
                return true;
            };
            let needle = needle.to_lowercase();
            row.name.to_lowercase().contains(&needle) || row.path.to_lowercase().contains(&needle)
        })
        .collect()
}

/// Decode a git-module filesystem name: `%XX` hex escapes for the
/// reserved set (`<>:"/\|?*`), control chars, and `%` itself
/// (round-trips on every OS). Invalid escapes ride verbatim.
fn decode_fs_name(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = &raw[i + 1..i + 3];
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The per-provider ROOT row: `[provider]` (the live browse's
/// provider shape).
fn provider_root_row(provider: &str) -> BrowseRow {
    BrowseRow {
        path: format!("[{provider}]"),
        name: provider.to_string(),
        tag_type: "Provider".to_string(),
        has_children: true,
        data_type: None,
    }
}

/// Walk one export SUBTREE into rows under `parent` (the 05-06
/// rule: an empty-named / Provider-typed wrapper lands its CHILDREN
/// at the current level — no row for the wrapper itself). A tag's
/// `tagType` rides verbatim when present; a subtree without one
/// infers Folder (has `tags`) vs AtomicTag.
fn walk_subtree(
    provider: &str,
    parent: &str,
    subtree: &serde_json::Value,
    rows: &mut Vec<BrowseRow>,
) {
    let Some(object) = subtree.as_object() else {
        return;
    };
    let name = object
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let tag_type = object
        .get("tagType")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let children = object.get("tags").and_then(serde_json::Value::as_array);
    if tag_type == "Provider" || name.is_empty() {
        // The provider-shaped wrapper: children land at the current
        // level (live-proven 05-06 — the effective-top-level rule).
        if let Some(kids) = children {
            for child in kids {
                walk_subtree(provider, parent, child, rows);
            }
        }
        return;
    }
    let path = if parent.is_empty() {
        format!("[{provider}]{name}")
    } else {
        format!("{parent}/{name}")
    };
    let has_children = children.is_some_and(|kids| !kids.is_empty());
    let tag_type = if tag_type.is_empty() {
        if has_children { "Folder" } else { "AtomicTag" }
    } else {
        tag_type
    };
    rows.push(BrowseRow {
        path: path.clone(),
        name: name.to_string(),
        tag_type: tag_type.to_string(),
        has_children,
        data_type: object
            .get("dataType")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
    });
    if let Some(kids) = children {
        for child in kids {
            walk_subtree(provider, &path, child, rows);
        }
    }
}

/// Walk a provider FOLDER in the git-module individual-file layout:
/// directories are folders, `.json` files are leaf tags (name from
/// the DECODED filename — the format strips the `name` field),
/// `_types_/` holds UDT definitions at the provider root (a missing
/// `tagType` defaults UdtType there, AtomicTag elsewhere).
fn walk_provider_dir(
    provider: &str,
    dir: &std::path::Path,
    parent: &str,
    rows: &mut Vec<BrowseRow>,
) -> Result<(), CoreError> {
    let mut entries: Vec<std::fs::DirEntry> = std::fs::read_dir(dir)
        .map_err(|err| CoreError::InvalidInput {
            reason: format!("cannot walk {}: {err}", dir.display()),
        })?
        .collect::<Result<_, _>>()
        .map_err(|err| CoreError::InvalidInput {
            reason: format!("cannot walk {}: {err}", dir.display()),
        })?;
    entries.sort_by_key(|entry| entry.file_name());
    let is_types_folder = dir.file_name().is_some_and(|name| name == "_types_");
    // The child path under an EMPTY parent rides directly after the
    // provider bracket (`[prov]tag` — the bracket-join convention
    // browse_depth renders); deeper levels join with `/`.
    let child_path = |decoded: &str| {
        if parent.is_empty() {
            format!("[{provider}]{decoded}")
        } else {
            format!("{parent}/{decoded}")
        }
    };
    for entry in entries {
        let raw_name = entry.file_name();
        let name = raw_name.to_string_lossy();
        if name.starts_with('.') {
            continue; // dot-entries skip (the module's own rule)
        }
        let path = entry.path();
        if path.is_dir() {
            let decoded = decode_fs_name(&name);
            let child = child_path(&decoded);
            rows.push(BrowseRow {
                path: child.clone(),
                name: decoded,
                tag_type: "Folder".to_string(),
                has_children: true,
                data_type: None,
            });
            walk_provider_dir(provider, &path, &child, rows)?;
        } else if let Some(stem) = name.strip_suffix(".json") {
            // A leaf tag file: tagType from the JSON (the native
            // TAG_GSON deterministic copy), name from the filename.
            let json: serde_json::Value = std::fs::read(&path)
                .map_err(|err| CoreError::InvalidInput {
                    reason: format!("cannot read {}: {err}", path.display()),
                })
                .and_then(|bytes| {
                    serde_json::from_slice(&bytes).map_err(|err| CoreError::InvalidInput {
                        reason: format!("{} is not valid JSON: {err}", path.display()),
                    })
                })?;
            let decoded = decode_fs_name(stem);
            let default_type = if is_types_folder {
                "UdtType"
            } else {
                "AtomicTag"
            };
            rows.push(BrowseRow {
                path: child_path(&decoded),
                name: decoded,
                tag_type: json
                    .get("tagType")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(default_type)
                    .to_string(),
                has_children: json
                    .get("tags")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|kids| !kids.is_empty()),
                data_type: json
                    .get("dataType")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
            });
        }
        // Non-.json files skip (binary payloads the module keeps
        // aside); folders under a leaf-name dir recurse above.
    }
    Ok(())
}

/// `ign tags browse --from-export PATH` — the OFFLINE parse (no
/// [`GatewayApi`], no route precondition, no credential): a
/// directory (the git-module layout — a `tags/` root, or the dir
/// itself when it directly holds provider folders/`.json` files) or
/// a file (the CLI's own export interchange, or a legacy
/// `<provider>.json` whole tree — provider = file stem). A
/// nonexistent path or unparseable JSON is usage-class (exit 2).
pub fn browse_rows_from_export(
    path: &std::path::Path,
    include_properties: bool,
    filter: Option<&str>,
) -> Result<TagBrowseFromExportResult, CoreError> {
    let invalid = |reason: String| CoreError::InvalidInput { reason };
    let meta = std::fs::metadata(path)
        .map_err(|err| invalid(format!("cannot read {}: {err}", path.display())))?;
    let mut rows: Vec<BrowseRow> = Vec::new();
    if meta.is_dir() {
        // The git-module root: <project>/tags/ when present, else the
        // dir itself IF it directly holds provider dirs/.json files.
        let tags_root = path.join("tags");
        let has_tags_root = tags_root.is_dir();
        let base = if has_tags_root {
            tags_root
        } else {
            path.to_path_buf()
        };
        let mut entries: Vec<std::fs::DirEntry> = std::fs::read_dir(&base)
            .map_err(|err| invalid(format!("cannot walk {}: {err}", base.display())))?
            .collect::<Result<_, _>>()
            .map_err(|err| invalid(format!("cannot walk {}: {err}", base.display())))?;
        entries.sort_by_key(|entry| entry.file_name());
        let providers: Vec<std::fs::DirEntry> = entries
            .into_iter()
            .filter(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with('.') || name == ".tag-config.json" {
                    return false; // config + dot-entries skip
                }
                entry.path().is_dir() || name.ends_with(".json")
            })
            .collect();
        if providers.is_empty() {
            return Err(invalid(format!(
                "{} is not a tag export layout (no provider folders or .json \
                 files under it{})",
                path.display(),
                if has_tags_root { "/tags" } else { "" }
            )));
        }
        for entry in providers {
            let provider_name = entry.file_name().to_string_lossy().into_owned();
            let provider_path = entry.path();
            if provider_path.is_dir() {
                // `System` is always excluded (`.tag-config.json`
                // semantics — managed provider, README notes).
                if provider_name == "System" {
                    continue;
                }
                rows.push(provider_root_row(&provider_name));
                walk_provider_dir(&provider_name, &provider_path, "", &mut rows)?;
            } else {
                // A legacy single-file provider tree at the root
                // level: `<provider>.json`.
                let stem = provider_name
                    .strip_suffix(".json")
                    .unwrap_or(&provider_name)
                    .to_string();
                let bytes = std::fs::read(&provider_path).map_err(|err| {
                    invalid(format!("cannot read {}: {err}", provider_path.display()))
                })?;
                let json: serde_json::Value = serde_json::from_slice(&bytes).map_err(|err| {
                    invalid(format!(
                        "{} is not valid JSON: {err}",
                        provider_path.display()
                    ))
                })?;
                rows.push(provider_root_row(&stem));
                match &json {
                    serde_json::Value::Array(subtrees) => {
                        for subtree in subtrees {
                            walk_subtree(&stem, "", subtree, &mut rows);
                        }
                    }
                    other => walk_subtree(&stem, "", other, &mut rows),
                }
            }
        }
    } else {
        // A FILE: the CLI's own interchange (array of subtrees) or a
        // legacy whole tree — provider = file stem.
        let stem = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "tags".to_string());
        let bytes = std::fs::read(path)
            .map_err(|err| invalid(format!("cannot read {}: {err}", path.display())))?;
        let json: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|err| invalid(format!("{} is not valid JSON: {err}", path.display())))?;
        match &json {
            serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                rows.push(provider_root_row(&stem));
                let subtrees: Vec<&serde_json::Value> = match &json {
                    serde_json::Value::Array(items) => items.iter().collect(),
                    serde_json::Value::Object(_) => vec![&json],
                    _ => unreachable!("the outer match bounded the shape"),
                };
                for subtree in subtrees {
                    walk_subtree(&stem, "", subtree, &mut rows);
                }
            }
            _ => {
                return Err(invalid(format!(
                    "{} is not a tag export (expected an array of tag \
                     subtrees or a provider tree object)",
                    path.display()
                )));
            }
        }
    }
    Ok(TagBrowseFromExportResult {
        source: "export",
        origin: path.display().to_string(),
        entries: filter_rows(rows, filter, include_properties),
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

// ---- alarms active/history/ack (05-06, TAGS-07) ----
//
// The alarms route's action set rides the same 05-03 generic seam:
// `active` (queryStatus with OPTIONAL filter kwargs — only present
// filters ride the body), `history` (queryJournal — a DEFAULT rig
// denies with the structured `no_alarm_journal` code, which the
// client seam maps to the actionable `alarm_journal_missing` exit 6),
// `acknowledge` (the gateway-scope 3-arg String[]/note/username form
// whose return IS the unacknowledged remainder — acknowledged is
// computed client-side honestly from requested − remainder).

/// The alarms route folder name (the deployed bundle's alarm
/// lifecycle route).
const ALARMS_ROUTE: &str = "alarms";

/// One active alarm row — unit-explicit keys over the route's
/// camelCase passthrough (`eventId` → `event_id`). State strings read
/// `'Active, Unacknowledged'` verbatim — never parsed further (the
/// quality-is-data convention).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AlarmRow {
    /// The alarm event's id (a UUID, stringified route-side).
    pub event_id: String,
    /// The alarm's source path.
    pub source: String,
    /// State string verbatim (`'Active, Unacknowledged'`, …).
    pub state: String,
    /// Priority string verbatim (`'High'`, …).
    pub priority: String,
    /// The alarm definition's name, when the event carries one.
    pub name: Option<String>,
}

/// `ign tags alarms active` result.
#[derive(Debug, Serialize)]
pub struct TagsAlarmsActiveResult {
    /// The project the route answered from.
    pub project: String,
    /// Active alarm rows, gateway order.
    pub alarms: Vec<AlarmRow>,
    /// Row count (== `alarms.len()`).
    pub count: usize,
}

/// `ign tags alarms history` result — journal rows ride VERBATIM as
/// raw values (the wire shape is journal-dataset-dependent: columns
/// vary by journal schema and Ignition version, so the row dicts pass
/// through unparsed under the `{columns, rows}` convention the
/// tagHistory query established). `columns` derives from the first
/// row's keys (empty when the journal answered no rows).
#[derive(Debug, Serialize)]
pub struct TagsAlarmsHistoryResult {
    /// The project the route answered from.
    pub project: String,
    /// Column names (the first row's keys; empty on an empty journal).
    pub columns: Vec<String>,
    /// Journal rows verbatim.
    pub rows: Vec<serde_json::Value>,
    /// Row count.
    pub count: usize,
}

/// `ign tags alarms ack` result — the 8.3 acknowledge return IS the
/// UNacknowledged remainder; `acknowledged` is computed client-side
/// honestly (requested − remainder), never trusted from a field the
/// route never sent.
#[derive(Debug, Serialize)]
pub struct TagsAlarmsAckResult {
    /// The project the route answered from.
    pub project: String,
    /// How many of the requested ids the gateway acknowledged.
    pub acknowledged: usize,
    /// The ids that REMAIN unacknowledged (the route's own return).
    pub unacknowledged: Vec<String>,
}

/// `ign tags alarms active` — route action `active`; only PRESENT
/// filters ride the body (the route passes them through to
/// `system.alarm.queryStatus` as kwargs verbatim).
pub async fn tags_alarms_active(
    api: &dyn GatewayApi,
    project: &str,
    source: Option<&str>,
    priority: Option<&str>,
    state: Option<&str>,
) -> Result<TagsAlarmsActiveResult, CoreError> {
    webdev_precondition(api, project).await?;
    let mut body = serde_json::json!({"action": "active"});
    for (key, value) in [("source", source), ("priority", priority), ("state", state)] {
        if let Some(value) = value {
            body[key] = serde_json::Value::String(value.to_string());
        }
    }
    let data = api
        .webdev_route_call(project, ALARMS_ROUTE, &body, &[])
        .await?;
    let wire_rows: Vec<serde_json::Value> = parse_results(&data, "active")?;
    let alarms: Vec<AlarmRow> = wire_rows
        .into_iter()
        .map(|row| AlarmRow {
            event_id: row["eventId"].as_str().unwrap_or_default().to_string(),
            source: row["source"].as_str().unwrap_or_default().to_string(),
            state: row["state"].as_str().unwrap_or_default().to_string(),
            priority: row["priority"].as_str().unwrap_or_default().to_string(),
            name: row["name"].as_str().map(str::to_string),
        })
        .collect();
    let count = alarms.len();
    Ok(TagsAlarmsActiveResult {
        project: project.to_string(),
        alarms,
        count,
    })
}

/// `ign tags alarms history` — route action `history`. On a DEFAULT
/// rig (no journal chain) this refuses
/// [`CoreError::AlarmJournalMissing`] — the honest, actionable
/// default-rig path (exit 6, hint names the provisioning chain).
pub async fn tags_alarms_history(
    api: &dyn GatewayApi,
    project: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<TagsAlarmsHistoryResult, CoreError> {
    webdev_precondition(api, project).await?;
    let data = api
        .webdev_route_call(
            project,
            ALARMS_ROUTE,
            &serde_json::json!({
                "action": "history",
                "startDateMs": start_ms,
                "endDateMs": end_ms,
            }),
            &[],
        )
        .await?;
    let rows: Vec<serde_json::Value> = parse_results(&data, "history")?;
    let columns: Vec<String> = rows
        .first()
        .and_then(serde_json::Value::as_object)
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default();
    let count = rows.len();
    Ok(TagsAlarmsHistoryResult {
        project: project.to_string(),
        columns,
        rows,
        count,
    })
}

/// `ign tags alarms ack ID...` — route action `acknowledge` (the
/// gateway-scope 3-arg form: String[] ids, note, username — the CLI
/// demands the username explicitly, no default-guessing). The route's
/// return is the UNacknowledged remainder; acknowledged is the honest
/// client-side difference.
///
/// ID normalization (the view→ack loop, 05-08): a 36-char
/// hyphenated id (the shape `tags alarms active` prints) passes
/// through VERBATIM; anything shorter rides prefix expansion against
/// the active-alarm list — exactly one match substitutes the full
/// UUID, ambiguous/unknown prefixes refuse `invalid_input` (exit 2)
/// naming the candidates / the miss. The wire call therefore always
/// carries full UUIDs.
pub async fn tags_alarms_ack(
    api: &dyn GatewayApi,
    project: &str,
    ids: &[String],
    note: &str,
    username: &str,
) -> Result<TagsAlarmsAckResult, CoreError> {
    webdev_precondition(api, project).await?;
    let expanded = normalize_ack_ids(api, project, ids).await?;
    let data = api
        .webdev_route_call(
            project,
            ALARMS_ROUTE,
            &serde_json::json!({
                "action": "acknowledge",
                "eventIds": expanded,
                "note": note,
                "username": username,
            }),
            &[],
        )
        .await?;
    let unacknowledged: Vec<String> = data
        .get("unacknowledged")
        .and_then(|value| value.as_array())
        .map(|array| {
            array
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .ok_or_else(|| {
            CoreError::Internal(
                "alarms route acknowledge returned an unexpected shape (missing `unacknowledged`)"
                    .to_string(),
            )
        })?;
    let acknowledged = ids.len().saturating_sub(unacknowledged.len());
    Ok(TagsAlarmsAckResult {
        project: project.to_string(),
        acknowledged,
        unacknowledged,
    })
}

/// The full-UUID shape check: 36 chars with hyphens at indices
/// 8/13/18/23 (the canonical stringified form the route demands).
/// Shape ONLY — no uuid crate, no validation of the hex body (the
/// route remains the authority on id validity).
fn is_full_uuid_shape(id: &str) -> bool {
    id.len() == 36
        && id.as_bytes().get(8) == Some(&b'-')
        && id.as_bytes().get(13) == Some(&b'-')
        && id.as_bytes().get(18) == Some(&b'-')
        && id.as_bytes().get(23) == Some(&b'-')
}

/// Expand ack ids to full UUIDs. All-full input passes through with
/// ZERO extra round trips; any short id triggers one `active` lookup
/// (same project — it runs its own precondition; the correctness-
/// over-latency trade the family locked in 05-04), and prefixes
/// match against the active list. Mixed short/full ids expand
/// independently.
async fn normalize_ack_ids(
    api: &dyn GatewayApi,
    project: &str,
    ids: &[String],
) -> Result<Vec<String>, CoreError> {
    if ids.iter().all(|id| is_full_uuid_shape(id)) {
        return Ok(ids.to_vec());
    }
    let active = tags_alarms_active(api, project, None, None, None).await?;
    let mut expanded = Vec::with_capacity(ids.len());
    for id in ids {
        if is_full_uuid_shape(id) {
            expanded.push(id.clone());
            continue;
        }
        let matches: Vec<&str> = active
            .alarms
            .iter()
            .map(|alarm| alarm.event_id.as_str())
            .filter(|event_id| event_id.starts_with(id.as_str()))
            .collect();
        match matches.as_slice() {
            [one] => expanded.push((*one).to_string()),
            [] => {
                return Err(CoreError::InvalidInput {
                    reason: format!(
                        "no active alarm's eventId starts with `{id}` — it may already be \
                         acknowledged or cleared; full ids ride `tags alarms active --json`"
                    ),
                });
            }
            many => {
                return Err(CoreError::InvalidInput {
                    reason: format!(
                        "eventId prefix `{id}` is ambiguous — it matches {} active alarms; \
                         pass a longer prefix or one of: {}",
                        many.len(),
                        many.join(", ")
                    ),
                });
            }
        }
    }
    Ok(expanded)
}

// ---- config get/create/edit/delete (05-05, TAGS-05) ----
//
// The tagConfig route's action set rides the same 05-03 generic
// seam (no new trait methods): `getConfig` (STRING tagPath — the
// route owns that trap), `configure` (basePath + per-tag names,
// collisionPolicy 'a'/'o'), `deleteTags` (batch paths).

// ---- tag history query (05-06, TAGS-08) ----
//
// The tagHistory route's `query` action rides the same seam. The
// route wraps `Date(long(ms))` route-side (Pitfall 12), so the
// CLIENT SENDS EPOCH MS — the CLI's --start/--end accept RFC3339 OR
// epoch-ms and parse to ms pre-resolution
// ([`parse_time_ms`]). Structurally safe on ANY rig (zero
// historians → a well-formed dataset with null values); data
// requires a provisioned historian (the e2e fixture provisions an
// InternalHistorian via native REST — no database needed).

/// The tagHistory route folder name (the deployed bundle's history
/// query route).
const TAG_HISTORY_ROUTE: &str = "tagHistory";

/// `ign tags history query` result — the dataset VERBATIM: the
/// `t_stamp` column preserved EXACTLY (never renamed — the
/// prior-art defect the route corrects), tag columns
/// provider-relative, every cell raw JSON.
#[derive(Debug, Serialize)]
pub struct TagsHistoryQueryResult {
    /// The project the route answered from.
    pub project: String,
    /// The tag paths queried, request order.
    pub paths: Vec<String>,
    /// Column names verbatim (`t_stamp` first, then per-tag paths).
    pub columns: Vec<String>,
    /// Row cells verbatim (aligned to `columns`).
    pub rows: Vec<Vec<serde_json::Value>>,
    /// Row count.
    pub row_count: usize,
}

/// `ign tags history query PATH...` — route action `query` →
/// `{columns, rows}` verbatim. `return_size` and `aggregation` ride
/// the body ONLY when present (the route defaults returnSize itself
/// and reads `aggregationMode` — an absent aggregation falls back to
/// route-side LastValue).
pub async fn tags_history_query(
    api: &dyn GatewayApi,
    project: &str,
    paths: &[String],
    start_ms: i64,
    end_ms: i64,
    return_size: Option<i64>,
    aggregation: Option<&str>,
) -> Result<TagsHistoryQueryResult, CoreError> {
    webdev_precondition(api, project).await?;
    let mut body = serde_json::json!({
        "action": "query",
        "paths": paths,
        "startDateMs": start_ms,
        "endDateMs": end_ms,
    });
    if let Some(size) = return_size {
        body["returnSize"] = serde_json::json!(size);
    }
    if let Some(mode) = aggregation {
        body["aggregationMode"] = serde_json::Value::String(mode.to_string());
    }
    let data = api
        .webdev_route_call(project, TAG_HISTORY_ROUTE, &body, &[])
        .await?;
    let columns: Vec<String> = data
        .get("columns")
        .and_then(serde_json::Value::as_array)
        .map(|array| {
            array
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect()
        })
        .ok_or_else(|| {
            CoreError::Internal(
                "tagHistory route query returned an unexpected shape (missing `columns`)"
                    .to_string(),
            )
        })?;
    let rows: Vec<Vec<serde_json::Value>> = data
        .get("rows")
        .and_then(serde_json::Value::as_array)
        .map(|array| {
            array
                .iter()
                .filter_map(|value| value.as_array().cloned())
                .collect()
        })
        .ok_or_else(|| {
            CoreError::Internal(
                "tagHistory route query returned an unexpected shape (missing `rows`)".to_string(),
            )
        })?;
    let row_count = rows.len();
    Ok(TagsHistoryQueryResult {
        project: project.to_string(),
        paths: paths.to_vec(),
        columns,
        rows,
        row_count,
    })
}

/// Parse a CLI time argument into EPOCH MILLISECONDS — either raw
/// epoch-ms (all digits) or an RFC3339 timestamp
/// (`2026-08-25T12:00:00Z`, `2026-08-25T12:00:00.123+02:00`; a
/// space separator and lowercase `t`/`z` tolerated). Zero-dep on
/// purpose: `days_from_civil` is the hand-rolled inverse of the
/// CLI's `iso_utc` (the Howard Hinnant pair). Pure — unit-pinned.
pub fn parse_time_ms(input: &str) -> Result<i64, CoreError> {
    let input = input.trim();
    if !input.is_empty() && input.bytes().all(|b| b.is_ascii_digit()) {
        return input.parse::<i64>().map_err(|err| CoreError::InvalidInput {
            reason: format!("epoch-ms time {input:?} does not parse as an integer: {err}"),
        });
    }
    parse_rfc3339_ms(input).ok_or_else(|| CoreError::InvalidInput {
        reason: format!(
            "time {input:?} is neither epoch milliseconds (digits) nor an RFC3339 \
             timestamp (e.g. 2026-08-25T12:00:00Z or 2026-08-25T14:00:00+02:00)"
        ),
    })
}

/// RFC3339 → epoch ms. Hand-rolled: date (Y-M-D) → days-from-civil,
/// time-of-day + fractional ms, timezone `Z`/`±HH:MM` offset. None
/// on any shape violation (the caller renders the usage refusal).
fn parse_rfc3339_ms(input: &str) -> Option<i64> {
    let bytes = input.as_bytes();
    if bytes.len() < 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !matches!(bytes[10], b'T' | b't' | b' ')
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return None;
    }
    let year: i64 = input.get(0..4)?.parse().ok()?;
    let month: u32 = input.get(5..7)?.parse().ok()?;
    let day: u32 = input.get(8..10)?.parse().ok()?;
    let hour: i64 = input.get(11..13)?.parse().ok()?;
    let minute: i64 = input.get(14..16)?.parse().ok()?;
    let second: i64 = input.get(17..19)?.parse().ok()?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let mut rest = &input[19..];
    // Fractional seconds: up to 3 digits become milliseconds
    // (`.1` = 100 ms, `.1234` truncates to 123 ms).
    let mut millis: i64 = 0;
    if let Some(frac) = rest.strip_prefix('.') {
        let digits = frac.chars().take_while(char::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        let taken = digits.min(3);
        let value: i64 = frac[..taken].parse().ok()?;
        millis = value * 10_i64.pow(3 - taken as u32);
        rest = &rest[1 + digits..];
    }
    let offset_s: i64 = match rest {
        "Z" | "z" => 0,
        _ if rest.len() == 6 && matches!(rest.as_bytes()[0], b'+' | b'-') => {
            if rest.as_bytes()[3] != b':' {
                return None;
            }
            let sign: i64 = if rest.starts_with('-') { -1 } else { 1 };
            let offset_hour: i64 = rest.get(1..3)?.parse().ok()?;
            let offset_minute: i64 = rest.get(4..6)?.parse().ok()?;
            if offset_hour > 23 || offset_minute > 59 {
                return None;
            }
            sign * (offset_hour * 3600 + offset_minute * 60)
        }
        _ => return None,
    };
    let days = days_from_civil(year, month, day);
    let secs = days * 86_400 + hour * 3600 + minute * 60 + second;
    Some(secs * 1000 + millis - offset_s * 1000)
}

/// (year, month, day) → days since 1970-01-01 — Howard Hinnant's
/// `days_from_civil`, the inverse of the CLI's `civil_from_days`.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400; // [0, 399]
    let month_prime: i64 = if month > 2 {
        month as i64 - 3
    } else {
        month as i64 + 9
    }; // [0, 11]
    let day_of_year = (153 * month_prime + 2) / 5 + day as i64 - 1; // [0, 365]
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year; // [0, 146096]
    era * 146_097 + day_of_era - 719_468
}

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

// ---- UDT types/definitions (05-05, TAGS-06) ----

/// One UDT type row from `listUDTTypes` — unit-explicit keys.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TagsUdtTypeRow {
    /// The UDT type's name (its `_types_` leaf).
    pub name: String,
    /// Wire `tagType` token verbatim (`UdtType`).
    pub tag_type: String,
}

/// `ign tags udt types` result.
#[derive(Debug, Serialize)]
pub struct TagsUdtTypesResult {
    /// The project the route answered from.
    pub project: String,
    /// The provider whose `_types_` folder was browsed.
    pub provider: String,
    /// The type entries, gateway order.
    pub types: Vec<TagsUdtTypeRow>,
}

/// `ign tags udt def NAME` result — the recursive definition with
/// the stringified re-parse applied.
#[derive(Debug, Serialize)]
pub struct TagsUdtDefResult {
    /// The project the route answered from.
    pub project: String,
    /// The provider whose `_types_` folder was read.
    pub provider: String,
    /// The UDT type's name.
    pub name: String,
    /// The full recursive definition (parameters + nested children),
    /// stringified values re-parsed.
    pub definition: serde_json::Value,
}

/// `ign tags udt types` — route action `listUDTTypes` (the route
/// browses `[provider]_types_`).
pub async fn tags_udt_types(
    api: &dyn GatewayApi,
    project: &str,
    provider: &str,
) -> Result<TagsUdtTypesResult, CoreError> {
    webdev_precondition(api, project).await?;
    let data = api
        .webdev_route_call(
            project,
            TAG_CONFIG_ROUTE,
            &serde_json::json!({"action": "listUDTTypes", "provider": provider}),
            &[],
        )
        .await?;
    let entries: Vec<BrowseEntry> = parse_results(&data, "listUDTTypes")?;
    Ok(TagsUdtTypesResult {
        project: project.to_string(),
        provider: provider.to_string(),
        types: entries
            .into_iter()
            .map(|entry| TagsUdtTypeRow {
                name: entry.name,
                tag_type: entry.tag_type,
            })
            .collect(),
    })
}

/// `ign tags udt def NAME` — route action `getUDTDefinition` (the
/// route walks `[provider]_types_/NAME` recursively); the SAME
/// stringified re-parse is applied so agents see real JSON.
pub async fn tags_udt_def(
    api: &dyn GatewayApi,
    project: &str,
    provider: &str,
    name: &str,
) -> Result<TagsUdtDefResult, CoreError> {
    webdev_precondition(api, project).await?;
    let data = api
        .webdev_route_call(
            project,
            TAG_CONFIG_ROUTE,
            &serde_json::json!({"action": "getUDTDefinition", "provider": provider, "name": name}),
            &[],
        )
        .await?;
    let mut definition = data
        .get("definition")
        .filter(|value| value.is_object())
        .cloned()
        .ok_or_else(|| {
            CoreError::Internal(
                "tagConfig route getUDTDefinition returned an unexpected shape (missing `definition`)"
                    .to_string(),
            )
        })?;
    reparse_stringified(&mut definition);
    Ok(TagsUdtDefResult {
        project: project.to_string(),
        provider: provider.to_string(),
        name: name.to_string(),
        definition,
    })
}

// ---- bulk export/import (05-05, TAGS-09) ----

/// `ign tags export` result — the artifact line's data (file mode)
/// or the payload itself (stdout mode, printed raw by the render
/// layer).
#[derive(Debug, Serialize)]
pub struct TagsExportResult {
    /// The project the route answered from.
    pub project: String,
    /// The exported paths, request order.
    pub paths: Vec<String>,
    /// The file the pretty JSON landed in (stdout mode: null).
    pub file: Option<String>,
    /// Whether the payload rode to stdout (the export-streaming
    /// convention's stdout half — `-o -`).
    pub stdout: bool,
    /// Top-level subtree count in the payload.
    pub tag_count: usize,
    /// The pretty payload in stdout mode — NEVER serialized (the
    /// render layer prints it raw as the sanctioned stdout
    /// exception; the ProjectSetResult serde-skip precedent).
    #[serde(skip)]
    pub payload: Option<String>,
}

/// `ign tags import` result — counts + provider.
#[derive(Debug, Serialize)]
pub struct TagsImportResult {
    /// The project the route answered from.
    pub project: String,
    /// The target provider imported into.
    pub provider: String,
    /// The policy that ran (`abort`/`overwrite` — the stable
    /// labels).
    pub collision_policy: String,
    /// Top-level subtree count imported.
    pub imported: usize,
}

/// The default export file name: the FIRST path's last segment
/// (provider brackets stripped), sanitized to filesystem-safe
/// characters, `.json` — the export-streaming convention's file
/// half. Pure — unit-pinned.
pub fn default_export_file_name(paths: &[String]) -> String {
    let first = paths.first().map(String::as_str).unwrap_or("tags");
    // Strip the provider bracket prefix ([p5e2e]P5 → P5; a
    // provider-only path falls back to the provider name itself).
    let stem = match first.find(']') {
        Some(close) if first.starts_with('[') => &first[close + 1..],
        _ => first,
    };
    let stem = if stem.is_empty() {
        &first[1..first.find(']').unwrap_or(first.len())]
    } else {
        stem
    };
    let last = stem.rsplit('/').next().unwrap_or(stem);
    let sanitized: String = last
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let stem = if sanitized.is_empty() {
        "tags".to_string()
    } else {
        sanitized
    };
    format!("{stem}.json")
}

/// `ign tags export PATH...` — route action `exportTags` (the
/// kwargs-only form is enforced route-side). The JSON-string payload
/// is PARSED (never stored opaque) and NORMALIZED to the list-of-
/// subtrees interchange format: the live gateway answers a SINGLE
/// subtree object for one path and the `{"tags": [...]}` wrapper for
/// several — never a bare array (live-proven 05-06; a bare array is
/// tolerated defensively) — then written PRETTY to the out file
/// (stdout mode when `out` is None — the render layer prints it
/// raw). JSON ONLY: the planner lock (the gateway's native
/// interchange; xml/csv deferred to backlog as documented
/// format-discretion).
pub async fn tags_export(
    api: &dyn GatewayApi,
    project: &str,
    paths: &[String],
    out: Option<&std::path::Path>,
) -> Result<TagsExportResult, CoreError> {
    webdev_precondition(api, project).await?;
    let data = api
        .webdev_route_call(
            project,
            TAG_CONFIG_ROUTE,
            &serde_json::json!({"action": "exportTags", "paths": paths}),
            &[],
        )
        .await?;
    let payload_text = data
        .get("payload")
        .and_then(|value| value.as_str())
        .ok_or_else(|| {
            CoreError::Internal(
                "tagConfig route exportTags returned an unexpected shape (missing `payload`)"
                    .to_string(),
            )
        })?;
    // Parse + normalize: the LIVE payload shapes (05-06 live-run
    // discovery) are a SINGLE subtree object (one path) or the
    // multi-path wrapper `{"tags": [...]}` — never a bare array. The
    // normalized list-of-subtrees is the CLI's interchange format
    // (import + the round-trip oracle); a bare array is tolerated
    // defensively.
    let parsed: serde_json::Value = serde_json::from_str(payload_text).map_err(|err| {
        CoreError::Internal(format!(
            "tagConfig route exportTags returned a non-JSON payload: {err}"
        ))
    })?;
    let subtrees: Vec<serde_json::Value> = match &parsed {
        serde_json::Value::Array(items) => items.clone(),
        serde_json::Value::Object(map)
            if map.len() == 1 && map.get("tags").is_some_and(serde_json::Value::is_array) =>
        {
            map["tags"].as_array().cloned().unwrap_or_default()
        }
        serde_json::Value::Object(_) => vec![parsed.clone()],
        _ => {
            return Err(CoreError::Internal(
                "tagConfig route exportTags payload is not tag subtrees (object or {tags: [...]})"
                    .to_string(),
            ));
        }
    };
    let tag_count = subtrees.len();
    let pretty = serde_json::to_string_pretty(&subtrees)
        .map_err(|err| CoreError::Internal(err.to_string()))?;
    match out {
        Some(path) => {
            std::fs::write(path, format!("{pretty}\n")).map_err(|err| {
                CoreError::Internal(format!("cannot write {}: {err}", path.display()))
            })?;
            Ok(TagsExportResult {
                project: project.to_string(),
                paths: paths.to_vec(),
                file: Some(path.display().to_string()),
                stdout: false,
                tag_count,
                payload: None,
            })
        }
        None => Ok(TagsExportResult {
            project: project.to_string(),
            paths: paths.to_vec(),
            file: None,
            stdout: true,
            tag_count,
            payload: Some(pretty),
        }),
    }
}

/// The top-level tag names a configure call with this subtree will
/// LAND at the target basePath: a named subtree lands itself; an
/// EMPTY-named subtree (the provider-shaped export wrapper) lands
/// its CHILDREN (live-proven 05-06 — configuring
/// `{name: "", tagType: Provider, tags: [...]}` under `[target]`
/// creates the children at the target's top level). Pure —
/// unit-pinned.
fn effective_top_level_names(subtree: &serde_json::Value) -> Vec<String> {
    let name = subtree
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if !name.is_empty() {
        return vec![name.to_string()];
    }
    subtree
        .get("tags")
        .and_then(serde_json::Value::as_array)
        .map(|children| {
            children
                .iter()
                .filter_map(|child| child.get("name").and_then(serde_json::Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// `ign tags import --file -` — the LOCKED Phase-3 collision matrix
/// mapped onto configure's `'a'`/`'o'` (03-02 verbatim):
///
/// - **abort (default)**: a browse of the target basePath
///   (`[provider]`) pre-checks for EXISTING top-level names — any
///   collision refuses [`CoreError::TagCollision`] (exit 6, hint
///   names `--collision-policy overwrite`) BEFORE any route write,
///   then configure runs with `'a'` (the server-side backstop).
/// - **overwrite**: `--yes`-guarded pre-resolution at the CLI layer
///   with NO pre-check (the server is the authority), configure
///   `'o'`.
/// - **merge** is not a clap value (Designer-only, README).
///
/// The payload is the `tags export` shape (a parsed JSON array of
/// subtrees) sent to configure VERBATIM — the gateway's own
/// interchange format round-trips untouched.
pub async fn tags_import(
    api: &dyn GatewayApi,
    project: &str,
    provider: &str,
    payload: serde_json::Value,
    collision: CollisionPolicy,
) -> Result<TagsImportResult, CoreError> {
    let tags = payload
        .as_array()
        .cloned()
        .ok_or_else(|| CoreError::InvalidInput {
            reason: "import payload must be a JSON array of tag definitions \
                     (the `tags export` shape)"
                .to_string(),
        })?;
    // The EFFECTIVE top-level names: a subtree with an EMPTY name is
    // the provider-shaped export wrapper — configure lands its
    // CHILDREN at the target (live-proven 05-06), so the children's
    // names are what actually arrive.
    let effective_names: Vec<String> = tags.iter().flat_map(effective_top_level_names).collect();
    let imported = effective_names.len();
    webdev_precondition(api, project).await?;
    if matches!(collision, CollisionPolicy::Abort) {
        // The zero-write pre-check: browse the target basePath, refuse
        // on ANY effective top-level name overlap (03-02's
        // find-precheck shape mapped onto the route seam).
        let data = api
            .webdev_route_call(
                project,
                TAGS_ROUTE,
                &serde_json::json!({"action": "browse", "path": format!("[{provider}]")}),
                &[],
            )
            .await?;
        let entries: Vec<BrowseEntry> = parse_results(&data, "browse")?;
        // `_types_` is STRUCTURAL — every provider carries the UDT
        // types folder, and the server's own abort policy accepts
        // configuring it (live-proven Good); it never counts as a
        // collision.
        let collisions: Vec<String> = effective_names
            .iter()
            .filter(|name| **name != "_types_" && entries.iter().any(|entry| &entry.name == *name))
            .cloned()
            .collect();
        if !collisions.is_empty() {
            return Err(CoreError::TagCollision {
                provider: provider.to_string(),
                names: collisions,
                endpoint: Some(crate::client::webdev::route_url(project, TAGS_ROUTE)),
            });
        }
    }
    let policy_char = match collision {
        CollisionPolicy::Abort => "a",
        CollisionPolicy::Overwrite => "o",
    };
    let data = api
        .webdev_route_call(
            project,
            TAG_CONFIG_ROUTE,
            &serde_json::json!({
                "action": "configure",
                "basePath": format!("[{provider}]"),
                "tags": tags,
                "collisionPolicy": policy_char,
            }),
            &[],
        )
        .await?;
    let _qualities: Vec<String> = parse_results(&data, "configure")?;
    Ok(TagsImportResult {
        project: project.to_string(),
        provider: provider.to_string(),
        collision_policy: collision.label().to_string(),
        imported,
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
        /// A per-call response queue (the export→import round-trip
        /// needs SEQUENTIAL different answers); when empty every call
        /// falls back to `route_data`.
        responses: Mutex<Vec<serde_json::Value>>,
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
                responses: Mutex::new(Vec::new()),
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

        /// Script a per-call response QUEUE (each route call pops
        /// the front; an exhausted queue reuses the last answer).
        fn responses(self, queue: Vec<serde_json::Value>) -> Self {
            *self.responses.lock().unwrap() = queue;
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
            let mut queue = self.responses.lock().unwrap();
            if queue.len() > 1 {
                Ok(queue.remove(0))
            } else if let Some(last) = queue.first() {
                Ok(last.clone())
            } else {
                Ok(self.route_data.clone())
            }
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
            _backup_type: crate::client::backup::BackupType,
        ) -> Result<crate::client::projects::ExportMeta, CoreError> {
            unreachable!("not part of this action")
        }
        async fn backup_restore(&self, _gwbk: &std::path::Path) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn eam_task_history(
            &self,
            _limit: Option<u32>,
            _search: Option<&str>,
        ) -> Result<crate::client::query::ListEnvelope<crate::client::eam::EamHistoryItem>, CoreError>
        {
            unreachable!("not part of this action")
        }
        async fn eam_task_definitions(
            &self,
        ) -> Result<crate::client::query::ListEnvelope<crate::client::eam::EamTaskRecord>, CoreError>
        {
            unreachable!("not part of this action")
        }
        async fn eam_task_find(
            &self,
            _name: &str,
        ) -> Result<crate::client::eam::EamTaskRecord, CoreError> {
            unreachable!("not part of this action")
        }
        async fn eam_task_create(&self, _definition: &serde_json::Value) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn eam_task_force(&self, _owner: &str, _name: &str) -> Result<(), CoreError> {
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

    // ---- UDT types/def + export/import (05-05, TAGS-06/09) ----

    use super::{
        CollisionPolicy, TagsExportResult, TagsImportResult, default_export_file_name, tags_export,
        tags_import, tags_udt_def, tags_udt_types,
    };

    /// udt types rides listUDTTypes with the provider on the body and
    /// maps the browse-entry results into `{name, tag_type}` rows.
    #[tokio::test]
    async fn udt_types_maps_rows_and_pins_body() {
        let rig = TagsRig::with(Vec::new()).route(
            present(),
            serde_json::json!({"results": [
                {"fullPath": "[default]_types_/Motor", "name": "Motor", "tagType": "UdtType", "hasChildren": true, "dataType": null},
                {"fullPath": "[default]_types_/Pump", "name": "Pump", "tagType": "UdtType", "hasChildren": true, "dataType": null}
            ]}),
        );
        let result = tags_udt_types(&rig, "ign-cli", "default")
            .await
            .expect("udt types parses");
        assert_eq!(result.types.len(), 2);
        assert_eq!(result.types[0].name, "Motor");
        assert_eq!(result.types[0].tag_type, "UdtType");
        let calls = rig.calls.lock().unwrap();
        assert_eq!(
            calls[0],
            serde_json::json!({"action": "listUDTTypes", "provider": "default"})
        );
    }

    /// udt def rides getUDTDefinition and applies the SAME
    /// stringified re-parse (agents see real JSON inside UDT
    /// parameters too).
    #[tokio::test]
    async fn udt_def_reparses_and_pins_body() {
        let rig = TagsRig::with(Vec::new()).route(
            present(),
            serde_json::json!({"definition": {
                "name": "Motor",
                "tagType": "UdtType",
                "parameters": {"speed": {"defaultValue": "{\"dataType\": \"Float8\", \"value\": 0.0}"}},
                "tags": [{"name": "Run", "tagType": "AtomicTag", "value": "{\"dataType\": \"Boolean\", \"value\": true}"}]
            }}),
        );
        let result = tags_udt_def(&rig, "ign-cli", "default", "Motor")
            .await
            .expect("udt def parses");
        assert_eq!(result.name, "Motor");
        assert_eq!(
            result.definition["parameters"]["speed"]["defaultValue"],
            serde_json::json!({"dataType": "Float8", "value": 0.0}),
            "parameter defaultValues are re-parsed"
        );
        assert_eq!(
            result.definition["tags"][0]["value"],
            serde_json::json!({"dataType": "Boolean", "value": true}),
            "nested child values are re-parsed"
        );
        let calls = rig.calls.lock().unwrap();
        assert_eq!(
            calls[0],
            serde_json::json!({"action": "getUDTDefinition", "provider": "default", "name": "Motor"})
        );
    }

    /// The default export file name: first path's last segment,
    /// provider brackets stripped, sanitized.
    #[test]
    fn default_export_file_name_derivations() {
        assert_eq!(
            default_export_file_name(&["[p5e2e]".to_string()]),
            "p5e2e.json"
        );
        assert_eq!(
            default_export_file_name(&["[p5e2e]P5".to_string()]),
            "P5.json"
        );
        assert_eq!(
            default_export_file_name(&["[default]Area/Motor 1".to_string()]),
            "Motor_1.json"
        );
        assert_eq!(default_export_file_name(&[]), "tags.json");
    }

    /// export PARSES the JSON-string payload (never opaque) and
    /// writes it PRETTY to the out file; the result carries the file
    /// + the top-level subtree count.
    #[tokio::test]
    async fn export_parses_payload_and_writes_pretty_file() {
        let payload = serde_json::json!([
            {"name": "P5", "tagType": "Folder", "tags": [{"name": "T1", "tagType": "AtomicTag", "value": "{\"dataType\": \"Int4\", \"value\": 123}"}]}
        ]);
        let rig = TagsRig::with(Vec::new()).route(
            present(),
            serde_json::json!({"payload": serde_json::to_string(&payload).expect("serializes")}),
        );
        let dir = tempfile::tempdir().expect("tempdir");
        let out = dir.path().join("p5.json");
        let result: TagsExportResult =
            tags_export(&rig, "ign-cli", &["[p5e2e]P5".to_string()], Some(&out))
                .await
                .expect("export writes");
        assert_eq!(
            result.file.as_deref(),
            Some(out.display().to_string().as_str())
        );
        assert!(!result.stdout);
        assert_eq!(result.tag_count, 1, "one top-level subtree");
        let written = std::fs::read_to_string(&out).expect("file written");
        let reparsed: serde_json::Value =
            serde_json::from_str(written.trim()).expect("pretty JSON");
        assert_eq!(
            reparsed, payload,
            "the payload round-trips byte-faithfully (verbatim — import fidelity)"
        );
        assert!(written.contains("\n  "), "pretty-printed");
        let calls = rig.calls.lock().unwrap();
        assert_eq!(
            calls[0],
            serde_json::json!({"action": "exportTags", "paths": ["[p5e2e]P5"]})
        );
    }

    /// Stdout mode (`-o -`): the result carries the pretty payload
    /// (render prints it raw); no file is touched.
    #[tokio::test]
    async fn export_stdout_mode_carries_the_payload() {
        let rig = TagsRig::with(Vec::new()).route(
            present(),
            serde_json::json!({"payload": "[{\"name\": \"T1\", \"tagType\": \"AtomicTag\"}]"}),
        );
        let result = tags_export(&rig, "ign-cli", &["[default]T1".to_string()], None)
            .await
            .expect("export stdout mode");
        assert!(result.stdout);
        assert_eq!(result.file, None);
        let payload = result.payload.expect("payload rides the result");
        assert!(payload.starts_with("[\n  {"), "pretty-printed: {payload}");
        assert_eq!(result.tag_count, 1);
    }

    /// A scalar payload (neither object nor array) is an
    /// internal-class honesty error.
    #[tokio::test]
    async fn export_refuses_scalar_payloads() {
        let rig = TagsRig::with(Vec::new()).route(present(), serde_json::json!({"payload": "42"}));
        let err = tags_export(&rig, "ign-cli", &["[default]T1".to_string()], None)
            .await
            .expect_err("scalar payload refuses");
        assert_eq!(err.code(), "internal");
    }

    /// THE LIVE payload shapes (05-06 live-run discovery): one path
    /// → a SINGLE subtree OBJECT; several → the `{"tags": [...]}`
    /// wrapper. Both normalize to the list-of-subtrees interchange
    /// (what the export file carries + import consumes).
    #[tokio::test]
    async fn export_normalizes_the_live_payload_shapes() {
        // Single subtree object.
        let rig = TagsRig::with(Vec::new()).route(
            present(),
            serde_json::json!({"payload": "{\"name\": \"T1\", \"tagType\": \"AtomicTag\", \"value\": 123}"}),
        );
        let result = tags_export(&rig, "ign-cli", &["[default]T1".to_string()], None)
            .await
            .expect("single-subtree payload normalizes");
        assert_eq!(result.tag_count, 1);
        let payload: serde_json::Value =
            serde_json::from_str(result.payload.expect("stdout payload").as_str())
                .expect("normalizes to a list");
        assert_eq!(
            payload,
            serde_json::json!([{"name": "T1", "tagType": "AtomicTag", "value": 123}]),
            "the single subtree rides as a one-element list"
        );

        // Multi-path wrapper {"tags": [...]}.
        let rig = TagsRig::with(Vec::new()).route(
            present(),
            serde_json::json!({"payload": "{\"tags\": [{\"name\": \"T1\", \"tagType\": \"AtomicTag\"}, {\"name\": \"_types_\", \"tagType\": \"Folder\"}]}"}),
        );
        let result = tags_export(&rig, "ign-cli", &["[default]T1".to_string()], None)
            .await
            .expect("wrapper payload normalizes");
        assert_eq!(result.tag_count, 2, "the wrapper's children count");

        // A bare array rides through unchanged (defensive arm).
        let rig = TagsRig::with(Vec::new()).route(
            present(),
            serde_json::json!({"payload": "[{\"name\": \"T1\", \"tagType\": \"AtomicTag\"}]"}),
        );
        let result = tags_export(&rig, "ign-cli", &["[default]T1".to_string()], None)
            .await
            .expect("array payload passes");
        assert_eq!(result.tag_count, 1);
    }

    /// THE zero-write collision proof (the 03-02 pattern on the
    /// route seam): abort-policy import browses the target, finds an
    /// existing top-level name, and refuses `tag_collision` (exit 6,
    /// hint names the overwrite policy) — the ONLY route call is the
    /// browse read; configure NEVER ran.
    #[tokio::test]
    async fn import_abort_refuses_collision_with_zero_writes() {
        let rig = TagsRig::with(Vec::new())
            .route(
                present(),
                serde_json::json!({"results": [
                    {"fullPath": "[p5import]T1", "name": "T1", "tagType": "AtomicTag", "hasChildren": false, "dataType": "Int4"}
                ]}),
            )
            .responses(vec![
                serde_json::json!({"results": [
                    {"fullPath": "[p5import]T1", "name": "T1", "tagType": "AtomicTag", "hasChildren": false, "dataType": "Int4"}
                ]}),
                serde_json::json!({"results": ["Good"]}),
            ]);
        let err = tags_import(
            &rig,
            "ign-cli",
            "p5import",
            serde_json::json!([{"name": "T1", "tagType": "AtomicTag"}]),
            CollisionPolicy::Abort,
        )
        .await
        .expect_err("collision refuses");
        assert_eq!(err.code(), "tag_collision");
        assert_eq!(err.exit_code(), 6);
        assert!(
            err.hint().unwrap().contains("--collision-policy overwrite"),
            "hint names the fix: {err}"
        );
        assert!(
            err.to_string().contains("T1"),
            "the colliding name rides the message"
        );
        let calls = rig.calls.lock().unwrap();
        assert_eq!(calls.len(), 1, "only the browse read ran");
        assert_eq!(calls[0]["action"], "browse");
        assert_eq!(calls[0]["path"], "[p5import]");
    }

    /// Abort with NO collision: browse (empty) → configure 'a' with
    /// `[provider]` as basePath and the payload VERBATIM as tags.
    #[tokio::test]
    async fn import_abort_clean_configures_with_policy_a() {
        let payload = serde_json::json!([
            {"name": "P5", "tagType": "Folder", "tags": [{"name": "T1", "tagType": "AtomicTag"}]}
        ]);
        let rig = TagsRig::with(Vec::new())
            .route(present(), serde_json::json!({"results": ["Good"]}))
            .responses(vec![
                serde_json::json!({"results": []}),
                serde_json::json!({"results": ["Good"]}),
            ]);
        let result: TagsImportResult = tags_import(
            &rig,
            "ign-cli",
            "p5import",
            payload.clone(),
            CollisionPolicy::Abort,
        )
        .await
        .expect("clean abort imports");
        assert_eq!(result.imported, 1);
        assert_eq!(result.collision_policy, "abort");
        let calls = rig.calls.lock().unwrap();
        assert_eq!(calls.len(), 2, "browse pre-check then configure");
        assert_eq!(calls[0]["action"], "browse");
        assert_eq!(
            calls[1],
            serde_json::json!({
                "action": "configure",
                "basePath": "[p5import]",
                "tags": payload,
                "collisionPolicy": "a"
            }),
            "the configure body is exactly basePath+tags+policy"
        );
    }

    /// Overwrite: NO pre-check (server authority) — configure 'o' is
    /// the ONLY route call; the CLI guards it with --yes upstream.
    #[tokio::test]
    async fn import_overwrite_skips_precheck_and_configures_with_policy_o() {
        let rig =
            TagsRig::with(Vec::new()).route(present(), serde_json::json!({"results": ["Good"]}));
        let result = tags_import(
            &rig,
            "ign-cli",
            "p5import",
            serde_json::json!([{"name": "T1", "tagType": "AtomicTag"}]),
            CollisionPolicy::Overwrite,
        )
        .await
        .expect("overwrite imports");
        assert_eq!(result.collision_policy, "overwrite");
        let calls = rig.calls.lock().unwrap();
        assert_eq!(calls.len(), 1, "no browse pre-check ran");
        assert_eq!(calls[0]["action"], "configure");
        assert_eq!(calls[0]["collisionPolicy"], "o");
    }

    /// A non-array payload refuses invalid_input with ZERO route
    /// calls (usage class, pre-wire).
    #[tokio::test]
    async fn import_refuses_non_array_payloads() {
        let rig = TagsRig::with(Vec::new()).route(present(), serde_json::json!({}));
        let err = tags_import(
            &rig,
            "ign-cli",
            "p5import",
            serde_json::json!({"name": "T1"}),
            CollisionPolicy::Abort,
        )
        .await
        .expect_err("non-array payload refuses");
        assert_eq!(err.code(), "invalid_input");
        assert!(rig.calls.lock().unwrap().is_empty());
    }

    /// THE provider-shaped pre-check pin (live-proven 05-06): a
    /// subtree with an EMPTY name is the export wrapper — its
    /// CHILDREN land at the target, so the collision pre-check (and
    /// the imported count) key on the children's names.
    #[test]
    fn effective_top_level_names_derivation() {
        let named = serde_json::json!({"name": "T1", "tagType": "AtomicTag"});
        assert_eq!(
            super::effective_top_level_names(&named),
            vec!["T1".to_string()]
        );
        let provider_wrapper = serde_json::json!({
            "name": "", "tagType": "Provider",
            "tags": [
                {"name": "T1", "tagType": "AtomicTag"},
                {"name": "_types_", "tagType": "Folder"}
            ]
        });
        assert_eq!(
            super::effective_top_level_names(&provider_wrapper),
            vec!["T1".to_string(), "_types_".to_string()],
            "the wrapper's children land at the target"
        );
    }

    /// The provider-shaped collision refusal: importing the exported
    /// provider wrapper into a provider whose top level already has
    /// the child's name refuses `tag_collision` BEFORE any write.
    #[tokio::test]
    async fn import_provider_shaped_payload_collides_on_children() {
        let rig = TagsRig::with(Vec::new())
            .route(
                present(),
                serde_json::json!({"results": [
                    {"fullPath": "[p5import]T1", "name": "T1", "tagType": "AtomicTag", "hasChildren": false, "dataType": "Int4"}
                ]}),
            )
            .responses(vec![
                serde_json::json!({"results": [
                    {"fullPath": "[p5import]T1", "name": "T1", "tagType": "AtomicTag", "hasChildren": false, "dataType": "Int4"}
                ]}),
                serde_json::json!({"results": ["Good"]}),
            ]);
        let err = tags_import(
            &rig,
            "ign-cli",
            "p5import",
            serde_json::json!([{"name": "", "tagType": "Provider", "tags": [{"name": "T1", "tagType": "AtomicTag"}]}]),
            CollisionPolicy::Abort,
        )
        .await
        .expect_err("the child collision refuses");
        assert_eq!(err.code(), "tag_collision");
        assert!(
            err.to_string().contains("T1"),
            "the CHILD's name rides the message: {err}"
        );
        let calls = rig.calls.lock().unwrap();
        assert_eq!(calls.len(), 1, "only the browse pre-check ran");
    }

    /// THE round-trip unit (the research-proven loop): export one
    /// provider's subtree, import the payload into a DIFFERENT
    /// provider — the configure body's tags match the parsed export
    /// payload exactly (values intact, verbatim interchange).
    #[tokio::test]
    async fn export_import_round_trip_shapes_match() {
        let payload = serde_json::json!([
            {"name": "T1", "tagType": "AtomicTag", "value": "{\"dataType\": \"Int4\", \"value\": 123}"}
        ]);
        let rig = TagsRig::with(Vec::new())
            .route(present(), serde_json::json!({"results": ["Good"]}))
            .responses(vec![
                serde_json::json!({"payload": serde_json::to_string(&payload).expect("serializes")}),
                serde_json::json!({"results": []}),
                serde_json::json!({"results": ["Good"]}),
            ]);
        // Export (stdout mode — the payload rides the result).
        let exported = tags_export(&rig, "ign-cli", &["[p5e2e]".to_string()], None)
            .await
            .expect("export parses");
        let text = exported.payload.expect("stdout payload");
        let parsed: serde_json::Value = serde_json::from_str(&text).expect("payload parses");
        // Import into a different provider (abort — clean target).
        let result = tags_import(&rig, "ign-cli", "p5import", parsed, CollisionPolicy::Abort)
            .await
            .expect("round-trip imports");
        assert_eq!(result.imported, 1);
        let calls = rig.calls.lock().unwrap();
        assert_eq!(calls.len(), 3, "export, browse pre-check, configure");
        assert_eq!(calls[0]["action"], "exportTags");
        assert_eq!(calls[2]["action"], "configure");
        assert_eq!(calls[2]["basePath"], "[p5import]");
        assert_eq!(
            calls[2]["tags"], payload,
            "the configure tags are the parsed export payload VERBATIM"
        );
    }

    // ---- alarms active/history/ack (05-06, TAGS-07) ----

    use super::{
        AlarmRow, TagsAlarmsAckResult, tags_alarms_ack, tags_alarms_active, tags_alarms_history,
    };

    /// Active alarms: rows map under unit-explicit keys (eventId →
    /// event_id, state verbatim, name Option), and the body carries
    /// ONLY the present filters — a bare call is exactly
    /// `{action: active}`.
    #[tokio::test]
    async fn alarms_active_maps_rows_and_pins_filter_kwargs() {
        let rig = TagsRig::with(Vec::new()).route(
            present(),
            serde_json::json!({"results": [
                {"eventId": "e-1", "source": "prov:tagprov:/T1/HighLimit", "state": "Active, Unacknowledged", "priority": "High", "name": "HighLimit"},
                {"eventId": "e-2", "source": "prov:tagprov:/T2/LowLimit", "state": "Active, Unacknowledged", "priority": "Medium", "name": null}
            ], "count": 2}),
        );
        let result = tags_alarms_active(
            &rig,
            "ign-cli",
            Some("prov:tagprov"),
            Some("High"),
            Some("Active, Unacknowledged"),
        )
        .await
        .expect("active parses");
        assert_eq!(result.count, 2);
        assert_eq!(
            result.alarms[0],
            AlarmRow {
                event_id: "e-1".into(),
                source: "prov:tagprov:/T1/HighLimit".into(),
                state: "Active, Unacknowledged".into(),
                priority: "High".into(),
                name: Some("HighLimit".into()),
            }
        );
        assert_eq!(result.alarms[1].name, None, "null name degrades to None");
        let first_call = {
            let calls = rig.calls.lock().unwrap();
            calls[0].clone()
        };
        assert_eq!(
            first_call,
            serde_json::json!({
                "action": "active",
                "source": "prov:tagprov",
                "priority": "High",
                "state": "Active, Unacknowledged"
            }),
            "only PRESENT filters ride the body (kwargs passthrough)"
        );

        // Bare call: the body is exactly the action — no filter keys.
        let rig = TagsRig::with(Vec::new()).route(present(), serde_json::json!({"results": []}));
        let result = tags_alarms_active(&rig, "ign-cli", None, None, None)
            .await
            .expect("bare active parses");
        assert_eq!(result.count, 0);
        let calls = rig.calls.lock().unwrap();
        assert_eq!(
            calls[0],
            serde_json::json!({"action": "active"}),
            "no filters, no filter keys"
        );
    }

    /// History success: rows ride VERBATIM, columns derive from the
    /// first row's keys (empty on an empty journal), and the body
    /// carries epoch-ms start/end.
    #[tokio::test]
    async fn alarms_history_passes_rows_verbatim_and_derives_columns() {
        let rig = TagsRig::with(Vec::new()).route(
            present(),
            serde_json::json!({"results": [
                {"eventId": "e-1", "source": "prov:x", "state": "Active, Unacknowledged", "priority": "High", "name": "HighLimit", "eventData": null}
            ], "count": 1}),
        );
        let result = tags_alarms_history(&rig, "ign-cli", 1_000, 2_000)
            .await
            .expect("history parses");
        assert_eq!(result.count, 1);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0]["eventId"], "e-1", "rows verbatim");
        assert!(
            result.columns.contains(&"eventId".to_string())
                && result.columns.contains(&"eventData".to_string()),
            "columns derive from the first row: {:?}",
            result.columns
        );
        let history_call = {
            let calls = rig.calls.lock().unwrap();
            calls[0].clone()
        };
        assert_eq!(
            history_call,
            serde_json::json!({"action": "history", "startDateMs": 1000, "endDateMs": 2000})
        );

        // Empty journal: empty columns, empty rows — still a success.
        let rig = TagsRig::with(Vec::new()).route(present(), serde_json::json!({"results": []}));
        let result = tags_alarms_history(&rig, "ign-cli", 1_000, 2_000)
            .await
            .expect("empty history parses");
        assert!(result.columns.is_empty());
        assert_eq!(result.count, 0);
    }

    /// THE ack body pin + the remainder-honest computation: the
    /// 3-arg String[]/note/username form rides the body; the route's
    /// return IS the unacknowledged remainder; acknowledged = 2 − 1
    /// client-side. Full-UUID ids pass through with NO active lookup
    /// (calls[0] is the ack itself).
    #[tokio::test]
    async fn alarms_ack_pins_three_arg_body_and_computes_remainder() {
        let rig = TagsRig::with(Vec::new()).route(
            present(),
            serde_json::json!({"unacknowledged": ["22222222-2222-2222-2222-222222222222"]}),
        );
        let result: TagsAlarmsAckResult = tags_alarms_ack(
            &rig,
            "ign-cli",
            &[
                "11111111-1111-1111-1111-111111111111".to_string(),
                "22222222-2222-2222-2222-222222222222".to_string(),
            ],
            "handled by on-call",
            "op",
        )
        .await
        .expect("ack parses");
        assert_eq!(result.acknowledged, 1, "requested 2, remainder 1");
        assert_eq!(
            result.unacknowledged,
            vec!["22222222-2222-2222-2222-222222222222".to_string()]
        );
        let ack_call = {
            let calls = rig.calls.lock().unwrap();
            assert_eq!(calls.len(), 1, "full ids: no active lookup fires");
            calls[0].clone()
        };
        assert_eq!(
            ack_call,
            serde_json::json!({
                "action": "acknowledge",
                "eventIds": [
                    "11111111-1111-1111-1111-111111111111",
                    "22222222-2222-2222-2222-222222222222"
                ],
                "note": "handled by on-call",
                "username": "op"
            }),
            "the ack body is the 3-arg form: string ids + note + username"
        );

        // Full acknowledgment: empty remainder, all acknowledged.
        let rig =
            TagsRig::with(Vec::new()).route(present(), serde_json::json!({"unacknowledged": []}));
        let result = tags_alarms_ack(
            &rig,
            "ign-cli",
            &["11111111-1111-1111-1111-111111111111".to_string()],
            "",
            "op",
        )
        .await
        .expect("clean ack parses");
        assert_eq!(result.acknowledged, 1);
        assert!(result.unacknowledged.is_empty());

        // A missing `unacknowledged` key is an internal-class honesty
        // error (never silently defaulted).
        let rig = TagsRig::with(Vec::new()).route(present(), serde_json::json!({}));
        let err = tags_alarms_ack(
            &rig,
            "ign-cli",
            &["11111111-1111-1111-1111-111111111111".to_string()],
            "",
            "op",
        )
        .await
        .expect_err("shape violation refuses");
        assert_eq!(err.code(), "internal");
    }

    /// THE prefix-expansion pin: a SHORT id triggers one `active`
    /// lookup, and the acknowledge body carries the EXPANDED full
    /// UUID (request-level proof — what the table prints, ack
    /// accepts). Mixed short/full ids expand independently.
    #[tokio::test]
    async fn alarms_ack_expands_short_id_prefixes_against_the_active_list() {
        let rig = TagsRig::with(Vec::new())
            .route(present(), serde_json::json!({"unacknowledged": []}))
            .responses(vec![
                serde_json::json!({"results": [
                    {"eventId": "3f2504e0-4f89-11d3-9a0c-0305e82c3301", "source": "prov:x:/T1", "state": "Active, Unacknowledged", "priority": "High", "name": "T1"},
                    {"eventId": "9b9e9e9e-1111-2222-3333-444455556666", "source": "prov:x:/T2", "state": "Active, Unacknowledged", "priority": "Medium", "name": null}
                ], "count": 2}),
                serde_json::json!({"unacknowledged": []}),
            ]);
        let result = tags_alarms_ack(
            &rig,
            "ign-cli",
            &[
                "3f2504e0".to_string(),
                "9b9e9e9e-1111-2222-3333-444455556666".to_string(),
            ],
            "",
            "op",
        )
        .await
        .expect("mixed short/full ack parses");
        assert_eq!(result.acknowledged, 2);
        let calls = rig.calls.lock().unwrap();
        assert_eq!(calls.len(), 2, "active lookup, then acknowledge");
        assert_eq!(calls[0]["action"], "active");
        assert_eq!(
            calls[1]["eventIds"],
            serde_json::json!([
                "3f2504e0-4f89-11d3-9a0c-0305e82c3301",
                "9b9e9e9e-1111-2222-3333-444455556666"
            ]),
            "the wire body carries the EXPANDED uuid, not the prefix"
        );
    }

    /// Ambiguous prefix: two active alarms share the prefix →
    /// `invalid_input` (exit 2) listing the matching FULL ids.
    #[tokio::test]
    async fn alarms_ack_refuses_ambiguous_prefixes_naming_candidates() {
        let rig = TagsRig::with(Vec::new())
            .route(present(), serde_json::json!({}))
            .responses(vec![serde_json::json!({"results": [
                {"eventId": "aaaaaaaa-1111-1111-1111-111111111111", "source": "prov:x:/T1", "state": "Active, Unacknowledged", "priority": "High", "name": null},
                {"eventId": "aaaaaaaa-2222-2222-2222-222222222222", "source": "prov:x:/T2", "state": "Active, Unacknowledged", "priority": "High", "name": null}
            ], "count": 2})]);
        let err = tags_alarms_ack(&rig, "ign-cli", &["aaaaaaaa".to_string()], "", "op")
            .await
            .expect_err("ambiguous prefix refuses");
        assert_eq!(err.code(), "invalid_input");
        assert_eq!(err.exit_code(), 2);
        let message = err.to_string();
        assert!(
            message.contains("aaaaaaaa-1111-1111-1111-111111111111")
                && message.contains("aaaaaaaa-2222-2222-2222-222222222222"),
            "the refusal names BOTH candidates: {message}"
        );
        let calls = rig.calls.lock().unwrap();
        assert_eq!(calls.len(), 1, "the lookup fired, the ack never did");
        assert_eq!(calls[0]["action"], "active");
    }

    /// Unknown prefix: zero active alarms match → `invalid_input`
    /// (exit 2) naming the miss + the already-acknowledged hint.
    #[tokio::test]
    async fn alarms_ack_refuses_unknown_prefixes_naming_the_miss() {
        let rig = TagsRig::with(Vec::new())
            .route(present(), serde_json::json!({}))
            .responses(vec![serde_json::json!({"results": [
                {"eventId": "3f2504e0-4f89-11d3-9a0c-0305e82c3301", "source": "prov:x:/T1", "state": "Active, Unacknowledged", "priority": "High", "name": null}
            ], "count": 1})]);
        let err = tags_alarms_ack(&rig, "ign-cli", &["deadbeef".to_string()], "", "op")
            .await
            .expect_err("unknown prefix refuses");
        assert_eq!(err.code(), "invalid_input");
        assert_eq!(err.exit_code(), 2);
        let message = err.to_string();
        assert!(
            message.contains("deadbeef") && message.contains("tags alarms active --json"),
            "the refusal names the miss + the full-id source: {message}"
        );
    }

    /// Precondition refusal inheritance: absent routes refuse
    /// BEFORE any alarms route call (zero route calls).
    #[tokio::test]
    async fn alarms_refuse_when_routes_absent() {
        let rig = TagsRig::with(Vec::new()).route(RouteProbe::Absent, serde_json::json!({}));
        let err = tags_alarms_active(&rig, "ign-cli", None, None, None)
            .await
            .expect_err("absent routes refuse");
        assert_eq!(err.code(), "routes_not_deployed");
        assert_eq!(err.exit_code(), 6);
        assert!(rig.calls.lock().unwrap().is_empty());
    }

    // ---- tag history query (05-06, TAGS-08) ----

    use super::{TagsHistoryQueryResult, parse_time_ms, tags_history_query};

    /// THE query body pin + t_stamp preservation: `{action, paths,
    /// startDateMs, endDateMs}` exactly when no optionals — the
    /// dataset comes back VERBATIM with `t_stamp` in place (never
    /// renamed) and null cells passing through on a historian-less
    /// rig (the structural default).
    #[tokio::test]
    async fn history_query_pins_body_and_preserves_t_stamp() {
        let rig = TagsRig::with(Vec::new()).route(
            present(),
            serde_json::json!({
                "columns": ["t_stamp", "[default]T1"],
                "rows": [["Mon Aug 24 00:00:00 UTC 2026", null]],
                "rowCount": 1
            }),
        );
        let result: TagsHistoryQueryResult = tags_history_query(
            &rig,
            "ign-cli",
            &["[default]T1".to_string()],
            1_000,
            2_000,
            None,
            None,
        )
        .await
        .expect("query parses");
        assert_eq!(
            result.columns,
            vec!["t_stamp".to_string(), "[default]T1".to_string()]
        );
        assert_eq!(result.row_count, 1);
        assert_eq!(result.rows[0][0], "Mon Aug 24 00:00:00 UTC 2026");
        assert_eq!(
            result.rows[0][1],
            serde_json::Value::Null,
            "null cells verbatim"
        );
        let calls = rig.calls.lock().unwrap();
        assert_eq!(
            calls[0],
            serde_json::json!({
                "action": "query",
                "paths": ["[default]T1"],
                "startDateMs": 1000,
                "endDateMs": 2000
            }),
            "no optionals, no optional keys"
        );
    }

    /// Optionals ride ONLY when present: `returnSize` +
    /// `aggregationMode` (the route's own kwarg names).
    #[tokio::test]
    async fn history_query_optionals_ride_only_when_present() {
        let rig = TagsRig::with(Vec::new()).route(
            present(),
            serde_json::json!({"columns": [], "rows": [], "rowCount": 0}),
        );
        tags_history_query(
            &rig,
            "ign-cli",
            &["[default]T1".to_string()],
            0,
            1,
            Some(100),
            Some("average"),
        )
        .await
        .expect("query with optionals parses");
        let calls = rig.calls.lock().unwrap();
        assert_eq!(calls[0]["returnSize"], 100);
        assert_eq!(calls[0]["aggregationMode"], "average");
    }

    /// A missing `columns`/`rows` key is an internal-class honesty
    /// error (never silently defaulted).
    #[tokio::test]
    async fn history_query_refuses_shape_violations() {
        let rig = TagsRig::with(Vec::new()).route(present(), serde_json::json!({"rowCount": 0}));
        let err = tags_history_query(
            &rig,
            "ign-cli",
            &["[default]T1".to_string()],
            0,
            1,
            None,
            None,
        )
        .await
        .expect_err("shape violation refuses");
        assert_eq!(err.code(), "internal");
    }

    /// THE time-arg parser: epoch-ms passthrough, RFC3339 with Z /
    /// offsets / fractional seconds, leap-day math — each pinned
    /// against independently computed epoch values.
    #[test]
    fn parse_time_ms_accepts_epoch_and_rfc3339() {
        assert_eq!(
            parse_time_ms("1787659200000").expect("epoch ms"),
            1_787_659_200_000
        );
        assert_eq!(
            parse_time_ms("1970-01-01T00:00:00Z").expect("epoch"),
            0,
            "the origin"
        );
        assert_eq!(
            parse_time_ms("2026-08-25T12:00:00Z").expect("z"),
            1_787_659_200_000
        );
        assert_eq!(
            parse_time_ms("2026-08-25T14:00:00+02:00").expect("east offset"),
            1_787_659_200_000,
            "+02:00 lands on the same instant"
        );
        assert_eq!(
            parse_time_ms("2026-08-25T10:00:00-04:00").expect("west offset"),
            1_787_666_400_000
        );
        assert_eq!(
            parse_time_ms("2026-08-25T12:00:00.123Z").expect("millis"),
            1_787_659_200_123
        );
        assert_eq!(
            parse_time_ms("2026-01-01T00:00:00.5Z").expect("sub-second padding"),
            1_767_225_600_500,
            ".5 → 500 ms"
        );
        assert_eq!(
            parse_time_ms("2024-02-29T12:00:00Z").expect("leap day"),
            1_709_208_000_000
        );
        // Tolerated forms: space separator, lowercase.
        assert_eq!(
            parse_time_ms("2026-08-25 12:00:00Z").expect("space separator"),
            1_787_659_200_000
        );
        assert_eq!(
            parse_time_ms("2026-08-25t12:00:00z").expect("lowercase"),
            1_787_659_200_000
        );
    }

    /// Unparseable times refuse invalid_input with the usage-class
    /// shape (what the caller must fix).
    #[test]
    fn parse_time_ms_refuses_garbage() {
        for bad in [
            "",
            "yesterday",
            "2026-08-25",
            "2026-8-25T12:00:00Z",
            "2026-13-01T00:00:00Z",
            "2026-08-25T25:00:00Z",
            "2026-08-25T12:00:00Zulu",
        ] {
            let err = parse_time_ms(bad).expect_err("garbage refuses");
            assert_eq!(err.code(), "invalid_input", "{bad}: {err}");
            assert_eq!(err.exit_code(), 2);
        }
    }

    // ---- offline export browsing (07-04, INTR-03) ----

    use super::{browse_rows_from_export, decode_fs_name};

    /// The git-module fs-name encoding round-trips the reserved set;
    /// invalid escapes ride verbatim.
    #[test]
    fn fs_name_decoding() {
        assert_eq!(
            decode_fs_name("Tag%2F1"),
            "Tag/1",
            "%2F is the encoded slash"
        );
        assert_eq!(decode_fs_name("a%3Ab.json"), "a:b.json", "%3A is the colon");
        assert_eq!(
            decode_fs_name("100%25"),
            "100%",
            "%25 is the escaped percent"
        );
        assert_eq!(decode_fs_name("plain_name"), "plain_name", "plain rides");
        assert_eq!(
            decode_fs_name("bad%zz"),
            "bad%zz",
            "invalid escapes ride verbatim"
        );
        assert_eq!(
            decode_fs_name("trail%2"),
            "trail%2",
            "truncated escapes ride verbatim"
        );
    }

    /// (a) THE git-module individual-file layout: provider folders,
    /// mirroring directory hierarchy, `_types_/` definitions at the
    /// provider root, an encoded filename, a dot-entry, and
    /// `.tag-config.json` + `System` excluded. Detected via the
    /// project's `tags/` root AND directly (dir itself = providers
    /// parent).
    #[test]
    fn from_export_walks_the_git_module_layout() {
        let dir = tempfile::tempdir().expect("tempdir");
        let tags = dir.path().join("tags");
        let prov = tags.join("default");
        std::fs::create_dir_all(prov.join("Area1")).expect("dirs");
        std::fs::create_dir_all(prov.join("_types_")).expect("types dir");
        // The per-project config + a dot-entry + System provider (all
        // skipped).
        std::fs::write(tags.join(".tag-config.json"), b"{}").expect("config");
        std::fs::write(prov.join(".gitkeep"), b"").expect("dot entry");
        std::fs::create_dir_all(tags.join("System")).expect("system dir");
        std::fs::write(tags.join("System").join("x.json"), b"{}").expect("system tag");
        // Leaf tags: name field STRIPPED in this format — the filename
        // is the name.
        std::fs::write(
            prov.join("T1.json"),
            br#"{"tagType":"AtomicTag","value":{"value":4}}"#,
        )
        .expect("t1");
        std::fs::write(prov.join("Tag%2F1.json"), br#"{"tagType":"AtomicTag"}"#)
            .expect("encoded name");
        std::fs::write(
            prov.join("Area1").join("Deep.json"),
            br#"{"tagType":"AtomicTag"}"#,
        )
        .expect("deep");
        std::fs::write(prov.join("_types_").join("Motor.json"), br#"{"tags":[]}"#).expect("udt");

        for base in [dir.path(), tags.as_path()] {
            let result = browse_rows_from_export(base, true, None).expect("the layout walks");
            assert_eq!(result.source, "export");
            let paths: Vec<&str> = result.entries.iter().map(|r| r.path.as_str()).collect();
            // Deterministic DFS in name-sorted order (ASCII: Area1 <
            // T1 < Tag%2F1 < _types_ — capital letters sort before
            // the underscore).
            assert_eq!(
                paths,
                vec![
                    "[default]",
                    "[default]Area1",
                    "[default]Area1/Deep",
                    "[default]T1",
                    "[default]Tag/1",
                    "[default]_types_",
                    "[default]_types_/Motor",
                ],
                "sorted walk; _types_/encoded/dot/System handled ({:?})",
                result.entries
            );
            let types_def = &result.entries[6];
            assert_eq!(types_def.tag_type, "UdtType", "the _types_ default");
            assert!(!types_def.has_children, "empty tags array = no children");
            let folder = &result.entries[1];
            assert_eq!(folder.tag_type, "Folder");
            assert!(folder.has_children);
        }
    }

    /// (b) The legacy single-file layout: `<provider>.json` carrying
    /// the whole tree (Provider-typed wrapper swallows; children at
    /// the stem's root).
    #[test]
    fn from_export_walks_the_legacy_single_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let tags = dir.path().join("tags");
        std::fs::create_dir_all(&tags).expect("tags dir");
        std::fs::write(
            tags.join("default.json"),
            br#"{"name":"default","tagType":"Provider","tags":[
                {"name":"T1","tagType":"AtomicTag","dataType":"Int4"},
                {"name":"Folder1","tagType":"Folder","tags":[
                    {"name":"Inner","tagType":"AtomicTag"}
                ]}
            ]}"#,
        )
        .expect("legacy tree");

        let result = browse_rows_from_export(dir.path(), true, None).expect("walks");
        let paths: Vec<&str> = result.entries.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                "[default]",
                "[default]T1",
                "[default]Folder1",
                "[default]Folder1/Inner",
            ]
        );
        assert_eq!(result.entries[1].data_type.as_deref(), Some("Int4"));
    }

    /// (c) The CLI's own interchange (a list of subtrees — what
    /// `tags export -o FILE` writes) browsed from the FILE; (d) the
    /// filter applies client-side; the provider-shaped wrapper lands
    /// its children at the stem's root.
    #[test]
    fn from_export_walks_the_cli_interchange_file_and_filters() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("default.json");
        std::fs::write(
            &file,
            br#"[{"name":"","tagType":"Provider","tags":[
                {"name":"T1","tagType":"AtomicTag"},
                {"name":"Pump","tagType":"AtomicTag"}
            ]}]"#,
        )
        .expect("interchange");

        let result = browse_rows_from_export(&file, true, None).expect("walks");
        let paths: Vec<&str> = result.entries.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths, vec!["[default]", "[default]T1", "[default]Pump"]);

        // (d) the filter: substring on name+path.
        let filtered = browse_rows_from_export(&file, true, Some("pump"))
            .expect("walks")
            .entries;
        assert_eq!(
            filtered.iter().map(|r| r.path.as_str()).collect::<Vec<_>>(),
            vec!["[default]Pump"]
        );
    }

    /// Nonexistent path / unparseable JSON / a non-export layout are
    /// usage-class refusals (exit 2, offline errors lead).
    #[test]
    fn from_export_error_shapes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = browse_rows_from_export(&dir.path().join("nope"), true, None)
            .expect_err("missing path refuses");
        assert!(matches!(err, CoreError::InvalidInput { .. }), "{err}");
        assert_eq!(err.exit_code(), 2);

        let bad = dir.path().join("bad.json");
        std::fs::write(&bad, b"<<<not json>>>").expect("write");
        let err = browse_rows_from_export(&bad, true, None).expect_err("bad json refuses");
        assert!(matches!(err, CoreError::InvalidInput { .. }), "{err}");

        // A directory with NO provider content is not an export layout.
        let empty = tempfile::tempdir().expect("tempdir");
        let err = browse_rows_from_export(empty.path(), true, None).expect_err("empty dir refuses");
        assert!(matches!(err, CoreError::InvalidInput { .. }), "{err}");
        assert!(err.to_string().contains("not a tag export layout"), "{err}");
    }
}
