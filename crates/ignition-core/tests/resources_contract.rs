//! Wiremock contract tests for the resource family re-point (05-02,
//! closing the Phase 3 cross-phase defect): every op orchestrates
//! export → zip-member surgery → import(overwrite) over the REAL
//! client — no `/projects/{p}/resources/**` request ever rides the
//! wire (those routes do not exist on real 8.3 gateways;
//! openapi-evidenced, 575 paths, zero matches).
//!
//! THE crown pins are REQUEST-SEQUENCE proofs at the actions layer
//! (the orchestration's home since the re-point):
//! - list/get fire EXACTLY ONE export GET and ZERO import POSTs
//!   (reads never mutate — an import would replace the project);
//! - put/delete fire export GET then import POST with
//!   `overwrite=true` on the QUERY string, `Content-Type:
//!   application/zip`, and a body the test round-trips through the
//!   SAME surgery helpers to assert the member changed (member-level
//!   honesty — byte-exact zip equality is not required);
//! - put APPENDS absent members (upsert) and preserves neighbors;
//! - a nonexistent project surfaces through export's existing 404
//!   path (`not_found`, exit 6) — for every op in the family.

use ignition_core::actions::resources as actions;
mod common;
use ignition_core::client::ReqwestGatewayApi;
use ignition_core::client::resources::{read_member, resource_members};
use ignition_core::error::CoreError;

/// Build a small export zip: `project.json` + one member per pair, in
/// order (the same zip crate the surgery rides — honest fixtures).
fn fixture_zip(members: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write as _;
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    writer
        .start_file("project.json", options)
        .expect("project.json starts");
    writer
        .write_all(br#"{"title":"T","enabled":true}"#)
        .expect("project.json writes");
    for (name, bytes) in members {
        writer.start_file(*name, options).expect("member starts");
        writer.write_all(bytes).expect("member writes");
    }
    writer.finish().expect("zip finalizes").into_inner()
}

/// The two-member fixture the sequence pins ride: a core script
/// (member form `<collection>/resources/<rest>`) and a Perspective
/// view file.
const SCRIPT_MEMBER: &str = "ignition/resources/script-python/e2e/scratch";
const SCRIPT_USER_PATH: &str = "ignition/script-python/e2e/scratch";
const VIEW_MEMBER: &str = "com.example/resources/views/Dashboard/view.json";

fn sample_export_zip() -> Vec<u8> {
    fixture_zip(&[
        (
            SCRIPT_MEMBER,
            br#"{"scope":"G","code":"print('old')"}"#.as_slice(),
        ),
        (VIEW_MEMBER, br#"{"scope":"A"}"#.as_slice()),
    ])
}

/// Mount the export GET (200 + the zip body) with `expect(n)`.
async fn mount_export(server: &wiremock::MockServer, project: &str, zip: Vec<u8>, n: u64) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(format!(
            "/data/api/v1/projects/export/{project}"
        )))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_raw(zip, "application/zip"))
        .expect(n)
        .mount(server)
        .await;
}

/// Mount the import POST (matching `overwrite=true` + the zip content
/// type) with `expect(n)` — the mutation half of every surgery
/// sequence.
async fn mount_import(server: &wiremock::MockServer, project: &str, n: u64) {
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(format!(
            "/data/api/v1/projects/import/{project}"
        )))
        .and(wiremock::matchers::query_param("overwrite", "true"))
        .and(wiremock::matchers::header(
            "content-type",
            "application/zip",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"success": true})),
        )
        .expect(n)
        .mount(server)
        .await;
}

/// THE read-sequence pin: `resource list` fires exactly ONE export
/// GET and ZERO import POSTs, and the member map lands in the result
/// (user-facing paths, `resources/` stripped, `project.json` gone).
#[tokio::test]
async fn resource_list_is_export_only() {
    let server = wiremock::MockServer::start().await;
    mount_export(&server, "p", sample_export_zip(), 1).await;
    mount_import(&server, "p", 0).await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = actions::resources_list(&api, "p", None)
        .await
        .expect("list orchestrates Ok");
    assert_eq!(
        result
            .resources
            .iter()
            .filter_map(|entry| entry.path.as_deref())
            .collect::<Vec<_>>(),
        vec![
            "ignition/script-python/e2e/scratch",
            "com.example/views/Dashboard/view.json",
        ],
        "member paths map to the user-facing form"
    );
}

/// The prefix filter is CLIENT-SIDE now: it narrows the member list
/// after the (single) export — no query param rides the wire.
#[tokio::test]
async fn resource_list_prefix_filters_client_side() {
    let server = wiremock::MockServer::start().await;
    mount_export(&server, "p", sample_export_zip(), 1).await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = actions::resources_list(&api, "p", Some("com.example"))
        .await
        .expect("prefix-filtered list Ok");
    assert_eq!(
        result.resources[0].path.as_deref(),
        Some("com.example/views/Dashboard/view.json"),
        "only the prefixed member survives"
    );
    assert_eq!(result.resources.len(), 1);
}

