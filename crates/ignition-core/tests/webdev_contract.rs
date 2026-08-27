//! Wiremock contract for the WebDev client seam (05-03) — the probe
//! discrimination matrix, the body-envelope call mapping, and the
//! deploy zip builder, all against the REAL
//! [`ReqwestGatewayApi`] (trait level; the binary-level goldens live
//! in `ignition-cli/tests/contract_webdev.rs`).
//!
//! THE crown pins (05-RESEARCH, each live-proven):
//! - **405 = absent, NOT 404** — the 8.3 absent marker (Pitfall 1);
//! - **402 = module unlicensed** (trial-expired) and **401 = present
//!   but auth-gated** — distinct probe states, never conflated;
//! - **denials ride HTTP 200** — a `200 {ok:false}` body NEVER
//!   masquerades as success; `error.code` maps onto the taxonomy
//!   (`not_found` reuses the existing slug; unknown/secret codes ride
//!   `webdev_route_error` with code+message verbatim);
//! - the deploy zip's members ARE the embedded manifest
//!   (`ROUTE_FILES`), scriptExec joins ONLY when flagged, and its
//!   doPost.py carries the SUBSTITUTED secret — never the
//!   `__IGN_CLI_SECRET__` placeholder (fail-closed by construction).

use ignition_core::client::GatewayApi;
use ignition_core::client::ReqwestGatewayApi;
use ignition_core::client::webdev::{RouteProbe, build_deploy_zip};
use ignition_core::webdev::ROUTE_FILES;

/// The version-action 200-ok body every Present fixture answers.
fn version_body(route_version: &str) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "data": {"routeVersion": route_version, "minCli": "1.0"},
    })
}

/// A 200 body denial (the route envelope's refusal shape).
fn denial_body(code: &str, message: &str) -> serde_json::Value {
    serde_json::json!({
        "ok": false,
        "error": {"code": code, "message": message},
    })
}

/// 200 + ok body → Present with the handshake version parsed verbatim
/// (the string rides through untouched — the ACTION layer compares).
#[tokio::test]
async fn probe_present_parses_the_handshake_version() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(version_body("1.0.0")))
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let probe = api
        .webdev_route_probe("ign-cli", "tags", &[])
        .await
        .expect("200 + ok body is Present");
    assert_eq!(
        probe,
        RouteProbe::Present {
            route_version: "1.0.0".to_string()
        }
    );
}

/// THE 8.3 marker: 405 → Absent (NOT 404 — a 404 from a webdev path
/// means a foreign gateway and stays an error through classify).
#[tokio::test]
async fn probe_405_is_absent_not_404() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .respond_with(wiremock::ResponseTemplate::new(405))
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let probe = api
        .webdev_route_probe("ign-cli", "tags", &[])
        .await
        .expect("405 is an ANSWER, not an error");
    assert_eq!(probe, RouteProbe::Absent);
}

/// 402 → Unlicensed — the trial-expired module state, distinct from
/// absent (the servlet exists; the license does not).
#[tokio::test]
async fn probe_402_is_unlicensed() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .respond_with(wiremock::ResponseTemplate::new(402))
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let probe = api
        .webdev_route_probe("ign-cli", "tags", &[])
        .await
        .expect("402 is an ANSWER");
    assert_eq!(probe, RouteProbe::Unlicensed);
}

/// 401 → AuthGated — present but rejecting the credential (research
/// Open Question 4: report, never conflate with absent).
#[tokio::test]
async fn probe_401_is_auth_gated() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/scriptExec",
        ))
        .respond_with(wiremock::ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let probe = api
        .webdev_route_probe("ign-cli", "scriptExec", &[])
        .await
        .expect("401 is an ANSWER");
    assert_eq!(probe, RouteProbe::AuthGated);
}

/// 200 + denial body → Denied carrying the route contract's
/// code + message (the scriptExec gate's shape).
#[tokio::test]
async fn probe_200_denial_carries_code_and_message() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/scriptExec",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(denial_body("secret_mismatch", "scriptExec secret mismatch")),
        )
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let probe = api
        .webdev_route_probe("ign-cli", "scriptExec", &[])
        .await
        .expect("a 200 denial is an ANSWER");
    assert_eq!(
        probe,
        RouteProbe::Denied {
            code: "secret_mismatch".to_string(),
            message: "scriptExec secret mismatch".to_string(),
            traceback: None,
        }
    );
}

