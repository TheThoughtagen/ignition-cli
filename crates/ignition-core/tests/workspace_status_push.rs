//! Contract tests for `ign workspace status` + `push` (13-06): the
//! manifest three-way compare over the ONE hash/normalize
//! implementation, and the guarded push splice.
//!
//! Task 1 pins (status):
//! - the exhaustive `classify` matrix (8 `Option` combinations → 4
//!   primary kinds, None-handling documented);
//! - ONE wiremock-served integration status with one row of EVERY
//!   kind (clean / local_edit / gateway_drift / conflict /
//!   deleted{local} both ways / added / untracked);
//! - PUSH-RELATIVE direction semantics (planner lock, load-bearing):
//!   a `local_edit` row says "push would WRITE this member";
//! - descriptor `lastModification` volatility is NOT drift at the
//!   status level (no byte-compare ever sneaks in);
//! - the additive envelope shapes.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::Path;

use ignition_core::actions::workspace::{
    StatusKind, StatusRow, WorkspaceStatus, classify, read_manifest, workspace_checkout,
    workspace_status,
};
use ignition_core::client::ReqwestGatewayApi;
use ignition_core::client::resources::{member_hashes, read_member, remove_member, replace_member};

// ---- Fixtures (project-export-shaped ONLY — never tag data) ---------------

/// Build a small export zip: `project.json` + one member per pair,
/// in order (the same zip crate the surgery rides — honest
/// fixtures). `project.json` is export metadata, NOT a resource
/// member, and never lands in the workspace tree.
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

/// Six members so every row kind fits in ONE workspace: the checkout
/// fixture's four (view + descriptor + nested + script) plus a pair
/// that get deleted (EXTRA locally, GONE gateway-side).
const VIEW_RAW: &str = "com.example/resources/views/Dashboard/view.json";
const VIEW: &str = "com.example/views/Dashboard/view.json";
const VIEW_JSON: &str = r#"{"scope":"G","children":[]}"#;

/// The view folder's descriptor — carries the
/// `attributes.lastModification` volatility noise the hash must
/// normalize away. `version` is SEMANTIC (survives normalization),
/// unlike the time/signature noise.
const DESC_RAW: &str = "com.example/resources/views/Dashboard/resource.json";
const DESC: &str = "com.example/views/Dashboard/resource.json";

fn desc_json_v(version: u32, time_ms: i64, signature: &str) -> String {
    format!(
        r#"{{"scope":"G","version":{version},"restricted":false,"overridable":true,"files":["view.json"],"attributes":{{"lastModification":{{"time":{time_ms},"nano":0}},"lastModificationSignature":"{signature}"}}}}"#
    )
}

/// Version-1 descriptor — the volatility test's SAME-content form.
fn desc_json(time_ms: i64, signature: &str) -> String {
    desc_json_v(1, time_ms, signature)
}

const NESTED_RAW: &str = "com.example/resources/views/Nested/Deep/other.json";
const NESTED: &str = "com.example/views/Nested/Deep/other.json";

