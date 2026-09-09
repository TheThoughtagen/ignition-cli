//! Wiremock contract tests for the EAM capability (07-02, BKUP-02):
//! the read family pinned on the REQUESTS — history through the
//! runtime seam (explicit limit ALWAYS; search passthrough), task
//! definitions through the config-resource seam (the tag-provider
//! family), and THE STATE GATE: the controller 403 (live-captured
//! message, Jetty HTML body) classifies to the additive
//! `eam_not_controller` slug — never a misleading `auth_rejected` —
//! while a generic 403 (no message, off the EAM path) keeps the Auth
//! mapping (the classification is path- AND content-scoped).
//!
//! 10-02 extends the pins to the WRITE SURFACE (suspend/resume/
//! cancel, the scheduled/{running} read, the full-record PUT modify,
//! the signature-keyed DELETE) — every fixture VERBATIM from
//! 10-LIVE-CAPTURES.md (both rigs, 8.3.3 + 8.3.6) or visibly marked
//! spec-shaped where 10-01 honestly could not capture (Running/
//! Pending rows, the confirm-demand shape, lifecycle 404s).

mod common;

use common::IgnitionMock;
use ignition_core::client::{GatewayApi, ReqwestGatewayApi};
use ignition_core::config::{Credential, Secret};

fn token_credential() -> Credential {
    Credential::Token(Secret::new("eam:tokengeneratedlive"))
}

/// The live-captured history page shape (trimmed to two items): the
/// UUID-string taskIds (8.3.3 wire-faithful — 07-05 gap 1), the
/// forced-run taskName suffix, the Failed level with GNET detail,
/// epoch-ms numbers.
fn history_page() -> serde_json::Value {
    serde_json::json!({
        "items": [
            {
                "taskId": "c3d5ebc2-0b91-40fc-8417-3af372071547",
                "taskName": "nightly-backup (forced)",
                "taskStart": 1787930000000_i64,
                "taskEnd": 1787930009000_i64,
                "target": "_controller",
                "level": "Failed",
                "detail": "Gateway network for agent '_controller' is currently not connected, the connection status is 'NotDefined'",
                "taskType": "eam_backup"
            },
            {
                "taskId": "d4e6fcd3-1c92-410d-8528-4ba483082658",
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
                        "scheduleMode": "OnDemand"
                    },
                    "settings": {
                        "targetGateways": ["gw-a"],
                        "targetGroups": [],
                        "concurrentBackups": 2,
                        "forceBackups": true
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
            "config": {
                "profile": {"scheduleMode": "OnDemand", "type": "eam_backup"},
                "settings": {
                    "concurrentBackups": 2,
                    "forceBackups": true,
                    "targetGateways": ["gw-a"],
                    "targetGroups": []
                }
            },
            "name": "nightly-backup"
        }]),
        "the ARRAY body, composed definition verbatim — the live 8.3.3 \
         profile/settings split with settings TYPED"
    );
}

/// THE create body pin (b) — the `--definition` file path: a
/// full-JSON overlay carrying the live-captured eam_backup settings
/// shape (`targetGateways`/`targetGroups` arrays, `concurrentBackups`
/// int, `forceBackups` bool) deep-merged over the composed
/// `config.settings` (zero `--target` defaults to
/// `["_controller"]`; the overlay's arrays REPLACE it).
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
            "config": {
                "profile": {"scheduleMode": "OnDemand", "type": "eam_backup"},
                "settings": {
                    "concurrentBackups": 3,
                    "forceBackups": false,
                    "targetGateways": ["gw-a", "gw-b"],
                    "targetGroups": []
                }
            },
            "name": "fleet-backup"
        }]),
        "the overlay's typed/array settings deep-merged over the composed config.settings"
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
                        "taskId": "e5f7ade4-2da3-421e-9639-5cb594193769",
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

// ---- 10-02: the write surface (capture-locked per 10-LIVE-CAPTURES.md) ----

