//! The gateway HTTP seam: a coarse [`GatewayApi`] trait so actions never
//! touch reqwest types, plus the production [`ReqwestGatewayApi`].
//!
//! LOCKED: the trait uses `async_trait` (research Open Question 2,
//! resolved) — dyn-compatible today, ubiquitous. The trait stays COARSE —
//! one method per capability, not per endpoint — so Phase 2 grows it
//! without churn.
//!
//! Auth-header rule (verified against a live 8.3.6 gateway, 02-RESEARCH
//! §Auth Model): a token credential sends `X-Ignition-API-Token`; a basic
//! credential sends `Authorization: Basic <b64>`; NEVER both — enforced by
//! a match in [`ReqwestGatewayApi::apply_auth`], the ONE place
//! [`Secret::expose`] is called outside the secret module (the
//! grep-auditable redaction boundary; CORE-02).
//!
//! Basic is loudly demoted there: valid Basic credentials → 401 on every
//! 8.3 `/data` route (verified), so each use warns — never silently
//! retried. Note gateway-info itself DOES require auth under 8.3 default
//! security (header-less → 401, re-verified live 2026-08-21 — the 83-api
//! collection's `auth: none` tag does not hold); a `None` credential
//! proceeds header-less and classifies the answer.
//!
//! Redirects are never followed (`Policy::none()`): an uncommissioned
//! gateway 302s EVERYTHING to `/welcome` and the default follow would
//! render the wizard's HTML as a 200 (02-RESEARCH Pitfall 6). The 3xx is
//! classified by [`classify`] instead.
//!
//! Every request runs the pipeline: build URL → apply auth (opt-in) →
//! send (transport error → `Network`) → [`classify`] → parse the body.
//! Nothing ever calls `.json()` on a response that skipped `classify()`.

use std::path::Path;
use std::time::Duration;

pub mod backup;
mod classify;
pub mod connections;
pub mod eam;
pub mod idp;
pub mod logs;
pub mod metrics;
pub mod projects;
pub mod query;
pub mod resources;
pub mod restart;
pub mod sessions;
pub mod status;
pub mod tags;
pub mod trial;
pub mod version;
pub mod webdev;

use crate::client::connections::GatewayConnection;
use crate::client::eam::{EamHistoryItem, EamTaskRecord};
use crate::client::logs::{LogDownload, LogEntry, LogQuery, LoggerInfo};
use crate::client::metrics::{CurrentGauges, PerformanceCharts, ThreadCounts};
use crate::client::projects::{
    ExportMeta, ImportOutcome, ProjectCopy, ProjectCreate, ProjectModify, ProjectRecord,
    ProjectRenameBody,
};
use crate::client::query::ListEnvelope;
use crate::client::restart::SecurityProperties;
use crate::client::sessions::{DesignerInfo, PerspectiveSession, VisionClient};
use crate::client::status::{ModuleInfo, Overview, StatusPing};
use crate::client::tags::{TagProviderCreate, TagProviderRecord};
use crate::client::trial::{BannerSet, TrialWire};
use crate::client::version::GatewayInfo;
use crate::client::webdev::{RouteBody, RouteProbe};
use crate::config::{Credential, Profile};
use crate::error::CoreError;

/// GET path of the gateway-info capability.
const GATEWAY_INFO_PATH: &str = "/data/api/v1/gateway-info";

