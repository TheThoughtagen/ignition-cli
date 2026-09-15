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
            wiremock::ResponseTemplate::new(200).set_body_raw(fixture_zip(), "application/zip"),
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

// ---- Full-loop machinery (Task 3): sequencing + temp-tree proof ------------

/// Serves the FETCH export on every even hit and the FRESH export on
/// every odd hit. Each edit run performs exactly TWO export GETs
/// (fetch, then the pipeline's staleness re-check), so even/odd
/// alternation pairs them correctly across any number of sequential
/// runs inside one test — the fetch-vs-recheck divergence the
/// staleness gate exists for.
#[derive(Clone)]
struct AlternatingResponder {
    hits: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    fetch: std::sync::Arc<Vec<u8>>,
    fresh: std::sync::Arc<Vec<u8>>,
}

impl AlternatingResponder {
    fn new(fetch: Vec<u8>, fresh: Vec<u8>) -> Self {
        use std::sync::atomic::AtomicUsize;
        Self {
            hits: std::sync::Arc::new(AtomicUsize::new(0)),
            fetch: std::sync::Arc::new(fetch),
            fresh: std::sync::Arc::new(fresh),
        }
    }
}

impl wiremock::Respond for AlternatingResponder {
    fn respond(&self, _request: &wiremock::Request) -> wiremock::ResponseTemplate {
        use std::sync::atomic::Ordering;
        let n = self.hits.fetch_add(1, Ordering::SeqCst);
        let body = if n.is_multiple_of(2) {
            &self.fetch
        } else {
            &self.fresh
        };
        wiremock::ResponseTemplate::new(200).set_body_raw(
            std::sync::Arc::unwrap_or_clone(body.clone()),
            "application/zip",
        )
    }
}

/// An `ign-edit-*` directory inside `dir` (the EditTempDir prefix),
/// when one lingers. `dir` is the spawned binary's TMPDIR — tests
/// redirect it so the scan is race-free (only THIS command's edit
/// trees can be present).
fn lingering_edit_tree(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("ign-edit-"))
        })
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
            .args([
                "edit",
                "proj",
                NESTED,
                "--verbose",
                "--verbose",
                "--verbose",
            ])
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

// ---- Task 3: the full loop — splice, staleness, fail-closed, no-op ---------

/// THE happy path at the binary level: the editor modifies ONE
/// member, `--yes` passes the ONE gate, and the recorded import
/// request carries the SPLICED bytes — re-parsed from the recorded
/// body: the edited member rides the new content, every untouched
/// member rides byte-exactly (13-03's round-trip invariant through
/// the REAL binary), and `project.json` survives. EXACTLY ONE import
/// fires; the temp edit tree is gone afterwards (the recovery path
/// only exists for failures).
#[tokio::test]
async fn edit_happy_path_pushes_the_spliced_member_exactly_once() {
    let (_dir, config) = isolated_config();
    let scratch = tempfile::tempdir().expect("scratch");
    let tmp = tempfile::tempdir().expect("tmpdir redirection");
    let server = wiremock::MockServer::start().await;
    write_profile_config(&config, &server.uri());
    let editor = editor_script(scratch.path(), "set99-editor", SET99_EDITOR);
    {
        let _gets = export_mock(&server, 2).await;
        let imports = import_mock(&server, 1).await;
        let out = ign_cmd(&config, &server.uri())
            .env("EDITOR", &editor)
            .env("TMPDIR", tmp.path())
            .args(["edit", "proj", NESTED, "--yes"])
            .output()
            .expect("spawn ign");
        assert!(
            out.status.success(),
            "the guarded push succeeds: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stdout.is_empty(), "zero stdout, always");

        // The recorded import body re-parsed: the spliced member +
        // the byte-exact untouched members + project.json.
        let requests = imports.received_requests().await;
        assert_eq!(requests.len(), 1, "EXACTLY ONE import fired");
        let body = &requests[0].body;
        let mut archive =
            zip::ZipArchive::new(std::io::Cursor::new(body.as_slice())).expect("walkable zip");
        let mut read_member = |name: &str| -> String {
            use std::io::Read as _;
            let mut member = archive
                .by_name(name)
                .expect("member present in the import zip");
            let mut bytes = Vec::new();
            member.read_to_end(&mut bytes).expect("member reads");
            String::from_utf8(bytes).expect("member is UTF-8")
        };
        assert_eq!(
            read_member(NESTED_ZIP),
            nested_json(99),
            "the edited member rides the SPLICED content"
        );
        assert_eq!(
            read_member(VIEW_ZIP),
            VIEW_JSON,
            "the untouched codec member rides byte-exactly"
        );
        assert_eq!(
            read_member("project.json"),
            r#"{"title":"T","enabled":true}"#,
            "project.json survives the surgery"
        );

        // The success prose (stderr): the blast-radius summary, then
        // the pushed line.
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("edit would write 1 member(s) to proj")
                && stderr.contains("edit: pushed 1 member(s) to proj"),
            "the success prose renders on stderr: {stderr}"
        );

        // The private edit tree is REMOVED on the success path (no
        // ign-edit-* entries linger in the redirected TMPDIR).
        assert!(
            lingering_edit_tree(tmp.path()).is_none(),
            "the temp edit tree is removed on success"
        );
    } // scope drop: 2 GETs (fetch + staleness re-check), 1 import
}

