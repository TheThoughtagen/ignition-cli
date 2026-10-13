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

// ---- 21-06 task 1: overlays and the tags layout --------------------------

use ignition_core::error::CoreError;

/// Render with a manifest that declares `types` and `providers`, for a given
/// environment; returns the error as text when the render is refused.
fn try_render(
    env: &str,
    types: &[&str],
    providers: &[&str],
    files: &[(&str, &str)],
) -> Result<RenderOutcome, CoreError> {
    let mut yaml = String::from("schema_version: 1\ntypes:\n");
    for t in types {
        yaml.push_str(&format!("  - {t}\n"));
    }
    if types.is_empty() {
        yaml = String::from("schema_version: 1\ntypes: []\n");
    }
    if !providers.is_empty() {
        yaml.push_str(&format!("tag_providers: [{}]\n", providers.join(", ")));
    }
    let manifest = Manifest::parse("manifest.yaml", yaml.as_bytes()).expect("manifest");
    let target = Target::parse("targets/dev.yaml", TARGET).expect("target");
    let source =
        SourceTree::from_files(files.iter().map(|(p, b)| (*p, b.as_bytes()))).expect("tree");
    render(&RenderInput {
        gateway: "g",
        env,
        manifest: &manifest,
        target: &target,
        source: &source,
    })
}

fn refused(r: Result<RenderOutcome, CoreError>) -> String {
    match r {
        Ok(o) => panic!("expected a refusal, rendered {} files", o.tree.files.len()),
        Err(e) => e.to_string(),
    }
}

fn bytes_at<'a>(o: &'a RenderOutcome, path: &str) -> &'a [u8] {
    o.tree
        .files
        .iter()
        .find(|(p, _)| p.as_str() == path)
        .unwrap_or_else(|| {
            panic!(
                "{path} not rendered: {:?}",
                o.tree.files.keys().collect::<Vec<_>>()
            )
        })
        .1
}

#[test]
fn overlay_replaces_whole_file() {
    let out = try_render(
        "dev",
        &["m/a"],
        &[],
        &[
            ("resources/m/a/x/config.json", "{\"base\": 1}"),
            ("resources/m/a/x/resource.json", "{\"keep\": 1}"),
            ("modes/dev/resources/m/a/x/config.json", "{\"over\": 2}"),
        ],
    )
    .unwrap();
    assert_eq!(bytes_at(&out, "m/a/x/config.json"), b"{\"over\": 2}");
    assert_eq!(bytes_at(&out, "m/a/x/resource.json"), b"{\"keep\": 1}");
    assert_eq!(
        out.tree.files.len(),
        2,
        "no modes/ path leaks into the output"
    );
}

#[test]
fn overlay_adds_new_file() {
    let out = try_render(
        "dev",
        &["m/a"],
        &[],
        &[
            ("resources/m/a/x/config.json", "{}"),
            ("modes/dev/resources/m/a/y/config.json", "{\"new\": true}"),
        ],
    )
    .unwrap();
    assert_eq!(bytes_at(&out, "m/a/x/config.json"), b"{}");
    assert_eq!(bytes_at(&out, "m/a/y/config.json"), b"{\"new\": true}");
    // an overlay cannot remove a base file (SF-7): without a counterpart the
    // base file is unchanged
    assert_eq!(out.tree.files.len(), 2);
}

#[test]
fn other_env_overlays_are_never_read() {
    // in memory: nothing under modes/staging reaches a dev render
    let files = [
        ("resources/m/a/x/config.json", "{}"),
        (
            "modes/staging/resources/m/a/x/config.json",
            "not json at all",
        ),
        ("modes/staging/resources/zzz/denied/x/config.json", "{}"),
        ("modes/staging/.hidden/x", "{}"),
        ("modes/dev2/resources/m/a/x/config.json", "[9]"),
    ];
    let out = try_render("dev", &["m/a"], &[], &files).unwrap();
    assert_eq!(bytes_at(&out, "m/a/x/config.json"), b"{}");
    assert_eq!(out.tree.files.len(), 1);

    // on disk: an invalid tree for another environment does not matter
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    copy_dir(Path::new(TRACER_REPO), &repo);
    let before = deploy_render(&repo, "dev").expect("baseline").render_sha256;
    let other = repo.join("gateways/tracer-gw/modes/staging");
    std::fs::create_dir_all(other.join("resources/ignition/database-translator/POSTGRES"))
        .expect("mkdir");
    std::fs::write(
        other.join("resources/ignition/database-translator/POSTGRES/config.json"),
        b"{ broken",
    )
    .expect("write");
    std::fs::write(other.join(".DS_Store"), b"x").expect("write");
    std::fs::create_dir_all(other.join(".hidden")).expect("mkdir");
    #[cfg(unix)]
    std::os::unix::fs::symlink("/etc", other.join("resources/link")).expect("symlink");
    let after = deploy_render(&repo, "dev").expect("staging content is never read");
    assert_eq!(before, after.render_sha256);
}