/// One capability per method — coarse on purpose. Phase 2 adds status,
/// modules, metrics, … as methods here; actions never see reqwest types.
///
/// (All impl bodies live in the ONE `impl GatewayApi for
/// ReqwestGatewayApi` block below: Rust rejects a second impl block of
/// the same trait for the same type, so the per-capability files own the
/// models + verified path constants and this block owns the delegation.)
#[async_trait::async_trait]
pub trait GatewayApi: Send + Sync {
    /// Fetch `/data/api/v1/gateway-info`.
    async fn gateway_info(&self) -> Result<GatewayInfo, CoreError>;
    /// Fetch `/data/api/v1/overview` (authed) — platform + runtime.
    async fn overview(&self) -> Result<Overview, CoreError>;
    /// Fetch `/StatusPing` **header-less** (auth=false) — the
    /// unauthenticated readiness anchor: it must keep answering when
    /// credentials are broken or absent and mid-restart (02-02).
    async fn status_ping(&self) -> Result<StatusPing, CoreError>;
    /// Fetch `/data/api/v1/modules/healthy` (`quarantined = false`) or
    /// `/modules/quarantined` (`true`) with the standard list params.
    async fn modules(
        &self,
        quarantined: bool,
        query: &query::ListQuery,
    ) -> Result<ListEnvelope<ModuleInfo>, CoreError>;
    /// Fetch `/data/api/v1/systemPerformance/currentGauges` (authed) —
    /// cpu in PERCENT (contrast [`Overview::cpu`], a 0–1 fraction).
    async fn metrics_current(&self) -> Result<CurrentGauges, CoreError>;
    /// Fetch `/data/api/v1/systemPerformance/charts` (authed) — historic
    /// cpu/heap/non-heap datapoints (epoch-ms timestamps).
    async fn metrics_historic(&self) -> Result<PerformanceCharts, CoreError>;
    /// Fetch `/data/api/v1/systemPerformance/threads` (authed) — thread
    /// execution counts (running/waiting/timedWaiting/blocked).
    async fn metrics_threads(&self) -> Result<ThreadCounts, CoreError>;
    /// Fetch `/data/api/v1/designers` (authed) — active Designer
    /// sessions (02-03, HLTH-08).
    async fn designers(
        &self,
        query: &query::ListQuery,
    ) -> Result<ListEnvelope<DesignerInfo>, CoreError>;
    /// Fetch `/data/perspective/api/v1/sessions/` (authed) — the EXACT
    /// trailing slash is the contract (Pitfall 8; module-scoped prefix).
    async fn perspective_sessions(
        &self,
        query: &query::ListQuery,
    ) -> Result<ListEnvelope<PerspectiveSession>, CoreError>;
    /// Fetch `/data/vision/api/v1/clients` (authed) — active Vision
    /// clients (designer shape + `tagCount`).
    async fn vision_clients(
        &self,
        query: &query::ListQuery,
    ) -> Result<ListEnvelope<VisionClient>, CoreError>;
    /// DELETE `/data/perspective/api/v1/sessions?sessionId=<id>` (+ an
    /// optional `message` shown to the session's user) — NO trailing
    /// slash on the DELETE (spec). Audit-logged server-side.
    async fn terminate_perspective_session(
        &self,
        id: &str,
        message: Option<&str>,
    ) -> Result<(), CoreError>;
    /// DELETE `/data/vision/api/v1/client/{id}` — terminate a Vision
    /// client. Audit-logged server-side.
    async fn terminate_vision_client(&self, id: &str) -> Result<(), CoreError>;
    /// DELETE `/data/api/v1/designer/{id}` — prune a Designer session.
    /// Audit-logged server-side.
    async fn prune_designer(&self, id: &str) -> Result<(), CoreError>;
    /// Fetch `/data/api/v1/resources/list/ignition/database-connection`
    /// (authed) — the web UI's Connections→Databases poll (HLTH-05).
    /// `healthchecks` is raw passthrough (LOW-confidence populated
    /// shape, research Open Question 1).
    async fn database_connections(&self) -> Result<ListEnvelope<GatewayConnection>, CoreError>;
    /// Fetch `/data/api/v1/resources/list/ignition/opc-connection`
    /// (authed) — the Connections→OPC poll (HLTH-06), same family.
    async fn opc_connections(&self) -> Result<ListEnvelope<GatewayConnection>, CoreError>;
    /// Fetch `/data/api/v1/logs` (authed) with [`LogQuery`] — the tail
    /// primitive: `startTime` (epoch ms) is the cursor, no server push
    /// exists (02-04, HLTH-03). The query ALWAYS carries an explicit
    /// `limit` (Pitfall 9 — the server default is unlimited).
    async fn logs(&self, filter: &LogQuery) -> Result<ListEnvelope<LogEntry>, CoreError>;
    /// GET `/data/api/v1/logs/download` (authed, per-request 120 s
    /// timeout) — a SQLite `.idb` archive, returned byte-for-byte with
    /// the `Content-Disposition` filename and `Content-Type`. NEVER
    /// zipped/extracted (Pitfall 7; Don't-Hand-Roll table).
    async fn logs_download(&self) -> Result<LogDownload, CoreError>;
    /// Fetch `/data/api/v1/logs/loggers` (authed) — the logger registry
    /// (HLTH-04; ~1250 loggers on a fresh gateway).
    async fn loggers(
        &self,
        query: &query::ListQuery,
    ) -> Result<ListEnvelope<LoggerInfo>, CoreError>;
    /// POST `/data/api/v1/logs/loggers/{loggerName}?level=X` (authed,
    /// empty body, NO CSRF — verified: token mutations need none).
    /// Logger names are Java identifiers `[A-Za-z0-9._]` — URL-safe,
    /// embedded as-is. Audit-logged server-side.
    async fn set_logger_level(&self, logger: &str, level: &str) -> Result<(), CoreError>;
    /// POST `/data/api/v1/logs/levelreset` (authed, empty body) — reset
    /// all custom logger levels to defaults. Audit-logged server-side.
    async fn reset_logger_levels(&self) -> Result<(), CoreError>;
    /// POST `/data/api/v1/restart-tasks/restart?confirm=true` (authed,
    /// empty body, NO CSRF — token mutations need none) — the one big
    /// red button. The gateway answers 200 with the literal body `true`
    /// almost immediately; the ~40 s wait is poller-side (02-05's
    /// `restart --wait` owns it). Audit-logged server-side.
    async fn restart(&self) -> Result<(), CoreError>;
    /// POST `/data/api/v1/scan/projects` (authed) — the harmless
    /// project-rescan write probe (`ign doctor --check-write`; 2xx =
    /// write permission, 403 = read-only token).
    async fn scan_projects(&self) -> Result<(), CoreError>;
    /// GET `/data/api/v1/resources/ignition/security-properties`
    /// (authed) — the security config singleton; the doctor's
    /// permissions deep-dive surfaces `readPermissions`/
    /// `writePermissions` verbatim (passthrough shape).
    async fn security_properties(&self) -> Result<SecurityProperties, CoreError>;
    /// GET `/system/webdev/<route>` (authed) reporting the RAW HTTP
    /// status — the doctor's route-presence probe (404 = absent;
    /// 200/401/403 = exists). Deliberately NOT classified: presence
    /// IS the answer; only transport failures are errors.
    async fn webdev_route_status(&self, route: &str) -> Result<u16, CoreError>;
    /// POST `/system/webdev/{project}/cli/{route}` (authed + any
    /// caller headers — scriptExec's secret gate) with the action
    /// JSON. classify() runs for transport/status errors, BUT the
    /// 200 BODY is the route envelope `{ok, data|error}` — WebDev
    /// IGNORES `status`, so denials ride HTTP 200: `ok:false` maps
    /// `error.code` onto the taxonomy (05-03), `ok:true` returns
    /// `data`. HTTP 200 alone is NEVER a success verdict.
    async fn webdev_route_call(
        &self,
        project: &str,
        route: &str,
        body: &serde_json::Value,
        extra_headers: &[(&str, &str)],
    ) -> Result<serde_json::Value, CoreError>;
    /// POST the route's `{"action":"version"}` handshake and
    /// discriminate ([`webdev::RouteProbe`]): 200-body-ok →
    /// `Present{route_version}`, 405 → `Absent` (the live-proven 8.3
    /// marker — NOT 404), 402 → `Unlicensed`, 401/403 → `AuthGated`,
    /// 200-body-denial → `Denied{code,message}`. Deliberately NOT
    /// classified — the status code IS the answer (the
    /// `webdev_route_status` precedent); only transport failures and
    /// shapes the enum has no variant for (wizard redirects, 503
    /// restarts, foreign 404s) are errors.
    async fn webdev_route_probe(
        &self,
        project: &str,
        route: &str,
        extra_headers: &[(&str, &str)],
    ) -> Result<RouteProbe, CoreError>;
    /// GET `/data/api/v1/projects/list` (authed) — every RUNNABLE
    /// project with inheritance info from the items themselves
    /// (PROJ-01; standard list params, `limit=-1` UI convention).
    async fn projects(
        &self,
        query: &query::ListQuery,
    ) -> Result<ListEnvelope<ProjectRecord>, CoreError>;
    /// GET `/data/api/v1/projects/find/{name}` (authed, name
    /// percent-encoded per segment) — one project's full record; 404 →
    /// `NotFound` via classify (this doubles as 03-02's collision
    /// pre-check).
    async fn project_find(&self, name: &str) -> Result<ProjectRecord, CoreError>;
    /// POST `/data/api/v1/projects` (authed, JSON body) — create. Ok
    /// classification IS the success contract (create's response body
    /// is unverified LOW — the restart `literal true` precedent;
    /// callers that want data re-`find`). Audit-logged server-side.
    async fn project_create(&self, body: &ProjectCreate) -> Result<(), CoreError>;
    /// POST `/data/api/v1/projects/copy` (authed, body exactly
    /// `{"fromName":…,"toName":…}`) — an exact copy of all resources.
    /// Audit-logged server-side.
    async fn project_copy(&self, from: &str, to: &str) -> Result<(), CoreError>;
    /// POST `/data/api/v1/projects/rename/{name}` (authed, body
    /// `{"name": "<new>"}`) — native rename, NOT copy+delete.
    /// Audit-logged server-side.
    async fn project_rename(&self, name: &str, new_name: &str) -> Result<(), CoreError>;
    /// PUT `/data/api/v1/projects/{name}` (authed, JSON body WITHOUT
    /// `name`) — modify/reparent (`set --parent` IS the inheritance
    /// move). Audit-logged server-side.
    async fn project_modify(&self, name: &str, body: &ProjectModify) -> Result<(), CoreError>;
    /// DELETE `/data/api/v1/projects/{name}?confirm=true` (authed,
    /// empty body) — the server's own confirmation guard rides the
    /// QUERY string (Pitfall 8: BOTH layers, always — the CLI's
    /// `--yes` and the wire's `confirm=true`). Audit-logged
    /// server-side.
    async fn project_delete(&self, name: &str) -> Result<(), CoreError>;
    /// GET `/data/api/v1/projects/export/{name}` (authed, per-request
    /// [`projects::PROJECT_EXPORT_TIMEOUT`] = 120 s) — the project ZIP
    /// STREAMED to `out` chunk-by-chunk via `bytes_stream` (Pitfall 2:
    /// NO `Vec<u8>` accumulation anywhere), with the disposition
    /// filename + byte count in the meta. Audit-relevant only as a
    /// read (exports never mutate).
    async fn project_export_to_file(&self, name: &str, out: &Path)
    -> Result<ExportMeta, CoreError>;
    /// POST `/data/api/v1/projects/import/{name}?overwrite=<bool>`
    /// (authed, per-request [`projects::PROJECT_IMPORT_TIMEOUT`] =
    /// 300 s) — the ZIP as the RAW body with `Content-Type:
    /// application/zip` and a known `Content-Length` (a `Vec<u8>`
    /// sidesteps the chunked-encoding question entirely — Pitfall 3's
    /// timeout is handled by the override). Synchronous, no job IDs
    /// (verified). Audit-logged server-side.
    async fn project_import(
        &self,
        name: &str,
        zip: Vec<u8>,
        overwrite: bool,
    ) -> Result<ImportOutcome, CoreError>;
    /// GET `/data/api/v1/resources/list/ignition/tag-provider`
    /// (authed) — the tag-provider resource list: full records
    /// incl. `config`, `metrics.tagCount`, `healthchecks.status`
    /// (05-04, TAGS-01 — the NATIVE provider seam; no deployed
    /// route involved). Standard list params (limit=-1, the UI
    /// convention).
    async fn tag_provider_list(
        &self,
        query: &query::ListQuery,
    ) -> Result<ListEnvelope<TagProviderRecord>, CoreError>;
    /// GET `/data/api/v1/resources/find/ignition/tag-provider/{name}`
    /// (authed, name percent-encoded per segment) — one provider's
    /// full record incl. the `signature` the chained delete needs.
    /// 404 → `NotFound` via classify.
    async fn tag_provider_find(&self, name: &str) -> Result<TagProviderRecord, CoreError>;
    /// POST `/data/api/v1/resources/ignition/tag-provider` (authed)
    /// with a JSON **ARRAY** body of create records — the
    /// live-proven create shape (05-RESEARCH provider table).
    /// Audit-logged server-side.
    async fn tag_provider_create(&self, body: &[TagProviderCreate]) -> Result<(), CoreError>;
    /// DELETE `/data/api/v1/resources/ignition/tag-provider/{name}/{signature}`
    /// (authed, both segments percent-encoded) — delete-by-signature;
    /// the signature comes from find. Audit-logged server-side.
    async fn tag_provider_delete(&self, name: &str, signature: &str) -> Result<(), CoreError>;
    /// GET `/data/api/v1/trial` — the trial state, live-verified
    /// UNAUTHENTICATED on 8.3.3 + 8.3.6 (both trial states): auth
    /// headers ride ONLY when the client carries a credential (fresh
    /// rigs have none — the version-command degradation precedent,
    /// rig-family edition).
    async fn trial_status_wire(&self) -> Result<TrialWire, CoreError>;
    /// GET `/data/api/v1/overview/banners` — the trial cross-check
    /// (severity/expireTime semantics, Pitfall 7). Same conditional
    /// auth as [`Self::trial_status_wire`].
    async fn banners(&self) -> Result<BannerSet, CoreError>;
    /// POST `/data/api/v1/trial` (authed, empty body) — the trial
    /// RESET, tier 0 of the ladder: a token credential plausibly
    /// satisfies it without CSRF (token mutations need none — the
    /// restart/set-logger precedent). The 2xx body IS the fresh
    /// [`TrialWire`] (live-observed). NOTE (live-discovered state
    /// gate): the gateway 403s resets on a NON-expired trial — the
    /// action layer pre-checks expiry.
    async fn trial_reset_wire(&self) -> Result<TrialWire, CoreError>;
    /// GET `/data/api/v1/backup?type={roaming|all}` (authed,
    /// [`backup::BACKUP_TIMEOUT`] = 300 s, `Accept:
    /// application/octet-stream`) — the portable gwbk STREAMED to
    /// `out` chunk-by-chunk through the 03-02 `download_to_file`
    /// pipeline (the ONE streaming body-consumption site — never a
    /// `Vec<u8>`, Pitfall 2). Byte count + metadata ride out in
    /// [`ExportMeta`] (04-04, RIG-04; 07-02 param-ized the type —
    /// `Roaming` stays the caller default).
    async fn backup_download(
        &self,
        out: &Path,
        backup_type: backup::BackupType,
    ) -> Result<ExportMeta, CoreError>;
    /// POST `/data/api/v1/backup` (authed, [`backup::BACKUP_TIMEOUT`]
    /// = 300 s) — the RESTORE: the gwbk bytes as a RAW
    /// `application/octet-stream` body (NOT multipart — the postman
    /// collection's exact shape) with the four scope params EXPLICIT
    /// on the query string. Synchronous AND followed by a gateway
    /// restart (Pitfall 6): the 2xx means the restore was ACCEPTED —
    /// the actions layer owns the post-restore RUNNING wait. The
    /// upload direction buffers by design (the import precedent).
    async fn backup_restore(&self, gwbk: &Path) -> Result<(), CoreError>;
    /// GET `/data/eam/api/v1/eam-tasks/history` (authed) — task run
    /// history, the standard `{items, metadata}` envelope. `limit`
    /// defaults to [`eam::EAM_HISTORY_DEFAULT_LIMIT`] (200 — EAM
    /// history grows unboundedly; an explicit limit ALWAYS rides the
    /// wire, the logs discipline). A stock (non-controller) gateway
    /// 403s → [`CoreError::EamNotController`] via classify
    /// (path-scoped message classification — never a misleading
    /// `auth_rejected`).
    async fn eam_task_history(
        &self,
        limit: Option<u32>,
        search: Option<&str>,
    ) -> Result<ListEnvelope<EamHistoryItem>, CoreError>;
    /// GET `/data/api/v1/resources/list/com.inductiveautomation.eam/
    /// eam-tasks` (authed) — task DEFINITIONS through the standard
    /// config-resource family (the tag-provider pattern; available
    /// on stock gateways — no controller needed for definitions).
    async fn eam_task_definitions(&self) -> Result<ListEnvelope<EamTaskRecord>, CoreError>;
    /// GET `/data/api/v1/resources/find/com.inductiveautomation.eam/
    /// eam-tasks/{name}` (authed) — one definition's full record
    /// incl. the `scheduledTaskState` healthcheck
    /// (`currentState`/`nextScheduled`/`owner` under `details`) and
    /// the mutation `signature`. 404 → `NotFound` via classify.
    async fn eam_task_find(&self, name: &str) -> Result<EamTaskRecord, CoreError>;
    /// POST `/data/api/v1/resources/com.inductiveautomation.eam/
    /// eam-tasks` (authed) with a JSON **ARRAY** body of one
    /// definition record — the config-resource create shape (the
    /// tag-provider precedent). Ok classification IS the success
    /// contract (create's response body is unverified — the
    /// project-create precedent; callers that want data re-find).
    /// Audit-logged server-side.
    async fn eam_task_create(&self, definition: &serde_json::Value) -> Result<(), CoreError>;
    /// POST `/data/eam/api/v1/eam-tasks/force/{owner}/{name}` (authed,
    /// empty body) — dispatch a task NOW. Live-proven success shape:
    /// **204** (any 2xx is done — the route-status style; execution
    /// OUTCOMES surface later in history as data, never on this
    /// response). Runtime seam: the controller gate classifies.
    async fn eam_task_force(&self, owner: &str, name: &str) -> Result<(), CoreError>;
}