/// The captured lifecycle-path segments (raw names, §1/§7).
const SUSPEND_PATH: &str = "/data/eam/api/v1/eam-tasks/suspend/nightly-backup";
const RESUME_PATH: &str = "/data/eam/api/v1/eam-tasks/resume/nightly-backup";
const CANCEL_PATH: &str = "/data/eam/api/v1/eam-tasks/cancel/nightly-backup";
const SCHEDULED_FALSE_PATH: &str = "/data/eam/api/v1/eam-tasks/scheduled/false";
const SCHEDULED_TRUE_PATH: &str = "/data/eam/api/v1/eam-tasks/scheduled/true";
// The delete path rides the ONE locked per-segment encoder (the
// find-path discipline pin above: hyphens over-encode to %2D).
const TASKS_DELETE_PATH: &str =
    "/data/api/v1/resources/com.inductiveautomation.eam/eam-tasks/nightly%2Dbackup/sig%2Dabc123";

/// The captured controller-state 403 HTML (the state-gate message;
/// identical body rides every /data/eam/api/v1/* route).
fn controller_403_body() -> Vec<u8> {
    "<html><head><title>Error 403</title></head><body><h2>HTTP ERROR 403 Forbidden</h2><table><tr><th>MESSAGE:</th><td>This operation can only be performed when EAM is configured as a controller.</td></tr></table></body></html>".as_bytes().to_vec()
}

/// The captured scheduled row, VERBATIM (8.3.6 09:59:39Z +
/// byte-identical 8.3.3 09:59:40Z — 10-LIVE-CAPTURES §2): all 13
/// keys, the human-label type, the Scheduled can* truth cell.
fn scheduled_false_page() -> serde_json::Value {
    serde_json::json!({
        "items": [
            {
                "name": "ign-p10-scratch-sched",
                "owner": "eam",
                "type": "Collect Backup",
                "execStart": null,
                "message": "",
                "repeats": true,
                "canPause": true,
                "canResume": false,
                "canCancel": true,
                "taskState": "Scheduled",
                "isForced": false,
                "isRunning": false,
                "progress": 0.0
            }
        ],
        "metadata": {"total": 1, "matching": 1, "limit": -1, "offset": 0}
    })
}

/// The captured quiet-controller body (10-LIVE-CAPTURES §8 — the
/// canonical scheduled/true answer).
fn scheduled_true_page() -> serde_json::Value {
    serde_json::json!({
        "items": [],
        "metadata": {"total": 0, "matching": 0, "limit": -1, "offset": 0}
    })
}

/// THE suspend pin (capture: 10-LIVE-CAPTURES §1c/§1d — 204 success
/// on both rigs): POST to the exact raw-name path, 204 consumed,
/// ZERO request body (the post_empty shape).
#[tokio::test]
async fn suspend_204_pins_post_path_and_empty_body() {
    let mock = IgnitionMock::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(SUSPEND_PATH))
        .respond_with(wiremock::ResponseTemplate::new(204))
        .expect(1)
        .mount_as_scoped(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    api.eam_task_suspend("nightly-backup")
        .await
        .expect("the 204 is the success shape");

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url.path(), SUSPEND_PATH);
    assert!(
        requests[0].body.is_empty(),
        "lifecycle POSTs carry NO body — params ride nothing"
    );
}

/// THE resume pin (capture: §1b/§1d — 204 on both rigs, incl. a
/// never-suspended task).
#[tokio::test]
async fn resume_204_pins_post_path_and_empty_body() {
    let mock = IgnitionMock::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(RESUME_PATH))
        .respond_with(wiremock::ResponseTemplate::new(204))
        .expect(1)
        .mount_as_scoped(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    api.eam_task_resume("nightly-backup")
        .await
        .expect("the 204 is the success shape");

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url.path(), RESUME_PATH);
    assert!(requests[0].body.is_empty());
}

