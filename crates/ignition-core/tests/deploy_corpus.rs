//! Byte-fidelity gate for the deploy render (Phase 21, plan 21-05, DRND-05).
//!
//! Three tests over committed fixtures:
//!
//! - `corpus_gson_census`: of the 183 sanitized corpus `.json` files, 178
//!   satisfy `write(parse(bytes)) == bytes` and exactly 5 are opaque.
//! - `captured_tag_files_follow_cd4`: the twelve gateway-written tag files
//!   captured on the 8.3.6 rig are all gateway-format (decision CD4 in
//!   21-LIVE-CAPTURES.md).
//! - `corpus_render_is_byte_identical`: rendering the sanitized corpus
//!   returns every file byte-for-byte.
//!
//! Cross-platform: this file runs in `cargo test --workspace` on every leg of
//! the CI matrix (`.github/workflows/ci.yml`: `os: [ubuntu-latest,
//! macos-latest, windows-latest]`). It uses `std::fs` only, builds in-tree
//! paths with `/` inside `RelPath`, and joins OS paths with `Path::join`.
//! `.gitattributes` marks `fixtures/deploy/**` as `-text`, so Windows checkouts
//! keep the bytes.

use std::collections::BTreeSet;
use std::path::Path;

use ignition_core::deploy::manifest::Manifest;
use ignition_core::deploy::node::{JsonClass, classify};
use ignition_core::deploy::render::{RenderInput, render};
use ignition_core::deploy::target::Target;
use ignition_core::deploy::tree::SourceTree;

const CORPUS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/deploy/corpus");
const CAPTURED: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/deploy/captured"
);

const TARGET: &[u8] =
    b"schema_version: 1\ngateway: g\nmode: dev\nnodes:\n  master:\n    profile: dev\n";

/// The corpus files that are not in the gateway's writer format (RESEARCH
/// Pattern 1, re-verified by `corpus_gson_census`): `config-mode.json` and
/// `security-properties/config.json` end in a newline; the other three are
/// compact one-liners.
const CORPUS_OPAQUE: [&str; 5] = [
    "com.cirruslink.mqtt.engine.gateway/namespace-server-set/Elecsys-Default Set/config.json",
    "com.cirruslink.mqtt.engine.gateway/namespace-server-set/Sparkplug B-Default Set/config.json",
    "config-mode.json",
    "ignition/quickstart/config.json",
    "ignition/security-properties/config.json",
];

/// Every regular file under `root` as (`/`-joined relative path, bytes),
/// byte-sorted by path.
fn read_tree(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, Vec<u8>)>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
            .map(|e| e.expect("dir entry"))
            .collect();
        entries.sort_by_key(|e| e.file_name().into_encoded_bytes());
        for entry in entries {
            let name = entry.file_name().into_string().expect("UTF-8 fixture name");
            let rel = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let kind = entry.file_type().expect("file type");
            if kind.is_dir() {
                walk(&dir.join(&name), &rel, out);
            } else if kind.is_file() {
                let bytes = std::fs::read(dir.join(&name)).expect("read fixture");
                out.push((rel, bytes));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, "", &mut out);
    out.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    out
}

fn json_files(tree: &[(String, Vec<u8>)]) -> Vec<&(String, Vec<u8>)> {
    tree.iter().filter(|(p, _)| p.ends_with(".json")).collect()
}

#[test]
fn corpus_gson_census() {
    let tree = read_tree(Path::new(CORPUS));
    let files = json_files(&tree);
    assert_eq!(files.len(), 183, "corpus .json file count");

    let mut opaque = Vec::new();
    let mut gateway_format = 0usize;
    let mut errors = Vec::new();
    for (path, bytes) in files {
        match classify(bytes) {
            Ok((JsonClass::GatewayFormat, _)) => gateway_format += 1,
            Ok((JsonClass::Opaque, _)) => opaque.push(path.as_str()),
            Err(why) => errors.push(format!("{path}: {why}")),
        }
    }
    assert!(
        errors.is_empty(),
        "files that failed to classify: {errors:?}"
    );
    assert_eq!(gateway_format, 178, "gateway-format count");
    assert_eq!(opaque, CORPUS_OPAQUE, "opaque files");
}

#[test]
fn captured_tag_files_follow_cd4() {
    // CD4 (21-LIVE-CAPTURES.md): all twelve gateway-written tag files are
    // Gson-identity candidates and none needs the opaque path. The
    // discriminating file is tag-definition/CAP21/FolderA/tags.json (1.0E10,
    // HTML-safe escapes, raw non-ASCII, an integer above 2^53).
    let tree = read_tree(Path::new(CAPTURED));
    let files = json_files(&tree);
    assert_eq!(files.len(), 12, "captured .json file count");
    for (path, bytes) in files {
        let (class, _) =
            classify(bytes).unwrap_or_else(|why| panic!("captured {path} failed: {why}"));
        assert_eq!(class, JsonClass::GatewayFormat, "{path} must follow CD4");
    }
    let discriminating = tree
        .iter()
        .find(|(p, _)| p == "ignition/tag-definition/CAP21/FolderA/tags.json")
        .expect("the discriminating captured file");
    let text = std::str::from_utf8(&discriminating.1).expect("UTF-8");
    assert!(text.contains("1.0E10"), "CD4 float lexeme");
    assert!(text.contains("\\u003c"), "CD4 HTML-safe escape");
    assert!(text.contains("9007199254740993"), "CD4 big integer");
}

