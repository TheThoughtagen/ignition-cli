//! The deploy wire: config resources, scan-lock and scan calls on a trait of
//! their own.
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

use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;

use crate::client::config_resources::{
    MutationOutcome, ResourceCreate, ResourceName, ResourceRecord, ResourceUpdate,
    list_query_pairs, resource_find_path, resource_list_path, resource_names_path,
    resource_singleton_path, resource_write_path, validate_segment,
};
use crate::client::mode::{
    GatewayModeProbe, MODE_PATH, ModeCreate, ModeRecord, normalize_deployment_mode,
};
use crate::client::query::{ListEnvelope, ListQuery};
use crate::client::scan::{
    LockObservation, LockRequest, ScanKind, ScanLockInfo, ScanOutcome, ScanStatus, scan_complete,
    scan_lock_path, scan_path,
};
use crate::client::{GATEWAY_INFO_PATH, ReqwestGatewayApi, classify};
use crate::error::{CoreError, SignatureReason};
use crate::poll::{self, PollConfig, PollState};

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
    /// Path with `{kind}` where the scan kind goes, and `{module}`, `{type}`
    /// and `{name}` where a resource route takes them.
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
        id: "resource_names",
        method: "GET",
        path_template: "/data/api/v1/resources/names/{module}/{type}",
        shape: ResponseShape::Json,
        capture_ref: "C-list",
    },
    EndpointDecl {
        id: "resource_list",
        method: "GET",
        path_template: "/data/api/v1/resources/list/{module}/{type}",
        shape: ResponseShape::Json,
        capture_ref: "C-list",
    },
    EndpointDecl {
        id: "resource_find",
        method: "GET",
        path_template: "/data/api/v1/resources/find/{module}/{type}/{name}",
        shape: ResponseShape::Json,
        capture_ref: "C3a",
    },
    EndpointDecl {
        id: "resource_singleton",
        method: "GET",
        path_template: "/data/api/v1/resources/singleton/{module}/{type}",
        shape: ResponseShape::Json,
        capture_ref: "C-list",
    },
    EndpointDecl {
        id: "resource_create",
        method: "POST",
        path_template: "/data/api/v1/resources/{module}/{type}",
        shape: ResponseShape::Json,
        capture_ref: "C3a",
    },
    EndpointDecl {
        id: "resource_update",
        method: "PUT",
        path_template: "/data/api/v1/resources/{module}/{type}",
        shape: ResponseShape::Json,
        capture_ref: "C3a",
    },
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
    EndpointDecl {
        id: "scan_status",
        method: "GET",
        path_template: "/data/api/v1/scan/{kind}",
        shape: ResponseShape::Json,
        capture_ref: "C1i",
    },
    EndpointDecl {
        id: "scan_request",
        method: "POST",
        path_template: "/data/api/v1/scan/{kind}",
        shape: ResponseShape::EmptyOk,
        capture_ref: "C1i",
    },
    EndpointDecl {
        id: "modes",
        method: "GET",
        path_template: "/data/api/v1/mode",
        shape: ResponseShape::Json,
        capture_ref: "C-mode",
    },
    EndpointDecl {
        id: "mode_create",
        method: "POST",
        path_template: "/data/api/v1/mode",
        shape: ResponseShape::EmptyOk,
        capture_ref: "C-mode",
    },
    EndpointDecl {
        id: "deployment_mode",
        method: "GET",
        path_template: "/data/api/v1/gateway-info",
        shape: ResponseShape::Json,
        capture_ref: "C-null",
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
    /// List the names of a resource type (`resource_names`, capture
    /// C-list "GET names"): each item carries the collections (`modes`)
    /// that hold it. The answer is the `{items, metadata}` envelope; a
    /// type with no resources is Ok with no items. Gateway order is kept.
    async fn resource_names(
        &self,
        module: &str,
        ty: &str,
    ) -> Result<ListEnvelope<ResourceName>, CoreError>;

    /// List the resources of a type (`resource_list`, capture C-list "GET
    /// list"). Decision D7: this sees the EFFECTIVE view only, one item per
    /// name, so a shadowed collection cannot be listed here; use
    /// [`resource_list_in_collection`] for that. The gateway reads a filter
    /// as its own `field[op]=value` parameter, so `q.filter` must be of
    /// that form (see `config_resources::list_query_pairs`). Gateway order
    /// is kept.
    async fn resource_list(
        &self,
        module: &str,
        ty: &str,
        q: &ListQuery,
    ) -> Result<ListEnvelope<ResourceRecord>, CoreError>;

    /// Read one resource by name (`resource_find`, captures C3a / C-list).
    /// `None` sends no `collection` parameter and reads the effective view;
    /// `Some(c)` sends exactly `collection=c` and reads that copy. The two
    /// carry different signatures (D7). An absent name is `not_found`.
    async fn resource_find(
        &self,
        module: &str,
        ty: &str,
        name: &str,
        collection: Option<&str>,
    ) -> Result<ResourceRecord, CoreError>;

    /// Read a singleton resource (`resource_singleton`, capture C-list
    /// "GET singleton"), from the effective view or from `collection`.
    async fn resource_singleton(
        &self,
        module: &str,
        ty: &str,
        collection: Option<&str>,
    ) -> Result<ResourceRecord, CoreError>;

    /// Create resources (`resource_create`), `POST` of an array body:
    /// DAPI-02 as amended 2026-10-12 (create by POST, update by the
    /// signature-checked array PUT). Capture C3a and Decision D10 are the
    /// 8.3.6 confirmation: `[{name, config}]` answers 200 with
    /// `{success, changes[].newSignature}`.
    ///
    /// A 2xx whose body says `success:false` is an error, never Ok. A
    /// duplicate name is a 409 (D10) and is `resource_exists` (exit 6). The POST is sent once
    /// per call and never retried: a repeated create against a production
    /// gateway must never be silent. An empty `items` is `invalid_input`
    /// with no request.
    async fn resource_create(
        &self,
        module: &str,
        ty: &str,
        items: &[ResourceCreate],
    ) -> Result<MutationOutcome, CoreError>;

    /// Update resources (`resource_update`), `PUT` of an array body whose
    /// every item carries a name and a signature: DAPI-02 as amended
    /// 2026-10-12. Capture C3a and Decision D10 are the 8.3.6
    /// confirmation: a `PUT` of an unknown name answers a bare 404
    /// (`not_found`), so a create is never done by PUT.
    ///
    /// Decision D3 fixes the refusals. A stale signature is a 500 whose
    /// `problem.message` says `signature mismatch`, and it comes back as
    /// `resource_signature_rejected` (stale, exit 6). An omitted or empty
    /// signature is one byte-identical 422 with `fieldName` `signature`
    /// (the two cannot be told apart) and comes back as the same error
    /// (missing); an empty signature is refused before any request. The
    /// refusals leave the resource unchanged. Neither error carries the
    /// signature or the gateway's problem text. Every successful write
    /// rotates the signature, so an update reusing the pre-write one is
    /// stale. The PUT is sent once per call and never retried. When two
    /// writers race, the gateway's check serializes them and the loser
    /// gets the stale refusal; the client adds no lock of its own.
    async fn resource_update(
        &self,
        module: &str,
        ty: &str,
        items: &[ResourceUpdate],
    ) -> Result<MutationOutcome, CoreError>;

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

    /// Read the scan state (`scan_status`, capture C1i).
    async fn scan_status(&self, kind: ScanKind) -> Result<ScanStatus, CoreError>;

    /// Trigger a scan (`scan_request`, capture C1i). The POST answering 200
    /// is NEVER completion: its body is a trigger-time snapshot (D4), so it
    /// is ignored; completion comes from [`DeployApi::scan_status`] via
    /// `scan_complete`. The scan-lock does not block another actor's scan
    /// (D4: a non-holder's POST answered 200 and ran), so no refusal is
    /// classified on this route. The POST is sent once per call.
    async fn scan_request(&self, kind: ScanKind) -> Result<(), CoreError>;

    /// List the deployment modes (`modes`, capture C-mode "GET /mode"):
    /// `limit=-1` and `offset=0` go out, the answer is the `{items,
    /// metadata}` envelope, and gateway order is kept. A gateway with no
    /// modes is Ok with no items.
    async fn modes(&self) -> Result<ListEnvelope<ModeRecord>, CoreError>;

    /// Define a deployment mode (`mode_create`, capture C-mode, D6).
    ///
    /// The name is validated before any request (`invalid_input`). The POST
    /// is sent exactly once and its 2xx body is ignored; success is
    /// confirmed by a follow-up [`DeployApi::modes`] read, and the listed
    /// record is returned. A 2xx whose mode is absent from that list is
    /// `internal`. A duplicate (409) is `mode_exists` (exit 6) and an
    /// illegal name (400) is `invalid_input` (exit 2), never Ok. This only DEFINES a mode: it never activates one, restarts
    /// a gateway or sets a JVM argument.
    async fn mode_create(&self, body: &ModeCreate) -> Result<ModeRecord, CoreError>;

    /// The gateway's active deployment mode (`deployment_mode`, captures
    /// C-null, D5): `None` when `deploymentMode` is `null`, `""`, absent
    /// or whitespace-only (no mode), else the name verbatim. Read through
    /// a private probe, so `GatewayInfo` is untouched.
    async fn deployment_mode(&self) -> Result<Option<String>, CoreError>;
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

    /// GET `path` with `pairs`, enforce the declared shape and parse the
    /// JSON body into `T`. The shared tail of the four resource reads.
    async fn read_resource_json<T: DeserializeOwned>(
        &self,
        id: &'static str,
        path: &str,
        pairs: &[(String, String)],
    ) -> Result<T, CoreError> {
        let decl = decl(id)?;
        let url = self.url_for(path);
        let mut request = self.client.get(url.clone());
        if !pairs.is_empty() {
            request = request.query(pairs);
        }
        let request = self.apply_auth(request);
        match self.send_shaped(decl, request, &url).await? {
            ShapedBody::Json(text) => parse_json(decl, &url, &text),
            ShapedBody::Empty | ShapedBody::NoContent => Err(CoreError::Internal(format!(
                "endpoint {} produced no body",
                decl.id
            ))),
        }
    }

    /// Send a resource write and decide its outcome. Deploy-scoped: this is
    /// the ONLY place a signature refusal gets its typed
    /// [`CoreError::ResourceSignatureRejected`], so the v1.1
    /// EAM routes (a 500 signature refusal is `internal` there) keep their
    /// behaviour, and `classify` gains no signature arm.
    ///
    /// Decision D3 (capture C3a) keys the guard:
    /// - a 500 whose JSON `problem.message` contains [`SIGNATURE_MARKER`]
    ///   is a stale signature; any other 500 is `internal`;
    /// - a 422 whose `fieldMessages` name the field `signature` is a
    ///   missing signature (omitted and empty are the same bytes); any
    ///   other 422 goes to the shared resource 422 mapping;
    /// - every other status goes to `classify` unchanged.
    ///
    /// On a 2xx the body is parsed: `success:true` is Ok, `success:false`
    /// is an error, stale when the problem carries the marker, `internal`
    /// otherwise. No error built here carries the signature, the problem
    /// message or the stacktrace, and the request is never retried.
    async fn send_resource_write(
        &self,
        decl: &EndpointDecl,
        request: reqwest::RequestBuilder,
        url: &url::Url,
        names: &[&str],
    ) -> Result<MutationOutcome, CoreError> {
        let network = |err: reqwest::Error| CoreError::Network {
            url: url.to_string(),
            source: Some(err),
            observation: None,
        };
        let response = request.send().await.map_err(network)?;
        let status = response.status();
        if status == reqwest::StatusCode::INTERNAL_SERVER_ERROR {
            let body = response.text().await.unwrap_or_default();
            if problem_has_marker(&body) {
                return Err(signature_rejected(
                    names,
                    SignatureReason::Stale,
                    url,
                    Some(&body),
                ));
            }
            return Err(CoreError::Internal(format!(
                "unexpected HTTP {status} from {url} (the gateway's problem text is withheld)"
            )));
        }
        if status == reqwest::StatusCode::UNPROCESSABLE_ENTITY {
            let body = response.text().await.unwrap_or_default();
            if names_signature_field(&body) {
                return Err(signature_rejected(
                    names,
                    SignatureReason::Missing,
                    url,
                    None,
                ));
            }
            return Err(classify::resource_body_rejected(url.as_str(), &body));
        }
        // D10: a duplicate create is a 409 with a JSON `problem` naming the
        // resource. Scoped to the create endpoint; a 409 on any other write
        // keeps the shared mapping.
        if status == reqwest::StatusCode::CONFLICT && decl.id == "resource_create" {
            let body = response.text().await.unwrap_or_default();
            return Err(CoreError::ResourceExists {
                name: resource_name_for(names, Some(&body)),
                endpoint: Some(url.to_string()),
            });
        }
        let response = classify::classify(response, url.as_str(), false).await?;
        let text = response.text().await.map_err(network)?;
        if text.trim().is_empty() {
            return Err(CoreError::Internal(format!(
                "endpoint {} answered HTTP {status} from {url} with an empty body, \
                 but its declared shape is {:?}",
                decl.id, decl.shape
            )));
        }
        let outcome: MutationOutcome = parse_json(decl, url, &text)?;
        if outcome.success {
            return Ok(outcome);
        }
        let stale = outcome
            .problem
            .as_ref()
            .is_some_and(|problem| problem.message.contains(SIGNATURE_MARKER));
        if stale {
            let message = outcome.problem.map(|p| p.message);
            return Err(signature_rejected(
                names,
                SignatureReason::Stale,
                url,
                message.as_deref(),
            ));
        }
        Err(CoreError::Internal(format!(
            "endpoint {} answered HTTP {status} from {url} with success:false \
             (the gateway's problem text is withheld)",
            decl.id
        )))
    }
}