/// THE read-sequence pin for get: ONE export, ZERO imports — and the
/// member bytes ride out VERBATIM for the sniffer.
#[tokio::test]
async fn resource_get_is_export_only_and_verbatim() {
    let server = wiremock::MockServer::start().await;
    mount_export(&server, "p", sample_export_zip(), 1).await;
    mount_import(&server, "p", 0).await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = actions::resource_get(&api, "p", SCRIPT_USER_PATH)
        .await
        .expect("get orchestrates Ok");
    assert_eq!(result.content_kind, "json");
    assert_eq!(result.content["code"], "print('old')");
}

/// A missing member is the family's not-found shape (exit 6) — from
/// the surgery helper, over the export transport, still ONE export
/// and ZERO imports.
#[tokio::test]
async fn resource_get_missing_member_is_not_found() {
    let server = wiremock::MockServer::start().await;
    mount_export(&server, "p", sample_export_zip(), 1).await;
    mount_import(&server, "p", 0).await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let err = actions::resource_get(&api, "p", "ignition/script-python/nope")
        .await
        .expect_err("missing member must fail");
    assert!(
        matches!(err, CoreError::NotFound { .. }),
        "wrong class: {err}"
    );
    assert_eq!(err.exit_code(), 6);
    assert_eq!(err.code(), "not_found");
}

/// THE binary fence on the new transport: a member whose BYTES sniff
/// binary refuses `resource_binary` (exit 6) — the export still rode
/// (the sniff happens after the member read), but ZERO imports: a
/// data.bin-class resource must never round-trip the JSON loop.
#[tokio::test]
async fn resource_get_binary_member_refuses() {
    let server = wiremock::MockServer::start().await;
    let binary_zip = fixture_zip(&[("com.x/resources/perms/data.bin", &[0x00, 0x50, 0x4B, 0x03])]);
    mount_export(&server, "p", binary_zip, 1).await;
    mount_import(&server, "p", 0).await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let err = actions::resource_get(&api, "p", "com.x/perms/data.bin")
        .await
        .expect_err("binary member must refuse");
    assert!(
        matches!(err, CoreError::ResourceBinary { .. }),
        "wrong class: {err}"
    );
    assert_eq!(err.exit_code(), 6);
    assert_eq!(err.code(), "resource_binary");
}

/// THE put-sequence pin: export GET THEN import POST
/// (`overwrite=true`, `application/zip`), and the import body —
/// round-tripped through the same surgery helpers — carries the NEW
/// member content with the neighbor preserved.
#[tokio::test]
async fn resource_put_runs_export_then_overwrite_import() {
    let server = wiremock::MockServer::start().await;
    mount_export(&server, "p", sample_export_zip(), 1).await;
    let import_guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/data/api/v1/projects/import/p"))
        .and(wiremock::matchers::query_param("overwrite", "true"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"success": true})),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let new_body = br#"{"scope":"G","code":"print('new')"}"#.to_vec();
    let result = actions::resource_put(&api, "p", SCRIPT_USER_PATH, new_body.clone())
        .await
        .expect("put orchestrates Ok");
    assert_eq!(result.content_kind, "json");

    let requests = import_guard.received_requests().await;
    assert_eq!(requests.len(), 1, "exactly ONE import POST");
    assert_eq!(
        requests[0]
            .headers
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("application/zip"),
        "the import body declares the zip content type"
    );
    // Member-level honesty on the surgical body (byte-exact zip
    // equality is NOT the contract — the members are).
    assert_eq!(
        read_member(&requests[0].body, SCRIPT_USER_PATH).expect("surgical body re-reads"),
        new_body,
        "the import body carries the NEW member content"
    );
    assert_eq!(
        read_member(&requests[0].body, "com.example/views/Dashboard/view.json")
            .expect("neighbor survives"),
        br#"{"scope":"A"}"#.to_vec(),
        "the untouched neighbor rides the surgical zip"
    );
    assert_eq!(resource_members(&requests[0].body).unwrap().len(), 2);
}

/// THE upsert pin: putting an ABSENT member appends it — the import
/// body carries the new member plus every original.
#[tokio::test]
async fn resource_put_appends_absent_member() {
    let server = wiremock::MockServer::start().await;
    mount_export(&server, "p", sample_export_zip(), 1).await;
    let import_guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/data/api/v1/projects/import/p"))
        .and(wiremock::matchers::query_param("overwrite", "true"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"success": true})),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    actions::resource_put(
        &api,
        "p",
        "ignition/script-python/e2e/brand-new",
        b"print('x')".to_vec(),
    )
    .await
    .expect("append put Ok");

    let requests = import_guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    let members = resource_members(&requests[0].body).expect("surgical body lists");
    assert!(members.contains(&"ignition/script-python/e2e/brand-new".to_string()));
    assert!(members.contains(&SCRIPT_USER_PATH.to_string()));
    assert_eq!(
        read_member(&requests[0].body, "ignition/script-python/e2e/brand-new").unwrap(),
        b"print('x')".to_vec()
    );
}