/// The staleness gate at the binary level: the gateway's export
/// DIVERGES between the fetch and the pipeline's re-check → exit 2
/// with the stable `changed on gateway since fetch` message — and it
/// fires IDENTICALLY WITH `--yes` (the 13-05 planner lock carries
/// through dispatch: forcing would clobber a concurrent Designer
/// edit). Traffic: both runs fetch + re-check (4 GETs), ZERO imports.
#[tokio::test]
async fn edit_staleness_refusal_fires_with_and_without_yes() {
    let scratch = tempfile::tempdir().expect("scratch");
    let editor = editor_script(scratch.path(), "set99-editor", SET99_EDITOR);
    let server = wiremock::MockServer::start().await;
    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    // Mount the fetch/fresh alternation: even hits serve value 42
    // (the fetch), odd hits serve 43 (the fresh export) — the target
    // member's hash drifts between the pipeline's snapshot and its
    // re-check, on every run.
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/projects/export/proj",
        ))
        .respond_with(AlternatingResponder::new(fixture_zip(), {
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
            writer.write_all(VIEW_JSON.as_bytes()).expect("writes");
            writer
                .start_file(NESTED_ZIP, options)
                .expect("nested member starts");
            writer
                .write_all(nested_json(43).as_bytes())
                .expect("writes");
            writer.finish().expect("zip finalizes").into_inner()
        }))
        .expect(4)
        .mount(&server)
        .await;
    {
        let _imports = import_mock(&server, 0).await;

        // WITHOUT --yes.
        let out = ign_cmd(&config, &server.uri())
            .env("EDITOR", &editor)
            .args(["edit", "proj", NESTED])
            .output()
            .expect("spawn ign");
        assert_eq!(out.status.code(), Some(2), "stale refusals exit 2");
        assert!(out.stdout.is_empty(), "refusals keep stdout empty");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(NESTED) && stderr.contains("changed on gateway since fetch"),
            "the stable staleness message names the member: {stderr}"
        );
        assert!(
            stderr.contains("re-run to fetch fresh"),
            "the recovery advice rides the message: {stderr}"
        );

        // WITH --yes — the refusal fires anyway (never --yes-able).
        let out = ign_cmd(&config, &server.uri())
            .env("EDITOR", &editor)
            .args(["edit", "proj", NESTED, "--yes"])
            .output()
            .expect("spawn ign");
        assert_eq!(
            out.status.code(),
            Some(2),
            "staleness refuses EVEN WITH --yes"
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("changed on gateway since fetch"),
            "the same refusal with --yes: {stderr}"
        );
    } // scope drop verifies: ZERO imports on both runs
}

