//! ign's client of the `cli/tagDelete` WebDev route (26-18, D-13,
//! TAGDEP-06).
//!
//! The deploy run deletes tags through one seam, [`TagDeleteRoute`]
//! (`handshake`, `delete`). The real implementation, [`TagDeleteClient`],
//! is a WebDev-only Basic client: it sends HTTP Basic of the caller's env
//! pair and the route secret in `X-Ignition-CLI-Secret`, and never the
//! profile's API token. Answers are parsed strictly (an unknown or a
//! missing field is refused), and a delete answer must hold exactly one
//! result per requested path, in request order, under the requested
//! provider. Anything else is `webdev_route_error` with code
//! `route_answer_invalid`, never a success.
//!
//! The route contract is 26-17's (`26-17-SUMMARY.md`, `route_contract:
//! as-proposed`): `version` answers `{routeVersion, minCli, providers,
//! maxPaths, maxPathChars}`; `delete` takes `{provider, paths}` and answers
//! `{provider, results: [{path, quality, good, existsAfter}], elapsedMs}`.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::actions::webdev::{SECRET_HEADER, TAG_DELETE_ROUTE};
use crate::client::webdev::{self as seam, RouteBody};
use crate::client::{GatewayApi, ReqwestGatewayApi};
use crate::config::Secret;
use crate::error::CoreError;
use crate::webdev::ROUTE_BUNDLE_VERSION;

/// The per-call ceiling of a delete: the tag import timeout class
/// (300 s). The 26-17 record has a 500-path delete at 2.0 s cold.
pub const TAG_DELETE_CALL_TIMEOUT_S: u64 = 300;

/// The route code of an answer ign refuses to read: an unknown or missing
/// field, or a delete answer that does not match its request.
pub const ROUTE_ANSWER_INVALID: &str = "route_answer_invalid";

/// The message prefix of the `internal` error a handshake answered with
/// HTTP 500 maps to. A freshly imported route answers 500 while WebDev
/// compiles it, so the readiness wait retries on it; every other `internal` error returns at once.
pub const TAG_DELETE_HTTP_500: &str = "the tagDelete route answered HTTP 500";

/// The route's version handshake.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RouteHandshake {
    /// The route's `routeVersion`; must equal [`ROUTE_BUNDLE_VERSION`].
    pub route_version: String,
    /// The oldest CLI version the route accepts.
    pub min_cli: String,
    /// The provider allowlist written at install.
    pub providers: Vec<String>,
    /// The most paths one delete call may carry.
    pub max_paths: usize,
    /// The longest path the route accepts, in characters.
    pub max_path_chars: usize,
}

/// One path's line in a delete answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PathResult {
    /// The path as requested (relative to the provider).
    pub path: String,
    /// The gateway's quality code string; null when `deleteTags` returned
    /// no per-path list. The key itself is required.
    #[serde(deserialize_with = "Option::deserialize")]
    pub quality: Option<String>,
    /// That code's `isGood()`; null with `quality`. The key is required.
    #[serde(deserialize_with = "Option::deserialize")]
    pub good: Option<bool>,
    /// `system.tag.exists` right after the call.
    pub exists_after: bool,
}

/// The route's answer to one delete call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeleteAnswer {
    /// The provider the route deleted under.
    pub provider: String,
    /// One result per requested path, in request order.
    pub results: Vec<PathResult>,
    /// Time inside the route, in milliseconds.
    pub elapsed_ms: u64,
}

impl DeleteAnswer {
    /// The answer belongs to this request: the same provider and exactly
    /// one result per requested path, in request order. Any mismatch is
    /// `webdev_route_error` / `route_answer_invalid`.
    pub fn check(&self, provider: &str, requested: &[String]) -> Result<(), CoreError> {
        let invalid = |message: String| {
            Err(CoreError::WebdevRouteError {
                code: ROUTE_ANSWER_INVALID.to_string(),
                message,
                endpoint: None,
            })
        };
        if self.provider != provider {
            return invalid(format!(
                "the delete answer names provider {:?}, the request {provider:?}",
                self.provider
            ));
        }
        if self.results.len() != requested.len() {
            return invalid(format!(
                "the delete answer holds {} results for {} requested paths",
                self.results.len(),
                requested.len()
            ));
        }
        for (index, (result, path)) in self.results.iter().zip(requested).enumerate() {
            if &result.path != path {
                return invalid(format!(
                    "delete answer result {index} is for path {:?}, the request's path {index} \
                     is {path:?}",
                    result.path
                ));
            }
        }
        Ok(())
    }
}

/// The one seam the deploy run deletes tags through.
#[async_trait::async_trait]
pub trait TagDeleteRoute: Send + Sync {
    /// The route's `version` handshake, its version checked against
    /// [`ROUTE_BUNDLE_VERSION`].
    async fn handshake(&self) -> Result<RouteHandshake, CoreError>;
    /// Delete `paths` (relative to `provider`) in one call. The answer has
    /// passed [`DeleteAnswer::check`].
    async fn delete(&self, provider: &str, paths: &[String]) -> Result<DeleteAnswer, CoreError>;
}

