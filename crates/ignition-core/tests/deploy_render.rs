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

/// Gateway-format `resource.json` of a Named type, with a uuid (as the
/// gateway writes it; Gson pretty, no trailing newline).
const NAMED_RJ: &str = "{\n  \"scope\": \"A\",\n  \"version\": 1,\n  \"restricted\": false,\n  \"overridable\": true,\n  \"files\": [\n    \"config.json\"\n  ],\n  \"attributes\": {\n    \"uuid\": \"4a14d749-bb78-47c7-847a-cafa17da4da8\",\n    \"enabled\": true\n  }\n}";

/// The same with `enabled` flipped: a different file, still valid.
const NAMED_RJ_DISABLED: &str = "{\n  \"scope\": \"A\",\n  \"version\": 1,\n  \"restricted\": false,\n  \"overridable\": true,\n  \"files\": [\n    \"config.json\"\n  ],\n  \"attributes\": {\n    \"uuid\": \"4a14d749-bb78-47c7-847a-cafa17da4da8\",\n    \"enabled\": false\n  }\n}";

/// Gateway-format `resource.json` of a Singleton type (no uuid).
const SINGLETON_RJ: &str = "{\n  \"scope\": \"A\",\n  \"version\": 1,\n  \"restricted\": false,\n  \"overridable\": true,\n  \"files\": [\n    \"config.json\"\n  ],\n  \"attributes\": {\n    \"enabled\": true\n  }\n}";

/// Gateway-format `resource.json` of a NamedNoUuid type (themes shape).
const NO_UUID_RJ: &str = "{\n  \"scope\": \"G\",\n  \"version\": 1,\n  \"restricted\": false,\n  \"overridable\": true,\n  \"files\": [\n    \"config.json\"\n  ],\n  \"attributes\": {\n    \"enabled\": true\n  }\n}";

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
        ("resources/ignition/schedule/x/config.json", b"{\"a\":1}"),
        (
            "resources/ignition/schedule/x/resource.json",
            NAMED_RJ.as_bytes(),
        ),
        ("resources/ignition/system-properties/config.json", b"{}"),
        (
            "resources/ignition/system-properties/resource.json",
            SINGLETON_RJ.as_bytes(),
        ),
        ("resources/ignition/email-profile/y/config.json", b"[]"),
    ];
    const TYPES: [&str; 3] = [
        "ignition/schedule",
        "ignition/system-properties",
        "ignition/email-profile",
    ];
    let mut reversed = files.clone();
    reversed.reverse();
    let mut rotated = files.clone();
    rotated.rotate_left(2);

    let first = render_files(&TYPES, &files);
    for order in [&reversed, &rotated] {
        let again = render_files(&TYPES, order);
        assert_eq!(first, again, "outcome must not depend on input order");
        assert_eq!(first.tree.render_sha256(), again.tree.render_sha256());
    }

    // the real tree, read from disk repeatedly, gives one digest
    let digests: Vec<String> = (0..3)
        .map(|_| {
            deploy_render(Path::new(TRACER_REPO), "dev", None)
                .expect("render")
                .render_sha256
        })
        .collect();
    assert!(digests.windows(2).all(|w| w[0] == w[1]), "{digests:?}");
}

#[test]
fn empty_render_digest_is_sha256_of_empty_listing() {
    // an allowlist that matches zero files
    let outcome = render_files(&["ignition/schedule"], &[]);
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

    let result = deploy_render(&repo, "dev", None).expect("render");
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
    // a 3-byte valid JSON body: render parses every .json member (21-05)
    let files: [(&str, &[u8]); 2] = [
        ("resources/ignition/system-properties/config.json", b"[1]"),
        (
            "resources/ignition/system-properties/resource.json",
            SINGLETON_RJ.as_bytes(),
        ),
    ];
    let outcome = render_files(&["ignition/system-properties"], &files);
    let expected = listing_digest(&[
        ("ignition/system-properties/config.json", b"[1]"),
        (
            "ignition/system-properties/resource.json",
            SINGLETON_RJ.as_bytes(),
        ),
    ]);
    assert_eq!(outcome.tree.render_sha256(), expected);

    // and the literal shape: path NUL hex LF, hand-assembled
    let mut literal = Vec::new();
    literal.extend_from_slice(b"ignition/system-properties/config.json\0");
    literal.extend_from_slice(sha256_hex(b"[1]").as_bytes());
    literal.push(b'\n');
    literal.extend_from_slice(b"ignition/system-properties/resource.json\0");
    literal.extend_from_slice(sha256_hex(SINGLETON_RJ.as_bytes()).as_bytes());
    literal.push(b'\n');
    assert_eq!(outcome.tree.render_sha256(), sha256_hex(&literal));
    assert_eq!(outcome.tree.bytes(), 3 + SINGLETON_RJ.len());
}

#[test]
fn paths_sort_by_utf8_bytes() {
    // 'B' 0x42 < 'Z' 0x5a < 'a' 0x61 < 'é' 0xc3 0xa9; no case folding and
    // no Unicode normalization. The resource.json of each is synthesized.
    let names = ["a", "é", "Z", "B"];
    let files: Vec<(String, &[u8])> = names
        .iter()
        .map(|n| {
            (
                format!("resources/ignition/schedule/{n}/config.json"),
                &b"{}"[..],
            )
        })
        .collect();
    let borrowed: Vec<(&str, &[u8])> = files.iter().map(|(p, b)| (p.as_str(), *b)).collect();
    let outcome = render_files(&["ignition/schedule"], &borrowed);

    let paths: Vec<&str> = outcome.tree.files.keys().map(|p| p.as_str()).collect();
    assert_eq!(
        paths,
        [
            "ignition/schedule/B/config.json",
            "ignition/schedule/B/resource.json",
            "ignition/schedule/Z/config.json",
            "ignition/schedule/Z/resource.json",
            "ignition/schedule/a/config.json",
            "ignition/schedule/a/resource.json",
            "ignition/schedule/é/config.json",
            "ignition/schedule/é/resource.json",
        ]
    );
    let resource_names: Vec<&str> = outcome
        .resources
        .iter()
        .map(|r| r.name.as_deref().expect("named"))
        .collect();
    assert_eq!(resource_names, ["B", "Z", "a", "é"]);
    assert_eq!(
        outcome.uuids_minted,
        [
            "ignition/schedule/B",
            "ignition/schedule/Z",
            "ignition/schedule/a",
            "ignition/schedule/é",
        ]
    );

    // the digest follows the same byte order
    let entries: Vec<(&str, &[u8])> = outcome
        .tree
        .files
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_slice()))
        .collect();
    assert_eq!(outcome.tree.render_sha256(), listing_digest(&entries));

    // composed and decomposed forms are different bytes and stay distinct,
    // down to their uuids
    let composed = "resources/ignition/schedule/\u{e9}/config.json";
    let decomposed = "resources/ignition/schedule/e\u{301}/config.json";
    let both = render_files(
        &["ignition/schedule"],
        &[(composed, b"{}".as_slice()), (decomposed, b"{}".as_slice())],
    );
    assert_eq!(both.tree.files.len(), 4, "no normalization");
    let uuids: std::collections::BTreeSet<&[u8]> = both
        .tree
        .files
        .iter()
        .filter(|(p, _)| p.as_str().ends_with("resource.json"))
        .map(|(_, b)| b.as_slice())
        .collect();
    assert_eq!(uuids.len(), 2, "NFC and NFD get different uuids");
}

#[test]
fn envelope_arrays_are_ordered() {
    let files: [(&str, &[u8]); 7] = [
        (
            "resources/com.inductiveautomation.historian/historian-provider/y/config.json",
            b"{}",
        ),
        ("resources/ignition/system-properties/config.json", b"{}"),
        (
            "resources/ignition/system-properties/resource.json",
            SINGLETON_RJ.as_bytes(),
        ),
        ("resources/ignition/schedule/z/config.json", b"{}"),
        ("resources/ignition/schedule/a/config.json", b"{}"),
        (
            "resources/ignition/schedule/a/resource.json",
            NAMED_RJ.as_bytes(),
        ),
        (
            "resources/ignition/schedule/z/resource.json",
            NAMED_RJ.as_bytes(),
        ),
    ];
    let outcome = render_files(
        &[
            "ignition/schedule",
            "ignition/system-properties",
            "com.inductiveautomation.historian/historian-provider",
        ],
        &files,
    );
    let keys: Vec<(String, String, Option<String>)> = outcome
        .resources
        .iter()
        .map(|r| (r.module.clone(), r.ty.clone(), r.name.clone()))
        .collect();
    let s = |m: &str, t: &str, n: Option<&str>| (m.to_string(), t.to_string(), n.map(String::from));
    assert_eq!(
        keys,
        [
            s(
                "com.inductiveautomation.historian",
                "historian-provider",
                Some("y")
            ),
            s("ignition", "schedule", Some("a")),
            s("ignition", "schedule", Some("z")),
            s("ignition", "system-properties", None),
        ],
        "(module, type, name) bytes"
    );
    assert_eq!(
        outcome.resources[1].files,
        ["config.json", "resource.json"],
        "files are resource-relative and byte-sorted"
    );

    // the action's envelope carries the same ordering
    let result = deploy_render(Path::new(TRACER_REPO), "dev", None).expect("render");
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
        &["ignition/schedule"],
        &[],
        &[
            ("resources/ignition/schedule/x/config.json", "{\"base\": 1}"),
            ("resources/ignition/schedule/x/resource.json", NAMED_RJ),
            (
                "modes/dev/resources/ignition/schedule/x/config.json",
                "{\"over\": 2}",
            ),
        ],
    )
    .unwrap();
    assert_eq!(
        bytes_at(&out, "ignition/schedule/x/config.json"),
        b"{\"over\": 2}"
    );
    assert_eq!(
        bytes_at(&out, "ignition/schedule/x/resource.json"),
        NAMED_RJ.as_bytes()
    );
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
        &["ignition/schedule"],
        &[],
        &[
            ("resources/ignition/schedule/x/config.json", "{}"),
            ("resources/ignition/schedule/x/resource.json", NAMED_RJ),
            (
                "modes/dev/resources/ignition/schedule/y/config.json",
                "{\"new\": true}",
            ),
            (
                "modes/dev/resources/ignition/schedule/y/resource.json",
                NAMED_RJ,
            ),
        ],
    )
    .unwrap();
    assert_eq!(bytes_at(&out, "ignition/schedule/x/config.json"), b"{}");
    assert_eq!(
        bytes_at(&out, "ignition/schedule/y/config.json"),
        b"{\"new\": true}"
    );
    // an overlay cannot remove a base file (SF-7): without a counterpart the
    // base file is unchanged
    assert_eq!(out.tree.files.len(), 4);
    assert!(out.uuids_minted.is_empty());
}

