//! Project-family capability models (03-01, PROJ-01/02) — the native
//! `/data/api/v1/projects/*` CRUD family: wire-faithful camelCase
//! models, verified path constants/builders, and the ONE per-segment
//! encoder.
//!
//! HIGH-confidence endpoints (03-RESEARCH §Verified Endpoint Catalog:
//! the official 83-api collection + the working ignition-mcp client
//! agree on every path). Item SHAPES stay MEDIUM until live capture
//! (research Open Question 2) — hence the `#[serde(flatten)] extra`
//! passthrough on [`ProjectRecord`] (the 02-02 ModuleInfo pattern:
//! wire-truth corrections stay cheap).
//!
//! Serialization discipline (Pitfall 5): every optional field on the
//! create/modify bodies is `Option` with `skip_serializing_if` —
//! absent means NOT SENT, never an empty-string reference
//! (`"parent": ""` would point at a nonexistent project). Create
//! always sends `name` + `enabled`; the modify body carries NO `name`
//! (the PUT must not rename — rename has its own route) and its
//! `enabled` is itself optional so a single-field `set` never
//! clobbers the flag.
//!
//! Path discipline (Pitfall 6): every `{name}` path segment rides
//! through [`encode_segment`] (percent-encoding, NON_ALPHANUMERIC set
//! — over-encoding is safe and mirrors mcp's `quote(name, safe='')`);
//! a spaced-name recorded-request proof pins it. 03-03's resource
//! paths encode per-segment through this same fn but keep their `/`
//! separators.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// GET path — list every RUNNABLE project (official description).
pub(crate) const PROJECTS_LIST_PATH: &str = "/data/api/v1/projects/list";

/// POST path — create a project (JSON body).
pub(crate) const PROJECTS_CREATE_PATH: &str = "/data/api/v1/projects";

/// POST path — copy a project (JSON body `fromName`/`toName`).
pub(crate) const PROJECTS_COPY_PATH: &str = "/data/api/v1/projects/copy";

/// GET path — one project's full record (`/find/{enc}`).
pub(crate) fn project_find_path(name: &str) -> String {
    format!("/data/api/v1/projects/find/{}", encode_segment(name))
}

/// POST path — rename (`/rename/{enc}` + body `{"name": "<new>"}`).
pub(crate) fn project_rename_path(name: &str) -> String {
    format!("/data/api/v1/projects/rename/{}", encode_segment(name))
}

/// PUT path — modify (`/{enc}`, body WITHOUT `name`). This is the
/// inheritance move: `set --parent` rides this route.
pub(crate) fn project_modify_path(name: &str) -> String {
    format!("/data/api/v1/projects/{}", encode_segment(name))
}

/// DELETE path — delete (`/{enc}` + `confirm=true` QUERY param — both
/// guard layers, Pitfall 8).
pub(crate) fn project_delete_path(name: &str) -> String {
    format!("/data/api/v1/projects/{}", encode_segment(name))
}

/// GET path — export (`/export/{enc}`) — the ZIP body streams back
/// with a `Content-Disposition` filename.
pub(crate) fn project_export_path(name: &str) -> String {
    format!("/data/api/v1/projects/export/{}", encode_segment(name))
}

/// POST path — import (`/import/{enc}` + `overwrite=<bool>` QUERY
/// param; body = the raw ZIP bytes).
pub(crate) fn project_import_path(name: &str) -> String {
    format!("/data/api/v1/projects/import/{}", encode_segment(name))
}

/// Per-request export timeout (Pitfall 3): 120 s, the logs-download
/// precedent — `RequestBuilder::timeout`, never a second client and
/// never a global change.
pub const PROJECT_EXPORT_TIMEOUT: Duration = Duration::from_secs(120);

/// Per-request import timeout (Pitfall 3, the classic default-timeout
/// death): imports are heavy and synchronous (no job IDs — verified),
/// so the upload rides a 300 s budget.
pub const PROJECT_IMPORT_TIMEOUT: Duration = Duration::from_secs(300);

/// Percent-encode ONE path segment with the NON_ALPHANUMERIC set:
/// everything outside `[A-Za-z0-9]` is encoded — over-encoding is SAFE
/// (the server decodes before matching) and mirrors mcp's
/// `quote(name, safe='')`. Project names with spaces/mixed case ride
/// the wire intact (`My Project` → `My%20Project`).
pub(crate) fn encode_segment(segment: &str) -> String {
    percent_encoding::utf8_percent_encode(segment, percent_encoding::NON_ALPHANUMERIC).to_string()
}

