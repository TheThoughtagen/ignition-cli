//! Wiremock contract of the tagDelete route client (26-18, D-13, TAGDEP-06).
//!
//! The client sends HTTP Basic of the caller's env pair and the
//! `X-Ignition-CLI-Secret` header, never the API token. It reads the
//! version handshake and the delete answer strictly: an unknown or missing
//! field, or a delete answer that does not hold exactly one result per
//! requested path in order, is `webdev_route_error` with code
//! `route_answer_invalid`. Statuses discriminate as the WebDev probe does.

use std::time::Duration;

use base64::Engine as _;

use ignition_core::config::secret::Secret;
use ignition_core::deploy::tag_delete::{
    PathResult, RouteHandshake, TagDeleteClient, TagDeleteRoute, wait_ready,
};
use ignition_core::error::CoreError;
use ignition_core::webdev::ROUTE_BUNDLE_VERSION;
use wiremock::matchers::{body_json, header, header_exists, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const ROUTE_PATH: &str = "/system/webdev/ign-cli/cli/tagDelete";
const FAKE_USER: &str = "td-user";
const FAKE_PASSWORD: &str = "Td-Pass-0001!";
const FAKE_SECRET: &str = "abababababababababababababababababababababababababababababababab";

fn client(server: &MockServer) -> TagDeleteClient {
    let url = url::Url::parse(&server.uri()).expect("mock url");
    TagDeleteClient::new(
        &url,
        true,
        "ign-cli",
        Secret::new(FAKE_USER),
        Secret::new(FAKE_PASSWORD),
        Secret::new(FAKE_SECRET),
    )
    .expect("client builds")
}

fn version_data() -> serde_json::Value {
    serde_json::json!({
        "routeVersion": ROUTE_BUNDLE_VERSION,
        "minCli": "1.0",
        "providers": ["PlantA", "PlantB"],
        "maxPaths": 500,
        "maxPathChars": 1024,
    })
}

fn ok_body(data: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"ok": true, "data": data})
}

fn denial(code: &str) -> serde_json::Value {
    serde_json::json!({"ok": false, "error": {"code": code, "message": format!("route said {code}")}})
}

fn result_row(path: &str) -> serde_json::Value {
    serde_json::json!({"path": path, "quality": "Good", "good": true, "existsAfter": false})
}

fn paths(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

fn route_code(err: &CoreError) -> Option<&str> {
    match err {
        CoreError::WebdevRouteError { code, .. } => Some(code.as_str()),
        _ => None,
    }
}

/// The request carries Basic of the env pair, the secret header, no API
/// token, the JSON body shape and the route path.
#[tokio::test]
async fn basic_auth_and_secret_header() {
    let server = MockServer::start().await;
    let expected_basic = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{FAKE_USER}:{FAKE_PASSWORD}"))
    );
    Mock::given(method("POST"))
        .and(path(ROUTE_PATH))
        .and(header("authorization", expected_basic.as_str()))
        .and(header("x-ignition-cli-secret", FAKE_SECRET))
        .and(body_json(serde_json::json!({"action": "version"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_body(version_data())))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(ROUTE_PATH))
        .and(header("authorization", expected_basic.as_str()))
        .and(header("x-ignition-cli-secret", FAKE_SECRET))
        .and(body_json(serde_json::json!({
            "action": "delete",
            "provider": "PlantA",
            "paths": ["Line1/A"],
        })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(ok_body(serde_json::json!({
                "provider": "PlantA",
                "results": [result_row("Line1/A")],
                "elapsedMs": 12,
            }))),
        )
        .expect(1)
        .mount(&server)
        .await;

    let route = client(&server);
    route.handshake().await.expect("handshake");
    route
        .delete("PlantA", &paths(&["Line1/A"]))
        .await
        .expect("delete");

    let requests: Vec<Request> = server.received_requests().await.expect("recording on");
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert!(
            !request.headers.contains_key("x-ignition-api-token"),
            "the tagDelete client must never send the API token"
        );
        assert!(request.headers.contains_key("authorization"));
    }
}

/// A token-bearing request would be refused by this mock; the client
/// never matches it (the second half of the API-token pin).
#[tokio::test]
async fn no_api_token_header_ever() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(ROUTE_PATH))
        .and(header_exists("x-ignition-api-token"))
        .respond_with(ResponseTemplate::new(418))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(ROUTE_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_body(version_data())))
        .mount(&server)
        .await;
    client(&server).handshake().await.expect("handshake");
}

