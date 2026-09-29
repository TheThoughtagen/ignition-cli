//! Native OIDC login + CSRF flow (04-03, tier 1 of the trial-reset
//! ladder) — the internal IdP's challenge dance, live-probed during
//! 04-RESEARCH and **live-verified END-TO-END on 8.3.3 during this
//! plan's spike**: login → session → CSRF → `POST /data/api/v1/trial`
//! flipped `expired:true → false` (`trialSecondsLeft 0 → 7199`).
//!
//! ## The flow (all steps live-observed; the research's 10-step map,
//! with the two LOW-confidence deliverables now resolved live)
//!
//! 1. `GET /data/app/login` → 302 into `/idp/<name>/oidc/auth?…` —
//!    the IdP NAME is derived from this redirect (gateways rename the
//!    internal IdP; `default` is only the common case, `temp` was
//!    live-observed on 8.3.x)
//!    (+ `idp-relay-*` cookie)
//! 2. `GET /idp/<name>/oidc/auth?…` → 302 to
//!    `/idp/<name>/authn/login?…&token=<T0>` (+ `idp-sid-<name>-*`
//!    cookie)
//! 3. `POST /idp/<name>/authn/next-challenge` `{"token":T0}` →
//!    `{"complete":false,"nextChallenge":[…],"token":<T1>}` — **the
//!    token ROTATES on every call; thread it forward or the next call
//!    400s in Jetty HTML** (research Pitfall 2)
//! 4. `POST /idp/<name>/authn/submit-challenge/basic`
//!    `{"token":T1,"rememberMe":false,"challenge":{username,password}}`
//!    → `{"success":bool,"token":<T2>}`; `success:false` = rejected
//!    credentials (live-observed on 8.3.6 with a wrong password)
//! 5. `POST next-challenge {"token":T2}` → `{"complete":true,…,
//!    "token":<T3>}`
//! 6. `GET /idp/<name>/oidc/auth?<orig params>&token=<T3>` → 302 to
//!    `/data/federate/callback/internal?code&state`
//! 7. `GET /data/federate/callback/internal?…` → 302 `/app` +
//!    **`Set-Cookie: webui-sid-<gatewayId>=…`** (the session cookie —
//!    name RESOLVED LIVE; `Path=/; HttpOnly; SameSite=Strict`)
//! 8. `GET /data/app/session` (session cookie) →
//!    `{"userPayload":{…},"csrfToken":"…"}` (field RESOLVED LIVE)
//! 9. `POST /data/api/v1/trial` (session cookie + `X-CSRF-Token`
//!    header) → 200 = the fresh [`TrialWire`]
//! 10. read-back: `GET /data/api/v1/trial` → `expired:false` (the
//!     action layer owns the flip check — mutations read back)
//!
//! ## Design rules (research anti-patterns, honored)
//!
//! - The flow NEVER touches the locked client pipeline: a DEDICATED
//!   flow-local `reqwest::Client` with `redirect(Policy::none())`
//!   consumes each 302 by hand (Location header → next GET).
//! - NO cookie store (the `cookies` feature stays OUT): the ~4 known
//!   Set-Cookies are captured into a `Vec<(name, value)>` and replayed
//!   verbatim — a fixed sequence, not arbitrary browsing.
//! - Non-JSON 4xx from the IdP endpoints (consumed-token replay →
//!   Jetty HTML 400) surfaces as a flow failure with the HTML `<title>`
//!   extracted (the classify-style sniff, flow-local edition).
//! - Passwords ride [`Secret`] end-to-end; the only exposure is the
//!   one JSON-body construction site (the redaction discipline).

use std::time::Duration;

use serde::Deserialize;
use serde_json::json;

use crate::client::trial::TrialWire;
use crate::config::Secret;
use crate::error::CoreError;

