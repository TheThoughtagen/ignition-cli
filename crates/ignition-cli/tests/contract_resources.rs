//! Golden-file contract tests for `ign resource` (05-02 re-point) —
//! the surgical edit loop now rides project-export ZIP surgery: the
//! fixtures speak the export GET + import(overwrite) POST wire shape
//! against the BUILT binary, harness inherited from
//! `contract_projects.rs` (03-01): isolated `IGNITION_CLI_CONFIG`,
//! `stdout_for_golden`, programmatic envelopes where the `endpoint`
//! embeds the random mock URI.
//!
//! THE crown pins:
//! - `resource put`/`delete` WITHOUT `--yes` exit 2 with the
//!   `confirmation_required` envelope and profile NULL — the guard
//!   fires BEFORE any resolution (no mock even exists) and the
//!   MESSAGE names the consequence (the op overwrite-imports the
//!   whole project);
//! - a BINARY resource get refuses with exit 6 `resource_binary`
//!   (now sniffed from the zip MEMBER bytes — never corrupted
//!   through the JSON loop, Pitfall 7) and a binary put input
//!   refuses BEFORE any network I/O;
//! - put/delete success runs the export → import(overwrite)
//!   sequence, the import body round-tripped through the surgery
//!   helpers for member-level honesty;
//! - get renders PRETTY JSON in human mode (the surgical edit
//!   loop's round-trip form).

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use ignition_core::client::resources::{read_member, remove_member, replace_member};
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

/// Spawn `ign` reading `stdin` fully (the `--file -` put path).
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

// ---- Fixture zips (the export wire shape the re-point rides) ----

/// Build a small export zip: `project.json` + one member per pair, in
/// order (the same zip crate the surgery rides — honest fixtures).
fn fixture_zip(members: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write as _;
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    writer
        .start_file("project.json", options)
        .expect("project.json starts");
    writer
        .write_all(br#"{"title":"T","enabled":true}"#)
        .expect("project.json writes");
    for (name, bytes) in members {
        writer.start_file(*name, options).expect("member starts");
        writer.write_all(bytes).expect("member writes");
    }
    writer.finish().expect("zip finalizes").into_inner()
}

/// Mount the export GET (200 + zip body) against the mock gateway.
async fn mount_export(server: &wiremock::MockServer, project: &str, zip: Vec<u8>) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(format!(
            "/data/api/v1/projects/export/{project}"
        )))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_raw(zip, "application/zip"))
        .expect(1..)
        .mount(server)
        .await;
}

/// Mount the import POST (`overwrite=true`, `application/zip`),
/// returning the scoped guard for recorded-request assertions.
async fn mount_import(server: &wiremock::MockServer, project: &str) -> wiremock::MockGuard {
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(format!(
            "/data/api/v1/projects/import/{project}"
        )))
        .and(wiremock::matchers::query_param("overwrite", "true"))
        .and(wiremock::matchers::header(
            "content-type",
            "application/zip",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"success": true})),
        )
        .expect(1..)
        .mount_as_scoped(server)
        .await
}

/// Mount the DENIAL import POST (05-07, UAT Gap 1's wire truth): the
/// gateway answers HTTP 200 with `{success:false, problem}` — the
/// live-witnessed shape (an append-member overwrite-import on 8.3.3
/// answers exactly this while landing NOTHING). Before the seam, the
/// CLI parsed this opaquely and reported ok:true.
async fn mount_import_denied(server: &wiremock::MockServer, project: &str) -> wiremock::MockGuard {
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(format!(
            "/data/api/v1/projects/import/{project}"
        )))
        .and(wiremock::matchers::query_param("overwrite", "true"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": false,
                "problem": "resource already exists: ResourceId{resourcePath=com.example, collectionName=views}"
            })),
        )
        .expect(1..)
        .mount_as_scoped(server)
        .await
}

