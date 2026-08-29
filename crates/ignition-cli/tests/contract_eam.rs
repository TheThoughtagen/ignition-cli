//! Golden-file contract tests for `ign eam` reads (07-02, BKUP-02):
//! the history human table + JSON envelope, the tasks list, and THE
//! CONTROLLER-REFUSAL golden — the additive `eam_not_controller`
//! slug with the manual-flip hint, pinned at the binary level (the
//! contract_status harness: wiremock gateway, isolated config,
//! snapbox inline goldens).

use std::path::{Path, PathBuf};

use assert_cmd::Command;

/// Isolated config dir + the config path inside it (file need not exist).
fn isolated_config() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.toml");
    (dir, path)
}

/// Write the one-profile dev config whose URL points at `url`.
fn write_profile_config(config: &Path, url: &str) {
    std::fs::write(
        config,
        format!(
            "active = \"dev\"\n\n[profiles.dev]\nurl = \"{url}\"\nauth = {{ token_env = \"IGNITION_TOKEN\" }}\n"
        ),
    )
    .expect("write config");
}

/// Spawn `ign` with an isolated config, the mock token, and args.
fn ign(config: &Path, url: &str, args: &[&str]) -> std::process::Output {
    let mut command = Command::cargo_bin("ign").expect("binary 'ign' not found");
    command
        .env("IGNITION_CLI_CONFIG", config)
        .env("IGNITION_TOKEN", "mock:name-key")
        .env("IGNITION_URL", url);
    command.args(args).output().expect("spawn ign")
}

/// stdout minus the single trailing newline `println!` appends.
fn stdout_for_golden(out: &std::process::Output) -> &str {
    let stdout = std::str::from_utf8(&out.stdout).expect("utf-8 stdout");
    stdout.strip_suffix('\n').unwrap_or(stdout)
}

/// stderr's JSON envelope starting at the first `{` (log-tolerant parse).
fn stderr_envelope(out: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&out.stderr);
    let start = stderr.find('{').unwrap_or(0);
    stderr[start..].to_string()
}

const HISTORY_PATH: &str = "/data/eam/api/v1/eam-tasks/history";
const TASKS_LIST_PATH: &str = "/data/api/v1/resources/list/com.inductiveautomation.eam/eam-tasks";

async fn mount_history(server: &wiremock::MockServer) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(HISTORY_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [
                {
                    "taskId": "c3d5ebc2-0b91-40fc-8417-3af372071547",
                    "taskName": "nightly-backup (forced)",
                    "taskStart": 1787930000000_i64,
                    "taskEnd": 1787930009000_i64,
                    "target": "_controller",
                    "level": "Failed",
                    "detail": "Gateway network for agent '_controller' is currently not connected",
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
        })))
        .expect(1..)
        .mount(server)
        .await;
}

async fn mount_tasks_list(server: &wiremock::MockServer) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(TASKS_LIST_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {
                        "name": "nightly-backup",
                        "collection": "eam-tasks",
                        "config": {
                            "profile": {"type": "eam_backup", "scheduleMode": "OnDemand"}
                        }
                    }
                ],
                "metadata": {"total": 1, "matching": 1, "limit": -1, "offset": 0}
            })),
        )
        .expect(1..)
        .mount(server)
        .await;
}

/// History HUMAN golden: the item table under the profile header —
/// taskName with forced marker, level/detail as data, ISO times.
#[tokio::test]
async fn eam_history_human_golden() {
    let server = wiremock::MockServer::start().await;
    mount_history(&server).await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let out = ign(&config, &server.uri(), &["eam", "history"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
2026-08-28T15:13:20.000Z  nightly-backup (forced)  [Failed]  target=_controller  Gateway network for agent '_controller' is currently not connected
2026-08-28T12:26:40.000Z  nightly-backup  [Success]  target=_controller  
(2 run(s))
"#]],
    );
}

/// History COMPACT JSON golden: items passthrough (wire keys) +
/// count.
#[tokio::test]
async fn eam_history_json_golden() {
    let server = wiremock::MockServer::start().await;
    mount_history(&server).await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let out = ign(&config, &server.uri(), &["eam", "history", "--compact"]);
    assert_eq!(out.status.code(), Some(0));
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[
            r#"{"ok":true,"profile":"dev","data":{"items":[{"taskId":"c3d5ebc2-0b91-40fc-8417-3af372071547","taskName":"nightly-backup (forced)","taskStart":1787930000000,"taskEnd":1787930009000,"target":"_controller","level":"Failed","detail":"Gateway network for agent '_controller' is currently not connected","taskType":"eam_backup"},{"taskId":"d4e6fcd3-1c92-410d-8528-4ba483082658","taskName":"nightly-backup","taskStart":1787920000000,"taskEnd":1787920005000,"target":"_controller","level":"Success","detail":null,"taskType":"eam_backup"}],"count":2}}"#
        ]],
    );
}

