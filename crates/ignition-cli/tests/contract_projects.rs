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

/// Spawn `ign` with an isolated config, the mock token in the env, args,
/// and a working directory (export's default naming writes to the CWD).
fn ign_in(dir: &Path, config: &Path, url: &str, args: &[&str]) -> std::process::Output {
    let mut command = Command::cargo_bin("ign").expect("binary 'ign' not found");
    command
        .env("IGNITION_CLI_CONFIG", config)
        .env("IGNITION_TOKEN", "mock:name-key")
        .env("IGNITION_URL", url)
        .current_dir(dir);
    command.args(args).output().expect("spawn ign")
}

/// Spawn `ign` reading `stdin` fully (the `--file -` import path).
fn ign_stdin(config: &Path, url: &str, args: &[&str], stdin: &[u8]) -> std::process::Output {
    let mut command = Command::cargo_bin("ign").expect("binary 'ign' not found");
    command
        .env("IGNITION_CLI_CONFIG", config)
        .env("IGNITION_TOKEN", "mock:name-key")
        .env("IGNITION_URL", url);
    command
        .args(args)
        .write_stdin(stdin)
        .output()
        .expect("spawn ign")
}

/// The binary-level ZIP fixture — real magic bytes + a known payload
/// (4 + 22 = 26 bytes, pinned in the goldens).
fn zip_fixture() -> Vec<u8> {
    let mut bytes = vec![0x50, 0x4B, 0x03, 0x04];
    bytes.extend_from_slice(b"project-export-fixture");
    bytes
}

/// `ign project export` goldens (PROJ-03): the ZIP streams to the
/// CWD under the SANITIZED `Content-Disposition` name — human lines
/// (artifact + scope summary) and the compact JSON carrying
/// `{project, file, bytes, scope}` with BOTH scope arrays.
#[tokio::test]
async fn project_export_success_golden() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/projects/export/My%20Proj",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                // set_body_raw: set_body_string would force text/plain
                .set_body_raw(zip_fixture(), "application/zip")
                .insert_header(
                    "Content-Disposition",
                    "attachment; filename=\"MyProj-export.zip\"",
                ),
        )
        .expect(1..)
        .mount(&server)
        .await;

    let workdir = tempfile::tempdir().expect("workdir");
    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    // Human: artifact line + the scope summary.
    let out = ign_in(
        workdir.path(),
        &config,
        &server.uri(),
        &["project", "export", "My Proj"],
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
exported My Proj → MyProj-export.zip (26 bytes)
scope: includes views/scripts/named-queries/vision-windows/perspective-themes-styles/reporting/alarm-notification-profiles/webdev-routes/translations/sfc-charts · excludes tag-providers/tags/udts/gateway-config/database-connections/users-roles/alarm-journal/certificates
"#]],
    );
    // The artifact landed byte-for-byte under the disposition name.
    assert_eq!(
        std::fs::read(workdir.path().join("MyProj-export.zip")).expect("file written"),
        zip_fixture()
    );
    // No .part remnant survived the atomic rename.
    assert!(
        !workdir.path().join("My Proj.zip.part").exists()
            && !workdir.path().join("MyProj-export.zip.part").exists(),
        "the .part temp is gone"
    );

    // Compact: the full agent shape incl. both scope arrays.
    let out = ign_in(
        workdir.path(),
        &config,
        &server.uri(),
        &["project", "export", "My Proj", "--compact"],
    );
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"project":"My Proj","file":"MyProj-export.zip","bytes":26,"scope":{"includes":["views","scripts","named-queries","vision-windows","perspective-themes-styles","reporting","alarm-notification-profiles","webdev-routes","translations","sfc-charts"],"excludes":["tag-providers","tags","udts","gateway-config","database-connections","users-roles","alarm-journal","certificates"]}}}"#]],
    );
}

