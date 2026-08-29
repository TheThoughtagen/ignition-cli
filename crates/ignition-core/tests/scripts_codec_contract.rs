//! PURE contract for the Flint script codec's tree round-trip
//! (07-04, INTR-01) — no gateway, no wiremock: the fixture zip is
//! the input, the byte equality is the oracle.
//!
//! THE crown pin: decode → encode with NO edits → every file member
//! BYTE-IDENTICAL (the sacred invariant at file level — the splice
//! approach's acceptance, 07-RESEARCH Pitfall 4). Around it: edit
//! one sidecar → only that value changed; missing sidecar → value
//! preserved; the manifest and sidecars never ride the re-zip; the
//! scope honesty (script-python members and expression values pass
//! through untouched).

use std::collections::HashMap;
use std::io::{Read, Write};

use ignition_core::client::scripts_codec::{
    self, MANIFEST_NAME, ManifestEntry, decode_export_tree, decode_member, encode_export_tree,
    encode_member,
};

/// A live-shaped export fixture: `project.json`, a Perspective view
/// member with TWO embedded scripts (different indent depths) + an
/// `expression` value that must PASS THROUGH, the view folder's
/// `resource.json` descriptor, and a plain `script-python` member
/// (already editable text — never decoded). Script strings carry the
/// GATEWAY-IMAGE escapes (`'` → `\u0027`, `<>&=` → `\u00XX`) — what
/// Ignition's writer actually emits, which is what byte-exact
/// round-tripping depends on.
fn fixture_zip() -> Vec<u8> {
    let view: &[u8] = br#"{
  "scope" : "G",
  "children" : [ {
    "type" : "ia.display.label",
    "meta" : {
      "name" : "lbl"
    },
    "props" : {
      "text" : "plain text with <>&=' needs no decode"
    },
    "eventScripts" : {
      "actionPerformed" : {
        "config" : {
          "script" : "\tprint \u0027clicked\u0027\n\tprint \u0027done \u003c\u003e\u0026\u003d\u0027"
        }
      }
    }
  }, {
    "type" : "ia.chart",
    "transform" : {
      "script" : "\t\tfor i in range(3):\n\t\t\tprint i\n\t\tprint \u0027end\u0027"
    },
    "props" : {
      "expression" : "toStr({view.args.x} * 2)"
    }
  } ]
}"#;
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let members: &[(&str, &[u8])] = &[
        ("project.json", br#"{"title":"Fixture","enabled":true}"#),
        (
            "com.inductiveautomation.perspective/resources/views/Dashboard/view.json",
            view,
        ),
        (
            "com.inductiveautomation.perspective/resources/views/Dashboard/resource.json",
            br#"{"scope":"G","version":1,"restricted":false,"overridable":true,"files":["view.json"],"attributes":{"lastModification":{"actor":"admin","timestamp":"2026-08-28T10:00:00Z","signature":"sig-a"},"lastModificationSignature":"sig-a"}}"#,
        ),
        (
            "ignition/resources/script-python/e2e/hello2",
            b"print('plain script-python member')",
        ),
    ];
    for (name, bytes) in members {
        writer.start_file(*name, options).expect("member starts");
        writer.write_all(bytes).expect("member writes");
    }
    writer.finish().expect("zip finalizes").into_inner()
}

/// Read every FILE member of a zip into a name→bytes map.
fn members_of(zip: &[u8]) -> std::collections::BTreeMap<String, Vec<u8>> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).expect("zip opens");
    let mut out = std::collections::BTreeMap::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).expect("member");
        if file.is_dir() {
            continue;
        }
        let name = file.name().to_string();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).expect("member reads");
        out.insert(name, bytes);
    }
    out
}

