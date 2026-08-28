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
//!
//! 03-02 appends the export/import half (PROJ-03/04):
//! - export STREAMS the ZIP body byte-for-byte to disk with the
//!   `Content-Disposition` filename + byte count in the meta
//!   (`set_body_raw` — the set_body_string-forces-text/plain gotcha);
//! - import's recorded request proves the exact encoded path, the
//!   `overwrite=true`/`false` QUERY variants, the
//!   `Content-Type: application/zip` header, and the body bytes.

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

/// A deterministic ZIP fixture — real `PK\x03\x04` magic + a payload
/// (a real export is binary; `set_body_raw` keeps it bytes — the
/// set_body_string-forces-text/plain gotcha).
fn zip_fixture() -> Vec<u8> {
    let mut bytes = vec![0x50, 0x4B, 0x03, 0x04];
    bytes.extend_from_slice(b"project-export-fixture");
    bytes
}

/// THE streaming pin: the export ZIP lands on disk BYTE-FOR-BYTE
/// (no buffering, no transformation) with the disposition filename +
/// chunk-counted byte total in the meta — against the exact
/// percent-encoded export path.
#[tokio::test]
async fn project_export_streams_fixture_byte_for_byte() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/projects/export/My%20Proj",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_raw(zip_fixture(), "application/zip")
                .insert_header("Content-Disposition", "attachment; filename=\"MyProj.zip\""),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("download.zip");
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let meta = api
        .project_export_to_file("My Proj", &out)
        .await
        .expect("export streams and classifies Ok");

    let fixture = zip_fixture();
    let on_disk = std::fs::read(&out).expect("file written");
    assert_eq!(on_disk, fixture, "byte-for-byte, exactly as received");
    assert_eq!(meta.bytes, fixture.len() as u64, "meta counts the chunks");
    assert_eq!(meta.filename.as_deref(), Some("MyProj.zip"));
    assert_eq!(meta.content_type.as_deref(), Some("application/zip"));

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].url.path(),
        "/data/api/v1/projects/export/My%20Proj",
        "the spaced name rode the wire percent-encoded"
    );
}

/// THE import recorded-request pin: exact encoded path,
/// `overwrite=true` AND `overwrite=false` QUERY variants, the
/// `Content-Type: application/zip` header, and the body bytes — the
/// known-Content-Length raw upload.
#[tokio::test]
async fn project_import_records_path_query_headers_and_body() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/data/api/v1/projects/import/My%20Proj",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"status": "imported"})),
        )
        .expect(2)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    for overwrite in [true, false] {
        let outcome = api
            .project_import("My Proj", zip_fixture(), overwrite)
            .await
            .expect("import classifies Ok");
        assert_eq!(
            outcome.response,
            serde_json::json!({"status": "imported"}),
            "a JSON body parses through"
        );
    }

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 2, "one POST per overwrite variant");
    let fixture = zip_fixture();
    for (index, overwrite) in [true, false].into_iter().enumerate() {
        let request = &requests[index];
        assert_eq!(request.url.path(), "/data/api/v1/projects/import/My%20Proj");
        let query = request.url.query().expect("query present");
        assert_eq!(
            query,
            format!("overwrite={}", if overwrite { "true" } else { "false" }),
            "the collision policy rides the QUERY string: {query}"
        );
        let content_type = request
            .headers
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .expect("content-type present");
        assert_eq!(content_type, "application/zip");
        assert_eq!(
            request.body, fixture,
            "the raw ZIP bytes are the body (known Content-Length)"
        );
        assert!(
            request
                .headers
                .get("content-length")
                .is_some_and(|value| value == &fixture.len().to_string()),
            "Content-Length announces the full body up front"
        );
    }
}

/// The opaque-success fallback: a NON-JSON 2xx body (the restart
/// `literal true` family) parses into `{"status":"success"}` instead
/// of erroring.
#[tokio::test]
async fn project_import_non_json_body_falls_back_to_success() {
    let mock = IgnitionMock::start().await;
    mock.literal_true("POST", "/data/api/v1/projects/import/x")
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let outcome = api
        .project_import("x", zip_fixture(), false)
        .await
        .expect("a literal-true body is still a success");
    assert_eq!(outcome.response, serde_json::json!({"status": "success"}));
}

