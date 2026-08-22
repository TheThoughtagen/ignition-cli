//! Golden-file contract tests for `ign project` (03-01, PROJ-01/02) —
//! wiremock fixtures shaped after the official 83-api schemas, harness
//! inherited from `contract_sessions.rs` (02-03): isolated
//! `IGNITION_CLI_CONFIG`, `stdout_for_golden`, programmatic envelopes
//! where the `endpoint` embeds the random mock URI.
//!
//! The crown pins:
//! - `project delete` WITHOUT `--yes` exits 2 with the
//!   `confirmation_required` envelope and profile null — the guard
//!   fires BEFORE any resolution (no mock even exists);
//! - `project delete --yes` drives the gateway-side DELETE with
//!   `confirm=true` on the wire (both guard layers, Pitfall 8);
//! - `project set --title` PUTs EXACTLY `{"title":"T"}` (the
//!   body_json matcher only fires on the exact body).

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::Value;

/// Isolated config dir + the config path inside it (file need not exist).
fn isolated_config() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.toml");
    (dir, path)
}

/// Write the one-profile dev config whose URL points at `url` and whose
/// token comes from `IGNITION_TOKEN`.
fn write_profile_config(config: &Path, url: &str) {
    std::fs::write(
        config,
        format!(
            "active = \"dev\"\n\n[profiles.dev]\nurl = \"{url}\"\nauth = {{ token_env = \"IGNITION_TOKEN\" }}\n"
        ),
    )
    .expect("write config");
}

/// Spawn `ign` with an isolated config, the mock token in the env, and args.
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

/// The projects list fixture: a child (full inheritance info +
/// passthrough extras) and a bare root project.
async fn mount_projects_list(server: &wiremock::MockServer) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/projects/list"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {
                        "name": "PlantFloor",
                        "title": "Plant Floor",
                        "description": "Line control screens",
                        "enabled": true,
                        "parent": "Base",
                        "inheritable": true,
                        "defaultDb": "MyPostgres",
                        "runtimeUsageFlags": 0
                    },
                    {
                        "name": "Base",
                        "enabled": true
                    }
                ],
                "metadata": {"total": 2, "matching": 2, "limit": -1, "offset": 0}
            })),
        )
        .expect(1..)
        .mount(server)
        .await;
}

/// `ign project list` goldens in all three modes: human table rows,
/// pretty JSON, compact — every value fixture-pinned. The agent shape
/// carries ALL six keys per project (null when absent — agents never
/// key-hunt); the passthrough extras (`defaultDb`, …) stay at the
/// client seam.
#[tokio::test]
async fn project_list_render_modes_golden() {
    let server = wiremock::MockServer::start().await;
    mount_projects_list(&server).await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    // Pretty JSON.
    let out = ign(&config, &server.uri(), &["project", "list", "--json"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
{
  "ok": true,
  "profile": "dev",
  "data": {
    "projects": [
      {
        "name": "PlantFloor",
        "title": "Plant Floor",
        "description": "Line control screens",
        "enabled": true,
        "parent": "Base",
        "inheritable": true
      },
      {
        "name": "Base",
        "title": null,
        "description": null,
        "enabled": true,
        "parent": null,
        "inheritable": null
      }
    ]
  }
}
"#]],
    );

    // Human: the webpage's project list as terminal rows.
    let out = ign(&config, &server.uri(), &["project", "list"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
PlantFloor  Plant Floor  true  Base  true
Base  -  true  -  -
"#]],
    );

    // Compact: one line, same shape.
    let out = ign(&config, &server.uri(), &["project", "list", "--compact"]);
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"projects":[{"name":"PlantFloor","title":"Plant Floor","description":"Line control screens","enabled":true,"parent":"Base","inheritable":true},{"name":"Base","title":null,"description":null,"enabled":true,"parent":null,"inheritable":null}]}}"#]],
    );
}

/// `ign project new` goldens: create fires, then the find read-back
/// fills the result — human confirmation line (with the parent) and
/// the flat JSON record.
#[tokio::test]
async fn project_new_success_golden() {
    let server = wiremock::MockServer::start().await;
    // The create mock only fires when the body carries the provided
    // fields (and never an empty-string parent — Pitfall 5).
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/data/api/v1/projects"))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "name": "child", "enabled": true, "title": "Child", "parent": "Base"
        })))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1..)
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/projects/find/child"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "name": "child", "title": "Child", "enabled": true,
                "parent": "Base", "inheritable": false
            })),
        )
        .expect(1..)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(
        &config,
        &server.uri(),
        &[
            "project", "new", "child", "--title", "Child", "--parent", "Base",
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
created child (parent Base)
"#]],
    );

    let out = ign(
        &config,
        &server.uri(),
        &[
            "project",
            "new",
            "child",
            "--title",
            "Child",
            "--parent",
            "Base",
            "--compact",
        ],
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"name":"child","title":"Child","description":null,"enabled":true,"parent":"Base","inheritable":false}}"#]],
    );
}

