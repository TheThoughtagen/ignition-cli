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