/// The list fixture zip: a view file and a script — the two families
/// the surgical loop edits most (user-facing paths identical to the
/// Phase-3 goldens: the members live under `<collection>/resources/`).
fn list_fixture_zip() -> Vec<u8> {
    fixture_zip(&[
        (
            "com.example/resources/views/Dashboard",
            br#"{"scope":"A"}"#.as_slice(),
        ),
        (
            "ignition/resources/script-python/e2e/scratch",
            b"print('scratch')".as_slice(),
        ),
    ])
}

/// `ign resource list` goldens in all three modes: one path per line
/// (human), the pretty envelope, and compact — surgery-sourced items
/// carry exactly the typed `path` (the member map IS the truth).
#[tokio::test]
async fn resource_list_render_modes_golden() {
    let server = wiremock::MockServer::start().await;
    mount_export(&server, "PlantFloor", list_fixture_zip()).await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    // Pretty JSON: one `path`-only item per member (zip order).
    let out = ign(
        &config,
        &server.uri(),
        &["resource", "list", "PlantFloor", "--json"],
    );
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
    "resources": [
      {
        "path": "com.example/views/Dashboard"
      },
      {
        "path": "ignition/script-python/e2e/scratch"
      }
    ]
  }
}
"#]],
    );

    // Human: the surgical loop's inventory — one path per line.
    let out = ign(&config, &server.uri(), &["resource", "list", "PlantFloor"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
com.example/views/Dashboard
ignition/script-python/e2e/scratch
"#]],
    );

    // Compact: one line, same shape.
    let out = ign(
        &config,
        &server.uri(),
        &["resource", "list", "PlantFloor", "--compact"],
    );
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"resources":[{"path":"com.example/views/Dashboard"},{"path":"ignition/script-python/e2e/scratch"}]}}"#]],
    );
}

/// `resource get` on a JSON resource: PRETTY JSON in human mode (the
/// round-trip form — redirect to a file, edit, put back) and the
/// `{project, path, content_kind, content}` agent shape in compact.
#[tokio::test]
async fn resource_get_json_pretty_golden() {
    let server = wiremock::MockServer::start().await;
    mount_export(
        &server,
        "p",
        fixture_zip(&[(
            "ignition/resources/script-python/e2e/scratch",
            br#"{"scope":"G","code":"print('hi')"}"#.as_slice(),
        )]),
    )
    .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(
        &config,
        &server.uri(),
        &["resource", "get", "p", "ignition/script-python/e2e/scratch"],
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
{
  "code": "print('hi')",
  "scope": "G"
}
"#]],
    );

    let out = ign(
        &config,
        &server.uri(),
        &[
            "resource",
            "get",
            "p",
            "ignition/script-python/e2e/scratch",
            "--compact",
        ],
    );
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"project":"p","path":"ignition/script-python/e2e/scratch","content_kind":"json","content":{"code":"print('hi')","scope":"G"}}}"#]],
    );
}

/// `resource get` on a TEXT resource: the raw text in human mode and
/// the text-as-JSON-string agent shape.
#[tokio::test]
async fn resource_get_text_raw_golden() {
    let server = wiremock::MockServer::start().await;
    mount_export(
        &server,
        "p",
        fixture_zip(&[("notes/resources/readme", b"just a text payload".as_slice())]),
    )
    .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(
        &config,
        &server.uri(),
        &["resource", "get", "p", "notes/readme"],
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
just a text payload
"#]],
    );

    let out = ign(
        &config,
        &server.uri(),
        &["resource", "get", "p", "notes/readme", "--compact"],
    );
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"project":"p","path":"notes/readme","content_kind":"text","content":"just a text payload"}}"#]],
    );
}

