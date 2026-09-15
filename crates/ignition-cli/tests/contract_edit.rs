//! Contract tests for `ign edit` (13-08) — the binary-level suite
//! over the REAL spawned binary with scripted-`$EDITOR` shims (the
//! 13-05 real-process harness heritage: `#!/bin/sh` doubles spawned
//! through the PRODUCTION arg-vector site; only the editor's identity
//! is a test double) + wiremock-served REAL project exports (the
//! contract_workspace.rs harness: isolated `IGNITION_CLI_CONFIG`,
//! snapbox goldens, scope-verified traffic pins).
//!
//! THE OutOfBand stdout contract (planner-locked, byte-scan pinned):
//! `ign edit` writes ZERO bytes to stdout in every mode — the child
//! editor owns the terminal; ALL prose (no-op/pushed summaries and
//! refusals) renders to stderr. The purity pins are ASSERT-based
//! (deliberately NOT snapbox goldens — `SNAPSHOTS=overwrite` cannot
//! sanitize a leaked byte) and run under MAXIMUM diagnostics:
//! `IGNITION_LOG=trace` + an unknown-key config + `--verbose`.
//!
//! Refusal shapes pinned here ride the standard error envelope with
//! 13-05's stable prefixes verbatim: `no $EDITOR set` (pre-resolve —
//! null profile, zero requests), the member-list naming refusal, and
//! the one-gate guard refusal whose message IS the staged diff
//! summary.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use assert_cmd::Command;

// ---- Harness (the contract_workspace.rs pattern) ---------------------------

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

/// Spawn-ready `ign` command with an isolated config, the mock token,
/// and the ambient IGNITION_* + EDITOR knobs stripped (a developer
/// shell with `EDITOR=nvim` exported would otherwise spawn an
/// INTERACTIVE editor inside the test — the hang this strip prevents
/// — or change the render mode / guard outcomes). Tests add `EDITOR`,
/// args, and diagnostics noise per case.
fn ign_cmd(config: &Path, url: &str) -> Command {
    let mut command = Command::cargo_bin("ign").expect("binary 'ign' not found");
    command
        .env("IGNITION_CLI_CONFIG", config)
        .env("IGNITION_TOKEN", "mock:name-key")
        .env("IGNITION_URL", url)
        .env_remove("IGNITION_PROFILE")
        .env_remove("IGNITION_JSON")
        .env_remove("IGNITION_YES")
        .env_remove("VISUAL")
        .env_remove("EDITOR");
    command
}

/// A Perspective view member bearing ONE embedded python script —
/// the codec-scanned member whose corruption must fail CLOSED
/// (13-05's breaker archetype target).
const VIEW: &str = "com.example/views/Dashboard/view.json";
const VIEW_ZIP: &str = "com.example/resources/views/Dashboard/view.json";
const VIEW_JSON: &str = r#"{
  "scope": "G",
  "children": [
    {
      "type": "ia.display.label",
      "eventScripts": {
        "actionPerformed": {
          "config": {
            "script": "\tprint \u0027clicked\u0027\n\tprint \u0027done \u003c\u003e\u0026\u003d\u0027"
          }
        }
      }
    }
  ]
}"#;

/// The plain member the deterministic editors rewrite (value 42 →
/// 99): a REAL content change with a REAL diff, no codec splice.
const NESTED: &str = "com.example/views/Nested/Deep/other.json";
const NESTED_ZIP: &str = "com.example/resources/views/Nested/Deep/other.json";

fn nested_json(value: u32) -> String {
    format!(r#"{{"scope":"G","value":{value}}}"#)
}

fn fixture_zip() -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    writer
        .start_file("project.json", options)
        .expect("project.json starts");
    writer
        .write_all(br#"{"title":"T","enabled":true}"#)
        .expect("project.json writes");
    writer
        .start_file(VIEW_ZIP, options)
        .expect("view member starts");
    writer
        .write_all(VIEW_JSON.as_bytes())
        .expect("view member writes");
    writer
        .start_file(NESTED_ZIP, options)
        .expect("nested member starts");
    writer
        .write_all(nested_json(42).as_bytes())
        .expect("nested member writes");
    writer.finish().expect("zip finalizes").into_inner()
}

// ---- Scripted-$EDITOR shims (the 13-05 real-process pattern) ---------------

/// Write an executable `#!/bin/sh` editor script; `$1` inside the
/// body is the target file the pipeline passes (arg vector, path
/// last — the production shape). Returns the script path (the
/// double's argv[0]).
fn editor_script(dir: &Path, name: &str, body: &str) -> String {
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("script written");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("script chmod +x");
    }
    path.display().to_string()
}

