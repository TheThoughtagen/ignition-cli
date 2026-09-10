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

    // Force and the five lifecycle/mutation verbs (10-04) are NOT on
    // this zero-work ladder: their guards run the blast-radius
    // preview fetch (a READ) after resolution — pre-WRITE, not
    // pre-network. Their refusal shapes are pinned in the 10-04
    // goldens below.

    // The pure Tier-0 prechecks ARE zero-work: a whitespace task
    // name refuses pre-resolution (profile null) on every verb.
    for verb in ["suspend", "resume", "cancel", "modify", "delete", "force"] {
        let out = ign(
            &config,
            &server.uri(),
            &["eam", "task", verb, "  ", "--compact"],
        );
        assert_eq!(out.status.code(), Some(2));
        let body: serde_json::Value =
            serde_json::from_str(&stderr_envelope(&out)).expect("envelope parses");
        assert_eq!(body["profile"], serde_json::Value::Null, "pre-resolution");
        assert_eq!(
            body["error"]["code"],
            serde_json::Value::String("invalid_input".into())
        );
        let message = body["error"]["message"].as_str().expect("message");
        assert!(
            message.contains(&format!("eam task {verb}")),
            "the verb is named: {message}"
        );
    }

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

/// The force SUCCESS golden (10-04: force rides the two-tier guard —
/// the preview fetch composes first, then the action's own find →
/// owner → POST → history): --yes passes the preview confirmation,
/// data carries owner + dispatched + the honest history entry + the
/// ADDITIVE preview key (EAMW-04).
#[tokio::test]
async fn task_force_success_golden() {
    let server = wiremock::MockServer::start().await;
    mount_task_fixture(&server).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/data/eam/api/v1/eam-tasks/force/eam/nightly-backup",
        ))
        .respond_with(wiremock::ResponseTemplate::new(204))
        .expect(2)
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
        .expect(2)
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
        snapbox::str![[
            r#"{"ok":true,"profile":"dev","data":{"task":"nightly-backup","owner":"eam","dispatched":true,"history":{"taskId":"e5f7ade4-2da3-421e-9639-5cb594193769","taskName":"nightly-backup (forced)","taskStart":1787930000000,"taskEnd":1787930009000,"target":"_controller","level":"Failed","detail":"Gateway network for agent '_controller' is currently not connected","taskType":"eam_backup"},"preview":{"task":"nightly-backup","task_type":"eam_backup","schedule_mode":"Scheduled","state":"Scheduled","owner":"eam","config_suspended":false,"target_gateways":["gw-a","gw-b"],"pending_executions":[{"name":"nightly-backup","owner":"eam","type":"Collect Backup","execStart":null,"message":"","repeats":true,"canPause":true,"canResume":false,"canCancel":true,"taskState":"Scheduled","isForced":false,"isRunning":false,"progress":0.0}],"controller_impact":"dispatches task nightly-backup (eam_backup) now to 2 agents","verb":"force"}}}"#
        ]],
    );

    // Human mode unchanged in shape (dispatch + outcome line); the
    // preview rides JSON only (additive key).
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