/// THE binary-refusal golden: a `data.bin`-class MEMBER get exits 6
/// `resource_binary` with the export/import hint — the content is
/// NEVER corrupted through the JSON loop (Pitfall 7), now sniffed
/// from the zip member bytes.
#[tokio::test]
async fn resource_get_binary_refuses_exit_6_golden() {
    let server = wiremock::MockServer::start().await;
    mount_export(
        &server,
        "p",
        fixture_zip(&[(
            "com.x/resources/perms/data.bin",
            [0x00, 0x01, 0x02, 0xFF, 0xFE].as_slice(),
        )]),
    )
    .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(
        &config,
        &server.uri(),
        &["resource", "get", "p", "com.x/perms/data.bin", "--compact"],
    );
    assert_eq!(out.status.code(), Some(6), "target-state class");
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stderr_envelope(&out),
        snapbox::str![[r#"
{"ok":false,"profile":"dev","error":{"code":"resource_binary","message":"resource /"com.x/perms/data.bin/" has binary content — not editable via the resource loop","endpoint":null,"hint":"resource content is binary — use `ign project export`/`import` for data.bin-class resources"}}

"#]],
    );
}

/// THE new crown pin (05-02): `resource put` WITHOUT `--yes` exits 2
/// with the `confirmation_required` envelope and profile NULL — the
/// operation MESSAGE names the consequence (the put re-imports the
/// whole project, replacing concurrent Designer edits). Fully static
/// content: the guard fires before any resolution — no mock, no
/// network, the config points at a dead URL and it does not matter.
#[tokio::test]
async fn resource_put_without_yes_exits_2_golden() {
    let body = br#"{"scope":"G","code":"print('hi')"}"#;
    let file_dir = tempfile::tempdir().expect("filedir");
    let file_path = file_dir.path().join("scratch.json");
    std::fs::write(&file_path, body).expect("write fixture");

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(
        &config,
        "http://127.0.0.1:1",
        &[
            "resource",
            "put",
            "p",
            "ignition/script-python/e2e/scratch",
            "--file",
            file_path.to_str().unwrap(),
            "--compact",
        ],
    );
    assert_eq!(out.status.code(), Some(2), "usage class, zero network");
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stderr_envelope(&out),
        snapbox::str![[r#"
{"ok":false,"profile":null,"error":{"code":"confirmation_required","message":"resource put (re-imports the project; concurrent Designer edits are replaced) is destructive; rerun with --yes to confirm","endpoint":null,"hint":"this operation is destructive; re-run with --yes or set IGNITION_YES=1"}}

"#]],
    );

    // Human mode: the message + hint on stderr.
    let out = ign(
        &config,
        "http://127.0.0.1:1",
        &[
            "resource",
            "put",
            "p",
            "some/path",
            "--file",
            file_path.to_str().unwrap(),
        ],
    );
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("re-imports the project"),
        "human message names the consequence: {stderr}"
    );
    assert!(stderr.contains("--yes"), "human hint names --yes: {stderr}");
}

/// `resource put --file F --yes` with JSON content: the export →
/// import(overwrite) sequence fires, the import body carries the NEW
/// member content (member-level honesty via the surgery helpers),
/// and the human line names the path + kind.
#[tokio::test]
async fn resource_put_from_file_json_golden() {
    let body = br#"{"scope":"G","code":"print('hi')"}"#.to_vec();
    let server = wiremock::MockServer::start().await;
    mount_export(
        &server,
        "p",
        fixture_zip(&[(
            "ignition/resources/script-python/e2e/scratch",
            br#"{"scope":"G","code":"print('old')"}"#.as_slice(),
        )]),
    )
    .await;
    let import = mount_import(&server, "p").await;

    let file_dir = tempfile::tempdir().expect("filedir");
    let file_path = file_dir.path().join("scratch.json");
    std::fs::write(&file_path, &body).expect("write fixture");

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(
        &config,
        &server.uri(),
        &[
            "resource",
            "put",
            "p",
            "ignition/script-python/e2e/scratch",
            "--file",
            file_path.to_str().unwrap(),
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
put ignition/script-python/e2e/scratch (json)
"#]],
    );

    let out = ign(
        &config,
        &server.uri(),
        &[
            "resource",
            "put",
            "p",
            "ignition/script-python/e2e/scratch",
            "--file",
            file_path.to_str().unwrap(),
            "--yes",
            "--compact",
        ],
    );
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"project":"p","path":"ignition/script-python/e2e/scratch","content_kind":"json"}}"#]],
    );

    // Member-level honesty on the import body (both invocations).
    let requests = import.received_requests().await;
    assert_eq!(requests.len(), 2, "one import per invocation");
    for request in &requests {
        assert_eq!(
            request.url.path(),
            "/data/api/v1/projects/import/p",
            "the import rode the surgery transport"
        );
        assert_eq!(
            read_member(&request.body, "ignition/script-python/e2e/scratch")
                .expect("surgical body re-reads"),
            body,
            "the import body carries the NEW member content"
        );
    }
}

