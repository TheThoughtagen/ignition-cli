//! Native OIDC login + CSRF flow (04-03, tier 1 of the trial-reset
//! ladder) — the internal IdP's challenge dance, live-probed during
//! 04-RESEARCH and **live-verified END-TO-END on 8.3.3 during this
//! plan's spike**: login → session → CSRF → `POST /data/api/v1/trial`
//! flipped `expired:true → false` (`trialSecondsLeft 0 → 7199`).
//!
//! ## The flow (all steps live-observed; the research's 10-step map,
//! with the two LOW-confidence deliverables now resolved live)
//!
//! 1. `GET /data/app/login` → 302 into `/idp/default/oidc/auth?…`
//!    (+ `idp-relay-*` cookie)
//! 2. `GET /idp/default/oidc/auth?…` → 302 to
//!    `/idp/default/authn/login?…&token=<T0>` (+ `idp-sid-default-*`
//!    cookie)
//! 3. `POST /idp/default/authn/next-challenge` `{"token":T0}` →
//!    `{"complete":false,"nextChallenge":[…],"token":<T1>}` — **the
//!    token ROTATES on every call; thread it forward or the next call
//!    400s in Jetty HTML** (research Pitfall 2)
//! 4. `POST /idp/default/authn/submit-challenge/basic`
//!    `{"token":T1,"rememberMe":false,"challenge":{username,password}}`
//!    → `{"success":bool,"token":<T2>}`; `success:false` = rejected
//!    credentials (live-observed on 8.3.6 with a wrong password)
//! 5. `POST next-challenge {"token":T2}` → `{"complete":true,…,
//!    "token":<T3>}`
//! 6. `GET /idp/default/oidc/auth?<orig params>&token=<T3>` → 302 to
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
/// The IdP's OIDC authorization endpoint (first path segment of step 1's
/// Location).
const OIDC_AUTH_PREFIX: &str = "/idp/default/oidc/auth";
/// The rotating-token challenge endpoints.
const NEXT_CHALLENGE_PATH: &str = "/idp/default/authn/next-challenge";
const SUBMIT_BASIC_PATH: &str = "/idp/default/authn/submit-challenge/basic";
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
    async fn follow_redirect(&mut self, step: &str, path_and_query: &str) -> Result<String, CoreError> {
        let url = self.url_for(path_and_query);
        let mut request = self.client.get(url.clone());
        if !self.cookies.is_empty() {
            request = request.header(reqwest::header::COOKIE, self.cookie_header());
        }
        let response = request.send().await.map_err(|err| CoreError::Network {
            url: url.to_string(),
            source: Some(err),
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
        let response = request.json(body).send().await.map_err(|err| CoreError::Network {
            url: url.to_string(),
            source: Some(err),
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
                format!(
                    "HTTP {status} ({})",
                    Self::html_title_or_excerpt(&text)
                ),
            ));
        }
        serde_json::from_str(&text).map_err(|err| {
            Self::flow_error(
                step,
                format!(
                    "non-JSON answer ({})",
                    Self::html_title_or_excerpt(&text)
                ),
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
            CoreError::Internal(message) => {
                CoreError::Internal(format!("{message}: {err}"))
            }
            other => other,
        }
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

    // 1. Entry: /data/app/login → the OIDC authorization URL.
    let oidc_start = flow
        .follow_redirect("step 1 (GET /data/app/login)", APP_LOGIN_PATH)
        .await?;
    if !oidc_start.starts_with(OIDC_AUTH_PREFIX) {
        return Err(IdpLoginFlow::flow_error(
            "step 1",
            format!("unexpected redirect target {oidc_start:?} (not the internal IdP)"),
        ));
    }

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
            NEXT_CHALLENGE_PATH,
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
            SUBMIT_BASIC_PATH,
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
            endpoint: Some(SUBMIT_BASIC_PATH.to_string()),
        });
    }
    let token2 = submit.token;

    // 5. next-challenge {token: T2} → complete + T3.
    let answer: ChallengeAnswer = serde_json::from_value(
        flow.post_json_flow(
            "step 5 (next-challenge)",
            NEXT_CHALLENGE_PATH,
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
    //    callback URL. (The orig query is step 1's Location minus its
    //    path — live-verified shape.)
    let oidc_query = oidc_start
        .split_once('?')
        .map(|(_, query)| query.to_string())
        .unwrap_or_default();
    let callback = flow
        .follow_redirect(
            "step 6 (GET oidc/auth + token)",
            &format!("{OIDC_AUTH_PREFIX}?{oidc_query}&token={token3}"),
        )
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
        CoreError::Internal(format!("trial reset response did not match the trial shape: {err}"))
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