#[tokio::test]
async fn handshake_reads_caps_and_providers() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(ROUTE_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_body(version_data())))
        .mount(&server)
        .await;
    let handshake = client(&server).handshake().await.expect("handshake");
    assert_eq!(
        handshake,
        RouteHandshake {
            route_version: ROUTE_BUNDLE_VERSION.to_string(),
            min_cli: "1.0".to_string(),
            providers: paths(&["PlantA", "PlantB"]),
            max_paths: 500,
            max_path_chars: 1024,
        }
    );
}

/// An unknown or missing handshake field, or a different route version,
/// refuses.
#[tokio::test]
async fn handshake_is_strict() {
    let mut extra = version_data();
    extra["more"] = serde_json::json!(1);
    let mut missing = version_data();
    missing.as_object_mut().expect("object").remove("maxPaths");
    for data in [extra, missing] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(ROUTE_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_body(data)))
            .mount(&server)
            .await;
        let err = client(&server).handshake().await.expect_err("strict");
        assert_eq!(route_code(&err), Some("route_answer_invalid"), "{err}");
    }

    let mut old = version_data();
    old["routeVersion"] = serde_json::json!("0.9.0");
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(ROUTE_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_body(old)))
        .mount(&server)
        .await;
    let err = client(&server).handshake().await.expect_err("mismatch");
    assert_eq!(err.code(), "route_version_mismatch", "{err}");
}

#[tokio::test]
async fn delete_answer_parsed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(ROUTE_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_body(serde_json::json!({
            "provider": "PlantA",
            "results": [
                result_row("Line1/A"),
                {"path": "Line1/Gone", "quality": "Bad_NotFound", "good": false, "existsAfter": false},
                {"path": "Line1/C", "quality": null, "good": null, "existsAfter": false},
            ],
            "elapsedMs": 359,
        }))))
        .mount(&server)
        .await;
    let answer = client(&server)
        .delete("PlantA", &paths(&["Line1/A", "Line1/Gone", "Line1/C"]))
        .await
        .expect("delete");
    assert_eq!(answer.provider, "PlantA");
    assert_eq!(answer.elapsed_ms, 359);
    assert_eq!(
        answer.results,
        vec![
            PathResult {
                path: "Line1/A".into(),
                quality: Some("Good".into()),
                good: Some(true),
                exists_after: false,
            },
            PathResult {
                path: "Line1/Gone".into(),
                quality: Some("Bad_NotFound".into()),
                good: Some(false),
                exists_after: false,
            },
            PathResult {
                path: "Line1/C".into(),
                quality: None,
                good: None,
                exists_after: false,
            },
        ]
    );
}