/// The no-op editor: opens, touches nothing. The structural NoOp.
const NOOP_EDITOR: &str = "exit 0";

/// The deterministic edit: set the nested member to value 99 — Ready
/// against the pristine fixture on EVERY run (the gateway never
/// changes unless a push lands), so repeated runs stay deterministic.
const SET99_EDITOR: &str = r#"printf '{"scope":"G","value":99}' > "$1""#;

/// The JSON breaker: corrupt the codec-scanned view member → the
/// fail-closed encode refusal with the kept tree.
const BREAKER_EDITOR: &str = r#"printf '{ broken' > "$1""#;

// ---- Wire mocks ------------------------------------------------------------

/// The export GET, pinned to the EXACT expected count: a no-op edit
/// fetches once (no staleness re-check); any staged push runs fetch +
/// fresh re-check = 2. A stray GET fails verification loudly.
async fn export_mock(server: &wiremock::MockServer, expected_gets: u64) -> wiremock::MockGuard {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/projects/export/proj",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_raw(fixture_zip(), "application/zip"),
        )
        .expect(expected_gets)
        .mount_as_scoped(server)
        .await
}

/// The import POST, scoped with the expected count: a stray import
/// fails verification at scope drop — the zero-write and
/// exactly-one honesty contracts hold at the spawned-binary level.
async fn import_mock(server: &wiremock::MockServer, expected_imports: u64) -> wiremock::MockGuard {
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/data/api/v1/projects/import/proj",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"status":"imported"})),
        )
        .expect(expected_imports)
        .mount_as_scoped(server)
        .await
}

/// stderr's JSON envelope starting at the first `{` (log-tolerant parse).
fn stderr_envelope(out: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&out.stderr);
    let start = stderr.find('{').unwrap_or(0);
    stderr[start..].to_string()
}

// ---- Task 2: the OutOfBand stdout purity byte-scan --------------------------