#[test]
fn overlay_findings_name_overlay_path() {
    let text = refused(try_render(
        "dev",
        &["m/a"],
        &[],
        &[
            ("resources/m/a/x/config.json", "{}"),
            ("modes/dev/resources/m/a/x/resource.json", "{ broken"),
            ("modes/dev/resources/m/other/x/config.json", "{}"),
        ],
    ));
    assert!(
        text.contains("gateways/g/modes/dev/resources/m/a/x/resource.json"),
        "{text}"
    );
    assert!(
        text.contains("gateways/g/modes/dev/resources/m/other/"),
        "{text}"
    );
}

#[test]
fn tags_map_to_collection_paths() {
    let out = try_render(
        "dev",
        &[],
        &["default"],
        &[
            ("tags/default/tag-provider/config.json", "{}"),
            ("tags/default/tag-provider/resource.json", "{}"),
            ("tags/default/tag-group/Default/config.json", "{}"),
            ("tags/default/tag-group/Default/resource.json", "{}"),
            ("tags/default/tag-type-definition/udts.json", "{}"),
            ("tags/default/tag-type-definition/unary-resource.json", "{}"),
            ("tags/default/tag-definition/unary-resource.json", "{}"),
            ("tags/default/tag-definition/FolderA/tags.json", "{}"),
            ("tags/default/tag-definition/FolderA/Sub/udts.json", "{}"),
        ],
    )
    .unwrap();
    let paths: Vec<&str> = out.tree.files.keys().map(|p| p.as_str()).collect();
    assert_eq!(
        paths,
        [
            "ignition/tag-definition/default/FolderA/Sub/udts.json",
            "ignition/tag-definition/default/FolderA/tags.json",
            "ignition/tag-definition/default/unary-resource.json",
            "ignition/tag-group/default/Default/config.json",
            "ignition/tag-group/default/Default/resource.json",
            "ignition/tag-provider/default/config.json",
            "ignition/tag-provider/default/resource.json",
            "ignition/tag-type-definition/default/udts.json",
            "ignition/tag-type-definition/default/unary-resource.json",
        ]
    );
    assert_eq!(out.tag_providers, ["default"]);
    // the tag types are named resources keyed by provider
    let provider = out
        .resources
        .iter()
        .find(|r| r.ty == "tag-provider")
        .expect("provider resource");
    assert_eq!(provider.module, "ignition");
    assert_eq!(provider.name.as_deref(), Some("default"));
    assert_eq!(provider.files, ["config.json", "resource.json"]);
    let group = out
        .resources
        .iter()
        .find(|r| r.ty == "tag-group")
        .expect("group resource");
    assert_eq!(
        group.files,
        ["Default/config.json", "Default/resource.json"]
    );

    // a tags/ overlay replaces the base tag file
    let over = try_render(
        "dev",
        &[],
        &["default"],
        &[
            ("tags/default/tag-definition/F/tags.json", "[1]"),
            ("modes/dev/tags/default/tag-definition/F/tags.json", "[2]"),
        ],
    )
    .unwrap();
    assert_eq!(
        bytes_at(&over, "ignition/tag-definition/default/F/tags.json"),
        b"[2]"
    );
}