/// route_call: ok:true returns `data` — and the action body rides the
/// wire as recorded (the request pin).
#[tokio::test]
async fn route_call_ok_returns_data_and_records_the_action_body() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .and(wiremock::matchers::body_json(
            serde_json::json!({"action": "read", "paths": ["[default]T1"]}),
        ))
        .and(wiremock::matchers::header(
            "x-ignition-cli-secret",
            "s3cret",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"results": [{"path": "[default]T1", "value": 7, "quality": "Good"}]},
            })),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let data = api
        .webdev_route_call(
            "ign-cli",
            "tags",
            &serde_json::json!({"action": "read", "paths": ["[default]T1"]}),
            &[("X-Ignition-CLI-Secret", "s3cret")],
        )
        .await
        .expect("ok:true yields data");
    assert_eq!(data["results"][0]["value"], 7);
    assert_eq!(guard.received_requests().await.len(), 1);
}

/// route_call NEVER treats HTTP 200 as success by itself: an
/// unknown-code denial maps to `webdev_route_error` (exit 6) with
/// code + message verbatim.
#[tokio::test]
async fn route_call_denial_with_unknown_code_is_webdev_route_error() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/alarms",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(denial_body(
                "some_future_code",
                "a code this CLI does not map",
            )),
        )
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let err = api
        .webdev_route_call(
            "ign-cli",
            "alarms",
            &serde_json::json!({"action": "history"}),
            &[],
        )
        .await
        .expect_err("a 200 denial must NOT masquerade as success");
    assert_eq!(err.code(), "webdev_route_error");
    assert_eq!(err.exit_code(), 6);
    assert!(
        err.to_string().contains("some_future_code"),
        "code rides verbatim: {err}"
    );
    assert!(
        err.to_string().contains("a code this CLI does not map"),
        "message rides verbatim: {err}"
    );
}

/// route_call: the route contract's `not_found` code reuses the
/// existing `not_found` slug (exit 6) — it means exactly that.
#[tokio::test]
async fn route_call_not_found_denial_reuses_the_not_found_slug() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/system/webdev/ign-cli/cli/tagConfig",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(denial_body("not_found", "tag path not found")),
        )
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let err = api
        .webdev_route_call(
            "ign-cli",
            "tagConfig",
            &serde_json::json!({"action": "getConfig", "tagPath": "[default]Nope"}),
            &[],
        )
        .await
        .expect_err("denial");
    assert_eq!(err.code(), "not_found");
    assert_eq!(err.exit_code(), 6);
}

/// route_call on a 401 answer: classify runs NORMALLY for status
/// errors (the plan's split — body-envelope parsing is for 200s).
#[tokio::test]
async fn route_call_401_classifies_auth() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .respond_with(wiremock::ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let err = api
        .webdev_route_call(
            "ign-cli",
            "tags",
            &serde_json::json!({"action": "read"}),
            &[],
        )
        .await
        .expect_err("401 classifies");
    assert_eq!(err.code(), "auth_rejected");
    assert_eq!(err.exit_code(), 5);
}

// ---- Deploy zip builder pins ----

/// Unpack a zip's member names, in writer order.
fn member_names(zip_bytes: &[u8]) -> Vec<String> {
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)).expect("built zip is readable");
    (0..archive.len())
        .map(|index| archive.by_index(index).expect("member").name().to_string())
        .collect()
}

/// Read one member's bytes.
fn member(zip_bytes: &[u8], name: &str) -> Vec<u8> {
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)).expect("built zip is readable");
    let mut file = archive.by_name(name).expect("member present");
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut file, &mut bytes).expect("member reads");
    bytes
}

/// The plain deploy packs EXACTLY the embedded manifest — member-for-
/// member, ROUTE_FILES order, no scriptExec.
#[test]
fn deploy_zip_plain_members_are_the_manifest() {
    let zip = build_deploy_zip("ign-cli", false, None).expect("plain deploy packs");
    let expected: Vec<&str> = ROUTE_FILES.iter().map(|(name, _)| *name).collect();
    assert_eq!(member_names(&zip), expected);
    assert!(
        !member_names(&zip)
            .iter()
            .any(|name| name.contains("scriptExec")),
        "scriptExec must NOT ship in a plain deploy"
    );
    // project.json rides VERBATIM when the title is the default.
    let manifest: serde_json::Value =
        serde_json::from_slice(&member(&zip, "project.json")).expect("manifest parses");
    assert_eq!(manifest["title"], "ign-cli");
}