/// Production [`GatewayApi`] over reqwest.
pub struct ReqwestGatewayApi {
    base: url::Url,
    credential: Option<Credential>,
    client: reqwest::Client,
}

impl ReqwestGatewayApi {
    /// Build from a resolved profile (post env-overlay — the dispatch site
    /// owns that precedence) and an optional credential (`None` = proceed
    /// header-less; the gateway's answer is then classified — 401 under
    /// 8.3 default security).
    ///
    /// Timeouts: 10s connect / 30s overall (per-class refinements land in
    /// Phase 2). `ssl_verify = false` accepts invalid certs — dev-rig
    /// only, per-profile, never global.
    pub fn new(profile: &Profile, credential: Option<Credential>) -> Result<Self, CoreError> {
        let client = build_client(profile.ssl_verify)?;
        Ok(Self {
            base: profile.url.clone(),
            credential,
            client,
        })
    }

    /// Test constructor: base URL + credential, no profile needed.
    pub fn for_tests(base_url: &str, credential: Option<Credential>) -> Self {
        Self {
            base: url::Url::parse(base_url).expect("test base URL parses"),
            credential,
            client: build_client(true).expect("test client builds"),
        }
    }

    /// The full request URL for `path` (bases are normalized to a trailing
    /// slash; an absolute path replaces from root).
    fn url_for(&self, path: &str) -> url::Url {
        self.base.join(path).expect("base joins an absolute path")
    }

