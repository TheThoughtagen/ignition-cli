//! Wiremock contract tests for the backup capability (04-04, RIG-04):
//! the gwbk wire pinned on the REQUESTS (the Phase-2/3 recorded-request
//! discipline) — the download streams the octet-stream body to disk
//! byte-identical (read-back compare; binary content NEVER goldens into
//! snapbox), the restore POSTs a RAW `application/octet-stream` body
//! (NOT multipart) with all four scope params EXPLICIT on the query
//! string, and the live-verified 401-HTML unauth shape classifies
//! through the standard pipeline (83-api: backup requires a token like
//! every `/data` route).

mod common;

use common::IgnitionMock;
use ignition_core::client::{GatewayApi, ReqwestGatewayApi};
use ignition_core::config::{Credential, Secret};

const BACKUP_PATH: &str = "/data/api/v1/backup";

fn token_credential() -> Credential {
    Credential::Token(Secret::new("backup:tokengeneratedlive"))
}

/// A deterministic binary-ish gwbk fixture: ZIP magic + non-UTF-8 high
/// bytes + NULs + a trailing text run — byte-patterned, never a
/// snapbox golden (binary content normalizes badly; the READ-BACK
/// compare is the exactness proof).
fn gwbk_fixture() -> Vec<u8> {
    let mut bytes: Vec<u8> = vec![0x50, 0x4B, 0x03, 0x04];
    for i in 0..256u16 {
        bytes.push((i % 256) as u8);
    }
    bytes.extend_from_slice(b"\x00\x00\x00gwbk-fixture-tail");
    bytes
}

/// THE download pin: `GET /data/api/v1/backup?type=roaming` streams
/// the body to disk BYTE-IDENTICAL (read-back compare, the exactness
/// proof), and the REQUEST carries the `Accept: application/octet-
/// stream` header + the token (the postman collection's exact shape).
#[tokio::test]
async fn backup_download_streams_bytes_identical() {
    let mock = IgnitionMock::start().await;
    let fixture = gwbk_fixture();
    let guard = wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(BACKUP_PATH))
        .and(wiremock::matchers::query_param("type", "roaming"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_raw(fixture.clone(), "application/octet-stream"),
        )
        .expect(1)
        .mount_as_scoped(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("rig.gwbk");
    let meta = api
        .backup_download(&out, ignition_core::client::backup::BackupType::Roaming)
        .await
        .expect("download streams");
    assert_eq!(meta.bytes, fixture.len() as u64, "chunk-counted total");
    assert_eq!(
        meta.content_type.as_deref(),
        Some("application/octet-stream"),
        "content type sniffed from the response"
    );

    let on_disk = std::fs::read(&out).expect("read back");
    assert_eq!(on_disk, fixture, "bytes land on disk IDENTICAL (read-back)");

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    let headers = format!("{:?}", requests[0].headers).to_lowercase();
    assert!(
        headers.contains("accept") && headers.contains("application/octet-stream"),
        "the download sends its Accept contract: {headers}"
    );
    assert!(
        headers.contains("x-ignition-api-token"),
        "backup is an authed /data route: {headers}"
    );
}

/// THE restore pin: the REQUEST is the contract — RAW octet-stream
/// body (body bytes = FILE bytes, exactly), `Content-Type:
/// application/octet-stream` (NOT multipart — no boundary, no
/// form-data), and all FOUR scope params explicit false on the query
/// string. The 2xx is only ACCEPTANCE (the actions layer owns the
/// post-restore RUNNING wait — Pitfall 6).
#[tokio::test]
async fn backup_restore_posts_raw_octet_stream_with_explicit_params() {
    let mock = IgnitionMock::start().await;
    let fixture = gwbk_fixture();
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(BACKUP_PATH))
        .and(wiremock::matchers::query_param("restoreDisabled", "false"))
        .and(wiremock::matchers::query_param(
            "disableTempProjectBackup",
            "false",
        ))
        .and(wiremock::matchers::query_param("renameEnabled", "false"))
        .and(wiremock::matchers::query_param("restoreLocal", "false"))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1)
        .mount_as_scoped(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let dir = tempfile::tempdir().expect("tempdir");
    let gwbk = dir.path().join("restore.gwbk");
    std::fs::write(&gwbk, &fixture).expect("write fixture gwbk");
    api.backup_restore(&gwbk).await.expect("restore accepted");

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].body, fixture,
        "body bytes = FILE bytes — the raw gwbk, exactly"
    );
    let headers = format!("{:?}", requests[0].headers).to_lowercase();
    assert!(
        headers.contains("content-type") && headers.contains("application/octet-stream"),
        "the restore declares its raw body type: {headers}"
    );
    assert!(
        !headers.contains("multipart") && !headers.contains("boundary"),
        "NOT multipart — the postman collection's raw-body shape: {headers}"
    );
    assert!(
        headers.contains("x-ignition-api-token"),
        "backup is an authed /data route: {headers}"
    );
}