/// Login entry point — 302s into the IdP OIDC flow.
const APP_LOGIN_PATH: &str = "/data/app/login";
/// The fixed shape of step 1's redirect: `/idp/<name>/oidc/auth?…`.
/// The `<name>` segment is DERIVED per gateway — the internal IdP is
/// not always called `default` (live-observed `temp` on 8.3.x), so
/// every IdP URL in the flow is built from the name the gateway
/// itself hands out.
const IDP_PATH_PREFIX: &str = "/idp/";
const OIDC_AUTH_PATH_SUFFIX: &str = "/oidc/auth";
/// The session/CSRF endpoint (step 8).
const APP_SESSION_PATH: &str = "/data/app/session";
/// The trial reset target (step 9).
const TRIAL_PATH: &str = "/data/api/v1/trial";
/// The session cookie's name prefix (the suffix is the gateway id —
/// captured generically from Set-Cookie, live-resolved).
const SESSION_COOKIE_PREFIX: &str = "webui-sid-";

/// The authenticated gateway session the flow yields.
#[derive(Debug, Clone)]
pub struct GatewaySession {
    /// The session cookie's name (`webui-sid-<gatewayId>`).
    pub cookie_name: String,
    /// The session cookie's value.
    pub cookie_value: String,
    /// The CSRF token (step 8's `csrfToken` field) — rides the
    /// `X-CSRF-Token` header on the reset POST.
    pub csrf_token: String,
}

impl GatewaySession {
    /// The `Cookie:` header value for this session.
    fn cookie_header(&self) -> String {
        format!("{}={}", self.cookie_name, self.cookie_value)
    }
}

/// Step 8's body — only `csrfToken` is consumed; the user payload
/// round-trips as passthrough.
#[derive(Debug, Deserialize)]
struct SessionInfo {
    #[serde(rename = "csrfToken", default)]
    csrf_token: String,
}

/// Step 3/5's body — the rotating token + completeness.
#[derive(Debug, Deserialize)]
struct ChallengeAnswer {
    #[serde(default)]
    complete: bool,
    #[serde(rename = "nextChallenge", default)]
    next_challenge: Vec<serde_json::Value>,
    #[serde(default)]
    token: String,
}

/// Step 4's body.
#[derive(Debug, Deserialize)]
struct SubmitAnswer {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    token: String,
}

/// One flow-local HTTP client for the whole login dance. Consumed by
/// [`login`] / [`trial_reset_via_session`]; never merged with the
/// locked [`crate::client::ReqwestGatewayApi`] pipeline.
pub struct IdpLoginFlow {
    base: url::Url,
    client: reqwest::Client,
    /// Every cookie the flow has captured, in capture order — replayed
    /// verbatim (the fixed ~4-cookie sequence; NO cookie store).
    cookies: Vec<(String, String)>,
}