    /// The auth-header rule in ONE place: token XOR basic XOR neither — a
    /// match, not if/if-else chains. [`Secret::expose`] is called at
    /// exactly this site (the redaction boundary MOVED here in 02-01, not
    /// duplicated).
    ///
    /// Basic carries a loud demotion warning: it cannot authenticate 8.3
    /// `/data` routes (verified: valid commissioned credentials → 401) —
    /// warn once per call, never silently retry (02-RESEARCH Auth §2).
    fn apply_auth(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let mut request = request;
        match &self.credential {
            Some(Credential::Token(token)) => {
                request = request.header("X-Ignition-API-Token", token.expose());
            }
            Some(Credential::Basic(user, password)) => {
                tracing::warn!(
                    "Basic auth does not authenticate Ignition 8.3 /data routes \
                     (verified: valid credentials → 401); use an API token"
                );
                request = request.basic_auth(user.expose(), Some(password.expose()));
            }
            None => {}
        }
        request
    }

    /// GET `path` (with pre-built query `pairs` when given) → classify →
    /// deserialize into `T`. `auth = false` fetches header-less (the
    /// `/StatusPing` readiness probe, 02-02 — it must work with broken
    /// credentials). Callers build pairs via `to_query_pairs()` so the
    /// param-name mapping stays in the capability files.
    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        pairs: Option<&[(String, String)]>,
        auth: bool,
    ) -> Result<T, CoreError> {
        let url = self.url_for(path);
        let mut request = self.client.get(url.clone());
        if let Some(pairs) = pairs {
            request = request.query(&pairs);
        }
        if auth {
            request = self.apply_auth(request);
        }
        let response = self.send_and_classify(request, &url).await?;
        response.json::<T>().await.map_err(|err| {
            CoreError::Internal(format!(
                "response from {url} did not match the expected shape: {err}"
            ))
        })
    }

    /// GET `path` → classify → read the response as BYTES plus the
    /// `Content-Disposition` filename and `Content-Type` — the
    /// archive-download pipeline (02-04). `timeout` overrides the 30 s
    /// client default PER REQUEST (a large `.idb` archive must not be
    /// truncated) — `RequestBuilder::timeout`, not a second client.
    async fn get_bytes(&self, path: &str, timeout: Duration) -> Result<LogDownload, CoreError> {
        let url = self.url_for(path);
        let request = self.client.get(url.clone()).timeout(timeout);
        let request = self.apply_auth(request);
        let response = self.send_and_classify(request, &url).await?;
        let filename = response
            .headers()
            .get(reqwest::header::CONTENT_DISPOSITION)
            .and_then(|value| value.to_str().ok())
            .and_then(logs::filename_from_content_disposition);
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let bytes = response.bytes().await.map_err(|err| CoreError::Network {
            url: url.to_string(),
            source: Some(err),
        })?;
        Ok(LogDownload {
            bytes: bytes.to_vec(),
            filename,
            content_type,
        })
    }

    /// GET `path` → classify → STREAM the body to `out` chunk-by-chunk
    /// — the file-download pipeline (03-02). The response body is
    /// consumed HERE, at a pipeline site, classify-first like every
    /// other: an error answer must classify (never stream), and on
    /// success each `bytes_stream()` chunk goes straight through
    /// `AsyncWriteExt::write_all` into a `tokio::fs::File` — NO
    /// `Vec<u8>` accumulation anywhere (Pitfall 2: a multi-hundred-MB
    /// export ZIP must not buffer in memory). The response metadata
    /// (`Content-Disposition` filename, `Content-Type`) and the
    /// chunk-counted byte total ride out in [`ExportMeta`]. Requires
    /// the workspace `reqwest` `stream` + `tokio` `fs` features (the
    /// research-flagged dep gap this plan closed).
    ///
    /// `accept` adds an OPTIONAL `Accept` header for the callers whose
    /// server contract names one (04-04's gwbk download sends
    /// `application/octet-stream`; the 03-02 export sends none) — a
    /// minimal parameterization that keeps THIS the one streaming
    /// site instead of forking a second copy of the chunk loop.
    async fn download_to_file(
        &self,
        path: &str,
        out: &Path,
        timeout: Duration,
        accept: Option<&str>,
    ) -> Result<ExportMeta, CoreError> {
        use futures_util::StreamExt;
        use tokio::io::AsyncWriteExt;

        let url = self.url_for(path);
        let mut request = self.client.get(url.clone()).timeout(timeout);
        if let Some(accept) = accept {
            request = request.header(reqwest::header::ACCEPT, accept);
        }
        let request = self.apply_auth(request);
        let response = self.send_and_classify(request, &url).await?;
        let filename = response
            .headers()
            .get(reqwest::header::CONTENT_DISPOSITION)
            .and_then(|value| value.to_str().ok())
            .and_then(logs::filename_from_content_disposition);
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);

        let mut file = tokio::fs::File::create(out).await.map_err(|err| {
            CoreError::Internal(format!("cannot create {}: {err}", out.display()))
        })?;
        let mut stream = response.bytes_stream();
        let mut bytes: u64 = 0;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|err| CoreError::Network {
                url: url.to_string(),
                source: Some(err),
            })?;
            file.write_all(&chunk).await.map_err(|err| {
                CoreError::Internal(format!("cannot write {}: {err}", out.display()))
            })?;
            bytes += chunk.len() as u64;
        }
        file.flush()
            .await
            .map_err(|err| CoreError::Internal(format!("cannot flush {}: {err}", out.display())))?;
        Ok(ExportMeta {
            filename,
            bytes,
            content_type,
        })
    }

    /// POST `path` with `pairs` as QUERY params and an empty body →
    /// classify → hand back the response (callers read `true`/JSON as
    /// their capability needs). Production callers since 02-04:
    /// `set_logger_level`, `reset_logger_levels` (and 02-05's restart
    /// with `confirm=true`). Token-auth POSTs need NO CSRF (verified
    /// 02-RESEARCH §Auth Model).
    async fn post_empty(
        &self,
        path: &str,
        pairs: &[(&str, String)],
        auth: bool,
    ) -> Result<reqwest::Response, CoreError> {
        let url = self.url_for(path);
        let mut request = self.client.post(url.clone()).query(pairs);
        if auth {
            request = self.apply_auth(request);
        }
        self.send_and_classify(request, &url).await
    }

    /// DELETE `path` with `pairs` as QUERY params (empty body) →
    /// classify → `Ok(())` on any classified success. Token-auth DELETEs
    /// need NO CSRF (verified 02-RESEARCH §Auth Model: CSRF is only for
    /// cookie/session auth); the classified bodies (`{terminated: N}`,
    /// `{message: …}`) are advisory — Ok classification IS the success
    /// contract.
    async fn delete_with_query(
        &self,
        path: &str,
        pairs: &[(&str, String)],
    ) -> Result<(), CoreError> {
        let url = self.url_for(path);
        let mut request = self.client.delete(url.clone()).query(pairs);
        request = self.apply_auth(request);
        self.send_and_classify(request, &url).await.map(|_| ())
    }

    /// POST `path` with a JSON body → classify → hand back the response
    /// (callers read the body as their capability needs; the project
    /// mutations treat Ok classification AS the success contract —
    /// those bodies are unverified LOW, the restart `literal true`
    /// precedent). Token-auth POSTs need NO CSRF (verified
    /// 02-RESEARCH §Auth Model). One of the two body-carrying pipeline
    /// helpers (03-01); serde serializes struct fields in declaration order, so
    /// recorded bodies are deterministic for the wiremock pins.
    async fn post_json<T: serde::Serialize + ?Sized>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<reqwest::Response, CoreError> {
        let url = self.url_for(path);
        let request = self.apply_auth(self.client.post(url.clone()).json(body));
        self.send_and_classify(request, &url).await
    }

    /// POST the action JSON to a webdev route with caller headers +
    /// auth applied, returning `(full URL, response)` — the shared
    /// head of the two webdev seam methods (05-03). NO classify here:
    /// `webdev_route_probe` reads the raw status (the code IS the
    /// answer); `webdev_route_call` classifies downstream. Transport
    /// failures map to `Network` like every pipeline.
    async fn webdev_post_raw(
        &self,
        project: &str,
        route: &str,
        body: &serde_json::Value,
        extra_headers: &[(&str, &str)],
    ) -> Result<(String, reqwest::Response), CoreError> {
        let path = webdev::route_url(project, route);
        let url = self.url_for(&path);
        let mut request = self.client.post(url.clone()).json(body);
        for (name, value) in extra_headers {
            request = request.header(*name, *value);
        }
        let request = self.apply_auth(request);
        let response = request.send().await.map_err(|err| CoreError::Network {
            url: url.to_string(),
            source: Some(err),
        })?;
        Ok((url.to_string(), response))
    }

    /// PUT `path` with a JSON body → classify → `Ok(())` (modify/
    /// reparent; resource puts in 03-03). Token-auth PUTs need NO
    /// CSRF. The classify-first rule holds: nothing consumes a body
    /// that skipped classify.
    async fn put_json<T: serde::Serialize + ?Sized>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<(), CoreError> {
        let url = self.url_for(path);
        let request = self.apply_auth(self.client.put(url.clone()).json(body));
        self.send_and_classify(request, &url).await.map(|_| ())
    }

    /// Send + transport-error mapping + [`classify`] — the shared tail of
    /// every pipeline helper. Transport failures (connect/timeout/TLS) →
    /// `Network` (exit 4); everything the gateway ANSWERED goes through
    /// the classifier.
    async fn send_and_classify(
        &self,
        request: reqwest::RequestBuilder,
        url: &url::Url,
    ) -> Result<reqwest::Response, CoreError> {
        let response = request.send().await.map_err(|err| CoreError::Network {
            url: url.to_string(),
            source: Some(err),
        })?;
        classify::classify(response, url.as_ref()).await
    }
}