/// `resource put --file - --yes` from stdin with NON-JSON text: the
/// import body carries the piped bytes as the member's content (the
/// sniffer's text arm labels the result; the transport is always
/// `application/zip` now — the surgery IS the wire).
#[tokio::test]
async fn resource_put_stdin_text_golden() {
    let server = wiremock::MockServer::start().await;
    mount_export(&server, "p", fixture_zip(&[])).await;
    let import = mount_import(&server, "p").await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign_stdin(
        &config,
        &server.uri(),
        &[
            "resource",
            "put",
            "p",
            "notes/readme",
            "--file",
            "-",
            "--yes",
            "--compact",
        ],
        b"piped text content",
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"project":"p","path":"notes/readme","content_kind":"text"}}"#]],
    );

    let requests = import.received_requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0]
            .headers
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("application/zip"),
        "the surgery import declares the zip content type"
    );
    assert_eq!(
        read_member(&requests[0].body, "notes/readme").expect("appended member reads"),
        b"piped text content".to_vec(),
        "the piped text rides the surgical member (upsert appended it)"
    );
}

/// A BINARY put input refuses with exit 6 `resource_binary` BEFORE
/// any network I/O — the config points at a DEAD URL and no mock
/// exists; if any HTTP had been attempted the command would exit 4
/// (or 5). `--yes` is present so the refusal provably comes from the
/// SNIFFER, not the guard.
#[tokio::test]
async fn resource_put_binary_input_refuses_before_network_golden() {
    let bad_dir = tempfile::tempdir().expect("bad dir");
    let bad_path = bad_dir.path().join("blob.bin");
    std::fs::write(&bad_path, [0x00, 0x50, 0x4B, 0x03, 0x04]).expect("write blob");

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");
    let out = ign(
        &config,
        "http://127.0.0.1:1",
        &[
            "resource",
            "put",
            "p",
            "com.x/perms/data.bin",
            "--file",
            bad_path.to_str().unwrap(),
            "--yes",
            "--compact",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(6),
        "target-state class, zero network"
    );
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stderr_envelope(&out),
        snapbox::str![[r#"
{"ok":false,"profile":"dev","error":{"code":"resource_binary","message":"resource /"com.x/perms/data.bin/" has binary content — not editable via the resource loop","endpoint":null,"hint":"resource content is binary — use `ign project export`/`import` for data.bin-class resources"}}

"#]],
    );
}

/// A nonexistent `--file` on put exits 2 `invalid_input` — the
/// caller's byte source, usage class, zero network (the input read
/// precedes the guard; a bad byte source fails before anything else).
#[tokio::test]
async fn resource_put_missing_file_exits_2_golden() {
    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");
    let out = ign(
        &config,
        "http://127.0.0.1:1",
        &[
            "resource",
            "put",
            "p",
            "some/path",
            "--file",
            "/nonexistent/scratch.json",
            "--compact",
        ],
    );
    assert_eq!(out.status.code(), Some(2), "usage class, zero network");
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    let body: Value = serde_json::from_str(&stderr_envelope(&out))
        .unwrap_or_else(|err| panic!("stderr envelope parses: {err}"));
    assert_eq!(body["error"]["code"], Value::String("invalid_input".into()));
    let hint = body["error"]["hint"].as_str().unwrap_or_default();
    assert!(
        hint.contains("--file") && hint.contains("stdin"),
        "hint names the fix: {hint}"
    );
}

/// THE delete-refusal golden (05-02 update): the message now names
/// the consequence too — delete re-imports the project without the
/// member. Without `--yes`: exit 2, profile NULL, fully static
/// content (the guard fires before any API construction — no mock,
/// no network, the config points at a dead URL and it does not
/// matter).
#[tokio::test]
async fn resource_delete_without_yes_exits_2_golden() {
    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(
        &config,
        "http://127.0.0.1:1",
        &["resource", "delete", "p", "some/path", "--compact"],
    );
    assert_eq!(out.status.code(), Some(2), "usage class");
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stderr_envelope(&out),
        snapbox::str![[r#"
{"ok":false,"profile":null,"error":{"code":"confirmation_required","message":"resource delete (re-imports the project; concurrent Designer edits are replaced) is destructive; rerun with --yes to confirm","endpoint":null,"hint":"this operation is destructive; re-run with --yes or set IGNITION_YES=1"}}

"#]],
    );

    // Human mode: the message + hint on stderr.
    let out = ign(
        &config,
        "http://127.0.0.1:1",
        &["resource", "delete", "p", "x"],
    );
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("resource delete"),
        "human message names the operation: {stderr}"
    );
    assert!(stderr.contains("--yes"), "human hint names --yes: {stderr}");
}

/// Delete WITH `--yes`: the export → import(overwrite) sequence
/// fires and the import body NO LONGER carries the member (asserted
/// through the same surgery helpers) — the success line golden-pins.
#[tokio::test]
async fn resource_delete_with_yes_runs_surgery_sequence() {
    let server = wiremock::MockServer::start().await;
    mount_export(
        &server,
        "p",
        fixture_zip(&[
            (
                "com.example/resources/views/My Folder/V1",
                br#"{"scope":"A"}"#.as_slice(),
            ),
            (
                "ignition/resources/script-python/e2e/scratch",
                b"print('scratch')".as_slice(),
            ),
        ]),
    )
    .await;
    let import = mount_import(&server, "p").await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(
        &config,
        &server.uri(),
        &[
            "resource",
            "delete",
            "p",
            "com.example/views/My Folder/V1",
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
deleted com.example/views/My Folder/V1
"#]],
    );

    let out = ign(
        &config,
        &server.uri(),
        &[
            "resource",
            "delete",
            "p",
            "com.example/views/My Folder/V1",
            "--yes",
            "--compact",
        ],
    );
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[
            r#"{"ok":true,"profile":"dev","data":{"deleted":"com.example/views/My Folder/V1"}}"#
        ]],
    );

    let requests = import.received_requests().await;
    assert_eq!(requests.len(), 2, "one import per invocation");
    for request in &requests {
        assert_eq!(request.url.path(), "/data/api/v1/projects/import/p");
        assert!(
            read_member(&request.body, "com.example/views/My Folder/V1").is_err(),
            "the member is GONE from the surgical body"
        );
        assert_eq!(
            read_member(&request.body, "ignition/script-python/e2e/scratch")
                .expect("neighbor survives"),
            b"print('scratch')".to_vec(),
            "the neighbor rides the surgical zip untouched"
        );
    }
}

/// THE denial-honesty golden (05-07): `resource put --yes` against a
/// gateway that answers the import with HTTP 200 `{success:false,
/// problem}` exits 6 `import_denied` — the problem text rides
/// VERBATIM in the message and the endpoint names the import request
/// (the `[..]` elides the mock's random port). Before the seam this
/// exact wire shape reported ok:true while nothing landed (UAT Gap 1).
#[tokio::test]
async fn resource_put_import_denied_exits_6_golden() {
    let body = br#"{"scope":"G","code":"print('hi')"}"#.to_vec();
    let server = wiremock::MockServer::start().await;
    mount_export(&server, "p", fixture_zip(&[])).await;
    let import = mount_import_denied(&server, "p").await;

    let file_dir = tempfile::tempdir().expect("filedir");
    let file_path = file_dir.path().join("new.json");
    std::fs::write(&file_path, &body).expect("write fixture");

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(
        &config,
        &server.uri(),
        &[
            "resource",
            "put",
            "p",
            "com.example/views/BrandNew",
            "--file",
            file_path.to_str().unwrap(),
            "--yes",
            "--compact",
        ],
    );
    assert_eq!(out.status.code(), Some(6), "target-state class");
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stderr_envelope(&out),
        snapbox::str![[r#"
{"ok":false,"profile":"dev","error":{"code":"import_denied","message":"gateway rejected the project import for /"p/": resource already exists: ResourceId{resourcePath=com.example, collectionName=views}","endpoint":"http://[..]/data/api/v1/projects/import/p","hint":"the gateway refused the import over a 200 answer — the problem text above is the gateway's own; `ign project export` of the current state is the honest baseline for hand-editing (resource already exists: ResourceId{resourcePath=com.example, collectionName=views})"}}

"#]],
    );

    // The import WAS attempted (the refusal is the gateway's answer,
    // not a CLI-side pre-check) — exactly one upload.
    let requests = import.received_requests().await;
    assert_eq!(requests.len(), 1, "the denied import reached the wire");
}

/// Same denial shape on `resource delete --yes`: the shared
/// `project_import` seam refuses identically (one seam, every
/// caller).
#[tokio::test]
async fn resource_delete_import_denied_exits_6() {
    let server = wiremock::MockServer::start().await;
    mount_export(
        &server,
        "p",
        fixture_zip(&[(
            "com.example/resources/views/Dashboard",
            br#"{"scope":"A"}"#.as_slice(),
        )]),
    )
    .await;
    let _import = mount_import_denied(&server, "p").await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");

    let out = ign(
        &config,
        &server.uri(),
        &[
            "resource",
            "delete",
            "p",
            "com.example/views/Dashboard",
            "--yes",
            "--compact",
        ],
    );
    assert_eq!(out.status.code(), Some(6), "target-state class");
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    let envelope: Value = serde_json::from_str(&stderr_envelope(&out))
        .unwrap_or_else(|err| panic!("stderr envelope parses: {err}"));
    assert_eq!(
        envelope["error"]["code"],
        Value::String("import_denied".into())
    );
    assert!(
        envelope["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("resource already exists"),
        "the gateway's problem text rides the message: {envelope}"
    );
    assert!(
        envelope["error"]["endpoint"].is_string(),
        "the endpoint names the import request: {envelope}"
    );
}


/// A nonexistent resource: the member is absent from the export zip
/// → exit 6 `not_found` (the surgery helper's error, endpoint null —
/// there was no 404 URL, there was a missing member).
#[tokio::test]
async fn resource_get_nonexistent_exits_6() {
    let server = wiremock::MockServer::start().await;
    mount_export(&server, "p", fixture_zip(&[])).await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");
    let out = ign(
        &config,
        &server.uri(),
        &["resource", "get", "p", "nope", "--compact"],
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
        Value::Null,
        "member-level not-found carries no endpoint (no 404 URL exists)"
    );
}

/// The surgery helpers are re-used verbatim by these goldens — a
/// compile-time sanity reference (read_member/replace_member/
/// remove_member are the SAME primitives the actions orchestrate).
#[test]
fn surgery_helpers_reachable_from_cli_tests() {
    let zip = fixture_zip(&[("a/resources/x", b"1".as_slice())]);
    assert_eq!(
        read_member(&zip, "a/x").expect("reads"),
        b"1".to_vec(),
        "fixture_zip + read_member agree on the member mapping"
    );
    let out = replace_member(&zip, "a/x", b"2").expect("replaces");
    assert_eq!(read_member(&out, "a/x").expect("re-reads"), b"2".to_vec());
    let out = remove_member(&out, "a/x").expect("removes");
    assert!(read_member(&out, "a/x").is_err());
}