impl IdpLoginFlow {
    /// Build the flow against a rig's base URL (e.g.
    /// `http://localhost:9088`).
    pub fn new(base_url: &str) -> Result<Self, CoreError> {
        super::install_crypto_provider();
        let client = reqwest::Client::builder()
            // The locked client's rule, flow-local edition: consume
            // every 302 BY HAND (the flow's steps ARE the redirects).
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|err| CoreError::Internal(format!("cannot build login client: {err}")))?;
        Ok(Self {
            base: url::Url::parse(base_url)
                .map_err(|err| CoreError::Internal(format!("invalid rig URL: {err}")))?,
            client,
            cookies: Vec::new(),
        })
    }

    fn url_for(&self, path_and_query: &str) -> url::Url {
        self.base
            .join(path_and_query)
            .expect("base joins an absolute path")
    }

    /// Capture every `Set-Cookie` on the response (name=value only —
    /// attributes dropped; the replay is manual).
    fn capture_cookies(&mut self, response: &reqwest::Response) {
        for value in response.headers().get_all(reqwest::header::SET_COOKIE) {
            if let Ok(cookie) = value.to_str()
                && let Some((name, cookie_value)) = cookie.split_once('=')
            {
                let name = name.trim().to_string();
                let cookie_value = cookie_value
                    .split(';')
                    .next()
                    .unwrap_or(cookie_value)
                    .trim()
                    .to_string();
                if !name.is_empty() && !cookie_value.is_empty() {
                    // A re-set cookie replaces its prior value.
                    self.cookies.retain(|(prior, _)| *prior != name);
                    self.cookies.push((name, cookie_value));
                }
            }
        }
    }

    /// The `Cookie:` header for everything captured so far.
    fn cookie_header(&self) -> String {
        self.cookies
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// A flow-local transport+shape failure: the message names the
    /// step so agents see exactly where the dance broke.
    fn flow_error(step: &str, detail: String) -> CoreError {
        CoreError::Internal(format!("gateway login flow failed at {step}: {detail}"))
    }

    /// Extract `<title>Error NNN</title>` from a Jetty HTML error page
    /// (the consumed-token replay shape — research Pitfall 2), else
    /// truncate the body.
    fn html_title_or_excerpt(body: &str) -> String {
        if let Some(start) = body.find("<title>")
            && let Some(end) = body[start + 7..].find("</title>")
        {
            return body[start + 7..start + 7 + end].to_string();
        }
        let excerpt: String = body.chars().take(120).collect();
        excerpt.replace(['\n', '\r'], " ")
    }

    /// GET `path_and_query` with the captured cookies; expect a 302
    /// and return its Location (path + query — the next hop).
    async fn follow_redirect(
        &mut self,
        step: &str,
        path_and_query: &str,
    ) -> Result<String, CoreError> {
        let url = self.url_for(path_and_query);
        let mut request = self.client.get(url.clone());
        if !self.cookies.is_empty() {
            request = request.header(reqwest::header::COOKIE, self.cookie_header());
        }
        let response = request.send().await.map_err(|err| CoreError::Network {
            url: url.to_string(),
            source: Some(err),
            observation: None,
        })?;
        self.capture_cookies(&response);
        match response.status().as_u16() {
            302 | 303 => {
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_string);
                location.ok_or_else(|| {
                    Self::flow_error(step, "redirect carried no Location header".into())
                })
            }
            status => {
                let body = response.text().await.unwrap_or_default();
                Err(Self::flow_error(
                    step,
                    format!(
                        "expected a redirect, got HTTP {status} ({})",
                        Self::html_title_or_excerpt(&body)
                    ),
                ))
            }
        }
    }

    /// POST `path` with a JSON body + captured cookies; expect 200 JSON
    /// (the challenge endpoints' contract). Non-2xx or non-JSON → flow
    /// failure with the HTML title sniff.
    async fn post_json_flow(
        &self,
        step: &str,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, CoreError> {
        let url = self.url_for(path);
        let mut request = self
            .client
            .post(url.clone())
            .header(reqwest::header::ACCEPT, "application/json");
        if !self.cookies.is_empty() {
            request = request.header(reqwest::header::COOKIE, self.cookie_header());
        }
        let response = request
            .json(body)
            .send()
            .await
            .map_err(|err| CoreError::Network {
                url: url.to_string(),
                source: Some(err),
                observation: None,
            })?;
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        if !(200..300).contains(&status) {
            // 401/403 from the challenge endpoints = auth-class; other
            // 4xx (the Jetty-HTML token-replay 400) = flow failure
            // with the title extracted.
            if status == 401 || status == 403 {
                return Err(CoreError::Auth {
                    status,
                    endpoint: Some(path.to_string()),
                });
            }
            return Err(Self::flow_error(
                step,
                format!("HTTP {status} ({})", Self::html_title_or_excerpt(&text)),
            ));
        }
        serde_json::from_str(&text).map_err(|err| {
            Self::flow_error(
                step,
                format!("non-JSON answer ({})", Self::html_title_or_excerpt(&text)),
            )
            .tap_detail(err)
        })
    }
}

/// Small helper to append the underlying parse error to a flow failure
/// without changing its class (kept local + trivial).
trait TapDetail {
    fn tap_detail(self, err: serde_json::Error) -> CoreError;
}

