//! Wiremock contract tests for the project CRUD family (03-01,
//! PROJ-01/02) — the HIGH-confidence native `/data/api/v1/projects/*`
//! paths. Fixtures follow the official 83-api body schemas.
//!
//! The crown pins here are the RECORDED-REQUEST proofs, not just
//! response parsing:
//! - find with a SPACED name hits exactly
//!   `/data/api/v1/projects/find/My%20Project` (Pitfall 6 — per-segment
//!   percent-encoding);
//! - a bare create body is EXACTLY `{"name":"x","enabled":true}` and
//!   optionals appear only when provided — never `"parent":""`
//!   (Pitfall 5);
//! - copy/rename bodies carry the official keys exactly
//!   (`fromName`/`toName`; rename's body key is `name`);
//! - the modify PUT body carries NO `name` key;
//! - the DELETE carries `confirm=true` as a QUERY param with an EMPTY
//!   body (Pitfall 8 — the server's own guard rides the query string).

mod common;

use common::IgnitionMock;
use ignition_core::client::projects::{ProjectCreate, ProjectModify, ProjectRecord};
use ignition_core::client::query::ListQuery;
use ignition_core::client::{GatewayApi, ReqwestGatewayApi};
use ignition_core::error::CoreError;

const PROJECTS_LIST_PATH: &str = "/data/api/v1/projects/list";
const PROJECTS_CREATE_PATH: &str = "/data/api/v1/projects";
const PROJECTS_COPY_PATH: &str = "/data/api/v1/projects/copy";

/// The list parses a plausible item (typed core + `defaultDb` +
/// passthrough extras surviving) with the UI's `limit=-1` convention
/// matcher-pinned on the query.
#[tokio::test]
async fn projects_list_parses_with_passthrough_extras() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(PROJECTS_LIST_PATH))
        .and(wiremock::matchers::query_param("limit", "-1"))
        .and(wiremock::matchers::query_param("offset", "0"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {
                        "name": "PlantFloor",
                        "title": "Plant Floor",
                        "description": "Line control screens",
                        "enabled": true,
                        "parent": "Base",
                        "inheritable": true,
                        "defaultDb": "MyPostgres",
                        "tagProvider": "default",
                        "userSource": "default",
                        "runtimeUsageFlags": 0
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
        .projects(&ListQuery::default())
        .await
        .expect("plausible list shape must parse");
    let project: &ProjectRecord = &page.items[0];
    assert_eq!(project.name, "PlantFloor");
    assert_eq!(project.title.as_deref(), Some("Plant Floor"));
    assert_eq!(project.parent.as_deref(), Some("Base"));
    assert_eq!(project.inheritable, Some(true));
    assert_eq!(project.default_db.as_deref(), Some("MyPostgres"));
    assert_eq!(
        project.extra.get("runtimeUsageFlags"),
        Some(&serde_json::json!(0)),
        "unmodeled known keys round-trip (MEDIUM shape, passthrough)"
    );
    assert_eq!(page.metadata.total, 1);
}

/// THE Pitfall-6 pin: find with a SPACED, mixed-case name hits the
/// EXACT percent-encoded path — the recorded request proves the wire
/// saw `/find/My%20Project`, never a raw space.
#[tokio::test]
async fn project_find_encodes_spaced_name_exact_path() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/projects/find/My%20Project",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "name": "My Project",
                "enabled": true
            })),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let record = api
        .project_find("My Project")
        .await
        .expect("encoded path classifies Ok");
    assert_eq!(record.name, "My Project");
    assert!(record.enabled);

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].url.path(),
        "/data/api/v1/projects/find/My%20Project",
        "the wire saw the percent-encoded segment"
    );
}

/// THE Pitfall-5 pin: a bare create's recorded body is EXACTLY
/// `{"name":"x","enabled":true}` (serde_json value equality) — no
/// `"parent":""`, no null keys.
#[tokio::test]
async fn project_create_bare_body_is_exactly_name_and_enabled() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(PROJECTS_CREATE_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"status": "success"})),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let body = ProjectCreate {
        name: "x".into(),
        enabled: true,
        title: None,
        description: None,
        parent: None,
        inheritable: None,
        default_db: None,
        tag_provider: None,
        user_source: None,
    };
    api.project_create(&body)
        .await
        .expect("Ok classification IS the success contract");

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    let recorded: serde_json::Value =
        serde_json::from_slice(&requests[0].body).expect("recorded body is JSON");
    assert_eq!(
        recorded,
        serde_json::json!({"name": "x", "enabled": true}),
        "bare create body is exactly name+enabled"
    );
}

/// Optionals ride the create body only when provided — and never as
/// empty strings.
#[tokio::test]
async fn project_create_optionals_appear_only_when_provided() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(PROJECTS_CREATE_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let body = ProjectCreate {
        name: "child".into(),
        enabled: true,
        title: Some("Child".into()),
        description: None,
        parent: Some("Base".into()),
        inheritable: Some(true),
        default_db: None,
        tag_provider: None,
        user_source: None,
    };
    api.project_create(&body)
        .await
        .expect("create classifies Ok");

    let requests = guard.received_requests().await;
    let recorded: serde_json::Value =
        serde_json::from_slice(&requests[0].body).expect("recorded body is JSON");
    let object = recorded.as_object().expect("body is an object");
    assert_eq!(object.len(), 5, "only the provided fields: {recorded}");
    assert_eq!(object["title"], "Child");
    assert_eq!(object["parent"], "Base");
    assert_eq!(object["inheritable"], true);
    assert!(
        !object.contains_key("description")
            && !object.contains_key("defaultDb")
            && !object.contains_key("tagProvider")
            && !object.contains_key("userSource"),
        "absent optionals are OMITTED entirely: {recorded}"
    );
    assert!(
        requests[0]
            .body
            .windows(11)
            .all(|w| w != b"\"parent\":\"\""),
        "never an empty-string parent reference: {:?}",
        String::from_utf8_lossy(&requests[0].body)
    );
}