/// One item of the list/find endpoints — typed core + passthrough
/// (`defaultDb`/`tagProvider`/`userSource` and every unmodeled key
/// round-trip so client-seam `--json` stays complete as the gateway
/// evolves).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRecord {
    /// Project name (unique key; the path segment everywhere).
    pub name: String,
    /// Display title.
    #[serde(default)]
    pub title: Option<String>,
    /// Long description.
    #[serde(default)]
    pub description: Option<String>,
    /// Whether the project runs.
    #[serde(default)]
    pub enabled: bool,
    /// Parent project name — the inheritance link.
    #[serde(default)]
    pub parent: Option<String>,
    /// Whether THIS project may serve as a parent (verified: real
    /// export `project.json` carries it).
    #[serde(default)]
    pub inheritable: Option<bool>,
    /// Default database connection name.
    #[serde(default)]
    pub default_db: Option<String>,
    /// Tag provider name.
    #[serde(default)]
    pub tag_provider: Option<String>,
    /// User source name.
    #[serde(default)]
    pub user_source: Option<String>,
    /// Unknown keys round-trip (passthrough-shaped `--json`).
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// POST body — create. `name` + `enabled` are ALWAYS sent; every
/// optional rides only when provided (Pitfall 5 — an absent optional
/// is OMITTED, never an empty string referencing a nonexistent
/// resource). A bare create serializes to exactly
/// `{"name":…,"enabled":true}` (wiremock recorded-body pin).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCreate {
    /// Project name.
    pub name: String,
    /// Whether the project starts enabled (always sent).
    pub enabled: bool,
    /// Display title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Long description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Parent project (inheritance).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Whether this project may serve as a parent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inheritable: Option<bool>,
    /// Default database connection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_db: Option<String>,
    /// Tag provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag_provider: Option<String>,
    /// User source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_source: Option<String>,
}

/// PUT body — modify: the create fields MINUS `name` (the PUT must not
/// rename), with `enabled` itself optional so a single-field `set`
/// never clobbers it. Same skip-serializing discipline: a
/// `set --title` body is exactly `{"title":"T"}` (unit-pinned).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectModify {
    /// Whether the project runs (sent only when the caller sets it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Display title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Long description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Parent project — the inheritance move.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Whether this project may serve as a parent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inheritable: Option<bool>,
    /// Default database connection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_db: Option<String>,
    /// Tag provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag_provider: Option<String>,
    /// User source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_source: Option<String>,
}

/// POST body — copy. Official body keys are `fromName`/`toName`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectCopy {
    /// Source project name.
    #[serde(rename = "fromName")]
    pub from_name: String,
    /// Destination name (must not already exist).
    #[serde(rename = "toName")]
    pub to_name: String,
}

/// POST body — rename. The official body key is `name` (the NEW name).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRenameBody {
    /// The new name.
    pub name: String,
}

/// The export download result — the ZIP was STREAMED to disk (never
/// buffered in a `Vec<u8>`, Pitfall 2) and this is what the response
/// metadata said about it. Not serialized into envelopes (the file is
/// the artifact; the command output model lives in the actions layer).
#[derive(Debug, Clone)]
pub struct ExportMeta {
    /// Filename from `Content-Disposition`, when the header carries one
    /// (the actions layer sanitizes it into the default output name;
    /// `None` falls back to `<name>.zip`).
    pub filename: Option<String>,
    /// Bytes written to disk (counted chunk-by-chunk as they streamed).
    pub bytes: u64,
    /// Response `Content-Type` — sniffed, never assumed.
    pub content_type: Option<String>,
}

/// The import result — OPAQUE-SUCCESS (the response body is
/// unverified MEDIUM; the mcp pattern parses JSON when it can and
/// falls back to `{"status":"success"}` otherwise — restart's literal
/// `true` is the same family style).
#[derive(Debug, Clone)]
pub struct ImportOutcome {
    /// The parsed response body when JSON, else the fallback success
    /// object.
    pub response: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::{ProjectCreate, ProjectModify, encode_segment};

    /// Pitfall 6: spaces and mixed case encode per segment with the
    /// NON_ALPHANUMERIC set — over-encoding is safe, under-encoding is
    /// a broken path.
    #[test]
    fn encode_segment_handles_spaces_and_symbols() {
        assert_eq!(encode_segment("My Project"), "My%20Project");
        assert_eq!(encode_segment("plain"), "plain");
        assert_eq!(
            encode_segment("a/b"),
            "a%2Fb",
            "even / encodes — resource paths split first"
        );
    }

    /// Pitfall 5: a bare create body is EXACTLY `{"name":…,"enabled":…}`
    /// — no `"parent":""`, no null keys.
    #[test]
    fn bare_create_serializes_exactly_name_and_enabled() {
        let body = ProjectCreate {
            name: "x".into(),
            enabled: true,
            title: None,
            description: None,
            parent: None,
            inheritable: None,
            default_db: None,
            tag_provider: None,
            user_source: None,
        };
        assert_eq!(
            serde_json::to_value(&body).expect("serializes"),
            serde_json::json!({"name": "x", "enabled": true}),
            "absent optionals are OMITTED, never null/empty strings"
        );
    }

    /// The Task-2 modify discipline lives at the model too: only the
    /// provided field rides the PUT body — and never a `name` key.
    #[test]
    fn modify_serializes_only_provided_fields() {
        let body = ProjectModify {
            enabled: None,
            title: Some("T".into()),
            description: None,
            parent: None,
            inheritable: None,
            default_db: None,
            tag_provider: None,
            user_source: None,
        };
        assert_eq!(
            serde_json::to_value(&body).expect("serializes"),
            serde_json::json!({"title": "T"})
        );
    }
}