/// The force CONFLICT golden (07-06 gap 4): a leftover '(forced)'
/// run occupies the slot — the gateway answers the force POST with
/// 409 + its Jetty error page (the live 8.3.3 capture, 07-UAT test
/// 7). The refusal is exit-6 `eam_task_in_flight` carrying the
/// page's own text verbatim — never internal_error. (10-04: the
/// preview fetch rides first — find + both scheduled segments —
/// then the action's composer find again.)
#[tokio::test]
async fn task_force_conflict_refusal_golden() {
    let server = wiremock::MockServer::start().await;
    // The find (owner resolution — hit twice: the guard's preview
    // fetch AND the action's composer find) + quiet scheduled
    // segments for the preview.
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/resources/find/com.inductiveautomation.eam/eam-tasks/cli%2Dresearch%2Dbackup",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "name": "cli-research-backup",
                "config": {"profile": {"type": "eam_backup", "scheduleMode": "OnDemand"}},
                "scheduledTaskState": {"currentState": "IDLE", "details": {"owner": "eam"}}
            })),
        )
        .expect(2)
        .mount(&server)
        .await;
    mount_scheduled_segments(&server, None).await;
    // The force POST answers 409 with the CAPTURED Jetty page body
    // (raw HTML — set_body_raw, never set_body_string's forced
    // text/plain; apostrophes HTML-escaped as the live 8.3.3 page
    // sends them).
    const CAPTURED_409_HTML: &str = "<html>\n<head>\n<meta http-equiv=\"Content-Type\" content=\"text/html;charset=ISO-8859-1\"/>\n<title>Error 409</title>\n</head>\n<body><h2>HTTP ERROR 409 Conflict</h2>\n<table>\n<tr><th>URI:</th><td>/data/eam/api/v1/eam-tasks/force/eam/cli-research-backup</td></tr>\n<tr><th>STATUS:</th><td>409</td></tr>\n<tr><th>MESSAGE:</th><td>Task &apos;cli-research-backup (forced)&apos; already exists! It must be completed or deleted before another task of this type can be force executed.</td></tr>\n</table>\n\n</body>\n</html>\n";
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/data/eam/api/v1/eam-tasks/force/eam/cli-research-backup",
        ))
        .respond_with(wiremock::ResponseTemplate::new(409).set_body_raw(
            CAPTURED_409_HTML.as_bytes().to_vec(),
            "text/html;charset=iso-8859-1",
        ))
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
            "force",
            "cli-research-backup",
            "--yes",
            "--compact",
        ],
    );
    assert_eq!(out.status.code(), Some(6), "target state, not internal");
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    let body: serde_json::Value =
        serde_json::from_str(&stderr_envelope(&out)).expect("error envelope parses");
    assert_eq!(
        body["profile"],
        serde_json::Value::String("dev".into()),
        "post-resolution refusal carries the resolved profile"
    );
    assert_eq!(
        body["error"]["code"],
        serde_json::Value::String("eam_task_in_flight".into())
    );
    let message = body["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("already exists"),
        "the gateway's page text rides verbatim: {message}"
    );
    assert!(
        message.contains("cli-research-backup"),
        "the task name rides the refusal: {message}"
    );
    assert!(
        message.contains("must be completed or deleted"),
        "the gateway's resolution text rides verbatim: {message}"
    );
    let hint = body["error"]["hint"].as_str().expect("hint");
    assert!(
        hint.contains("EAM console"),
        "the hint names where to resolve the leftover run: {hint}"
    );
}

// ---- 10-04: the two-tier guarded lifecycle/mutation verbs ----
//
// The guard shape at the binary: (a) pure precheck (zero network) →
// (b) resolution → (c) the blast-radius preview fetch (find + both
// scheduled segments — a bad name refuses not_found BEFORE any
// prompt) → (d) require_confirmation whose operation string IS the
// preview line → (e) the action. The refusal message IS the blast
// radius; zero writes fire without --yes.

const NIGHTLY_FIND_PATH: &str =
    "/data/api/v1/resources/find/com.inductiveautomation.eam/eam-tasks/nightly%2Dbackup";
const NIGHTLY_SUSPEND_PATH: &str = "/data/eam/api/v1/eam-tasks/suspend/nightly-backup";
const NIGHTLY_RESUME_PATH: &str = "/data/eam/api/v1/eam-tasks/resume/nightly-backup";
const NIGHTLY_CANCEL_PATH: &str = "/data/eam/api/v1/eam-tasks/cancel/nightly-backup";
const NIGHTLY_DELETE_PATH: &str = "/data/api/v1/resources/com.inductiveautomation.eam/eam-tasks/nightly%2Dbackup/sig%2Dabc123def456";
const NIGHTLY_PUT_PATH: &str = "/data/api/v1/resources/com.inductiveautomation.eam/eam-tasks";