#[test]
fn other_env_overlays_are_never_read() {
    // in memory: nothing under modes/staging reaches a dev render
    let files = [
        ("resources/ignition/schedule/x/config.json", "{}"),
        ("resources/ignition/schedule/x/resource.json", NAMED_RJ),
        (
            "modes/staging/resources/ignition/schedule/x/config.json",
            "not json at all",
        ),
        ("modes/staging/resources/zzz/denied/x/config.json", "{}"),
        ("modes/staging/.hidden/x", "{}"),
        (
            "modes/dev2/resources/ignition/schedule/x/config.json",
            "[9]",
        ),
    ];
    let out = try_render("dev", &["ignition/schedule"], &[], &files).unwrap();
    assert_eq!(bytes_at(&out, "ignition/schedule/x/config.json"), b"{}");
    assert_eq!(out.tree.files.len(), 2);

    // on disk: an invalid tree for another environment does not matter
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    copy_dir(Path::new(TRACER_REPO), &repo);
    let before = deploy_render(&repo, "dev", None)
        .expect("baseline")
        .render_sha256;
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
    let after = deploy_render(&repo, "dev", None).expect("staging content is never read");
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
            NAMED_RJ_DISABLED,
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
    let first = deploy_render(&repo, "dev", None).expect("render");
    let second = deploy_render(&repo, "dev", None).expect("render again");
    assert_eq!(first.render_sha256, second.render_sha256);
    assert_eq!(first.tag_providers, ["default"]);
    assert_eq!(first, second);
    assert_eq!(before, snapshot(&repo), "render must not write anything");
    // the overlay is part of the dev digest and absent from another env
    let base = deploy_render(Path::new(TRACER_REPO), "dev", None).expect("baseline");
    assert_ne!(first.render_sha256, base.render_sha256);
}

// ---- 21-06 task 2: policy refusals ----------------------------------------

const OPENAPI_TYPES: &str = include_str!("fixtures/deploy/openapi-8.3.6-resource-types.json");

const DENIED: [&str; 4] = [
    "security-properties",
    "security-levels",
    "api-token",
    "user-source",
];

#[test]
fn each_denied_type_is_refused_even_when_allowlisted() {
    for ty in DENIED {
        let entry = format!("ignition/{ty}");
        let file = format!("resources/ignition/{ty}/config.json");
        let named = format!("resources/ignition/{ty}/Some Name/config.json");
        for path in [file, named] {
            let text = refused(try_render(
                "dev",
                &[entry.as_str()],
                &[],
                &[(path.as_str(), "{}")],
            ));
            assert!(text.contains("gateways/g/manifest.yaml"), "{ty}: {text}");
            assert!(text.contains(&format!("`{entry}`")), "{ty}: {text}");
            assert!(
                text.contains(&format!("gateways/g/resources/ignition/{ty}/")),
                "{ty}: {text}"
            );
        }
    }
    // the same type under an overlay names the overlay directory
    let text = refused(try_render(
        "dev",
        &["ignition/user-source"],
        &[],
        &[(
            "modes/dev/resources/ignition/user-source/u/config.json",
            "{}",
        )],
    ));
    assert!(
        text.contains("gateways/g/modes/dev/resources/ignition/user-source/"),
        "{text}"
    );
    // a type merely next to a denied one is not treated as denied: it is
    // refused only for having no render rule (21-07)
    let text = refused(try_render(
        "dev",
        &["ignition/user-source-x"],
        &[],
        &[("resources/ignition/user-source-x/u/config.json", "{}")],
    ));
    assert!(text.contains("no render rule"), "{text}");
    assert!(!text.contains("denied type"), "{text}");
}

#[test]
fn denied_types_exist_in_openapi_extract() {
    let extract: serde_json::Value = serde_json::from_str(OPENAPI_TYPES).expect("extract");
    let types = extract["types"].as_array().expect("types array");
    for pair in ignition_core::deploy::rules::DENIED_TYPES {
        assert!(
            types
                .iter()
                .any(|t| t["module"] == pair.0 && t["type"] == pair.1),
            "{pair:?} is not a type id in the 8.3.6 OpenAPI extract"
        );
    }
    for ty in DENIED {
        assert!(ignition_core::deploy::rules::is_denied("ignition", ty));
    }
    assert_eq!(ignition_core::deploy::rules::DENIED_TYPES.len(), 4);
}

#[test]
fn config_mode_json_is_refused_anywhere() {
    for (path, shown) in [
        (
            "resources/m/a/x/config-mode.json",
            "gateways/g/resources/m/a/x/config-mode.json",
        ),
        (
            "resources/m/a/config-mode.json",
            "gateways/g/resources/m/a/config-mode.json",
        ),
        (
            "modes/dev/resources/m/a/x/config-mode.json",
            "gateways/g/modes/dev/resources/m/a/x/config-mode.json",
        ),
        (
            "tags/P/tag-definition/F/config-mode.json",
            "gateways/g/tags/P/tag-definition/F/config-mode.json",
        ),
    ] {
        let text = refused(try_render(
            "dev",
            &["m/a"],
            &["P"],
            &[("resources/m/a/ok/config.json", "{}"), (path, "{}")],
        ));
        assert!(text.contains(shown), "{path}: {text}");
        assert!(text.contains("written by the gateway"), "{text}");
    }
}

/// A copy of the tracer repo with `.gitkeep` files at every directory level
/// the reader walks.
fn gitkeep_repo(root: &Path) -> PathBuf {
    let repo = overlay_repo(root);
    let gw = repo.join("gateways/tracer-gw");
    for dir in [
        "",
        "resources",
        "resources/ignition",
        "resources/ignition/database-translator/POSTGRES",
        "tags",
        "tags/default/tag-definition/F",
        "modes",
        "modes/dev",
        "modes/dev/resources",
        "modes/staging",
    ] {
        let path = gw.join(dir);
        std::fs::create_dir_all(&path).expect("mkdir");
        std::fs::write(path.join(".gitkeep"), b"").expect("write");
    }
    repo
}