/// The marker D3 recorded for a stale signature: the lowercase phrase in
/// the 500 body's `problem.message` (`MODIFY illegal: signature mismatch
/// for 'ResourceId{...}'`). It is the same discriminator v1.1 recorded for
/// the EAM routes; frames and prose around it drift between versions.
const SIGNATURE_MARKER: &str = "signature mismatch";

/// Does `body` parse as a mutation envelope whose problem message carries
/// [`SIGNATURE_MARKER`]?
fn problem_has_marker(body: &str) -> bool {
    serde_json::from_str::<MutationOutcome>(body)
        .ok()
        .and_then(|outcome| outcome.problem)
        .is_some_and(|problem| problem.message.contains(SIGNATURE_MARKER))
}

/// Does a 422 body name the field `signature` in `fieldMessages` (D3:
/// `Missing 'signature' parameter`)?
fn names_signature_field(body: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|parsed| parsed["fieldMessages"].as_array().cloned())
        .is_some_and(|fields| {
            fields
                .iter()
                .any(|field| field["fieldName"].as_str() == Some("signature"))
        })
}

/// The resource a refusal is about: the batch item whose name appears in the
/// gateway's resource path (`message`), else the only item, else
/// `<batch of N>`. Never echoes the message itself.
fn resource_name_for(names: &[&str], message: Option<&str>) -> String {
    let from_message = message.and_then(|message| {
        names
            .iter()
            .find(|name| message.contains(&format!("/{name},")))
            .map(|name| (*name).to_string())
    });
    from_message.unwrap_or_else(|| match names {
        [only] => (*only).to_string(),
        _ => format!("<batch of {}>", names.len()),
    })
}

