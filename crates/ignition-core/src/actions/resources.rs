//! Resource actions (05-02 re-point): the surgical edit loop —
//! list/get/put/delete ONE resource inside a project — riding
//! project-export ZIP surgery instead of the nonexistent
//! `/projects/{p}/resources/**` REST routes (the Phase 3 cross-phase
//! defect, closed here; 05-RESEARCH §Resource Family Decision).
//! Serde models OUT, no printing (ARCHITECTURE.md layering: the
//! Phase-6 TUI rides this same layer).
//!
//! THE ORCHESTRATION (transport swapped, UX contract untouched):
//! - list/get: [`GatewayApi::project_export_to_file`] to a temp zip
//!   → read the bytes → the pure helpers in
//!   [`crate::client::resources`] (`resource_members` / `read_member`)
//!   → the existing result shapes. A nonexistent project surfaces
//!   through export's existing 404 path (`not_found`, exit 6).
//! - put: sniff the INPUT first (binary refuses before ANY network) →
//!   export → `replace_member` (append-when-absent = upsert) →
//!   [`GatewayApi::project_import`] with `overwrite=true` — put
//!   implicitly REPLACES the entire project, so the CLI guards it
//!   `--yes` like every destructive verb (05-02; the 03-03 unguarded
//!   put is superseded, README documents the consequence).
//! - delete: same surgery with `remove_member` (missing member →
//!   `not_found`) → import overwrite.
//!
//! Perf honesty (research's accepted trade): every resource op
//! round-trips the WHOLE project zip. Rigs and dev projects are
//! small; the alternative was the family not working at all.
//!
//! The heart that survives from 03-03 is [`classify_content`] — the
//! classify/HTML-sniffer discipline INVERTED: it sniffs resource
//! CONTENT to pick the wire representation. The order is the
//! contract (Pitfall 7):
//! 1. NUL byte in the first 8 KiB → Binary (refuse — a `data.bin`
//!    resource must NEVER round-trip through the JSON/text loop;
//!    export/import owns binary resources);
//! 2. valid UTF-8 that JSON-parses → Json;
//! 3. valid UTF-8 → Text;
//! 4. invalid UTF-8 (no NUL in the head) → Binary all the same.
//!
//! The get result keeps the family's stable agent shape
//! (`{project, path, content_kind, content}` — all keys always
//! present; `content` is the parsed JSON value or the text as a JSON
//! string); a Binary get refuses with [`CoreError::ResourceBinary`]
//! before any result is built — now sniffed from the zip MEMBER
//! bytes.

use serde::Serialize;

use crate::client::GatewayApi;
use crate::client::resources::{
    ResourceEntry, read_member, remove_member, replace_member, resource_members,
};
use crate::error::CoreError;

/// How far into a resource body the binary heuristic looks — real
/// JSON/text payloads never carry NUL anywhere, and 8 KiB catches
/// every `data.bin`-class resource's magic long before the tail.
const BINARY_SNIFF_WINDOW: usize = 8 * 1024;

/// The sniffed content kind — see the module docs for the order.
#[derive(Debug, Clone, PartialEq)]
pub enum ContentKind {
    /// Valid UTF-8 that JSON-parsed (the parsed value rides along).
    Json(serde_json::Value),
    /// Valid UTF-8 that did not JSON-parse.
    Text(String),
    /// NUL in the first 8 KiB, or non-UTF-8 — refused downstream.
    Binary,
}

/// Sniff resource bytes: NUL-in-first-8KiB → Binary; UTF-8 +
/// JSON-parse → Json; UTF-8 → Text; else Binary. Pure — testable
/// without any gateway.
pub fn classify_content(bytes: &[u8]) -> ContentKind {
    let head = &bytes[..bytes.len().min(BINARY_SNIFF_WINDOW)];
    if head.contains(&0) {
        return ContentKind::Binary;
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => match serde_json::from_str::<serde_json::Value>(text) {
            Ok(value) => ContentKind::Json(value),
            Err(_) => ContentKind::Text(text.to_string()),
        },
        Err(_) => ContentKind::Binary,
    }
}

impl ContentKind {
    /// The stable agent-facing label ("json" | "text" — Binary never
    /// reaches a result; it refuses).
    fn label(&self) -> &'static str {
        match self {
            Self::Json(_) => "json",
            Self::Text(_) => "text",
            Self::Binary => "binary",
        }
    }
}