/// THE sacred invariant at file level: decode → encode with NO edits
/// → every member byte-identical, the same member set, and the
/// manifest + sidecars never ride the re-zip.
#[test]
fn unedited_round_trip_is_byte_identical_per_member() {
    let zip = fixture_zip();
    let dir = tempfile::tempdir().expect("tempdir");
    let scripts = decode_export_tree(&zip, dir.path()).expect("decodes");
    assert_eq!(scripts, 2, "the two embedded scripts (expression excluded)");

    // The tree shape: members at their paths, sidecars as siblings,
    // the manifest at the root.
    let view_dir = dir
        .path()
        .join("com.inductiveautomation.perspective/resources/views/Dashboard");
    assert!(view_dir.join("view.json").is_file());
    assert!(view_dir.join("view.json.1.py").is_file());
    assert!(view_dir.join("view.json.2.py").is_file());
    assert!(dir.path().join(MANIFEST_NAME).is_file());
    // The sidecar text is decoded + DEDENTED.
    assert_eq!(
        std::fs::read_to_string(view_dir.join("view.json.1.py")).expect("sidecar"),
        "print 'clicked'\nprint 'done <>&='"
    );
    assert_eq!(
        std::fs::read_to_string(view_dir.join("view.json.2.py")).expect("sidecar"),
        "for i in range(3):\n\tprint i\nprint 'end'"
    );
    // The manifest maps JSON-pointer addresses → sidecar + prefix.
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join(MANIFEST_NAME)).expect("manifest"),
    )
    .expect("manifest json");
    let entries = &manifest["members"]["com.inductiveautomation.perspective/resources/views/Dashboard/view.json"];
    assert_eq!(
        entries[0]["pointer"],
        "/children/0/eventScripts/actionPerformed/config/script"
    );
    assert_eq!(entries[0]["sidecar"], "view.json.1.py");
    assert_eq!(entries[0]["indent_prefix"], "\t");
    assert_eq!(entries[1]["pointer"], "/children/1/transform/script");
    assert_eq!(entries[1]["indent_prefix"], "\t\t");

    // The re-encode: member set unchanged, every member byte-equal.
    let re_zipped = encode_export_tree(dir.path()).expect("re-encodes");
    let before = members_of(&zip);
    let after = members_of(&re_zipped);
    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        after.keys().collect::<Vec<_>>(),
        "the same member set (manifest + sidecars stripped)"
    );
    for (name, original) in &before {
        assert_eq!(
            after.get(name).expect("member present"),
            original,
            "{name} byte-identical through the unedited round-trip"
        );
    }
}

/// Editing ONE sidecar changes only that value: the view member
/// differs (and parses, the new text re-dented under its recorded
/// prefix), every OTHER member stays byte-identical.
#[test]
fn editing_one_sidecar_splices_only_that_value() {
    let zip = fixture_zip();
    let dir = tempfile::tempdir().expect("tempdir");
    decode_export_tree(&zip, dir.path()).expect("decodes");
    let sidecar = dir
        .path()
        .join("com.inductiveautomation.perspective/resources/views/Dashboard/view.json.1.py");
    std::fs::write(&sidecar, "print 'edited'\nif 1 < 2:\n\tprint 'spliced'").expect("edit");

    let re_zipped = encode_export_tree(dir.path()).expect("re-encodes");
    let before = members_of(&zip);
    let after = members_of(&re_zipped);
    let view_name = "com.inductiveautomation.perspective/resources/views/Dashboard/view.json";
    for (name, original) in &before {
        if name == view_name {
            continue;
        }
        assert_eq!(
            after.get(name).expect("member"),
            original,
            "{name} untouched by the sibling edit"
        );
    }
    let view = after.get(view_name).expect("view present");
    let parsed: serde_json::Value = serde_json::from_slice(view).expect("still valid JSON");
    assert_eq!(
        parsed["children"][0]["eventScripts"]["actionPerformed"]["config"]["script"],
        "\tprint 'edited'\n\tif 1 < 2:\n\t\tprint 'spliced'",
        "the edit re-dents under the recorded tab prefix and re-escapes (< → \u{3c} in the raw bytes)"
    );
    // The raw bytes really carry the Flint escapes.
    let raw = String::from_utf8_lossy(view);
    assert!(
        raw.contains("print \\u0027edited\\u0027"),
        "apostrophes re-escape"
    );
    assert!(raw.contains("1 \\u003c 2"), "less-than re-escapes");
    // The unedited sibling value is content-identical.
    assert_eq!(
        parsed["children"][1]["transform"]["script"],
        "\t\tfor i in range(3):\n\t\t\tprint i\n\t\tprint 'end'"
    );
}

