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
    /// `user` / `password` and the route `secret`. The transport rule
    /// (https, or http to a loopback host) is the caller's: outside tests,
    /// build clients from the profile's tag-delete configuration.
    pub fn new(
        url: &url::Url,
        ssl_verify: bool,
        project: &str,
        user: Secret,
        password: Secret,
        secret: Secret,
    ) -> Result<Self, CoreError> {
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
            exists_after: false,
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
}