fn nested_json(value: u32) -> String {
    format!(r#"{{"scope":"G","value":{value}}}"#)
}

const SCRIPT_RAW: &str = "ignition/resources/script-python/e2e/scratch";
const SCRIPT: &str = "ignition/script-python/e2e/scratch";
const SCRIPT_GW: &[u8] = b"print('gateway-version')\n";

/// Deleted LOCALLY in the every-kind scenario.
const EXTRA_RAW: &str = "com.example/resources/views/Extra/extra.json";
const EXTRA: &str = "com.example/views/Extra/extra.json";
const EXTRA_BYTES: &[u8] = b"{\"scope\":\"G\"}";

/// Deleted GATEWAY-side in the every-kind scenario.
const GONE_RAW: &str = "com.example/resources/views/Gone/gone.json";
const GONE: &str = "com.example/views/Gone/gone.json";
const GONE_BYTES: &[u8] = b"{\"scope\":\"G\"}";

/// The gateway-added member (user path; replace_member appends it
/// into the fresh export under `…/resources/views/New/…` and ALSO
/// synthesizes that folder's descriptor — BOTH land as added rows).
const NEW: &str = "com.example/views/New/new.json";
const NEW_DESC: &str = "com.example/views/New/resource.json";

/// The checkout baseline zip (six members).
fn baseline_zip() -> Vec<u8> {
    fixture_zip(&[
        (VIEW_RAW, VIEW_JSON.as_bytes()),
        (DESC_RAW, desc_json(1_757_904_000_000, "sig-a").as_bytes()),
        (NESTED_RAW, nested_json(42).as_bytes()),
        (SCRIPT_RAW, b"print('hi')\n".as_slice()),
        (EXTRA_RAW, EXTRA_BYTES),
        (GONE_RAW, GONE_BYTES),
    ])
}

/// The GATEWAY-MOVED-ON zip: the script member changed on the gateway
/// (drift), the descriptor changed SEMANTICALLY on the gateway (the
/// conflict side — version 2 survives normalization), and a NEW
/// member appeared (added).
fn moved_on_zip(baseline: &[u8]) -> Vec<u8> {
    let step = replace_member(baseline, SCRIPT, SCRIPT_GW).expect("script drift");
    let step = replace_member(
        &step,
        DESC,
        desc_json_v(2, 1_757_990_400_000, "sig-gw").as_bytes(),
    )
    .expect("descriptor conflict");
    let step = remove_member(&step, GONE).expect("gateway-side deletion");
    replace_member(&step, NEW, b"{\"new\":true}").expect("gateway-side addition")
}

/// Checkout the baseline into a fresh tempdir over its own wiremock
/// server; returns (root, baseline zip).
async fn checkout_baseline() -> (tempfile::TempDir, Vec<u8>) {
    let zip = baseline_zip();
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/projects/export/demo",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_raw(zip.clone(), "application/zip"),
        )
        .expect(1) // checkout is read-only: ONE export
        .mount(&server)
        .await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let target = tempfile::tempdir().expect("target tempdir");
    workspace_checkout(&api, "demo", target.path(), "test-profile", false)
        .await
        .expect("checkout Ok");
    (target, zip)
}

/// Apply the every-kind LOCAL mutations to a checked-out tree:
/// NESTED edited locally (local_edit), DESC edited locally too
/// (conflict — the gateway also moved, semantically), EXTRA deleted
/// locally (deleted{local:true}), one untracked file added.
fn mutate_tree_locally(root: &Path) {
    let manifest = read_manifest(root).expect("manifest");
    std::fs::write(
        root.join(&manifest.members[NESTED].local_path),
        nested_json(99),
    )
    .expect("local edit");
    std::fs::write(
        root.join(&manifest.members[DESC].local_path),
        desc_json_v(3, 1_757_904_000_000, "sig-local"),
    )
    .expect("local descriptor edit");
    std::fs::remove_file(root.join(&manifest.members[EXTRA].local_path)).expect("local deletion");
    std::fs::write(root.join("notes.txt"), b"untracked\n").expect("untracked file");
}

fn rows_by_path(status: &WorkspaceStatus) -> BTreeMap<&str, &StatusKind> {
    status
        .rows
        .iter()
        .map(|row| (row.path.as_str(), &row.kind))
        .collect()
}

// ---- The exhaustive classify matrix (pure fn, 8 combos) --------------------