/// The `nightly-backup` record fixture — Scheduled cadence, two
/// agent targets, the mutation signature the modify/delete verbs
/// quote, `isSuspended` as given.
fn task_record_json(suspended: bool) -> serde_json::Value {
    serde_json::json!({
        "name": "nightly-backup",
        "signature": "sig-abc123def456",
        "collection": "core",
        "enabled": true,
        "description": "nightly controller backup",
        "config": {
            "profile": {
                "type": "eam_backup",
                "scheduleMode": "Scheduled",
                "isSuspended": suspended
            },
            "settings": {"targetGateways": ["gw-a", "gw-b"], "targetGroups": []}
        },
        "scheduledTaskState": {
            "currentState": "Scheduled",
            "nextScheduled": "N/A",
            "details": {"owner": "eam"}
        }
    })
}

/// The pending-execution row fixture (the captured `Scheduled` cell,
/// 10-LIVE-CAPTURES §2 — all 13 keys).
fn scheduled_row_json(name: &str) -> serde_json::Value {
    serde_json::json!({
        "name": name,
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
    })
}

/// Mount BOTH scheduled segments: one pending (cancellable) row for
/// `pending_name` on the TRUE segment, an empty list on FALSE — the
/// captured `{items, metadata}` envelope shape (§2/§8).
async fn mount_scheduled_segments(server: &wiremock::MockServer, pending_name: Option<&str>) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/eam/api/v1/eam-tasks/scheduled/false",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [],
                "metadata": {"total": 0, "matching": 0, "limit": -1, "offset": 0}
            })),
        )
        .expect(0..)
        .mount(server)
        .await;
    let items = pending_name
        .map(|name| vec![scheduled_row_json(name)])
        .unwrap_or_default();
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/eam/api/v1/eam-tasks/scheduled/true",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": items,
                "metadata": {
                    "total": items.len(),
                    "matching": items.len(),
                    "limit": -1,
                    "offset": 0
                }
            })),
        )
        .expect(0..)
        .mount(server)
        .await;
}

/// Mount the `nightly-backup` find (the full fixture record,
/// `isSuspended` as given — one mock answering every call) plus the
/// scheduled segments (one pending row).
async fn mount_task_fixture(server: &wiremock::MockServer) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(NIGHTLY_FIND_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(task_record_json(false)))
        .expect(0..)
        .mount(server)
        .await;
    mount_scheduled_segments(server, Some("nightly-backup")).await;
}

/// A find responder answering `isSuspended: false` for the FIRST
/// `pre_calls` calls and `true` afterwards — the suspend sequence's
/// persistence proof (the read-back must DIFFER from the pre-write
/// find; a stateful responder keeps the answer order off wiremock's
/// multi-mock match ordering, the core contract suite's pattern).
fn suspended_after_find_responder(
    pre_calls: usize,
) -> impl Fn(&wiremock::Request) -> wiremock::ResponseTemplate {
    use std::sync::Mutex;
    let calls = Mutex::new(0usize);
    move |_request| {
        let mut calls = calls.lock().expect("counter locks");
        *calls += 1;
        let suspended = *calls > pre_calls;
        wiremock::ResponseTemplate::new(200).set_body_json(task_record_json(suspended))
    }
}

/// A find responder flipping `enabled` true→false after `pre_calls`
/// calls (the modify read-back's landed-write echo).
fn disabled_after_find_responder(
    pre_calls: usize,
) -> impl Fn(&wiremock::Request) -> wiremock::ResponseTemplate {
    use std::sync::Mutex;
    let calls = Mutex::new(0usize);
    let record = task_record_json(false);
    let mut landed = task_record_json(false);
    landed["enabled"] = serde_json::Value::Bool(false);
    move |_request| {
        let mut calls = calls.lock().expect("counter locks");
        *calls += 1;
        let body = if *calls > pre_calls {
            landed.clone()
        } else {
            record.clone()
        };
        wiremock::ResponseTemplate::new(200).set_body_json(body)
    }
}

