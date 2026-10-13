//! Determinism and format tests for the deploy render pipeline (21-04).
//!
//! These pin the properties Phase 22 (`plan`, `--base`) will rely on: the
//! digest does not depend on read order or the OS, the listing format is
//! the documented one, paths order by UTF-8 bytes, and render never writes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ignition_core::actions::deploy::deploy_render;
use ignition_core::deploy::manifest::Manifest;
use ignition_core::deploy::render::{RenderInput, RenderOutcome, render};
use ignition_core::deploy::target::Target;
use ignition_core::deploy::tree::{EMPTY_RENDER_SHA256, SourceTree, sha256_hex};

const TRACER_REPO: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/deploy/repos/tracer"
);

const TARGET: &[u8] =
    b"schema_version: 1\ngateway: g\nmode: dev\nnodes:\n  master:\n    profile: dev\n";

fn render_files(types: &[&str], files: &[(&str, &[u8])]) -> RenderOutcome {
    let mut yaml = String::from("schema_version: 1\ntypes:\n");
    for t in types {
        yaml.push_str(&format!("  - {t}\n"));
    }
    let manifest = Manifest::parse("manifest.yaml", yaml.as_bytes()).expect("manifest");
    let target = Target::parse("targets/dev.yaml", TARGET).expect("target");
    let source = SourceTree::from_files(files.iter().copied()).expect("source tree");
    render(&RenderInput {
        gateway: "g",
        env: "dev",
        manifest: &manifest,
        target: &target,
        source: &source,
    })
    .expect("render")
}

/// The documented listing, computed here from scratch (never via the
/// production digest function).
fn listing_digest(entries: &[(&str, &[u8])]) -> String {
    let mut sorted: Vec<&(&str, &[u8])> = entries.iter().collect();
    sorted.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    let mut listing = Vec::new();
    for (path, bytes) in sorted {
        listing.extend_from_slice(path.as_bytes());
        listing.push(0);
        listing.extend_from_slice(sha256_hex(bytes).as_bytes());
        listing.push(b'\n');
    }
    sha256_hex(&listing)
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).expect("read_dir") {
            let entry = entry.expect("entry");
            let path = entry.path();
            let rel = path.strip_prefix(root).expect("prefix").to_path_buf();
            if entry.file_type().expect("type").is_dir() {
                out.insert(rel, Vec::new());
                walk(&path, root, out);
            } else {
                out.insert(rel, std::fs::read(&path).expect("read"));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("mkdir");
    for entry in std::fs::read_dir(from).expect("read_dir") {
        let entry = entry.expect("entry");
        let dest = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_dir(&entry.path(), &dest);
        } else {
            std::fs::copy(entry.path(), &dest).expect("copy");
        }
    }
}

#[test]
fn determinism_is_read_order_independent() {
    let files: Vec<(&str, &[u8])> = vec![
        ("resources/m/a/x/config.json", b"{\"a\":1}"),
        ("resources/m/a/x/resource.json", b"{\"b\":2}"),
        ("resources/m/a/config.json", b"{}"),
        ("resources/n/b/y/config.json", b"[]"),
    ];
    let mut reversed = files.clone();
    reversed.reverse();
    let mut rotated = files.clone();
    rotated.rotate_left(2);

    let first = render_files(&["m/a", "n/b"], &files);
    for order in [&reversed, &rotated] {
        let again = render_files(&["m/a", "n/b"], order);
        assert_eq!(first, again, "outcome must not depend on input order");
        assert_eq!(first.tree.render_sha256(), again.tree.render_sha256());
    }

    // the real tree, read from disk repeatedly, gives one digest
    let digests: Vec<String> = (0..3)
        .map(|_| {
            deploy_render(Path::new(TRACER_REPO), "dev")
                .expect("render")
                .render_sha256
        })
        .collect();
    assert!(digests.windows(2).all(|w| w[0] == w[1]), "{digests:?}");
}

#[test]
fn empty_render_digest_is_sha256_of_empty_listing() {
    // an allowlist that matches zero files
    let outcome = render_files(&["m/a"], &[]);
    assert_eq!(outcome.tree.files.len(), 0);
    assert_eq!(outcome.tree.render_sha256(), EMPTY_RENDER_SHA256);
    assert_eq!(sha256_hex(b""), EMPTY_RENDER_SHA256);
    assert_eq!(listing_digest(&[]), EMPTY_RENDER_SHA256);
    assert_eq!(outcome.warnings, ["render produced no files"]);
    assert!(outcome.resources.is_empty());
}

#[test]
fn render_is_read_only() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    copy_dir(Path::new(TRACER_REPO), &repo);
    let before = snapshot(&repo);
    assert!(!before.is_empty());

    let result = deploy_render(&repo, "dev").expect("render");
    assert_eq!(result.files, 4);

    let after = snapshot(&repo);
    assert_eq!(before, after, "render must not add, remove or change files");
    // nothing was created beside the repo either
    let siblings: Vec<_> = std::fs::read_dir(tmp.path())
        .expect("read_dir")
        .map(|e| e.expect("entry").file_name())
        .collect();
    assert_eq!(siblings, ["repo"]);
}