#[test]
fn dot_names_are_refused_but_gitkeep_is_skipped() {
    // in memory: .gitkeep is dropped, never rendered, never listed
    let out = try_render(
        "dev",
        &["ignition/schedule"],
        &[],
        &[
            ("resources/ignition/schedule/x/config.json", "{}"),
            ("resources/ignition/schedule/x/resource.json", NAMED_RJ),
            ("resources/ignition/schedule/x/.gitkeep", ""),
            ("resources/.gitkeep", ""),
            ("modes/dev/resources/ignition/schedule/.gitkeep", ""),
        ],
    )
    .unwrap();
    assert_eq!(out.tree.files.len(), 2);
    assert!(
        out.resources
            .iter()
            .all(|r| r.files == ["config.json", "resource.json"])
    );

    let text = refused(try_render(
        "dev",
        &["ignition/schedule"],
        &[],
        &[
            ("resources/ignition/schedule/x/config.json", "{}"),
            ("resources/ignition/schedule/x/.DS_Store", "x"),
            ("resources/ignition/schedule/.hidden/y/config.json", "{}"),
            ("resources/ignition/schedule/.hidden/z/config.json", "{}"),
            ("modes/dev/resources/ignition/schedule/.env", "x"),
            ("resources/ignition/schedule/.gitkeep/inner", "x"),
        ],
    ));
    assert!(
        text.contains("gateways/g/resources/ignition/schedule/x/.DS_Store"),
        "{text}"
    );
    assert_eq!(
        text.matches("gateways/g/resources/ignition/schedule/.hidden")
            .count(),
        1,
        "a dot-named directory is reported once: {text}"
    );
    assert!(
        text.contains("gateways/g/modes/dev/resources/ignition/schedule/.env"),
        "{text}"
    );
    assert!(
        text.contains("gateways/g/resources/ignition/schedule/.gitkeep"),
        "{text}"
    );

    // on disk: the reader skips .gitkeep at every depth, and the digest is
    // the one a tree without them gives
    let tmp = tempfile::tempdir().expect("tempdir");
    let plain = overlay_repo(&tmp.path().join("plain"));
    let kept = gitkeep_repo(&tmp.path().join("kept"));
    let a = deploy_render(&plain, "dev", None).expect("plain");
    let b = deploy_render(&kept, "dev", None).expect("with .gitkeep files");
    assert_eq!(a.render_sha256, b.render_sha256);
    assert_eq!(a.files, b.files);

    // on disk: other dot-names are refused, a dot-named directory is not
    // walked, and every finding is reported together
    let gw = kept.join("gateways/tracer-gw");
    std::fs::write(gw.join("resources/.DS_Store"), b"x").expect("write");
    std::fs::create_dir_all(gw.join("tags/.hidden")).expect("mkdir");
    std::fs::write(gw.join("tags/.hidden/tags.json"), b"{ broken").expect("write");
    std::fs::write(gw.join("modes/dev/.DS_Store"), b"x").expect("write");
    let err = deploy_render(&kept, "dev", None).unwrap_err().to_string();
    assert!(
        err.contains("gateways/tracer-gw/resources/.DS_Store"),
        "{err}"
    );
    assert!(err.contains("gateways/tracer-gw/tags/.hidden"), "{err}");
    assert!(
        err.contains("gateways/tracer-gw/modes/dev/.DS_Store"),
        "{err}"
    );
    assert!(
        !err.contains("broken"),
        "dot-named directories are not walked: {err}"
    );
}

#[test]
fn dotted_module_names_are_not_dot_names() {
    let out = try_render(
        "dev",
        &["com.inductiveautomation.perspective/themes"],
        &[],
        &[
            (
                "resources/com.inductiveautomation.perspective/themes/Dark.Mode/config.json",
                "{}",
            ),
            (
                "resources/com.inductiveautomation.perspective/themes/Dark.Mode/resource.json",
                NO_UUID_RJ,
            ),
        ],
    )
    .unwrap();
    assert_eq!(
        out.tree
            .files
            .keys()
            .map(|p| p.as_str())
            .collect::<Vec<_>>(),
        [
            "com.inductiveautomation.perspective/themes/Dark.Mode/config.json",
            "com.inductiveautomation.perspective/themes/Dark.Mode/resource.json"
        ]
    );
    // on disk too
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    copy_dir(Path::new(TRACER_REPO), &repo);
    let gw = repo.join("gateways/tracer-gw");
    let theme = gw.join("resources/com.inductiveautomation.perspective/themes/Dark.Mode");
    std::fs::create_dir_all(&theme).expect("mkdir");
    std::fs::write(theme.join("config.json"), b"{}").expect("write");
    std::fs::write(theme.join("resource.json"), NO_UUID_RJ).expect("write");
    let mut manifest = std::fs::read_to_string(gw.join("manifest.yaml")).expect("manifest");
    manifest = manifest.replace(
        "types:\n",
        "types:\n  - com.inductiveautomation.perspective/themes\n",
    );
    std::fs::write(gw.join("manifest.yaml"), manifest).expect("write");
    let result = deploy_render(&repo, "dev", None).expect("render");
    assert_eq!(result.files, 6);
}

#[test]
fn policy_findings_are_collected() {
    let text = refused(try_render(
        "dev",
        &["ignition/user-source", "m/a"],
        &["default"],
        &[
            // not allowlisted (D-02)
            ("resources/m/unlisted/x/config.json", "{}"),
            // denied, allowlisted
            ("resources/ignition/user-source/u/config.json", "{}"),
            // gateway-owned
            ("resources/m/a/x/config-mode.json", "{}"),
            // dot-name
            ("resources/m/a/.hidden/config.json", "{}"),
            // undeclared provider and unknown tag type
            ("tags/rogue/tag-provider/config.json", "{}"),
            ("tags/default/tag-widget/config.json", "{}"),
            // tag type under resources
            ("resources/ignition/tag-group/P/Default/config.json", "{}"),
            // unparseable JSON (stage 5 joins the same error)
            ("resources/m/a/y/config.json", "{ broken"),
            // overlay finding
            ("modes/dev/resources/m/a/z/config.json", "{ broken"),
        ],
    ));
    for needle in [
        "gateways/g/resources/m/unlisted/",
        "gateways/g/resources/ignition/user-source/",
        "gateways/g/manifest.yaml",
        "gateways/g/resources/m/a/x/config-mode.json",
        "gateways/g/resources/m/a/.hidden",
        "gateways/g/tags/rogue/",
        "gateways/g/tags/default/tag-widget/config.json",
        "gateways/g/resources/ignition/tag-group/",
        "gateways/g/resources/m/a/y/config.json",
        "gateways/g/modes/dev/resources/m/a/z/config.json",
    ] {
        assert!(text.contains(needle), "missing {needle}: {text}");
    }
}

// ---- 21-07: resource identity (attributes.uuid, DRND-06) -------------------

use ignition_core::deploy::uuid::name_based_uuid;

fn text_at(o: &RenderOutcome, path: &str) -> String {
    String::from_utf8(bytes_at(o, path).to_vec()).expect("UTF-8")
}

/// A real gateway-format `resource.json` (Gson pretty, no trailing newline)
/// with the attribute lines between `{` and `}` given by `attributes`.
fn rj_with_attributes(attributes: &str) -> String {
    format!(
        "{{\n  \"scope\": \"A\",\n  \"version\": 1,\n  \"restricted\": false,\n  \"overridable\": true,\n  \"files\": [\n    \"config.json\"\n  ],\n  \"attributes\": {{\n{attributes}\n  }}\n}}"
    )
}

fn one_named(rj: &str) -> Result<RenderOutcome, CoreError> {
    try_render(
        "dev",
        &["ignition/schedule"],
        &[],
        &[
            ("resources/ignition/schedule/x/config.json", "{}"),
            ("resources/ignition/schedule/x/resource.json", rj),
        ],
    )
}

#[test]
fn new_named_resource_gets_resource_json_with_uuid() {
    let files = [
        ("resources/ignition/schedule/x/config.json", "{}"),
        ("resources/ignition/schedule/x/index.css", "a {}"),
        ("resources/ignition/schedule/x/sub/z.css", "b {}"),
    ];
    let out = try_render("dev", &["ignition/schedule"], &[], &files).unwrap();
    let uuid = name_based_uuid("g", "ignition", "schedule", "x");
    let expected = format!(
        "{{\n  \"scope\": \"A\",\n  \"version\": 1,\n  \"restricted\": false,\n  \"overridable\": true,\n  \"files\": [\n    \"config.json\",\n    \"index.css\",\n    \"sub/z.css\"\n  ],\n  \"attributes\": {{\n    \"uuid\": \"{uuid}\",\n    \"enabled\": true\n  }}\n}}"
    );
    assert_eq!(text_at(&out, "ignition/schedule/x/resource.json"), expected);
    assert!(!expected.ends_with('\n'), "no trailing newline");
    assert_eq!(out.uuids_minted, ["ignition/schedule/x"]);
    assert_eq!(out.resources.len(), 1);
    assert_eq!(
        out.resources[0].files,
        ["config.json", "index.css", "resource.json", "sub/z.css"]
    );
    assert!(out.opaque_files.is_empty(), "the Gson writer's own output");
    // the member files are untouched
    assert_eq!(bytes_at(&out, "ignition/schedule/x/index.css"), b"a {}");
    // and repeatable
    let again = try_render("dev", &["ignition/schedule"], &[], &files).unwrap();
    assert_eq!(out, again);
}

#[test]
fn missing_uuid_is_injected_in_gateway_format_resource_json() {
    let uuid = name_based_uuid("g", "ignition", "schedule", "x");
    let line = format!("    \"uuid\": \"{uuid}\",\n");
    let last_mod = "    \"lastModification\": {\n      \"actor\": \"a\",\n      \"timestamp\": \"2026-03-01T18:14:26Z\"\n    },\n";
    let sig = "    \"lastModificationSignature\": \"4de66e8b\",\n";
    let cases = [
        // after lastModification, before the signature
        (
            format!("{last_mod}{sig}    \"enabled\": true"),
            format!("{last_mod}{line}{sig}    \"enabled\": true"),
        ),
        // after legacyId when present
        (
            format!("{last_mod}    \"legacyId\": 3,\n{sig}    \"enabled\": true"),
            format!("{last_mod}    \"legacyId\": 3,\n{line}{sig}    \"enabled\": true"),
        ),
        // neither: first attribute
        (
            "    \"enabled\": true".to_string(),
            format!("{line}    \"enabled\": true"),
        ),
    ];
    for (before, after) in cases {
        let original = rj_with_attributes(&before);
        let out = one_named(&original).unwrap();
        assert_eq!(
            text_at(&out, "ignition/schedule/x/resource.json"),
            rj_with_attributes(&after),
            "inserted only the uuid line"
        );
        assert_eq!(out.uuids_minted, ["ignition/schedule/x"]);
        assert!(out.opaque_files.is_empty());
    }

    // an empty attributes object
    let original = "{\n  \"scope\": \"A\",\n  \"version\": 1,\n  \"restricted\": false,\n  \"overridable\": true,\n  \"files\": [\n    \"config.json\"\n  ],\n  \"attributes\": {}\n}";
    let out = one_named(original).unwrap();
    assert_eq!(
        text_at(&out, "ignition/schedule/x/resource.json"),
        original.replace(
            "\"attributes\": {}",
            &format!("\"attributes\": {{\n    \"uuid\": \"{uuid}\"\n  }}")
        )
    );

    // no attributes at all: an attributes object is appended last
    let original = "{\n  \"scope\": \"A\",\n  \"version\": 1,\n  \"restricted\": false,\n  \"overridable\": true,\n  \"files\": [\n    \"config.json\"\n  ]\n}";
    let out = one_named(original).unwrap();
    assert_eq!(
        text_at(&out, "ignition/schedule/x/resource.json"),
        original.replace(
            "\n  ]\n}",
            &format!("\n  ],\n  \"attributes\": {{\n    \"uuid\": \"{uuid}\"\n  }}\n}}")
        )
    );
}