fn build_client(ssl_verify: bool) -> Result<reqwest::Client, CoreError> {
    let mut builder = reqwest::Client::builder()
        // Never follow redirects: an uncommissioned gateway 302s everything
        // to /welcome and the follow would render the wizard HTML as a 200
        // (02-RESEARCH Pitfall 6). classify() maps the 3xx instead.
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30));
    if !ssl_verify {
        builder = builder.danger_accept_invalid_certs(true);
    }
    builder
        .build()
        .map_err(|err| CoreError::Internal(format!("cannot build HTTP client: {err}")))
}

#[async_trait::async_trait]
impl GatewayApi for ReqwestGatewayApi {
    async fn gateway_info(&self) -> Result<GatewayInfo, CoreError> {
        let mut info: GatewayInfo = self.get_json(GATEWAY_INFO_PATH, None, true).await?;
        info.endpoint = Some(self.url_for(GATEWAY_INFO_PATH).to_string());
        Ok(info)
    }

    async fn overview(&self) -> Result<Overview, CoreError> {
        self.get_json(status::OVERVIEW_PATH, None, true).await
    }

    async fn status_ping(&self) -> Result<StatusPing, CoreError> {
        // auth = false — the whole point: the readiness anchor must not
        // depend on credentials (pinned by the wiremock header-absence
        // proof in tests/status_contract.rs).
        self.get_json(status::STATUS_PING_PATH, None, false).await
    }

    async fn modules(
        &self,
        quarantined: bool,
        query: &query::ListQuery,
    ) -> Result<ListEnvelope<ModuleInfo>, CoreError> {
        let path = if quarantined {
            status::MODULES_QUARANTINED_PATH
        } else {
            status::MODULES_HEALTHY_PATH
        };
        self.get_json(path, Some(&query.to_query_pairs()), true)
            .await
    }

