//! Log capability models (02-04, HLTH-03/04) — log entries, the query
//! (with the tail cursor), the logger registry, and the archive
//! download. Field names match the live 8.3.6 captures (02-RESEARCH
//! §Logs + loggers) and the gateway's openapi schema; every model rides
//! `#[serde(flatten)] extra` passthrough so `--json` stays complete as
//! gateway responses evolve.
//!
//! THREE wire facts are pinned here (all live-verified):
//! - `startTime` (epoch ms) IS the tail cursor: only entries with
//!   `timestamp >= startTime` return, and there is NO server push —
//!   polling this query is the tail primitive (Don't-Hand-Roll table).
//! - `logs/download` answers a SQLite database
//!   (`application/x-sqlite3`, filename from `Content-Disposition`) —
//!   NOT a zip; the bytes ship exactly as received (Pitfall 7).
//! - An UNSET `limit` means the server's UNLIMITED default (metadata
//!   showed `limit: -1` with everything returned) — every request this
//!   CLI sends carries an EXPLICIT limit ([`DEFAULT_LOG_LIMIT`] = 200;
//!   Pitfall 9: a 2M-entry gateway log must not flood agents).
//!
//! Logger names are Java identifiers (`[A-Za-z0-9._]` — openapi), so
//! they embed URL-safe in the set-level path as-is; documented rather
//! than percent-encoded.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// GET path of the log query — the tail primitive.
pub(crate) const LOGS_PATH: &str = "/data/api/v1/logs";

/// GET path of the archive download (SQLite `.idb` bytes).
pub(crate) const LOGS_DOWNLOAD_PATH: &str = "/data/api/v1/logs/download";

/// GET path of the logger registry (~1250 loggers on a fresh gateway).
pub(crate) const LOGGERS_PATH: &str = "/data/api/v1/logs/loggers";

/// POST path that resets all custom logger levels to defaults.
pub(crate) const LEVEL_RESET_PATH: &str = "/data/api/v1/logs/levelreset";

/// POST path of the set-level route (`?level=X` query param).
pub(crate) fn logger_set_path(logger: &str) -> String {
    format!("/data/api/v1/logs/loggers/{logger}")
}

/// The explicit limit every logs request carries (Pitfall 9) — the
/// server default is UNLIMITED and a 2M-entry gateway log would flood
/// agents and terminals alike.
pub const DEFAULT_LOG_LIMIT: i64 = 200;

/// The per-request download timeout in seconds — a large archive must
/// not be truncated by the 30s client default (per-class timeout via
/// `RequestBuilder::timeout`, not a second client).
pub const LOGS_DOWNLOAD_TIMEOUT_SECS: u64 = 120;

/// One item of `GET /data/api/v1/logs` — the shape of the live capture
/// (camelCase keys, serde-renamed). `timestamp` is epoch **MILLISECONDS**
/// and doubles as the tail cursor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    /// Epoch **MILLISECONDS** — the tail cursor (`startTime` param).
    pub timestamp: i64,
    /// Logger name (`loggerName` on the wire), e.g.
    /// `"GatewayManager"` or `"Common.BasicExecutionEngine.Thread$"`.
    #[serde(rename = "loggerName", alias = "logger_name")]
    pub logger_name: String,
    /// `"TRACE"` / `"DEBUG"` / `"INFO"` / `"WARN"` / `"ERROR"` /
    /// `"FATAL"` (the wire keeps them uppercase).
    #[serde(default)]
    pub level: String,
    /// The rendered log message.
    #[serde(default)]
    pub message: String,
    /// Stack-trace lines when the entry carries a throwable (absent on
    /// the wire for plain entries — `default` + skip keeps output clean).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stack: Vec<String>,
    /// Mapped diagnostic context, when present.
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub mdc: serde_json::Map<String, serde_json::Value>,
    /// Unknown keys round-trip (passthrough-shaped `--json`).
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// The logs query — every param optional server-side, but `limit` is
/// ALWAYS sent explicitly by this CLI (Pitfall 9). `start_time` is the
/// tail cursor; `end_time` bounds historical windows; `sort_by` uses
/// the gateway's own `asc(field)` / `desc(field)` syntax (openapi).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LogQuery {
    /// Include results from this epoch-ms timestamp (the tail cursor).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time: Option<i64>,
    /// Include results up to this epoch-ms timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<i64>,
    /// Only entries of `min_level` OR HIGHER
    /// (`minLevel` on the wire; TRACE..OFF, server-side filtering).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_level: Option<String>,
    /// Filter to one logger name prefix (`logger` on the wire).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logger: Option<String>,
    /// Max items — ALWAYS explicit ([`DEFAULT_LOG_LIMIT`]); `-1` would
    /// mean the server's unlimited default (Pitfall 9).
    pub limit: i64,
    /// Skip the first `offset` items.
    pub offset: i64,
    /// Server-side sort: `asc(fieldName)` / `desc(fieldName)`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_by: Option<String>,
}