#[test]
fn opaque_resource_json_without_uuid_is_refused() {
    // compact, so not in the gateway's writer format
    let text = refused(one_named(
        "{\"scope\":\"A\",\"version\":1,\"attributes\":{\"enabled\":true}}",
    ));
    assert!(
        text.contains("gateways/g/resources/ignition/schedule/x/resource.json"),
        "{text}"
    );
    assert!(text.contains("not in gateway format"), "{text}");
    assert!(text.contains("will not rewrite"), "{text}");
}

#[test]
fn existing_uuid_is_kept_byte_for_byte() {
    // upper-case hex is a valid 8-4-4-4-12 uuid and is not normalized
    let pretty = rj_with_attributes(
        "    \"uuid\": \"4A14D749-BB78-47C7-847A-CAFA17DA4DA8\",\n    \"enabled\": true",
    );
    let out = one_named(&pretty).unwrap();
    assert_eq!(
        bytes_at(&out, "ignition/schedule/x/resource.json"),
        pretty.as_bytes()
    );
    assert!(out.uuids_minted.is_empty());

    // an opaque (compact) file with a valid uuid passes through untouched
    let compact =
        "{\"scope\":\"A\",\"attributes\":{\"uuid\":\"4a14d749-bb78-47c7-847a-cafa17da4da8\"}}";
    let out = one_named(compact).unwrap();
    assert_eq!(
        bytes_at(&out, "ignition/schedule/x/resource.json"),
        compact.as_bytes()
    );
    assert!(out.uuids_minted.is_empty());
    assert_eq!(
        out.opaque_files,
        ["ignition/schedule/x/resource.json"],
        "reported as opaque, not rewritten"
    );
}

#[test]
fn invalid_existing_uuid_is_refused() {
    let bad = [
        ("\"\"", "is empty"),
        ("null", "is null"),
        ("42", "not a string"),
        ("[\"x\"]", "not a string"),
        ("\"not-a-uuid-at-all\"", "8-4-4-4-12"),
        ("\"4a14d749bb7847c7847acafa17da4da8\"", "8-4-4-4-12"),
        ("\"4a14d749-bb78-47c7-847a-cafa17da4daZ\"", "8-4-4-4-12"),
    ];
    for (value, why) in bad {
        let rj = rj_with_attributes(&format!("    \"uuid\": {value},\n    \"enabled\": true"));
        let text = refused(one_named(&rj));
        assert!(
            text.contains("gateways/g/resources/ignition/schedule/x/resource.json"),
            "{value}: {text}"
        );
        assert!(text.contains(why), "{value}: {text}");
        assert!(
            !text.contains("not-a-uuid-at-all") && !text.contains("4a14d749bb78"),
            "never echoes the value: {text}"
        );
    }
    // the same defect in an opaque file is refused too, never replaced
    let text = refused(one_named("{\"attributes\":{\"uuid\":\"\"}}"));
    assert!(text.contains("is empty"), "{text}");
    // attributes of the wrong kind, and a non-object document
    let text = refused(one_named(
        "{\n  \"scope\": \"A\",\n  \"attributes\": null\n}",
    ));
    assert!(text.contains("attributes is not a JSON object"), "{text}");
    let text = refused(one_named("[1]"));
    assert!(text.contains("not a JSON object"), "{text}");
}

#[test]
fn singleton_and_named_no_uuid_get_none() {
    let singleton =
        |files: &[(&str, &str)]| try_render("dev", &["ignition/system-properties"], &[], files);
    let sp = "resources/ignition/system-properties";
    let cfg = format!("{sp}/config.json");
    let rj = format!("{sp}/resource.json");

    // a valid singleton renders with no uuid and nothing minted
    let out = singleton(&[(cfg.as_str(), "{}"), (rj.as_str(), SINGLETON_RJ)]).unwrap();
    assert_eq!(
        bytes_at(&out, "ignition/system-properties/resource.json"),
        SINGLETON_RJ.as_bytes()
    );
    assert!(out.uuids_minted.is_empty());

    // no resource.json: refused, not synthesized
    let text = refused(singleton(&[(cfg.as_str(), "{}")]));
    assert!(text.contains("ignition/system-properties"), "{text}");
    assert!(text.contains("no resource.json"), "{text}");

    // a uuid on a singleton is a shape change
    let with_uuid = rj_with_attributes(
        "    \"uuid\": \"4a14d749-bb78-47c7-847a-cafa17da4da8\",\n    \"enabled\": true",
    );
    let text = refused(singleton(&[
        (cfg.as_str(), "{}"),
        (rj.as_str(), with_uuid.as_str()),
    ]));
    assert!(text.contains("gateways/g/resources/ignition/system-properties/resource.json"));
    assert!(text.contains("singleton"), "{text}");

    // NamedNoUuid: themes and store-and-forward-engine
    for (module, ty) in [
        ("com.inductiveautomation.perspective", "themes"),
        ("ignition", "store-and-forward-engine"),
    ] {
        let entry = format!("{module}/{ty}");
        let dir = format!("resources/{module}/{ty}/x");
        let cfg = format!("{dir}/config.json");
        let rj = format!("{dir}/resource.json");
        let render = |files: &[(&str, &str)]| try_render("dev", &[entry.as_str()], &[], files);

        // never synthesized
        let text = refused(render(&[(cfg.as_str(), "{}")]));
        assert!(
            text.contains(&format!("gateways/g/{dir}/resource.json")),
            "{text}"
        );
        assert!(text.contains("no resource.json"), "{text}");

        // an existing one without a uuid is left exactly as it is
        let out = render(&[(cfg.as_str(), "{}"), (rj.as_str(), NO_UUID_RJ)]).unwrap();
        assert_eq!(
            bytes_at(&out, &format!("{module}/{ty}/x/resource.json")),
            NO_UUID_RJ.as_bytes()
        );
        assert!(out.uuids_minted.is_empty(), "{entry}");

        // a gateway-written uuid (CD3: store-and-forward-engine carries one on
        // 8.3.6) is kept, not refused and not recomputed
        let out = render(&[(cfg.as_str(), "{}"), (rj.as_str(), NAMED_RJ)]).unwrap();
        assert_eq!(
            bytes_at(&out, &format!("{module}/{ty}/x/resource.json")),
            NAMED_RJ.as_bytes()
        );
        assert!(out.uuids_minted.is_empty());

        // but a malformed one is still refused
        let bad = rj_with_attributes("    \"uuid\": \"\",\n    \"enabled\": true");
        let text = refused(render(&[(cfg.as_str(), "{}"), (rj.as_str(), bad.as_str())]));
        assert!(text.contains("is empty"), "{text}");
    }
}

#[test]
fn type_without_rule_is_refused() {
    // an allowlisted type with no evidence, with files
    let text = refused(try_render(
        "dev",
        &["ignition/made-up-type"],
        &[],
        &[
            ("resources/ignition/made-up-type/a/config.json", "{}"),
            ("resources/ignition/made-up-type/b/config.json", "{}"),
        ],
    ));
    assert!(text.contains("gateways/g/manifest.yaml"), "{text}");
    assert!(
        text.contains("no render rule for ignition/made-up-type"),
        "{text}"
    );
    assert!(
        text.contains("gateways/g/resources/ignition/made-up-type/"),
        "{text}"
    );
    assert!(text.contains("only evidence-backed types render"), "{text}");

    // an allowlisted type with no files is refused on the manifest entry alone
    let text = refused(try_render("dev", &["m/never-seen"], &[], &[]));
    assert!(text.contains("gateways/g/manifest.yaml"), "{text}");
    assert!(text.contains("no render rule for m/never-seen"), "{text}");

    // a type that is not allowlisted keeps its single D-02 finding
    let text = refused(try_render(
        "dev",
        &["ignition/schedule"],
        &[],
        &[("resources/ignition/made-up-type/a/config.json", "{}")],
    ));
    assert!(text.contains("type not in manifest.yaml types"), "{text}");
    assert!(!text.contains("no render rule"), "{text}");

    // denied and tag types are reported once, as what they are
    let text = refused(try_render(
        "dev",
        &["ignition/security-levels"],
        &[],
        &[("resources/ignition/security-levels/config.json", "{}")],
    ));
    assert!(text.contains("denied type"), "{text}");
    assert!(!text.contains("no render rule"), "{text}");
}

