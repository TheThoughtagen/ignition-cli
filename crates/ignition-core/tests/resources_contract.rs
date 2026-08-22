//! Wiremock contract tests for the project-resource family (03-03,
//! PROJ-05) — ⚠ THE MEDIUM-confidence family: these paths exist only
//! in ignition-mcp (single source; absent from the official 83-api
//! collection — Phase 2 caught that client inventing paths), so every
//! fixture here is mcp-derived and the live-capture gate lives in
//! `crates/ignition-cli/tests/e2e_projects.rs` (openapi extract).
//!
//! The crown pins are RECORDED-REQUEST proofs, per the family rules:
//! - the `path=<prefix>` QUERY filter rides the request ONLY when a
//!   prefix is given (mcp's `params={"path": …}`) — no prefix, no
//!   param at all;
//! - a SPACED resource path hits the exact per-segment-encoded path
//!   with `/` separators intact (`…/resources/com%2Ex/views/
//!   My%20Folder/V1` — over-encoding the `.` is safe, the server
//!   decodes before matching);
//! - put carries the EXACT body bytes + the declared Content-Type;
//! - delete hits the exact path;
//! - 404 get → `not_found`; 401 Jetty HTML → `Auth` (the family
//!   classifies like every other).

mod common;

use common::IgnitionMock;
use ignition_core::client::{GatewayApi, ReqwestGatewayApi};
use ignition_core::error::CoreError;

/// The list parses a plausible mcp-shaped item (typed `path` +
/// passthrough extras) inside the standard `{items, metadata}`
/// envelope — and the recorded request proves NO `path` query param
/// rode the wire (the filter is opt-in).
#[tokio::test]
async fn resources_list_parses_without_prefix_param() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/projects/PlantFloor/resources",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {
                        "path": "com.inductiveautomation.perspective/views/Dashboard",
                        "scope": "A",
                        "version": 1,
                        "restricted": false
                    },
                    {
                        "path": "ignition/script-python/e2e/scratch",
                        "scope": "G"
                    }
                ],
                "metadata": {"total": 2, "matching": 2, "limit": -1, "offset": 0}
            })),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let page = api
        .project_resources("PlantFloor", None)
        .await
        .expect("plausible list shape must parse");
    assert_eq!(page.items.len(), 2);
    assert_eq!(
        page.items[0].path.as_deref(),
        Some("com.inductiveautomation.perspective/views/Dashboard")
    );
    assert_eq!(
        page.items[0].extra.get("scope"),
        Some(&serde_json::json!("A")),
        "unmodeled keys round-trip (MEDIUM shape, passthrough)"
    );

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0].url.query().is_none(),
        "NO prefix given = NO path param on the wire: {}",
        requests[0].url
    );
}

/// THE filter pin: `--prefix view` rides the wire as `path=view` —
/// and ONLY that param (no list-envelope limit/offset extras; mcp's
/// params carry just the prefix).
#[tokio::test]
async fn resources_list_prefix_rides_path_query_param() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/projects/PlantFloor/resources",
        ))
        .and(wiremock::matchers::query_param("path", "ignition/views"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"items": [], "metadata": {}})),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let page = api
        .project_resources("PlantFloor", Some("ignition/views"))
        .await
        .expect("prefix-filtered list classifies Ok");
    assert!(page.items.is_empty());

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].url.query(),
        Some("path=ignition%2Fviews".to_string()).as_deref(),
        "the prefix rides the path query param (its own / encodes): {}",
        requests[0].url
    );
}

/// THE Pitfall-6 pin: a SPACED resource path (mixed case, dot in the
/// module segment, space in the folder) hits the EXACT per-segment
/// encoded path with `/` separators intact — the recorded request
/// proves the wire form.
#[tokio::test]
async fn resource_get_encodes_spaced_path_exact() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/projects/My%20Proj/resources/com%2Ex/views/My%20Folder/V1",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                // set_body_raw: the resource body is JSON bytes with a
                // JSON content type — exactly as the gateway would.
                .set_body_raw(b"{\"scope\":\"A\"}".to_vec(), "application/json"),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let content = api
        .project_resource_get("My Proj", "com.x/views/My Folder/V1")
        .await
        .expect("encoded path classifies Ok");
    assert_eq!(content.bytes, b"{\"scope\":\"A\"}".to_vec());
    assert_eq!(content.content_type.as_deref(), Some("application/json"));

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].url.path(),
        "/data/api/v1/projects/My%20Proj/resources/com%2Ex/views/My%20Folder/V1",
        "per-segment encoding, slashes intact (project fully encoded)"
    );
}