/// ALL 8 `Option` combinations → the four primary kinds, with the
/// documented None-handling: an absent side never equals a present
/// hash (so it counts as "moved" until the action layer refines the
/// deletion verdicts), both-absent is Clean (nothing to reconcile),
/// and no-baseline sides that agree are Clean while disagreeing
/// sides refuse.
#[test]
fn classify_matrix_is_exhaustive_over_all_eight_combinations() {
    let (m, x, y) = (1u64, 2u64, 3u64);
    assert_ne!(x, m);
    assert_ne!(y, m);
    assert_ne!(x, y);

    // Baseline present — the four canonical rows.
    assert_eq!(classify(Some(m), Some(m), Some(m)), StatusKind::Clean);
    assert_eq!(classify(Some(m), Some(x), Some(m)), StatusKind::LocalEdit);
    assert_eq!(
        classify(Some(m), Some(m), Some(x)),
        StatusKind::GatewayDrift
    );
    assert_eq!(classify(Some(m), Some(x), Some(y)), StatusKind::Conflict);

    // Locally absent: local "moved". Gateway at baseline → the raw
    // LocalEdit verdict the action layer refines into
    // Deleted { local: true }; gateway moved too → Conflict (a
    // deletion would clobber the gateway's new state).
    assert_eq!(classify(Some(m), None, Some(m)), StatusKind::LocalEdit);
    assert_eq!(classify(Some(m), None, Some(x)), StatusKind::Conflict);

    // Gateway-absent: gateway "moved". Local at baseline → the raw
    // GatewayDrift verdict refined into Deleted { local: false };
    // local moved too → Conflict (a write would resurrect against
    // the gateway's deletion).
    assert_eq!(classify(Some(m), Some(m), None), StatusKind::GatewayDrift);
    assert_eq!(classify(Some(m), Some(x), None), StatusKind::Conflict);

    // BOTH absent: both sides deleted it — nothing to reconcile.
    assert_eq!(classify(Some(m), None, None), StatusKind::Clean);

    // No baseline (totality; not a production row): agreeing sides
    // are Clean, disagreeing sides refuse.
    assert_eq!(classify(None, Some(x), Some(x)), StatusKind::Clean);
    assert_eq!(classify(None, Some(x), Some(y)), StatusKind::Conflict);
    assert_eq!(classify(None, None, Some(x)), StatusKind::Conflict);
    assert_eq!(classify(None, None, None), StatusKind::Clean);
}

// ---- Direction semantics: PUSH-RELATIVE, pinned in source + behavior -------

/// The direction doc is CONTRACT (planner lock: the label direction
/// is load-bearing for agents reading the output) — pinned here the
/// readme_exit_table_agreement way, by include_str!-ing the source:
/// if the StatusKind docs ever stop saying what push does per row,
/// this goes red.
#[test]
fn status_kind_direction_docs_are_push_relative() {
    // Whitespace-normalized with doc markers stripped: doc-comment
    // wrapping splits phrases across lines and interleaves `///` —
    // the CONTRACT is the prose, not the wrap.
    let source = include_str!("../src/actions/workspace.rs");
    let prose = source
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            trimmed
                .strip_prefix("/// ")
                .or_else(|| trimmed.strip_prefix("///"))
                .unwrap_or(trimmed)
        })
        .collect::<Vec<_>>()
        .join(" ");
    let normalized = prose.split_whitespace().collect::<Vec<_>>().join(" ");
    for phrase in [
        "push would WRITE this member",
        "push would NOT touch this member",
        "push REFUSES outright",
        "PUSH-RELATIVE",
    ] {
        assert!(
            normalized.contains(phrase),
            "StatusKind direction doc is contract and must contain {phrase:?}"
        );
    }
}

/// The operational pin behind the docs: the member status flags
/// `local_edit` is exactly the one whose LOCAL bytes diverged while
/// the gateway still matches the recorded baseline — i.e. the
/// member push would write (Task 2's happy-path test proves the
/// local bytes are what land).
#[tokio::test]
async fn local_edit_row_means_push_would_write_that_member() {
    let (target, zip) = checkout_baseline().await;
    let manifest = read_manifest(target.path()).expect("manifest");
    let baseline_hash = manifest.members[NESTED].hash;

    // Gateway unchanged; local edited.
    std::fs::write(
        target.path().join(&manifest.members[NESTED].local_path),
        nested_json(99),
    )
    .expect("edit");

    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/projects/export/demo",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_raw(zip.clone(), "application/zip"),
        )
        .mount(&server)
        .await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let status = workspace_status(target.path(), &api, "demo")
        .await
        .expect("status");

    let rows = rows_by_path(&status);
    assert_eq!(
        rows[NESTED],
        &StatusKind::LocalEdit,
        "locally-edited member must be local_edit"
    );
    // The operational meaning: local diverged, gateway at baseline.
    assert_eq!(
        member_hashes(&zip).expect("hashes")[NESTED],
        baseline_hash,
        "the gateway still carries the baseline content"
    );
}