/// The copy body is EXACTLY `{"fromName":"a","toName":"b"}` (official
/// body keys — serde-renamed from the snake_case fields).
#[tokio::test]
async fn project_copy_body_is_exactly_from_to() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(PROJECTS_COPY_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    api.project_copy("a", "b")
        .await
        .expect("copy classifies Ok");

    let requests = guard.received_requests().await;
    let recorded: serde_json::Value =
        serde_json::from_slice(&requests[0].body).expect("recorded body is JSON");
    assert_eq!(
        recorded,
        serde_json::json!({"fromName": "a", "toName": "b"}),
        "official body keys exactly"
    );
}

/// The rename body is EXACTLY `{"name":"new"}` against the
/// `/rename/{old}` path.
#[tokio::test]
async fn project_rename_body_is_exactly_name() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/data/api/v1/projects/rename/Old"))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    api.project_rename("Old", "New")
        .await
        .expect("rename classifies Ok");

    let requests = guard.received_requests().await;
    assert_eq!(requests[0].url.path(), "/data/api/v1/projects/rename/Old");
    let recorded: serde_json::Value =
        serde_json::from_slice(&requests[0].body).expect("recorded body is JSON");
    assert_eq!(
        recorded,
        serde_json::json!({"name": "New"}),
        "the official body key carries the NEW name"
    );
}

/// THE modify pin: the PUT body carries NO `name` key — only the
/// provided fields — against the `/{name}` path.
#[tokio::test]
async fn project_modify_put_body_carries_no_name() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("PUT"))
        .and(wiremock::matchers::path("/data/api/v1/projects/x"))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let body = ProjectModify {
        enabled: None,
        title: Some("T".into()),
        description: None,
        parent: Some("Base".into()),
        inheritable: None,
        default_db: None,
        tag_provider: None,
        user_source: None,
    };
    api.project_modify("x", &body)
        .await
        .expect("modify classifies Ok");

    let requests = guard.received_requests().await;
    let recorded: serde_json::Value =
        serde_json::from_slice(&requests[0].body).expect("recorded body is JSON");
    assert_eq!(
        recorded,
        serde_json::json!({"title": "T", "parent": "Base"}),
        "only provided fields; enabled untouched; NO name key"
    );
}

/// THE Pitfall-8 pin: the DELETE carries `confirm=true` as a QUERY
/// param with an EMPTY body, and the auth header rides along (authed
/// mutation).
#[tokio::test]
async fn project_delete_carries_confirm_true_query_and_empty_body() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path("/data/api/v1/projects/x"))
        .and(wiremock::matchers::query_param("confirm", "true"))
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
    api.project_delete("x").await.expect("delete classifies Ok");

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    let query = request.url.query().expect("query string present");
    assert!(
        query.contains("confirm=true"),
        "the server's own guard rides the query string: {query}"
    );
    assert!(
        request.body.is_empty(),
        "the DELETE carries NO body (confirm is a query param)"
    );
    let headers = format!("{:?}", request.headers).to_lowercase();
    assert!(
        headers.contains("x-ignition-api-token"),
        "the mutation is authed: {headers}"
    );
}

/// A spaced project name encodes on the DELETE path too (the same
/// per-segment encoder everywhere).
#[tokio::test]
async fn project_delete_encodes_spaced_name() {
    let mock = IgnitionMock::start().await;
    mock.status_json(
        "DELETE",
        "/data/api/v1/projects/My%20Project",
        404,
        serde_json::json!({"message": "Project not found"}),
    )
    .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let err = api
        .project_delete("My Project")
        .await
        .expect_err("404 must fail (and proves the encoded path matched)");
    assert!(
        matches!(&err, CoreError::NotFound { .. }),
        "wrong class: {err}"
    );
    assert_eq!(err.exit_code(), 6);
}

/// Finding a nonexistent project: 404 → `NotFound` (exit 6) — the
/// classification 03-02's collision pre-check leans on.
#[tokio::test]
async fn project_find_nonexistent_is_not_found() {
    let mock = IgnitionMock::start().await;
    mock.status_json(
        "GET",
        "/data/api/v1/projects/find/nope",
        404,
        serde_json::json!({"message": "Project not found"}),
    )
    .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let err = api.project_find("nope").await.expect_err("404 must fail");
    assert!(
        matches!(&err, CoreError::NotFound { .. }),
        "wrong class: {err}"
    );
    assert_eq!(err.exit_code(), 6);
    assert_eq!(err.code(), "not_found");
}

/// 401 Jetty HTML (header-less create against default security) →
/// `Auth` (exit 5) — the standard page that crashes naive `.json()`.
#[tokio::test]
async fn project_create_html_401_classifies_auth() {
    let mock = IgnitionMock::start().await;
    mock.html_error("POST", PROJECTS_CREATE_PATH, 401).await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let err = api
        .project_create(&ProjectCreate {
            name: "x".into(),
            enabled: true,
            title: None,
            description: None,
            parent: None,
            inheritable: None,
            default_db: None,
            tag_provider: None,
            user_source: None,
        })
        .await
        .expect_err("401 must fail");
    assert!(
        matches!(&err, CoreError::Auth { status: 401, .. }),
        "wrong class: {err}"
    );
    assert_eq!(err.exit_code(), 5);
}
