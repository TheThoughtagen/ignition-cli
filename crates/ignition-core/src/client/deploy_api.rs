//! The deploy wire: scan-lock and scan calls on a trait of their own.
//!
//! A separate trait by roadmap lock: [`super::GatewayApi`] has 15
//! hand-written test doubles, and the deploy wire must not force a change
//! on any of them. Every method here maps to exactly one [`EndpointDecl`]
//! in [`DEPLOY_ENDPOINTS`], whose declared [`ResponseShape`] is enforced in
//! one place, `send_shaped`. A state-changing call is confirmed by a
//! follow-up read, never by its status: the scan POST's own 200 is a
//! trigger-time snapshot (D4), and an acquire's 200 body adds nothing the
//! info read does not give.
//!
//! [`ReqwestGatewayApi::apply_auth`] is the only credential site; every
//! request built here passes through it before it is sent.
//!
//! Neither POST (lock acquire, scan trigger) is ever retried, refusals
//! included: a repeated non-idempotent call against a production gateway
//! is never silent. The poll engine wraps only the GET probes.

use std::time::Duration;

use serde::de::DeserializeOwned;

use crate::client::ReqwestGatewayApi;
use crate::client::scan::{LockObservation, LockRequest, ScanKind, ScanLockInfo, scan_lock_path};
use crate::error::CoreError;

/// Added to `acquireTimeout` for the per-request client timeout, so a
/// gateway that does wait out the acquire window answers before the client
/// gives up and reports a fake `network_error` (RESEARCH Pitfall 4). The
/// live gateway never waited (D1); the slack costs nothing when it does not.
pub const ACQUIRE_TIMEOUT_SLACK: Duration = Duration::from_secs(10);

/// Longest actor string taken from a gateway answer.
const ACTOR_CAP: usize = 256;

/// What a 2xx answer on an endpoint may look like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseShape {
    /// A non-empty JSON body.
    Json,
    /// Any 2xx; the body is discarded.
    EmptyOk,
    /// 204 (no body) or 200 with a JSON body. A 200 with an empty body is
    /// an error: only the 204 status means "nothing there".
    JsonOrNoContent,
}

/// One declared endpoint: its id, wire verb, path and 2xx shape, and the
/// capture block the shape was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointDecl {
    /// Stable id, also named by shape errors.
    pub id: &'static str,
    /// HTTP method.
    pub method: &'static str,
    /// Path with `{kind}` where the scan kind goes.
    pub path_template: &'static str,
    /// What a 2xx answer must look like.
    pub shape: ResponseShape,
    /// The `20-LIVE-CAPTURES.md` block(s) this row rests on.
    pub capture_ref: &'static str,
}

/// The declared deploy endpoints. Shapes are copied from the
/// `20-LIVE-CAPTURES.md` Endpoint shapes table.
pub const DEPLOY_ENDPOINTS: &[EndpointDecl] = &[
    EndpointDecl {
        id: "scan_lock_acquire",
        method: "POST",
        path_template: "/data/api/v1/scan-lock/{kind}",
        shape: ResponseShape::EmptyOk,
        capture_ref: "C1b",
    },
    EndpointDecl {
        id: "scan_lock_info",
        method: "GET",
        path_template: "/data/api/v1/scan-lock/{kind}",
        shape: ResponseShape::JsonOrNoContent,
        capture_ref: "C1c",
    },
];

/// Look a declared endpoint up by id.
pub fn endpoint_decl(id: &str) -> Option<&'static EndpointDecl> {
    DEPLOY_ENDPOINTS.iter().find(|decl| decl.id == id)
}

