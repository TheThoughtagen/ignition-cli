//! Contract tests for `ign workspace checkout`'s core (13-03): the
//! manifest-recorded, mapped checkout over a wiremock-served REAL
//! export (the resources_contract transport pattern — export fires,
//! zero imports: checkout is read-only on the wire).
//!
//! THE crown pins:
//! - the unedited `--decode-scripts` checkout re-encodes through
//!   [`ignition_core::client::scripts_codec::encode_export_tree`]
//!   with EVERY member byte-identical to the source zip
//!   (`assert_eq!` on raw member bytes — the codec's sacred
//!   invariant proven at tree scale, the roadmap's precondition for
//!   `ign edit` riding the same codec leg);
//! - the tree contains EXACTLY the zip-derived member set + the
//!   workspace's own files (+ codec artifacts when decoding) —
//!   SC-2's tag-value exclusion clause, structurally pinned because
//!   checkout ingests ONLY the project export zip;
//! - clobber safety: an unrelated non-empty target refuses before
//!   ANY write; a foreign manifest refuses naming both projects; a
//!   case-collision fixture propagates 13-02's refusal with BOTH
//!   member names.
//!
//! Descriptor-normalized hashing is pinned LIVE here: two exports
//! differing ONLY in `attributes.lastModification{time,…Signature}`
//! record IDENTICAL manifest hashes for the descriptor member,
//! while a real content change in a non-descriptor member moves its
//! hash — the volatility guard, end to end at the manifest.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read as _, Write as _};
use std::path::Path;

use ignition_core::actions::workspace::{
    CheckoutOutcome, WORKSPACE_MANIFEST_NAME, read_manifest, workspace_checkout,
};
use ignition_core::client::ReqwestGatewayApi;
use ignition_core::client::resources::{read_member, resource_members};
use ignition_core::client::scripts_codec;
use ignition_core::client::workspace::build_mapping;

// ---- Fixtures (project-export-shaped ONLY — never tag data) ---------------

/// Build a small export zip: `project.json` + one member per pair,
/// in order (the same zip crate the surgery rides — honest
/// fixtures). `project.json` is the export's project metadata, NOT a
/// resource member (`resource_members` skips it) and is never
/// written into the workspace tree.
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

/// A Perspective view member with ONE embedded python script (the
/// Flint-escaped form: tabs, newlines, the HTML five) — the codec
/// leg's live exercise.
const VIEW_MEMBER: &str = "com.example/resources/views/Dashboard/view.json";
const VIEW_USER: &str = "com.example/views/Dashboard/view.json";
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

/// The view folder's descriptor — carries the
/// `attributes.lastModification` volatility noise the hash must
/// normalize away.
const DESC_MEMBER: &str = "com.example/resources/views/Dashboard/resource.json";
const DESC_USER: &str = "com.example/views/Dashboard/resource.json";

fn desc_json(time_ms: i64, signature: &str) -> String {
    format!(
        r#"{{"scope":"G","version":1,"restricted":false,"overridable":true,"files":["view.json"],"attributes":{{"lastModification":{{"time":{time_ms},"nano":0}},"lastModificationSignature":"{signature}"}}}}"#
    )
}

/// A nested plain member (nested folders in the fixture) whose
/// content is the negative hash control.
const NESTED_MEMBER: &str = "com.example/resources/views/Nested/Deep/other.json";
const NESTED_USER: &str = "com.example/views/Nested/Deep/other.json";

fn nested_json(value: u32) -> String {
    format!(r#"{{"scope":"G","value":{value}}}"#)
}

/// A plain script-python member (already `.py` text — never decodes).
const SCRIPT_MEMBER: &str = "ignition/resources/script-python/e2e/scratch";
const SCRIPT_USER: &str = "ignition/script-python/e2e/scratch";

fn export_zip(time_ms: i64, signature: &str, nested_value: u32) -> Vec<u8> {
    fixture_zip(&[
        (VIEW_MEMBER, VIEW_JSON.as_bytes()),
        (DESC_MEMBER, desc_json(time_ms, signature).as_bytes()),
        (NESTED_MEMBER, nested_json(nested_value).as_bytes()),
        (SCRIPT_MEMBER, b"print('hi')\n".as_slice()),
    ])
}

/// The expected checkout mapping for the fixture — through the REAL
/// mapping (which is the identity here: every fixture name rides the
/// safe alphabet, diffable and editor-friendly).
fn expected_mapping(zip: &[u8]) -> BTreeMap<String, std::path::PathBuf> {
    build_mapping(&resource_members(zip).expect("members")).expect("mapping")
}

// ---- Transport -------------------------------------------------------------

async fn mount_export(server: &wiremock::MockServer, project: &str, zip: Vec<u8>) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(format!(
            "/data/api/v1/projects/export/{project}"
        )))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_raw(zip, "application/zip"))
        .expect(1) // checkout is read-only: exactly ONE export, ZERO imports
        .mount(server)
        .await;
}

