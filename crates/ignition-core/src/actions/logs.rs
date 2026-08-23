//! Log actions (02-04, HLTH-03/04): the poll-based TAIL loop — serde
//! models and a sink OUT, no printing (ARCHITECTURE.md layering: the
//! Phase-6 TUI rides this same layer).
//!
//! There is NO server push for gateway logs (02-RESEARCH Don't-Hand-Roll
//! table): `GET /logs?startTime=<epoch-ms>` IS the tail primitive. The
//! loop polls through the shared [`crate::poll`] engine (×1.5 adaptive
//! backoff, Network/GatewayRestarting retried, Auth never) — the same
//! engine 02-05's `wait` reuses.
//!
//! Cursor semantics (plan key_link): start at `since` (or 0 = the
//! whole buffer); every page advances the cursor to the max timestamp
//! seen; the next query sends `startTime = cursor + 1` — no overlap,
//! no gaps. Entries are sorted client-side so the stream order is
//! timestamp order regardless of the server's page ordering.
//!
//! `deadline: None` = run until Ctrl-C (the process default kill —
//! research: keep Ctrl-C simple, README-documented); `Some(d)` ends
//! GRACEFULLY: the poll's deadline expiry maps to `Ok` (exit 0 — the
//! entries already streamed through the sink).

use std::cell::Cell;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::client::logs::{LogDownload, LogEntry, LogQuery, LoggerInfo};
// Public re-export: the list action's return type is the wire-faithful
// page — callers (and ActionOutput) name it via the action module.
use crate::client::GatewayApi;
pub use crate::client::logs::LogPage;
use crate::client::query::ListEnvelope;
use crate::error::CoreError;
use crate::poll::{self, PollConfig, PollState};

/// `ign logs download` output model.
#[derive(Debug, Serialize)]
pub struct DownloadResult {
    /// Path of the file written.
    pub file: String,
    /// Bytes written.
    pub bytes: usize,
    /// Response content type (`application/x-sqlite3` — verified).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
}

/// `ign logs loggers set` output model.
#[derive(Debug, Serialize)]
pub struct SetLevelResult {
    /// The logger that was changed.
    pub logger: String,
    /// The level it now carries (uppercase wire form).
    pub level: String,
}

/// `ign logs loggers reset` output model.
#[derive(Debug, Serialize)]
pub struct ResetResult {
    /// Always `true` — the reset reset every custom level.
    pub reset: bool,
}

/// The logger registry page (`ign logs loggers`).
pub type LoggersEnvelope = ListEnvelope<LoggerInfo>;

/// `ign logs` (no `--follow`): newest entries first. The query always
/// sorts `desc(timestamp)` and carries an explicit `limit` — together
/// they make "the recent log entries" (must-have truth #1) without a
/// `--since` window guess: `--limit 200` = the NEWEST 200, not the
/// oldest.
pub async fn list_logs(
    api: &dyn GatewayApi,
    logger: Option<&str>,
    min_level: Option<&str>,
    since_ms: Option<i64>,
    limit: i64,
) -> Result<LogPage, CoreError> {
    let query = LogQuery {
        start_time: since_ms,
        logger: logger.map(str::to_string),
        min_level: min_level.map(str::to_string),
        limit,
        sort_by: Some("desc(timestamp)".to_string()),
        ..LogQuery::default()
    };
    api.logs(&query).await
}

/// `ign logs loggers`: the logger registry, explicit `limit` (must-have
/// truth #5 — even the registry never rides the unlimited default),
/// optional substring `search`.
pub async fn loggers(
    api: &dyn GatewayApi,
    search: Option<&str>,
) -> Result<ListEnvelope<LoggerInfo>, CoreError> {
    let query = crate::client::query::ListQuery {
        limit: crate::client::logs::DEFAULT_LOG_LIMIT,
        search: search.map(str::to_string),
        ..Default::default()
    };
    api.loggers(&query).await
}

/// `ign logs loggers set <name> <LEVEL>`. Confirmation guarding belongs
/// to the CALLER (the CLI refuses without `--yes` before any API
/// construction) — the action is the obedient arm.
pub async fn set_logger_level(
    api: &dyn GatewayApi,
    logger: &str,
    level: &str,
) -> Result<SetLevelResult, CoreError> {
    api.set_logger_level(logger, level).await?;
    Ok(SetLevelResult {
        logger: logger.to_string(),
        level: level.to_string(),
    })
}

