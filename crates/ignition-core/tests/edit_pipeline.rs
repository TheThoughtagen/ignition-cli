//! Contract tests for `ign edit`'s core pipeline (13-05): the
//! FIVE-ARCHETYPE adversarial-editor harness as REAL spawned
//! processes, plus the pipeline behavior pins (no-op by content,
//! fail-closed recovery, staleness gate).
//!
//! THE roadmap-mandated plan-level verification: editor archetypes
//! are REAL `sh` scripts written by the test, chmod +x, and spawned
//! through the PRODUCTION spawn site ([`run_editor_argv`] — arg
//! vector, advisory exit). Nothing about the process layer is
//! mocked (Pitfall 15); only the editor's identity is a test double.
//!
//! The archetypes:
//! - **blocking vim-style** — edits the file, exits 0 → `Ready`
//!   with the member in the blast radius;
//! - **`--wait` deferred-exit shim** — writes, lingers, then exits
//!   → the pipeline WAITS for the editor, and the exit stays
//!   advisory → `Ready`;
//! - **daemon-style** — exits 0 immediately while a background
//!   child writes LATE → `NoOp`: content AT ENCODE TIME decides,
//!   never the exit code, and a post-exit write cannot flip the
//!   decision;
//! - **no-change editor** — opens, touches nothing → the
//!   structural no-op: `NoOp` + `import_zip: None` (there are no
//!   bytes to push — the caller cannot push, cannot prompt; this is
//!   the spy-push proof made structural by the plan's corrected
//!   signature, where push stays OUT of core);
//! - **JSON breaker** — corrupts a script-bearing member →
//!   fail-closed via the codec's own `InvalidInput`, with the edit
//!   tree KEPT and its path in the message (recovery path).
//!
//! Staleness (the NOT-`--yes`-able gate): a wiremock responder that
//! serves a DIFFERENT export on the second GET (fetch vs re-check)
//! refuses with the stable `changed on gateway since fetch`
//! message; an identical fresh export allows `Ready`.

use std::io::Write as _;
use std::path::Path;
#[cfg(unix)] // kept_path_from-only
use std::path::PathBuf;
#[cfg(unix)] // EDITOR_ENV_LOCK-only
use std::sync::Mutex;
#[cfg(unix)] // SequenceResponder-only
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(unix)] // consumed only by the spawn-path tests
use ignition_core::actions::edit::{EditStatus, TokioEditor};
use ignition_core::actions::edit::{Editor, edit_pipeline, run_editor_argv};
use ignition_core::client::ReqwestGatewayApi;
use ignition_core::error::CoreError;

// ---- Fixtures (project-export-shaped ONLY — never tag data) ---------------

/// A Perspective view member with ONE embedded python script (the
/// Flint-escaped form) — the member whose corruption must fail
/// CLOSED (it carries a codec manifest entry, so `encode_member`
/// re-scans it).
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

/// A plain member (no scripts — copied verbatim by the codec) whose
/// `value` the sed-based editors flip 42 → 43: a REAL content change
/// with a REAL diff, no codec splice involved.
const NESTED: &str = "com.example/views/Nested/Deep/other.json";
const NESTED_ZIP: &str = "com.example/resources/views/Nested/Deep/other.json";

