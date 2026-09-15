//! Contract tests for `ign workspace` (13-07) — the binary-level
//! goldens over the real spawned binary against wiremock-served REAL
//! project exports (the contract_projects.rs harness: isolated
//! `IGNITION_CLI_CONFIG`, `IGNITION_URL` overlay, snapbox goldens).
//!
//! The status golden fixture is ONE workspace carrying EVERY row
//! kind (the 13-06 one-row-of-every-kind matrix at the binary level):
//! clean, local_edit, gateway_drift, conflict, deleted(local),
//! deleted(gateway), added, untracked — plus the four-counter summary
//! line — goldenned in human, pretty-JSON, and compact modes.
//!
//! Wiremock sequencing (13-06's mount-order convention): the
//! checkout-time export mounts FIRST with `up_to_n_times(1)`;
//! the post-drift export mounts second — checkout consumes zip0,
//! status/push consume zip1.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use assert_cmd::Command;

// ---- Harness (the contract_projects.rs pattern) ----------------------------

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

// ---- Export-zip fixtures (project-export-shaped ONLY — never tag data) -----

fn fixture_zip(members: &[(&str, &[u8])]) -> Vec<u8> {
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

/// A Perspective view member (never touched by any mutation — the
/// clean row).
const VIEW_MEMBER: &str = "com.example/resources/views/Dashboard/view.json";
const VIEW_JSON: &str = r#"{"scope":"G","children":[]}"#;

/// The view folder's descriptor. `version` is SEMANTIC (survives
/// descriptor normalization) — 1 at checkout, 2 in the fresh export:
/// the gateway_drift row.
const DESC_MEMBER: &str = "com.example/resources/views/Dashboard/resource.json";

fn desc_json(version: u32) -> String {
    format!(
        r#"{{"scope":"G","version":{version},"restricted":false,"overridable":true,"files":["view.json"],"attributes":{{"lastModification":{{"time":1700000000000,"nano":0}},"lastModificationSignature":"sig-1"}}}}"#
    )
}

/// The nested member. Value 1 at checkout and in the fresh export;
/// the LOCAL tree copy is rewritten to 2 — the local_edit row.
const NESTED_MEMBER: &str = "com.example/resources/views/Nested/Deep/other.json";

fn nested_json(value: u32) -> String {
    format!(r#"{{"scope":"G","value":{value}}}"#)
}

/// The script-python member. `print('hi')` at checkout; the LOCAL
/// tree copy becomes `print('local')` AND the fresh export carries
/// `print('gw')` — the conflict row.
const SCRIPT_MEMBER: &str = "ignition/resources/script-python/e2e/scratch";

/// Deleted LOCALLY after checkout (file removed from the tree); the
/// fresh export still carries the baseline — the deleted(local) row.
const GONE_MEMBER: &str = "com.example/resources/views/Gone/extra.json";

/// Present at checkout and on disk; the fresh export dropped it —
/// the deleted(gateway) row.
const REMOVED_MEMBER: &str = "com.example/resources/views/Removed/stale.json";

/// In the fresh export only — the added row.
const NEW_MEMBER: &str = "com.example/resources/views/New/new.json";

/// A Perspective view member bearing ONE embedded python script (the
/// Flint-escaped form) — the `--decode-scripts` leg's binary-level
/// exercise.
const VIEW_JSON_SCRIPTED: &str = r#"{
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

fn checkout_zip() -> Vec<u8> {
    fixture_zip(&[
        (VIEW_MEMBER, VIEW_JSON.as_bytes()),
        (DESC_MEMBER, desc_json(1).as_bytes()),
        (NESTED_MEMBER, nested_json(1).as_bytes()),
        (SCRIPT_MEMBER, b"print('hi')\n".as_slice()),
        (GONE_MEMBER, br#"{"scope":"G","value":7}"#),
        (REMOVED_MEMBER, br#"{"scope":"G","value":8}"#),
    ])
}

fn drifted_zip() -> Vec<u8> {
    fixture_zip(&[
        (VIEW_MEMBER, VIEW_JSON.as_bytes()),
        (DESC_MEMBER, desc_json(2).as_bytes()),
        (NESTED_MEMBER, nested_json(1).as_bytes()),
        (SCRIPT_MEMBER, b"print('gw')\n".as_slice()),
        (GONE_MEMBER, br#"{"scope":"G","value":7}"#),
        (NEW_MEMBER, br#"{"scope":"G","value":9}"#),
    ])
}

/// THE status golden: every row kind renders in one workspace (plus
/// the summary line), pinned in human, pretty-JSON, and compact
/// modes. Status is read-only: exactly one export GET per run (the
/// drift export, post-checkout).
#[tokio::test]
async fn workspace_status_render_modes_golden() {
    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");
    let ws = tempfile::tempdir().expect("ws tempdir");
    let root = ws.path().join("ws");

    // The wiremock server must live as long as the whole flow — run
    // the choreography inline here rather than in a helper that drops
    // it (the helper owns checkout only; the export mock below is
    // this test's own server).
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/projects/export/proj",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_raw(checkout_zip(), "application/zip"),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/projects/export/proj",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_raw(drifted_zip(), "application/zip"),
        )
        .expect(1..)
        .mount(&server)
        .await;

    let out = ign(
        &config,
        &server.uri(),
        &[
            "workspace",
            "checkout",
            "proj",
            root.to_str().expect("utf-8 root"),
        ],
    );
    assert!(
        out.status.success(),
        "checkout failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::write(
        root.join("com.example/views/Nested/Deep/other.json"),
        nested_json(2),
    )
    .expect("local edit writes");
    std::fs::write(
        root.join("ignition/script-python/e2e/scratch"),
        b"print('local')\n",
    )
    .expect("conflict edit writes");
    std::fs::remove_file(root.join("com.example/views/Gone/extra.json"))
        .expect("local deletion removes");
    std::fs::write(root.join("notes.txt"), "scratch notes\n").expect("untracked writes");

    // Human mode: the full PATH/STATE table (every row kind, clean
    // rows included) + the four-counter summary.
    let out = ign(
        &config,
        &server.uri(),
        &["workspace", "status", root.to_str().expect("utf-8 root")],
    );
    assert!(
        out.status.success(),
        "status failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
[profile: dev]
com.example/views/Dashboard/resource.json  gateway_drift
com.example/views/Dashboard/view.json  clean
com.example/views/Gone/extra.json  deleted (local)
com.example/views/Nested/Deep/other.json  local_edit
com.example/views/Removed/stale.json  deleted (gateway)
ignition/script-python/e2e/scratch  conflict
com.example/views/New/new.json  added
notes.txt  untracked
1 local edits, 1 gateway drift, 1 conflicts, 1 untracked
"#]],
    );

    // Pretty JSON: the WorkspaceStatus envelope verbatim.
    let out = ign(
        &config,
        &server.uri(),
        &[
            "workspace",
            "status",
            root.to_str().expect("utf-8 root"),
            "--json",
        ],
    );
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
{
  "ok": true,
  "profile": "dev",
  "data": {
    "project": "proj",
    "clean": false,
    "rows": [
      {
        "path": "com.example/views/Dashboard/resource.json",
        "kind": "gateway_drift"
      },
      {
        "path": "com.example/views/Dashboard/view.json",
        "kind": "clean"
      },
      {
        "path": "com.example/views/Gone/extra.json",
        "kind": {
          "deleted": {
            "local": true
          }
        }
      },
      {
        "path": "com.example/views/Nested/Deep/other.json",
        "kind": "local_edit"
      },
      {
        "path": "com.example/views/Removed/stale.json",
        "kind": {
          "deleted": {
            "local": false
          }
        }
      },
      {
        "path": "ignition/script-python/e2e/scratch",
        "kind": "conflict"
      },
      {
        "path": "com.example/views/New/new.json",
        "kind": {
          "added": {
            "local": false
          }
        }
      },
      {
        "path": "notes.txt",
        "kind": "untracked"
      }
    ]
  }
}
"#]],
    );

    // Compact: the same envelope on one line.
    let out = ign(
        &config,
        &server.uri(),
        &[
            "workspace",
            "status",
            root.to_str().expect("utf-8 root"),
            "--compact",
        ],
    );
    assert!(out.status.success());
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"project":"proj","clean":false,"rows":[{"path":"com.example/views/Dashboard/resource.json","kind":"gateway_drift"},{"path":"com.example/views/Dashboard/view.json","kind":"clean"},{"path":"com.example/views/Gone/extra.json","kind":{"deleted":{"local":true}}},{"path":"com.example/views/Nested/Deep/other.json","kind":"local_edit"},{"path":"com.example/views/Removed/stale.json","kind":{"deleted":{"local":false}}},{"path":"ignition/script-python/e2e/scratch","kind":"conflict"},{"path":"com.example/views/New/new.json","kind":{"added":{"local":false}}},{"path":"notes.txt","kind":"untracked"}]}}"#]],
    );
}

// ---- Task 3: refusal goldens + checkout/push binary contract tests ---------

/// Sequenced export server (13-06's mount-order convention): checkout
/// consumes zip0 (mounted FIRST, `up_to_n_times(1)`); every later
/// export falls through to zip1.
async fn sequenced_export_server(zip0: Vec<u8>, zip1: Vec<u8>) -> wiremock::MockServer {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/projects/export/proj",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_raw(zip0, "application/zip"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/projects/export/proj",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_raw(zip1, "application/zip"))
        .mount(&server)
        .await;
    server
}