/// `project export -o FILE`: the bytes land at EXACTLY the given path
/// (no disposition renaming) and the data carries that path.
#[tokio::test]
async fn project_export_explicit_output_golden() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/projects/export/x"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_raw(zip_fixture(), "application/zip")
                .insert_header(
                    "Content-Disposition",
                    "attachment; filename=\"ignored-by-o.zip\"",
                ),
        )
        .expect(1..)
        .mount(&server)
        .await;

    let workdir = tempfile::tempdir().expect("workdir");
    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign_in(
        workdir.path(),
        &config,
        &server.uri(),
        &["project", "export", "x", "-o", "out.zip", "--compact"],
    );
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"project":"x","file":"out.zip","bytes":26,"scope":{"includes":["views","scripts","named-queries","vision-windows","perspective-themes-styles","reporting","alarm-notification-profiles","webdev-routes","translations","sfc-charts"],"excludes":["tag-providers","tags","udts","gateway-config","database-connections","users-roles","alarm-journal","certificates"]}}}"#]],
    );
    assert_eq!(
        std::fs::read(workdir.path().join("out.zip")).expect("file written"),
        zip_fixture()
    );
    assert!(
        !workdir.path().join("ignored-by-o.zip").exists(),
        "-o wins over the disposition name"
    );
}

/// THE collision golden: abort-policy import over an EXISTING project
/// exits 6 `project_exists` with the overwrite-naming hint — and the
/// import mock (expect 0) proves ZERO uploads reached the gateway.
#[tokio::test]
async fn project_import_abort_collision_exits_6_golden() {
    let server = wiremock::MockServer::start().await;
    let import_guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/data/api/v1/projects/import/x"))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(0)
        .mount_as_scoped(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/projects/find/x"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"name": "x", "enabled": true})),
        )
        .expect(1..)
        .mount(&server)
        .await;

    let zip_dir = tempfile::tempdir().expect("zipdir");
    let zip_path = zip_dir.path().join("proj.zip");
    std::fs::write(&zip_path, zip_fixture()).expect("write fixture");

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");
    let out = ign(
        &config,
        &server.uri(),
        &[
            "project",
            "import",
            "x",
            "--file",
            zip_path.to_str().unwrap(),
            "--compact",
        ],
    );
    assert_eq!(out.status.code(), Some(6), "target-state class");
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stderr_envelope(&out),
        snapbox::str![[r#"
{"ok":false,"profile":"dev","error":{"code":"project_exists","message":"project x already exists on the gateway","endpoint":null,"hint":"the default collision policy refuses to overwrite; re-run with --collision-policy overwrite to replace it — overwrite REPLACES the ENTIRE project (resources absent from the ZIP are deleted; merge is Designer-only)"}}

"#]],
    );
    assert!(
        import_guard.received_requests().await.is_empty(),
        "the refusal happened BEFORE any upload"
    );
}

/// THE guard golden: overwrite-policy import WITHOUT `--yes` exits 2
/// with the `confirmation_required` envelope and profile NULL — the
/// guard fires before any resolution (dead URL, no mocks, no matter).
#[tokio::test]
async fn project_import_overwrite_without_yes_exits_2_golden() {
    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(
        &config,
        "http://127.0.0.1:1",
        &[
            "project",
            "import",
            "x",
            "--file",
            "whatever.zip",
            "--collision-policy",
            "overwrite",
            "--compact",
        ],
    );
    assert_eq!(out.status.code(), Some(2), "usage class");
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stderr_envelope(&out),
        snapbox::str![[r#"
{"ok":false,"profile":null,"error":{"code":"confirmation_required","message":"project import --collision-policy overwrite is destructive; rerun with --yes to confirm","endpoint":null,"hint":"this operation is destructive; re-run with --yes or set IGNITION_YES=1"}}

"#]],
    );
}

/// Overwrite WITH `--yes`: the upload fires carrying
/// `overwrite=true`, the exact ZIP bytes, and
/// `Content-Type: application/zip` — and the find mock (expect 0)
/// proves overwrite performs NO pre-check (the server is the
/// authority).
#[tokio::test]
async fn project_import_overwrite_with_yes_uploads() {
    let server = wiremock::MockServer::start().await;
    let import_guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/data/api/v1/projects/import/x"))
        .and(wiremock::matchers::query_param("overwrite", "true"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"status": "imported"})),
        )
        .expect(1..)
        .mount_as_scoped(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/projects/find/x"))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;

    let zip_dir = tempfile::tempdir().expect("zipdir");
    let zip_path = zip_dir.path().join("proj.zip");
    std::fs::write(&zip_path, zip_fixture()).expect("write fixture");

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");
    let out = ign(
        &config,
        &server.uri(),
        &[
            "project",
            "import",
            "x",
            "--file",
            zip_path.to_str().unwrap(),
            "--collision-policy",
            "overwrite",
            "--yes",
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
imported x (26 bytes, policy overwrite)
"#]],
    );

    let requests = import_guard.received_requests().await;
    assert!(!requests.is_empty(), "the upload fired");
    for request in &requests {
        let query = request.url.query().expect("query present");
        assert_eq!(query, "overwrite=true", "the policy rode the wire: {query}");
        assert_eq!(request.body, zip_fixture(), "the exact ZIP bytes");
        assert_eq!(
            request
                .headers
                .get("content-type")
                .and_then(|value| value.to_str().ok()),
            Some("application/zip")
        );
    }
}

/// Stdin import (`--file -`): piped bytes ride the same guards and
/// the abort policy's find pre-check (404 = name free) precedes the
/// `overwrite=false` upload.
#[tokio::test]
async fn project_import_stdin_golden() {
    let server = wiremock::MockServer::start().await;
    let import_guard = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/data/api/v1/projects/import/piped",
        ))
        .and(wiremock::matchers::query_param("overwrite", "false"))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1)
        .mount_as_scoped(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/projects/find/piped"))
        .respond_with(
            wiremock::ResponseTemplate::new(404)
                .set_body_json(serde_json::json!({"message": "Project not found"})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");
    let out = ign_stdin(
        &config,
        &server.uri(),
        &["project", "import", "piped", "--file", "-", "--compact"],
        &zip_fixture(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"name":"piped","collision_policy":"abort","bytes":26,"scope":{"includes":["views","scripts","named-queries","vision-windows","perspective-themes-styles","reporting","alarm-notification-profiles","webdev-routes","translations","sfc-charts"],"excludes":["tag-providers","tags","udts","gateway-config","database-connections","users-roles","alarm-journal","certificates"]},"outcome":{"status":"success"}}}"#]],
    );
    let requests = import_guard.received_requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].body, zip_fixture(), "the piped bytes uploaded");
}

