//! Wiremock contract tests for the session-family capabilities (02-03,
//! HLTH-08): designer/Perspective/Vision lists + the three terminate
//! routes. Fixtures follow the openapi item shapes (02-RESEARCH
//! §Sessions + terminate).
//!
//! The crown pins here are the RECORDED-REQUEST proofs:
//! - the Perspective GET hits the EXACT trailing-slash path
//!   `/data/perspective/api/v1/sessions/` (Pitfall 8 — module-scoped
//!   prefix, base-URL joining must not collapse it);
//! - the Perspective DELETE carries `sessionId` as a QUERY param with an
//!   EMPTY body, against the no-trailing-slash path (spec).

mod common;

use common::IgnitionMock;
use ignition_core::client::sessions::{DesignerInfo, PerspectiveSession, VisionClient};
use ignition_core::client::{GatewayApi, ReqwestGatewayApi};
use ignition_core::error::CoreError;

const DESIGNERS_PATH: &str = "/data/api/v1/designers";
/// The EXACT trailing slash is part of the LIST contract (Pitfall 8).
const PERSPECTIVE_LIST_PATH: &str = "/data/perspective/api/v1/sessions/";
/// The DELETE route has NO trailing slash (spec).
const PERSPECTIVE_TERMINATE_PATH: &str = "/data/perspective/api/v1/sessions";
const VISION_CLIENTS_PATH: &str = "/data/vision/api/v1/clients";

/// Lowercased Debug dump of a recorded request's headers (01-04 pattern).
fn headers_debug(request: &wiremock::Request) -> String {
    format!("{:?}", request.headers).to_lowercase()
}

/// The designers list parses the openapi item shape (object `memory`,
/// ms numerics) and rides the standard list params through.
#[tokio::test]
async fn designers_parses_the_openapi_shape() {
    let mock = IgnitionMock::start().await;
    mock.list_json(
        "GET",
        DESIGNERS_PATH,
        serde_json::json!({
            "items": [
                {
                    "id": "d-1",
                    "user": "admin",
                    "uptime": 600000,
                    "lastcomm": 1787346747022i64,
                    "timeout": 3600000,
                    "memory": {"used": 268435456i64, "max": 1073741824i64},
                    "project": "MyProject",
                    "address": "192.168.1.50:52526",
                    "timezone": "America/New_York"
                }
            ],
            "metadata": {"total": 1, "matching": 1, "limit": -1, "offset": 0}
        }),
    )
    .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let page = api
        .designers(&Default::default())
        .await
        .expect("openapi designer shape must parse");
    assert_eq!(page.items.len(), 1);
    let designer: &DesignerInfo = &page.items[0];
    assert_eq!(designer.id, "d-1");
    assert_eq!(designer.user, "admin");
    assert_eq!(designer.uptime, 600000, "epoch ms");
    assert_eq!(
        designer.memory["used"], 268435456i64,
        "memory is an OBJECT — passthrough"
    );
    assert_eq!(page.metadata.total, 1);
}

/// THE Pitfall-8 pin: the Perspective sessions GET must hit the EXACT
/// trailing-slash path `/data/perspective/api/v1/sessions/` (a path
/// matcher without the slash would not fire) with the standard list
/// params — and the camelCase items parse through the renames.
#[tokio::test]
async fn perspective_sessions_pin_the_trailing_slash_list_path() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(PERSPECTIVE_LIST_PATH))
        .and(wiremock::matchers::query_param("limit", "-1"))
        .and(wiremock::matchers::query_param("offset", "0"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {
                        "id": "psess-1",
                        "username": "admin",
                        "authorized": true,
                        "project": "MyProject",
                        "clientAddress": "10.0.0.5",
                        "lastComm": 1787346747022i64,
                        "sessionScope": "G",
                        "activePages": 2,
                        "pageIds": ["viewA", "viewB"],
                        "recentBytesSent": 1024,
                        "totalBytesSent": 4096,
                        "userAgent": "Mozilla/5.0"
                    }
                ],
                "metadata": {"total": 1, "matching": 1, "limit": -1, "offset": 0}
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let page = api
        .perspective_sessions(&Default::default())
        .await
        .expect("exact trailing-slash path + camelCase shape must parse");
    let session: &PerspectiveSession = &page.items[0];
    assert_eq!(session.id, "psess-1");
    assert_eq!(session.client_address, "10.0.0.5", "clientAddress rename");
    assert_eq!(session.last_comm, 1787346747022, "lastComm rename");
    assert!(session.authorized);
    assert_eq!(
        session.extra.get("sessionScope"),
        Some(&serde_json::json!("G")),
        "unmodeled known keys round-trip"
    );
}