/// THE blast-radius refusal golden (10-04): `suspend` without --yes
/// — exit 2, the preview line IN the refusal message (task + agents
/// + factual impact), the resolved profile riding the envelope
/// (post-resolution refusal convention), and ZERO WRITES on the wire
/// (exactly 3 reads: find + both scheduled segments).
#[tokio::test]
async fn task_suspend_refusal_carries_the_blast_radius() {
    let server = wiremock::MockServer::start().await;
    mount_task_fixture(&server).await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());

    let out = ign(
        &config,
        &server.uri(),
        &["eam", "task", "suspend", "nightly-backup", "--compact"],
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    let body: serde_json::Value =
        serde_json::from_str(&stderr_envelope(&out)).expect("error envelope parses");
    assert_eq!(
        body["profile"],
        serde_json::Value::String("dev".into()),
        "post-resolution refusal carries the resolved profile"
    );
    assert_eq!(
        body["error"]["code"],
        serde_json::Value::String("confirmation_required".into())
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        body["error"]["message"].as_str().expect("message"),
        snapbox::str![[
            r#"suspend nightly-backup: suspends task nightly-backup (eam_backup) — future scheduled dispatches to 2 agents stop until resumed targets: [gw-a, gw-b] pending: 1 is destructive; rerun with --yes to confirm"#
        ]],
    );

    // The refusal's traffic is READS ONLY — exactly the preview
    // fetch (find + both scheduled segments), zero mutations.
    let requests = server.received_requests().await.unwrap_or_default();
    assert_eq!(requests.len(), 3, "preview fetch only");
    assert!(
        requests.iter().all(|request| request.method == "GET"),
        "no mutation rides a refusal"
    );
}

/// The suspend SUCCESS golden: --yes passes the preview
/// confirmation, the action rides (find → suspend POST → find
/// read-back), the read-back proves the flag PERSISTED (Decision 1).
/// Two servers: the stateful find flips `isSuspended` after the
/// write, so the human-mode run starts from a fresh fixture (a
/// second run against the flipped state would — correctly — refuse
/// already-suspended).
#[tokio::test]
async fn task_suspend_success_golden() {
    let (_config_dir, config) = isolated_config();

    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(NIGHTLY_FIND_PATH))
        .respond_with(suspended_after_find_responder(2))
        .expect(3)
        .mount(&server)
        .await;
    mount_scheduled_segments(&server, Some("nightly-backup")).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(NIGHTLY_SUSPEND_PATH))
        .respond_with(wiremock::ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    write_profile_config(&config, &server.uri());
    let out = ign(
        &config,
        &server.uri(),
        &[
            "eam",
            "task",
            "suspend",
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
        snapbox::str![[
            r#"{"ok":true,"profile":"dev","data":{"task":"nightly-backup","action":"suspended","previous_state":"Scheduled","config_suspended":true,"pending":null,"fired":true,"reason":null}}"#
        ]],
    );

    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(NIGHTLY_FIND_PATH))
        .respond_with(suspended_after_find_responder(2))
        .expect(3)
        .mount(&server)
        .await;
    mount_scheduled_segments(&server, Some("nightly-backup")).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(NIGHTLY_SUSPEND_PATH))
        .respond_with(wiremock::ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    write_profile_config(&config, &server.uri());
    let out = ign(
        &config,
        &server.uri(),
        &["eam", "task", "suspend", "nightly-backup", "--yes"],
    );
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
suspended nightly-backup  previous-state: Scheduled
config.profile.isSuspended: true"#]],
    );
}