/// Build the typed [`CoreError::ResourceSignatureRejected`] without the
/// signature or the problem text. The resource is named by
/// [`resource_name_for`].
fn signature_rejected(
    names: &[&str],
    reason: SignatureReason,
    url: &url::Url,
    message: Option<&str>,
) -> CoreError {
    CoreError::ResourceSignatureRejected {
        name: resource_name_for(names, message),
        reason,
        endpoint: Some(url.to_string()),
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
    async fn resource_names(
        &self,
        module: &str,
        ty: &str,
    ) -> Result<ListEnvelope<ResourceName>, CoreError> {
        let path = resource_names_path(module, ty)?;
        self.read_resource_json("resource_names", &path, &[]).await
    }

    async fn resource_list(
        &self,
        module: &str,
        ty: &str,
        q: &ListQuery,
    ) -> Result<ListEnvelope<ResourceRecord>, CoreError> {
        let path = resource_list_path(module, ty)?;
        let pairs = list_query_pairs(q)?;
        self.read_resource_json("resource_list", &path, &pairs)
            .await
    }

    async fn resource_find(
        &self,
        module: &str,
        ty: &str,
        name: &str,
        collection: Option<&str>,
    ) -> Result<ResourceRecord, CoreError> {
        let path = resource_find_path(module, ty, name)?;
        self.read_resource_json("resource_find", &path, &collection_pairs(collection))
            .await
    }

    async fn resource_singleton(
        &self,
        module: &str,
        ty: &str,
        collection: Option<&str>,
    ) -> Result<ResourceRecord, CoreError> {
        let path = resource_singleton_path(module, ty)?;
        self.read_resource_json("resource_singleton", &path, &collection_pairs(collection))
            .await
    }

    async fn resource_create(
        &self,
        module: &str,
        ty: &str,
        items: &[ResourceCreate],
    ) -> Result<MutationOutcome, CoreError> {
        let path = resource_write_path(module, ty)?;
        if items.is_empty() {
            return Err(CoreError::InvalidInput {
                reason: "resource create needs at least one item".to_string(),
            });
        }
        if items.iter().any(|item| item.name.is_empty()) {
            return Err(CoreError::InvalidInput {
                reason: "resource name must not be empty".to_string(),
            });
        }
        let decl = decl("resource_create")?;
        let url = self.url_for(&path);
        let request = self.apply_auth(self.client.post(url.clone()).json(items));
        let names: Vec<&str> = items.iter().map(|item| item.name.as_str()).collect();
        self.send_resource_write(decl, request, &url, &names).await
    }

    async fn resource_update(
        &self,
        module: &str,
        ty: &str,
        items: &[ResourceUpdate],
    ) -> Result<MutationOutcome, CoreError> {
        let path = resource_write_path(module, ty)?;
        if items.is_empty() {
            return Err(CoreError::InvalidInput {
                reason: "resource update needs at least one item".to_string(),
            });
        }
        let decl = decl("resource_update")?;
        let url = self.url_for(&path);
        let request = self.apply_auth(self.client.put(url.clone()).json(items));
        let names: Vec<&str> = items.iter().map(|item| item.name.as_str()).collect();
        self.send_resource_write(decl, request, &url, &names).await
    }

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

    async fn scan_status(&self, kind: ScanKind) -> Result<ScanStatus, CoreError> {
        let decl = decl("scan_status")?;
        let url = self.url_for(&scan_path(kind));
        let request = self.apply_auth(self.client.get(url.clone()));
        match self.send_shaped(decl, request, &url).await? {
            ShapedBody::Json(text) => parse_json(decl, &url, &text),
            ShapedBody::Empty | ShapedBody::NoContent => Err(CoreError::Internal(format!(
                "endpoint {} produced no body",
                decl.id
            ))),
        }
    }

    async fn scan_request(&self, kind: ScanKind) -> Result<(), CoreError> {
        let decl = decl("scan_request")?;
        let url = self.url_for(&scan_path(kind));
        let request = self.apply_auth(self.client.post(url.clone()));
        self.send_shaped(decl, request, &url).await.map(|_| ())
    }

    async fn modes(&self) -> Result<ListEnvelope<ModeRecord>, CoreError> {
        let decl = decl("modes")?;
        let url = self.url_for(MODE_PATH);
        let request = self.apply_auth(
            self.client
                .get(url.clone())
                .query(&ListQuery::default().to_query_pairs()),
        );
        match self.send_shaped(decl, request, &url).await? {
            ShapedBody::Json(text) => parse_json(decl, &url, &text),
            ShapedBody::Empty | ShapedBody::NoContent => Err(CoreError::Internal(format!(
                "endpoint {} produced no body",
                decl.id
            ))),
        }
    }

    async fn mode_create(&self, body: &ModeCreate) -> Result<ModeRecord, CoreError> {
        validate_segment("mode name", &body.name)?;
        decl("mode_create")?;
        let url = self.url_for(MODE_PATH);
        let request = self.apply_auth(self.client.post(url.clone()).json(body));
        // Exactly one POST: a repeated create is never silent. D6: a
        // duplicate is a 409 and an illegal name a 400, both Jetty HTML;
        // both are typed here (deploy-scoped, so classify stays as is) and
        // neither echoes the gateway's page.
        let response = request.send().await.map_err(|err| CoreError::Network {
            url: url.to_string(),
            source: Some(err),
            observation: None,
        })?;
        match response.status() {
            reqwest::StatusCode::CONFLICT => {
                return Err(CoreError::ModeExists {
                    name: body.name.clone(),
                    endpoint: Some(url.to_string()),
                });
            }
            reqwest::StatusCode::BAD_REQUEST => {
                return Err(CoreError::InvalidInput {
                    reason: format!(
                        "gateway rejected the deployment mode name {:?} (HTTP 400 from {url})",
                        body.name
                    ),
                });
            }
            _ => {}
        }
        // Success is any 2xx (EmptyOk); every other status is classified.
        classify::classify(response, url.as_str(), false).await?;
        // The 2xx is not the confirmation (D6): the list is.
        let listed = self.modes().await?;
        listed
            .items
            .into_iter()
            .find(|mode| mode.name == body.name)
            .ok_or_else(|| {
                CoreError::Internal(format!(
                    "mode_create answered 2xx from {url} but mode {:?} is not in the \
                     follow-up mode list",
                    body.name
                ))
            })
    }

    async fn deployment_mode(&self) -> Result<Option<String>, CoreError> {
        let decl = decl("deployment_mode")?;
        let url = self.url_for(GATEWAY_INFO_PATH);
        let request = self.apply_auth(self.client.get(url.clone()));
        match self.send_shaped(decl, request, &url).await? {
            ShapedBody::Json(text) => {
                let probe: GatewayModeProbe = parse_json(decl, &url, &text)?;
                Ok(normalize_deployment_mode(probe.deployment_mode))
            }
            ShapedBody::Empty | ShapedBody::NoContent => Err(CoreError::Internal(format!(
                "endpoint {} produced no body",
                decl.id
            ))),
        }
    }
}

/// The `collection` query pair, only when a collection was asked for.
fn collection_pairs(collection: Option<&str>) -> Vec<(String, String)> {
    collection
        .map(|c| vec![("collection".to_string(), c.to_string())])
        .unwrap_or_default()
}

/// Read every resource of a type from one NAMED collection.
///
/// Decision D7: `list` sees the effective view only, so a collection that
/// is shadowed (C-list: a `core` copy under a `production` override) cannot
/// be listed. This composes the two calls that can: `names` (each item's
/// `modes` says which collections hold it), then `find?collection=` per
/// name. Results keep the order of `names`.
///
/// A name whose `modes` is non-empty and lacks `collection` is skipped
/// without a request. A `not_found` for a name means it is absent from that
/// collection and is skipped; any other error propagates. Because the
/// captures never show what `find?collection=` answers for a collection
/// that does not hold the name, a record that comes back stamped with a
/// different collection is skipped too, never returned as if it were
/// `collection`'s copy.
pub async fn resource_list_in_collection(
    api: &dyn DeployApi,
    module: &str,
    ty: &str,
    collection: &str,
) -> Result<Vec<ResourceRecord>, CoreError> {
    if collection.is_empty() {
        return Err(CoreError::InvalidInput {
            reason: "resource collection must not be empty".to_string(),
        });
    }
    let names = api.resource_names(module, ty).await?;
    let mut records = Vec::new();
    for item in names.items {
        if !item.modes.is_empty() && !item.modes.iter().any(|m| m == collection) {
            continue;
        }
        match api
            .resource_find(module, ty, &item.name, Some(collection))
            .await
        {
            Ok(record) => {
                if record
                    .collection
                    .as_deref()
                    .is_none_or(|held| held == collection)
                {
                    records.push(record);
                }
            }
            Err(CoreError::NotFound { .. }) => {}
            Err(other) => return Err(other),
        }
    }
    Ok(records)
}

/// Everything a wait probe reads, owned by the poll loop.
#[derive(Clone, Copy)]
struct WaitCtx<'a> {
    api: &'a dyn DeployApi,
    kind: ScanKind,
    started: Instant,
    budget: Duration,
    /// The pre-trigger reading; `None` during the first phase.
    before: Option<&'a ScanStatus>,
}

