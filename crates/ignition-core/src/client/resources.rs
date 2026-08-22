//! Project-resource capability models (03-03, PROJ-05) — the
//! surgical-edit family: list/get/put/delete ONE resource inside a
//! project.
//!
//! ⚠ MEDIUM-CONFIDENCE FAMILY (03-RESEARCH Open Question 1): these
//! endpoints exist ONLY in ignition-mcp (client + docs + tools) —
//! absent from the official 83-api collection and from public code
//! search. Phase 2 caught that same client inventing paths
//! (`/connections/database`), so every model here is
//! passthrough-heavy and the live-capture gate lives in
//! `crates/ignition-cli/tests/e2e_projects.rs` (the openapi-extract
//! `#[ignore]` test) — wire-truth corrections stay CHEAP until it
//! runs. Do NOT copy mcp's `com.inductiveautomation.ignition/…` doc
//! examples: the observed core-module folder in a real 8.3 export is
//! `ignition/` (03-RESEARCH, verified from
//! `whk-distillery01-ignition-global`). The gateway owns the resource
//! taxonomy — no `{module}/{type}` knowledge is hardcoded here (the
//! Don't-Hand-Roll table).
//!
//! Path discipline (Pitfall 6): the project name rides fully encoded
//! through 03-01's [`crate::client::projects::encode_segment`] (the
//! ONE per-segment encoder); the resource path is encoded PER SEGMENT
//! through the same fn, keeping its `/` separators (mcp's
//! `quote(path, safe='/')`). Over-encoding is SAFE — the server
//! decodes before matching — so `.` rides as `%2E` etc.; a spaced
//! folder pins the exact wire form in tests/resources_contract.rs.
//!
//! [`ResourceContent`] keeps the get result as RAW bytes: the
//! JSON/text/binary decision belongs to the ACTIONS layer
//! (`actions::resources` — the classify/HTML-sniffer discipline
//! inverted), never the client seam.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::client::projects::encode_segment;

/// GET path — list a project's resources (the optional
/// `path=<prefix>` QUERY filter rides only when a prefix is given).
pub(crate) fn resources_list_path(project: &str) -> String {
    format!(
        "/data/api/v1/projects/{}/resources",
        encode_segment(project)
    )
}

/// GET/PUT/DELETE path — one resource: the project name fully
/// encoded, the resource path encoded PER SEGMENT with `/` kept
/// (Pitfall 6; the spaced-path recorded-request pin lives in
/// tests/resources_contract.rs).
pub(crate) fn resource_path(project: &str, path: &str) -> String {
    format!(
        "/data/api/v1/projects/{}/resources/{}",
        encode_segment(project),
        encode_resource_path(path)
    )
}

/// Percent-encode a resource path per segment, preserving `/`:
/// each `split('/')` segment goes through 03-01's `encode_segment`
/// (NON_ALPHANUMERIC — over-encoding is safe, the server decodes
/// before matching; mirrors mcp's `quote(path, safe='/')`).
pub(crate) fn encode_resource_path(path: &str) -> String {
    path.split('/')
        .map(encode_segment)
        .collect::<Vec<_>>()
        .join("/")
}

/// One list item — ⚠ MEDIUM: the item shape is unverified until live
/// capture, so only `path` is typed (and even that `Option` — a bare
/// folder marker without one must still parse); EVERYTHING else
/// round-trips through the passthrough map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceEntry {
    /// The resource path, e.g. `"com.example/views/Dashboard"` (typed
    /// when present; the human renderer prints one per line).
    #[serde(default)]
    pub path: Option<String>,
    /// `scope`, `version`, `restricted`, folder-vs-leaf flags, … —
    /// unknown keys round-trip (wire-truth corrections stay cheap).
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// The get result — RAW bytes plus the response `Content-Type`,
/// exactly as received. The JSON/text/binary classification is the
/// ACTION layer's job (a `data.bin`-class resource must be refused
/// there, never corrupted through the JSON loop — Pitfall 7); the
/// client seam stays faithful to the wire.
#[derive(Debug, Clone)]
pub struct ResourceContent {
    /// The resource body, byte-for-byte.
    pub bytes: Vec<u8>,
    /// Response `Content-Type`, when the header carried one (sniffed,
    /// never assumed).
    pub content_type: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{ResourceEntry, encode_resource_path, resource_path};

    /// Pitfall 6: resource paths encode PER SEGMENT with `/` kept —
    /// spaces and mixed case encode, separators survive, and
    /// over-encoded characters (the `.` in module ids) decode to the
    /// same thing server-side.
    #[test]
    fn resource_paths_encode_per_segment_keeping_slashes() {
        assert_eq!(
            encode_resource_path("com.x/views/My Folder/V1"),
            "com%2Ex/views/My%20Folder/V1",
            "spaces encode, slashes survive, over-encoding the dot is safe"
        );
        assert_eq!(encode_resource_path("plain"), "plain");
        assert_eq!(
            encode_resource_path("ignition/script-python/e2e/scratch"),
            "ignition/script%2Dpython/e2e/scratch"
        );
        // The full builder composes project encoding + per-segment
        // resource encoding.
        assert_eq!(
            resource_path("My Proj", "com.x/views/My Folder/V1"),
            "/data/api/v1/projects/My%20Proj/resources/com%2Ex/views/My%20Folder/V1"
        );
    }

    /// A plausible list item parses with `path` typed and every other
    /// key passthrough (MEDIUM shape — nothing else is modeled).
    #[test]
    fn resource_entry_parses_with_passthrough_extras() {
        let entry: ResourceEntry = serde_json::from_value(serde_json::json!({
            "path": "com.inductiveautomation.perspective/views/Dashboard",
            "scope": "A",
            "version": 1,
            "restricted": false
        }))
        .expect("plausible item must parse");
        assert_eq!(
            entry.path.as_deref(),
            Some("com.inductiveautomation.perspective/views/Dashboard")
        );
        assert_eq!(
            entry.extra.get("scope"),
            Some(&serde_json::json!("A")),
            "unknown keys round-trip unmodeled"
        );

        // An item WITHOUT a path (folder markers / shape drift) still
        // parses — `path` is Option by design.
        let bare: ResourceEntry = serde_json::from_value(serde_json::json!({"folder": true}))
            .expect("pathless item parses");
        assert_eq!(bare.path, None);
    }
}