/// Run a checkout against its own wiremock server; returns the
/// target dir (kept alive by the TempDir) and the outcome.
async fn checkout(
    project: &str,
    zip: Vec<u8>,
    decode: bool,
) -> (tempfile::TempDir, CheckoutOutcome) {
    let server = wiremock::MockServer::start().await;
    mount_export(&server, project, zip).await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let target = tempfile::tempdir().expect("target tempdir");
    let outcome = workspace_checkout(&api, project, target.path(), "test-profile", decode)
        .await
        .expect("checkout Ok");
    (target, outcome)
}

// ---- Tree/zip helpers ------------------------------------------------------

/// Every regular file under `root`, as `/`-separated relative paths.
fn walk_tree(root: &Path) -> BTreeSet<String> {
    fn walk(dir: &Path, prefix: &str, out: &mut BTreeSet<String>) {
        for entry in std::fs::read_dir(dir).expect("read_dir") {
            let entry = entry.expect("entry");
            let name = entry.file_name().to_string_lossy().into_owned();
            let rel = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if entry.path().is_dir() {
                walk(&entry.path(), &rel, out);
            } else {
                out.insert(rel);
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(root, "", &mut out);
    out
}

fn open_zip(bytes: &[u8]) -> zip::ZipArchive<std::io::Cursor<Vec<u8>>> {
    zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).expect("zip opens")
}

fn zip_member_bytes(
    archive: &mut zip::ZipArchive<std::io::Cursor<Vec<u8>>>,
    name: &str,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    archive
        .by_name(name)
        .unwrap_or_else(|_| panic!("member {name} present"))
        .read_to_end(&mut bytes)
        .expect("member reads");
    bytes
}

/// Temporarily move the two workspace-owned root files out of the
/// tree so [`scripts_codec::encode_export_tree`] (which zips EVERY
/// remaining file) reconstructs exactly the member set. The test
/// puts them back in spirit by dropping both tempdirs.
fn set_aside_workspace_files(target: &Path, aside: &Path) {
    std::fs::rename(target.join(".gitignore"), aside.join(".gitignore")).expect("aside gitignore");
    std::fs::rename(
        target.join(WORKSPACE_MANIFEST_NAME),
        aside.join(WORKSPACE_MANIFEST_NAME),
    )
    .expect("aside manifest");
}

// ---- The crown: byte-exact unedited round-trip at tree scale ---------------

/// THE pinned invariant (must_have truth 2): an UNEDITED
/// `--decode-scripts` checkout encodes back — through the codec's
/// own `encode_export_tree` — with EVERY member byte-identical to
/// the source zip. The assert is `assert_eq!` on RAW member bytes:
/// byte-exact, not structural.
#[tokio::test]
async fn unedited_decode_checkout_round_trips_byte_exact() {
    let zip = export_zip(1_757_904_000_000, "sig-a", 42);
    let (target, outcome) = checkout("demo", zip.clone(), true).await;
    assert_eq!(outcome.member_count, 4);
    assert!(outcome.scripts_decoded);

    let aside = tempfile::tempdir().expect("aside");
    set_aside_workspace_files(target.path(), aside.path());

    let re_zipped = scripts_codec::encode_export_tree(target.path()).expect("re-encodes");
    let mut re = open_zip(&re_zipped);

    // The re-zip is EXACTLY the member set: no workspace files, no
    // codec manifest, no sidecars.
    assert_eq!(
        re.len() as usize,
        outcome.member_count,
        "the re-encode carries exactly the member set"
    );
    assert!(re.by_name(scripts_codec::MANIFEST_NAME).is_err());
    assert!(re.by_name(".gitignore").is_err());
    assert!(re.by_name(WORKSPACE_MANIFEST_NAME).is_err());
    assert!(
        re.by_name("com.example/views/Dashboard/view.json.1.py")
            .is_err()
    );

    // ⚖ THE byte-exact assert — every member's bytes equal the
    // source zip's bytes (raw `assert_eq!` on Vec<u8>, no hashes,
    // no normalization, no structural equivalence).
    for (user, local) in expected_mapping(&zip) {
        let expected = read_member(&zip, &user).expect("source member");
        let name = local.to_string_lossy().into_owned();
        let actual = zip_member_bytes(&mut re, &name);
        assert_eq!(actual, expected, "BYTE-EXACT member {user}");
    }
}

// ---- Tree shapes ------------------------------------------------------------

/// `--decode-scripts` produces the sidecar + codec-manifest shape at
/// the MAPPED paths; plain checkout writes raw member bytes only.
#[tokio::test]
async fn decode_tree_shape_vs_plain_checkout() {
    let zip = export_zip(1_757_904_000_000, "sig-a", 42);
    let (target, outcome) = checkout("demo", zip.clone(), true).await;

    // The sidecar beside the member's MAPPED path, decoded + dedented.
    let sidecar = target
        .path()
        .join("com.example/views/Dashboard/view.json.1.py");
    let sidecar_text = std::fs::read_to_string(&sidecar).expect("sidecar exists");
    assert_eq!(
        sidecar_text, "print 'clicked'\nprint 'done <>&='",
        "nvim-editable: decoded, dedented python"
    );

    // The codec manifest keys the member by its MAPPED tree-relative
    // path (what encode_export_tree resolves).
    let codec_manifest: scripts_codec::Manifest = serde_json::from_str(
        &std::fs::read_to_string(target.path().join(scripts_codec::MANIFEST_NAME))
            .expect("codec manifest exists"),
    )
    .expect("parses");
    let entries = codec_manifest
        .members
        .get("com.example/views/Dashboard/view.json")
        .expect("the view member is keyed");
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].pointer,
        "/children/0/eventScripts/actionPerformed/config/script"
    );
    assert_eq!(entries[0].sidecar, "view.json.1.py");
    assert_eq!(entries[0].indent_prefix, "\t");

    // The workspace manifest records pairs + hashes + identity.
    let manifest = read_manifest(target.path()).expect("workspace manifest reads");
    assert_eq!(manifest.project, "demo");
    assert_eq!(manifest.profile, "test-profile");
    assert!(manifest.checked_out_at.ends_with('Z'));
    assert_eq!(manifest.members.len(), 4);
    for (user, local) in expected_mapping(&zip) {
        let recorded = &manifest.members[&user];
        assert_eq!(
            recorded.local_path,
            local.to_string_lossy().into_owned(),
            "the recorded pair matches the mapping output verbatim"
        );
    }
    assert!(manifest.members.contains_key(DESC_USER));

    // Outcome envelope shape (13-07's additive-only discipline).
    let value = serde_json::to_value(&outcome).expect("serializes");
    let mut keys: Vec<_> = value.as_object().expect("object").keys().cloned().collect();
    keys.sort();
    assert_eq!(
        keys,
        vec!["member_count", "project", "scripts_decoded", "target"],
        "exactly the four outcome keys (serde_json sorts object keys)"
    );

    // The plain checkout: raw member bytes, NO codec artifacts.
    let (plain, plain_outcome) = checkout("demo", zip.clone(), false).await;
    assert!(!plain_outcome.scripts_decoded);
    for (user, local) in expected_mapping(&zip) {
        let on_disk = std::fs::read(plain.path().join(&local)).expect("member file");
        assert_eq!(on_disk, read_member(&zip, &user).expect("source"));
    }
    let tree = walk_tree(plain.path());
    assert!(
        !tree.iter().any(|path| path.ends_with(".py")),
        "no sidecars without --decode-scripts: {tree:?}"
    );
    assert!(!tree.contains(scripts_codec::MANIFEST_NAME));
}