impl Default for LogQuery {
    fn default() -> Self {
        Self {
            start_time: None,
            end_time: None,
            min_level: None,
            logger: None,
            limit: DEFAULT_LOG_LIMIT,
            offset: 0,
            sort_by: None,
        }
    }
}

impl LogQuery {
    /// Serialize into query pairs under the gateway-native param names;
    /// `limit`/`offset` always present, optional keys only when `Some`.
    pub fn to_query_pairs(&self) -> Vec<(String, String)> {
        let mut pairs = Vec::with_capacity(7);
        if let Some(start_time) = self.start_time {
            pairs.push(("startTime".to_string(), start_time.to_string()));
        }
        if let Some(end_time) = self.end_time {
            pairs.push(("endTime".to_string(), end_time.to_string()));
        }
        if let Some(min_level) = &self.min_level {
            pairs.push(("minLevel".to_string(), min_level.clone()));
        }
        if let Some(logger) = &self.logger {
            pairs.push(("logger".to_string(), logger.clone()));
        }
        pairs.push(("limit".to_string(), self.limit.to_string()));
        pairs.push(("offset".to_string(), self.offset.to_string()));
        if let Some(sort_by) = &self.sort_by {
            pairs.push(("sortBy".to_string(), sort_by.clone()));
        }
        pairs
    }
}

/// A page of log entries in the standard list envelope.
pub type LogPage = crate::client::query::ListEnvelope<LogEntry>;

/// One item of `GET /data/api/v1/logs/loggers` — `{name, level,
/// context}`; `level` is `None` for inherited loggers and `context` is
/// modeled as passthrough (its populated shape was not captured).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoggerInfo {
    /// Logger name, e.g. `"Common.BasicExecutionEngine.Thread$"`.
    pub name: String,
    /// Explicit level, when the logger carries one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    /// Context block, passthrough (shape not live-captured).
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub context: serde_json::Value,
    /// Unknown keys round-trip.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// The archive download result — raw SQLite bytes EXACTLY as received
/// (never zipped, never extracted) plus the response metadata the CLI
/// needs for naming. Not serialized into envelopes (the bytes are the
/// artifact; the command output model lives in the actions layer).
#[derive(Debug, Clone)]
pub struct LogDownload {
    /// The `.idb` payload, byte-for-byte as the gateway sent it.
    pub bytes: Vec<u8>,
    /// Filename from `Content-Disposition`, when the header carries one
    /// (the live gateway always sends `<Gateway>_Ignition_logs_<ts>.idb`).
    pub filename: Option<String>,
    /// Response `Content-Type` — `application/x-sqlite3` (verified).
    pub content_type: Option<String>,
}