/// THE cancel pin (capture: §7 — 204 always, nothing-pending AND
/// unknown-name alike; wire-idempotent).
#[tokio::test]
async fn cancel_204_pins_post_path_and_empty_body() {
    let mock = IgnitionMock::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(CANCEL_PATH))
        .respond_with(wiremock::ResponseTemplate::new(204))
        .expect(1)
        .mount_as_scoped(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    api.eam_task_cancel("nightly-backup")
        .await
        .expect("the 204 is the success shape");

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url.path(), CANCEL_PATH);
    assert!(requests[0].body.is_empty());
}

/// The controller-403 gate on SUSPEND (10-02 pitfall-6 discipline,
/// one test per verb): the path-scoped arm catches the new runtime
/// URL — the captured controller message classifies
/// `eam_not_controller`, never auth_rejected.
#[tokio::test]
async fn suspend_controller_403_classifies_eam_not_controller() {
    let mock = IgnitionMock::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(SUSPEND_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(403).set_body_raw(
                controller_403_body(),
                "text/html;charset=iso-8859-1",
            ),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let err = api
        .eam_task_suspend("nightly-backup")
        .await
        .expect_err("the controller 403 refuses");
    assert_eq!(err.exit_code(), 6, "target state, not auth");
    assert_eq!(err.code(), "eam_not_controller");
}

/// The controller-403 gate on RESUME (same proof, second verb).
#[tokio::test]
async fn resume_controller_403_classifies_eam_not_controller() {
    let mock = IgnitionMock::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(RESUME_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(403).set_body_raw(
                controller_403_body(),
                "text/html;charset=iso-8859-1",
            ),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let err = api
        .eam_task_resume("nightly-backup")
        .await
        .expect_err("the controller 403 refuses");
    assert_eq!(err.exit_code(), 6);
    assert_eq!(err.code(), "eam_not_controller");
}

/// The controller-403 gate on CANCEL (same proof, third verb).
#[tokio::test]
async fn cancel_controller_403_classifies_eam_not_controller() {
    let mock = IgnitionMock::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(CANCEL_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(403).set_body_raw(
                controller_403_body(),
                "text/html;charset=iso-8859-1",
            ),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let err = api
        .eam_task_cancel("nightly-backup")
        .await
        .expect_err("the controller 403 refuses");
    assert_eq!(err.exit_code(), 6);
    assert_eq!(err.code(), "eam_not_controller");
}

/// Message-scoping still holds on the NEW paths (the existing
/// non-message test extended to one new verb): an EAM-path 403
/// WITHOUT the controller message keeps the honest Auth mapping.
#[tokio::test]
async fn new_runtime_verb_403_without_the_message_stays_auth() {
    let mock = IgnitionMock::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(SUSPEND_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(403).set_body_raw(
                "<html><head><title>Error 403</title></head><body><h2>HTTP ERROR 403 Forbidden</h2><table><tr><th>MESSAGE:</th><td>Forbidden</td></tr></table></body></html>".as_bytes().to_vec(),
                "text/html;charset=iso-8859-1",
            ),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let err = api
        .eam_task_suspend("nightly-backup")
        .await
        .expect_err("a plain 403 stays auth");
    assert_eq!(err.exit_code(), 5);
    assert_eq!(err.code(), "auth_rejected");
}

/// THE scheduled read pin, `scheduled/false` (capture VERBATIM:
/// 10-LIVE-CAPTURES §2): the row parses wire-faithful — the captured
/// taskState string rides verbatim, the can* truth cell holds, the
/// human-label type never conflates with profile.type.
#[tokio::test]
async fn scheduled_false_read_parses_the_captured_row() {
    let mock = IgnitionMock::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(SCHEDULED_FALSE_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(scheduled_false_page()),
        )
        .expect(1)
        .mount_as_scoped(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let tasks = api
        .eam_tasks_scheduled(false)
        .await
        .expect("the read parses");
    assert_eq!(tasks.len(), 1);
    let row = &tasks[0];
    assert_eq!(row.name, "ign-p10-scratch-sched");
    assert_eq!(row.owner, "eam");
    assert_eq!(
        row.task_type.as_deref(),
        Some("Collect Backup"),
        "the human label rides verbatim"
    );
    assert_eq!(row.exec_start, None, "execStart null while scheduled");
    assert!(row.can_pause && !row.can_resume && row.can_cancel);
    assert_eq!(
        row.task_state, "Scheduled",
        "the captured taskState string, verbatim"
    );
    assert!(!row.is_forced && !row.is_running);
    assert_eq!(row.progress, 0.0);
    assert_eq!(guard.received_requests().await.len(), 1);
}

/// THE scheduled read pin, `scheduled/true` (capture VERBATIM:
/// §8's empty-list quiet-controller body) — empty Vec, honestly.
#[tokio::test]
async fn scheduled_true_read_parses_the_empty_body() {
    let mock = IgnitionMock::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(SCHEDULED_TRUE_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(scheduled_true_page()),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let tasks = api.eam_tasks_scheduled(true).await.expect("empty parses");
    assert!(tasks.is_empty(), "the quiet body is an honest empty list");
}

/// THE modify pin (capture: §6a full-record echo-modify): the
/// REQUEST BODY is the FULL single-element array INCLUDING
/// `config.settings` (the 422 trap — §6b) and the ORIGINAL
/// `signature` key; the 200 body parses into ModifyOutcome with the
/// post-write newSignature (§6a verbatim response).
#[tokio::test]
async fn task_modify_puts_full_array_body_and_parses_the_outcome() {
    let mock = IgnitionMock::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("PUT"))
        .and(wiremock::matchers::path(TASKS_CREATE_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
            serde_json::json!({
                "success": true,
                "changes": [
                    {
                        "name": "ign-p10-scratch-sched",
                        "type": "com.inductiveautomation.eam/eam-tasks",
                        "collection": "core",
                        "newSignature": "0d0dfea2919abb1f02fc86baea73d99696626524169a9ac36526044f89ac16e0"
                    }
                ],
                "problem": null
            }),
        ))
        .expect(1)
        .mount_as_scoped(&mock.server)
        .await;

    // The FULL find record (the §6a baseline shape): settings ride,
    // the ORIGINAL signature rides, only the mutated key differs.
    let full_record = serde_json::json!({
        "name": "ign-p10-scratch-sched",
        "type": "com.inductiveautomation.eam",
        "collection": "core",
        "enabled": true,
        "version": 1,
        "signature": "e5ac8bee3a6ba85e40923c0e02d29507600c57519eb8e4d78bd8c258197fe9c6",
        "config": {
            "profile": {
                "type": "eam_backup",
                "isSuspended": false,
                "scheduleMode": "Scheduled",
                "scheduleDetails": "0/30 * * * * ?"
            },
            "settings": {
                "targetGateways": ["_controller"],
                "targetGroups": [],
                "concurrentBackups": 0,
                "forceBackups": false
            }
        },
        "data": ["config.json"]
    });

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let outcome = api
        .eam_task_modify(&full_record)
        .await
        .expect("the 200 body parses")
        .expect("the body is present (not the lenient None)");
    assert!(outcome.success);
    assert_eq!(outcome.changes.len(), 1);
    assert_eq!(
        outcome.changes[0].new_signature.as_deref(),
        Some("0d0dfea2919abb1f02fc86baea73d99696626524169a9ac36526044f89ac16e0"),
        "newSignature is authoritative for the NEXT mutation"
    );
    assert_eq!(outcome.problem, None, "problem null on every success");

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value =
        serde_json::from_slice(&requests[0].body).expect("body parses");
    let array = body.as_array().expect("the body is a JSON ARRAY");
    assert_eq!(array.len(), 1, "single-element array — the §6a shape");
    let sent = &array[0];
    assert!(
        sent["config"]["settings"].is_object() && !sent["config"]["settings"].as_object().unwrap().is_empty(),
        "config.settings RIDES — omitting it is the 422 trap (§6b)"
    );
    assert!(
        sent["signature"].is_string(),
        "the ORIGINAL signature key is present — the modify contract"
    );
    assert_eq!(sent, &full_record, "echo-modify: the full record lands verbatim");
}

/// THE delete pin (capture: §3b — correct signature, no confirm, 200
/// success; §3c — `collection=core` is the proven-correct value, the
/// type token 404s): the RECORDED request carries `collection=core`
/// and NO confirm param; the captured success body parses into
/// DeleteOutcome (references []).
#[tokio::test]
async fn task_delete_pins_query_params_and_parses_the_success_body() {
    let mock = IgnitionMock::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path(TASKS_DELETE_PATH))
        .and(wiremock::matchers::query_param("collection", "core"))
        .and(wiremock::matchers::query_param_is_missing("confirm"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
            serde_json::json!({
                "success": true,
                "changes": [
                    {
                        "name": "ign-p10-scratch-sched",
                        "type": "com.inductiveautomation.eam/eam-tasks",
                        "collection": "core",
                        "newSignature": "ec961ee921c63b18013094870ed2664331e965c4770fdf84bfe136e0b4164244"
                    }
                ],
                "problem": null,
                "references": []
            }),
        ))
        .expect(1)
        .mount_as_scoped(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let outcome = api
        .eam_task_delete("nightly-backup", "sig-abc123", false)
        .await
        .expect("the captured success body parses");
    assert!(outcome.success);
    assert_eq!(outcome.changes[0].name, "ign-p10-scratch-sched");
    assert_eq!(
        outcome.changes[0].new_signature.as_deref(),
        Some("ec961ee921c63b18013094870ed2664331e965c4770fdf84bfe136e0b4164244"),
        "the DELETED resource's final signature rides changes[].newSignature"
    );
    assert_eq!(
        outcome.references,
        Some(Vec::new()),
        "references [] on success, honestly empty"
    );

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].url.query(),
        Some("collection=core"),
        "the exact captured query — confirm absent on the default delete"
    );
}

/// The confirm OPT-IN rides the wire (capture: §3c's successful
/// `?confirm=true&collection=core` request): both params together,
/// exact order pinned on the recorded query string.
#[tokio::test]
async fn task_delete_confirm_opt_in_rides_the_query() {
    let mock = IgnitionMock::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path(TASKS_DELETE_PATH))
        .and(wiremock::matchers::query_param("collection", "core"))
        .and(wiremock::matchers::query_param("confirm", "true"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
            serde_json::json!({
                "success": true,
                "changes": [
                    {
                        "name": "ign-p10-scratch-sched",
                        "type": "com.inductiveautomation.eam/eam-tasks",
                        "collection": "core",
                        "newSignature": "e3610cfe01c7df086da6596ccfbb7735abd5b8f2ceafd5916944919975902acf"
                    }
                ],
                "problem": null,
                "references": []
            }),
        ))
        .expect(1)
        .mount_as_scoped(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    api.eam_task_delete("nightly-backup", "sig-abc123", true)
        .await
        .expect("the confirmed delete parses");

    let requests = guard.received_requests().await;
    assert_eq!(
        requests[0].url.query(),
        Some("collection=core&confirm=true"),
        "the §3c captured request shape"
    );
}

/// THE FINDING PIN (10-LIVE-CAPTURES §3a/§4 + Decision 4, verbatim
/// 8.3.6 body): a signature mismatch answers HTTP **500** with a JSON
/// `{success:false, changes:[], problem{message, stacktrace}}` — the
/// current taxonomy has no honest slug (Three-Place rule: the
/// client layer does NOT classify unilaterally), so it lands as
/// `internal` (exit 1) with the problem body DISCARDED by the
/// classifier. Recorded here as the visible reference for the
/// 10-03/10-04 slug decision — the `signature mismatch` substring is
/// the stable discriminator across 8.3.3/8.3.6.
#[tokio::test]
async fn task_delete_signature_mismatch_500_is_the_recorded_finding() {
    let mock = IgnitionMock::start().await;
    wiremock::Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path(TASKS_DELETE_PATH))
        .and(wiremock::matchers::query_param("collection", "core"))
        .respond_with(wiremock::ResponseTemplate::new(500).set_body_json(
            serde_json::json!({
                "success": false,
                "changes": [],
                "problem": {
                    "message": "DELETE illegal: signature mismatch for 'ResourceId{resourcePath=com.inductiveautomation.eam/eam-tasks/ign-p10-scratch-sched, collectionName=core}'",
                    "stacktrace": [
                        "com.inductiveautomation.ignition.common.resourcecollection.PushException: DELETE illegal: signature mismatch for …",
                        "\tat com.inductiveautomation.ignition.gateway.resourcecollection.ChangeOperationValidationHandler$AtomicPushValidationHandler.throwIfInvalid(ChangeOperationValidationHandler.java:61)"
                    ]
                },
                "references": null
            }),
        ))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let err = api
        .eam_task_delete("nightly-backup", "sig-abc123", false)
        .await
        .expect_err("the 500 refusal is not a success");
    assert_eq!(err.exit_code(), 1, "the FINDING: exit-1 for a client-fixable mismatch");
    assert_eq!(err.code(), "internal", "no honest slug yet — 10-03/10-04 decides");
}

/// The captured lifecycle FAILURE shape (§1a verbatim): suspend of
/// an OnDemand/untriggered task answers 500 Jetty HTML — classified
/// `internal` with the page's own message surfaced (the
/// html_error_parts enrichment). Honest reference for 10-03's
/// find-before-write: suspend cannot distinguish unknown-task from
/// untriggered-task by status alone (§7).
#[tokio::test]
async fn suspend_500_html_failure_surfaces_the_page_message() {
    let mock = IgnitionMock::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(SUSPEND_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(500).set_body_raw(
                "<html>\n<head>\n<meta http-equiv=\"Content-Type\" content=\"text/html;charset=ISO-8859-1\"/>\n<title>Error 500</title>\n</head>\n<body><h2>HTTP ERROR 500 Task could not be suspended</h2>\n<table>\n<tr><th>URI:</th><td>/data/eam/api/v1/eam-tasks/suspend/ign-p10-scratch</td></tr>\n<tr><th>STATUS:</th><td>500</td></tr>\n<tr><th>MESSAGE:</th><td>Task could not be suspended</td></tr>\n</table>\n\n</body>\n</html>\n".as_bytes().to_vec(),
                "text/html;charset=iso-8859-1",
            ),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let err = api
        .eam_task_suspend("nightly-backup")
        .await
        .expect_err("the captured 500 refusal");
    assert_eq!(err.exit_code(), 1);
    assert_eq!(err.code(), "internal");
    assert!(
        err.to_string().contains("Task could not be suspended"),
        "the Jetty page's message rides the error detail: {err}"
    );
}

/// The 404 classification catches the new runtime URLs —
/// **spec-shaped, NOT capture-proven (10-01 limitation: 10-LIVE-CAPTURES
/// §7 records that lifecycle verbs answer 500/204 for unknown names;
/// NO 404 was ever captured on the /data/eam seam)**. This pins the
/// defensive classification only: if a curated-path 404 ever answers
/// here (route absence after a version change), it stays `not_found`
/// via the existing arm — zero new classify sites for 10-02.
#[tokio::test]
async fn runtime_verb_404_is_the_defensive_not_found_classification() {
    let mock = IgnitionMock::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(SUSPEND_PATH))
        .respond_with(wiremock::ResponseTemplate::new(404))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let err = api
        .eam_task_suspend("nightly-backup")
        .await
        .expect_err("404 classifies NotFound");
    assert_eq!(err.exit_code(), 6);
    assert_eq!(err.code(), "not_found");
}

// ---- 10-03: the ACTION layer (find-first lifecycle with
// authoritative re-checks — the composition the pure fns + the
// client pins above only PARTIALLY prove) ----

/// A find responder answering the FIRST call with `first` and every
/// later call with `then` — the find→write→read-back flows need the
/// two answers to differ (e.g. `isSuspended` false → true). A
/// stateful responder (not two same-matcher mocks) so the answer
/// order can NEVER depend on wiremock's multi-mock match ordering.
fn find_responder(
    first: serde_json::Value,
    then: serde_json::Value,
) -> impl Fn(&wiremock::Request) -> wiremock::ResponseTemplate {
    use std::sync::Mutex;
    let calls = Mutex::new(0usize);
    move |_request| {
        let mut calls = calls.lock().expect("counter locks");
        *calls += 1;
        let body = if *calls == 1 { first.clone() } else { then.clone() };
        wiremock::ResponseTemplate::new(200).set_body_json(body)
    }
}

/// THE suspend action sequence: find (isSuspended false) → suspend
/// POST (204) → find read-back (isSuspended true — Decision 1's
/// persistence proof). The result reports the pre-write state, the
/// persisted flag, and `fired: true` — exactly 2 finds + 1 POST.
#[tokio::test]
async fn suspend_action_finds_then_posts_then_readbacks_the_flag() {
    let mock = IgnitionMock::start().await;
    let find = |suspended: bool| {
        serde_json::json!({
            "name": "nightly-backup",
            "config": {"profile": {"type": "eam_backup", "isSuspended": suspended, "scheduleMode": "Scheduled"}},
            "signature": "sig-abc123",
            "scheduledTaskState": {"currentState": "Scheduled", "details": {"owner": "eam"}}
        })
    };
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(TASKS_FIND_PATH))
        .respond_with(find_responder(find(false), find(true)))
        .expect(2)
        .mount(&mock.server)
        .await;
    let post = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(SUSPEND_PATH))
        .respond_with(wiremock::ResponseTemplate::new(204))
        .expect(1)
        .mount_as_scoped(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let result = ignition_core::actions::eam::eam_task_suspend(&api, "nightly-backup")
        .await
        .expect("the lifecycle sequence completes");
    assert_eq!(result.task, "nightly-backup");
    assert_eq!(result.action, "suspended");
    assert_eq!(
        result.previous_state.as_deref(),
        Some("Scheduled"),
        "the find healthcheck's currentState, pre-write"
    );
    assert_eq!(
        result.config_suspended,
        Some(true),
        "the read-back proves the flag PERSISTED (capture Decision 1)"
    );
    assert_eq!(result.pending, None);
    assert!(result.fired);
    assert_eq!(result.reason, None);
    assert_eq!(post.received_requests().await.len(), 1, "one POST rode");
}

/// The suspend re-check at the ACTION layer: an already-suspended
/// task (find proves isSuspended true) refuses exit 2 naming the
/// task BEFORE the wire — the gateway's own answer would be the
/// indistinguishable 500 "Task could not be suspended" (§1a/§7).
#[tokio::test]
async fn suspend_action_refuses_already_suspended_pre_write() {
    let mock = IgnitionMock::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(TASKS_FIND_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
            serde_json::json!({
                "name": "nightly-backup",
                "config": {"profile": {"type": "eam_backup", "isSuspended": true, "scheduleMode": "Scheduled"}},
                "scheduledTaskState": {"currentState": "Suspended", "details": {"owner": "eam"}}
            }),
        ))
        .expect(1)
        .mount(&mock.server)
        .await;
    // expect(0): if the action wrongly fires, the server-drop
    // verification fails the test.
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(SUSPEND_PATH))
        .respond_with(wiremock::ResponseTemplate::new(204))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let err = ignition_core::actions::eam::eam_task_suspend(&api, "nightly-backup")
        .await
        .expect_err("already-suspended refuses pre-write");
    assert_eq!(err.exit_code(), 2);
    assert_eq!(err.code(), "invalid_input");
    let message = err.to_string();
    assert!(
        message.contains("nightly-backup") && message.contains("already suspended"),
        "the refusal names the task + state: {message}"
    );
}