/// The fail-closed recovery at the binary level: a JSON-breaking
/// editor corrupts the codec-scanned view member → exit 2 with the
/// codec-verbatim reason + the kept-tree clause, and the printed
/// `preserved at <path>` tree REALLY EXISTS on disk afterwards (the
/// user's edit survives the failed run). The test redirects TMPDIR so
/// the kept tree provably lands where the message says.
#[tokio::test]
async fn edit_fail_closed_keeps_the_tree_at_the_printed_path() {
    let (_dir, config) = isolated_config();
    let scratch = tempfile::tempdir().expect("scratch");
    let tmp = tempfile::tempdir().expect("tmpdir redirection");
    let server = wiremock::MockServer::start().await;
    write_profile_config(&config, &server.uri());
    let editor = editor_script(scratch.path(), "breaker-editor", BREAKER_EDITOR);
    {
        let _gets = export_mock(&server, 1).await;
        let _imports = import_mock(&server, 0).await;
        let out = ign_cmd(&config, &server.uri())
            .env("EDITOR", &editor)
            .env("TMPDIR", tmp.path())
            .args(["edit", "proj", VIEW])
            .output()
            .expect("spawn ign");
        assert_eq!(out.status.code(), Some(2), "fail-closed refuses exit 2");
        assert!(out.stdout.is_empty(), "refusals keep stdout empty");
        let stderr = String::from_utf8_lossy(&out.stderr);

        // The kept-path clause + the recovery advice.
        let marker = "preserved at ";
        assert!(
            stderr.contains(marker) && stderr.contains("fix the member by hand and re-run"),
            "the kept-tree recovery clause renders: {stderr}"
        );
        let at = stderr.find(marker).expect("marker present");
        let raw = &stderr[at + marker.len()..];
        let kept = PathBuf::from(raw.split(" — ").next().expect("path before the dash"));

        // The tree REALLY exists, inside the redirected TMPDIR, still
        // carrying the user's broken edit for hand-repair. The decode
        // tree lays members out at their RAW zip names (the
        // `<collection>/resources/<rest>` form member_path maps to).
        assert!(kept.exists(), "the kept edit tree exists: {kept:?}");
        assert!(
            kept.starts_with(tmp.path()),
            "the kept path is the redirected TMPDIR tree: {kept:?}"
        );
        let broken =
            std::fs::read_to_string(kept.join(VIEW_ZIP)).expect("the broken member survives");
        assert!(
            broken.contains("{ broken"),
            "the user's (broken) edit is preserved for hand-repair: {broken:?}"
        );
    } // scope drop: 1 GET (the codec fails before any staleness re-check), 0 imports
}

/// The structural no-op at the binary level: a byte-compare editor
/// (opens, touches nothing) exits 0 as a clean no-op — zero import
/// traffic (nothing to push, nothing to prompt), stdout empty, and
/// the private edit tree REMOVED (the recovery path exists only for
/// failures).
#[tokio::test]
async fn edit_noop_is_clean_and_cleans_its_temp_tree() {
    let (_dir, config) = isolated_config();
    let scratch = tempfile::tempdir().expect("scratch");
    let tmp = tempfile::tempdir().expect("tmpdir redirection");
    let server = wiremock::MockServer::start().await;
    write_profile_config(&config, &server.uri());
    let editor = editor_script(scratch.path(), "noop-editor", NOOP_EDITOR);
    {
        let _gets = export_mock(&server, 1).await;
        let _imports = import_mock(&server, 0).await;
        let out = ign_cmd(&config, &server.uri())
            .env("EDITOR", &editor)
            .env("TMPDIR", tmp.path())
            .args(["edit", "proj", NESTED])
            .output()
            .expect("spawn ign");
        assert!(
            out.status.success(),
            "the no-op exits clean: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stdout.is_empty(), "zero stdout, always");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("edit: no changes — nothing pushed"),
            "the no-op prose renders on stderr: {stderr}"
        );
        assert!(
            lingering_edit_tree(tmp.path()).is_none(),
            "the temp edit tree is removed on the no-op path"
        );
    } // scope drop: exactly 1 GET (a no-op never re-checks staleness), ZERO imports
}