/// Vision clients = the designer shape + `tagCount`.
#[tokio::test]
async fn vision_clients_parse_designer_shape_plus_tag_count() {
    let mock = IgnitionMock::start().await;
    mock.list_json(
        "GET",
        VISION_CLIENTS_PATH,
        serde_json::json!({
            "items": [
                {
                    "id": "v-1",
                    "user": "operator",
                    "uptime": 120000,
                    "lastcomm": 1787346747022i64,
                    "timeout": 3600000,
                    "memory": {"used": 134217728i64, "max": 536870912i64},
                    "project": "PlantFloor",
                    "address": "10.0.0.9:443",
                    "timezone": "UTC",
                    "tagCount": 1523
                }
            ],
            "metadata": {"total": 1, "matching": 1, "limit": -1, "offset": 0}
        }),
    )
    .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let page = api
        .vision_clients(&Default::default())
        .await
        .expect("vision shape must parse");
    let client: &VisionClient = &page.items[0];
    assert_eq!(client.id, "v-1");
    assert_eq!(client.tag_count, 1523);
}

/// THE terminate-route pin (recorded-request proof): the Perspective
/// DELETE hits the NO-trailing-slash path with `sessionId` (+ optional
/// `message`) as QUERY params and an EMPTY body — the spec's shape, not
/// an invention — and the auth header rides along (authed mutation).
#[tokio::test]
async fn perspective_terminate_sends_query_param_and_no_body() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path(PERSPECTIVE_TERMINATE_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"terminated": 1})),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    // A credential is configured — mutations are authed reads' mirror.
    let api = ReqwestGatewayApi::for_tests(
        &server.uri(),
        Some(ignition_core::config::Credential::Token(
            ignition_core::config::Secret::new("name:key"),
        )),
    );
    api.terminate_perspective_session("psess-1", Some("maintenance restart"))
        .await
        .expect("200 {terminated: 1} classifies Ok");

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1, "exactly one DELETE");
    let request = &requests[0];
    let query = request.url.query().expect("query string present");
    assert!(
        query.contains("sessionId=psess-1"),
        "sessionId rides the QUERY string: {query}"
    );
    assert!(
        query.contains("message=maintenance+restart")
            || query.contains("message=maintenance%20restart"),
        "optional message rides the query string (form-encoded): {query}"
    );
    assert!(
        request.body.is_empty(),
        "the terminate DELETE carries NO body (spec)"
    );
    let headers = headers_debug(request);
    assert!(
        headers.contains("x-ignition-api-token"),
        "the mutation is authed: {headers}"
    );
}

/// Without a message, only `sessionId` rides the query.
#[tokio::test]
async fn perspective_terminate_without_message_sends_only_session_id() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path(PERSPECTIVE_TERMINATE_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"terminated": 1})),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    api.terminate_perspective_session("psess-1", None)
        .await
        .expect("terminates without a message");

    let requests = guard.received_requests().await;
    let query = requests[0].url.query().expect("query string present");
    assert!(
        query.contains("sessionId=psess-1") && !query.contains("message="),
        "no message param when none given: {query}"
    );
}

/// Terminating a nonexistent id: the gateway ANSWERS 404 (spec: "No
/// valid sessions found to close.") → `NotFound` (exit 6).
#[tokio::test]
async fn perspective_terminate_nonexistent_id_is_not_found() {
    let mock = IgnitionMock::start().await;
    mock.status_json(
        "DELETE",
        PERSPECTIVE_TERMINATE_PATH,
        404,
        serde_json::json!({"message": "No valid sessions found to close."}),
    )
    .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let err = api
        .terminate_perspective_session("nope", None)
        .await
        .expect_err("404 must fail");
    assert!(
        matches!(&err, CoreError::NotFound { .. }),
        "wrong class: {err}"
    );
    assert_eq!(err.exit_code(), 6);
    assert_eq!(err.code(), "not_found");
}

/// HTML 403 on the DELETE (Jetty page) → Auth (exit 5) — the
/// under-permitted-token case.
#[tokio::test]
async fn perspective_terminate_html_403_classifies_auth() {
    let mock = IgnitionMock::start().await;
    mock.html_error("DELETE", PERSPECTIVE_TERMINATE_PATH, 403)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let err = api
        .terminate_perspective_session("psess-1", None)
        .await
        .expect_err("403 must fail");
    assert!(
        matches!(&err, CoreError::Auth { status: 403, .. }),
        "wrong class: {err}"
    );
    assert_eq!(err.exit_code(), 5);
    assert_eq!(err.code(), "auth_rejected");
}