/// THE raw-capture contract (07-05 gap 1): a wiremock body shaped
/// EXACTLY like the live 8.3.3 capture in
/// `.planning/debug/eam-history-raw.json` — UUID-string `taskId`,
/// `" (forced)"` taskName, `Failed` level, the GNET-not-connected
/// detail, the `{items, metadata}` envelope — decodes and renders
/// exit 0 with the entry passthrough (the old numeric-`taskId`
/// model died here with a decode `internal_error`).
#[tokio::test]
async fn eam_history_decodes_the_raw_capture() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(HISTORY_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
            serde_json::json!({
                "items": [
                    {
                        "taskId": "a2f4dab1-9a8f-4feb-9306-29e261f60453",
                        "taskName": "cli-research-backup (forced)",
                        "taskStart": 1788012345678_i64,
                        "taskEnd": 1788012345890_i64,
                        "target": "_controller",
                        "level": "Failed",
                        "detail": "Attempt 1: Gateway network for agent '_controller' is currently not connected, the connection status is 'NotDefined'",
                        "taskType": "backup"
                    }
                ],
                "metadata": {"total": 1, "matching": 1, "limit": 200, "offset": 0}
            }),
        ))
        .expect(1..)
        .mount(&server)
        .await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let out = ign(&config, &server.uri(), &["eam", "history", "--compact"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[
            r#"{"ok":true,"profile":"dev","data":{"items":[{"taskId":"a2f4dab1-9a8f-4feb-9306-29e261f60453","taskName":"cli-research-backup (forced)","taskStart":1788012345678,"taskEnd":1788012345890,"target":"_controller","level":"Failed","detail":"Attempt 1: Gateway network for agent '_controller' is currently not connected, the connection status is 'NotDefined'","taskType":"backup"}],"count":1}}"#
        ]],
    );
}

/// Tasks LIST golden (human + compact): the agent-stable summary
/// keys — null current_state honestly (list records carry none).
#[tokio::test]
async fn eam_tasks_list_goldens() {
    let server = wiremock::MockServer::start().await;
    mount_tasks_list(&server).await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());

    let out = ign(&config, &server.uri(), &["eam", "tasks"]);
    assert_eq!(out.status.code(), Some(0));
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
nightly-backup  type=eam_backup  schedule=OnDemand  state=-"#]],
    );

    let out = ign(&config, &server.uri(), &["eam", "tasks", "--compact"]);
    assert_eq!(out.status.code(), Some(0));
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[
            r#"{"ok":true,"profile":"dev","data":{"tasks":[{"name":"nightly-backup","task_type":"eam_backup","schedule_mode":"OnDemand","current_state":null}]}}"#
        ]],
    );
}

/// THE controller-refusal golden: the runtime seam's 403 (Jetty HTML
/// + the live-captured message) surfaces as `eam_not_controller`
/// (exit 6) with the manual-flip hint — never auth_rejected.
#[tokio::test]
async fn controller_refusal_golden() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(HISTORY_PATH))
        .respond_with(wiremock::ResponseTemplate::new(403).set_body_raw(
            "<html><head><title>Error 403</title></head><body><h2>HTTP ERROR 403 Forbidden</h2><table><tr><th>MESSAGE:</th><td>This operation can only be performed when EAM is configured as a controller.</td></tr></table></body></html>".as_bytes().to_vec(),
            "text/html;charset=iso-8859-1",
        ))
        .expect(1..)
        .mount(&server)
        .await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());

    let out = ign(&config, &server.uri(), &["eam", "history", "--compact"]);
    assert_eq!(out.status.code(), Some(6), "target state");
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    let body: serde_json::Value =
        serde_json::from_str(&stderr_envelope(&out)).expect("error envelope parses");
    assert_eq!(body["profile"], serde_json::Value::String("dev".into()));
    assert_eq!(
        body["error"]["code"],
        serde_json::Value::String("eam_not_controller".into())
    );
    let hint = body["error"]["hint"].as_str().expect("hint");
    assert!(
        hint.contains("installMode") && hint.contains("Controller"),
        "the hint names the manual flip: {hint}"
    );
}

// ---- Task 3: the guarded writes' binary goldens ----