/// THE put pin: the EXACT body bytes ride the request with the
/// DECLARED Content-Type (the actions-layer sniffer decides both —
/// the client seam passes them through untouched).
#[tokio::test]
async fn resource_put_carries_exact_bytes_and_content_type() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("PUT"))
        .and(wiremock::matchers::path(
            // over-encoding is safe: `-` rides as %2D (NON_ALPHANUMERIC)
            "/data/api/v1/projects/p/resources/ignition/script%2Dpython/e2e/scratch",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let body = br#"{"scope":"G","code":"print('hi')"}"#.to_vec();
    api.project_resource_put(
        "p",
        "ignition/script-python/e2e/scratch",
        body.clone(),
        "application/json",
    )
    .await
    .expect("put classifies Ok (upsert semantics)");

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].body, body,
        "the EXACT bytes ride the request, untransformed"
    );
    assert_eq!(
        requests[0]
            .headers
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("application/json"),
        "the DECLARED content type rides the request"
    );
    assert_eq!(
        requests[0].url.path(),
        "/data/api/v1/projects/p/resources/ignition/script%2Dpython/e2e/scratch",
        "the wire saw the per-segment-encoded path"
    );
}

/// The delete pin: DELETE hits the exact per-segment-encoded path
/// (authed, empty body — the destructive verb's wire shape).
#[tokio::test]
async fn resource_delete_hits_exact_path() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path(
            "/data/api/v1/projects/p/resources/com%2Ex/views/My%20Folder/V1",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(
        &server.uri(),
        Some(ignition_core::config::Credential::Token(
            ignition_core::config::Secret::new("name:key"),
        )),
    );
    api.project_resource_delete("p", "com.x/views/My Folder/V1")
        .await
        .expect("delete classifies Ok");

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].url.path(),
        "/data/api/v1/projects/p/resources/com%2Ex/views/My%20Folder/V1"
    );
    assert!(requests[0].body.is_empty(), "the DELETE carries no body");
    let headers = format!("{:?}", requests[0].headers).to_lowercase();
    assert!(
        headers.contains("x-ignition-api-token"),
        "the mutation is authed: {headers}"
    );
}

/// A nonexistent resource: 404 → `NotFound` (exit 6) — the
/// classification the surgical loop's `resource get` surfaces.
#[tokio::test]
async fn resource_get_nonexistent_is_not_found() {
    let mock = IgnitionMock::start().await;
    mock.status_json(
        "GET",
        "/data/api/v1/projects/p/resources/nope",
        404,
        serde_json::json!({"message": "Resource not found"}),
    )
    .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let err = api
        .project_resource_get("p", "nope")
        .await
        .expect_err("404 must fail");
    assert!(
        matches!(&err, CoreError::NotFound { .. }),
        "wrong class: {err}"
    );
    assert_eq!(err.exit_code(), 6);
    assert_eq!(err.code(), "not_found");
}

/// 401 Jetty HTML (header-less get against default security) →
/// `Auth` (exit 5) — the standard page that crashes naive `.json()`;
/// classify runs before any body consumption, binary or otherwise.
#[tokio::test]
async fn resource_get_html_401_classifies_auth() {
    let mock = IgnitionMock::start().await;
    mock.html_error("GET", "/data/api/v1/projects/p/resources/x", 401)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let err = api
        .project_resource_get("p", "x")
        .await
        .expect_err("401 must fail");
    assert!(
        matches!(&err, CoreError::Auth { status: 401, .. }),
        "wrong class: {err}"
    );
    assert_eq!(err.exit_code(), 5);
}

/// The list stays on the standard envelope path: the family does NOT
/// smuggle ListQuery params (limit/offset) the way other lists do —
/// mcp's list carries only the optional prefix. A spaced project
/// name encodes on the list path too (the same encoder everywhere).
#[tokio::test]
async fn resources_list_encodes_spaced_project_name() {
    let mock = IgnitionMock::start().await;
    mock.list_json(
        "GET",
        "/data/api/v1/projects/My%20Proj/resources",
        serde_json::json!({"items": [], "metadata": {"total": 0}}),
    )
    .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let page = api
        .project_resources("My Proj", None)
        .await
        .expect("spaced project name encodes (exact-path mock matched)");
    assert_eq!(page.metadata.total, 0);
}