/// A BINARY put input refuses BEFORE any network I/O: no export, no
/// import — binary content never enters the surgery (Pitfall 7).
#[tokio::test]
async fn resource_put_binary_input_refuses_before_network() {
    let server = wiremock::MockServer::start().await;
    mount_export(&server, "p", sample_export_zip(), 0).await;
    mount_import(&server, "p", 0).await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let err = actions::resource_put(&api, "p", "com.x/perms/data.bin", vec![0x00, 0x50, 0x4B])
        .await
        .expect_err("binary input must refuse");
    assert!(matches!(err, CoreError::ResourceBinary { .. }), "{err}");
    assert_eq!(err.exit_code(), 6);
}

/// THE delete-sequence pin: export THEN overwrite-import, and the
/// import body NO LONGER carries the member (surgically removed) —
/// while the neighbor survives.
#[tokio::test]
async fn resource_delete_runs_export_then_overwrite_import() {
    let server = wiremock::MockServer::start().await;
    mount_export(&server, "p", sample_export_zip(), 1).await;
    let import_guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/data/api/v1/projects/import/p"))
        .and(wiremock::matchers::query_param("overwrite", "true"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"success": true})),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = actions::resource_delete(&api, "p", "com.example/views/Dashboard/view.json")
        .await
        .expect("delete orchestrates Ok");
    assert_eq!(result.deleted, "com.example/views/Dashboard/view.json");

    let requests = import_guard.received_requests().await;
    assert_eq!(requests.len(), 1, "exactly ONE import POST");
    let gone = read_member(&requests[0].body, "com.example/views/Dashboard/view.json");
    assert!(
        matches!(gone, Err(CoreError::NotFound { .. })),
        "the member is GONE from the surgical body"
    );
    assert_eq!(
        resource_members(&requests[0].body).expect("surgical body lists"),
        vec![SCRIPT_USER_PATH.to_string()],
        "exactly the neighbor survives"
    );
}

/// Deleting a MISSING member is not-found — and ZERO imports fire
/// (nothing to remove; the project is never touched).
#[tokio::test]
async fn resource_delete_missing_member_is_not_found_no_import() {
    let server = wiremock::MockServer::start().await;
    mount_export(&server, "p", sample_export_zip(), 1).await;
    mount_import(&server, "p", 0).await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let err = actions::resource_delete(&api, "p", "com.example/views/Never")
        .await
        .expect_err("missing member must fail");
    assert!(matches!(err, CoreError::NotFound { .. }), "{err}");
    assert_eq!(err.exit_code(), 6);
}

/// THE project-error path pin: a nonexistent project surfaces through
/// export's existing classification (404 JSON → `not_found`, exit 6)
/// for EVERY op in the family.
#[tokio::test]
async fn nonexistent_project_surfaces_export_not_found() {
    let server = wiremock::MockServer::start().await;
    for method in ["GET", "POST"] {
        wiremock::Mock::given(wiremock::matchers::method(method))
            .and(wiremock::matchers::path(
                "/data/api/v1/projects/export/ghost",
            ))
            .respond_with(
                wiremock::ResponseTemplate::new(404)
                    .set_body_json(serde_json::json!({"message": "No project ghost"})),
            )
            .expect(if method == "GET" { 4 } else { 0 })
            .mount(&server)
            .await;
    }

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    for err in [
        actions::resources_list(&api, "ghost", None)
            .await
            .expect_err("list on ghost project must fail"),
        actions::resource_get(&api, "ghost", "x/y")
            .await
            .expect_err("get on ghost project must fail"),
        actions::resource_put(&api, "ghost", "x/y", b"{}".to_vec())
            .await
            .expect_err("put on ghost project must fail"),
        actions::resource_delete(&api, "ghost", "x/y")
            .await
            .expect_err("delete on ghost project must fail"),
    ] {
        assert!(
            matches!(err, CoreError::NotFound { .. }),
            "wrong class: {err}"
        );
        assert_eq!(err.exit_code(), 6);
    }
}

/// The family classifies like every other: 401 Jetty HTML on the
/// export (header-less under default security) → `Auth` (exit 5) —
/// classify runs before any body consumption, zip or otherwise.
#[tokio::test]
async fn export_html_401_classifies_auth() {
    let mock = common::IgnitionMock::start().await;
    mock.html_error("GET", "/data/api/v1/projects/export/p", 401)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let err = actions::resources_list(&api, "p", None)
        .await
        .expect_err("401 must fail");
    assert!(
        matches!(&err, CoreError::Auth { status: 401, .. }),
        "wrong class: {err}"
    );
    assert_eq!(err.exit_code(), 5);
}