/// THE zero-stdout pin (planner lock, byte-scan): under MAXIMUM
/// diagnostics (`IGNITION_LOG=trace` + unknown-key config warnings +
/// `--verbose`), both success verdicts write EXACTLY zero bytes to
/// stdout over the REAL spawned binary — the no-op AND the guarded
/// happy path. All prose (the no-op line, the blast-radius summary,
/// the pushed line) renders to stderr; the child editor inherits the
/// terminal. Traffic pins ride along: the no-op fetches once (no
/// staleness re-check, zero imports); the `--yes` push runs fetch +
/// re-check and EXACTLY ONE import.
#[tokio::test]
async fn edit_stdout_is_byte_pure_under_max_diagnostics() {
    // -- The no-op leg: stdout 0 bytes, the prose on stderr. --
    let (_dir, config) = isolated_config();
    let scratch = tempfile::tempdir().expect("scratch");
    let server = wiremock::MockServer::start().await;
    // Noise: unknown top-level key fires the unknown-key warning on
    // EVERY config load; trace logging floods stderr diagnostics.
    std::fs::write(
        &config,
        format!(
            "bogus_key = 1\nactive = \"dev\"\n\n[profiles.dev]\nurl = \"{}\"\nauth = {{ token_env = \"IGNITION_TOKEN\" }}\n",
            server.uri()
        ),
    )
    .expect("noisy config");
    let editor = editor_script(scratch.path(), "noop-editor", NOOP_EDITOR);
    {
        let _gets = export_mock(&server, 1).await;
        let _imports = import_mock(&server, 0).await;
        let out = ign_cmd(&config, &server.uri())
            .env("EDITOR", &editor)
            .env("IGNITION_LOG", "trace")
            .args(["edit", "proj", NESTED, "--verbose", "--verbose", "--verbose"])
            .output()
            .expect("spawn ign");
        assert!(
            out.status.success(),
            "no-op edit exits clean: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        // THE PIN: zero stdout bytes under max diagnostics.
        assert!(
            out.stdout.is_empty(),
            "edit must write ZERO stdout bytes (no-op, max diagnostics) — got: {:?}",
            String::from_utf8_lossy(&out.stdout)
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("edit: no changes — nothing pushed"),
            "the no-op prose renders on stderr: {stderr}"
        );
        // The noise really fired (the pin runs under real diagnostics).
        assert!(
            stderr.contains("bogus_key"),
            "expected the unknown-key warning on stderr (noise precondition): {stderr}"
        );
    } // scope drop: exactly 1 GET, ZERO imports fired

    // -- The happy-path leg (`--yes`): stdout 0 bytes again. --
    let (_dir, config) = isolated_config();
    let scratch = tempfile::tempdir().expect("scratch");
    let server = wiremock::MockServer::start().await;
    std::fs::write(
        &config,
        format!(
            "bogus_key = 1\nactive = \"dev\"\n\n[profiles.dev]\nurl = \"{}\"\nauth = {{ token_env = \"IGNITION_TOKEN\" }}\n",
            server.uri()
        ),
    )
    .expect("noisy config");
    let editor = editor_script(scratch.path(), "set99-editor", SET99_EDITOR);
    {
        let _gets = export_mock(&server, 2).await;
        let _imports = import_mock(&server, 1).await;
        let out = ign_cmd(&config, &server.uri())
            .env("EDITOR", &editor)
            .env("IGNITION_LOG", "trace")
            .args([
                "edit",
                "proj",
                NESTED,
                "--yes",
                "--verbose",
                "--verbose",
                "--verbose",
            ])
            .output()
            .expect("spawn ign");
        assert!(
            out.status.success(),
            "happy-path edit exits clean: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        // THE PIN, happy-path side.
        assert!(
            out.stdout.is_empty(),
            "edit must write ZERO stdout bytes (pushed, max diagnostics) — got: {:?}",
            String::from_utf8_lossy(&out.stdout)
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("edit would write 1 member(s) to proj")
                && stderr.contains("edit: pushed 1 member(s) to proj"),
            "the blast-radius summary + pushed line render on stderr: {stderr}"
        );
        assert!(
            stderr.contains("bogus_key"),
            "expected the unknown-key warning on stderr (noise precondition): {stderr}"
        );
    } // scope drop: 2 GETs, EXACTLY ONE import fired
}

// ---- Task 2: the usage-class refusals ---------------------------------------

/// The editor-env refusal resolves BEFORE the profile: `VISUAL` and
/// `EDITOR` both unset → exit 2 `invalid_input` with 13-05's stable
/// `no $EDITOR set` prefix, the envelope's profile NULL (nothing
/// resolved), and ZERO requests (the export/import mocks carry
/// expect(0) — any wire hit fails verification loudly).
#[tokio::test]
async fn edit_without_editor_refuses_pre_resolve_with_null_profile_and_zero_requests() {
    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");
    let server = wiremock::MockServer::start().await;
    {
        let _gets = export_mock(&server, 0).await;
        let _imports = import_mock(&server, 0).await;
        // Human mode: no [profile: …] header (nothing resolved) and
        // the stable prefix on stderr; stdout byte-empty.
        let out = ign_cmd(&config, &server.uri())
            .args(["edit", "proj", NESTED])
            .output()
            .expect("spawn ign");
        assert_eq!(out.status.code(), Some(2), "usage-class refusal");
        assert!(out.stdout.is_empty(), "refusals keep stdout empty");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("no $EDITOR set"),
            "13-05's stable editor-env prefix: {stderr}"
        );
        assert!(
            !stderr.contains("[profile: dev]"),
            "pre-resolve refusals echo NO profile: {stderr}"
        );

        // JSON mode: the envelope carries profile null + invalid_input.
        let out = ign_cmd(&config, &server.uri())
            .args(["edit", "proj", NESTED, "--json"])
            .output()
            .expect("spawn ign");
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty(), "refusals keep stdout empty");
        let envelope: serde_json::Value =
            serde_json::from_str(stderr_envelope(&out).trim()).expect("envelope parses");
        assert_eq!(envelope["ok"], serde_json::Value::Bool(false));
        assert_eq!(envelope["profile"], serde_json::Value::Null);
        assert_eq!(envelope["error"]["code"], "invalid_input");
        let message = envelope["error"]["message"].as_str().expect("message");
        assert!(
            message.contains("no $EDITOR set") && message.contains("code --wait"),
            "the envelope message carries the stable prefix + the IDE hint: {message}"
        );
    } // scope drop verifies: ZERO requests fired
}