fn nested_json(value: u32) -> String {
    format!(r#"{{"scope":"G","value":{value}}}"#)
}

/// The small export zip: `project.json` + the two members (the same
/// zip crate the codec's own writer rides — honest fixtures).
fn fixture_zip(nested_value: u32) -> Vec<u8> {
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
        .write_all(nested_json(nested_value).as_bytes())
        .expect("nested member writes");
    writer.finish().expect("zip finalizes").into_inner()
}

// ---- Transport: a fetch-vs-recheck sequence responder ----------------------

/// Serves `first` on GET #1 (the fetch) and `rest` on every
/// subsequent GET (the staleness re-check). Interior mutability via
/// an atomic — wiremock's `Respond` needs only Send + Sync.
#[cfg(unix)] // staleness spawn tests only
#[derive(Clone)]
struct SequenceResponder {
    hits: std::sync::Arc<AtomicUsize>,
    first: std::sync::Arc<Vec<u8>>,
    rest: std::sync::Arc<Vec<u8>>,
}

#[cfg(unix)]
impl SequenceResponder {
    fn new(first: Vec<u8>, rest: Vec<u8>) -> Self {
        Self {
            hits: std::sync::Arc::new(AtomicUsize::new(0)),
            first: std::sync::Arc::new(first),
            rest: std::sync::Arc::new(rest),
        }
    }
}

#[cfg(unix)]
impl wiremock::Respond for SequenceResponder {
    fn respond(&self, _request: &wiremock::Request) -> wiremock::ResponseTemplate {
        let n = self.hits.fetch_add(1, Ordering::SeqCst);
        let body = if n == 0 { &self.first } else { &self.rest };
        wiremock::ResponseTemplate::new(200).set_body_raw(
            std::sync::Arc::unwrap_or_clone(body.clone()),
            "application/zip",
        )
    }
}

/// Mount the export GET. `expected_gets` pins the wire contract:
/// 1 for a no-op edit (fetch only), 2 when the staleness gate runs
/// (fetch + fresh re-check).
async fn mount_export(server: &wiremock::MockServer, zip: Vec<u8>, expected_gets: u64) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/projects/export/demo",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_raw(zip, "application/zip"))
        .expect(expected_gets)
        .mount(server)
        .await;
}

/// Mount the export GET with a fetch-vs-recheck SEQUENCE (staleness
/// tests): GET #1 → `first`, GET #2+ → `rest`.
#[cfg(unix)] // staleness spawn tests only
async fn mount_export_sequence(server: &wiremock::MockServer, first: Vec<u8>, rest: Vec<u8>) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/projects/export/demo",
        ))
        .respond_with(SequenceResponder::new(first, rest))
        .mount(server)
        .await;
}

// ---- The REAL-process editor double ----------------------------------------

/// An editor double that IS a real process: `argv` is
/// `<program> <flags...>` — the target path is appended LAST by the
/// shared production spawn site ([`run_editor_argv`]). The editor's
/// identity is what the test fakes; the spawn mechanics, arg-vector
/// discipline, and advisory-exit contract are the production path.
struct ScriptEditor(String);

#[async_trait::async_trait]
impl Editor for ScriptEditor {
    async fn open(&self, path: &Path) -> Result<(), CoreError> {
        let mut parts = self.0.split_whitespace();
        let program = parts.next().expect("script path is argv[0]");
        let flags: Vec<String> = parts.map(str::to_string).collect();
        run_editor_argv(program, &flags, path).await
    }
}

/// Write an executable `#!/bin/sh` editor script; `$1` inside the
/// body is the target file the pipeline passes (arg vector, path
/// last — the double's argv is `[script] + [target]`, mirroring the
/// production `<editor> <flags...> <target>` shape; the flag-split
/// behavior itself is pinned by TokioEditor's arg-vector unit
/// tests). Returns the script path (the double's argv[0]).
fn editor_script(dir: &tempfile::TempDir, name: &str, body: &str) -> String {
    let path = dir.path().join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("script written");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("script chmod +x");
    }
    path.display().to_string()
}

/// The sed edit the archetypes share: flip `"value":42` → 43 in the
/// (plain, verbatim-copied) nested member. POSIX-portable (no
/// `sed -i`), so macOS and Linux sh both pass.
#[cfg(unix)]
const SED_EDIT: &str = r#"sed 's/"value":42/"value":43/' "$1" > "$1.tmp" && mv "$1.tmp" "$1""#;

/// The kept-path clause of the fail-closed message — extracted and
/// PROVEN on disk (the tree survives the failed run).
#[cfg(unix)] // fail-closed spawn tests only
fn kept_path_from(reason: &str) -> PathBuf {
    let marker = "preserved at ";
    let at = reason.find(marker).expect("kept-path marker present");
    let raw = &reason[at + marker.len()..];
    let path = raw.split(" — ").next().expect("path before the dash");
    PathBuf::from(path)
}

// ---- Archetype (a): blocking vim-style -------------------------------------