/// The real [`TagDeleteRoute`]: a WebDev-only Basic client of
/// `/system/webdev/<project>/cli/tagDelete`. It holds no API token.
pub struct TagDeleteClient {
    api: ReqwestGatewayApi,
    project: String,
    secret: Secret,
}

impl std::fmt::Debug for TagDeleteClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TagDeleteClient")
            .field("project", &self.project)
            .field("credential", &"<redacted>")
            .field("secret", &"<redacted>")
            .finish()
    }
}

impl TagDeleteClient {
    /// A client for the route in `project` at `url`, sending Basic of
    /// `user` / `password` and the route `secret`.
    ///
    /// The transport rule is enforced here, before any request is built,
    /// whatever the caller checked: the scheme must be https, or http to a
    /// loopback host (`localhost`, 127.0.0.0/8, `::1`), and `ssl_verify =
    /// false` is accepted only for a loopback host. Otherwise the answer is
    /// `tag_delete_not_configured` for `profile`, naming the scheme and host
    /// (never the URL's userinfo, the credential or the secret).
    pub fn new(
        profile: &str,
        url: &url::Url,
        ssl_verify: bool,
        project: &str,
        user: Secret,
        password: Secret,
        secret: Secret,
    ) -> Result<Self, CoreError> {
        if let Some(reason) = transport_refusal(url, ssl_verify) {
            return Err(CoreError::TagDeleteNotConfigured {
                profile: profile.to_string(),
                missing: vec![reason],
            });
        }
        Ok(Self {
            api: ReqwestGatewayApi::webdev_basic(url, ssl_verify, user, password)?,
            project: project.to_string(),
            secret,
        })
    }

    fn headers(&self) -> [(&str, &str); 1] {
        [(SECRET_HEADER, self.secret.expose())]
    }
}

/// Parse route data strictly into `T`; a failure is
/// `route_answer_invalid` naming what was read.
fn strict<T: serde::de::DeserializeOwned>(
    data: serde_json::Value,
    what: &str,
    endpoint: &str,
) -> Result<T, CoreError> {
    serde_json::from_value(data).map_err(|err| CoreError::WebdevRouteError {
        code: ROUTE_ANSWER_INVALID.to_string(),
        message: format!("the tagDelete {what} answer is not the 26-17 contract: {err}"),
        endpoint: Some(endpoint.to_string()),
    })
}

#[async_trait::async_trait]
impl TagDeleteRoute for TagDeleteClient {
    async fn handshake(&self) -> Result<RouteHandshake, CoreError> {
        let (url, response) = self
            .api
            .webdev_post_raw(
                &self.project,
                TAG_DELETE_ROUTE,
                &serde_json::json!({"action": "version"}),
                &self.headers(),
                None,
            )
            .await?;
        let status = response.status();
        if status.is_success() {
            let text = response.text().await.unwrap_or_default();
            let data = match seam::parse_route_body(&text)? {
                RouteBody::Ok(data) => data,
                RouteBody::Denied {
                    code,
                    message,
                    traceback,
                } => {
                    return Err(seam::denial_to_error(
                        &code,
                        &message,
                        traceback.as_deref(),
                        url,
                    ));
                }
            };
            let handshake: RouteHandshake = strict(data, "version", &url)?;
            if handshake.max_paths == 0 || handshake.max_path_chars == 0 {
                return Err(CoreError::WebdevRouteError {
                    code: ROUTE_ANSWER_INVALID.to_string(),
                    message: "the tagDelete version answer has a zero maxPaths or maxPathChars"
                        .to_string(),
                    endpoint: Some(url),
                });
            }
            if handshake.route_version != ROUTE_BUNDLE_VERSION {
                return Err(CoreError::RouteVersionMismatch {
                    route: TAG_DELETE_ROUTE.to_string(),
                    deployed: handshake.route_version,
                    expected: ROUTE_BUNDLE_VERSION.to_string(),
                    endpoint: Some(url),
                });
            }
            return Ok(handshake);
        }
        match status.as_u16() {
            401 | 403 => Err(CoreError::Auth {
                status: status.as_u16(),
                endpoint: Some(url),
            }),
            402 => Err(CoreError::WebdevUnlicensed {
                endpoint: Some(url),
            }),
            405 => Err(CoreError::RoutesNotDeployed {
                project: self.project.clone(),
                route: TAG_DELETE_ROUTE.to_string(),
                endpoint: Some(url),
            }),
            500 => Err(CoreError::Internal(format!(
                "{TAG_DELETE_HTTP_500} at {url} (WebDev may still be compiling a fresh import)"
            ))),
            _ => match crate::client::classify::classify(response, &url, false).await {
                Err(err) => Err(err),
                Ok(_) => Err(CoreError::Internal(format!(
                    "unexpected HTTP {status} from the tagDelete handshake at {url}"
                ))),
            },
        }
    }

