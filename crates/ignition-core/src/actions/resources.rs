//! Resource actions (03-03, PROJ-05): the surgical edit loop —
//! list/get/put/delete ONE resource inside a project, serde models
//! OUT, no printing (ARCHITECTURE.md layering: the Phase-6 TUI rides
//! this same layer).
//!
//! The heart is [`classify_content`] — the classify/HTML-sniffer
//! discipline INVERTED: instead of sniffing an ERROR body to pick an
//! error class, it sniffs resource CONTENT to pick the wire
//! representation. The order is the contract (Pitfall 7):
//! 1. NUL byte in the first 8 KiB → Binary (refuse — a `data.bin`
//!    resource must NEVER round-trip through the JSON/text loop;
//!    export/import owns binary resources);
//! 2. valid UTF-8 that JSON-parses → Json (`application/json` on
//!    put; pretty-printed on get);
//! 3. valid UTF-8 → Text (`text/plain; charset=utf-8` on put; raw
//!    passthrough on get);
//! 4. invalid UTF-8 (no NUL in the head) → Binary all the same —
//!    non-UTF-8 payloads have no honest textual representation.
//!
//! The get result keeps the family's stable agent shape
//! (`{project, path, content_kind, content}` — all keys always
//! present; `content` is the parsed JSON value or the text as a JSON
//! string); a Binary get refuses with [`CoreError::ResourceBinary`]
//! before any result is built.

use serde::Serialize;

use crate::client::GatewayApi;
use crate::client::resources::ResourceEntry;
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

    /// The wire Content-Type a put of this kind declares.
    fn put_content_type(&self) -> &'static str {
        match self {
            Self::Json(_) => "application/json",
            Self::Text(_) => "text/plain; charset=utf-8",
            Self::Binary => "application/octet-stream",
        }
    }
}

/// `ign resource list` output model: the passthrough entries, one
/// path per line in human mode.
#[derive(Debug, Serialize)]
pub struct ResourcesResult {
    /// The project's resources (passthrough-heavy — MEDIUM shape).
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
    /// "json" | "text" — the sniffed kind that rode the wire.
    pub content_kind: String,
}

/// `ign resource delete` output model.
#[derive(Debug, Serialize)]
pub struct ResourceDeleteResult {
    /// The deleted resource's path.
    pub deleted: String,
}

/// `ign resource list PROJECT [--prefix P]` — the passthrough entries;
/// the prefix rides the wire as the server-side `path` filter.
pub async fn resources_list(
    api: &dyn GatewayApi,
    project: &str,
    prefix: Option<&str>,
) -> Result<ResourcesResult, CoreError> {
    let page = api.project_resources(project, prefix).await?;
    Ok(ResourcesResult {
        resources: page.items,
    })
}

/// `ign resource get PROJECT PATH` — fetch the RAW bytes, sniff, and
/// hand back the stable shape. Binary → [`CoreError::ResourceBinary`]
/// (exit 6): a `data.bin`-class resource must never be corrupted
/// through the JSON loop (Pitfall 7).
pub async fn resource_get(
    api: &dyn GatewayApi,
    project: &str,
    path: &str,
) -> Result<ResourceGetResult, CoreError> {
    let raw = api.project_resource_get(project, path).await?;
    match classify_content(&raw.bytes) {
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

/// `ign resource put PROJECT PATH --file F|-` — sniff the INPUT: Json
/// rides as `application/json`, Text as `text/plain; charset=utf-8`,
/// Binary refuses (exit 6). Upsert semantics server-side: the
/// resource is created when absent, replaced when present.
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
    let content_type = kind.put_content_type();
    api.project_resource_put(project, path, input, content_type)
        .await?;
    Ok(ResourcePutResult {
        project: project.to_string(),
        path: path.to_string(),
        content_kind: kind.label().to_string(),
    })
}

/// `ign resource delete PROJECT PATH` — the obedient arm; the `--yes`
/// guard belongs to the CLI CALLER (it refuses pre-resolution, the
/// LOCKED 02-03 shape). Audit-logged server-side.
pub async fn resource_delete(
    api: &dyn GatewayApi,
    project: &str,
    path: &str,
) -> Result<ResourceDeleteResult, CoreError> {
    api.project_resource_delete(project, path).await?;
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

    /// The labels and content types the results/requests ride on.
    #[test]
    fn content_kind_labels_and_content_types() {
        assert_eq!(ContentKind::Json(serde_json::json!(1)).label(), "json");
        assert_eq!(ContentKind::Text(String::new()).label(), "text");
        assert_eq!(ContentKind::Binary.label(), "binary");
        assert_eq!(
            ContentKind::Json(serde_json::json!(1)).put_content_type(),
            "application/json"
        );
        assert_eq!(
            ContentKind::Text(String::new()).put_content_type(),
            "text/plain; charset=utf-8"
        );
    }
}