/// `ign resource list` output model: the entries, one path per line
/// in human mode (surgery-sourced entries carry only `path`).
#[derive(Debug, Serialize)]
pub struct ResourcesResult {
    /// The project's resources (member paths in zip order).
    pub resources: Vec<ResourceEntry>,
}

/// `ign resource get` output model — the stable agent shape:
/// `{project, path, content_kind, content}` (a Binary get refuses
/// before this is built; `content` is the parsed JSON value or the
/// text as a JSON string).
#[derive(Debug, Serialize)]
pub struct ResourceGetResult {
    /// The project the resource lives in.
    pub project: String,
    /// The resource path.
    pub path: String,
    /// "json" | "text" — the sniffed kind.
    pub content_kind: String,
    /// The content: parsed JSON (any shape) or the UTF-8 text.
    pub content: serde_json::Value,
}

/// `ign resource put` output model.
#[derive(Debug, Serialize)]
pub struct ResourcePutResult {
    /// The project the resource landed in.
    pub project: String,
    /// The resource path (created if absent — upsert).
    pub path: String,
    /// "json" | "text" — the sniffed kind that rode the surgery.
    pub content_kind: String,
}

/// `ign resource delete` output model.
#[derive(Debug, Serialize)]
pub struct ResourceDeleteResult {
    /// The deleted resource's path.
    pub deleted: String,
}

/// The shared first half of every resource op: stream the project
/// export into a unique temp file, then read the bytes back for
/// in-memory surgery. A nonexistent project fails inside export's
/// existing classification (404 → `not_found`, exit 6). The
/// `tempfile` dependency (promoted from dev — already in the
/// workspace graph) owns uniqueness and cleanup-on-drop.
///
/// 07-01: promoted pub — the cross-gateway diff/sync actions ride the
/// SAME export-to-bytes seam (two clients, one helper).
pub async fn export_zip_bytes(api: &dyn GatewayApi, project: &str) -> Result<Vec<u8>, CoreError> {
    let temp = tempfile::NamedTempFile::new()
        .map_err(|err| CoreError::Internal(format!("cannot create temp export file: {err}")))?;
    api.project_export_to_file(project, temp.path()).await?;
    tokio::fs::read(temp.path()).await.map_err(|err| {
        CoreError::Internal(format!(
            "cannot read back export {}: {err}",
            temp.path().display()
        ))
    })
}

/// `ign resource list PROJECT [--prefix P]` — export → member list.
/// The prefix filters CLIENT-SIDE now (member paths, `starts_with`):
/// the old server-side `path` query param rode routes that never
/// existed; the UX contract (one path per line) is unchanged.
pub async fn resources_list(
    api: &dyn GatewayApi,
    project: &str,
    prefix: Option<&str>,
) -> Result<ResourcesResult, CoreError> {
    let zip = export_zip_bytes(api, project).await?;
    let resources = resource_members(&zip)?
        .into_iter()
        .filter(|path| prefix.is_none_or(|prefix| path.starts_with(prefix)))
        .map(|path| ResourceEntry {
            path: Some(path),
            extra: Default::default(),
        })
        .collect();
    Ok(ResourcesResult { resources })
}

/// `ign resource get PROJECT PATH` — export → member read → sniff →
/// the stable shape. Binary (now sniffed from the zip member bytes)
/// → [`CoreError::ResourceBinary`] (exit 6): a `data.bin`-class
/// resource must never be corrupted through the JSON loop (Pitfall
/// 7). A missing member is `not_found` from the surgery helper.
pub async fn resource_get(
    api: &dyn GatewayApi,
    project: &str,
    path: &str,
) -> Result<ResourceGetResult, CoreError> {
    let zip = export_zip_bytes(api, project).await?;
    let bytes = read_member(&zip, path)?;
    match classify_content(&bytes) {
        ContentKind::Json(value) => Ok(ResourceGetResult {
            project: project.to_string(),
            path: path.to_string(),
            content_kind: "json".to_string(),
            content: value,
        }),
        ContentKind::Text(text) => Ok(ResourceGetResult {
            project: project.to_string(),
            path: path.to_string(),
            content_kind: "text".to_string(),
            content: serde_json::Value::String(text),
        }),
        ContentKind::Binary => Err(CoreError::ResourceBinary {
            path: path.to_string(),
            endpoint: None,
        }),
    }
}