/// The delete pair (10-04): the refusal carries the delete preview
/// line with zero writes; the --yes success rides find → DELETE
/// (signature-keyed, `collection=core`) and reports the affected
/// names.
#[tokio::test]
async fn task_delete_refusal_and_success_goldens() {
    let server = wiremock::MockServer::start().await;
    mount_task_fixture(&server).await;
    wiremock::Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path(NIGHTLY_DELETE_PATH))
        .and(wiremock::matchers::query_param("collection", "core"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true,
                "changes": [{
                    "name": "nightly-backup",
                    "type": "com.inductiveautomation.eam/eam-tasks",
                    "collection": "core",
                    "newSignature": "sig-final-777"
                }],
                "references": []
            })),
        )
        .expect(2)
        .mount(&server)
        .await;
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());

    // Refusal: the delete preview line rides the confirmation
    // message; reads only.
    let out = ign(
        &config,
        &server.uri(),
        &["eam", "task", "delete", "nightly-backup", "--compact"],
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    let body: serde_json::Value =
        serde_json::from_str(&stderr_envelope(&out)).expect("error envelope parses");
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        body["error"]["message"].as_str().expect("message"),
        snapbox::str![[
            r#"delete nightly-backup: deletes task nightly-backup (eam_backup) permanently — dispatches to 2 agents stop targets: [gw-a, gw-b] pending: 1 is destructive; rerun with --yes to confirm"#
        ]],
    );
    let refusals = server.received_requests().await.unwrap_or_default();
    assert_eq!(refusals.len(), 3, "preview fetch only");
    assert!(refusals.iter().all(|request| request.method == "GET"));

    // Success: --yes, the signature-keyed DELETE, the affected name.
    let out = ign(
        &config,
        &server.uri(),
        &[
            "eam",
            "task",
            "delete",
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
        snapbox::str![[
            r#"{"ok":true,"profile":"dev","data":{"task":"nightly-backup","deleted":true,"changes":[{"name":"nightly-backup","type":"com.inductiveautomation.eam/eam-tasks","collection":"core","newSignature":"sig-final-777"}],"affected":["nightly-backup"]}}"#
        ]],
    );

    let out = ign(
        &config,
        &server.uri(),
        &["eam", "task", "delete", "nightly-backup", "--yes"],
    );
    assert_eq!(out.status.code(), Some(0));
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
deleted nightly-backup  affected: nightly-backup"#]],
    );
}

/// The modify pair (10-04, programmatic): the no-targeted-keys and
/// malformed-`--setting` refusals do ZERO network work pre-resolution
/// (the tags-write byte-source precedent); --enable/--disable
/// conflict is clap's own usage error; the --yes success PUTs the
/// FULL-RECORD clone (signature + settings + scheduledTaskState ride
/// — never compose-from-scratch) with ONLY the targeted key changed,
/// and the envelope carries the targeted paths + the authoritative
/// new signature.
#[tokio::test]
async fn task_modify_refusals_and_full_record_put() {
    // Refusals against a dead endpoint: zero network allowed.
    let dead = "http://127.0.0.1:1";
    let (_config_dir, config) = isolated_config();
    write_profile_config(&config, dead);

    let out = ign(
        &config,
        dead,
        &["eam", "task", "modify", "nightly-backup", "--compact"],
    );
    assert_eq!(out.status.code(), Some(2), "no targeted keys");
    let body: serde_json::Value =
        serde_json::from_str(&stderr_envelope(&out)).expect("envelope parses");
    assert_eq!(body["profile"], serde_json::Value::Null, "pre-resolution");
    assert!(
        body["error"]["message"]
            .as_str()
            .expect("message")
            .contains("no targeted keys"),
        "the change requirement is named"
    );

    let out = ign(
        &config,
        dead,
        &[
            "eam",
            "task",
            "modify",
            "nightly-backup",
            "--setting",
            "noequals",
            "--compact",
        ],
    );
    assert_eq!(out.status.code(), Some(2), "malformed K=V");
    let body: serde_json::Value =
        serde_json::from_str(&stderr_envelope(&out)).expect("envelope parses");
    assert_eq!(body["profile"], serde_json::Value::Null);
    assert!(
        body["error"]["message"]
            .as_str()
            .expect("message")
            .contains("--setting expects K=V"),
        "the K=V rule is named"
    );

    let out = ign(
        &config,
        dead,
        &[
            "eam",
            "task",
            "modify",
            "nightly-backup",
            "--enable",
            "--disable",
        ],
    );
    assert_eq!(out.status.code(), Some(2), "clap conflict");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("cannot be used with"),
        "clap's conflict message leads: {stderr}"
    );

    // Success: --disable against the wiremock.
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(NIGHTLY_FIND_PATH))
        .respond_with(disabled_after_find_responder(2))
        .expect(3)
        .mount(&server)
        .await;
    mount_scheduled_segments(&server, Some("nightly-backup")).await;
    wiremock::Mock::given(wiremock::matchers::method("PUT"))
        .and(wiremock::matchers::path(NIGHTLY_PUT_PATH))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true,
                "changes": [{
                    "name": "nightly-backup",
                    "type": "com.inductiveautomation.eam/eam-tasks",
                    "collection": "core",
                    "newSignature": "sig-after-0001"
                }]
            })),
        )
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
            "modify",
            "nightly-backup",
            "--disable",
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
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("envelope parses");
    assert_eq!(body["data"]["task"], "nightly-backup");
    assert_eq!(body["data"]["changed"], serde_json::json!(["enabled"]));
    assert_eq!(body["data"]["put_outcome"]["success"], true);
    assert_eq!(
        body["data"]["put_outcome"]["changes"][0]["newSignature"],
        "sig-after-0001"
    );
    assert_eq!(body["data"]["readback"]["enabled"], false, "the echo");

    // The PUT body: the FULL-RECORD clone — original signature,
    // settings, scheduledTaskState, unknown round-trip keys — with
    // ONLY `enabled` changed.
    let requests = server.received_requests().await.unwrap_or_default();
    let put = requests
        .iter()
        .find(|request| request.method == "PUT")
        .expect("the PUT rode");
    let sent: serde_json::Value = serde_json::from_slice(&put.body).expect("body parses");
    assert_eq!(sent[0]["signature"], "sig-abc123def456", "original");
    assert_eq!(sent[0]["enabled"], false, "the targeted key");
    assert_eq!(
        sent[0]["config"]["settings"]["targetGateways"],
        serde_json::json!(["gw-a", "gw-b"]),
        "settings ride untouched"
    );
    assert_eq!(
        sent[0]["config"]["profile"]["scheduleMode"], "Scheduled",
        "profile rides untouched"
    );
    assert_eq!(
        sent[0]["description"], "nightly controller backup",
        "unknown round-trip keys ride"
    );
    assert!(
        sent[0]["scheduledTaskState"].is_object(),
        "the healthcheck rides the clone"
    );
}