/// The live-verified unauth shape: 401 + Jetty HTML (backup requires a
/// token like every `/data` route) classifies through the standard
/// pipeline as `auth_rejected` (exit 5) — never a parse crash on the
/// HTML body.
#[tokio::test]
async fn backup_unauth_401_html_classifies_auth() {
    let mock = IgnitionMock::start().await;
    mock.html_error("GET", BACKUP_PATH, 401).await;
    let api = ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let dir = tempfile::tempdir().expect("tempdir");
    let err = api
        .backup_download(
            &dir.path().join("x.gwbk"),
            ignition_core::client::backup::BackupType::default(),
        )
        .await
        .expect_err("401 HTML classifies Auth");
    assert_eq!(err.exit_code(), 5);
    assert_eq!(err.code(), "auth_rejected");
}

/// 07-02's type param: `BackupType::All` sends `?type=all` (the
/// gateway-specific full backup) — the `?type=roaming` default is
/// pinned above; this is the other wire value.
#[tokio::test]
async fn backup_download_all_type_sends_all_query() {
    let mock = IgnitionMock::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(BACKUP_PATH))
        .and(wiremock::matchers::query_param("type", "all"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_raw(b"PK-all-type".to_vec(), "application/octet-stream"),
        )
        .expect(1)
        .mount_as_scoped(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("all.gwbk");
    api.backup_download(&out, ignition_core::client::backup::BackupType::All)
        .await
        .expect("all-type download streams");
    assert_eq!(std::fs::read(&out).unwrap(), b"PK-all-type");
    assert_eq!(guard.received_requests().await.len(), 1);
}

/// The standalone ACTION layer's default naming (07-02, BKUP-01): no
/// `-o` → stream to `<stem>-backup.gwbk.part`, rename to the
/// Content-Disposition basename when the gateway sends one (the
/// project-export convention). The `.part` never survives.
#[tokio::test]
async fn backup_download_action_renames_to_disposition_basename() {
    let mock = IgnitionMock::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(BACKUP_PATH))
        .and(wiremock::matchers::query_param("type", "roaming"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .insert_header(
                    "Content-Disposition",
                    "attachment; filename=\"gw-2026-08-28.gwbk\"",
                )
                .set_body_raw(b"PK-dispo".to_vec(), "application/octet-stream"),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let cwd = tempfile::tempdir().expect("cwd tempdir");
    let previous = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(cwd.path()).expect("enter cwd");
    let result = ignition_core::actions::backup::backup_download(
        &api,
        None,
        "dev",
        ignition_core::client::backup::BackupType::default(),
    )
    .await;
    std::env::set_current_dir(previous).expect("restore cwd");
    let result = result.expect("default-naming download");
    assert_eq!(
        result.file, "gw-2026-08-28.gwbk",
        "disposition basename wins"
    );
    assert_eq!(result.r#type, "roaming");
    assert!(
        !cwd.path().join("dev-backup.gwbk.part").exists(),
        "the .part never survives"
    );
    assert_eq!(
        std::fs::read(cwd.path().join("gw-2026-08-28.gwbk")).unwrap(),
        b"PK-dispo"
    );
}

/// The standalone ACTION layer's restore pre-checks (the `rig
/// restore` shape): a nonexistent gwbk refuses `invalid_input` (exit
/// 2) BEFORE any network work — the mock receives zero requests.
#[tokio::test]
async fn backup_restore_action_prechecks_before_network() {
    let mock = IgnitionMock::start().await;
    assert!(
        mock.server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty(),
        "fixture starts empty"
    );

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("missing.gwbk");
    let err = ignition_core::actions::backup::backup_restore(&api, &missing)
        .await
        .expect_err("nonexistent gwbk refuses");
    assert_eq!(err.exit_code(), 2);
    assert_eq!(err.code(), "invalid_input");

    let empty = dir.path().join("empty.gwbk");
    std::fs::write(&empty, b"").expect("write empty");
    let err = ignition_core::actions::backup::backup_restore(&api, &empty)
        .await
        .expect_err("empty gwbk refuses");
    assert_eq!(err.exit_code(), 2);
    assert_eq!(err.code(), "invalid_input");

    // ZERO requests hit the wire for either refusal.
    assert!(
        mock.server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty(),
        "usage-class refusals do no network work"
    );
}

/// The standalone ACTION layer's restore success: the raw POST
/// through the existing trait method, flat `{restored: true}` shape.
#[tokio::test]
async fn backup_restore_action_posts_and_reports_restored() {
    let mock = IgnitionMock::start().await;
    let fixture = gwbk_fixture();
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(BACKUP_PATH))
        .and(wiremock::matchers::query_param("restoreDisabled", "false"))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let dir = tempfile::tempdir().expect("tempdir");
    let gwbk = dir.path().join("ok.gwbk");
    std::fs::write(&gwbk, &fixture).expect("write fixture gwbk");
    let result = ignition_core::actions::backup::backup_restore(&api, &gwbk)
        .await
        .expect("restore accepted");
    assert!(result.restored);
    let requests = mock.server.received_requests().await.unwrap_or_default();
    assert_eq!(requests.len(), 1, "exactly the restore POST");
    assert_eq!(requests[0].body, fixture, "raw gwbk bytes, exactly");
}