/// `ign logs loggers reset` — same guard contract as set.
pub async fn reset_logger_levels(api: &dyn GatewayApi) -> Result<ResetResult, CoreError> {
    api.reset_logger_levels().await?;
    Ok(ResetResult { reset: true })
}

/// The download filename: `-o FILE` wins, then the gateway's
/// `Content-Disposition` name, then `<stem>-logs-<unix_ts>.idb` — NEVER
/// `.zip` (Pitfall 7: the archive is SQLite).
fn download_filename(
    output: Option<&Path>,
    download: &LogDownload,
    stem: &str,
    now_secs: i64,
) -> String {
    if let Some(output) = output {
        return output.display().to_string();
    }
    if let Some(filename) = download.filename.as_deref().filter(|name| !name.is_empty()) {
        return filename.to_string();
    }
    format!("{stem}-logs-{now_secs}.idb")
}

/// `ign logs download` — fetch the `.idb` archive and write it EXACTLY
/// as received (no transformation, no extraction). `stem` names the
/// gateway for the fallback filename (the CLI passes the profile name).
pub async fn download(
    api: &dyn GatewayApi,
    output: Option<&Path>,
    fallback_stem: &str,
) -> Result<DownloadResult, CoreError> {
    let fetched = api.logs_download().await?;
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or_default();
    let file = download_filename(output, &fetched, fallback_stem, now_secs);
    std::fs::write(&file, &fetched.bytes)
        .map_err(|err| CoreError::Internal(format!("cannot write log archive {file}: {err}")))?;
    Ok(DownloadResult {
        bytes: fetched.bytes.len(),
        content_type: fetched.content_type,
        file,
    })
}

/// Parse `--since`: an absolute `EPOCH_MS` value or a relative span
/// `Nms` / `Ns` / `Nmin` / `Nh` (resolved against `now_ms`). Returned
/// as a plain `String` error so clap can surface it as a usage-class
/// parse failure (exit 2) — validation happens at arg-parse time, not
/// deep in dispatch.
pub fn parse_since(spec: &str, now_ms: i64) -> Result<i64, String> {
    let spec = spec.trim();
    // Order matters: "ms" before bare "s"; "min" between them.
    for (suffix, unit_ms) in [
        ("ms", 1_i64),
        ("min", 60_000),
        ("h", 3_600_000),
        ("s", 1_000),
    ] {
        if let Some(digits) = spec.strip_suffix(suffix)
            && let Ok(count) = digits.parse::<i64>()
            && count >= 0
        {
            return Ok(now_ms - count * unit_ms);
        }
    }
    // Absolute epoch-ms (also covers "0" = the whole buffer).
    match spec.parse::<i64>() {
        Ok(epoch_ms) if epoch_ms >= 0 => Ok(epoch_ms),
        _ => Err(format!(
            "invalid --since {spec:?}: expected EPOCH-MS or a relative span like 500ms, 30s, 5min, 2h"
        )),
    }
}

/// `ign logs --follow` result — how much streamed before the tail
/// ended. (Ctrl-C never reaches here: the process default kill emits
/// no envelope at all, README-documented.)
#[derive(Debug, Default, Serialize)]
pub struct TailResult {
    /// Entries delivered to the sink.
    pub streamed: usize,
}

/// The tail loop's probe scratch: the cursor (epoch ms) and the sink.
/// Owned by [`poll`], lent fresh to every probe call.
struct TailState<'a> {
    /// Max timestamp delivered so far (-1 before the first page).
    cursor: i64,
    /// Receives each entry as it arrives (the action stays
    /// printer-free — the dispatch owns stdout).
    sink: &'a mut dyn FnMut(&LogEntry),
}

