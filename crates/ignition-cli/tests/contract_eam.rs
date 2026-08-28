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
                    "taskId": 2,
                    "taskName": "nightly-backup (forced)",
                    "taskStart": 1787930000000_i64,
                    "taskEnd": 1787930009000_i64,
                    "target": "_controller",
                    "level": "Failed",
                    "detail": "Gateway network for agent '_controller' is currently not connected",
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
            r#"{"ok":true,"profile":"dev","data":{"items":[{"taskId":2,"taskName":"nightly-backup (forced)","taskStart":1787930000000,"taskEnd":1787930009000,"target":"_controller","level":"Failed","detail":"Gateway network for agent '_controller' is currently not connected","taskType":"eam_backup"},{"taskId":1,"taskName":"nightly-backup","taskStart":1787920000000,"taskEnd":1787920005000,"target":"_controller","level":"Success","detail":null,"taskType":"eam_backup"}],"count":2}}"#
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