/// `ign resource put PROJECT PATH --file F|-` — sniff the INPUT
/// first: Binary refuses (exit 6) before ANY network I/O. Then the
/// surgery loop: export → `replace_member` (append-when-absent =
/// upsert) → import `overwrite=true`. The import REPLACES the entire
/// project — replace-not-merge wipes concurrent Designer edits — so
/// the CLI dispatch guards this verb `--yes` BEFORE resolution (the
/// 05-02 destructive-verb set; 03-03's unguarded put is superseded).
pub async fn resource_put(
    api: &dyn GatewayApi,
    project: &str,
    path: &str,
    input: Vec<u8>,
) -> Result<ResourcePutResult, CoreError> {
    let kind = classify_content(&input);
    if matches!(kind, ContentKind::Binary) {
        return Err(CoreError::ResourceBinary {
            path: path.to_string(),
            endpoint: None,
        });
    }
    let zip = export_zip_bytes(api, project).await?;
    let surgical = replace_member(&zip, path, &input)?;
    api.project_import(project, surgical, true).await?;
    Ok(ResourcePutResult {
        project: project.to_string(),
        path: path.to_string(),
        content_kind: kind.label().to_string(),
    })
}

/// `ign resource delete PROJECT PATH` — export → `remove_member`
/// (missing member → `not_found`) → import overwrite. The `--yes`
/// guard belongs to the CLI CALLER (it refuses pre-resolution, the
/// LOCKED 02-03 shape) — this arm only runs once confirmed.
pub async fn resource_delete(
    api: &dyn GatewayApi,
    project: &str,
    path: &str,
) -> Result<ResourceDeleteResult, CoreError> {
    let zip = export_zip_bytes(api, project).await?;
    let surgical = remove_member(&zip, path)?;
    api.project_import(project, surgical, true).await?;
    Ok(ResourceDeleteResult {
        deleted: path.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::{ContentKind, classify_content};

    /// The sniffer's three outcomes, each pinned: JSON parses (value
    /// preserved), non-JSON UTF-8 stays text, NUL-in-head is binary.
    #[test]
    fn classify_content_sniffs_all_three_kinds() {
        assert_eq!(
            classify_content(br#"{"scope":"G","code":"print('hi')"}"#),
            ContentKind::Json(serde_json::json!({"scope":"G","code":"print('hi')"})),
            "UTF-8 that JSON-parses → Json (value preserved)"
        );
        assert_eq!(
            classify_content(b"print('just a script')\n"),
            ContentKind::Text("print('just a script')\n".to_string()),
            "UTF-8 that does not parse → Text"
        );
        assert_eq!(
            classify_content(&[0x00, 0x50, 0x4B, 0x03]),
            ContentKind::Binary,
            "NUL in the head → Binary (data.bin class)"
        );
        assert_eq!(
            classify_content(&[0xFF, 0xFE, 0x00, 0x01]),
            ContentKind::Binary,
            "non-UTF-8 → Binary too (no honest textual form)"
        );
    }

    /// The NUL window's honest boundary: a NUL PAST the first 8 KiB
    /// in otherwise-valid UTF-8 is NOT caught by the head heuristic —
    /// and `from_utf8` accepts NUL as text (0x00 is valid UTF-8), so
    /// it classifies Text. That is the documented trade: real
    /// data.bin-class resources carry binary magic well inside the
    /// window; only adversarial input hides a NUL past it.
    #[test]
    fn classify_content_nul_past_window_is_text() {
        let mut bytes = vec![b'x'; super::BINARY_SNIFF_WINDOW + 64];
        bytes[super::BINARY_SNIFF_WINDOW + 32] = 0;
        assert_eq!(
            classify_content(&bytes),
            ContentKind::Text(String::from_utf8(bytes.clone()).expect("NUL is valid UTF-8")),
            "a lone NUL past the 8 KiB window in UTF-8 input classifies Text \
             (the heuristic's documented boundary)"
        );
        // …while the SAME NUL inside the window refuses.
        bytes[16] = 0;
        assert_eq!(classify_content(&bytes), ContentKind::Binary);
    }

    /// The labels the results ride on.
    #[test]
    fn content_kind_labels() {
        assert_eq!(ContentKind::Json(serde_json::json!(1)).label(), "json");
        assert_eq!(ContentKind::Text(String::new()).label(), "text");
        assert_eq!(ContentKind::Binary.label(), "binary");
    }
}