    async fn delete(&self, provider: &str, paths: &[String]) -> Result<DeleteAnswer, CoreError> {
        let endpoint = seam::route_url(&self.project, TAG_DELETE_ROUTE);
        let data = self
            .api
            .webdev_route_call_with_timeout(
                &self.project,
                TAG_DELETE_ROUTE,
                &serde_json::json!({"action": "delete", "provider": provider, "paths": paths}),
                &self.headers(),
                Duration::from_secs(TAG_DELETE_CALL_TIMEOUT_S),
            )
            .await?;
        let answer: DeleteAnswer = strict(data, "delete", &endpoint)?;
        answer.check(provider, paths).map_err(|err| match err {
            CoreError::WebdevRouteError { code, message, .. } => CoreError::WebdevRouteError {
                code,
                message,
                endpoint: Some(endpoint.clone()),
            },
            other => other,
        })?;
        Ok(answer)
    }
}

/// How long [`wait_ready`] waits for a freshly installed route by default.
/// `ign webdev deploy` leaves WebDev answering HTTP 500 for about 12 s while
/// it compiles the imported routes (261106-tel), and the 26-17 record has
/// the tagDelete route answering `version` 9.6 s after a cold import with
/// 500 and 405 in between; 30 s covers both with room.
pub const TAG_DELETE_READY_TIMEOUT_S: u64 = 30;

/// The pause between two handshakes in [`wait_ready`] by default (the 26-17
/// gates retried every 2 s).
pub const TAG_DELETE_READY_POLL_S: u64 = 2;

/// Wait for the route to answer its handshake. HTTP 500 (WebDev still
/// compiling) and 405 (route not yet registered) are retried every `poll`
/// until `timeout`; the last one is returned when the time is up, so an
/// absent route ends `routes_not_deployed` naming tagDelete. Every other
/// answer returns at once: 401/403 `auth_rejected`, 402
/// `webdev_unlicensed`, a secret or other denial `webdev_route_error`, a
/// version mismatch `route_version_mismatch`.
pub async fn wait_ready(
    route: &dyn TagDeleteRoute,
    timeout: Duration,
    poll: Duration,
) -> Result<RouteHandshake, CoreError> {
    let started = tokio::time::Instant::now();
    loop {
        match route.handshake().await {
            Ok(handshake) => return Ok(handshake),
            Err(err) if is_warming(&err) => {
                let elapsed = started.elapsed();
                if elapsed >= timeout {
                    return Err(err);
                }
                tokio::time::sleep(poll.min(timeout - elapsed)).await;
            }
            Err(err) => return Err(err),
        }
    }
}

/// The two answers of a route WebDev has not finished installing.
fn is_warming(err: &CoreError) -> bool {
    match err {
        CoreError::RoutesNotDeployed { route, .. } => route == TAG_DELETE_ROUTE,
        CoreError::Internal(message) => message.starts_with(TAG_DELETE_HTTP_500),
        _ => false,
    }
}

/// The most paths one tagDelete call carries: the route contract's
/// `MAX_PATHS` (`doPost.py`), whatever larger `maxPaths` a route answers
/// (26-REVIEW IN-03).
pub const TAG_DELETE_MAX_PATHS: usize = 500;

/// Split `paths` into delete calls of at most the route's `maxPaths` (and
/// never more than [`TAG_DELETE_MAX_PATHS`]), keeping request order. A path longer than `maxPathChars` (in
/// characters, as the route counts) is `deploy_tree_invalid` naming it,
/// before any call.
pub fn delete_chunks(
    paths: &[String],
    handshake: &RouteHandshake,
) -> Result<Vec<Vec<String>>, CoreError> {
    let mut findings = Vec::new();
    for path in paths {
        let chars = path.chars().count();
        if chars > handshake.max_path_chars {
            findings.push(crate::error::DeployFinding {
                file: path.clone(),
                line: None,
                column: None,
                message: format!(
                    "tag path is {chars} characters; the tagDelete route accepts at most {}",
                    handshake.max_path_chars
                ),
            });
        }
    }
    if !findings.is_empty() {
        return Err(CoreError::deploy_findings(findings));
    }
    Ok(paths
        .chunks(handshake.max_paths.clamp(1, TAG_DELETE_MAX_PATHS))
        .map(<[String]>::to_vec)
        .collect())
}

impl DeleteAnswer {
    /// Every result is a clean delete: `good` is not false and, because the
    /// 26-17 record reads `existsAfter after good delete: false`,
    /// `existsAfter` is false. A null `good` (no per-path list from
    /// `deleteTags`) counts as clean only with `existsAfter` false.
    pub fn is_clean(&self) -> bool {
        self.results
            .iter()
            .all(|result| result.good != Some(false) && !result.exists_after)
    }
}

/// The missing-part name of a plain http URL to a non-loopback host.
pub const PLAIN_HTTP_REMOTE: &str = "plain http to a non-loopback host";

/// The missing-part name of `ssl_verify = false` to a non-loopback host.
pub const UNVERIFIED_TLS_REMOTE: &str = "ssl_verify false to a non-loopback host";