/// The resume + cancel envelopes (10-04, programmatic): resume fires
/// unconditionally (the capture-locked silent-204 idempotence) and
/// reports the read-back flag; cancel with NOTHING pending is the
/// honest no-op (`fired: false` + the reason) riding ZERO writes.
#[tokio::test]
async fn task_resume_and_cancel_envelopes() {
    // Resume.
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(NIGHTLY_FIND_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(task_record_json(false)))
        .expect(3)
        .mount(&server)
        .await;
    mount_scheduled_segments(&server, None).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(NIGHTLY_RESUME_PATH))
        .respond_with(wiremock::ResponseTemplate::new(204))
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
            "resume",
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
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("envelope parses");
    assert_eq!(body["data"]["action"], "resumed");
    assert_eq!(body["data"]["fired"], true);
    assert_eq!(body["data"]["config_suspended"], false, "the read-back");
    assert_eq!(body["data"]["reason"], serde_json::Value::Null);

    // Cancel with nothing pending: the honest no-op.
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(NIGHTLY_FIND_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(task_record_json(false)))
        .expect(2)
        .mount(&server)
        .await;
    mount_scheduled_segments(&server, None).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(NIGHTLY_CANCEL_PATH))
        .respond_with(wiremock::ResponseTemplate::new(204))
        .expect(0)
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
            "cancel",
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
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("envelope parses");
    assert_eq!(body["data"]["action"], "cancelled");
    assert_eq!(body["data"]["fired"], false);
    assert_eq!(
        body["data"]["reason"], "no pending execution",
        "the honest no-op reason"
    );
    // Zero writes: the cancel POST never rode.
    let requests = server.received_requests().await.unwrap_or_default();
    assert!(
        requests.iter().all(|request| request.method == "GET"),
        "the no-op fires nothing"
    );
}
