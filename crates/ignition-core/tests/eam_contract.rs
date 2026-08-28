//! Wiremock contract tests for the EAM capability (07-02, BKUP-02):
//! the read family pinned on the REQUESTS — history through the
//! runtime seam (explicit limit ALWAYS; search passthrough), task
//! definitions through the config-resource seam (the tag-provider
//! family), and THE STATE GATE: the controller 403 (live-captured
//! message, Jetty HTML body) classifies to the additive
//! `eam_not_controller` slug — never a misleading `auth_rejected` —
//! while a generic 403 (no message, off the EAM path) keeps the Auth
//! mapping (the classification is path- AND content-scoped).

mod common;

use common::IgnitionMock;
use ignition_core::client::{GatewayApi, ReqwestGatewayApi};
use ignition_core::config::{Credential, Secret};

fn token_credential() -> Credential {
    Credential::Token(Secret::new("eam:tokengeneratedlive"))
}

/// The live-captured history page shape (trimmed to two items): the
/// forced-run taskName suffix, the Failed level with GNET detail,
/// epoch-ms numbers.
fn history_page() -> serde_json::Value {
    serde_json::json!({
        "items": [
            {
                "taskId": 2,
                "taskName": "nightly-backup (forced)",
                "taskStart": 1787930000000_i64,
                "taskEnd": 1787930009000_i64,
                "target": "_controller",
                "level": "Failed",
                "detail": "Gateway network for agent '_controller' is currently not connected, the connection status is 'NotDefined'",
                "taskType": "eam_backup"
            },
            {
                "taskId": 1,
                "taskName": "nightly-backup",
                "taskStart": 1787920000000_i64,
                "taskEnd": 1787920005000_i64,
                "target": "_controller",
                "level": "Success",
                "detail": null,
                "taskType": "eam_backup"
            }
        ],
        "metadata": {"total": 2, "matching": 2, "limit": 200, "offset": 0}
    })
}

/// The task-definition LIST page (config-resource shape; no state on
/// list records) + the FIND answer (definition + scheduledTaskState
/// + signature).
fn definition_list_page() -> serde_json::Value {
    serde_json::json!({
        "items": [
            {
                "name": "nightly-backup",
                "collection": "eam-tasks",
                "type": "com.inductiveautomation.eam",
                "config": {
                    "profile": {
                        "type": "eam_backup",
                        "scheduleMode": "OnDemand",
                        "settings": {
                            "targetGateways": ["gw-a"],
                            "targetGroups": [],
                            "concurrentBackups": 2,
                            "forceBackups": true
                        }
                    }
                }
            }
        ],
        "metadata": {"total": 1, "matching": 1, "limit": -1, "offset": 0}
    })
}

fn definition_find_body() -> serde_json::Value {
    serde_json::json!({
        "name": "nightly-backup",
        "collection": "eam-tasks",
        "type": "com.inductiveautomation.eam",
        "config": {"profile": {"type": "eam_backup", "scheduleMode": "OnDemand"}},
        "signature": "sig-abc123",
        "scheduledTaskState": {
            "currentState": "IDLE",
            "details": {"owner": "eam", "nextScheduled": null}
        }
    })
}

const HISTORY_PATH: &str = "/data/eam/api/v1/eam-tasks/history";
const TASKS_LIST_PATH: &str = "/data/api/v1/resources/list/com.inductiveautomation.eam/eam-tasks";
// The find path rides the ONE locked per-segment encoder — hyphens
// over-encode to %2D (over-encoding is safe; the server decodes
// before matching). Pinning the ENCODED path IS the discipline pin.
const TASKS_FIND_PATH: &str =
    "/data/api/v1/resources/find/com.inductiveautomation.eam/eam-tasks/nightly%2Dbackup";