impl TapDetail for CoreError {
    fn tap_detail(self, err: serde_json::Error) -> CoreError {
        match self {
            CoreError::Internal(message) => CoreError::Internal(format!("{message}: {err}")),
            other => other,
        }
    }
}

/// Step 1's redirect must have the shape `/idp/<name>/oidc/auth?…` —
/// the IdP name is DERIVED from the redirect itself (the gateway is
/// authoritative; hardcoding `default` breaks renamed-IdP gateways).
/// Returns the derived name; a shape mismatch fails with the observed
/// target named so a genuine non-IdP redirect stays loud and legible.
fn parse_oidc_start(location: &str) -> Result<&str, CoreError> {
    let path = location.split('?').next().unwrap_or(location);
    let name = path
        .strip_prefix(IDP_PATH_PREFIX)
        .and_then(|rest| rest.strip_suffix(OIDC_AUTH_PATH_SUFFIX))
        .filter(|name| !name.is_empty() && !name.contains('/'));
    match name {
        Some(name) => Ok(name),
        None => Err(IdpLoginFlow::flow_error(
            "step 1",
            format!(
                "unexpected redirect target {location:?} (not the gateway IdP OIDC \
                 endpoint — expected /idp/<name>/oidc/auth)"
            ),
        )),
    }
}

