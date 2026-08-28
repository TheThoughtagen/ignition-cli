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

// ---- Task 3: the guarded writes ----

const TASKS_CREATE_PATH: &str = "/data/api/v1/resources/com.inductiveautomation.eam/eam-tasks";

/// THE create body pin (a) — K=V auto-typing: `--target gw-a
/// --setting concurrentBackups=2 --setting forceBackups=true`
/// composes the ARRAY body with `targetGateways: ["gw-a"]`,
/// `concurrentBackups: 2` (JSON number), `forceBackups: true` (JSON
/// bool) — NO stringly-typed leaks. The body is pinned VERBATIM
/// (serde_json maps are key-sorted — the deterministic order the
/// recorded-request discipline pins).
#[tokio::test]
async fn task_create_posts_array_body_with_typed_settings() {
    let mock = IgnitionMock::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(TASKS_CREATE_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1)
        .mount_as_scoped(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let result = ignition_core::actions::eam::eam_task_create(
        &api,
        "nightly-backup",
        "eam_backup",
        &["gw-a".to_string()],
        &[
            "concurrentBackups=2".to_string(),
            "forceBackups=true".to_string(),
        ],
        None,
        "OnDemand",
    )
    .await
    .expect("create posts");
    assert_eq!(result.task_type, "eam_backup");

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).expect("body parses");
    assert_eq!(
        body,
        serde_json::json!([{
            "config": {"profile": {
                "concurrentBackups": 2,
                "forceBackups": true,
                "scheduleMode": "OnDemand",
                "targetGateways": ["gw-a"],
                "type": "eam_backup"
            }},
            "name": "nightly-backup"
        }]),
        "the ARRAY body, composed definition verbatim — settings TYPED"
    );
}

/// THE create body pin (b) — the `--definition` file path: a
/// full-JSON overlay carrying the live-captured eam_backup settings
/// shape (`targetGateways`/`targetGroups` arrays, `concurrentBackups`
/// int, `forceBackups` bool) deep-merged over the composed base
/// `{name, profile: {type, scheduleMode}}`.
#[tokio::test]
async fn task_create_deep_merges_the_definition_file() {
    let mock = IgnitionMock::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(TASKS_CREATE_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1)
        .mount_as_scoped(&mock.server)
        .await;

    let overlay = serde_json::json!({
        "targetGateways": ["gw-a", "gw-b"],
        "targetGroups": [],
        "concurrentBackups": 3,
        "forceBackups": false
    });
    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    ignition_core::actions::eam::eam_task_create(
        &api,
        "fleet-backup",
        "eam_backup",
        &[],
        &[],
        Some(&overlay),
        "OnDemand",
    )
    .await
    .expect("create posts");

    let requests = guard.received_requests().await;
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).expect("body parses");
    assert_eq!(
        body,
        serde_json::json!([{
            "config": {"profile": {
                "concurrentBackups": 3,
                "forceBackups": false,
                "scheduleMode": "OnDemand",
                "targetGateways": ["gw-a", "gw-b"],
                "targetGroups": [],
                "type": "eam_backup"
            }},
            "name": "fleet-backup"
        }]),
        "the overlay's typed/array settings deep-merged over the base"
    );
}

/// The refused ladder rung at the ACTION layer: a fleet-destructive
/// type NEVER reaches a client (zero requests) — exit 6,
/// `eam_task_type_refused`, the message names the EXT-03 scope.
#[tokio::test]
async fn task_create_refused_type_never_reaches_the_wire() {
    let mock = IgnitionMock::start().await;
    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    for refused in [
        "eam_restoreBackup",
        "eam_installModules",
        "eam_remoteUpgrade",
    ] {
        let err = ignition_core::actions::eam::eam_task_create(
            &api,
            "danger",
            refused,
            &["gw-a".to_string()],
            &[],
            None,
            "OnDemand",
        )
        .await
        .expect_err("fleet-destructive types refuse");
        assert_eq!(err.exit_code(), 6);
        assert_eq!(err.code(), "eam_task_type_refused");
        let message = err.to_string();
        assert!(
            message.contains(refused) && message.contains("fleet-destructive"),
            "the message names the type + consequence: {message}"
        );
        assert!(
            message.contains("EXT-03"),
            "the message points at the v2 scope: {message}"
        );
    }
    assert!(
        mock.server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty(),
        "refusals do no network work"
    );
}