/// Directory (module/type/name) of every resource whose `config.json` carries
/// an Embedded secret object.
fn embedded_dirs(tree: &[(String, Vec<u8>)]) -> BTreeSet<String> {
    tree.iter()
        .filter(|(p, b)| {
            p.ends_with("/config.json")
                && String::from_utf8_lossy(b).contains("\"type\": \"Embedded\"")
        })
        .map(|(p, _)| p[..p.len() - "config.json".len()].to_string())
        .collect()
}

#[test]
fn corpus_render_is_byte_identical() {
    let tree = read_tree(Path::new(CORPUS));
    let embedded = embedded_dirs(&tree);
    assert!(!embedded.is_empty(), "the corpus has Embedded resources");
    let denied_types = [
        "ignition/security-properties/",
        "ignition/security-levels/",
        "ignition/user-source/",
        "ignition/api-token/",
    ];

    let included: Vec<&(String, Vec<u8>)> = tree
        .iter()
        .filter(|(p, _)| p != "config-mode.json")
        .filter(|(p, _)| !denied_types.iter().any(|t| p.starts_with(t)))
        .filter(|(p, _)| !embedded.iter().any(|d| p.starts_with(d.as_str())))
        .collect();
    assert!(included.len() > 100, "enough corpus left to be meaningful");

    let types: BTreeSet<String> = included
        .iter()
        .map(|(p, _)| {
            let mut parts = p.split('/');
            format!("{}/{}", parts.next().unwrap(), parts.next().unwrap())
        })
        .collect();
    let mut yaml = String::from("schema_version: 1\ntypes:\n");
    for t in &types {
        yaml.push_str(&format!("  - {t}\n"));
    }
    let manifest = Manifest::parse("manifest.yaml", yaml.as_bytes()).expect("manifest");
    let target = Target::parse("targets/dev.yaml", TARGET).expect("target");
    let source = SourceTree::from_files(
        included
            .iter()
            .map(|(p, b)| (format!("resources/{p}"), b.clone())),
    )
    .expect("source tree");

    let outcome = render(&RenderInput {
        gateway: "g",
        env: "dev",
        manifest: &manifest,
        target: &target,
        source: &source,
    })
    .expect("the sanitized corpus renders");

    assert_eq!(outcome.tree.files.len(), included.len(), "rendered count");
    for (path, bytes) in &included {
        let got = outcome
            .tree
            .files
            .iter()
            .find(|(k, _)| k.as_str() == path)
            .unwrap_or_else(|| panic!("{path} missing from the rendered tree"));
        assert!(got.1 == bytes, "{path} was not rendered byte-identically");
    }

    let expected_opaque: Vec<&str> = CORPUS_OPAQUE
        .iter()
        .copied()
        .filter(|p| included.iter().any(|(q, _)| q == p))
        .collect();
    assert_eq!(
        expected_opaque,
        [
            "com.cirruslink.mqtt.engine.gateway/namespace-server-set/Elecsys-Default Set/config.json",
            "com.cirruslink.mqtt.engine.gateway/namespace-server-set/Sparkplug B-Default Set/config.json",
            "ignition/quickstart/config.json",
        ],
        "the included opaque set"
    );
    assert_eq!(outcome.opaque_files, expected_opaque);
    assert!(outcome.uuids_minted.is_empty(), "render mints no uuid yet");
}

#[test]
fn render_refuses_unparseable_json() {
    let manifest = Manifest::parse(
        "manifest.yaml",
        b"schema_version: 1\ntypes:\n  - m/a\n  - m/b\n",
    )
    .expect("manifest");
    let target = Target::parse("targets/dev.yaml", TARGET).expect("target");
    let bom_prefixed = "\u{feff}{}";
    let source = SourceTree::from_files([
        ("resources/m/a/x/config.json", "{".as_bytes()),
        (
            "resources/m/a/y/config.json",
            br#"{"k":"s","k":"t"}"#.as_slice(),
        ),
        ("resources/m/b/z/config.json", bom_prefixed.as_bytes()),
        ("resources/m/b/ok/config.json", b"{}".as_slice()),
    ])
    .expect("source tree");
    let err = render(&RenderInput {
        gateway: "g",
        env: "dev",
        manifest: &manifest,
        target: &target,
        source: &source,
    })
    .expect_err("unparseable JSON is refused");
    assert_eq!(err.code(), "deploy_tree_invalid");
    let text = err.to_string();
    for path in [
        "gateways/g/resources/m/a/x/config.json",
        "gateways/g/resources/m/a/y/config.json",
        "gateways/g/resources/m/b/z/config.json",
    ] {
        assert!(text.contains(path), "{path} missing from: {text}");
    }
    assert!(
        !text.contains("m/b/ok"),
        "a valid file is not named: {text}"
    );
    assert!(text.contains("duplicate key `k`"), "{text}");
    assert!(!text.contains("\"s\"") && !text.contains("\"t\""), "{text}");
}