/// SC-2's tag-value exclusion clause, structurally pinned: the tree
/// contains EXACTLY the zip-derived members + the workspace's own
/// files (+ codec artifacts when decoding) — nothing else. Checkout
/// ingests ONLY the project export zip, so no tag value CAN appear.
#[tokio::test]
async fn tree_contains_exactly_the_zip_derived_set() {
    let zip = export_zip(1_757_904_000_000, "sig-a", 42);

    // decode=true: members + sidecar + codec manifest + the two
    // workspace files.
    let (target, _) = checkout("demo", zip.clone(), true).await;
    let mut expected: BTreeSet<String> = expected_mapping(&zip)
        .values()
        .map(|local| local.to_string_lossy().into_owned())
        .collect();
    expected.insert("com.example/views/Dashboard/view.json.1.py".to_string());
    expected.insert(scripts_codec::MANIFEST_NAME.to_string());
    expected.insert(".gitignore".to_string());
    expected.insert(WORKSPACE_MANIFEST_NAME.to_string());
    assert_eq!(
        walk_tree(target.path()),
        expected,
        "EXACT tree set (decode) — nothing beyond zip members + workspace files"
    );

    // decode=false: members + the two workspace files, nothing else.
    let (plain, _) = checkout("demo", zip.clone(), false).await;
    let mut expected: BTreeSet<String> = expected_mapping(&zip)
        .values()
        .map(|local| local.to_string_lossy().into_owned())
        .collect();
    expected.insert(".gitignore".to_string());
    expected.insert(WORKSPACE_MANIFEST_NAME.to_string());
    assert_eq!(
        walk_tree(plain.path()),
        expected,
        "EXACT tree set (plain) — nothing beyond zip members + workspace files"
    );
}