/// Stream new log entries to `sink` as they arrive. The action is
/// printer-free — the dispatch owns stdout (human lines or NDJSON).
///
/// Every query carries an explicit limit ([`LogQuery::default`] —
/// Pitfall 9); a page larger than the limit still advances the cursor
/// correctly (cursor = max timestamp seen, so the next poll resumes
/// exactly past it).
pub async fn tail(
    api: &dyn GatewayApi,
    logger: Option<&str>,
    min_level: Option<&str>,
    since_ms: Option<i64>,
    interval: Duration,
    deadline: Option<Duration>,
    sink: &mut dyn FnMut(&LogEntry),
) -> Result<TailResult, CoreError> {
    // -1 so the FIRST query's start_time = cursor + 1 = since exactly
    // (or 0 when no --since — the whole buffer).
    let state = TailState {
        cursor: since_ms.unwrap_or(0) - 1,
        sink,
    };
    // The stream count lives OUTSIDE the poll call (the probe bumps it
    // through a shared borrow; poll consumes the state).
    let streamed = Cell::new(0usize);

    let cfg = PollConfig {
        subject: "log tail (GET /data/api/v1/logs)".to_string(),
        interval,
        deadline: deadline.unwrap_or(Duration::MAX),
        ..PollConfig::default()
    };

    let outcome = poll::poll(cfg, state, |state| {
        Box::pin(async {
            let query = LogQuery {
                start_time: Some(state.cursor + 1),
                logger: logger.map(str::to_string),
                min_level: min_level.map(str::to_string),
                ..LogQuery::default()
            };
            let page = api.logs(&query).await?;
            let mut entries = page.items;
            // Timestamp order regardless of server page ordering.
            entries.sort_by_key(|entry| entry.timestamp);
            let observation = entries
                .last()
                .map(|last| format!("{} entries, latest at {}", entries.len(), last.timestamp));
            for entry in &entries {
                (state.sink)(entry);
            }
            if let Some(last) = entries.last() {
                state.cursor = last.timestamp;
            }
            streamed.set(streamed.get() + entries.len());
            Ok(PollState::<()>::Pending(observation))
        })
    })
    .await;

    match outcome {
        // The probe never reports Done (T = ()) — the only Ok is
        // unreachable; kept for match totality.
        Ok(()) => Ok(TailResult {
            streamed: streamed.get(),
        }),
        // Deadline expiry = GRACEFUL end (exit 0): poll retries genuine
        // Network errors until the deadline, so a None-source Network
        // error IS the timeout. The entries already streamed.
        Err(CoreError::Network { source: None, .. }) => Ok(TailResult {
            streamed: streamed.get(),
        }),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Duration;

    use super::{download_filename, parse_since, tail};
    use crate::client::GatewayApi;
    use crate::client::logs::{LogDownload, LogEntry, LogQuery};
    use crate::client::query::{ListEnvelope, ListMetadata};
    use crate::error::CoreError;

    /// `--since` accepts EPOCH-MS and every relative suffix, parsed
    /// against a fixed now; junk is a usage-class String error.
    #[test]
    fn parse_since_accepts_epoch_and_relative_spans() {
        const NOW: i64 = 1_787_346_747_022;
        assert_eq!(parse_since("1787346747022", NOW), Ok(1787346747022));
        assert_eq!(parse_since("0", NOW), Ok(0));
        assert_eq!(parse_since("500ms", NOW), Ok(NOW - 500));
        assert_eq!(parse_since("30s", NOW), Ok(NOW - 30_000));
        assert_eq!(parse_since("5min", NOW), Ok(NOW - 300_000));
        assert_eq!(parse_since("2h", NOW), Ok(NOW - 7_200_000));
        // suffix order matters: "ms" before bare "s"
        assert_eq!(parse_since("1s", NOW), Ok(NOW - 1_000));
        assert!(parse_since("banana", NOW).is_err());
        assert!(
            parse_since("-5s", NOW).is_err(),
            "negative spans are invalid"
        );
        assert!(parse_since("", NOW).is_err());
    }

    /// Filename precedence: `-o FILE` > Content-Disposition > the
    /// `<stem>-logs-<ts>.idb` fallback — and NEVER a `.zip` (Pitfall 7).
    #[test]
    fn download_filename_precedence() {
        let fetched = LogDownload {
            bytes: Vec::new(),
            filename: Some("GW_Ignition_logs_20260822-0307.idb".into()),
            content_type: Some("application/x-sqlite3".into()),
        };
        assert_eq!(
            download_filename(
                Some(std::path::Path::new("/tmp/out.idb")),
                &fetched,
                "dev",
                1000
            ),
            "/tmp/out.idb",
            "-o wins"
        );
        assert_eq!(
            download_filename(None, &fetched, "dev", 1000),
            "GW_Ignition_logs_20260822-0307.idb",
            "Content-Disposition name second"
        );
        let anonymous = LogDownload {
            bytes: Vec::new(),
            filename: None,
            content_type: None,
        };
        assert_eq!(
            download_filename(None, &anonymous, "dev", 1_787_346_747),
            "dev-logs-1787346747.idb",
            "fallback = <stem>-logs-<unix_ts>.idb — never .zip"
        );
    }

    /// A scripted double: serves `pages` in order (then empty pages
    /// forever) and records every query it saw.
    #[derive(Default)]
    struct TailRig {
        pages: Mutex<std::collections::VecDeque<Vec<LogEntry>>>,
        queries: Mutex<Vec<LogQuery>>,
    }

    fn entry(timestamp: i64, message: &str) -> LogEntry {
        LogEntry {
            timestamp,
            logger_name: "GatewayManager".into(),
            level: "INFO".into(),
            message: message.into(),
            stack: Vec::new(),
            mdc: Default::default(),
            extra: Default::default(),
        }
    }

    #[async_trait::async_trait]
    impl GatewayApi for TailRig {

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
        async fn logs(&self, filter: &LogQuery) -> Result<ListEnvelope<LogEntry>, CoreError> {
            self.queries.lock().unwrap().push(filter.clone());
            let items = self.pages.lock().unwrap().pop_front().unwrap_or_default();
            Ok(ListEnvelope {
                metadata: ListMetadata {
                    total: items.len() as i64,
                    matching: items.len() as i64,
                    limit: 200,
                    offset: 0,
                },
                items,
            })
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
            _query: &crate::client::query::ListQuery,
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
            _query: &crate::client::query::ListQuery,
        ) -> Result<ListEnvelope<crate::client::sessions::DesignerInfo>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn perspective_sessions(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<ListEnvelope<crate::client::sessions::PerspectiveSession>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn vision_clients(
            &self,
            _query: &crate::client::query::ListQuery,
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
        async fn logs_download(&self) -> Result<crate::client::logs::LogDownload, CoreError> {
            unreachable!("not part of this action")
        }
        async fn loggers(
            &self,
            _query: &crate::client::query::ListQuery,
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
        async fn projects(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::projects::ProjectRecord>,
            CoreError,
        > {
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
        async fn project_resources(
            &self,
            _project: &str,
            _prefix: Option<&str>,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::resources::ResourceEntry>,
            CoreError,
        > {
            unreachable!("not part of this action")
        }
        async fn project_resource_get(
            &self,
            _project: &str,
            _path: &str,
        ) -> Result<crate::client::resources::ResourceContent, CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_resource_put(
            &self,
            _project: &str,
            _path: &str,
            _body: Vec<u8>,
            _content_type: &str,
        ) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_resource_delete(
            &self,
            _project: &str,
            _path: &str,
        ) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
    }

    /// Two pages then silence under a short deadline: entries arrive in
    /// TIMESTAMP order through the sink, the cursor advances past each
    /// page's max (next query's startTime = max + 1), and the deadline
    /// expiry ends the tail CLEANLY (Ok, exit 0 semantics).
    #[tokio::test]
    async fn tail_streams_pages_in_order_and_ends_cleanly_on_deadline() {
        let rig = TailRig {
            pages: Mutex::new(
                vec![
                    // Deliberately out of order WITHIN the page: client-side
                    // sort must fix the stream order.
                    vec![entry(1010, "second"), entry(1005, "first")],
                    vec![entry(1022, "third"), entry(1018, "wait, also")],
                ]
                .into(),
            ),
            queries: Mutex::new(Vec::new()),
        };

        let mut received: Vec<(i64, String)> = Vec::new();
        let sink: &mut dyn FnMut(&LogEntry) = &mut |entry: &LogEntry| {
            received.push((entry.timestamp, entry.message.clone()));
        };

        let result = tail(
            &rig,
            None,
            None,
            Some(1000), // since → first query startTime = 1000
            Duration::from_millis(5),
            Some(Duration::from_millis(40)),
            sink,
        )
        .await
        .expect("deadline expiry ends the tail cleanly");

        // Entries delivered in timestamp order across BOTH pages.
        assert_eq!(
            received,
            vec![
                (1005, "first".into()),
                (1010, "second".into()),
                (1018, "wait, also".into()),
                (1022, "third".into()),
            ],
            "stream order is timestamp order (client-side sort)"
        );
        assert_eq!(result.streamed, 4);

        // Cursor discipline: first query starts at `since` exactly;
        // after page 1 (max 1010) the next startTime = 1011; after
        // page 2 (max 1022) the next startTime = 1023 (then silence).
        let queries = rig.queries.lock().unwrap();
        assert_eq!(queries[0].start_time, Some(1000), "first = since");
        assert!(
            queries.len() >= 3,
            "polled again after each page: {}",
            queries.len()
        );
        assert_eq!(queries[1].start_time, Some(1011), "cursor = max + 1");
        assert!(
            queries[2].start_time == Some(1023),
            "cursor advanced past page 2: {:?}",
            queries[2].start_time
        );
        // Every query carries the explicit limit (Pitfall 9).
        assert!(queries.iter().all(|query| query.limit == 200));
    }

    /// Auth failures surface immediately — the tail never retries a
    /// rejected token (the poll engine's never-retry rule, proven at
    /// the action seam).
    #[tokio::test]
    async fn tail_fails_fast_on_auth() {
        struct AuthRig;
        #[async_trait::async_trait]
        impl GatewayApi for AuthRig {

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
            async fn logs(&self, _filter: &LogQuery) -> Result<ListEnvelope<LogEntry>, CoreError> {
                Err(CoreError::Auth {
                    status: 401,
                    endpoint: Some("http://gw/data/api/v1/logs".into()),
                })
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
                _query: &crate::client::query::ListQuery,
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
            async fn metrics_threads(
                &self,
            ) -> Result<crate::client::metrics::ThreadCounts, CoreError> {
                unreachable!("not part of this action")
            }
            async fn designers(
                &self,
                _query: &crate::client::query::ListQuery,
            ) -> Result<ListEnvelope<crate::client::sessions::DesignerInfo>, CoreError>
            {
                unreachable!("not part of this action")
            }
            async fn perspective_sessions(
                &self,
                _query: &crate::client::query::ListQuery,
            ) -> Result<ListEnvelope<crate::client::sessions::PerspectiveSession>, CoreError>
            {
                unreachable!("not part of this action")
            }
            async fn vision_clients(
                &self,
                _query: &crate::client::query::ListQuery,
            ) -> Result<ListEnvelope<crate::client::sessions::VisionClient>, CoreError>
            {
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
            async fn logs_download(&self) -> Result<crate::client::logs::LogDownload, CoreError> {
                unreachable!("not part of this action")
            }
            async fn loggers(
                &self,
                _query: &crate::client::query::ListQuery,
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
            async fn projects(
                &self,
                _query: &crate::client::query::ListQuery,
            ) -> Result<
                crate::client::query::ListEnvelope<crate::client::projects::ProjectRecord>,
                CoreError,
            > {
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
            async fn project_resources(
                &self,
                _project: &str,
                _prefix: Option<&str>,
            ) -> Result<
                crate::client::query::ListEnvelope<crate::client::resources::ResourceEntry>,
                CoreError,
            > {
                unreachable!("not part of this action")
            }
            async fn project_resource_get(
                &self,
                _project: &str,
                _path: &str,
            ) -> Result<crate::client::resources::ResourceContent, CoreError> {
                unreachable!("not part of this action")
            }
            async fn project_resource_put(
                &self,
                _project: &str,
                _path: &str,
                _body: Vec<u8>,
                _content_type: &str,
            ) -> Result<(), CoreError> {
                unreachable!("not part of this action")
            }
            async fn project_resource_delete(
                &self,
                _project: &str,
                _path: &str,
            ) -> Result<(), CoreError> {
                unreachable!("not part of this action")
            }
        }

        let sink: &mut dyn FnMut(&LogEntry) = &mut |_| {};
        let err = tail(
            &AuthRig,
            None,
            None,
            None,
            Duration::from_millis(5),
            Some(Duration::from_secs(5)),
            sink,
        )
        .await
        .expect_err("auth must fail fast");
        assert!(matches!(err, CoreError::Auth { status: 401, .. }));
        assert_eq!(err.exit_code(), 5);
    }
}