/// 401 Jetty HTML on the import POST → `Auth` (exit 5) — classify
/// runs BEFORE any body consumption, streaming or otherwise.
#[tokio::test]
async fn project_import_html_401_classifies_auth() {
    let mock = IgnitionMock::start().await;
    mock.html_error("POST", "/data/api/v1/projects/import/x", 401)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let err = api
        .project_import("x", zip_fixture(), false)
        .await
        .expect_err("401 must fail");
    assert!(
        matches!(&err, CoreError::Auth { status: 401, .. }),
        "wrong class: {err}"
    );
    assert_eq!(err.exit_code(), 5);
}

// ---- Cross-gateway diff (07-01, SYNC-01) ----
//
// TWO MockServer instances — one per gateway side — with real-zip
// export fixtures; the crown pins are the REQUEST-SEQUENCE proof
// (exactly ONE export GET per side, ZERO import POSTs anywhere: a
// diff is a read and must never mutate) and the end-to-end
// normalization pass (same-content/differing-attribute members
// report `same` through the whole action, not just the units).

/// Build a real export zip: `project.json` + one member per pair, in
/// order (the diff engine walks the archive — honest fixtures).
fn diff_export_zip(project_json: &[u8], members: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write as _;
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    writer
        .start_file("project.json", options)
        .expect("project.json starts");
    writer.write_all(project_json).expect("project.json writes");
    for (name, bytes) in members {
        writer.start_file(*name, options).expect("member starts");
        writer.write_all(bytes).expect("member writes");
    }
    writer.finish().expect("zip finalizes").into_inner()
}

/// A live-shaped folder descriptor carrying the two volatility fields.
fn volatile_descriptor(timestamp: &str) -> Vec<u8> {
    format!(
        r#"{{"scope":"G","version":1,"files":["script.py"],"attributes":{{"lastModification":{{"actor":"a","timestamp":"{timestamp}"}},"lastModificationSignature":"sig-{timestamp}"}}}}"#
    )
    .into_bytes()
}

/// Mount the export GET (200 + the zip body) with `expect(n)` and a
/// zero-expect import POST guard on the same server.
async fn mount_export_pin(server: &wiremock::MockServer, project: &str, zip: Vec<u8>, n: u64) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(format!(
            "/data/api/v1/projects/export/{project}"
        )))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_raw(zip, "application/zip"))
        .expect(n)
        .mount(server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(format!(
            "/data/api/v1/projects/import/{project}"
        )))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(0)
        .mount(server)
        .await;
}

/// THE diff sequence pin: exactly ONE export GET per side, ZERO
/// imports anywhere — and the normalization pass holds END-TO-END
/// (same-content members with differing lastModification attributes
/// report `same` through the whole action), alongside the raw
/// direction semantics (added = in B only; changed = differing bytes).
#[tokio::test]
async fn project_diff_two_exports_zero_imports_normalized_same() {
    let server_a = wiremock::MockServer::start().await;
    let server_b = wiremock::MockServer::start().await;
    let zip_a = diff_export_zip(
        br#"{"title":"T","enabled":true}"#,
        &[
            (
                "ignition/resources/script-python/uat/resource.json",
                volatile_descriptor("2026-08-28T10:00:00Z").as_slice(),
            ),
            (
                "ignition/resources/script-python/uat/script.py",
                b"print('old')",
            ),
        ],
    );
    let zip_b = diff_export_zip(
        br#"{"title":"T","enabled":true}"#,
        &[
            (
                "ignition/resources/script-python/uat/resource.json",
                volatile_descriptor("2026-08-28T11:30:00Z").as_slice(),
            ),
            (
                "ignition/resources/script-python/uat/script.py",
                b"print('new')",
            ),
            (
                "com.example/resources/views/Fresh/view.json",
                br#"{"scope":"G"}"#.as_slice(),
            ),
        ],
    );
    mount_export_pin(&server_a, "p", zip_a, 1).await;
    mount_export_pin(&server_b, "p", zip_b, 1).await;

    let api_a = ReqwestGatewayApi::for_tests(&server_a.uri(), None);
    let api_b = ReqwestGatewayApi::for_tests(&server_b.uri(), None);
    let result = ignition_core::actions::projects::project_diff(
        &api_a,
        &api_b,
        "p",
        "gateway-a",
        "gateway-b",
    )
    .await
    .expect("diff orchestrates Ok");

    // The flat agent shape: ALL keys always, scope literal.
    assert_eq!(result.scope, "project");
    assert_eq!(result.profile_a, "gateway-a");
    assert_eq!(result.profile_b, "gateway-b");
    assert_eq!(result.project, "p");
    assert!(
        result.project_meta.is_empty(),
        "identical project.json metas"
    );
    assert_eq!(
        result.summary.same, 1,
        "the volatile descriptor normalized to same"
    );
    assert_eq!(result.summary.added, 1);
    assert_eq!(result.summary.removed, 0);
    assert_eq!(result.summary.changed, 1, "the script bytes differ");
    let by_path = |path: &str| {
        result
            .entries
            .iter()
            .find(|entry| entry.path == path)
            .unwrap_or_else(|| panic!("entry {path:?} present"))
            .status
    };
    assert_eq!(
        by_path("ignition/script-python/uat/resource.json"),
        ignition_core::client::resources::MemberStatus::Same
    );
    assert_eq!(
        by_path("ignition/script-python/uat/script.py"),
        ignition_core::client::resources::MemberStatus::Changed
    );
    assert_eq!(
        by_path("com.example/views/Fresh/view.json"),
        ignition_core::client::resources::MemberStatus::Added
    );
}