/// Extract the filename from a `Content-Disposition` header value —
/// supports both the classic `filename="..."` (quoted or bare) and
/// RFC 5987 `filename*=UTF-8''...` forms via substring scan (a header
/// is not HTML; 20 lines beat a MIME crate).
pub fn filename_from_content_disposition(value: &str) -> Option<String> {
    // RFC 5987 extended form wins when present.
    if let Some(start) = value.find("filename*=") {
        let rest = &value[start + "filename*=".len()..];
        // charset''name — split twice at most; the name runs to ; or end.
        let name = rest.split(';').next().unwrap_or(rest);
        let decoded = match name.split_once('\'') {
            Some((_, after_charset)) => after_charset
                .split_once('\'')
                .map(|(_, raw)| raw)
                .unwrap_or(name),
            None => name,
        };
        if !decoded.is_empty() {
            return Some(decoded.to_string());
        }
    }
    let start = value.find("filename=")? + "filename=".len();
    let rest = value[start..].split(';').next().unwrap_or(&value[start..]);
    let trimmed = rest.trim().trim_matches('"');
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_LOG_LIMIT, LogEntry, LogQuery, filename_from_content_disposition};

    /// The live-captured entry shape parses: camelCase renames, a stack
    /// trace entry, an MDC map — and a plain entry (no stack/mdc).
    #[test]
    fn log_entry_parses_the_live_capture_incl_stack() {
        let entry: LogEntry = serde_json::from_value(serde_json::json!({
            "timestamp": 1787346747022i64,
            "loggerName": "Common.BasicExecutionEngine.Thread$",
            "level": "ERROR",
            "message": "Execution halted by exception",
            "stack": [
                "java.lang.RuntimeException: boom",
                "\tat com.inductiveautomation.ignition.common.Sample.run(Sample.java:42)"
            ],
            "mdc": {"thread": "Thread-12"}
        }))
        .expect("live capture shape must parse");
        assert_eq!(entry.timestamp, 1787346747022, "epoch ms — the tail cursor");
        assert_eq!(entry.logger_name, "Common.BasicExecutionEngine.Thread$");
        assert_eq!(entry.level, "ERROR");
        assert_eq!(entry.stack.len(), 2, "stack trace lines parse");
        assert_eq!(entry.mdc["thread"], "Thread-12");

        // Wire-faithful round-trip: gateway-native keys on the way out,
        // empty stack/mdc omitted (as the wire omits them).
        let round = serde_json::to_value(&entry).expect("serialize");
        assert_eq!(round["loggerName"], "Common.BasicExecutionEngine.Thread$");
        assert_eq!(round["stack"].as_array().unwrap().len(), 2);

        let plain: LogEntry = serde_json::from_value(serde_json::json!({
            "timestamp": 1787346747030i64,
            "loggerName": "GatewayManager",
            "level": "INFO",
            "message": "Gateway started"
        }))
        .expect("plain entry (no stack, no mdc) must parse");
        assert!(plain.stack.is_empty());
        assert!(plain.mdc.is_empty());
        let round = serde_json::to_value(&plain).expect("serialize");
        assert!(
            round.get("stack").is_none() && round.get("mdc").is_none(),
            "empty stack/mdc stay absent on the way out"
        );
    }

    /// Pitfall 9: the default query carries an EXPLICIT limit (200) and
    /// offset 0; every optional key serializes under its gateway-native
    /// name only when present.
    #[test]
    fn query_pairs_carry_explicit_limit_and_native_names() {
        let pairs = LogQuery::default().to_query_pairs();
        assert_eq!(
            pairs,
            vec![
                ("limit".to_string(), DEFAULT_LOG_LIMIT.to_string()),
                ("offset".to_string(), "0".to_string()),
            ],
            "default = explicit limit 200 + offset 0 (Pitfall 9)"
        );

        let full = LogQuery {
            start_time: Some(1787346747022),
            end_time: Some(1787346757022),
            min_level: Some("INFO".into()),
            logger: Some("GatewayManager".into()),
            limit: 50,
            offset: 100,
            sort_by: Some("desc(timestamp)".into()),
        };
        let pairs = full.to_query_pairs();
        assert_eq!(
            pairs,
            vec![
                ("startTime".to_string(), "1787346747022".to_string()),
                ("endTime".to_string(), "1787346757022".to_string()),
                ("minLevel".to_string(), "INFO".to_string()),
                ("logger".to_string(), "GatewayManager".to_string()),
                ("limit".to_string(), "50".to_string()),
                ("offset".to_string(), "100".to_string()),
                ("sortBy".to_string(), "desc(timestamp)".to_string()),
            ],
            "camelCase param names, declared order"
        );
    }

    /// The verified header shape yields the `.idb` filename; quoted,
    /// bare, RFC 5987, and absent forms all behave.
    #[test]
    fn content_disposition_filename_extraction() {
        assert_eq!(
            filename_from_content_disposition(
                "attachment; filename=MyGateway_Ignition_logs_20260822-0307.idb"
            ),
            Some("MyGateway_Ignition_logs_20260822-0307.idb".to_string())
        );
        assert_eq!(
            filename_from_content_disposition("attachment; filename=\"quoted.idb\""),
            Some("quoted.idb".to_string())
        );
        assert_eq!(
            filename_from_content_disposition("attachment; filename*=UTF-8''encoded.idb"),
            Some("encoded.idb".to_string())
        );
        assert_eq!(filename_from_content_disposition("attachment"), None);
        assert_eq!(filename_from_content_disposition("inline"), None);
    }
}