/// `ign project set --title`: the PUT mock's `body_json` matcher only
/// fires on the EXACT body `{"title":"T"}` — the only-provided-fields
/// proof at the binary level — and the human line names the field.
#[tokio::test]
async fn project_set_title_success_golden() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("PUT"))
        .and(wiremock::matchers::path("/data/api/v1/projects/x"))
        .and(wiremock::matchers::body_json(
            serde_json::json!({"title": "T"}),
        ))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1..)
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/projects/find/x"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "name": "x", "title": "T", "enabled": true
            })),
        )
        .expect(1..)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(
        &config,
        &server.uri(),
        &["project", "set", "x", "--title", "T"],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
set title on x
"#]],
    );

    // `set` with NO field flags is a clap usage error (the ArgGroup
    // requires at least one) — exit 2, never a wasted round-trip.
    let out = ign(&config, &server.uri(), &["project", "set", "x"]);
    assert_eq!(out.status.code(), Some(2));
}

/// Copy + rename dispatch arms (non-destructive — no --yes): human
/// confirmation lines; wire shapes pinned in the core contract suite.
#[tokio::test]
async fn project_copy_and_rename_human_lines() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/data/api/v1/projects/copy"))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1..)
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/data/api/v1/projects/rename/Old"))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1..)
        .mount(&server)
        .await;
    for name in ["dst", "New"] {
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(format!(
                "/data/api/v1/projects/find/{name}"
            )))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "name": name, "enabled": true, "parent": "Base"
                })),
            )
            .expect(1)
            .mount(&server)
            .await;
    }

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(&config, &server.uri(), &["project", "copy", "src", "dst"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
copied src → dst
"#]],
    );

    let out = ign(&config, &server.uri(), &["project", "rename", "Old", "New"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
renamed Old → New
"#]],
    );
}

/// THE crown pin: `project delete` WITHOUT `--yes` exits 2 with the
/// `confirmation_required` envelope and profile NULL — fully static
/// content (the guard fires before any API construction — no mock, no
/// network, the config points at a dead URL and it does not matter).
#[tokio::test]
async fn project_delete_without_yes_exits_2_golden() {
    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(
        &config,
        "http://127.0.0.1:1",
        &["project", "delete", "x", "--compact"],
    );
    assert_eq!(out.status.code(), Some(2), "usage class");
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stderr_envelope(&out),
        snapbox::str![[r#"
{"ok":false,"profile":null,"error":{"code":"confirmation_required","message":"project delete is destructive; rerun with --yes to confirm","endpoint":null,"hint":"this operation is destructive; re-run with --yes or set IGNITION_YES=1"}}

"#]],
    );

    // Human mode: the message + hint on stderr.
    let out = ign(&config, "http://127.0.0.1:1", &["project", "delete", "x"]);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("project delete"),
        "human message names the operation: {stderr}"
    );
    assert!(stderr.contains("--yes"), "human hint names --yes: {stderr}");
}

/// Delete WITH `--yes`: the gateway-side DELETE fires carrying
/// `confirm=true` as a QUERY param with an empty body (Pitfall 8's
/// wire half — the matcher only fires when the param matches), and
/// the success line golden-pins.
#[tokio::test]
async fn project_delete_with_yes_proves_confirm_true_on_wire() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path("/data/api/v1/projects/x"))
        .and(wiremock::matchers::query_param("confirm", "true"))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1..)
        .mount_as_scoped(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(&config, &server.uri(), &["project", "delete", "x", "--yes"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
deleted x
"#]],
    );

    let out = ign(
        &config,
        &server.uri(),
        &["project", "delete", "x", "--yes", "--compact"],
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"deleted":"x"}}"#]],
    );

    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 2, "one DELETE per invocation");
    for request in &requests {
        let query = request.url.query().expect("query present");
        assert!(
            query.contains("confirm=true"),
            "the server's own guard rode the wire: {query}"
        );
        assert!(request.body.is_empty(), "the DELETE carries no body");
    }
}

/// Deleting a nonexistent project: the gateway answers 404 → exit 6,
/// `not_found` (endpoint embeds the dynamic mock URI — programmatic
/// envelope assertions, the version_gateway_contract pattern).
#[tokio::test]
async fn project_delete_nonexistent_exits_6() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path("/data/api/v1/projects/nope"))
        .and(wiremock::matchers::query_param("confirm", "true"))
        .respond_with(
            wiremock::ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "message": "Project not found"
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");
    let out = ign(
        &config,
        &server.uri(),
        &["project", "delete", "nope", "--yes", "--compact"],
    );
    assert_eq!(out.status.code(), Some(6), "target-state class");
    assert!(out.stdout.is_empty(), "errors never touch stdout");

    let body: Value = serde_json::from_str(&stderr_envelope(&out))
        .unwrap_or_else(|err| panic!("stderr envelope parses: {err}"));
    assert_eq!(body["ok"], Value::Bool(false));
    assert_eq!(body["profile"], Value::String("dev".into()));
    assert_eq!(
        body["error"]["code"],
        Value::String("not_found".into()),
        "full envelope: {}",
        stderr_envelope(&out)
    );
    assert_eq!(
        body["error"]["endpoint"],
        Value::String(format!("{}/data/api/v1/projects/nope", server.uri()))
    );
}