    async fn metrics_current(&self) -> Result<CurrentGauges, CoreError> {
        self.get_json(metrics::CURRENT_GAUGES_PATH, None, true)
            .await
    }

    async fn metrics_historic(&self) -> Result<PerformanceCharts, CoreError> {
        self.get_json(metrics::CHARTS_PATH, None, true).await
    }

    async fn metrics_threads(&self) -> Result<ThreadCounts, CoreError> {
        self.get_json(metrics::THREADS_PATH, None, true).await
    }

    async fn designers(
        &self,
        query: &query::ListQuery,
    ) -> Result<ListEnvelope<DesignerInfo>, CoreError> {
        self.get_json(
            sessions::DESIGNERS_PATH,
            Some(&query.to_query_pairs()),
            true,
        )
        .await
    }

    async fn perspective_sessions(
        &self,
        query: &query::ListQuery,
    ) -> Result<ListEnvelope<PerspectiveSession>, CoreError> {
        // The trailing slash is PART OF THE PATH (Pitfall 8) — url_for's
        // join preserves it; the exact-path wiremock matcher in
        // tests/sessions_contract.rs pins it.
        self.get_json(
            sessions::PERSPECTIVE_SESSIONS_LIST_PATH,
            Some(&query.to_query_pairs()),
            true,
        )
        .await
    }

    async fn vision_clients(
        &self,
        query: &query::ListQuery,
    ) -> Result<ListEnvelope<VisionClient>, CoreError> {
        self.get_json(
            sessions::VISION_CLIENTS_PATH,
            Some(&query.to_query_pairs()),
            true,
        )
        .await
    }

    async fn terminate_perspective_session(
        &self,
        id: &str,
        message: Option<&str>,
    ) -> Result<(), CoreError> {
        // sessionId is a QUERY param on the spec's DELETE route — never
        // a body (recorded-request proof in tests/sessions_contract.rs).
        let mut pairs = vec![("sessionId", id.to_string())];
        if let Some(message) = message {
            pairs.push(("message", message.to_string()));
        }
        self.delete_with_query(sessions::PERSPECTIVE_SESSIONS_TERMINATE_PATH, &pairs)
            .await
    }

    async fn terminate_vision_client(&self, id: &str) -> Result<(), CoreError> {
        self.delete_with_query(&sessions::vision_client_terminate_path(id), &[])
            .await
    }

    async fn prune_designer(&self, id: &str) -> Result<(), CoreError> {
        self.delete_with_query(&sessions::designer_prune_path(id), &[])
            .await
    }

    async fn database_connections(&self) -> Result<ListEnvelope<GatewayConnection>, CoreError> {
        // The UI polls the resource list with limit=-1 — same convention
        // as every other list capability.
        self.get_json(
            connections::DATABASE_CONNECTIONS_PATH,
            Some(&query::ListQuery::default().to_query_pairs()),
            true,
        )
        .await
    }

    async fn opc_connections(&self) -> Result<ListEnvelope<GatewayConnection>, CoreError> {
        self.get_json(
            connections::OPC_CONNECTIONS_PATH,
            Some(&query::ListQuery::default().to_query_pairs()),
            true,
        )
        .await
    }

    async fn logs(&self, filter: &LogQuery) -> Result<ListEnvelope<LogEntry>, CoreError> {
        // Explicit limit ALWAYS rides the wire (Pitfall 9) — enforced by
        // LogQuery::to_query_pairs, pinned by the contract test.
        self.get_json(logs::LOGS_PATH, Some(&filter.to_query_pairs()), true)
            .await
    }

    async fn logs_download(&self) -> Result<LogDownload, CoreError> {
        // Per-request timeout override: the 30 s client default would
        // truncate large archives (per-class timeout WITHOUT a second
        // client — RequestBuilder::timeout, 02-RESEARCH §Architecture).
        self.get_bytes(logs::LOGS_DOWNLOAD_PATH, Duration::from_secs(120))
            .await
    }

    async fn loggers(
        &self,
        query: &query::ListQuery,
    ) -> Result<ListEnvelope<LoggerInfo>, CoreError> {
        self.get_json(logs::LOGGERS_PATH, Some(&query.to_query_pairs()), true)
            .await
    }

    async fn set_logger_level(&self, logger: &str, level: &str) -> Result<(), CoreError> {
        // `level` rides the QUERY string against an EMPTY body (verified
        // live: 200 + the level flips; recorded-request proof in
        // tests/logs_contract.rs).
        self.post_empty(
            &logs::logger_set_path(logger),
            &[("level", level.to_string())],
            true,
        )
        .await
        .map(|_| ())
    }

    async fn reset_logger_levels(&self) -> Result<(), CoreError> {
        self.post_empty(logs::LEVEL_RESET_PATH, &[], true)
            .await
            .map(|_| ())
    }

    async fn restart(&self) -> Result<(), CoreError> {
        // `confirm=true` rides the QUERY string against an empty body
        // (the verified shape; recorded-request proof in
        // tests/restart_wait_contract.rs). Token-auth POSTs need no
        // CSRF (02-RESEARCH §Auth Model).
        let response = self
            .post_empty(
                restart::RESTART_PATH,
                &[("confirm", "true".to_string())],
                true,
            )
            .await?;
        // Success-shape drift guard: the verified body is the literal
        // `true`. Any other 2xx body still means the POST was accepted
        // — warn, don't fail (the wait half reports what happens next).
        let body = response.text().await.unwrap_or_default();
        if body.trim() != "true" {
            tracing::warn!(
                body = %body,
                "restart POST answered an unexpected 2xx body (expected the literal `true`)"
            );
        }
        Ok(())
    }

    async fn scan_projects(&self) -> Result<(), CoreError> {
        self.post_empty(restart::SCAN_PROJECTS_PATH, &[], true)
            .await
            .map(|_| ())
    }

    async fn security_properties(&self) -> Result<SecurityProperties, CoreError> {
        self.get_json(restart::SECURITY_PROPERTIES_PATH, None, true)
            .await
    }

    async fn webdev_route_status(&self, route: &str) -> Result<u16, CoreError> {
        // The raw-status probe: send, surface the status code, never
        // classify (404 vs 200/401/403 is the ANSWER, not an error).
        // Only transport failures (DNS/refused/timeout) error out.
        let path = restart::webdev_route_path(route);
        let url = self.url_for(&path);
        let request = self.apply_auth(self.client.get(url.clone()));
        let response = request.send().await.map_err(|err| CoreError::Network {
            url: url.to_string(),
            source: Some(err),
        })?;
        Ok(response.status().as_u16())
    }

    async fn webdev_route_call(
        &self,
        project: &str,
        route: &str,
        body: &serde_json::Value,
        extra_headers: &[(&str, &str)],
    ) -> Result<serde_json::Value, CoreError> {
        // classify() runs normally for transport/status errors; the
        // 200 BODY is then the route envelope — WebDev ignores
        // `status`, so denials ride HTTP 200 and the body verdict is
        // the ONLY success oracle (never the status line alone).
        let (url, response) = self
            .webdev_post_raw(project, route, body, extra_headers)
            .await?;
        let response = classify::classify(response, &url).await?;
        let text = response.text().await.unwrap_or_default();
        match webdev::parse_route_body(&text)? {
            RouteBody::Ok(data) => Ok(data),
            RouteBody::Denied {
                code,
                message,
                traceback,
            } => Err(webdev::denial_to_error(
                &code,
                &message,
                traceback.as_deref(),
                url,
            )),
        }
    }