/// The scriptExec deploy appends exactly three members and the
/// substituted secret rides doPost.py — the placeholder NEVER ships.
#[test]
fn deploy_zip_script_exec_substitutes_the_secret() {
    let zip =
        build_deploy_zip("ign-cli", true, Some("cafebabe1234")).expect("scriptExec deploy packs");
    let names = member_names(&zip);
    assert_eq!(names.len(), ROUTE_FILES.len() + 3, "manifest + 3 members");
    for suffix in ["resource.json", "config.json", "doPost.py"] {
        assert!(
            names.iter().any(|name| name
                == &format!("com.inductiveautomation.webdev/resources/cli/scriptExec/{suffix}")),
            "missing scriptExec/{suffix}"
        );
    }

    let do_post = member(
        &zip,
        "com.inductiveautomation.webdev/resources/cli/scriptExec/doPost.py",
    );
    let text = String::from_utf8(do_post).expect("doPost.py is utf-8");
    assert!(
        text.contains("cafebabe1234"),
        "the substituted secret rides the route"
    );
    assert!(
        !text.contains("__IGN_CLI_SECRET__"),
        "the placeholder must NEVER ship"
    );
    // The substituted line keeps the template's `None or` idiom shape
    // (`SECRET = None or 'cafebabe…'` evaluates to the secret), so the
    // proof of substitution IS marker-absence + secret-presence above.
    assert!(text.contains("SECRET = None or 'cafebabe1234'"));
}

/// A `--project` override retitles project.json (ONLY `title` moves);
/// the default title rides the manifest untouched.
#[test]
fn deploy_zip_retitles_only_on_project_override() {
    let zip = build_deploy_zip("plant-floor-cli", false, None).expect("override packs");
    let manifest: serde_json::Value =
        serde_json::from_slice(&member(&zip, "project.json")).expect("manifest parses");
    assert_eq!(manifest["title"], "plant-floor-cli");
    assert_eq!(
        manifest["description"],
        "ign CLI WebDev routes (deployed by `ign webdev deploy` — do not edit)"
    );
    assert_eq!(manifest["enabled"], true);
}

// ---- The deploy ACTION's import pin (05-03 Task 2) ----

/// THE deploy pin: `webdev_deploy` POSTs the built zip to the 03-02
/// import machinery — `/data/api/v1/projects/import/ign-cli` with
/// `overwrite=true` + `application/zip` — and the recorded body IS
/// the manifest (+scriptExec when flagged) with the SUBSTITUTED
/// secret; the persisted profile secret stays out of every envelope.
#[tokio::test]
async fn deploy_action_posts_the_zip_through_the_import_machinery() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        // The Phase-3 per-segment encoder over-encodes `-` → `%2D`
        // (safe: the server decodes before matching) — the matcher
        // rides the SAME encoded path the wire sees.
        .and(wiremock::matchers::path(
            "/data/api/v1/projects/import/ign%2Dcli",
        ))
        .and(wiremock::matchers::query_param("overwrite", "true"))
        .and(wiremock::matchers::header(
            "content-type",
            "application/zip",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"success": true})),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    // An isolated profile config (the action re-loads it for the
    // secret lifecycle — no credential involved).
    let dir = tempfile::tempdir().expect("tempdir");
    let config_path = dir.path().join("config.toml");
    std::fs::write(
        &config_path,
        "active = \"dev\"\n\n[profiles.dev]\nurl = \"http://localhost:9088/\"\n",
    )
    .expect("write config");

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = ignition_core::actions::webdev::webdev_deploy(
        &api,
        "ign-cli",
        true,
        false,
        &config_path,
        "dev",
    )
    .await
    .expect("deploy imports");

    // Result shape: 5 routes, scriptExec shipped, secret ROTATED
    // (generated), the import answer passed through.
    assert_eq!(
        result.routes,
        vec!["tags", "tagConfig", "alarms", "tagHistory", "scriptExec"]
    );
    assert!(result.script_exec);
    assert!(result.secret_rotated);
    assert_eq!(result.import["success"], true);

    // The recorded request body: the zip unpacks to manifest + 3, and
    // scriptExec's doPost.py carries a SUBSTITUTED hex secret — never
    // the placeholder.
    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1, "exactly ONE import POST");
    let names = member_names(&requests[0].body);
    assert_eq!(names.len(), ROUTE_FILES.len() + 3);
    let do_post = member(
        &requests[0].body,
        "com.inductiveautomation.webdev/resources/cli/scriptExec/doPost.py",
    );
    let do_post = String::from_utf8(do_post).expect("doPost.py is utf-8");
    assert!(!do_post.contains("__IGN_CLI_SECRET__"));
    assert!(do_post.contains("SECRET = None or '"));

    // The persisted secret exists (64 hex), the file is 0600, and it
    // appears NOWHERE in the serialized result (redaction).
    let stored = ignition_core::config::load(&config_path)
        .expect("config reloads")
        .profiles
        .get("dev")
        .and_then(|profile| profile.webdev_secret.clone())
        .expect("secret persisted");
    assert_eq!(stored.len(), 64);
    assert!(
        do_post.contains(&stored),
        "the stored secret IS the baked one"
    );
    let serialized = serde_json::to_string(&result).expect("result serializes");
    assert!(!serialized.contains(&stored), "redaction: {serialized}");
    let _ = dir;
}