/// The cancel no-op: nothing pending (both scheduled reads quiet)
/// returns the honest no-op result (`fired: false`, the reason
/// names it) WITHOUT firing the POST — mirroring the gateway's own
/// silent-204 semantics for cancel-with-nothing-pending (§7) minus
/// the pointless round trip.
#[tokio::test]
async fn cancel_action_without_pending_is_an_honest_noop() {
    let mock = IgnitionMock::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(TASKS_FIND_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
            serde_json::json!({
                "name": "nightly-backup",
                "config": {"profile": {"type": "eam_backup", "isSuspended": false, "scheduleMode": "OnDemand"}},
                "scheduledTaskState": {"currentState": "Stopped", "details": {"owner": "eam"}}
            }),
        ))
        .expect(1)
        .mount(&mock.server)
        .await;
    for running in ["false", "true"] {
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(format!(
                "/data/eam/api/v1/eam-tasks/scheduled/{running}"
            )))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(scheduled_true_page()),
            )
            .expect(1)
            .mount(&mock.server)
            .await;
    }
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(CANCEL_PATH))
        .respond_with(wiremock::ResponseTemplate::new(204))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let result = ignition_core::actions::eam::eam_task_cancel(&api, "nightly-backup")
        .await
        .expect("the no-op is a success-shaped result");
    assert_eq!(result.action, "cancelled");
    assert!(!result.fired, "nothing pending — the POST did not ride");
    assert_eq!(
        result.reason.as_deref(),
        Some("no pending execution"),
        "the honest reason rides the always-keys model"
    );
    assert_eq!(result.previous_state.as_deref(), Some("Stopped"));
}

