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

use std::time::Duration;

mod classify;
pub mod connections;
pub mod logs;
pub mod metrics;
pub mod query;
pub mod restart;
pub mod sessions;
pub mod status;
pub mod version;

use crate::client::connections::GatewayConnection;
use crate::client::logs::{LogDownload, LogEntry, LogQuery, LoggerInfo};
use crate::client::metrics::{CurrentGauges, PerformanceCharts, ThreadCounts};
use crate::client::query::ListEnvelope;
use crate::client::restart::SecurityProperties;
use crate::client::sessions::{DesignerInfo, PerspectiveSession, VisionClient};
use crate::client::status::{ModuleInfo, Overview, StatusPing};
use crate::client::version::GatewayInfo;
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