// ---- THE every-kind integration status --------------------------------------

/// One status over a synthetic tree + wiremock-served fresh export
/// with one row of EVERY kind — the full three-way matrix plus the
/// set differences, all through the ONE hash/normalize
/// implementation.
#[tokio::test]
async fn status_reports_one_row_of_every_kind() {
    let (target, zip) = checkout_baseline().await;
    let fresh = moved_on_zip(&zip);
    mutate_tree_locally(target.path());

    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/projects/export/demo",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_raw(fresh.clone(), "application/zip"),
        )
        .expect(1) // status is read-only: ONE export GET
        .mount(&server)
        .await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let status = workspace_status(target.path(), &api, "demo")
        .await
        .expect("status");

    assert!(!status.clean, "every-kind workspace is not clean");
    assert_eq!(status.project, "demo");

    // The EXACT expected row set — every kind, every path.
    let expected: BTreeMap<String, StatusKind> = [
        (VIEW.to_string(), StatusKind::Clean),
        (NESTED.to_string(), StatusKind::LocalEdit),
        (SCRIPT.to_string(), StatusKind::GatewayDrift),
        (DESC.to_string(), StatusKind::Conflict),
        (EXTRA.to_string(), StatusKind::Deleted { local: true }),
        (GONE.to_string(), StatusKind::Deleted { local: false }),
        (NEW.to_string(), StatusKind::Added { local: false }),
        // replace_member's append synthesized the new folder's
        // descriptor — the fresh export carries it too (added).
        (NEW_DESC.to_string(), StatusKind::Added { local: false }),
        ("notes.txt".to_string(), StatusKind::Untracked),
    ]
    .into_iter()
    .collect();
    let actual: BTreeMap<String, StatusKind> = status
        .rows
        .iter()
        .map(|row| (row.path.clone(), row.kind.clone()))
        .collect();
    assert_eq!(actual, expected, "EXACT row set, every kind accounted for");
    assert_eq!(status.rows.len(), 9);
}

/// Descriptor `lastModification` volatility is NOT drift: a re-export
/// whose ONLY change is the descriptor's time/signature noise
/// statuses CLEAN for that member — the normalization guard, end to
/// end at the status level (no byte-compare ever sneaks in).
#[tokio::test]
async fn descriptor_volatility_is_not_drift() {
    let (target, zip) = checkout_baseline().await;
    let noisy = replace_member(
        &zip,
        DESC,
        desc_json(1_757_990_400_000, "sig-moved-on-gateway").as_bytes(),
    )
    .expect("descriptor noise");
    // Cross-check the fixture: the noisy descriptor's RAW bytes differ
    // from checkout's — only the NORMALIZED hash may agree.
    assert_ne!(
        read_member(&zip, DESC).expect("baseline descriptor"),
        read_member(&noisy, DESC).expect("noisy descriptor"),
        "fixture must differ byte-wise so only normalization can see through it"
    );

    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/projects/export/demo",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_raw(noisy, "application/zip"))
        .mount(&server)
        .await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let status = workspace_status(target.path(), &api, "demo")
        .await
        .expect("status");

    assert!(
        status.clean,
        "volatility-only re-export is clean: {:?}",
        status.rows
    );
    let rows = rows_by_path(&status);
    assert_eq!(rows[DESC], &StatusKind::Clean);
}

/// The untracked walk reports genuine user files but never the
/// workspace's own machinery or codec artifacts (`.gitignore`, both
/// manifests, `.py` sidecars) — and never a recorded member path.
/// An untracked row is still dirt (`clean` is false — every
/// non-Clean row makes it so).
#[tokio::test]
async fn untracked_never_includes_workspace_machinery() {
    let (target, zip) = checkout_baseline().await;
    std::fs::write(target.path().join("notes.txt"), b"x").expect("user file");
    std::fs::write(target.path().join("scratch.1.py"), b"# sidecar").expect("sidecar");

    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/projects/export/demo",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_raw(zip.clone(), "application/zip"),
        )
        .mount(&server)
        .await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let status = workspace_status(target.path(), &api, "demo")
        .await
        .expect("status");

    let untracked: Vec<&str> = status
        .rows
        .iter()
        .filter_map(|row| matches!(row.kind, StatusKind::Untracked).then_some(row.path.as_str()))
        .collect();
    assert_eq!(untracked, vec!["notes.txt"], "only the genuine user file");
    assert!(
        !status.clean,
        "an untracked row is dirt (all-rows-always semantics)"
    );
    let rows = rows_by_path(&status);
    assert_eq!(rows["notes.txt"], &StatusKind::Untracked);
}