/// Run the full login dance (steps 1–8) and yield the gateway session.
/// `password` exposure happens at exactly ONE site: the step-4 JSON
/// body construction (the redaction discipline).
pub async fn login(
    flow: IdpLoginFlow,
    username: &str,
    password: &Secret,
) -> Result<(IdpLoginFlow, GatewaySession), CoreError> {
    let mut flow = flow;

    // 1. Entry: /data/app/login → the OIDC authorization URL. The IdP
    //    name comes out of the redirect itself.
    let oidc_start = flow
        .follow_redirect("step 1 (GET /data/app/login)", APP_LOGIN_PATH)
        .await?;
    let idp_name = parse_oidc_start(&oidc_start)?;
    let next_challenge_path = format!("/idp/{idp_name}/authn/next-challenge");
    let submit_basic_path = format!("/idp/{idp_name}/authn/submit-challenge/basic");

    // 2. OIDC auth → the login challenge page URL carrying T0.
    let login_url = flow
        .follow_redirect("step 2 (GET oidc/auth)", &oidc_start)
        .await?;
    let token0 = query_param(&login_url, "token").ok_or_else(|| {
        IdpLoginFlow::flow_error("step 2", "the authn/login redirect carried no token".into())
    })?;

    // 3. next-challenge {token: T0} → T1 (TOKEN ROTATES — thread forward).
    let answer: ChallengeAnswer = serde_json::from_value(
        flow.post_json_flow(
            "step 3 (next-challenge)",
            &next_challenge_path,
            &json!({ "token": token0 }),
        )
        .await?,
    )
    .map_err(|err| IdpLoginFlow::flow_error("step 3", format!("answer shape: {err}")))?;
    if answer.complete {
        return Err(IdpLoginFlow::flow_error(
            "step 3",
            "flow already complete before credentials were offered".into(),
        ));
    }
    let token1 = answer.token;

    // 4. submit-challenge/basic — the ONLY password exposure site.
    let submit: SubmitAnswer = serde_json::from_value(
        flow.post_json_flow(
            "step 4 (submit-challenge/basic)",
            &submit_basic_path,
            &json!({
                "token": token1,
                "rememberMe": false,
                "challenge": { "username": username, "password": password.expose() }
            }),
        )
        .await?,
    )
    .map_err(|err| IdpLoginFlow::flow_error("step 4", format!("answer shape: {err}")))?;
    if !submit.success {
        // Live-observed shape on 8.3.6: 200 {"success":false,"token":…}.
        // Auth class + slug are right; the variant's token-flavored hint
        // is the accepted trade-off (documented at the flow's module).
        return Err(CoreError::Auth {
            status: 401,
            endpoint: Some(submit_basic_path),
        });
    }
    let token2 = submit.token;

    // 5. next-challenge {token: T2} → complete + T3.
    let answer: ChallengeAnswer = serde_json::from_value(
        flow.post_json_flow(
            "step 5 (next-challenge)",
            &next_challenge_path,
            &json!({ "token": token2 }),
        )
        .await?,
    )
    .map_err(|err| IdpLoginFlow::flow_error("step 5", format!("answer shape: {err}")))?;
    if !answer.complete {
        let kinds: Vec<String> = answer
            .next_challenge
            .iter()
            .filter_map(|challenge| challenge.get("type").and_then(|t| t.as_str()))
            .map(str::to_string)
            .collect();
        return Err(IdpLoginFlow::flow_error(
            "step 5",
            format!(
                "the IdP presented another challenge beyond basic auth \
                 ({kinds:?}) — headless login does not continue past it"
            ),
        ));
    }
    let token3 = answer.token;

    // 6. oidc/auth with the ORIGINAL params + token=T3 → the federate
    //    callback URL. (Step 1's Location is replayed VERBATIM with the
    //    token appended — the IdP name, path and query all come from the
    //    gateway, live-verified shape.)
    let step6_url = if oidc_start.contains('?') {
        format!("{oidc_start}&token={token3}")
    } else {
        format!("{oidc_start}?token={token3}")
    };
    let callback = flow
        .follow_redirect("step 6 (GET oidc/auth + token)", &step6_url)
        .await?;

    // 7. The federate callback → the webui-sid-* session cookie.
    flow.follow_redirect("step 7 (GET federate callback)", &callback)
        .await?;
    let (session_name, session_value) = flow
        .cookies
        .iter()
        .find(|(name, _)| name.starts_with(SESSION_COOKIE_PREFIX))
        .cloned()
        .ok_or_else(|| {
            IdpLoginFlow::flow_error(
                "step 7",
                format!("no {SESSION_COOKIE_PREFIX}* session cookie was set"),
            )
        })?;

    // 8. /data/app/session → the CSRF token.
    let session_url = flow.url_for(APP_SESSION_PATH);
    let mut request = flow.client.get(session_url.clone());
    if !flow.cookies.is_empty() {
        request = request.header(reqwest::header::COOKIE, flow.cookie_header());
    }
    let response = request.send().await.map_err(|err| CoreError::Network {
        url: session_url.to_string(),
        source: Some(err),
        observation: None,
    })?;
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    if status == 401 || status == 403 {
        return Err(CoreError::Auth {
            status,
            endpoint: Some(APP_SESSION_PATH.to_string()),
        });
    }
    if !(200..300).contains(&status) {
        return Err(IdpLoginFlow::flow_error(
            "step 8",
            format!("HTTP {status} fetching the session CSRF token"),
        ));
    }
    let info: SessionInfo = serde_json::from_str(&text).map_err(|err| {
        IdpLoginFlow::flow_error("step 8", format!("session answer shape: {err}"))
    })?;
    if info.csrf_token.is_empty() {
        return Err(IdpLoginFlow::flow_error(
            "step 8",
            "the session answer carried no csrfToken".into(),
        ));
    }

    Ok((
        flow,
        GatewaySession {
            cookie_name: session_name,
            cookie_value: session_value,
            csrf_token: info.csrf_token,
        },
    ))
}

/// Step 9: the reset POST with the session cookie + CSRF header, on
/// the flow-local path (never the locked pipeline). The 2xx body IS
/// the fresh [`TrialWire`] (live-observed on 8.3.3: expired true →
/// false, 7199s).
pub async fn trial_reset_via_session(
    flow: &IdpLoginFlow,
    session: &GatewaySession,
) -> Result<TrialWire, CoreError> {
    let url = flow.url_for(TRIAL_PATH);
    let response = flow
        .client
        .post(url.clone())
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::COOKIE, session.cookie_header())
        .header("X-CSRF-Token", &session.csrf_token)
        .send()
        .await
        .map_err(|err| CoreError::Network {
            url: url.to_string(),
            source: Some(err),
            observation: None,
        })?;
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    if status == 401 || status == 403 {
        return Err(CoreError::Auth {
            status,
            endpoint: Some(TRIAL_PATH.to_string()),
        });
    }
    if !(200..300).contains(&status) {
        return Err(IdpLoginFlow::flow_error(
            "step 9 (POST trial)",
            format!(
                "HTTP {status} ({})",
                IdpLoginFlow::html_title_or_excerpt(&text)
            ),
        ));
    }
    serde_json::from_str(&text).map_err(|err| {
        CoreError::Internal(format!(
            "trial reset response did not match the trial shape: {err}"
        ))
    })
}