// ---- Hash semantics ----------------------------------------------------------

/// Descriptor-normalized hashing, end to end: two exports differing
/// ONLY in `attributes.lastModification` record IDENTICAL manifest
/// hashes for the descriptor member; a real content change in a
/// non-descriptor member moves its hash.
#[tokio::test]
async fn manifest_hashes_normalize_descriptor_volatility() {
    let zip_a = export_zip(1_757_904_000_000, "sig-a", 42);
    let zip_b = export_zip(1_757_990_400_000, "sig-b", 42); // ONLY the volatility noise differs
    let (target_a, _) = checkout("demo", zip_a.clone(), false).await;
    let (target_b, _) = checkout("demo", zip_b, false).await;
    let manifest_a = read_manifest(target_a.path()).expect("a");
    let manifest_b = read_manifest(target_b.path()).expect("b");

    assert_eq!(
        manifest_a.members[DESC_USER].hash, manifest_b.members[DESC_USER].hash,
        "descriptor hash ignores lastModification time/signature"
    );
    assert_eq!(
        manifest_a.members[VIEW_USER].hash, manifest_b.members[VIEW_USER].hash,
        "unchanged bytes hash identically"
    );

    // Negative control: a REAL content change moves the hash.
    let zip_c = export_zip(1_757_904_000_000, "sig-a", 43);
    let (target_c, _) = checkout("demo", zip_c, false).await;
    let manifest_c = read_manifest(target_c.path()).expect("c");
    assert_ne!(
        manifest_a.members[NESTED_USER].hash, manifest_c.members[NESTED_USER].hash,
        "a content change is drift, not noise"
    );
}

// ---- Edit splice (the edit pipeline's leg, structurally proven) -------------

/// Editing ONE sidecar re-splices only that member; every other
/// member re-encodes byte-identical.
#[tokio::test]
async fn edited_sidecar_ressplices_only_the_edited_member() {
    let zip = export_zip(1_757_904_000_000, "sig-a", 42);
    let (target, _) = checkout("demo", zip.clone(), true).await;

    let sidecar = target
        .path()
        .join("com.example/views/Dashboard/view.json.1.py");
    std::fs::write(&sidecar, "print 'edited'\nprint 'twice'").expect("edit");

    let aside = tempfile::tempdir().expect("aside");
    set_aside_workspace_files(target.path(), aside.path());
    let re_zipped = scripts_codec::encode_export_tree(target.path()).expect("re-encodes");
    let mut re = open_zip(&re_zipped);

    // The edited member carries the new script under the recorded
    // indent prefix. (The re-zip member name is the MAPPED path —
    // the user path — not the raw zip member name.)
    let view = zip_member_bytes(&mut re, VIEW_USER);
    let parsed: serde_json::Value = serde_json::from_slice(&view).expect("still JSON");
    assert_eq!(
        parsed["children"][0]["eventScripts"]["actionPerformed"]["config"]["script"],
        "\tprint 'edited'\n\tprint 'twice'",
        "the edit re-splices under the recorded prefix"
    );

    // Every OTHER member is byte-identical to the source.
    let mapping = expected_mapping(&zip);
    for user in [DESC_USER, NESTED_USER, SCRIPT_USER] {
        let expected = read_member(&zip, user).expect("source member");
        let actual = zip_member_bytes(&mut re, mapping[user].to_str().expect("utf-8 path"));
        assert_eq!(actual, expected, "unedited member {user} rides byte-exact");
    }
}