#[test]
fn rendered_bytes_equal_source_bytes() {
    let manifest = Manifest::parse(
        "manifest.yaml",
        &std::fs::read(Path::new(TRACER_REPO).join("gateways/tracer-gw/manifest.yaml"))
            .expect("manifest"),
    )
    .expect("parse manifest");
    let target = Target::parse(
        "targets/dev.yaml",
        &std::fs::read(Path::new(TRACER_REPO).join("targets/dev.yaml")).expect("target"),
    )
    .expect("parse target");
    let source =
        SourceTree::read_gateway(&Path::new(TRACER_REPO).join("gateways/tracer-gw"), "dev")
            .expect("read tree");
    let outcome = render(&RenderInput {
        gateway: "tracer-gw",
        env: "dev",
        manifest: &manifest,
        target: &target,
        source: &source,
    })
    .expect("render");

    assert_eq!(outcome.tree.files.len(), 4);
    for (path, bytes) in &outcome.tree.files {
        let on_disk = std::fs::read(
            Path::new(TRACER_REPO)
                .join("gateways/tracer-gw/resources")
                .join(path.as_str()),
        )
        .expect("source file");
        assert_eq!(bytes, &on_disk, "{} changed bytes", path.as_str());
    }
}

#[test]
fn listing_format_is_pinned() {
    // 3-byte valid JSON bodies: render parses every .json member (21-05)
    let files: [(&str, &[u8]); 2] = [
        ("resources/m/a/config.json", b"[1]"),
        ("resources/m/a/x/config.json", b"[2]"),
    ];
    let outcome = render_files(&["m/a"], &files);
    let expected = listing_digest(&[("m/a/config.json", b"[1]"), ("m/a/x/config.json", b"[2]")]);
    assert_eq!(outcome.tree.render_sha256(), expected);

    // and the literal shape: path NUL hex LF, hand-assembled
    let mut literal = Vec::new();
    literal.extend_from_slice(b"m/a/config.json\0");
    literal.extend_from_slice(sha256_hex(b"[1]").as_bytes());
    literal.push(b'\n');
    literal.extend_from_slice(b"m/a/x/config.json\0");
    literal.extend_from_slice(sha256_hex(b"[2]").as_bytes());
    literal.push(b'\n');
    assert_eq!(outcome.tree.render_sha256(), sha256_hex(&literal));
    assert_eq!(outcome.tree.bytes(), 6);
}

#[test]
fn paths_sort_by_utf8_bytes() {
    // 'B' 0x42 < 'Z' 0x5a < 'a' 0x61 < 'é' 0xc3 0xa9; no case folding and
    // no Unicode normalization
    let names = ["a", "é", "Z", "B"];
    let files: Vec<(String, &[u8])> = names
        .iter()
        .map(|n| (format!("resources/m/t/{n}/config.json"), &b"{}"[..]))
        .collect();
    let borrowed: Vec<(&str, &[u8])> = files.iter().map(|(p, b)| (p.as_str(), *b)).collect();
    let outcome = render_files(&["m/t"], &borrowed);

    let paths: Vec<&str> = outcome.tree.files.keys().map(|p| p.as_str()).collect();
    assert_eq!(
        paths,
        [
            "m/t/B/config.json",
            "m/t/Z/config.json",
            "m/t/a/config.json",
            "m/t/é/config.json",
        ]
    );
    let resource_names: Vec<&str> = outcome
        .resources
        .iter()
        .map(|r| r.name.as_deref().expect("named"))
        .collect();
    assert_eq!(resource_names, ["B", "Z", "a", "é"]);

    // the digest follows the same byte order
    let entries: Vec<(&str, &[u8])> = paths.iter().map(|p| (*p, &b"{}"[..])).collect();
    assert_eq!(outcome.tree.render_sha256(), listing_digest(&entries));

    // composed and decomposed forms are different bytes and stay distinct
    let composed = "resources/m/t/\u{e9}/config.json";
    let decomposed = "resources/m/t/e\u{301}/config.json";
    let both = render_files(
        &["m/t"],
        &[(composed, b"{}".as_slice()), (decomposed, b"{}".as_slice())],
    );
    assert_eq!(both.tree.files.len(), 2, "no normalization");
}

#[test]
fn envelope_arrays_are_ordered() {
    let files: [(&str, &[u8]); 6] = [
        ("resources/n/b/y/config.json", b"{}"),
        ("resources/m/b/config.json", b"{}"),
        ("resources/m/a/z/config.json", b"{}"),
        ("resources/m/a/config.json", b"{}"),
        ("resources/m/a/a/config.json", b"{}"),
        ("resources/m/a/a/resource.json", b"{}"),
    ];
    let outcome = render_files(&["m/a", "m/b", "n/b"], &files);
    let keys: Vec<(String, String, Option<String>)> = outcome
        .resources
        .iter()
        .map(|r| (r.module.clone(), r.ty.clone(), r.name.clone()))
        .collect();
    let s = |m: &str, t: &str, n: Option<&str>| (m.to_string(), t.to_string(), n.map(String::from));
    assert_eq!(
        keys,
        [
            s("m", "a", None),
            s("m", "a", Some("a")),
            s("m", "a", Some("z")),
            s("m", "b", None),
            s("n", "b", Some("y")),
        ],
        "(module, type, name) bytes, singleton first"
    );
    assert_eq!(
        outcome.resources[1].files,
        ["config.json", "resource.json"],
        "files are resource-relative and byte-sorted"
    );

    // the action's envelope carries the same ordering
    let result = deploy_render(Path::new(TRACER_REPO), "dev").expect("render");
    let tracer: Vec<(&str, &str, Option<&str>)> = result
        .resources
        .iter()
        .map(|r| (r.module.as_str(), r.ty.as_str(), r.name.as_deref()))
        .collect();
    assert_eq!(
        tracer,
        [
            ("ignition", "database-translator", Some("POSTGRES")),
            ("ignition", "general-alarm-settings", None),
        ]
    );
    for list in [
        &result.tag_providers,
        &result.uuids_minted,
        &result.opaque_files,
    ] {
        let mut sorted = list.clone();
        sorted.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        assert_eq!(list, &sorted, "byte order");
    }
}
