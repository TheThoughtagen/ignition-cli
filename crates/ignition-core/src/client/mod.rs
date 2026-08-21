//! The gateway HTTP seam: a coarse [`GatewayApi`] trait so actions never
//! touch reqwest types, plus the production [`ReqwestGatewayApi`].
//!
//! LOCKED: the trait uses `async_trait` (research Open Question 2,
//! resolved) — dyn-compatible today, ubiquitous. The trait stays COARSE —
//! one method per capability, not per endpoint — so Phase 2 grows it
//! without churn.
//!
//! Auth-header rule (verified from ignition-mcp `ignition_client.py`):
//! a token credential sends `X-Ignition-API-Token`; a basic credential
//! sends `Authorization: Basic <b64>`; NEVER both — enforced by a `match`.
//! [`Secret::expose`] is called at exactly this one construction site (the
//! grep-auditable redaction boundary; CORE-02).
//!
//! gateway-info is marked `auth: none` in the 83-api collection, so a
//! `None` credential proceeds header-less (credentials attach when present
//! but are not required on a 200; verified empirically in Phase 2).

use std::time::Duration;

pub mod version;

use crate::client::version::GatewayInfo;
use crate::config::{Credential, Profile};
use crate::error::CoreError;

/// GET path of the gateway-info capability.
const GATEWAY_INFO_PATH: &str = "/data/api/v1/gateway-info";

/// One capability per method — coarse on purpose. Phase 2 adds status,
/// modules, logs, … as methods here; actions never see reqwest types.
#[async_trait::async_trait]
pub trait GatewayApi: Send + Sync {
    /// Fetch `/data/api/v1/gateway-info`.
    async fn gateway_info(&self) -> Result<GatewayInfo, CoreError>;
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
    /// header-less; gateway-info is `auth: none`).
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
}

fn build_client(ssl_verify: bool) -> Result<reqwest::Client, CoreError> {
    let mut builder = reqwest::Client::builder()
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
        let url = self.url_for(GATEWAY_INFO_PATH);
        let mut request = self.client.get(url.clone());
        // The auth-header rule: token XOR basic XOR neither — a match, not
        // if/if-else chains. expose() is called at exactly this site.
        match &self.credential {
            Some(Credential::Token(token)) => {
                request = request.header("X-Ignition-API-Token", token.expose());
            }
            Some(Credential::Basic(user, password)) => {
                request = request.basic_auth(user.expose(), Some(password.expose()));
            }
            None => {}
        }
        // Transport failures (connect/timeout/TLS) → Network (exit 4).
        let response = request.send().await.map_err(|err| CoreError::Network {
            url: url.to_string(),
            source: err,
        })?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(CoreError::Auth {
                status: status.as_u16(),
                endpoint: Some(url.to_string()),
            });
        }
        if !status.is_success() {
            // Only 401/403 and 2xx are contract shapes today; Phase 2
            // refines per-endpoint semantics.
            return Err(CoreError::Internal(format!(
                "unexpected HTTP {status} from {url}"
            )));
        }
        let mut info = response.json::<GatewayInfo>().await.map_err(|err| {
            CoreError::Internal(format!(
                "gateway-info response at {url} did not match the expected shape: {err}"
            ))
        })?;
        info.endpoint = Some(url.to_string());
        Ok(info)
    }
}