/// A blocking editor that makes a REAL edit and exits 0 → `Ready`
/// with exactly the edited member in the blast radius (changed =
/// what a push would write) and the staged import zip present.
#[cfg(unix)] // scripted-$EDITOR shims are sh-only — Windows cmd-shim harness is the follow-up
#[tokio::test]
async fn blocking_editor_edit_yields_ready_with_blast_radius() {
    let zip = fixture_zip(42);
    let server = wiremock::MockServer::start().await;
    mount_export(&server, zip, 2).await; // Ready runs the staleness gate: fetch + fresh
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);

    let scratch = tempfile::tempdir().expect("scratch");
    let editor = ScriptEditor(editor_script(&scratch, "vim-like", SED_EDIT));

    let staged = edit_pipeline(&api, &editor, "demo", Some(NESTED))
        .await
        .expect("pipeline stages the edit");

    assert_eq!(staged.project, "demo");
    let EditStatus::Ready { changed } = staged.status else {
        panic!("expected Ready, got {:?}", staged.status);
    };
    assert_eq!(changed, vec![NESTED.to_string()], "the blast radius");
    let import_zip = staged.import_zip.expect("staged payload present");

    // The staged zip really carries the edit: the nested member's
    // bytes differ from the source (what a push would write).
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(import_zip)).expect("zip opens");
    let mut member = archive.by_name(NESTED_ZIP).expect("member present");
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut member, &mut bytes).expect("member reads");
    assert_eq!(bytes, nested_json(43).as_bytes(), "the push would write 43");
}

// ---- Archetype (b): --wait deferred-exit shim -------------------------------

/// The IDE-shim shape: write, LINGER (the process is still alive —
/// like `code --wait` holding while the user edits), then exit. The
/// pipeline WAITS for the editor process, and the eventual exit
/// stays advisory — `Ready` because the CONTENT changed.
#[cfg(unix)] // scripted-$EDITOR shims are sh-only — Windows cmd-shim harness is the follow-up
#[tokio::test]
async fn deferred_exit_editor_is_waited_and_stays_advisory() {
    let zip = fixture_zip(42);
    let server = wiremock::MockServer::start().await;
    mount_export(&server, zip, 2).await; // Ready runs the staleness gate: fetch + fresh
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);

    let scratch = tempfile::tempdir().expect("scratch");
    let editor = ScriptEditor(editor_script(
        &scratch,
        "wait-shim",
        &format!("{SED_EDIT}\nsleep 0.5"),
    ));

    let staged = edit_pipeline(&api, &editor, "demo", Some(NESTED))
        .await
        .expect("the pipeline waits out the deferred exit");
    let EditStatus::Ready { changed } = staged.status else {
        panic!("expected Ready, got {:?}", staged.status);
    };
    assert_eq!(changed, vec![NESTED.to_string()]);
}

// ---- Archetype (c): daemon-style -------------------------------------------

/// The daemon shape (Pitfall E1's worst case): the editor process
/// exits 0 IMMEDIATELY while a background child writes LATER. The
/// pipeline decides by content AT ENCODE TIME — the exit code never
/// enters the decision, and the late write (still in flight when
/// the verdict lands) cannot flip it. Pinned: `NoOp`.
#[cfg(unix)] // scripted-$EDITOR shims are sh-only — Windows cmd-shim harness is the follow-up
#[tokio::test]
async fn daemon_style_immediate_exit_decides_by_content_at_encode_time() {
    let zip = fixture_zip(42);
    let server = wiremock::MockServer::start().await;
    mount_export(&server, zip, 1).await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);

    let scratch = tempfile::tempdir().expect("scratch");
    // The child appends garbage long after the parent exited and the
    // pipeline has encoded (5s ≫ encode time); by then the edit tree
    // is dropped and the write lands nowhere.
    let editor = ScriptEditor(editor_script(
        &scratch,
        "daemon",
        r#"( sleep 5; printf 'GARBAGE' >> "$1" ) &
exit 0"#,
    ));

    let staged = edit_pipeline(&api, &editor, "demo", Some(NESTED))
        .await
        .expect("the daemon's immediate exit is fine — content decides");
    assert_eq!(
        staged.status,
        EditStatus::NoOp,
        "content at encode: unchanged"
    );
    assert!(staged.import_zip.is_none(), "no payload on a no-op");
}