/// Pull one query parameter out of a path?query string.
fn query_param(path_and_query: &str, name: &str) -> Option<String> {
    let query = path_and_query.split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then(|| value.to_string())
    })
}

// --- The session-tier generic helpers (ADOPT-01) -----------------------
//
// The adopt capability (client/adopt.rs) needs GET/POST/PUT against
// /data routes on the SAME session+CSRF footing as the trial reset,
// but lives outside this module (wire models + tests of its own). The
// three helpers below are the minimal pub(crate) surface: session
// cookie + X-CSRF-Token + JSON in/out, the trial-reset error
// classification verbatim (401/403 auth-class; other non-2xx flow
// failures with the Jetty title sniff; non-JSON shape failures).
// Response-body verdicts (success:false and friends) belong to the
// CALLER — these helpers only move JSON.

impl IdpLoginFlow {
    /// GET `path?pairs` on the session tier → JSON.
    pub(crate) async fn session_get_json(
        &self,
        session: &GatewaySession,
        path: &str,
        pairs: &[(&str, &str)],
    ) -> Result<serde_json::Value, CoreError> {
        let mut url = self.url_for(path);
        url.query_pairs_mut().extend_pairs(pairs.iter().copied());
        let request = self
            .client
            .get(url.clone())
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::COOKIE, session.cookie_header());
        let response = request.send().await.map_err(|err| CoreError::Network {
            url: url.to_string(),
            source: Some(err),
            observation: None,
        })?;
        self.session_finish(url.as_str(), response).await
    }

    /// POST `path` with a JSON body on the session tier → JSON.
    pub(crate) async fn session_post_json(
        &self,
        session: &GatewaySession,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, CoreError> {
        self.session_send(reqwest::Method::POST, session, path, Some(body))
            .await
    }

    /// PUT `path` with a JSON body on the session tier → JSON.
    pub(crate) async fn session_put_json(
        &self,
        session: &GatewaySession,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, CoreError> {
        self.session_send(reqwest::Method::PUT, session, path, Some(body))
            .await
    }

    async fn session_send(
        &self,
        method: reqwest::Method,
        session: &GatewaySession,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, CoreError> {
        let url = self.url_for(path);
        let mut request = self
            .client
            .request(method, url.clone())
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::COOKIE, session.cookie_header())
            .header("X-CSRF-Token", &session.csrf_token);
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request.send().await.map_err(|err| CoreError::Network {
            url: url.to_string(),
            source: Some(err),
            observation: None,
        })?;
        self.session_finish(url.as_str(), response).await
    }

    /// The shared verdict: 401/403 auth-class; other non-2xx flow
    /// failure (title sniff); 2xx must be JSON.
    async fn session_finish(
        &self,
        path: &str,
        response: reqwest::Response,
    ) -> Result<serde_json::Value, CoreError> {
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        if status == 401 || status == 403 {
            return Err(CoreError::Auth {
                status,
                endpoint: Some(path.to_string()),
            });
        }
        if !(200..300).contains(&status) {
            return Err(IdpLoginFlow::flow_error(
                "session request",
                format!(
                    "HTTP {status} ({})",
                    IdpLoginFlow::html_title_or_excerpt(&text)
                ),
            ));
        }
        serde_json::from_str(&text).map_err(|err| {
            CoreError::Internal(format!("session response from {path} was not JSON ({err})"))
        })
    }
}