#[test]
fn tag_provider_not_in_manifest_is_refused() {
    let text = refused(try_render(
        "dev",
        &[],
        &["default"],
        &[
            ("tags/default/tag-provider/config.json", "{}"),
            ("tags/rogue/tag-provider/config.json", "{}"),
            ("tags/rogue/tag-group/Default/config.json", "{}"),
            ("modes/dev/tags/other/tag-provider/config.json", "{}"),
        ],
    ));
    assert!(text.contains("gateways/g/tags/rogue/"), "{text}");
    assert!(text.contains("gateways/g/modes/dev/tags/other/"), "{text}");
    assert_eq!(
        text.matches("tags/rogue/").count(),
        1,
        "one finding per provider: {text}"
    );
    assert!(!text.contains("tags/default/"), "{text}");
}

#[test]
fn unknown_tag_type_is_refused() {
    let text = refused(try_render(
        "dev",
        &[],
        &["default"],
        &[
            ("tags/default/tag-provider/config.json", "{}"),
            ("tags/default/tag-widget/config.json", "{}"),
            ("tags/default/loose.json", "{}"),
        ],
    ));
    assert!(
        text.contains("gateways/g/tags/default/tag-widget/config.json"),
        "{text}"
    );
    assert!(text.contains("is not a tag type"), "{text}");
    assert!(
        text.contains("gateways/g/tags/default/loose.json"),
        "{text}"
    );
}

#[test]
fn tag_types_under_resources_are_refused() {
    let text = refused(try_render(
        "dev",
        &["ignition/tag-group", "ignition/tag-definition"],
        &[],
        &[
            ("resources/ignition/tag-group/P/Default/config.json", "{}"),
            ("resources/ignition/tag-group/P/Other/config.json", "{}"),
            (
                "resources/ignition/tag-definition/P/unary-resource.json",
                "{}",
            ),
        ],
    ));
    assert_eq!(
        text.matches("gateways/g/resources/ignition/tag-group/")
            .count(),
        1,
        "one finding per type directory: {text}"
    );
    assert!(
        text.contains("gateways/g/resources/ignition/tag-definition/"),
        "{text}"
    );
    assert!(text.contains("tags/"), "{text}");
}

#[test]
fn colliding_sources_are_refused() {
    let text = refused(try_render(
        "dev",
        &["ignition/tag-group"],
        &["P"],
        &[
            ("tags/P/tag-group/Default/config.json", "{}"),
            ("resources/ignition/tag-group/P/Default/config.json", "{}"),
        ],
    ));
    assert!(
        text.contains("gateways/g/resources/ignition/tag-group/P/Default/config.json"),
        "{text}"
    );
    assert!(
        text.contains("gateways/g/tags/P/tag-group/Default/config.json"),
        "{text}"
    );
    assert!(
        text.contains("ignition/tag-group/P/Default/config.json"),
        "{text}"
    );
}

fn overlay_repo(root: &Path) -> PathBuf {
    let repo = root.join("repo");
    copy_dir(Path::new(TRACER_REPO), &repo);
    let gw = repo.join("gateways/tracer-gw");
    let mut manifest = std::fs::read_to_string(gw.join("manifest.yaml")).expect("manifest");
    manifest.push_str("tag_providers: [default]\n");
    std::fs::write(gw.join("manifest.yaml"), manifest).expect("write");
    for (rel, body) in [
        (
            "modes/dev/resources/ignition/database-translator/POSTGRES/resource.json",
            "{\"o\": 1}",
        ),
        ("modes/dev/tags/default/tag-provider/config.json", "{}"),
        ("tags/default/tag-provider/resource.json", "{}"),
        ("tags/default/tag-definition/F/tags.json", "{}"),
    ] {
        let path = gw.join(rel);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, body).expect("write");
    }
    repo
}

#[test]
fn overlay_render_is_read_only_and_repeatable() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = overlay_repo(tmp.path());
    let before = snapshot(&repo);
    let first = deploy_render(&repo, "dev").expect("render");
    let second = deploy_render(&repo, "dev").expect("render again");
    assert_eq!(first.render_sha256, second.render_sha256);
    assert_eq!(first.tag_providers, ["default"]);
    assert_eq!(first, second);
    assert_eq!(before, snapshot(&repo), "render must not write anything");
    // the overlay is part of the dev digest and absent from another env
    let base = deploy_render(Path::new(TRACER_REPO), "dev").expect("baseline");
    assert_ne!(first.render_sha256, base.render_sha256);
}