/// The resource-path refusal: a path that is not a resource member
/// refuses with the candidate list (13-05's target-resolution
/// message) BEFORE the editor ever opens — pinned by a marker file
/// the editor shim would have written. Traffic: exactly the one
/// fetch GET, zero imports.
#[tokio::test]
async fn edit_unknown_resource_path_refuses_naming_valid_members_before_the_editor_opens() {
    let (_dir, config) = isolated_config();
    let scratch = tempfile::tempdir().expect("scratch");
    let server = wiremock::MockServer::start().await;
    write_profile_config(&config, &server.uri());
    let marker = scratch.path().join("editor-ran.marker");
    let editor = editor_script(
        scratch.path(),
        "marker-editor",
        &format!("touch {}", marker.display()),
    );
    {
        let _gets = export_mock(&server, 1).await;
        let _imports = import_mock(&server, 0).await;
        let out = ign_cmd(&config, &server.uri())
            .env("EDITOR", &editor)
            .args(["edit", "proj", "views/Missing/thing.json"])
            .output()
            .expect("spawn ign");
        assert_eq!(out.status.code(), Some(2), "usage-class refusal");
        assert!(out.stdout.is_empty(), "refusals keep stdout empty");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("\"views/Missing/thing.json\" is not a resource member of proj"),
            "the refusal names the bad path: {stderr}"
        );
        assert!(
            stderr.contains("valid members:") && stderr.contains(VIEW) && stderr.contains(NESTED),
            "the refusal lists the valid candidates: {stderr}"
        );
        assert!(
            !marker.exists(),
            "the editor never opened — the refusal fires BEFORE the edit"
        );
    } // scope drop: exactly 1 GET (the fetch), ZERO imports
}

/// THE ONE-GATE golden: without `--yes`, a staged edit refuses exit 2
/// `confirmation_required` and the refusal message IS the staged diff
/// summary (the blast radius — write lines under the count header,
/// the workspace-push preview genre). Traffic pin: the pipeline's
/// fetch + staleness re-check (2 GETs), ZERO imports — the guard is
/// the only thing that stopped the push.
#[tokio::test]
async fn edit_guard_refusal_golden_without_yes() {
    let (_dir, config) = isolated_config();
    let scratch = tempfile::tempdir().expect("scratch");
    let server = wiremock::MockServer::start().await;
    write_profile_config(&config, &server.uri());
    let editor = editor_script(scratch.path(), "set99-editor", SET99_EDITOR);
    {
        // TWO refusal runs follow (human golden + JSON envelope), each
        // running the pipeline's fetch + staleness re-check = 2 GETs
        // per run → 4 total. THE PIN is the ZERO imports: the guard
        // is the only thing that stopped the push.
        let _gets = export_mock(&server, 4).await;
        let _imports = import_mock(&server, 0).await;

        // Human mode: the golden — profile header, the summary AS the
        // error message, the destructive hint.
        let out = ign_cmd(&config, &server.uri())
            .env("EDITOR", &editor)
            .args(["edit", "proj", NESTED])
            .output()
            .expect("spawn ign");
        assert_eq!(out.status.code(), Some(2), "the guard refuses exit 2");
        assert!(out.stdout.is_empty(), "refusals keep stdout empty");
        let stderr = String::from_utf8_lossy(&out.stderr);
        snapbox::Assert::new().action_env("SNAPSHOTS").eq(
            stderr.trim_end(),
            snapbox::str![[r#"
[profile: dev]
error: edit would write 1 member(s) to proj
  write: com.example/views/Nested/Deep/other.json is destructive; rerun with --yes to confirm
hint: this operation is destructive; re-run with --yes or set IGNITION_YES=1
"#]],
        );

        // JSON mode: the same summary rides the error envelope's
        // message, `confirmation_required` code, resolved profile.
        let out = ign_cmd(&config, &server.uri())
            .env("EDITOR", &editor)
            .args(["edit", "proj", NESTED, "--json"])
            .output()
            .expect("spawn ign");
        assert_eq!(out.status.code(), Some(2));
        let envelope: serde_json::Value =
            serde_json::from_str(stderr_envelope(&out).trim()).expect("envelope parses");
        assert_eq!(envelope["ok"], serde_json::Value::Bool(false));
        assert_eq!(envelope["profile"], "dev");
        assert_eq!(envelope["error"]["code"], "confirmation_required");
        let message = envelope["error"]["message"].as_str().expect("message");
        assert!(
            message.contains("edit would write 1 member(s) to proj")
                && message.contains("  write: com.example/views/Nested/Deep/other.json"),
            "the message IS the staged diff summary: {message}"
        );
    } // scope drop verifies: ZERO imports fired — the guard held
}