/// One result per requested path, in request order, under the requested
/// provider; a missing result field is refused too.
#[tokio::test]
async fn delete_answer_with_a_missing_path_is_refused() {
    let bodies = [
        // one path short
        serde_json::json!({"provider": "PlantA", "results": [result_row("A")], "elapsedMs": 1}),
        // wrong order
        serde_json::json!({"provider": "PlantA", "results": [result_row("B"), result_row("A")], "elapsedMs": 1}),
        // another provider
        serde_json::json!({"provider": "PlantB", "results": [result_row("A"), result_row("B")], "elapsedMs": 1}),
        // one result too many
        serde_json::json!({"provider": "PlantA", "results": [result_row("A"), result_row("B"), result_row("C")], "elapsedMs": 1}),
        // a result without existsAfter
        serde_json::json!({"provider": "PlantA", "results": [result_row("A"), {"path": "B", "quality": "Good", "good": true}], "elapsedMs": 1}),
        // a result without good (null is allowed, absent is not)
        serde_json::json!({"provider": "PlantA", "results": [result_row("A"), {"path": "B", "quality": "Good", "existsAfter": false}], "elapsedMs": 1}),
        // an unknown key
        serde_json::json!({"provider": "PlantA", "results": [result_row("A"), result_row("B")], "elapsedMs": 1, "extra": true}),
    ];
    for body in bodies {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(ROUTE_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_body(body.clone())))
            .mount(&server)
            .await;
        let err = client(&server)
            .delete("PlantA", &paths(&["A", "B"]))
            .await
            .expect_err("refused");
        assert_eq!(
            route_code(&err),
            Some("route_answer_invalid"),
            "{body}: {err}"
        );
    }
}

/// Each 26-17 route code maps to `webdev_route_error` with that code, on
/// the handshake and on a delete.
#[tokio::test]
async fn denial_codes_map() {
    for code in [
        "secret_required",
        "secret_mismatch",
        "unknown_action",
        "provider_not_allowed",
        "path_invalid",
        "too_many_paths",
        "route_error",
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(ROUTE_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_json(denial(code)))
            .mount(&server)
            .await;
        let route = client(&server);
        let err = route.handshake().await.expect_err("denied");
        assert_eq!(err.code(), "webdev_route_error");
        assert_eq!(route_code(&err), Some(code));
        let err = route
            .delete("PlantA", &paths(&["A"]))
            .await
            .expect_err("denied");
        assert_eq!(route_code(&err), Some(code));
        let rendered = format!("{err} {err:?} {}", err.hint().unwrap_or_default());
        for value in [FAKE_SECRET, FAKE_PASSWORD] {
            assert!(!rendered.contains(value), "a credential leaked: {rendered}");
        }
    }
}

#[tokio::test]
async fn statuses_map() {
    for (status, slug) in [
        (401, "auth_rejected"),
        (403, "auth_rejected"),
        (402, "webdev_unlicensed"),
        (405, "routes_not_deployed"),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(ROUTE_PATH))
            .respond_with(ResponseTemplate::new(status))
            .mount(&server)
            .await;
        let err = client(&server).handshake().await.expect_err("status");
        assert_eq!(err.code(), slug, "HTTP {status}: {err}");
        if let CoreError::RoutesNotDeployed { route, project, .. } = &err {
            assert_eq!(route, "tagDelete");
            assert_eq!(project, "ign-cli");
            let hint = err.hint().expect("hint");
            assert!(hint.contains("--with-tag-delete"), "{hint}");
        }
    }
}

/// The readiness wait against a freshly imported route: HTTP 500, then
/// 405, then the handshake (26-17: the route answered `version` 9.6 s after
/// a cold import, 500 and 405 in between).
#[tokio::test]
async fn readiness_wait_rides_500_and_405() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(ROUTE_PATH))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(ROUTE_PATH))
        .respond_with(ResponseTemplate::new(405))
        .up_to_n_times(1)
        .with_priority(2)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(ROUTE_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_body(version_data())))
        .with_priority(3)
        .mount(&server)
        .await;
    let route = client(&server);
    let handshake = wait_ready(&route, Duration::from_secs(5), Duration::from_millis(10))
        .await
        .expect("ready after the window");
    assert_eq!(handshake.max_paths, 500);
    assert_eq!(
        server.received_requests().await.expect("recording").len(),
        3
    );
}

/// A 401 during the wait refuses at once: one request, no retry.
#[tokio::test]
async fn readiness_wait_refuses_auth_at_once() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(ROUTE_PATH))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let route = client(&server);
    let err = wait_ready(&route, Duration::from_secs(5), Duration::from_millis(10))
        .await
        .expect_err("auth");
    assert_eq!(err.code(), "auth_rejected");
    assert_eq!(
        server.received_requests().await.expect("recording").len(),
        1
    );
}