/// THE ladder-at-the-binary pins: verdicts computed from parsed
/// args PRE-RESOLUTION — zero requests on every refusal, profile
/// null, the consequence named per rung.
#[tokio::test]
async fn task_new_guard_ladder_refusals_do_zero_work() {
    let server = wiremock::MockServer::start().await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());

    // Mutating type without --yes: exit 2, the TYPE names the
    // consequence.
    let out = ign(
        &config,
        &server.uri(),
        &["eam", "task", "new", "r1", "eam_restart", "--compact"],
    );
    assert_eq!(out.status.code(), Some(2));
    let body: serde_json::Value =
        serde_json::from_str(&stderr_envelope(&out)).expect("envelope parses");
    assert_eq!(body["profile"], serde_json::Value::Null, "pre-resolution");
    assert_eq!(
        body["error"]["code"],
        serde_json::Value::String("confirmation_required".into())
    );
    let message = body["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("eam_restart") && message.contains("mutates"),
        "the type + consequence are named: {message}"
    );

    // Non-OnDemand schedule without --yes: exit 2, the SCHEDULE
    // names the consequence (even for eam_backup).
    let out = ign(
        &config,
        &server.uri(),
        &[
            "eam",
            "task",
            "new",
            "s1",
            "eam_backup",
            "--schedule-mode",
            "immediate",
            "--compact",
        ],
    );
    assert_eq!(out.status.code(), Some(2));
    let body: serde_json::Value =
        serde_json::from_str(&stderr_envelope(&out)).expect("envelope parses");
    let message = body["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("Immediate") && message.contains("autonomous"),
        "the schedule rung is named: {message}"
    );

    // The refused trio: exit 6 eam_task_type_refused — profile null,
    // ZERO requests (never reaches a client).
    for refused in [
        "eam_restoreBackup",
        "eam_installModules",
        "eam_remoteUpgrade",
    ] {
        let out = ign(
            &config,
            &server.uri(),
            &["eam", "task", "new", "d1", refused, "--compact"],
        );
        assert_eq!(out.status.code(), Some(6));
        let body: serde_json::Value =
            serde_json::from_str(&stderr_envelope(&out)).expect("envelope parses");
        assert_eq!(body["profile"], serde_json::Value::Null);
        assert_eq!(
            body["error"]["code"],
            serde_json::Value::String("eam_task_type_refused".into())
        );
        assert!(
            body["error"]["message"]
                .as_str()
                .expect("message")
                .contains("EXT-03"),
            "the v2 scope pointer rides the refusal"
        );
    }

    // Force without --yes: exit 2, dispatch consequence named.
    let out = ign(
        &config,
        &server.uri(),
        &["eam", "task", "force", "t1", "--compact"],
    );
    assert_eq!(out.status.code(), Some(2));
    let body: serde_json::Value =
        serde_json::from_str(&stderr_envelope(&out)).expect("envelope parses");
    let message = body["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("eam task force") && message.contains("NOW"),
        "the dispatch consequence is named: {message}"
    );

    // ZERO network on every refusal above.
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty(),
        "guard-ladder refusals do no network work"
    );
}

/// `--setting` + `--definition` conflict: clap's own usage error
/// (exit 2) — the mutually-exclusive forms refuse at parse time.
#[test]
fn task_new_setting_definition_conflict_is_a_usage_error() {
    let (_config_dir, config) = isolated_config();
    let out = ign(
        &config,
        "http://127.0.0.1:1",
        &[
            "eam",
            "task",
            "new",
            "t",
            "eam_backup",
            "--setting",
            "k=1",
            "--definition",
            "def.json",
        ],
    );
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("cannot be used with"),
        "clap's conflict message leads: {stderr}"
    );
}

/// The unguarded create SUCCESS golden: eam_backup + OnDemand needs
/// no --yes — the array POST rides, data carries the composed
/// definition verbatim.
#[tokio::test]
async fn task_new_backup_ondemand_fires_unguarded() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/data/api/v1/resources/com.inductiveautomation.eam/eam-tasks",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());

    let out = ign(
        &config,
        &server.uri(),
        &[
            "eam",
            "task",
            "new",
            "nightly-backup",
            "eam_backup",
            "--target",
            "gw-a",
            "--setting",
            "concurrentBackups=2",
            "--setting",
            "forceBackups=true",
            "--compact",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[
            r#"{"ok":true,"profile":"dev","data":{"name":"nightly-backup","task_type":"eam_backup","schedule_mode":"OnDemand","definition":{"config":{"profile":{"scheduleMode":"OnDemand","type":"eam_backup"},"settings":{"concurrentBackups":2,"forceBackups":true,"targetGateways":["gw-a"],"targetGroups":[]}},"name":"nightly-backup"}}}"#
        ]],
    );
    // The typed settings rode the wire under config.SETTINGS (the
    // live 8.3.3 profile/settings split — never in profile).
    let requests = server.received_requests().await.unwrap_or_default();
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).expect("body parses");
    assert_eq!(body[0]["config"]["settings"]["concurrentBackups"], 2);
    assert_eq!(body[0]["config"]["settings"]["forceBackups"], true);
    assert_eq!(
        body[0]["config"]["settings"]["targetGateways"],
        serde_json::json!(["gw-a"])
    );
    assert!(
        body[0]["config"]["profile"]
            .get("concurrentBackups")
            .is_none(),
        "profile carries no settings keys"
    );
}