/// What one wait phase reached.
enum Waited {
    /// The phase's condition was met; carries the reading that met it.
    Reached(ScanStatus),
    /// The budget ran out first; carries the last reading.
    OutOfBudget(ScanStatus),
}

/// Trigger a scan and wait until it is done: `scanActive` is false and
/// `lastScanTimestamp` exceeds the pre-trigger value read from the same
/// gateway. An HTTP 200 on the trigger is never success on its own.
///
/// Steps: read the status; if a scan is already active, wait it out BEFORE
/// triggering, so its completion is never counted as ours (D4); POST the
/// trigger exactly once; then poll the status until
/// [`scan_complete`]. The budget is `cfg.deadline`, measured here with a
/// local monotonic clock; a gateway timestamp is never compared with it.
/// When the budget runs out the answer is `Ok(NotComplete)`, not an error.
///
/// `crate::poll::poll` drives both waits, so transient network and
/// restarting answers are retried and an auth failure fails fast. The
/// probe owns the budget check, and poll's own deadline (budget plus two
/// intervals) is only a transport backstop, so `Err` means a transport or
/// auth failure, not a slow scan.
pub async fn scan_and_wait(
    api: &dyn DeployApi,
    kind: ScanKind,
    cfg: PollConfig,
) -> Result<ScanOutcome, CoreError> {
    let started = Instant::now();
    let budget = cfg.deadline;
    let backstop = budget.saturating_add(cfg.interval.saturating_mul(2));
    let poll_cfg = PollConfig {
        deadline: backstop,
        ..cfg
    };

    // The probe state carries every borrow: poll lends the probe a
    // `&mut` of it, which ties the borrows' lifetimes to the poll call.
    let mut ctx = WaitCtx {
        api,
        kind,
        started,
        budget,
        before: None,
    };

    // Phase 1: the pre-trigger reading, taken while no scan is active.
    let before = match poll::poll(poll_cfg.clone(), ctx, |ctx| {
        Box::pin(async move {
            let status = ctx.api.scan_status(ctx.kind).await?;
            if !status.scan_active {
                return Ok(PollState::Done(Waited::Reached(status)));
            }
            if ctx.started.elapsed() >= ctx.budget {
                return Ok(PollState::Done(Waited::OutOfBudget(status)));
            }
            Ok(PollState::Pending(Some(
                "a scan is already active — waiting it out before triggering".to_string(),
            )))
        })
    })
    .await?
    {
        Waited::Reached(before) => before,
        Waited::OutOfBudget(last) => {
            return Ok(ScanOutcome::NotComplete {
                last,
                waited: started.elapsed(),
            });
        }
    };

    // Exactly one trigger POST, never retried.
    api.scan_request(kind).await?;

    // Phase 2: wait for inactive plus an advanced timestamp.
    ctx = WaitCtx {
        before: Some(&before),
        ..ctx
    };
    let outcome = poll::poll(poll_cfg, ctx, |ctx| {
        Box::pin(async move {
            let now = ctx.api.scan_status(ctx.kind).await?;
            let Some(before) = ctx.before else {
                return Err(CoreError::Internal(
                    "scan wait lost its pre-trigger reading".to_string(),
                ));
            };
            if scan_complete(before, &now) {
                return Ok(PollState::Done(Waited::Reached(now)));
            }
            if ctx.started.elapsed() >= ctx.budget {
                return Ok(PollState::Done(Waited::OutOfBudget(now)));
            }
            Ok(PollState::Pending(Some(format!(
                "scanActive {} lastScanTimestamp {} (before {})",
                now.scan_active, now.last_scan_timestamp, before.last_scan_timestamp
            ))))
        })
    })
    .await?;
    Ok(match outcome {
        Waited::Reached(now) => ScanOutcome::Complete(now),
        Waited::OutOfBudget(last) => ScanOutcome::NotComplete {
            last,
            waited: started.elapsed(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rows copied from the 20-LIVE-CAPTURES Endpoint shapes table.
    #[test]
    fn endpoint_rows_match_the_capture_table() {
        let expected: [(&str, &str, &str, ResponseShape, &str); 13] = [
            (
                "resource_names",
                "GET",
                "/data/api/v1/resources/names/{module}/{type}",
                ResponseShape::Json,
                "C-list",
            ),
            (
                "resource_list",
                "GET",
                "/data/api/v1/resources/list/{module}/{type}",
                ResponseShape::Json,
                "C-list",
            ),
            (
                "resource_find",
                "GET",
                "/data/api/v1/resources/find/{module}/{type}/{name}",
                ResponseShape::Json,
                "C3a",
            ),
            (
                "resource_singleton",
                "GET",
                "/data/api/v1/resources/singleton/{module}/{type}",
                ResponseShape::Json,
                "C-list",
            ),
            (
                "resource_create",
                "POST",
                "/data/api/v1/resources/{module}/{type}",
                ResponseShape::Json,
                "C3a",
            ),
            (
                "resource_update",
                "PUT",
                "/data/api/v1/resources/{module}/{type}",
                ResponseShape::Json,
                "C3a",
            ),
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
            (
                "scan_status",
                "GET",
                "/data/api/v1/scan/{kind}",
                ResponseShape::Json,
                "C1i",
            ),
            (
                "scan_request",
                "POST",
                "/data/api/v1/scan/{kind}",
                ResponseShape::EmptyOk,
                "C1i",
            ),
            (
                "modes",
                "GET",
                "/data/api/v1/mode",
                ResponseShape::Json,
                "C-mode",
            ),
            (
                "mode_create",
                "POST",
                "/data/api/v1/mode",
                ResponseShape::EmptyOk,
                "C-mode",
            ),
            (
                "deployment_mode",
                "GET",
                "/data/api/v1/gateway-info",
                ResponseShape::Json,
                "C-null",
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
        for id in [
            "resource_names",
            "resource_list",
            "resource_find",
            "resource_singleton",
            "resource_create",
            "resource_update",
            "scan_lock_acquire",
            "scan_lock_info",
            "scan_status",
            "scan_request",
            "modes",
            "mode_create",
            "deployment_mode",
        ] {
            assert!(decl(id).is_ok(), "{id}");
        }
    }

    /// DAPI-06: the table is closed. Thirteen rows, in trait declaration
    /// order, unique ids, every row backed by a capture block, every verb
    /// one of GET, POST and PUT.
    #[test]
    fn deploy_endpoints_cover_every_method() {
        let ids: Vec<&str> = DEPLOY_ENDPOINTS.iter().map(|row| row.id).collect();
        assert_eq!(
            ids,
            [
                "resource_names",
                "resource_list",
                "resource_find",
                "resource_singleton",
                "resource_create",
                "resource_update",
                "scan_lock_acquire",
                "scan_lock_info",
                "scan_status",
                "scan_request",
                "modes",
                "mode_create",
                "deployment_mode",
            ]
        );
        assert_eq!(DEPLOY_ENDPOINTS.len(), 13);
        let unique: std::collections::BTreeSet<&str> = ids.iter().copied().collect();
        assert_eq!(unique.len(), ids.len(), "ids are unique");
        for row in DEPLOY_ENDPOINTS {
            assert!(!row.capture_ref.is_empty(), "{}", row.id);
            assert!(
                matches!(row.method, "GET" | "POST" | "PUT"),
                "{} uses {}",
                row.id,
                row.method
            );
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