// ---- Refusals (clobber safety) ------------------------------------------------

/// An unrelated non-empty target refuses BEFORE any write; the
/// pre-existing file is untouched. (The export still happens — the
/// plan's order is export → refusal-before-any-write.)
#[tokio::test]
async fn non_empty_unrelated_target_refuses_without_clobber() {
    let server = wiremock::MockServer::start().await;
    let zip = export_zip(1_757_904_000_000, "sig-a", 42);
    mount_export(&server, "demo", zip).await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);

    let target = tempfile::tempdir().expect("target");
    std::fs::write(target.path().join("keep.txt"), b"precious").expect("seed");
    let err = workspace_checkout(&api, "demo", target.path(), "test-profile", false)
        .await
        .expect_err("refuses");
    let message = err.to_string();
    assert!(
        message.contains("not empty and is not an ign workspace"),
        "stable refusal phrase: {message}"
    );
    assert!(
        message.contains("keep.txt") || message.contains(target.path().to_str().unwrap()),
        "names the directory: {message}"
    );
    assert_eq!(
        std::fs::read(target.path().join("keep.txt")).expect("still there"),
        b"precious",
        "nothing clobbered"
    );
}

/// A foreign manifest refuses re-checkout, naming both projects;
/// the existing workspace is untouched.
#[tokio::test]
async fn foreign_manifest_refuses_recheckout() {
    let (target, _) = checkout("p1", export_zip(1, "s", 42), false).await;

    let server = wiremock::MockServer::start().await;
    mount_export(&server, "p2", export_zip(1, "s", 42)).await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let err = workspace_checkout(&api, "p2", target.path(), "test-profile", false)
        .await
        .expect_err("refuses");
    let message = err.to_string();
    assert!(
        message.contains("\"p1\"") && message.contains("\"p2\""),
        "names both projects: {message}"
    );
    assert_eq!(
        read_manifest(target.path())
            .expect("manifest survives")
            .project,
        "p1",
        "the existing workspace is untouched"
    );
}

/// Re-checkout over one's OWN workspace is allowed (13-06's refresh
/// semantics) and stays idempotent (the .gitignore is not re-written).
#[tokio::test]
async fn same_project_recheckout_refreshes() {
    let (target, _) = checkout("demo", export_zip(1, "s", 42), true).await;
    let gitignore_before =
        std::fs::read_to_string(target.path().join(".gitignore")).expect("gitignore");

    let server = wiremock::MockServer::start().await;
    mount_export(&server, "demo", export_zip(1, "s", 42)).await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let outcome = workspace_checkout(&api, "demo", target.path(), "test-profile", true)
        .await
        .expect("same-project re-checkout is allowed");
    assert_eq!(outcome.member_count, 4);

    assert_eq!(
        std::fs::read_to_string(target.path().join(".gitignore")).expect("gitignore"),
        gitignore_before,
        "the .gitignore write is idempotent"
    );
    assert_eq!(
        read_manifest(target.path()).expect("manifest").project,
        "demo"
    );
}

/// 13-02's set-level case-collision refusal propagates VERBATIM,
/// naming BOTH members — and nothing is written.
#[tokio::test]
async fn case_collision_refusal_propagates_with_both_names() {
    let zip = fixture_zip(&[
        ("P13/resources/A", b"x".as_slice()),
        ("p13/resources/a", b"y".as_slice()),
    ]);
    let server = wiremock::MockServer::start().await;
    mount_export(&server, "demo", zip).await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let target = tempfile::tempdir()
        .expect("target tempdir")
        .path()
        .join("never-created");
    let err = workspace_checkout(&api, "demo", &target, "test-profile", false)
        .await
        .expect_err("refuses");
    let message = err.to_string();
    assert!(
        message.contains("P13/A") && message.contains("p13/a"),
        "names BOTH colliding members: {message}"
    );
    assert!(
        !target.exists(),
        "the refusal precedes ANY write — the target is never created"
    );
}