    async fn webdev_route_probe(
        &self,
        project: &str,
        route: &str,
        extra_headers: &[(&str, &str)],
    ) -> Result<RouteProbe, CoreError> {
        // NOT classified — the status code IS the answer (the
        // webdev_route_status precedent): 405/402/401 discriminate
        // presence/licensing/gating, and a 200 body carries the
        // version handshake or the structured denial.
        let (url, response) = self
            .webdev_post_raw(
                project,
                route,
                &serde_json::json!({"action": "version"}),
                extra_headers,
            )
            .await?;
        let status = response.status();
        if status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return match webdev::parse_route_body(&text)? {
                RouteBody::Ok(data) => {
                    let route_version = data
                        .get("routeVersion")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string)
                        .ok_or_else(|| {
                            CoreError::Internal(format!(
                                "webdev route version action from {url} answered no routeVersion"
                            ))
                        })?;
                    Ok(RouteProbe::Present { route_version })
                }
                RouteBody::Denied {
                    code,
                    message,
                    traceback,
                } => Ok(RouteProbe::Denied {
                    code,
                    message,
                    traceback,
                }),
            };
        }
        match status.as_u16() {
            401 | 403 => Ok(RouteProbe::AuthGated),
            402 => Ok(RouteProbe::Unlicensed),
            405 => Ok(RouteProbe::Absent),
            // Shapes the enum has no variant for (wizard redirects,
            // mid-restart 503s, foreign 404s) — reuse classify's
            // status mappings verbatim; every non-success response
            // classifies to Err, and the Ok arm is unreachable by
            // construction (all 2xx took the body branch above).
            _ => match classify::classify(response, &url).await {
                Err(err) => Err(err),
                Ok(_) => Err(CoreError::Internal(format!(
                    "unexpected HTTP {status} from webdev route probe at {url}"
                ))),
            },
        }
    }

    async fn projects(
        &self,
        query: &query::ListQuery,
    ) -> Result<ListEnvelope<ProjectRecord>, CoreError> {
        // Standard list params (limit=-1 = the UI's "everything").
        self.get_json(
            projects::PROJECTS_LIST_PATH,
            Some(&query.to_query_pairs()),
            true,
        )
        .await
    }

    async fn project_find(&self, name: &str) -> Result<ProjectRecord, CoreError> {
        // The {name} segment is percent-encoded (Pitfall 6) — the
        // spaced-name recorded-request proof in tests/projects_contract.rs.
        self.get_json(&projects::project_find_path(name), None, true)
            .await
    }

    async fn project_create(&self, body: &ProjectCreate) -> Result<(), CoreError> {
        // Ok classification IS the success contract; callers that want
        // data re-`find` (the actions layer's read-back).
        self.post_json(projects::PROJECTS_CREATE_PATH, body)
            .await
            .map(|_| ())
    }

    async fn project_copy(&self, from: &str, to: &str) -> Result<(), CoreError> {
        let body = ProjectCopy {
            from_name: from.to_string(),
            to_name: to.to_string(),
        };
        self.post_json(projects::PROJECTS_COPY_PATH, &body)
            .await
            .map(|_| ())
    }

    async fn project_rename(&self, name: &str, new_name: &str) -> Result<(), CoreError> {
        let body = ProjectRenameBody {
            name: new_name.to_string(),
        };
        self.post_json(&projects::project_rename_path(name), &body)
            .await
            .map(|_| ())
    }

    async fn project_modify(&self, name: &str, body: &ProjectModify) -> Result<(), CoreError> {
        self.put_json(&projects::project_modify_path(name), body)
            .await
    }

    async fn project_delete(&self, name: &str) -> Result<(), CoreError> {
        // BOTH guard layers (Pitfall 8): the CLI already refused
        // without --yes (exit 2, pre-resolution) AND the wire request
        // always carries the server's own `confirm=true` query param
        // (wiremock recorded-request proof).
        self.delete_with_query(
            &projects::project_delete_path(name),
            &[("confirm", "true".to_string())],
        )
        .await
    }

    async fn project_export_to_file(
        &self,
        name: &str,
        out: &Path,
    ) -> Result<ExportMeta, CoreError> {
        // The 120 s per-request override rides the RequestBuilder (the
        // logs-download precedent); the streaming itself lives in
        // download_to_file (classify FIRST, then chunk loop). No
        // `Accept` header — the export contract never named one.
        self.download_to_file(
            &projects::project_export_path(name),
            out,
            projects::PROJECT_EXPORT_TIMEOUT,
            None,
        )
        .await
    }

    async fn project_import(
        &self,
        name: &str,
        zip: Vec<u8>,
        overwrite: bool,
    ) -> Result<ImportOutcome, CoreError> {
        // `overwrite` rides the QUERY string; the ZIP is the RAW body
        // with Content-Type application/zip and a known Content-Length
        // (Vec<u8> — chunked encoding never enters the picture). The
        // 300 s per-request override owns Pitfall 3. Token-auth POSTs
        // need no CSRF (02-RESEARCH §Auth Model).
        let url = self.url_for(&projects::project_import_path(name));
        let request = self
            .client
            .post(url.clone())
            .timeout(projects::PROJECT_IMPORT_TIMEOUT)
            .query(&[("overwrite", if overwrite { "true" } else { "false" })])
            .header(reqwest::header::CONTENT_TYPE, "application/zip")
            .body(zip);
        let request = self.apply_auth(request);
        let response = self.send_and_classify(request, &url).await?;
        // Opaque-success: parse the body when it is a JSON OBJECT,
        // else the fallback object (the body is unverified MEDIUM —
        // restart's `literal true` is the same family and normalizes
        // the same way, so agents always see a stable object shape).
        let body = response.text().await.unwrap_or_default();
        let parsed = serde_json::from_str::<serde_json::Value>(body.trim())
            .ok()
            .filter(|value| value.is_object())
            .unwrap_or_else(|| serde_json::json!({"status": "success"}));
        // Denial honesty (05-07, UAT Gap 1): the gateway refuses
        // imports over HTTP 200 with {success:false, problem} —
        // live-witnessed while NOTHING landed. ONE seam here fixes
        // every import caller at once (resource put/delete, project
        // import, webdev deploy) — per-caller checks are forbidden;
        // this IS the contract (the WebDev 200-denial precedent
        // applied to the import family).
        if let Some(problem) = projects::import_denied(&parsed) {
            return Err(CoreError::ImportDenied {
                project: name.to_string(),
                problem,
                endpoint: Some(url.to_string()),
            });
        }
        Ok(ImportOutcome { response: parsed })
    }

    async fn tag_provider_list(
        &self,
        query: &query::ListQuery,
    ) -> Result<ListEnvelope<TagProviderRecord>, CoreError> {
        // Standard list params (limit=-1 = the UI's "everything") —
        // the connections-family resource lists' exact shape.
        self.get_json(
            tags::TAG_PROVIDERS_LIST_PATH,
            Some(&query.to_query_pairs()),
            true,
        )
        .await
    }

    async fn tag_provider_find(&self, name: &str) -> Result<TagProviderRecord, CoreError> {
        self.get_json(&tags::tag_provider_find_path(name), None, true)
            .await
    }

    async fn tag_provider_create(&self, body: &[TagProviderCreate]) -> Result<(), CoreError> {
        // The ARRAY body is the wire contract (a bare object 400s);
        // serde serializes elements in declaration order so the
        // recorded body is deterministic. Ok classification IS the
        // success contract (the project-create precedent).
        self.post_json(tags::TAG_PROVIDERS_CREATE_PATH, body)
            .await
            .map(|_| ())
    }

    async fn tag_provider_delete(&self, name: &str, signature: &str) -> Result<(), CoreError> {
        // The signature rides the PATH (from find) — the
        // live-proven delete-by-signature chain; both segments
        // percent-encoded through the ONE locked encoder.
        self.delete_with_query(&tags::tag_provider_delete_path(name, signature), &[])
            .await
    }

    async fn trial_status_wire(&self) -> Result<TrialWire, CoreError> {
        // Conditional auth: the endpoints answer unauthenticated
        // (live-verified both rigs), so a header-less client degrades
        // cleanly — but a carried credential rides along harmlessly
        // (future-proofing if a gateway version starts gating them).
        let auth = self.credential.is_some();
        self.get_json(trial::TRIAL_PATH, None, auth).await
    }

    async fn banners(&self) -> Result<BannerSet, CoreError> {
        let auth = self.credential.is_some();
        self.get_json(trial::BANNERS_PATH, None, auth).await
    }

    async fn trial_reset_wire(&self) -> Result<TrialWire, CoreError> {
        // Empty body, authed POST (the UI mutation's exact shape —
        // decompiled ia-gateway.js: {method:"POST",
        // url:"/data/api/v1/trial"}). Token-auth POSTs need no CSRF
        // (02-RESEARCH §Auth Model); on 403 the tier-1 session+CSRF
        // flow takes over (actions layer owns the ladder).
        let response = self.post_empty(trial::TRIAL_PATH, &[], true).await?;
        let body = response.text().await.unwrap_or_default();
        serde_json::from_str(&body).map_err(|err| {
            CoreError::Internal(format!(
                "trial reset response did not match the trial shape: {err}"
            ))
        })
    }

    async fn backup_download(
        &self,
        out: &Path,
        backup_type: backup::BackupType,
    ) -> Result<ExportMeta, CoreError> {
        // Pure reuse: the type query rides the path builder, the
        // Accept header rides the helper's optional param, and the
        // 300 s class rides the RequestBuilder — the 03-02 chunk loop
        // stays THE one streaming body-consumption site (04-04).
        self.download_to_file(
            &backup::backup_download_path(backup_type),
            out,
            backup::BACKUP_TIMEOUT,
            Some(backup::BACKUP_ACCEPT),
        )
        .await
    }

    async fn backup_restore(&self, gwbk: &Path) -> Result<(), CoreError> {
        // The upload direction buffers BY DESIGN (the import
        // precedent: a known Content-Length raw body sidesteps the
        // chunked-encoding question entirely). Token-auth POSTs need
        // no CSRF (02-RESEARCH §Auth Model). Ok classification IS the
        // acceptance contract — the actions layer owns the
        // post-restore RUNNING wait (Pitfall 6: the gateway restarts
        // after answering).
        let body = tokio::fs::read(gwbk)
            .await
            .map_err(|err| CoreError::InvalidInput {
                reason: format!("cannot read {}: {err}", gwbk.display()),
            })?;
        let url = self.url_for(backup::BACKUP_PATH);
        let request = self
            .client
            .post(url.clone())
            .timeout(backup::BACKUP_TIMEOUT)
            .query(&backup::restore_query())
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(body);
        let request = self.apply_auth(request);
        self.send_and_classify(request, &url).await.map(|_| ())
    }

    async fn eam_task_history(
        &self,
        limit: Option<u32>,
        search: Option<&str>,
    ) -> Result<ListEnvelope<EamHistoryItem>, CoreError> {
        let query = query::ListQuery {
            limit: limit
                .map(i64::from)
                .unwrap_or(eam::EAM_HISTORY_DEFAULT_LIMIT),
            search: search.map(str::to_string),
            ..query::ListQuery::default()
        };
        self.get_json(eam::EAM_HISTORY_PATH, Some(&query.to_query_pairs()), true)
            .await
    }

    async fn eam_task_definitions(&self) -> Result<ListEnvelope<EamTaskRecord>, CoreError> {
        // Standard list params (limit=-1 = the UI's "everything") —
        // definition counts are small; the connections-family
        // resource lists' exact shape.
        self.get_json(
            &eam::eam_tasks_list_path(),
            Some(&query::ListQuery::default().to_query_pairs()),
            true,
        )
        .await
    }

    async fn eam_task_find(&self, name: &str) -> Result<EamTaskRecord, CoreError> {
        self.get_json(&eam::eam_task_find_path(name), None, true)
            .await
    }

    async fn eam_task_create(&self, definition: &serde_json::Value) -> Result<(), CoreError> {
        // The ARRAY body is the wire contract (a bare object 400s —
        // the tag-provider create precedent); the caller's composed
        // definition rides as the single element. Ok classification
        // IS the success contract.
        self.post_json(&eam::eam_tasks_create_path(), &[definition])
            .await
            .map(|_| ())
    }

    async fn eam_task_force(&self, owner: &str, name: &str) -> Result<(), CoreError> {
        // Empty body, authed POST — 204 is the live-proven success
        // shape; classify()'s 2xx pass-through IS the oracle (any
        // 2xx = dispatched; outcomes land in history as data).
        self.post_empty(&eam::eam_force_path(owner, name), &[], true)
            .await
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::ReqwestGatewayApi;

    /// Exercises `post_empty` end-to-end with the shape 02-04's
    /// set-logger-level route uses (query param + empty body): the
    /// verified restart shape is a 200 with literal body `true`,
    /// classified Ok — and the query param rides the request.
    #[tokio::test]
    async fn post_empty_sends_query_param_and_empty_body() {
        let server = wiremock::MockServer::start().await;
        let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path(
                "/data/api/v1/restart-tasks/restart",
            ))
            .and(wiremock::matchers::query_param("confirm", "true"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("true"))
            .expect(1)
            .mount_as_scoped(&server)
            .await;

        let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
        let response = api
            .post_empty(
                "/data/api/v1/restart-tasks/restart",
                &[("confirm", "true".to_string())],
                true,
            )
            .await
            .expect("200 classifies Ok");
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let requests = guard.received_requests().await;
        assert_eq!(requests.len(), 1);
        assert!(
            requests[0].body.is_empty(),
            "the POST carries NO body — params ride the query string"
        );
    }
}