// ---- Refusals + envelope shapes ----------------------------------------------

/// Statusing a foreign project refuses, naming both projects.
#[tokio::test]
async fn status_refuses_foreign_project() {
    let (target, _zip) = checkout_baseline().await;
    let server = wiremock::MockServer::start().await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let err = workspace_status(target.path(), &api, "other")
        .await
        .expect_err("refuses");
    assert!(matches!(
        err,
        ignition_core::error::CoreError::InvalidInput { .. }
    ));
    let message = err.to_string();
    assert!(
        message.contains("\"demo\"") && message.contains("\"other\""),
        "{message}"
    );
}

/// Statusing a non-workspace directory surfaces 13-03's stable
/// missing-manifest prefix (the 13-07 golden anchor).
#[tokio::test]
async fn status_on_non_workspace_refuses_with_stable_prefix() {
    let root = tempfile::tempdir().expect("root");
    let server = wiremock::MockServer::start().await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let err = workspace_status(root.path(), &api, "demo")
        .await
        .expect_err("refuses");
    assert!(
        err.to_string()
            .contains("not an ign workspace — run `ign workspace checkout` first"),
        "{}",
        err
    );
}

/// Envelope shapes (additive-only discipline): exactly the three
/// status keys, exactly the two row keys, snake_case kinds with
/// payload objects and no nulls anywhere.
#[test]
fn envelope_shapes_are_additive_and_snake_case() {
    let status = WorkspaceStatus {
        project: "p".to_string(),
        clean: false,
        rows: vec![
            StatusRow {
                path: "a".to_string(),
                kind: StatusKind::LocalEdit,
            },
            StatusRow {
                path: "b".to_string(),
                kind: StatusKind::Deleted { local: true },
            },
            StatusRow {
                path: "c".to_string(),
                kind: StatusKind::Added { local: false },
            },
            StatusRow {
                path: "d".to_string(),
                kind: StatusKind::Untracked,
            },
        ],
    };
    let value = serde_json::to_value(&status).expect("serializes");
    let mut keys: Vec<_> = value.as_object().expect("object").keys().cloned().collect();
    keys.sort();
    assert_eq!(keys, vec!["clean", "project", "rows"]);
    assert_eq!(value["rows"][0]["kind"], serde_json::json!("local_edit"));
    assert_eq!(
        value["rows"][1]["kind"],
        serde_json::json!({"deleted": {"local": true}})
    );
    assert_eq!(
        value["rows"][2]["kind"],
        serde_json::json!({"added": {"local": false}})
    );
    assert_eq!(value["rows"][3]["kind"], serde_json::json!("untracked"));
    let row = &value["rows"][0];
    let mut row_keys: Vec<_> = row
        .as_object()
        .expect("row object")
        .keys()
        .cloned()
        .collect();
    row_keys.sort();
    assert_eq!(row_keys, vec!["kind", "path"]);
}

/// A pristine checkout statuses CLEAN — the same-content re-export
/// hashes identically through the recorded manifest (the no-drift
/// control for the every-kind test).
#[tokio::test]
async fn pristine_checkout_statuses_clean() {
    let (target, zip) = checkout_baseline().await;
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/data/api/v1/projects/export/demo",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_raw(zip.clone(), "application/zip"),
        )
        .mount(&server)
        .await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let status = workspace_status(target.path(), &api, "demo")
        .await
        .expect("status");
    assert!(status.clean, "{:?}", status.rows);
    assert_eq!(status.rows.len(), 6, "one row per manifest member");
    assert!(
        status
            .rows
            .iter()
            .all(|row| matches!(row.kind, StatusKind::Clean))
    );
}