/// The daemon's OTHER content direction, deterministic: the child's
/// write lands BEFORE the encode (the parent waits it out before
/// exiting), the parent still exits 0 — and the CONTENT decides:
/// garbage in a script-bearing member fails CLOSED. Exit 0 does not
/// save a broken edit; the pipeline never trusted the code.
#[cfg(unix)] // scripted-$EDITOR shims are sh-only — Windows cmd-shim harness is the follow-up
#[tokio::test]
async fn daemon_child_write_landing_before_encode_fails_closed() {
    let zip = fixture_zip(42);
    let server = wiremock::MockServer::start().await;
    mount_export(&server, zip, 1).await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);

    let scratch = tempfile::tempdir().expect("scratch");
    // Backgrounded child corrupts the SCRIPT member; the parent
    // `wait`s it out (the write is guaranteed pre-encode) and exits 0.
    let editor = ScriptEditor(editor_script(
        &scratch,
        "daemon-breaker",
        r#"( printf '{{ not json' > "$1" ) &
wait
exit 0"#,
    ));

    let err = edit_pipeline(&api, &editor, "demo", Some(VIEW))
        .await
        .expect_err("content decided: the landed garbage fails closed");
    let CoreError::InvalidInput { reason } = err else {
        panic!("expected InvalidInput, got {err:?}");
    };
    assert!(
        reason.starts_with("the edited member no longer parses as JSON"),
        "fail-closed per content, not per exit: {reason}"
    );
}

// ---- Archetype (d): no-change editor ---------------------------------------

/// An editor that opens and touches NOTHING → the structural no-op:
/// `NoOp` with `import_zip: None`. There are no bytes to push — the
/// caller cannot push and cannot prompt (the plan's spy-push proof,
/// made structural by the corrected signature that keeps push OUT
/// of core).
#[cfg(unix)] // scripted-$EDITOR shims are sh-only — Windows cmd-shim harness is the follow-up
#[tokio::test]
async fn no_change_editor_yields_structural_noop() {
    let zip = fixture_zip(42);
    let server = wiremock::MockServer::start().await;
    mount_export(&server, zip, 1).await; // no-op never re-checks
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);

    let scratch = tempfile::tempdir().expect("scratch");
    let editor = ScriptEditor(editor_script(
        &scratch,
        "no-change",
        r#"cat "$1" > /dev/null
exit 0"#,
    ));

    let staged = edit_pipeline(&api, &editor, "demo", Some(NESTED))
        .await
        .expect("clean no-op");
    assert_eq!(staged.status, EditStatus::NoOp);
    assert!(
        staged.import_zip.is_none(),
        "zero payload — nothing to push"
    );
}

// ---- Archetype (e): JSON breaker -------------------------------------------

/// An editor that CORRUPTS a script-bearing member → the re-encode
/// refuses via the codec's own `encode_member` `InvalidInput`
/// (verbatim), and the edit tree is KEPT: the error message carries
/// its path and the tree is still on disk — the recovery path.
#[cfg(unix)] // scripted-$EDITOR shims are sh-only — Windows cmd-shim harness is the follow-up
#[tokio::test]
async fn json_breaking_editor_fails_closed_and_keeps_the_tree() {
    let zip = fixture_zip(42);
    let server = wiremock::MockServer::start().await;
    mount_export(&server, zip, 1).await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);

    let scratch = tempfile::tempdir().expect("scratch");
    // Corrupt the SCRIPT member (it carries a manifest entry, so the
    // encode re-scans it) — the fail-closed path.
    let editor = ScriptEditor(editor_script(
        &scratch,
        "breaker",
        r#"printf '{{ not json' > "$1""#,
    ));

    let err = edit_pipeline(&api, &editor, "demo", Some(VIEW))
        .await
        .expect_err("a broken member refuses, fail-closed");
    let CoreError::InvalidInput { reason } = err else {
        panic!("expected InvalidInput, got {err:?}");
    };
    // The codec's own refusal, VERBATIM — not a re-worded summary.
    assert!(
        reason.starts_with("the edited member no longer parses as JSON"),
        "codec verbatim prefix: {reason}"
    );
    // The kept path rides the message — and the tree EXISTS.
    let kept = kept_path_from(&reason);
    assert!(kept.is_dir(), "the edit tree is kept: {}", kept.display());
    assert!(
        kept.file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("ign-edit-")),
        "the kept tree is the private edit dir: {}",
        kept.display()
    );
    assert!(
        kept.join("scripts-manifest.json").exists(),
        "the decoded tree (manifest intact) survived: {}",
        kept.display()
    );
    // The user's corrupting edit is in there too — recoverable.
    let corrupted = std::fs::read_to_string(kept.join(VIEW_ZIP)).is_ok();
    assert!(corrupted, "the edited member file is in the kept tree");
}