#[test]
fn rule_structure_mismatch_is_refused() {
    // a singleton type with a name directory, a named type with a loose file
    let text = refused(try_render(
        "dev",
        &["ignition/system-properties", "ignition/schedule"],
        &[],
        &[
            ("resources/ignition/system-properties/x/config.json", "{}"),
            ("resources/ignition/system-properties/x/other.json", "{}"),
            ("resources/ignition/schedule/loose.json", "{}"),
            ("resources/ignition/schedule/ok/config.json", "{}"),
        ],
    ));
    assert_eq!(
        text.matches("gateways/g/resources/ignition/system-properties/x/")
            .count(),
        1,
        "one finding per name directory: {text}"
    );
    assert!(text.contains("is a singleton type"), "{text}");
    assert!(
        text.contains("gateways/g/resources/ignition/schedule/loose.json"),
        "{text}"
    );
    assert!(text.contains("is a named type"), "{text}");
    assert!(!text.contains("schedule/ok"), "{text}");

    // an overlay path is named as the overlay
    let text = refused(try_render(
        "dev",
        &["ignition/schedule"],
        &[],
        &[("modes/dev/resources/ignition/schedule/loose.json", "{}")],
    ));
    assert!(
        text.contains("gateways/g/modes/dev/resources/ignition/schedule/loose.json"),
        "{text}"
    );
}

#[test]
fn uuid_is_stable_across_envs_and_differs_across_gateways() {
    let manifest = Manifest::parse(
        "manifest.yaml",
        b"schema_version: 1\ntypes:\n  - ignition/schedule\n",
    )
    .expect("manifest");
    let target = Target::parse("targets/dev.yaml", TARGET).expect("target");
    let source = SourceTree::from_files([("resources/ignition/schedule/x/config.json", "{}")])
        .expect("tree");
    let render_for = |gateway: &str, env: &str| {
        render(&RenderInput {
            gateway,
            env,
            manifest: &manifest,
            target: &target,
            source: &source,
        })
        .expect("render")
    };
    let dev = render_for("gw-a", "dev");
    let staging = render_for("gw-a", "staging");
    let other_gateway = render_for("gw-b", "dev");

    let rj = "ignition/schedule/x/resource.json";
    assert_eq!(
        bytes_at(&dev, rj),
        bytes_at(&staging, rj),
        "env is not an input"
    );
    assert_ne!(
        bytes_at(&dev, rj),
        bytes_at(&other_gateway, rj),
        "the gateway is an input"
    );
    let text = text_at(&dev, rj);
    assert!(text.contains(&name_based_uuid("gw-a", "ignition", "schedule", "x")));
    assert_eq!(dev.uuids_minted, ["ignition/schedule/x"]);
}

// ---- 21-08 task 1: the secret lint ---------------------------------------

const CANARY_EMBEDDED: &str = "CANARY-EMBEDDED-21";
const CANARY_PLAINTEXT: &str = "CANARY-PLAINTEXT-21";
const CANARY_KEYNAME: &str = "CANARY-KEYNAME-21";
const CANARY_WARN: &str = "CANARY-WARN-21";

const DB: &str = "resources/ignition/database-connection/Main/config.json";
const MQTT: &str = "resources/com.cirruslink.mqtt.engine.gateway/server/Broker/config.json";
const MQTT_TYPE: &str = "com.cirruslink.mqtt.engine.gateway/server";

/// Render `files` allowing `types`; returns the result unchanged.
fn secret_render(types: &[&str], files: &[(&str, &str)]) -> Result<RenderOutcome, CoreError> {
    try_render("dev", types, &[], files)
}

/// No canary anywhere a human or agent can read it: error Display and Debug
/// (which is also what `--json` serialises from), and every warning.
fn assert_no_canary(result: &Result<RenderOutcome, CoreError>, canaries: &[&str]) {
    let mut shown = String::new();
    match result {
        Ok(o) => shown.push_str(&o.warnings.join("\n")),
        Err(e) => {
            shown.push_str(&e.to_string());
            shown.push_str(&format!("{e:?}"));
        }
    }
    for canary in canaries {
        assert!(!shown.contains(canary), "{canary} leaked into: {shown}");
    }
}

#[test]
fn embedded_secret_is_refused_by_path() {
    let config = format!(
        r#"{{"password":{{"type":"Embedded","data":{{"ciphertext":"{CANARY_EMBEDDED}"}}}},"url":"jdbc:x"}}"#
    );
    let result = secret_render(&["ignition/database-connection"], &[(DB, &config)]);
    assert_no_canary(&result, &[CANARY_EMBEDDED]);
    let err = result.expect_err("embedded secret is refused");
    assert_eq!(err.code(), "embedded_secret");
    assert_eq!(err.exit_code(), 2);
    let text = err.to_string();
    assert!(
        text.contains(
            "gateways/g/resources/ignition/database-connection/Main/config.json: password (embedded)"
        ),
        "{text}"
    );
    // reported once even though the path is also a schema path
    assert_eq!(text.matches("password (").count(), 1, "{text}");
}

#[test]
fn embedded_anywhere_is_refused() {
    let config = format!(
        r#"{{"a":{{"b":[{{"ok":1}},{{"c":{{"type":"Embedded","data":{{"x":"{CANARY_EMBEDDED}"}}}}}}]}}}}"#
    );
    let result = secret_render(
        &["ignition/schedule"],
        &[("resources/ignition/schedule/S/config.json", &config)],
    );
    assert_no_canary(&result, &[CANARY_EMBEDDED]);
    let text = result.expect_err("refused").to_string();
    assert!(text.contains("config.json: a.b[1].c (embedded)"), "{text}");
}