/// A differing project.json title rides `project_meta` (never the
/// member entries) end-to-end — the exclusion contract through the
/// action.
#[tokio::test]
async fn project_diff_surfaces_project_meta_delta() {
    let server_a = wiremock::MockServer::start().await;
    let server_b = wiremock::MockServer::start().await;
    mount_export_pin(
        &server_a,
        "p",
        diff_export_zip(br#"{"title":"Old","enabled":true}"#, &[]),
        1,
    )
    .await;
    mount_export_pin(
        &server_b,
        "p",
        diff_export_zip(br#"{"title":"New","enabled":true}"#, &[]),
        1,
    )
    .await;

    let api_a = ReqwestGatewayApi::for_tests(&server_a.uri(), None);
    let api_b = ReqwestGatewayApi::for_tests(&server_b.uri(), None);
    let result = ignition_core::actions::projects::project_diff(
        &api_a,
        &api_b,
        "p",
        "gateway-a",
        "gateway-b",
    )
    .await
    .expect("diff orchestrates Ok");
    assert_eq!(
        result.entries.len(),
        0,
        "no resource members in either export"
    );
    assert_eq!(result.project_meta.len(), 1);
    assert_eq!(result.project_meta[0].field, "title");
    assert_eq!(result.project_meta[0].a, "Old");
    assert_eq!(result.project_meta[0].b, "New");
}

/// A missing project on side B surfaces through export's existing
/// not-found path (exit 6) — side A's export already fired (the
/// honest order: A first, then B).
#[tokio::test]
async fn project_diff_missing_project_on_b_is_not_found() {
    let server_a = wiremock::MockServer::start().await;
    let server_b = wiremock::MockServer::start().await;
    mount_export_pin(&server_a, "p", diff_export_zip(br#"{"title":"T"}"#, &[]), 1).await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/projects/export/p"))
        .respond_with(
            wiremock::ResponseTemplate::new(404)
                .set_body_json(serde_json::json!({"message": "No project p"})),
        )
        .expect(1)
        .mount(&server_b)
        .await;

    let api_a = ReqwestGatewayApi::for_tests(&server_a.uri(), None);
    let api_b = ReqwestGatewayApi::for_tests(&server_b.uri(), None);
    let err = ignition_core::actions::projects::project_diff(
        &api_a,
        &api_b,
        "p",
        "gateway-a",
        "gateway-b",
    )
    .await
    .expect_err("the missing side-B project must fail");
    assert!(
        matches!(err, CoreError::NotFound { .. }),
        "wrong class: {err}"
    );
    assert_eq!(err.exit_code(), 6);
    assert_eq!(err.code(), "not_found");
}