/// THE history pin: the runtime GET rides with an EXPLICIT limit
/// (default 200 — never the server's unlimited default) and the
/// search passthrough; items round-trip wire-faithful.
#[tokio::test]
async fn eam_history_sends_explicit_limit_and_search() {
    let mock = IgnitionMock::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(HISTORY_PATH))
        .and(wiremock::matchers::query_param("limit", "50"))
        .and(wiremock::matchers::query_param("offset", "0"))
        .and(wiremock::matchers::query_param("search", "backup"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(history_page()))
        .expect(1)
        .mount_as_scoped(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let result = ignition_core::actions::eam::eam_history(&api, Some(50), Some("backup"))
        .await
        .expect("history reads");
    assert_eq!(result.count, 2);
    assert_eq!(result.items[0].task_name, "nightly-backup (forced)");
    assert_eq!(result.items[0].level.as_deref(), Some("Failed"));
    assert!(
        result.items[0]
            .detail
            .as_deref()
            .is_some_and(|d| d.contains("not connected"))
    );
    assert_eq!(guard.received_requests().await.len(), 1);
}

/// The default limit pin: no --limit → limit=200 rides the wire
/// (Pitfall 9's discipline, EAM edition).
#[tokio::test]
async fn eam_history_defaults_to_the_explicit_200_limit() {
    let mock = IgnitionMock::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(HISTORY_PATH))
        .and(wiremock::matchers::query_param("limit", "200"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(history_page()))
        .expect(1)
        .mount_as_scoped(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    ignition_core::actions::eam::eam_history(&api, None, None)
        .await
        .expect("history reads");
    assert_eq!(guard.received_requests().await.len(), 1);
}

/// The definitions LIST pin: the config-resource seam with the
/// standard list params (limit=-1, the UI everything convention) —
/// available on STOCK gateways (no controller needed).
#[tokio::test]
async fn eam_tasks_list_rides_the_config_resource_seam() {
    let mock = IgnitionMock::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(TASKS_LIST_PATH))
        .and(wiremock::matchers::query_param("limit", "-1"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(definition_list_page()))
        .expect(1)
        .mount_as_scoped(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let result = ignition_core::actions::eam::eam_tasks(&api)
        .await
        .expect("definitions list");
    assert_eq!(result.tasks.len(), 1);
    assert_eq!(result.tasks[0].name, "nightly-backup");
    assert_eq!(result.tasks[0].task_type.as_deref(), Some("eam_backup"));
    assert_eq!(result.tasks[0].schedule_mode.as_deref(), Some("OnDemand"));
    assert_eq!(
        result.tasks[0].current_state, None,
        "list records carry no state — null, honestly"
    );
    assert_eq!(guard.received_requests().await.len(), 1);
}

/// The FIND pin: the definition + its scheduledTaskState (the
/// summary's current_state source) + the signature.
#[tokio::test]
async fn eam_task_detail_carries_definition_and_state() {
    let mock = IgnitionMock::start().await;
    mock.list_json("GET", TASKS_FIND_PATH, definition_find_body())
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let result = ignition_core::actions::eam::eam_task_detail(&api, "nightly-backup")
        .await
        .expect("find reads");
    assert_eq!(result.name, "nightly-backup");
    assert_eq!(
        result.state["currentState"],
        serde_json::json!("IDLE"),
        "the healthcheck rides as data"
    );
    assert_eq!(
        result.definition["scheduledTaskState"]["details"]["owner"],
        serde_json::json!("eam"),
        "the owner (force's target) round-trips"
    );
}

/// An unknown definition name rides the config-resource not_found
/// path (404 → classify).
#[tokio::test]
async fn eam_task_detail_unknown_name_is_not_found() {
    let mock = IgnitionMock::start().await;
    mock.html_error(
        "GET",
        "/data/api/v1/resources/find/com.inductiveautomation.eam/eam-tasks/nope",
        404,
    )
    .await;
    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let err = ignition_core::actions::eam::eam_task_detail(&api, "nope")
        .await
        .expect_err("404 classifies NotFound");
    assert_eq!(err.exit_code(), 6);
    assert_eq!(err.code(), "not_found");
}

/// THE STATE GATE: the controller 403 on the RUNTIME seam (Jetty
/// HTML body carrying the live-captured message) classifies to the
/// additive `eam_not_controller` slug — exit 6, never auth_rejected —
/// pinned at the ACTION layer (through the whole client pipeline).
#[tokio::test]
async fn controller_403_classifies_eam_not_controller() {
    let mock = IgnitionMock::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(HISTORY_PATH))
        .respond_with(wiremock::ResponseTemplate::new(403).set_body_raw(
            "<html><head><title>Error 403</title></head><body><h2>HTTP ERROR 403 Forbidden</h2><table><tr><th>MESSAGE:</th><td>This operation can only be performed when EAM is configured as a controller.</td></tr></table></body></html>".as_bytes().to_vec(),
            "text/html;charset=iso-8859-1",
        ))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let err = ignition_core::actions::eam::eam_history(&api, None, None)
        .await
        .expect_err("the controller 403 refuses");
    assert_eq!(err.exit_code(), 6, "target state, not auth");
    assert_eq!(err.code(), "eam_not_controller");
    let hint = err.hint().expect("hint required");
    assert!(
        hint.contains("installMode") && hint.contains("Controller"),
        "the hint names the manual flip: {hint}"
    );
}

/// The state gate is CONTENT-scoped: an EAM-path 403 WITHOUT the
/// controller message keeps the honest under-permitted Auth mapping.
#[tokio::test]
async fn eam_403_without_the_message_stays_auth() {
    let mock = IgnitionMock::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(HISTORY_PATH))
        .respond_with(wiremock::ResponseTemplate::new(403).set_body_raw(
            "<html><head><title>Error 403</title></head><body><h2>HTTP ERROR 403 Forbidden</h2><table><tr><th>MESSAGE:</th><td>Forbidden</td></tr></table></body></html>".as_bytes().to_vec(),
            "text/html;charset=iso-8859-1",
        ))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let err = ignition_core::actions::eam::eam_history(&api, None, None)
        .await
        .expect_err("a plain 403 stays auth");
    assert_eq!(err.exit_code(), 5);
    assert_eq!(err.code(), "auth_rejected");
}

/// The state gate is PATH-scoped: a NON-EAM path answering the same
/// message cannot shift the classification (generic 403 → Auth).
#[tokio::test]
async fn non_eam_403_with_the_message_stays_auth() {
    let mock = IgnitionMock::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/gateway-info"))
        .respond_with(
            wiremock::ResponseTemplate::new(403).set_body_raw(
                "This operation can only be performed when EAM is configured as a controller."
                    .as_bytes()
                    .to_vec(),
                "text/plain",
            ),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let err = api
        .gateway_info()
        .await
        .expect_err("off-path 403 stays auth");
    assert_eq!(err.exit_code(), 5);
    assert_eq!(err.code(), "auth_rejected");
}