/// A malformed `--setting` refuses `invalid_input` (exit 2) before
/// any network work.
#[tokio::test]
async fn task_create_malformed_setting_refuses_pre_network() {
    let mock = IgnitionMock::start().await;
    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let err = ignition_core::actions::eam::eam_task_create(
        &api,
        "t",
        "eam_backup",
        &[],
        &["noequalsign".to_string()],
        None,
        "OnDemand",
    )
    .await
    .expect_err("malformed K=V refuses");
    assert_eq!(err.exit_code(), 2);
    assert_eq!(err.code(), "invalid_input");
    assert!(
        mock.server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
}

/// THE force sequence pin: find GET → force POST (owner from the
/// healthcheck's `scheduledTaskState.details.owner`) → history GET —
/// exactly 3 requests, the 204 accepted, and the honest history
/// read-back surfaces the Forced/Failed outcome as data.
#[tokio::test]
async fn task_force_is_the_three_request_sequence() {
    let mock = IgnitionMock::start().await;
    // 1. find — carries the owner under the healthcheck details.
    mock.list_json(
        "GET",
        "/data/api/v1/resources/find/com.inductiveautomation.eam/eam-tasks/nightly%2Dbackup",
        serde_json::json!({
            "name": "nightly-backup",
            "config": {"profile": {"type": "eam_backup", "scheduleMode": "OnDemand"}},
            "scheduledTaskState": {
                "currentState": "IDLE",
                "details": {"owner": "eam"}
            }
        }),
    )
    .await;
    // 2. force — the live-proven 204.
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/data/eam/api/v1/eam-tasks/force/eam/nightly-backup",
        ))
        .respond_with(wiremock::ResponseTemplate::new(204))
        .expect(1)
        .mount(&mock.server)
        .await;
    // 3. history re-read — the forced run's entry.
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(HISTORY_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {
                        "taskId": 9,
                        "taskName": "nightly-backup (forced)",
                        "taskStart": 1787930000000_i64,
                        "taskEnd": 1787930009000_i64,
                        "target": "_controller",
                        "level": "Failed",
                        "detail": "Gateway network for agent '_controller' is currently not connected",
                        "taskType": "eam_backup"
                    }
                ],
                "metadata": {"total": 1, "matching": 1, "limit": 20, "offset": 0}
            })),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let result = ignition_core::actions::eam::eam_task_force(&api, "nightly-backup")
        .await
        .expect("the sequence completes");
    assert_eq!(result.owner, "eam", "owner resolved from the healthcheck");
    assert!(result.dispatched);
    let entry = result.history.expect("the forced entry is visible");
    assert_eq!(entry.task_name, "nightly-backup (forced)");
    assert_eq!(
        entry.level.as_deref(),
        Some("Failed"),
        "the outcome is data"
    );

    let requests = mock.server.received_requests().await.unwrap_or_default();
    assert_eq!(requests.len(), 3, "find → force → history, exactly");
    let sequence: Vec<(&str, String)> = requests
        .iter()
        .map(|request| (request.method.as_str(), request.url.path().to_string()))
        .collect();
    assert_eq!(
        sequence,
        vec![
            (
                "GET",
                "/data/api/v1/resources/find/com.inductiveautomation.eam/eam-tasks/nightly%2Dbackup"
                    .to_string()
            ),
            (
                "POST",
                "/data/eam/api/v1/eam-tasks/force/eam/nightly-backup".to_string()
            ),
            ("GET", "/data/eam/api/v1/eam-tasks/history".to_string()),
        ],
        "the request SEQUENCE is the contract"
    );
}

/// Owner fallback: a find answer WITHOUT the healthcheck owner
/// forces against the live-captured default `"eam"`.
#[tokio::test]
async fn task_force_owner_falls_back_to_eam() {
    let mock = IgnitionMock::start().await;
    mock.list_json(
        "GET",
        "/data/api/v1/resources/find/com.inductiveautomation.eam/eam-tasks/bare",
        serde_json::json!({"name": "bare", "config": {}}),
    )
    .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/data/eam/api/v1/eam-tasks/force/eam/bare",
        ))
        .respond_with(wiremock::ResponseTemplate::new(204))
        .expect(1)
        .mount(&mock.server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(HISTORY_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [],
                "metadata": {"total": 0, "matching": 0, "limit": 20, "offset": 0}
            })),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let result = ignition_core::actions::eam::eam_task_force(&api, "bare")
        .await
        .expect("fallback owner forces");
    assert_eq!(result.owner, "eam");
    assert!(result.history.is_none(), "no entry yet — null, honestly");
}