/// The import mock — scoped with the expected count (13-06's guard
/// shape): a stray import fails verification loudly at scope drop, a
/// confirmed push is pinned to exactly ONE.
async fn import_guard(
    server: &wiremock::MockServer,
    expected_imports: usize,
) -> wiremock::MockGuard {
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/data/api/v1/projects/import/proj",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"status":"imported"})),
        )
        .expect(expected_imports as u64)
        .mount_as_scoped(server)
        .await
}

/// A fresh workspace root: `<tempdir>/ws` (the subdir keeps the
/// checkout target pristine).
fn workspace_root() -> (tempfile::TempDir, PathBuf) {
    let ws = tempfile::tempdir().expect("ws tempdir");
    let root = ws.path().join("ws");
    (ws, root)
}

/// Run the binary's checkout into `root` and demand success.
fn checkout_ok(config: &Path, url: &str, root: &Path) {
    let out = ign(
        config,
        url,
        &[
            "workspace",
            "checkout",
            "proj",
            root.to_str().expect("utf-8 root"),
        ],
    );
    assert!(
        out.status.success(),
        "checkout failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// THE push-refusal golden: without `--yes`, a push with work to do
/// refuses exit 2 `confirmation_required` and the stderr message IS
/// the blast-radius preview (13-06's `render_push_preview`, verbatim
/// through the CLI — the one-gate property holds end to end). Traffic
/// pin: the status read GET only, ZERO imports (scope-verified).
#[tokio::test]
async fn push_without_yes_goldens_the_blast_radius_preview() {
    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");
    let (_ws, root) = workspace_root();
    let server = sequenced_export_server(checkout_zip(), drifted_zip()).await;
    checkout_ok(&config, &server.uri(), &root);
    std::fs::write(
        root.join("com.example/views/Nested/Deep/other.json"),
        nested_json(2),
    )
    .expect("local edit writes");

    {
        let _imports = import_guard(&server, 0).await;

        // Human mode: profile header + preview-as-message + hint.
        let out = ign(
            &config,
            &server.uri(),
            &["workspace", "push", root.to_str().expect("utf-8 root")],
        );
        assert_eq!(out.status.code(), Some(2), "the guard refuses exit 2");
        let stderr = String::from_utf8_lossy(&out.stderr);
        snapbox::Assert::new().action_env("SNAPSHOTS").eq(
            stderr.trim_end(),
            snapbox::str![[r#"
[profile: dev]
error: workspace push would write 1 member(s) and delete 0 member(s)
  write: com.example/views/Nested/Deep/other.json is destructive; rerun with --yes to confirm
hint: this operation is destructive; re-run with --yes or set IGNITION_YES=1
"#]],
        );

        // JSON mode: the same preview rides the error envelope's
        // message, `confirmation_required` code, resolved profile.
        let out = ign(
            &config,
            &server.uri(),
            &[
                "workspace",
                "push",
                root.to_str().expect("utf-8 root"),
                "--json",
            ],
        );
        assert_eq!(out.status.code(), Some(2));
        let envelope: serde_json::Value =
            serde_json::from_str(stderr_envelope(&out).trim()).expect("envelope parses");
        assert_eq!(envelope["ok"], serde_json::Value::Bool(false));
        assert_eq!(envelope["profile"], "dev");
        assert_eq!(envelope["error"]["code"], "confirmation_required");
        let message = envelope["error"]["message"].as_str().expect("message");
        assert!(
            message.contains("workspace push would write 1 member(s) and delete 0 member(s)")
                && message.contains("  write: com.example/views/Nested/Deep/other.json"),
            "the message IS the preview: {message}"
        );
    } // scope drop verifies: ZERO imports fired
}

/// The conflict refusal: a BOTH-sides-diverged member refuses exit 2
/// naming every diverged path — and fires EVEN WITH `--yes` (Pitfall
/// W2: clobbering a concurrent Designer edit is beyond any flag).
/// Traffic pin: zero imports on both runs.
#[tokio::test]
async fn push_conflict_refuses_even_with_yes_naming_both_diverged_members() {
    // Gateway moved semantically on nested AND scratch; the local
    // tree will move both the other way.
    let conflict_zip = fixture_zip(&[
        (VIEW_MEMBER, VIEW_JSON.as_bytes()),
        (DESC_MEMBER, desc_json(1).as_bytes()),
        (NESTED_MEMBER, nested_json(3).as_bytes()),
        (SCRIPT_MEMBER, b"print('gw2')\n".as_slice()),
        (GONE_MEMBER, br#"{"scope":"G","value":7}"#),
        (REMOVED_MEMBER, br#"{"scope":"G","value":8}"#),
    ]);
    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");
    let (_ws, root) = workspace_root();
    let server = sequenced_export_server(checkout_zip(), conflict_zip).await;
    checkout_ok(&config, &server.uri(), &root);
    std::fs::write(
        root.join("com.example/views/Nested/Deep/other.json"),
        nested_json(2),
    )
    .expect("local edit writes");
    std::fs::write(
        root.join("ignition/script-python/e2e/scratch"),
        b"print('local2')\n",
    )
    .expect("conflict edit writes");

    {
        let _imports = import_guard(&server, 0).await;

        // WITHOUT --yes.
        let out = ign(
            &config,
            &server.uri(),
            &["workspace", "push", root.to_str().expect("utf-8 root")],
        );
        assert_eq!(out.status.code(), Some(2));
        let stderr = String::from_utf8_lossy(&out.stderr);
        for phrase in [
            "workspace push refused",
            "2 member(s) changed on BOTH sides since checkout",
            "\"com.example/views/Nested/Deep/other.json\"",
            "\"ignition/script-python/e2e/scratch\"",
            "conflicts are never force-pushed",
        ] {
            assert!(
                stderr.contains(phrase),
                "conflict refusal must name the diverged sides ({phrase:?}): {stderr}"
            );
        }

        // WITH --yes — the refusal fires anyway (never --yes-able).
        let out = ign(
            &config,
            &server.uri(),
            &[
                "workspace",
                "push",
                root.to_str().expect("utf-8 root"),
                "--yes",
            ],
        );
        assert_eq!(
            out.status.code(),
            Some(2),
            "conflicts refuse even with --yes"
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("never force-pushed"),
            "the same refusal with --yes: {stderr}"
        );
    } // scope drop verifies: ZERO imports fired
}

/// Checkout's clobber ladder at the binary level: an unrelated
/// non-empty target refuses (pre-existing file untouched), a
/// foreign-manifest target refuses naming BOTH projects.
#[tokio::test]
async fn checkout_refuses_unrelated_nonempty_and_foreign_manifest_targets() {
    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/projects/export/proj",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_raw(checkout_zip(), "application/zip"),
        )
        .expect(2)
        .mount(&server)
        .await;

    // (a) Unrelated non-empty target.
    let target = tempfile::tempdir().expect("target tempdir");
    std::fs::write(target.path().join("precious.txt"), "keep me").expect("seed file");
    let out = ign(
        &config,
        &server.uri(),
        &[
            "workspace",
            "checkout",
            "proj",
            target.path().to_str().expect("utf-8 target"),
        ],
    );
    assert_eq!(out.status.code(), Some(2), "clobber refuses exit 2");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("is not empty and is not an ign workspace"),
        "13-03's stable clobber refusal: {stderr}"
    );
    assert_eq!(
        std::fs::read_to_string(target.path().join("precious.txt")).expect("file survives"),
        "keep me",
        "the pre-existing file is untouched"
    );

    // (b) Foreign-manifest target.
    let foreign = tempfile::tempdir().expect("foreign tempdir");
    std::fs::write(
        foreign.path().join(".ign-workspace.json"),
        r#"{"schema_version":1,"project":"other","profile":"dev","checked_out_at":"2026-09-15T00:00:00.000Z","members":{}}"#,
    )
    .expect("foreign manifest");
    let out = ign(
        &config,
        &server.uri(),
        &[
            "workspace",
            "checkout",
            "proj",
            foreign.path().to_str().expect("utf-8 target"),
        ],
    );
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("checked out from project \"other\"")
            && stderr.contains("refusing to check out \"proj\" over it"),
        "the refusal names BOTH projects: {stderr}"
    );
}

/// `--decode-scripts` happy path at the binary level: the human
/// output golden (target path normalized) and BOTH manifests + the
/// editable sidecar in the target tree.
#[tokio::test]
async fn checkout_decode_scripts_happy_path_golden() {
    let decode_zip = fixture_zip(&[
        (VIEW_MEMBER, VIEW_JSON_SCRIPTED.as_bytes()),
        (DESC_MEMBER, desc_json(1).as_bytes()),
        (NESTED_MEMBER, nested_json(1).as_bytes()),
        (SCRIPT_MEMBER, b"print('hi')\n".as_slice()),
    ]);
    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");
    let (_ws, root) = workspace_root();
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/projects/export/proj",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_raw(decode_zip, "application/zip"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let out = ign(
        &config,
        &server.uri(),
        &[
            "workspace",
            "checkout",
            "proj",
            root.to_str().expect("utf-8 root"),
            "--decode-scripts",
        ],
    );
    assert!(
        out.status.success(),
        "decode checkout failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Human golden with the random target path normalized.
    let stdout = String::from_utf8_lossy(&out.stdout)
        .replace(root.to_str().expect("utf-8 root"), "<TARGET>");
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout.trim_end(),
        snapbox::str![[r#"
[profile: dev]
checked out 4 members → <TARGET>
scripts decoded — *.py sidecars + scripts-manifest.json; an unedited re-encode is byte-exact
"#]],
    );

    // The decode artifacts really landed: the sidecar beside the
    // member's mapped path (decoded + dedented), the codec manifest,
    // and the workspace manifest.
    let sidecar = std::fs::read_to_string(root.join("com.example/views/Dashboard/view.json.1.py"))
        .expect("sidecar exists");
    assert_eq!(sidecar, "print 'clicked'\nprint 'done <>&='");
    assert!(
        root.join("scripts-manifest.json").exists(),
        "codec manifest"
    );
    assert!(
        root.join(".ign-workspace.json").exists(),
        "workspace manifest"
    );
}

/// Status against a FOREIGN-SCHEMA manifest: 13-03's stable refusal
/// prefix, exit 2, and — because the manifest read needs no gateway —
/// the JSON envelope carries profile null (the pre-resolve posture).
#[tokio::test]
async fn status_foreign_schema_manifest_refuses_with_stable_prefix() {
    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");
    let foreign = tempfile::tempdir().expect("foreign tempdir");
    std::fs::write(
        foreign.path().join(".ign-workspace.json"),
        r#"{"schema_version":2,"project":"proj","profile":"dev","checked_out_at":"2026-09-15T00:00:00.000Z","members":{}}"#,
    )
    .expect("foreign-schema manifest");

    let out = ign(
        &config,
        "http://ignored.example.com",
        &[
            "workspace",
            "status",
            foreign.path().to_str().expect("utf-8 path"),
        ],
    );
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unsupported workspace manifest schema_version 2 (expected 1)"),
        "13-03's stable foreign-schema prefix: {stderr}"
    );
}

/// The guarded push happy path: a clean workspace push is an honest
/// no-op even WITHOUT --yes (zero-write, never prompts); after one
/// local edit + one local deletion, `push --yes --delete` writes and
/// deletes exactly the selected members with EXACTLY ONE import
/// (scope-verified) — and the outcome renders as data in every mode.
#[tokio::test]
async fn push_yes_delete_splices_and_imports_exactly_once() {
    let (_dir, config) = isolated_config();
    write_profile_config(&config, "http://ignored.example.com");
    let (_ws, root) = workspace_root();
    // Exports: checkout + clean-status + splice (status + fresh) = 4.
    let server = sequenced_export_server(checkout_zip(), drifted_zip()).await;
    checkout_ok(&config, &server.uri(), &root);

    {
        // Import arithmetic: the clean no-op fires ZERO; each real
        // splice push fires EXACTLY ONE; two real pushes follow
        // (human + compact goldens) → 2 total. Any extra import
        // fails verification loudly at scope drop.
        let _imports = import_guard(&server, 2).await;

        // Clean workspace: a no-op even without --yes.
        let out = ign(
            &config,
            &server.uri(),
            &["workspace", "push", root.to_str().expect("utf-8 root")],
        );
        assert!(
            out.status.success(),
            "a clean push is a no-op, never a prompt: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        snapbox::Assert::new().action_env("SNAPSHOTS").eq(
            stdout_for_golden(&out),
            snapbox::str![[r#"
[profile: dev]
pushed proj — wrote 0, deleted 0, skipped 0
"#]],
        );

        // One local edit + one local deletion → the guarded splice.
        std::fs::write(
            root.join("com.example/views/Nested/Deep/other.json"),
            nested_json(2),
        )
        .expect("local edit writes");
        std::fs::remove_file(root.join("com.example/views/Gone/extra.json"))
            .expect("local deletion removes");
        let out = ign(
            &config,
            &server.uri(),
            &[
                "workspace",
                "push",
                root.to_str().expect("utf-8 root"),
                "--yes",
                "--delete",
            ],
        );
        assert!(
            out.status.success(),
            "push --yes --delete failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        snapbox::Assert::new().action_env("SNAPSHOTS").eq(
            stdout_for_golden(&out),
            snapbox::str![[r#"
[profile: dev]
pushed proj — wrote 1, deleted 1, skipped 0
  wrote: com.example/views/Nested/Deep/other.json
  deleted: com.example/views/Gone/extra.json
"#]],
        );

        // Compact: the PushOutcome envelope verbatim.
        let out = ign(
            &config,
            &server.uri(),
            &[
                "workspace",
                "push",
                root.to_str().expect("utf-8 root"),
                "--yes",
                "--delete",
                "--compact",
            ],
        );
        assert!(out.status.success());
        snapbox::Assert::new().action_env("SNAPSHOTS").eq(
            stdout_for_golden(&out),
            snapbox::str![[r#"{"ok":true,"profile":"dev","data":{"project":"proj","wrote":["com.example/views/Nested/Deep/other.json"],"deleted":["com.example/views/Gone/extra.json"],"skipped":[]}}"#]],
        );
    } // scope drop verifies: EXACTLY ONE import fired
}