/// Whether the profile may use the route, and with what. Values are
/// [`Secret`]s and the `Debug` form redacts them.
pub enum TagDeleteConfig {
    /// Everything is present and the transport is safe.
    Configured {
        /// The caller's user, from the env.
        user: Secret,
        /// The caller's password, from the env.
        password: Secret,
        /// The route secret stored at install.
        secret: Secret,
        /// The gateway URL.
        url: url::Url,
        /// The profile's `ssl_verify`.
        ssl_verify: bool,
    },
    /// Something is missing or refused; nothing may be sent.
    NotConfigured {
        /// The missing parts by name, never by value.
        missing: Vec<String>,
    },
}

impl std::fmt::Debug for TagDeleteConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Configured {
                url, ssl_verify, ..
            } => f
                .debug_struct("Configured")
                .field("user", &"<redacted>")
                .field("password", &"<redacted>")
                .field("secret", &"<redacted>")
                .field("url", &url.as_str())
                .field("ssl_verify", ssl_verify)
                .finish(),
            Self::NotConfigured { missing } => f
                .debug_struct("NotConfigured")
                .field("missing", missing)
                .finish(),
        }
    }
}

impl TagDeleteConfig {
    /// The route client in [`seam::DEFAULT_PROJECT`], or
    /// `tag_delete_not_configured` for `profile_name` naming the missing
    /// parts. Nothing is sent either way.
    pub fn client(&self, profile_name: &str) -> Result<TagDeleteClient, CoreError> {
        match self {
            Self::Configured {
                user,
                password,
                secret,
                url,
                ssl_verify,
            } => TagDeleteClient::new(
                profile_name,
                url,
                *ssl_verify,
                seam::DEFAULT_PROJECT,
                user.clone(),
                password.clone(),
                secret.clone(),
            ),
            Self::NotConfigured { missing } => Err(CoreError::TagDeleteNotConfigured {
                profile: profile_name.to_string(),
                missing: missing.clone(),
            }),
        }
    }
}

/// The host is `localhost`, an address in 127.0.0.0/8 or `::1`.
fn host_is_loopback(url: &url::Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(address)) => address.is_loopback(),
        Some(url::Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    }
}

/// Https, or http to a loopback host.
fn transport_is_safe(url: &url::Url) -> bool {
    match url.scheme() {
        "https" => true,
        "http" => host_is_loopback(url),
        _ => false,
    }
}

/// Unverified TLS (`ssl_verify = false`) is allowed only to a loopback host.
fn tls_verify_is_safe(url: &url::Url, ssl_verify: bool) -> bool {
    ssl_verify || host_is_loopback(url)
}

/// Why [`TagDeleteClient::new`] refuses `url`, naming the scheme and host
/// only; `None` when the transport is safe.
pub(crate) fn transport_refusal(url: &url::Url, ssl_verify: bool) -> Option<String> {
    let scheme = url.scheme();
    let host = url.host_str().unwrap_or("(none)");
    if !transport_is_safe(url) {
        return Some(if scheme == "http" {
            format!("{PLAIN_HTTP_REMOTE} (scheme {scheme}, host {host})")
        } else {
            format!("a scheme that is neither https nor http (scheme {scheme}, host {host})")
        });
    }
    if !tls_verify_is_safe(url, ssl_verify) {
        return Some(format!(
            "{UNVERIFIED_TLS_REMOTE} (scheme {scheme}, host {host})"
        ));
    }
    None
}