/// The ZERO-TARGET default pin (07-05 gap 3): no `--target` composes
/// `targetGateways: ["_controller"]` — the live-captured zero-config
/// default on a controller-mode gateway.
#[tokio::test]
async fn task_new_backup_no_target_defaults_to_controller() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/data/api/v1/resources/com.inductiveautomation.eam/eam-tasks",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());

    let out = ign(
        &config,
        &server.uri(),
        &["eam", "task", "new", "t2", "eam_backup", "--compact"],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let requests = server.received_requests().await.unwrap_or_default();
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).expect("body parses");
    assert_eq!(
        body[0]["config"]["settings"]["targetGateways"],
        serde_json::json!(["_controller"]),
        "zero --target defaults to the controller itself"
    );
    assert_eq!(
        body[0]["config"]["settings"]["targetGroups"],
        serde_json::json!([])
    );
}

/// THE 422 classification contract (07-05 gap 3): a config-resource
/// create answered 422 with the live body
/// `{"messages":["Settings cannot be null"],"fieldMessages":[]}`
/// surfaces as exit-2 `invalid_input` carrying the gateway's own
/// message and naming the endpoint — NEVER `internal_error`.
#[tokio::test]
async fn task_new_422_classifies_invalid_input() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/data/api/v1/resources/com.inductiveautomation.eam/eam-tasks",
        ))
        .respond_with(wiremock::ResponseTemplate::new(422).set_body_raw(
            br#"{"messages":["Settings cannot be null"],"fieldMessages":[]}"#.to_vec(),
            "application/json",
        ))
        .expect(1)
        .mount(&server)
        .await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());

    let out = ign(
        &config,
        &server.uri(),
        &["eam", "task", "new", "t-422", "eam_backup", "--compact"],
    );
    assert_eq!(
        out.status.code(),
        Some(2),
        "invalid_input, not internal_error"
    );
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    let body: serde_json::Value =
        serde_json::from_str(&stderr_envelope(&out)).expect("error envelope parses");
    assert_eq!(
        body["error"]["code"],
        serde_json::Value::String("invalid_input".into())
    );
    let message = body["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("Settings cannot be null"),
        "the gateway's own message rides verbatim: {message}"
    );
    assert!(
        message.contains("/data/api/v1/resources/com.inductiveautomation.eam/eam-tasks"),
        "the endpoint is named: {message}"
    );
}

/// The force SUCCESS golden: --yes passes the guard, the 3-request
/// sequence rides, data carries owner + dispatched + the honest
/// history entry.
#[tokio::test]
async fn task_force_success_golden() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/resources/find/com.inductiveautomation.eam/eam-tasks/nightly%2Dbackup",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "name": "nightly-backup",
                "config": {"profile": {"type": "eam_backup", "scheduleMode": "OnDemand"}},
                "scheduledTaskState": {"currentState": "IDLE", "details": {"owner": "eam"}}
            })),
        )
        .expect(1..)
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/data/eam/api/v1/eam-tasks/force/eam/nightly-backup",
        ))
        .respond_with(wiremock::ResponseTemplate::new(204))
        .expect(1..)
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/eam/api/v1/eam-tasks/history"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
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
        })))
        .expect(1..)
        .mount(&server)
        .await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());

    let out = ign(
        &config,
        &server.uri(),
        &[
            "eam",
            "task",
            "force",
            "nightly-backup",
            "--yes",
            "--compact",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"task":"nightly-backup","owner":"eam","dispatched":true,"history":{"taskId":"e5f7ade4-2da3-421e-9639-5cb594193769","taskName":"nightly-backup (forced)","taskStart":1787930000000,"taskEnd":1787930009000,"target":"_controller","level":"Failed","detail":"Gateway network for agent '_controller' is currently not connected","taskType":"eam_backup"}}}"#]],
    );

    let out = ign(
        &config,
        &server.uri(),
        &["eam", "task", "force", "nightly-backup", "--yes"],
    );
    assert_eq!(out.status.code(), Some(0));
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
dispatched nightly-backup (owner eam) — run outcomes:
  [Failed] Gateway network for agent '_controller' is currently not connected"#]],
    );
}