/// The declaration for a method of this file. Every call site names an id
/// that exists in [`DEPLOY_ENDPOINTS`] (pinned by a unit test), so a miss
/// is a programming error surfaced as `internal`, not a panic.
fn decl(id: &'static str) -> Result<&'static EndpointDecl, CoreError> {
    endpoint_decl(id).ok_or_else(|| CoreError::Internal(format!("no endpoint declaration {id}")))
}

/// Per-request client timeout of a lock acquire.
fn acquire_request_timeout(acquire_s: u64) -> Duration {
    Duration::from_secs(acquire_s).saturating_add(ACQUIRE_TIMEOUT_SLACK)
}

/// The deploy-facing gateway calls. Not part of [`super::GatewayApi`].
#[async_trait::async_trait]
pub trait DeployApi: Send + Sync {
    /// Acquire the scan-lock (`scan_lock_acquire`, capture C1b).
    ///
    /// Success is the gateway's 2xx; the 200 body is ignored. A lock held
    /// by anyone, the caller's own token included (D2), is refused with
    /// `scan_lock_held` naming the holder: taken from the refusal body
    /// (D1), else from ONE follow-up [`DeployApi::scan_lock_info`]. The
    /// POST is sent once per call. There is no release route (D8); the
    /// lock ends when the holder's scan runs (D12) or `hold_s` lapses.
    async fn scan_lock_acquire(
        &self,
        kind: ScanKind,
        acquire_s: u64,
        hold_s: u64,
    ) -> Result<(), CoreError>;

    /// Read the scan-lock (`scan_lock_info`, capture C1c): 200 with a body
    /// is `Held`, 204 is `Free`.
    async fn scan_lock_info(&self, kind: ScanKind) -> Result<LockObservation, CoreError>;
}

/// A 2xx body after the declared shape was enforced.
enum ShapedBody {
    /// The text of a JSON body.
    Json(String),
    /// An `EmptyOk` answer; the body was discarded.
    Empty,
    /// A 204.
    NoContent,
}

impl ReqwestGatewayApi {
    /// Send `request`, classify the answer, and enforce `decl.shape` on a
    /// 2xx. Keeps no state between requests: nothing here is shared, so
    /// concurrent callers cannot observe each other's statuses or bodies.
    async fn send_shaped(
        &self,
        decl: &EndpointDecl,
        request: reqwest::RequestBuilder,
        url: &url::Url,
    ) -> Result<ShapedBody, CoreError> {
        let response = self.send_and_classify(request, url).await?;
        let status = response.status();
        if decl.shape == ResponseShape::EmptyOk {
            return Ok(ShapedBody::Empty);
        }
        // Decided from the status, before any body read.
        if decl.shape == ResponseShape::JsonOrNoContent && status == reqwest::StatusCode::NO_CONTENT
        {
            return Ok(ShapedBody::NoContent);
        }
        let text = response.text().await.map_err(|err| CoreError::Network {
            url: url.to_string(),
            source: Some(err),
            observation: None,
        })?;
        if text.trim().is_empty() {
            return Err(CoreError::Internal(format!(
                "endpoint {} answered HTTP {status} from {url} with an empty body, \
                 but its declared shape is {:?}",
                decl.id, decl.shape
            )));
        }
        Ok(ShapedBody::Json(text))
    }
}

/// Parse a JSON body, naming the endpoint id and URL on failure and never
/// echoing the body.
fn parse_json<T: DeserializeOwned>(
    decl: &EndpointDecl,
    url: &url::Url,
    text: &str,
) -> Result<T, CoreError> {
    serde_json::from_str(text).map_err(|err| {
        CoreError::Internal(format!(
            "endpoint {} answered from {url} with a body that did not match the \
             expected shape: {err}",
            decl.id
        ))
    })
}

#[async_trait::async_trait]
impl DeployApi for ReqwestGatewayApi {
    async fn scan_lock_acquire(
        &self,
        kind: ScanKind,
        acquire_s: u64,
        hold_s: u64,
    ) -> Result<(), CoreError> {
        let decl = decl("scan_lock_acquire")?;
        let url = self.url_for(&scan_lock_path(kind));
        let request = self.apply_auth(
            self.client
                .post(url.clone())
                .json(&LockRequest {
                    acquire_timeout: acquire_s,
                    hold_timeout: hold_s,
                })
                .timeout(acquire_request_timeout(acquire_s)),
        );
        match self.send_shaped(decl, request, &url).await {
            Ok(_) => Ok(()),
            // The refusal did not name the holder: ONE follow-up read may.
            // Any enrichment failure keeps the original refusal, so the
            // held signal is never masked. The POST is not re-sent.
            Err(CoreError::ScanLockHeld {
                actor: None,
                kind: held_kind,
                endpoint,
            }) => {
                let actor = match self.scan_lock_info(kind).await {
                    Ok(LockObservation::Held(info)) => info
                        .actor
                        .map(|actor| actor.chars().take(ACTOR_CAP).collect::<String>()),
                    _ => None,
                };
                Err(CoreError::ScanLockHeld {
                    actor,
                    kind: held_kind,
                    endpoint,
                })
            }
            Err(other) => Err(other),
        }
    }

    async fn scan_lock_info(&self, kind: ScanKind) -> Result<LockObservation, CoreError> {
        let decl = decl("scan_lock_info")?;
        let url = self.url_for(&scan_lock_path(kind));
        let request = self.apply_auth(self.client.get(url.clone()));
        match self.send_shaped(decl, request, &url).await? {
            ShapedBody::NoContent => Ok(LockObservation::Free),
            ShapedBody::Json(text) => {
                parse_json::<ScanLockInfo>(decl, &url, &text).map(LockObservation::Held)
            }
            ShapedBody::Empty => Err(CoreError::Internal(format!(
                "endpoint {} produced no body shape",
                decl.id
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rows copied from the 20-LIVE-CAPTURES Endpoint shapes table.
    #[test]
    fn endpoint_rows_match_the_capture_table() {
        let expected: [(&str, &str, &str, ResponseShape, &str); 2] = [
            (
                "scan_lock_acquire",
                "POST",
                "/data/api/v1/scan-lock/{kind}",
                ResponseShape::EmptyOk,
                "C1b",
            ),
            (
                "scan_lock_info",
                "GET",
                "/data/api/v1/scan-lock/{kind}",
                ResponseShape::JsonOrNoContent,
                "C1c",
            ),
        ];
        assert_eq!(DEPLOY_ENDPOINTS.len(), expected.len());
        for (id, method, template, shape, capture) in expected {
            let row = endpoint_decl(id).unwrap_or_else(|| panic!("row {id}"));
            assert_eq!(row.method, method, "{id}");
            assert_eq!(row.path_template, template, "{id}");
            assert_eq!(row.shape, shape, "{id}");
            assert_eq!(row.capture_ref, capture, "{id}");
        }
        assert!(endpoint_decl("nope").is_none());
    }

    /// Every id a method of this file asks for resolves.
    #[test]
    fn every_method_id_is_declared() {
        for id in ["scan_lock_acquire", "scan_lock_info"] {
            assert!(decl(id).is_ok(), "{id}");
        }
    }

    /// T-20-15: the client waits at least as long as the gateway may.
    #[test]
    fn acquire_request_timeout_is_acquire_plus_slack() {
        assert_eq!(acquire_request_timeout(10), Duration::from_secs(20));
        assert_eq!(acquire_request_timeout(0), ACQUIRE_TIMEOUT_SLACK);
        assert_eq!(ACQUIRE_TIMEOUT_SLACK, Duration::from_secs(10));
    }
}