/// The profile's tagDelete configuration: the stored
/// `webdev_tag_delete_secret`, both env vars of
/// [`crate::config::secret::tag_delete_env_names`], https or http to a
/// loopback host, and `ssl_verify` on for a non-loopback host. Missing parts
/// are named in that order. [`TagDeleteClient::new`] checks the transport
/// again.
pub fn tag_delete_config(profile_name: &str, profile: &crate::config::Profile) -> TagDeleteConfig {
    let (user_name, password_name) = crate::config::secret::tag_delete_env_names(profile_name);
    let (user, password) = crate::config::secret::tag_delete_env_parts(profile_name);
    let secret = profile
        .webdev_tag_delete_secret
        .as_deref()
        .filter(|secret| !secret.is_empty())
        .map(Secret::new);
    let mut missing = Vec::new();
    if secret.is_none() {
        missing.push("webdev_tag_delete_secret".to_string());
    }
    if user.is_none() {
        missing.push(user_name);
    }
    if password.is_none() {
        missing.push(password_name);
    }
    if !transport_is_safe(&profile.url) {
        missing.push(PLAIN_HTTP_REMOTE.to_string());
    } else if !tls_verify_is_safe(&profile.url, profile.ssl_verify) {
        missing.push(UNVERIFIED_TLS_REMOTE.to_string());
    }
    match (user, password, secret) {
        (Some(user), Some(password), Some(secret)) if missing.is_empty() => {
            TagDeleteConfig::Configured {
                user,
                password,
                secret,
                url: profile.url.clone(),
                ssl_verify: profile.ssl_verify,
            }
        }
        _ => TagDeleteConfig::NotConfigured { missing },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn requested(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    fn row(path: &str) -> PathResult {
        PathResult {
            path: path.into(),
            quality: Some("Good".into()),
            good: Some(true),
            exists_after: Some(false),
        }
    }

    #[test]
    fn path_result_null_quality_is_read_but_a_missing_key_is_not() {
        let read: PathResult = serde_json::from_value(serde_json::json!({
            "path": "A", "quality": null, "good": null, "existsAfter": false
        }))
        .expect("nulls are the contract");
        assert_eq!(read.quality, None);
        assert_eq!(read.good, None);
        for missing in ["quality", "good", "existsAfter", "path"] {
            let mut value = serde_json::json!({
                "path": "A", "quality": "Good", "good": true, "existsAfter": false
            });
            value.as_object_mut().expect("object").remove(missing);
            assert!(
                serde_json::from_value::<PathResult>(value).is_err(),
                "missing {missing} must be refused"
            );
        }
    }

    #[test]
    fn check_requires_provider_count_and_order() {
        let answer = DeleteAnswer {
            provider: "P".into(),
            results: vec![row("A"), row("B")],
            elapsed_ms: 1,
        };
        answer.check("P", &requested(&["A", "B"])).expect("matches");
        for (provider, paths) in [
            ("Q", requested(&["A", "B"])),
            ("P", requested(&["B", "A"])),
            ("P", requested(&["A"])),
            ("P", requested(&["A", "B", "C"])),
        ] {
            let err = answer.check(provider, &paths).expect_err("mismatch");
            assert_eq!(err.code(), "webdev_route_error");
            assert!(matches!(
                &err,
                CoreError::WebdevRouteError { code, .. } if code == ROUTE_ANSWER_INVALID
            ));
        }
    }

    #[test]
    fn client_debug_redacts() {
        let url = url::Url::parse("http://127.0.0.1:1").expect("url");
        let client = TagDeleteClient::new(
            "dev",
            &url,
            true,
            "ign-cli",
            Secret::new("CANARY-user"),
            Secret::new("CANARY-pass"),
            Secret::new("CANARY-secret"),
        )
        .expect("client");
        let debug = format!("{client:?}");
        assert!(!debug.contains("CANARY"), "{debug}");
    }

    /// Build a client with canary credentials; `Err` is the refusal.
    fn build(url: &str, ssl_verify: bool) -> Result<TagDeleteClient, CoreError> {
        TagDeleteClient::new(
            "td-new",
            &url::Url::parse(url).expect("url"),
            ssl_verify,
            "ign-cli",
            Secret::new("CANARY-user"),
            Secret::new("CANARY-pass"),
            Secret::new("CANARY-secret"),
        )
    }

    /// 26-18 security fix: the constructor itself refuses a transport that
    /// would expose the Basic credential or the secret, before any request
    /// is built, naming the scheme and host and no value.
    #[test]
    fn client_new_refuses_unsafe_transport() {
        for (url, ssl_verify, scheme, host) in [
            ("http://gw.example.com:8088", true, "http", "gw.example.com"),
            ("http://192.0.2.10:8088", true, "http", "192.0.2.10"),
            (
                "http://localhost.example.com",
                true,
                "http",
                "localhost.example.com",
            ),
            (
                "https://gw.example.com:8043",
                false,
                "https",
                "gw.example.com",
            ),
            ("https://192.0.2.10:8043", false, "https", "192.0.2.10"),
        ] {
            let err = build(url, ssl_verify).expect_err(url);
            assert_eq!(err.code(), "tag_delete_not_configured", "{url}");
            assert_eq!(err.exit_code(), 6);
            assert_eq!(err.endpoint(), None, "nothing was sent");
            let message = err.to_string();
            assert!(message.contains(&format!("scheme {scheme}")), "{message}");
            assert!(message.contains(&format!("host {host}")), "{message}");
            let rendered = format!("{err} {err:?} {}", err.hint().unwrap_or_default());
            assert!(!rendered.contains("CANARY"), "{rendered}");
        }
    }

    /// A URL with userinfo never echoes it in the refusal.
    #[test]
    fn client_new_refusal_never_echoes_userinfo() {
        let err = build("http://CANARY-u:CANARY-p@gw.example.com", true).expect_err("refused");
        let rendered = format!("{err} {err:?} {}", err.hint().unwrap_or_default());
        assert!(!rendered.contains("CANARY"), "{rendered}");
    }

    #[test]
    fn client_new_accepts_https_and_loopback_http() {
        for (url, ssl_verify) in [
            ("https://gw.example.com:8043", true),
            ("http://localhost:8088", true),
            ("http://127.0.0.1:18290", true),
            ("http://127.4.5.6:8088", true),
            ("http://[::1]:8088", true),
            // A loopback rig with a self-signed certificate.
            ("https://localhost:8043", false),
            ("https://127.0.0.1:8043", false),
        ] {
            build(url, ssl_verify).unwrap_or_else(|err| panic!("{url} {ssl_verify}: {err}"));
        }
    }

    // ---- 26-18 Task 2: configuration, readiness, chunks, clean answers ----

    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::config::{AuthRef, ENV_LOCK, Profile};

    const CANARY_USER: &str = "CANARY-td-user";
    const CANARY_PASSWORD: &str = "CANARY-td-pass";
    const CANARY_SECRET: &str = "c0ffeec0ffeec0ffeec0ffeec0ffeec0ffeec0ffeec0ffeec0ffeec0ffee0000";

    fn profile(url: &str, secret: Option<&str>) -> Profile {
        Profile {
            url: url::Url::parse(url).expect("url"),
            label: None,
            ssl_verify: true,
            auth: AuthRef::default(),
            webdev_secret: None,
            webdev_tag_delete_secret: secret.map(str::to_string),
            poll_interval_secs: None,
        }
    }

    /// Run `body` with the given env vars set (and removed after), under
    /// the crate's env lock.
    fn with_env<T>(vars: &[(&str, &str)], body: impl FnOnce() -> T) -> T {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
        // SAFETY: env mutation is serialized by ENV_LOCK; removed before return.
        unsafe {
            for (name, value) in vars {
                std::env::set_var(name, value);
            }
        }
        let out = body();
        // SAFETY: as above.
        unsafe {
            for (name, _) in vars {
                std::env::remove_var(name);
            }
        }
        out
    }

    fn missing_of(config: &TagDeleteConfig) -> Vec<String> {
        match config {
            TagDeleteConfig::NotConfigured { missing } => missing.clone(),
            TagDeleteConfig::Configured { .. } => Vec::new(),
        }
    }

    #[test]
    fn config_needs_secret_pair_and_safe_transport() {
        let env = [
            ("IGNITION_TAG_DELETE_USER_TD_CFG_A", CANARY_USER),
            ("IGNITION_TAG_DELETE_PASSWORD_TD_CFG_A", CANARY_PASSWORD),
        ];
        for url in [
            "https://gw.example.com:8043",
            "http://localhost:8088",
            "http://127.0.0.1:18290",
            "http://127.4.5.6:8088",
            "http://[::1]:8088",
        ] {
            let config = with_env(&env, || {
                tag_delete_config("td-cfg-a", &profile(url, Some(CANARY_SECRET)))
            });
            assert!(
                matches!(config, TagDeleteConfig::Configured { .. }),
                "{url}: {config:?}"
            );
        }
        for url in [
            "http://gw.example.com:8088",
            "http://192.0.2.10:8088",
            "http://localhost.example.com:8088",
            "http://[::2]:8088",
        ] {
            let config = with_env(&env, || {
                tag_delete_config("td-cfg-a", &profile(url, Some(CANARY_SECRET)))
            });
            assert_eq!(
                missing_of(&config),
                vec!["plain http to a non-loopback host".to_string()],
                "{url}"
            );
        }
    }

    #[test]
    fn config_names_every_missing_part_never_a_value() {
        // Nothing set at all.
        let config = with_env(&[], || {
            tag_delete_config("td-cfg-b", &profile("http://gw.example.com", None))
        });
        assert_eq!(
            missing_of(&config),
            vec![
                "webdev_tag_delete_secret".to_string(),
                "IGNITION_TAG_DELETE_USER_TD_CFG_B".to_string(),
                "IGNITION_TAG_DELETE_PASSWORD_TD_CFG_B".to_string(),
                "plain http to a non-loopback host".to_string(),
            ]
        );
        // The user only, and an empty password: the empty one is absent;
        // an empty stored secret is absent too.
        let config = with_env(
            &[
                ("IGNITION_TAG_DELETE_USER_TD_CFG_B", CANARY_USER),
                ("IGNITION_TAG_DELETE_PASSWORD_TD_CFG_B", ""),
            ],
            || tag_delete_config("td-cfg-b", &profile("https://gw.example.com", Some(""))),
        );
        assert_eq!(
            missing_of(&config),
            vec![
                "webdev_tag_delete_secret".to_string(),
                "IGNITION_TAG_DELETE_PASSWORD_TD_CFG_B".to_string(),
            ]
        );
        let err = config.client("td-cfg-b").expect_err("not configured");
        assert_eq!(err.code(), "tag_delete_not_configured");
        assert_eq!(err.exit_code(), 6);
        let rendered = format!(
            "{err} {err:?} {} {config:?}",
            err.hint().unwrap_or_default()
        );
        assert!(!rendered.contains("CANARY"), "{rendered}");
    }

    /// The caller-side check also refuses `ssl_verify = false` to a
    /// non-loopback host (26-18 security fix).
    #[test]
    fn config_refuses_unverified_tls_to_a_remote_host() {
        let env = [
            ("IGNITION_TAG_DELETE_USER_TD_CFG_D", CANARY_USER),
            ("IGNITION_TAG_DELETE_PASSWORD_TD_CFG_D", CANARY_PASSWORD),
        ];
        let mut remote = profile("https://gw.example.com:8043", Some(CANARY_SECRET));
        remote.ssl_verify = false;
        let config = with_env(&env, || tag_delete_config("td-cfg-d", &remote));
        assert_eq!(missing_of(&config), vec![UNVERIFIED_TLS_REMOTE.to_string()]);
        let mut rig = profile("https://localhost:8043", Some(CANARY_SECRET));
        rig.ssl_verify = false;
        let config = with_env(&env, || tag_delete_config("td-cfg-d", &rig));
        assert!(
            matches!(config, TagDeleteConfig::Configured { .. }),
            "{config:?}"
        );
    }

    #[test]
    fn env_names_follow_the_token_rule() {
        assert_eq!(
            crate::config::secret::tag_delete_env_names("plant-a"),
            (
                "IGNITION_TAG_DELETE_USER_PLANT_A".to_string(),
                "IGNITION_TAG_DELETE_PASSWORD_PLANT_A".to_string()
            )
        );
    }

    #[test]
    fn configured_values_never_render() {
        let config = with_env(
            &[
                ("IGNITION_TAG_DELETE_USER_TD_CFG_C", CANARY_USER),
                ("IGNITION_TAG_DELETE_PASSWORD_TD_CFG_C", CANARY_PASSWORD),
            ],
            || {
                tag_delete_config(
                    "td-cfg-c",
                    &profile("https://gw.example.com", Some(CANARY_SECRET)),
                )
            },
        );
        let debug = format!("{config:?}");
        assert!(!debug.contains("CANARY"), "{debug}");
        assert!(!debug.contains(CANARY_SECRET), "{debug}");
        let client = config.client("td-cfg-c").expect("configured");
        let debug = format!("{client:?}");
        assert!(
            !debug.contains("CANARY") && !debug.contains(CANARY_SECRET),
            "{debug}"
        );
    }

    /// A scripted route for the readiness wait.
    struct Scripted {
        answers: Mutex<Vec<Result<RouteHandshake, CoreError>>>,
        calls: AtomicUsize,
        repeat_last: bool,
    }

    impl Scripted {
        fn new(answers: Vec<Result<RouteHandshake, CoreError>>, repeat_last: bool) -> Self {
            let mut answers = answers;
            answers.reverse();
            Self {
                answers: Mutex::new(answers),
                calls: AtomicUsize::new(0),
                repeat_last,
            }
        }
    }

    #[async_trait::async_trait]
    impl TagDeleteRoute for Scripted {
        async fn handshake(&self) -> Result<RouteHandshake, CoreError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let mut answers = self.answers.lock().expect("lock");
            if self.repeat_last && answers.len() == 1 {
                return clone_answer(&answers[0]);
            }
            answers.pop().expect("a scripted answer")
        }
        async fn delete(&self, _: &str, _: &[String]) -> Result<DeleteAnswer, CoreError> {
            unreachable!("the readiness wait never deletes")
        }
    }

    fn clone_answer(
        answer: &Result<RouteHandshake, CoreError>,
    ) -> Result<RouteHandshake, CoreError> {
        match answer {
            Ok(handshake) => Ok(handshake.clone()),
            Err(CoreError::RoutesNotDeployed {
                project,
                route,
                endpoint,
            }) => Err(CoreError::RoutesNotDeployed {
                project: project.clone(),
                route: route.clone(),
                endpoint: endpoint.clone(),
            }),
            Err(CoreError::Internal(message)) => Err(CoreError::Internal(message.clone())),
            Err(other) => panic!("not scripted for repeat: {other}"),
        }
    }

    fn ready() -> RouteHandshake {
        RouteHandshake {
            route_version: ROUTE_BUNDLE_VERSION.into(),
            min_cli: "1.0".into(),
            providers: vec!["P".into()],
            max_paths: 2,
            max_path_chars: 8,
        }
    }

    fn absent() -> CoreError {
        CoreError::RoutesNotDeployed {
            project: "ign-cli".into(),
            route: TAG_DELETE_ROUTE.into(),
            endpoint: None,
        }
    }

    fn http_500() -> CoreError {
        CoreError::Internal(format!("{TAG_DELETE_HTTP_500} at test"))
    }

    const SHORT: Duration = Duration::from_millis(60);
    const POLL: Duration = Duration::from_millis(5);

    #[tokio::test]
    async fn wait_ready_rides_500_then_405() {
        let route = Scripted::new(vec![Err(http_500()), Err(absent()), Ok(ready())], false);
        let handshake = wait_ready(&route, SHORT, POLL).await.expect("ready");
        assert_eq!(handshake, ready());
        assert_eq!(route.calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn wait_ready_absent_until_timeout_is_routes_not_deployed() {
        let route = Scripted::new(vec![Err(absent())], true);
        let started = std::time::Instant::now();
        let err = wait_ready(&route, SHORT, POLL).await.expect_err("absent");
        assert!(started.elapsed() < Duration::from_secs(2), "bounded");
        assert!(route.calls.load(Ordering::SeqCst) >= 2, "retried");
        assert!(matches!(
            &err,
            CoreError::RoutesNotDeployed { route, .. } if route == TAG_DELETE_ROUTE
        ));
    }

    #[tokio::test]
    async fn wait_ready_refuses_at_once() {
        for err in [
            CoreError::Auth {
                status: 401,
                endpoint: None,
            },
            CoreError::Auth {
                status: 403,
                endpoint: None,
            },
            CoreError::WebdevUnlicensed { endpoint: None },
            CoreError::WebdevRouteError {
                code: "secret_mismatch".into(),
                message: "m".into(),
                endpoint: None,
            },
            CoreError::RouteVersionMismatch {
                route: TAG_DELETE_ROUTE.into(),
                deployed: "0.9.0".into(),
                expected: ROUTE_BUNDLE_VERSION.into(),
                endpoint: None,
            },
        ] {
            let slug = err.code();
            let route = Scripted::new(vec![Err(err), Ok(ready())], false);
            let got = wait_ready(&route, SHORT, POLL).await.expect_err("at once");
            assert_eq!(got.code(), slug);
            assert_eq!(route.calls.load(Ordering::SeqCst), 1, "{slug}: no retry");
        }
    }

    #[test]
    fn ready_constants() {
        assert_eq!(TAG_DELETE_READY_TIMEOUT_S, 30);
        assert_eq!(TAG_DELETE_READY_POLL_S, 2);
    }

    #[test]
    fn chunks_keep_order_and_respect_caps() {
        let paths = requested(&["A", "B", "C", "D", "E"]);
        let chunks = delete_chunks(&paths, &ready()).expect("chunks");
        assert_eq!(
            chunks,
            vec![
                requested(&["A", "B"]),
                requested(&["C", "D"]),
                requested(&["E"])
            ]
        );
        assert!(delete_chunks(&[], &ready()).expect("empty").is_empty());
        let long = requested(&["A", "Folder/Toolong"]);
        let err = delete_chunks(&long, &ready()).expect_err("too long");
        assert_eq!(err.code(), "deploy_tree_invalid");
        assert!(err.to_string().contains("Folder/Toolong"), "{err}");
    }

    /// 26-REVIEW IN-03: a route answering a `maxPaths` above the contract
    /// value (500, the route's `MAX_PATHS`) still gets calls of at most 500.
    #[test]
    fn chunks_never_exceed_the_contract_max_paths() {
        let names: Vec<String> = (0..1200).map(|i| format!("T{i}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let paths = requested(&refs);
        let wide = RouteHandshake {
            max_paths: 1_000_000,
            ..ready()
        };
        let chunks = delete_chunks(&paths, &wide).expect("chunks");
        assert_eq!(
            chunks.iter().map(Vec::len).collect::<Vec<_>>(),
            [500, 500, 200]
        );
        let route = include_str!(
            "../../webdev/routes/com.inductiveautomation.webdev/resources/cli/tagDelete/doPost.py"
        );
        // Line by line, so a CRLF checkout (Windows) matches too.
        let cap = format!("MAX_PATHS = {TAG_DELETE_MAX_PATHS}");
        assert!(
            route.lines().any(|line| line.trim() == cap),
            "the route's own cap"
        );
    }

    #[test]
    fn is_clean_needs_good_and_gone() {
        let answer = |results: Vec<PathResult>| DeleteAnswer {
            provider: "P".into(),
            results,
            elapsed_ms: 1,
        };
        assert!(answer(vec![row("A"), row("B")]).is_clean());
        let mut null = row("A");
        null.quality = None;
        null.good = None;
        assert!(
            answer(vec![null]).is_clean(),
            "null good with existsAfter false"
        );
        let mut bad = row("A");
        bad.quality = Some("Bad_NotFound".into());
        bad.good = Some(false);
        assert!(!answer(vec![row("B"), bad]).is_clean());
        // 26-17: `existsAfter after good delete: false`, so a path still
        // there is not clean even with a good code.
        let mut still = row("A");
        still.exists_after = Some(true);
        assert!(!answer(vec![still]).is_clean());
    }

    /// 26-REVIEW-D WR-D1: the route answers `existsAfter: null` when its
    /// `exists` read raised. The key stays required; null reads as not
    /// confirmed and is never clean.
    #[test]
    fn null_exists_after_is_read_and_never_clean() {
        let read: PathResult = serde_json::from_value(serde_json::json!({
            "path": "A", "quality": "Good", "good": true, "existsAfter": null
        }))
        .expect("null existsAfter is the contract");
        assert_eq!(read.exists_after, None);
        assert!(!read.removed());
        let answer = DeleteAnswer {
            provider: "P".into(),
            results: vec![row("B"), read],
            elapsed_ms: 1,
        };
        assert!(!answer.is_clean());
        assert!(row("B").removed());
    }
}