/// A missing sidecar keeps the JSON's current value (never silently
/// drop edits) — with ALL sidecars missing the member round-trips
/// byte-identically.
#[test]
fn missing_sidecar_keeps_the_current_value() {
    let zip = fixture_zip();
    let dir = tempfile::tempdir().expect("tempdir");
    decode_export_tree(&zip, dir.path()).expect("decodes");
    let view_dir = dir
        .path()
        .join("com.inductiveautomation.perspective/resources/views/Dashboard");
    std::fs::remove_file(view_dir.join("view.json.1.py")).expect("delete one sidecar");
    std::fs::remove_file(view_dir.join("view.json.2.py")).expect("delete the other");

    let re_zipped = encode_export_tree(dir.path()).expect("re-encodes");
    let before = members_of(&zip);
    let after = members_of(&re_zipped);
    for (name, original) in &before {
        assert_eq!(
            after.get(name).expect("member"),
            original,
            "{name} preserved with its sidecars gone"
        );
    }
}

/// Scope honesty: the script-python member's bytes pass through
/// verbatim (never decoded — it is already plain text), the
/// expression value never lands a sidecar, and a hand-edited member
/// (spacing changed elsewhere) still splices at the RE-RESOLVED span.
#[test]
fn scope_and_hand_edit_reresolution() {
    // The script-python member and the expression pass through —
    // proven by the byte-identity above; here the hand-edit half:
    // pretty-print the view differently (a cosmetic reformat), then
    // splice via the ORIGINAL manifest pointers — the scanner
    // re-resolves spans in the CURRENT bytes.
    let zip = fixture_zip();
    let dir = tempfile::tempdir().expect("tempdir");
    decode_export_tree(&zip, dir.path()).expect("decodes");
    let view_path = dir
        .path()
        .join("com.inductiveautomation.perspective/resources/views/Dashboard/view.json");
    // Hand-edit: reformat the JSON compactly (serde parse→string;
    // READ-ONLY discipline note: this is the test's hand-edit, not a
    // codec code path — the codec never re-serializes a Value).
    let parsed: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&view_path).expect("view")).expect("json");
    std::fs::write(
        &view_path,
        serde_json::to_string_pretty(&parsed).expect("pretty"),
    )
    .expect("rewrite");

    let re_zipped = encode_export_tree(dir.path()).expect("re-encodes");
    let after = members_of(&re_zipped);
    let view = after
        .get("com.inductiveautomation.perspective/resources/views/Dashboard/view.json")
        .expect("view");
    let reparsed: serde_json::Value = serde_json::from_slice(view).expect("valid");
    assert_eq!(
        reparsed["children"][0]["eventScripts"]["actionPerformed"]["config"]["script"],
        "\tprint 'clicked'\n\tprint 'done <>&='",
        "the unedited sidecar's value still lands at the re-resolved span"
    );
    // The script-python member rides byte-verbatim through it all.
    assert_eq!(
        after
            .get("ignition/resources/script-python/e2e/hello2")
            .expect("scratch"),
        b"print('plain script-python member')"
    );
}

/// The member-level API is independently consistent with the tree
/// wrappers (decode_member's entries splice back through
/// encode_member onto the ORIGINAL bytes byte-identically).
#[test]
fn member_level_and_tree_level_agree() {
    let zip = fixture_zip();
    let view_name = "com.inductiveautomation.perspective/resources/views/Dashboard/view.json";
    let view_bytes = members_of(&zip).get(view_name).expect("view").clone();
    let decoded = decode_member(&view_bytes, view_name).expect("decodes");
    let entries: Vec<ManifestEntry> = decoded.entries.iter().map(|e| e.entry.clone()).collect();
    let texts: HashMap<String, String> = decoded
        .entries
        .iter()
        .map(|e| (e.entry.sidecar.clone(), e.text.clone()))
        .collect();
    let out = encode_member(&view_bytes, &entries, &texts).expect("encodes");
    assert_eq!(out, view_bytes, "member-level unedited round-trip is exact");
}

/// The public-surface sanity: the module exports the codec and the
/// wrappers the CLI rides (compile-time presence, no behavior).
#[test]
fn module_surface() {
    let _ = scripts_codec::flint_encode("x");
    let _ = scripts_codec::flint_decode("x");
    let _ = scripts_codec::SCRIPT_KEYS.len();
}