/// The cancel fire path: a pending row with the captured can* truth
/// cell (canCancel true) → POST → the post-write scheduled read
/// shows the row GONE — `fired: true`, `pending: null`.
#[tokio::test]
async fn cancel_action_fires_against_a_cancellable_row() {
    let mock = IgnitionMock::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(TASKS_FIND_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
            serde_json::json!({
                "name": "nightly-backup",
                "config": {"profile": {"type": "eam_backup", "isSuspended": false, "scheduleMode": "Scheduled"}},
                "scheduledTaskState": {"currentState": "Scheduled", "details": {"owner": "eam"}}
            }),
        ))
        .expect(1)
        .mount(&mock.server)
        .await;
    // scheduled/false is read pre-write (the captured row present —
    // name adjusted to the test task) and post-write (row gone).
    let row_page = |name: &str| {
        let mut page = scheduled_false_page();
        page["items"][0]["name"] = serde_json::json!(name);
        page
    };
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(SCHEDULED_FALSE_PATH))
        .respond_with(find_responder(row_page("nightly-backup"), scheduled_true_page()))
        .expect(2)
        .mount(&mock.server)
        .await;
    // Only the POST-write read reaches scheduled/true — the pre-write
    // read finds the row in the false segment and short-circuits.
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(SCHEDULED_TRUE_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(scheduled_true_page()),
        )
        .expect(1)
        .mount(&mock.server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(CANCEL_PATH))
        .respond_with(wiremock::ResponseTemplate::new(204))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
    let result = ignition_core::actions::eam::eam_task_cancel(&api, "nightly-backup")
        .await
        .expect("the cancel sequence completes");
    assert!(result.fired);
    assert_eq!(result.reason, None);
    assert_eq!(
        result.pending, None,
        "the post-write read proves the execution is gone"
    );
    assert_eq!(result.previous_state.as_deref(), Some("Scheduled"));
}