/// Vision terminate + designer prune hit their SINGULAR paths exactly
/// (`/data/vision/api/v1/client/{id}`, `/data/api/v1/designer/{id}`) —
/// recorded-request proof of both route shapes.
#[tokio::test]
async fn vision_terminate_and_designer_prune_hit_singular_paths() {
    let server = wiremock::MockServer::start().await;
    let vision = wiremock::Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path("/data/vision/api/v1/client/v-1"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"message": "Session terminated"})),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;
    let designer = wiremock::Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path("/data/api/v1/designer/d-1"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"message": "The session was pruned."})),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    api.terminate_vision_client("v-1")
        .await
        .expect("vision terminate classifies Ok");
    api.prune_designer("d-1")
        .await
        .expect("designer prune classifies Ok");

    for (guard, expected_path) in [
        (vision, "/data/vision/api/v1/client/v-1"),
        (designer, "/data/api/v1/designer/d-1"),
    ] {
        let requests = guard.received_requests().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url.path(), expected_path);
        assert!(requests[0].body.is_empty(), "neither DELETE carries a body");
    }
}

/// Terminating a nonexistent VISION id → 404 → NotFound (exit 6) — the
/// same class as the Perspective route (one taxonomy, no additions).
#[tokio::test]
async fn vision_terminate_nonexistent_id_is_not_found() {
    let mock = IgnitionMock::start().await;
    mock.status_json(
        "DELETE",
        "/data/vision/api/v1/client/nope",
        404,
        serde_json::json!({"message": "Client session not found"}),
    )
    .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let err = api
        .terminate_vision_client("nope")
        .await
        .expect_err("404 must fail");
    assert!(matches!(&err, CoreError::NotFound { .. }));
    assert_eq!(err.exit_code(), 6);
}

// ---------------------------------------------------------------------------
// DB/OPC connections (Task 2, HLTH-05/06) — the resource-list mechanism
// the web UI polls; the ignition-mcp `/connections/*` paths are
// inventions and appear nowhere. `healthchecks` is raw passthrough
// (LOW-confidence populated shape, research Open Question 1).
// ---------------------------------------------------------------------------

/// Both resource-list paths parse a plausible item
/// `{name, enabled, healthchecks: {…}}` through the passthrough model,
/// with the UI's `limit=-1` convention on the query (matcher-pinned).
#[tokio::test]
async fn database_connections_parse_with_limit_minus_one() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/resources/list/ignition/database-connection",
        ))
        .and(wiremock::matchers::query_param("limit", "-1"))
        .and(wiremock::matchers::query_param("offset", "0"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {
                        "name": "MyPostgres",
                        "enabled": true,
                        "healthchecks": {"jdbc": "FAIR"},
                        "collection": "database-connections"
                    }
                ],
                "metadata": {"total": 1, "matching": 1, "limit": -1, "offset": 0}
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let page = api
        .database_connections()
        .await
        .expect("resource list must parse");
    let connection = &page.items[0];
    assert_eq!(connection.name, "MyPostgres");
    assert!(connection.enabled);
    assert_eq!(connection.healthchecks["jdbc"], "FAIR");
    assert_eq!(page.metadata.total, 1);
}

/// The OPC family rides the same mechanism (path-pinned separately).
#[tokio::test]
async fn opc_connections_parse_the_same_mechanism() {
    let mock = IgnitionMock::start().await;
    mock.list_json(
        "GET",
        "/data/api/v1/resources/list/ignition/opc-connection",
        serde_json::json!({
            "items": [],
            "metadata": {"total": 0, "matching": 0, "limit": -1, "offset": 0}
        }),
    )
    .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let page = api
        .opc_connections()
        .await
        .expect("empty OPC list is the research gateway's observed state");
    assert_eq!(page.items.len(), 0, "the research rig had zero connections");
}

/// HTML 401 on the resource list → Auth (exit 5) — authed read, the
/// standard Jetty body.
#[tokio::test]
async fn database_connections_html_401_classifies_auth() {
    let mock = IgnitionMock::start().await;
    mock.html_error(
        "GET",
        "/data/api/v1/resources/list/ignition/database-connection",
        401,
    )
    .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let err = api.database_connections().await.expect_err("401 must fail");
    assert!(
        matches!(&err, CoreError::Auth { status: 401, .. }),
        "wrong class: {err}"
    );
    assert_eq!(err.exit_code(), 5);
}