// ---- Staleness gate (NOT --yes-able) ---------------------------------------

/// The gateway moved between fetch and re-check (GET #1 = value 42,
/// GET #2 = value 43): the staged edit refuses with the stable
/// `changed on gateway since fetch` message. No flag overrides this
/// — re-run to fetch fresh (forcing it would clobber a concurrent
/// Designer edit).
#[cfg(unix)] // scripted-$EDITOR shims are sh-only — Windows cmd-shim harness is the follow-up
#[tokio::test]
async fn staleness_drift_refuses_with_stable_message() {
    let server = wiremock::MockServer::start().await;
    mount_export_sequence(&server, fixture_zip(42), fixture_zip(43)).await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);

    let scratch = tempfile::tempdir().expect("scratch");
    let editor = ScriptEditor(editor_script(&scratch, "vim-like", SED_EDIT));

    let err = edit_pipeline(&api, &editor, "demo", Some(NESTED))
        .await
        .expect_err("drift refuses");
    let CoreError::InvalidInput { reason } = err else {
        panic!("expected InvalidInput, got {err:?}");
    };
    assert!(
        reason.starts_with(&format!(
            "resource \"{NESTED}\" changed on gateway since fetch"
        )),
        "stable prefix: {reason}"
    );
    assert!(
        reason.contains("re-run to fetch fresh"),
        "the remedy: {reason}"
    );
    // NOT --yes-able: the refusal is the usage class, not a
    // confirmation gate — there is no flag shape that satisfies it.
}

/// A fresh export identical (hash-equal) to the fetch → the gate
/// passes and the edit stages `Ready`.
#[cfg(unix)] // scripted-$EDITOR shims are sh-only — Windows cmd-shim harness is the follow-up
#[tokio::test]
async fn staleness_fresh_export_allows_ready() {
    let server = wiremock::MockServer::start().await;
    mount_export_sequence(&server, fixture_zip(42), fixture_zip(42)).await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);

    let scratch = tempfile::tempdir().expect("scratch");
    let editor = ScriptEditor(editor_script(&scratch, "vim-like", SED_EDIT));

    let staged = edit_pipeline(&api, &editor, "demo", Some(NESTED))
        .await
        .expect("no drift — the gate passes");
    let EditStatus::Ready { changed } = staged.status else {
        panic!("expected Ready, got {:?}", staged.status);
    };
    assert_eq!(changed, vec![NESTED.to_string()]);
    assert!(staged.import_zip.is_some());
}

// ---- Target resolution refusals --------------------------------------------

/// No `resource_path` → refusal pointing at `ign workspace checkout`
/// and listing the valid members (editing "the whole tree" is
/// checkout's job — edit is ONE resource).
#[tokio::test]
async fn no_resource_path_refuses_with_the_member_list() {
    let zip = fixture_zip(42);
    let server = wiremock::MockServer::start().await;
    mount_export(&server, zip, 1).await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);

    let scratch = tempfile::tempdir().expect("scratch");
    let editor = ScriptEditor(editor_script(&scratch, "unused", "exit 0"));

    let err = edit_pipeline(&api, &editor, "demo", None)
        .await
        .expect_err("None refuses");
    let CoreError::InvalidInput { reason } = err else {
        panic!("expected InvalidInput, got {err:?}");
    };
    assert!(reason.contains("`ign workspace checkout`"), "{reason}");
    assert!(reason.contains(VIEW) && reason.contains(NESTED), "{reason}");
}