/// A non-ZIP file refuses with exit 2 `invalid_import_file` BEFORE
/// any network I/O — the config points at a DEAD URL and no mock
/// exists; if any HTTP had been attempted the command would exit 4.
#[tokio::test]
async fn project_import_non_zip_exits_2_golden() {
    let bad_dir = tempfile::tempdir().expect("bad dir");
    let bad_path = bad_dir.path().join("not-a-zip.txt");
    std::fs::write(&bad_path, b"definitely not a zip").expect("write junk");

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");
    let out = ign(
        &config,
        "http://127.0.0.1:1",
        &[
            "project",
            "import",
            "x",
            "--file",
            bad_path.to_str().unwrap(),
            "--compact",
        ],
    );
    assert_eq!(out.status.code(), Some(2), "usage class, zero network");
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stderr_envelope(&out),
        // NOTE: snapbox's `str!` normalizes backslashes in the ACTUAL
        // output to forward slashes (cross-platform path handling) —
        // the wire text is `PK\\x03\\x04` (JSON-escaped literal
        // backslash), which the golden spells `PK//x03//x04`.
        snapbox::str![[r#"
{"ok":false,"profile":"dev","error":{"code":"invalid_import_file","message":"invalid import file: missing ZIP magic (PK//x03//x04) — not a project export archive","endpoint":null,"hint":"import expects a project-export ZIP (PK//x03//x04 magic) of at most 512 MB — pass a file produced by `ign project export` via --file (or `-` to pipe one on stdin)"}}

"#]],
    );
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