#[test]
fn embedded_in_any_json_member_is_refused() {
    let side = format!(r#"{{"k":{{"type":"Embedded","data":{{"x":"{CANARY_EMBEDDED}"}}}}}}"#);
    let result = secret_render(
        &["ignition/schedule"],
        &[
            ("resources/ignition/schedule/S/config.json", "{}"),
            ("resources/ignition/schedule/S/extra.json", &side),
        ],
    );
    assert_no_canary(&result, &[CANARY_EMBEDDED]);
    let text = result.expect_err("refused").to_string();
    assert!(text.contains("extra.json: k (embedded)"), "{text}");
}

#[test]
fn plaintext_secret_at_schema_path_is_refused() {
    let config = format!(r#"{{"password":"{CANARY_PLAINTEXT}","settings":{{"x":1}}}}"#);
    let result = secret_render(&["ignition/database-connection"], &[(DB, &config)]);
    assert_no_canary(&result, &[CANARY_PLAINTEXT]);
    let err = result.expect_err("plaintext secret is refused");
    assert_eq!(err.code(), "embedded_secret");
    assert!(
        err.to_string()
            .contains("config.json: password (plaintext)"),
        "{err}"
    );

    // a nested schema path, and a non-string, non-null value
    let result = secret_render(
        &["ignition/email-profile"],
        &[(
            "resources/ignition/email-profile/Mail/config.json",
            &format!(r#"{{"settings":{{"settings":{{"password":"{CANARY_PLAINTEXT}"}}}}}}"#),
        )],
    );
    assert_no_canary(&result, &[CANARY_PLAINTEXT]);
    assert!(
        result
            .expect_err("refused")
            .to_string()
            .contains("Mail/config.json: settings.settings.password (plaintext)")
    );
    let result = secret_render(
        &["ignition/database-connection"],
        &[(DB, r#"{"password":123}"#)],
    );
    assert!(
        result
            .expect_err("refused")
            .to_string()
            .contains("password (plaintext)")
    );
}

#[test]
fn referenced_and_null_secrets_pass() {
    let referenced = r#"{"password":{"data":{"providerName":"vault","secretName":"db-pass"},"type":"Referenced"}}"#;
    for config in [referenced, r#"{"password":null}"#, r#"{"url":"x"}"#] {
        let out = secret_render(&["ignition/database-connection"], &[(DB, config)])
            .unwrap_or_else(|e| panic!("{config} refused: {e}"));
        assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    }
}

#[test]
fn possible_plaintext_secret_warns_without_value() {
    let config = format!(r#"{{"settings":{{"mqttPassword":"{CANARY_WARN}"}}}}"#);
    let result = secret_render(&["ignition/database-connection"], &[(DB, &config)]);
    assert_no_canary(&result, &[CANARY_WARN]);
    let out = result.expect("a described type only warns");
    assert_eq!(out.warnings.len(), 1, "{:?}", out.warnings);
    assert!(out.warnings[0].starts_with("possible plaintext secret at gateways/g/"));
    assert!(out.warnings[0].contains("config.json: settings.mqttPassword;"));
}

#[test]
fn plaintext_by_key_name_in_undescribed_type_is_refused() {
    let config = format!(
        r#"{{"password":"{CANARY_KEYNAME}","settings":{{"clientSecret":"{CANARY_KEYNAME}"}},"url":"tcp://x"}}"#
    );
    let result = secret_render(&[MQTT_TYPE], &[(MQTT, &config)]);
    assert_no_canary(&result, &[CANARY_KEYNAME]);
    let err = result.expect_err("refused");
    assert_eq!(err.code(), "embedded_secret");
    let text = err.to_string();
    assert!(
        text.contains("Broker/config.json: password (plaintext, by key name)"),
        "{text}"
    );
    assert!(
        text.contains("Broker/config.json: settings.clientSecret (plaintext, by key name)"),
        "{text}"
    );
    // an array of strings under a secret-looking key is caught too
    let result = secret_render(
        &[MQTT_TYPE],
        &[(MQTT, &format!(r#"{{"passwords":["{CANARY_KEYNAME}"]}}"#))],
    );
    assert_no_canary(&result, &[CANARY_KEYNAME]);
    assert!(
        result
            .expect_err("refused")
            .to_string()
            .contains("passwords[0] (plaintext, by key name)")
    );
}

#[test]
fn empty_or_referenced_key_name_values_pass_in_undescribed_type() {
    let referenced = r#"{"password":{"data":{"providerName":"v","secretName":"mqtt-pass"},"type":"Referenced"},"settings":{"clientSecret":{"data":{"providerName":"v","secretName":"cs"},"type":"Referenced"}}}"#;
    for config in [
        r#"{"password":""}"#,
        r#"{"password":null}"#,
        r#"{"passwordMinLength":8,"secretCount":2,"requirePassword":true}"#,
        referenced,
    ] {
        let out = secret_render(&[MQTT_TYPE], &[(MQTT, config)])
            .unwrap_or_else(|e| panic!("{config} refused: {e}"));
        assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    }
}

#[test]
fn key_name_rule_is_limited_to_config_json() {
    // a sibling member is not config.json: only the Embedded rule applies
    let out = secret_render(
        &[MQTT_TYPE],
        &[
            (MQTT, "{}"),
            (
                "resources/com.cirruslink.mqtt.engine.gateway/server/Broker/extra.json",
                r#"{"password":"x"}"#,
            ),
        ],
    )
    .expect("renders");
    assert!(out.warnings.is_empty());
}

#[test]
fn secret_findings_win_and_carry_other_findings() {
    let config = format!(r#"{{"password":"{CANARY_PLAINTEXT}"}}"#);
    let result = secret_render(
        &["ignition/database-connection"],
        &[
            (DB, &config),
            ("resources/other/unlisted/x/config.json", "{}"),
        ],
    );
    assert_no_canary(&result, &[CANARY_PLAINTEXT]);
    let err = result.expect_err("refused");
    assert_eq!(err.code(), "embedded_secret");
    let text = err.to_string();
    assert!(text.contains("password (plaintext)"), "{text}");
    assert!(text.contains("also"), "{text}");
    assert!(text.contains("resources/other/unlisted/"), "{text}");
}

// ---- 21-08 task 2: Windows names, case folds, path budget, CR ------------

/// Render byte members against an explicit target document.
fn try_render_bytes(
    target_yaml: &[u8],
    types: &[&str],
    files: &[(&str, &[u8])],
) -> Result<RenderOutcome, CoreError> {
    let mut yaml = String::from("schema_version: 1\ntypes:\n");
    for t in types {
        yaml.push_str(&format!("  - {t}\n"));
    }
    let manifest = Manifest::parse("manifest.yaml", yaml.as_bytes()).expect("manifest");
    let target = Target::parse("targets/dev.yaml", target_yaml).expect("target");
    let source = SourceTree::from_files(files.iter().copied()).expect("tree");
    render(&RenderInput {
        gateway: "g",
        env: "dev",
        manifest: &manifest,
        target: &target,
        source: &source,
    })
}

#[test]
fn cr_in_jar_is_not_refused() {
    // the corpus stand-in jar contains 0x0D bytes and a NUL, like a real
    // binary; the driver's JSON files carry no CR
    let jar = include_bytes!(
        "fixtures/deploy/corpus/ignition/database-driver/MariaDB/mariadb-java-client-3.3.3.jar"
    );
    let config =
        include_bytes!("fixtures/deploy/corpus/ignition/database-driver/MariaDB/config.json");
    let resource =
        include_bytes!("fixtures/deploy/corpus/ignition/database-driver/MariaDB/resource.json");
    assert!(jar.contains(&b'\r'), "the stand-in jar must contain CR");
    let out = try_render_bytes(
        TARGET,
        &["ignition/database-driver"],
        &[
            (
                "resources/ignition/database-driver/MariaDB/mariadb-java-client-3.3.3.jar",
                jar,
            ),
            (
                "resources/ignition/database-driver/MariaDB/config.json",
                config,
            ),
            (
                "resources/ignition/database-driver/MariaDB/resource.json",
                resource,
            ),
        ],
    )
    .expect("a jar with CR bytes renders");
    assert_eq!(
        bytes_at(
            &out,
            "ignition/database-driver/MariaDB/mariadb-java-client-3.3.3.jar"
        ),
        jar.as_slice()
    );
}

#[test]
fn cr_in_text_member_is_refused_through_render() {
    let result = try_render_bytes(
        TARGET,
        &["ignition/schedule"],
        &[
            ("resources/ignition/schedule/S/config.json", b"{}"),
            ("resources/ignition/schedule/S/notes.txt", b"one\r\ntwo\n"),
        ],
    );
    let err = result.expect_err("CR in a text member is refused");
    assert_eq!(err.code(), "deploy_tree_invalid");
    let text = err.to_string();
    assert!(
        text.contains("gateways/g/resources/ignition/schedule/S/notes.txt:1:4: carriage return"),
        "{text}"
    );
}

#[test]
fn empty_member_and_empty_tree_edges() {
    // a zero-byte non-JSON member renders as a zero-byte file and passes the CR lint
    let out = try_render_bytes(
        TARGET,
        &["ignition/schedule"],
        &[
            ("resources/ignition/schedule/S/config.json", b"{}"),
            ("resources/ignition/schedule/S/empty.txt", b""),
        ],
    )
    .expect("renders");
    assert_eq!(bytes_at(&out, "ignition/schedule/S/empty.txt"), b"");
    // an empty source contributes no file and no resource entry
    let out = try_render_bytes(TARGET, &["ignition/schedule"], &[]).expect("renders");
    assert!(out.tree.files.is_empty() && out.resources.is_empty());
}

#[test]
fn lints_apply_to_tag_files_and_every_environment() {
    // the name rules cover tag-origin paths too (mapped to ignition/<type>/<P>/...)
    let result = try_render(
        "dev",
        &["ignition/schedule"],
        &["default"],
        &[("tags/default/tag-definition/Fo:lder/tags.json", "{}")],
    );
    let text = refused(result);
    assert!(text.contains("path component `Fo:lder`"), "{text}");
}

#[test]
fn all_lint_findings_reported_together() {
    let long_name = "x".repeat(200);
    let long_path = format!("resources/ignition/schedule/{long_name}/config.json");
    let files: Vec<(&str, &[u8])> = vec![
        ("resources/ignition/schedule/a:b/config.json", b"{}"),
        ("resources/ignition/schedule/Pump/config.json", b"{}"),
        ("resources/ignition/schedule/pump/config.json", b"{}"),
        ("resources/ignition/schedule/NUL/config.json", b"{}"),
        ("resources/ignition/schedule/trail./config.json", b"{}"),
        ("resources/ignition/schedule/S/notes.txt", b"a\r\nb"),
        (long_path.as_str(), b"{}"),
    ];
    let err = try_render_bytes(TARGET, &["ignition/schedule"], &files).expect_err("refused");
    assert_eq!(err.code(), "deploy_tree_invalid");
    let text = err.to_string();
    for want in [
        "path component `a:b`",
        "sibling names collide when case is folded",
        "`ignition/schedule/Pump`",
        "`ignition/schedule/pump`",
        "path component `NUL`",
        "path component `trail.`",
        "notes.txt:1:2: carriage return",
        "UTF-16 code units long on Windows",
    ] {
        assert!(text.contains(want), "{want} missing from: {text}");
    }

    // with a secret too, one embedded_secret lists the secret path and the rest
    let mut with_secret = files.clone();
    let secret = br#"{"password":{"type":"Embedded","data":{"c":"CANARY-EMBEDDED-21"}}}"#;
    with_secret.push(("resources/ignition/schedule/Sec/config.json", secret));
    let err = try_render_bytes(TARGET, &["ignition/schedule"], &with_secret).expect_err("refused");
    assert_eq!(err.code(), "embedded_secret");
    let text = err.to_string();
    assert!(
        text.contains("Sec/config.json: password (embedded)"),
        "{text}"
    );
    assert!(text.contains("also"), "{text}");
    assert!(text.contains("path component `a:b`"), "{text}");
    assert!(!text.contains("CANARY-EMBEDDED-21"), "{text}");
}

#[test]
fn windows_node_data_dir_lowers_the_render_budget() {
    // default prefix 51 + 18 + 3 (`dev`) + 1 = 73: the longest relative path
    // that passes is 174 units; with a 12-unit-longer Windows data_dir it is 162
    let relative = |units: usize| {
        let prefix = "ignition/schedule/S/";
        format!("resources/{prefix}{}", "a".repeat(units - prefix.len()))
    };
    let linux = TARGET;
    let windows = format!(
        "schema_version: 1\ngateway: g\nmode: dev\nnodes:\n  master:\n    profile: dev\n    ssh:\n      host: h\n      user: u\n      os: windows\n      data_dir: 'D:\\{}'\n",
        "d".repeat(60)
    );
    let at_limit = |target: &[u8], units: usize| {
        let path = relative(units);
        try_render_bytes(
            target,
            &["ignition/schedule"],
            &[
                ("resources/ignition/schedule/S/config.json", b"{}"),
                (path.as_str(), b"x"),
            ],
        )
    };
    assert!(
        at_limit(linux, 174).is_ok(),
        "174 passes with the default prefix"
    );
    assert!(at_limit(linux, 175).is_err(), "175 is refused");
    assert!(at_limit(windows.as_bytes(), 162).is_ok());
    assert!(at_limit(windows.as_bytes(), 163).is_err());
}

// ---- 21-09 task 1: --out and the clobber ladder ---------------------------

use ignition_core::deploy::tree::{RENDER_MARKER, RenderMarker};

/// A scratch copy of the tracer repo plus a second, empty scratch area for
/// `--out` targets. Tempdirs only: nothing here touches the real fixtures.
fn out_fixture() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    copy_dir(Path::new(TRACER_REPO), &repo);
    (tmp, repo)
}

const ALARM_CONFIG: &str =
    "gateways/tracer-gw/resources/ignition/general-alarm-settings/config.json";

/// Every file under `out` (relative, `/`-separated), marker included.
fn out_files(out: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, root: &Path, acc: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).expect("read_dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                walk(&path, root, acc);
            } else {
                let rel = path.strip_prefix(root).expect("prefix");
                acc.insert(
                    rel.to_string_lossy().replace('\\', "/"),
                    std::fs::read(&path).expect("read"),
                );
            }
        }
    }
    let mut acc = BTreeMap::new();
    walk(out, out, &mut acc);
    acc
}

fn read_marker(out: &Path) -> RenderMarker {
    serde_json::from_slice(&std::fs::read(out.join(RENDER_MARKER)).expect("marker")).expect("json")
}

fn invalid_input(err: &ignition_core::error::CoreError) -> String {
    assert_eq!(err.code(), "invalid_input", "{err}");
    assert_eq!(err.exit_code(), 2);
    err.to_string()
}

#[test]
fn out_absent_is_created_and_matches_render() {
    let (tmp, repo) = out_fixture();
    let out = tmp.path().join("deep/er/rendered");
    let result = deploy_render(&repo, "dev", Some(&out)).expect("render");
    assert_eq!(result.out.as_deref(), Some(out.to_string_lossy().as_ref()));

    let marker = read_marker(&out);
    assert_eq!(marker.schema_version, 1);
    assert_eq!(marker.gateway, "tracer-gw");
    assert_eq!(marker.env, "dev");
    assert_eq!(marker.mode, "dev");
    assert_eq!(marker.render_sha256, result.render_sha256);
    assert_eq!(marker.files.len(), result.files);

    // every listed file is on disk byte-for-byte as listed, and the marker
    // lists exactly the files on disk
    let on_disk = out_files(&out);
    let listed: Vec<&str> = marker.files.iter().map(|f| f.path.as_str()).collect();
    let mut sorted = listed.clone();
    sorted.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    assert_eq!(listed, sorted, "marker files are byte-sorted");
    for f in &marker.files {
        let bytes = &on_disk[&f.path];
        assert_eq!(sha256_hex(bytes), f.sha256);
        assert_eq!(bytes.len() as u64, f.bytes);
    }
    let mut expected = listed.clone();
    expected.push(RENDER_MARKER);
    expected.sort_unstable();
    assert_eq!(
        on_disk.keys().map(String::as_str).collect::<Vec<_>>(),
        expected
    );

    // the digest of what landed on disk is the reported digest, computed
    // from scratch (never via the production digest function)
    let entries: Vec<(&str, &[u8])> = marker
        .files
        .iter()
        .map(|f| (f.path.as_str(), on_disk[&f.path].as_slice()))
        .collect();
    assert_eq!(listing_digest(&entries), result.render_sha256);

    // the marker ends with exactly one LF
    let raw = std::fs::read(out.join(RENDER_MARKER)).expect("marker");
    assert!(raw.ends_with(b"}\n") && !raw.ends_with(b"\n\n"));
}

#[test]
fn out_empty_dir_is_used() {
    let (tmp, repo) = out_fixture();
    let out = tmp.path().join("empty");
    std::fs::create_dir(&out).expect("mkdir");
    let result = deploy_render(&repo, "dev", Some(&out)).expect("render");
    assert_eq!(out_files(&out).len(), result.files + 1);
}

#[test]
fn out_refresh_of_same_gateway_env_overwrites_listed_files() {
    let (tmp, repo) = out_fixture();
    let out = tmp.path().join("out");
    let first = deploy_render(&repo, "dev", Some(&out)).expect("first");

    let cfg = repo.join(ALARM_CONFIG);
    let text = std::fs::read_to_string(&cfg).expect("config");
    assert!(text.contains("\"liveEventLimit\": 5"));
    std::fs::write(
        &cfg,
        text.replace("\"liveEventLimit\": 5", "\"liveEventLimit\": 6"),
    )
    .expect("write");

    let second = deploy_render(&repo, "dev", Some(&out)).expect("refresh");
    assert_ne!(first.render_sha256, second.render_sha256);
    assert_eq!(read_marker(&out).render_sha256, second.render_sha256);
    let landed = std::fs::read_to_string(out.join("ignition/general-alarm-settings/config.json"))
        .expect("file");
    assert!(landed.contains("\"liveEventLimit\": 6"));
    // no temporary file is left behind
    assert!(
        out_files(&out)
            .keys()
            .all(|k| !k.ends_with(".ign-render-tmp"))
    );

    // an identical refresh is accepted too
    deploy_render(&repo, "dev", Some(&out)).expect("idempotent");
}

#[test]
fn out_refresh_refuses_when_new_render_drops_a_file() {
    let (tmp, repo) = out_fixture();
    let out = tmp.path().join("out");
    deploy_render(&repo, "dev", Some(&out)).expect("first");
    let before = snapshot(&out);

    let manifest = repo.join("gateways/tracer-gw/manifest.yaml");
    let text = std::fs::read_to_string(&manifest).expect("manifest");
    std::fs::write(
        &manifest,
        text.replace("  - ignition/general-alarm-settings\n", ""),
    )
    .expect("write");
    std::fs::remove_dir_all(
        repo.join("gateways/tracer-gw/resources/ignition/general-alarm-settings"),
    )
    .expect("rm source dir");

    let err = deploy_render(&repo, "dev", Some(&out)).expect_err("dropped file");
    let reason = invalid_input(&err);
    assert!(
        reason.contains("ignition/general-alarm-settings/"),
        "{reason}"
    );
    assert!(reason.contains("no longer produces"), "{reason}");
    assert_eq!(before, snapshot(&out), "the directory is byte-unchanged");
}

#[test]
fn out_refuses_unrelated_non_empty_dir() {
    let (tmp, repo) = out_fixture();
    let out = tmp.path().join("notes");
    std::fs::create_dir(&out).expect("mkdir");
    std::fs::write(out.join("notes.txt"), b"mine").expect("write");
    let before = snapshot(&out);

    let err = deploy_render(&repo, "dev", Some(&out)).expect_err("unrelated dir");
    let reason = invalid_input(&err);
    assert!(reason.contains("notes.txt"), "{reason}");
    assert!(reason.contains(RENDER_MARKER), "{reason}");
    assert_eq!(before, snapshot(&out));
    assert_eq!(std::fs::read(out.join("notes.txt")).expect("read"), b"mine");
}

#[test]
fn out_refuses_modified_prior_render() {
    let (tmp, repo) = out_fixture();
    let out = tmp.path().join("out");
    deploy_render(&repo, "dev", Some(&out)).expect("first");
    let edited = out.join("ignition/database-translator/POSTGRES/config.json");
    let mut bytes = std::fs::read(&edited).expect("read");
    bytes.extend_from_slice(b" ");
    std::fs::write(&edited, &bytes).expect("edit");
    let before = snapshot(&out);

    let reason = invalid_input(&deploy_render(&repo, "dev", Some(&out)).expect_err("modified"));
    assert!(
        reason.contains("ignition/database-translator/POSTGRES/config.json"),
        "{reason}"
    );
    assert!(reason.contains("modified"), "{reason}");
    assert_eq!(before, snapshot(&out));
    assert_eq!(
        std::fs::read(&edited).expect("read"),
        bytes,
        "the user's edit survives"
    );
}

#[test]
fn out_refuses_unlisted_extra_file_missing_file_and_empty_subdir() {
    let (tmp, repo) = out_fixture();
    let out = tmp.path().join("out");
    deploy_render(&repo, "dev", Some(&out)).expect("first");

    // an extra file the render never wrote
    std::fs::write(out.join("ignition/extra.txt"), b"x").expect("write");
    let reason = invalid_input(&deploy_render(&repo, "dev", Some(&out)).expect_err("extra"));
    assert!(reason.contains("ignition/extra.txt"), "{reason}");
    std::fs::remove_file(out.join("ignition/extra.txt")).expect("cleanup");

    // a directory that holds no listed file
    std::fs::create_dir(out.join("stray")).expect("mkdir");
    let reason = invalid_input(&deploy_render(&repo, "dev", Some(&out)).expect_err("stray"));
    assert!(reason.contains("stray/"), "{reason}");
    std::fs::remove_dir(out.join("stray")).expect("cleanup");

    // a listed file that was removed
    std::fs::remove_file(out.join("ignition/general-alarm-settings/config.json")).expect("rm");
    let reason = invalid_input(&deploy_render(&repo, "dev", Some(&out)).expect_err("missing"));
    assert!(
        reason.contains("general-alarm-settings/config.json"),
        "{reason}"
    );
    assert!(reason.contains("missing"), "{reason}");
}

#[test]
fn out_refuses_other_gateway_or_env_marker() {
    let (tmp, repo) = out_fixture();
    // a second environment on the same gateway
    std::fs::copy(repo.join("targets/dev.yaml"), repo.join("targets/stg.yaml")).expect("copy");
    let out = tmp.path().join("out");
    deploy_render(&repo, "dev", Some(&out)).expect("first");
    let before = snapshot(&out);

    let reason = invalid_input(&deploy_render(&repo, "stg", Some(&out)).expect_err("other env"));
    assert!(reason.contains("env dev"), "{reason}");
    assert_eq!(before, snapshot(&out));

    // a marker that names another gateway
    let marker_path = out.join(RENDER_MARKER);
    let text = std::fs::read_to_string(&marker_path).expect("marker");
    std::fs::write(&marker_path, text.replace("\"tracer-gw\"", "\"other-gw\"")).expect("write");
    let before = snapshot(&out);
    let reason = invalid_input(&deploy_render(&repo, "dev", Some(&out)).expect_err("other gw"));
    assert!(reason.contains("other-gw"), "{reason}");
    assert_eq!(before, snapshot(&out));

    // unreadable, wrong-version and unknown-field markers
    for bad in [
        "not json".to_string(),
        text.replace("\"schema_version\": 1", "\"schema_version\": 2"),
        text.replace("\"mode\"", "\"extra\": 1,\n  \"mode\""),
    ] {
        std::fs::write(&marker_path, bad).expect("write");
        let before = snapshot(&out);
        invalid_input(&deploy_render(&repo, "dev", Some(&out)).expect_err("bad marker"));
        assert_eq!(before, snapshot(&out));
    }
}

#[test]
fn out_interrupted_write_is_refused_until_removed() {
    let (tmp, repo) = out_fixture();
    let out = tmp.path().join("out");
    deploy_render(&repo, "dev", Some(&out)).expect("first");
    // simulate a crash before the marker landed: files, no marker
    std::fs::remove_file(out.join(RENDER_MARKER)).expect("rm marker");
    let before = snapshot(&out);
    let reason = invalid_input(&deploy_render(&repo, "dev", Some(&out)).expect_err("no marker"));
    assert!(reason.contains(RENDER_MARKER), "{reason}");
    assert!(reason.contains("remove the directory"), "{reason}");
    assert_eq!(before, snapshot(&out));
}

#[test]
fn out_refuses_file_path() {
    let (tmp, repo) = out_fixture();
    let out = tmp.path().join("afile");
    std::fs::write(&out, b"keep me").expect("write");
    let reason = invalid_input(&deploy_render(&repo, "dev", Some(&out)).expect_err("file"));
    assert!(reason.contains("not a directory"), "{reason}");
    assert_eq!(std::fs::read(&out).expect("read"), b"keep me");
}

#[cfg(unix)]
#[test]
fn out_refuses_symlink_out_and_symlinked_entries() {
    use std::os::unix::fs::symlink;
    let (tmp, repo) = out_fixture();

    // the out path itself is a symlink to an empty directory
    let real = tmp.path().join("real");
    std::fs::create_dir(&real).expect("mkdir");
    let link = tmp.path().join("link");
    symlink(&real, &link).expect("symlink");
    let reason = invalid_input(&deploy_render(&repo, "dev", Some(&link)).expect_err("symlink out"));
    assert!(reason.contains("symlink"), "{reason}");
    assert!(
        std::fs::read_dir(&real).expect("read_dir").next().is_none(),
        "target untouched"
    );

    // a dangling symlink at the out path is refused too, not created through
    let dangling = tmp.path().join("dangling");
    symlink(tmp.path().join("nowhere"), &dangling).expect("symlink");
    invalid_input(&deploy_render(&repo, "dev", Some(&dangling)).expect_err("dangling"));
    assert!(!tmp.path().join("nowhere").exists());

    // a prior render that now holds a symlink entry
    let out = tmp.path().join("out");
    deploy_render(&repo, "dev", Some(&out)).expect("first");
    let outside = tmp.path().join("outside.txt");
    std::fs::write(&outside, b"outside").expect("write");
    symlink(&outside, out.join("ignition/sneaky")).expect("symlink");
    let before = snapshot(&out);
    let reason = invalid_input(&deploy_render(&repo, "dev", Some(&out)).expect_err("entry"));
    assert!(reason.contains("ignition/sneaky"), "{reason}");
    assert_eq!(before, snapshot(&out));
    assert_eq!(std::fs::read(&outside).expect("read"), b"outside");

    // a listed file replaced by a symlink is not followed or overwritten
    std::fs::remove_file(out.join("ignition/sneaky")).expect("rm");
    let listed = out.join("ignition/general-alarm-settings/config.json");
    std::fs::remove_file(&listed).expect("rm");
    symlink(&outside, &listed).expect("symlink");
    invalid_input(&deploy_render(&repo, "dev", Some(&out)).expect_err("listed link"));
    assert_eq!(std::fs::read(&outside).expect("read"), b"outside");
}

#[test]
fn refused_render_writes_nothing() {
    let (tmp, repo) = out_fixture();
    let gw = repo.join("gateways/tracer-gw");
    let manifest = gw.join("manifest.yaml");
    let base = std::fs::read_to_string(&manifest).expect("manifest");

    // a secret refusal: a database connection with an embedded password
    let db = gw.join("resources/ignition/database-connection/Main");
    std::fs::create_dir_all(&db).expect("mkdir");
    std::fs::write(
        db.join("config.json"),
        r#"{"password":{"type":"Embedded","data":{"ciphertext":"CANARY-OUT-21"}},"url":"jdbc:x"}"#,
    )
    .expect("write");
    std::fs::write(db.join("resource.json"), NAMED_RJ).expect("write");
    std::fs::write(
        &manifest,
        base.replace("types:\n", "types:\n  - ignition/database-connection\n"),
    )
    .expect("write");

    let absent = tmp.path().join("absent");
    let err = deploy_render(&repo, "dev", Some(&absent)).expect_err("secret");
    assert_eq!(err.code(), "embedded_secret");
    assert!(!absent.exists(), "an absent --out stays absent");

    let deep = tmp.path().join("new-parent/out");
    deploy_render(&repo, "dev", Some(&deep)).expect_err("secret");
    assert!(
        !tmp.path().join("new-parent").exists(),
        "no ancestor was created"
    );

    let empty = tmp.path().join("empty");
    std::fs::create_dir(&empty).expect("mkdir");
    deploy_render(&repo, "dev", Some(&empty)).expect_err("secret");
    assert!(
        std::fs::read_dir(&empty)
            .expect("read_dir")
            .next()
            .is_none(),
        "empty stays empty"
    );

    // a policy refusal: a denied type that the manifest lists
    std::fs::remove_dir_all(&db).expect("rm");
    let user = gw.join("resources/ignition/user-source/u");
    std::fs::create_dir_all(&user).expect("mkdir");
    std::fs::write(user.join("config.json"), "{}").expect("write");
    std::fs::write(
        &manifest,
        base.replace("types:\n", "types:\n  - ignition/user-source\n"),
    )
    .expect("write");
    let err = deploy_render(&repo, "dev", Some(&absent)).expect_err("policy");
    assert_eq!(err.code(), "deploy_tree_invalid");
    assert!(!absent.exists());
    deploy_render(&repo, "dev", Some(&empty)).expect_err("policy");
    assert!(
        std::fs::read_dir(&empty)
            .expect("read_dir")
            .next()
            .is_none()
    );
}

#[test]
fn out_second_writer_is_refused_and_nothing_is_replaced() {
    use ignition_core::deploy::layout::RelPath;
    use ignition_core::deploy::tree::{RenderedTree, write_out};
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path().join("race");
    let mut tree = RenderedTree::default();
    tree.files
        .insert(RelPath::new("a/b").expect("path"), b"one".to_vec());
    let marker = RenderMarker::for_tree("g", "dev", "dev", &tree);

    write_out(&tree, &out, &marker).expect("first writer");
    // a competitor's output without our marker: refused, never replaced
    std::fs::remove_file(out.join(RENDER_MARKER)).expect("rm marker");
    std::fs::write(out.join("a/b"), b"theirs").expect("write");
    invalid_input(&write_out(&tree, &out, &marker).expect_err("second writer"));
    assert_eq!(std::fs::read(out.join("a/b")).expect("read"), b"theirs");
}