/// An unknown resource path → refusal naming the valid candidates.
#[tokio::test]
async fn unknown_resource_path_refuses_naming_candidates() {
    let zip = fixture_zip(42);
    let server = wiremock::MockServer::start().await;
    mount_export(&server, zip, 1).await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);

    let scratch = tempfile::tempdir().expect("scratch");
    let editor = ScriptEditor(editor_script(&scratch, "unused", "exit 0"));

    let err = edit_pipeline(&api, &editor, "demo", Some("com.example/views/Nope/x.json"))
        .await
        .expect_err("unknown member refuses");
    let CoreError::InvalidInput { reason } = err else {
        panic!("expected InvalidInput, got {err:?}");
    };
    assert!(
        reason.starts_with("\"com.example/views/Nope/x.json\" is not a resource member"),
        "{reason}"
    );
    assert!(reason.contains(VIEW) && reason.contains(NESTED), "{reason}");
}

// ---- The REAL TokioEditor, wired end-to-end through the env -----------------

/// Env is process-global and tests run in parallel threads; edition
/// 2024 makes `set_var` unsafe for exactly this reason — under this
/// lock it is sound. VISUAL/EDITOR restore on drop.
#[cfg(unix)] // consumed only by the real-TokioEditor spawn test
static EDITOR_ENV_LOCK: Mutex<()> = Mutex::new(());

#[cfg(unix)]
struct EditorEnvGuard {
    visual: Option<std::ffi::OsString>,
    editor: Option<std::ffi::OsString>,
    _lock: std::sync::MutexGuard<'static, ()>,
}

#[cfg(unix)]
impl EditorEnvGuard {
    /// Point VISUAL at `script` (and blank EDITOR) for the guard's
    /// lifetime.
    fn set_visual(script: &str) -> Self {
        let lock = EDITOR_ENV_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let visual = std::env::var_os("VISUAL");
        let editor = std::env::var_os("EDITOR");
        unsafe {
            std::env::set_var("VISUAL", script);
            std::env::remove_var("EDITOR");
        }
        Self {
            visual,
            editor,
            _lock: lock,
        }
    }
}

#[cfg(unix)]
impl Drop for EditorEnvGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.visual {
                Some(value) => std::env::set_var("VISUAL", value),
                None => std::env::remove_var("VISUAL"),
            }
            match &self.editor {
                Some(value) => std::env::set_var("EDITOR", value),
                None => std::env::remove_var("EDITOR"),
            }
        }
    }
}

/// The REAL [`TokioEditor`] — env resolution (`VISUAL`), the
/// whitespace split, the target appended LAST, the advisory exit —
/// driving the actual pipeline end to end with a real spawned
/// script as the editor. `Ready` proves the whole seam wiring.
#[cfg(unix)] // spawns the sh editor for real — Windows cmd-shim follow-up
#[tokio::test]
async fn tokio_editor_env_resolution_drives_the_real_pipeline() {
    let zip = fixture_zip(42);
    let server = wiremock::MockServer::start().await;
    mount_export(&server, zip, 2).await; // Ready runs the staleness gate: fetch + fresh
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);

    let scratch = tempfile::tempdir().expect("scratch");
    let script = {
        let path = scratch.path().join("env-editor");
        std::fs::write(&path, format!("#!/bin/sh\n{SED_EDIT}\n")).expect("script");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("chmod +x");
        }
        path.display().to_string()
    };
    let _env = EditorEnvGuard::set_visual(&script);

    let staged = edit_pipeline(&api, &TokioEditor, "demo", Some(NESTED))
        .await
        .expect("TokioEditor resolves VISUAL and runs the real pipeline");
    let EditStatus::Ready { changed } = staged.status else {
        panic!("expected Ready, got {:?}", staged.status);
    };
    assert_eq!(changed, vec![NESTED.to_string()]);
}
