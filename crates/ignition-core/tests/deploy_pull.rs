//! The pull engine and the `deploy pull` action (22-08, DPLN-06).
//!
//! Engine tests build an `ObservedFiles` in memory. Action tests drive the
//! full sequence the verb uses over the local transport: a `PlanFixture` data
//! directory is the live gateway, `deploy_pull` writes a repo, and
//! `load_declared` plus `deploy_plan` prove the round trip.

mod common;

use std::collections::BTreeMap;

use ignition_core::deploy::layout::RelPath;
use ignition_core::deploy::manifest::Manifest;
use ignition_core::deploy::pull::{PULL_MAX_ROUNDS, PullInput, Skipped, generate_manifest, pull};
use ignition_core::deploy::target::Target;
use ignition_core::deploy::transport::{FileBlob, ObservedFiles};
use ignition_core::deploy::tree::{EMPTY_RENDER_SHA256, sha256_hex};
use ignition_core::error::CoreError;

const CANARY: &str = "CIPHERCANARY-7f3a9c-do-not-write";

const TARGET_YAML: &str =
    "schema_version: 1\ngateway: pulled-gw\nmode: dev\nnodes:\n  master:\n    profile: dev\n";

fn target() -> Target {
    Target::parse("targets/dev.yaml", TARGET_YAML.as_bytes()).expect("target")
}

fn manifest(types: &[&str], providers: &[&str]) -> Manifest {
    let mut yaml = String::from("schema_version: 1\ntypes:\n");
    if types.is_empty() {
        yaml = "schema_version: 1\ntypes: []\n".to_string();
    }
    for t in types {
        yaml.push_str(&format!("  - {t}\n"));
    }
    if !providers.is_empty() {
        yaml.push_str("tag_providers:\n");
        for p in providers {
            yaml.push_str(&format!("  - {p}\n"));
        }
    }
    Manifest::parse("gateways/pulled-gw/manifest.yaml", yaml.as_bytes()).expect("manifest")
}

fn blob(bytes: &[u8]) -> FileBlob {
    FileBlob {
        bytes: bytes.to_vec(),
        sha256: sha256_hex(bytes),
    }
}

fn layer(files: &[(&str, &str)]) -> BTreeMap<RelPath, FileBlob> {
    files
        .iter()
        .map(|(p, b)| (RelPath::new(p).expect("path"), blob(b.as_bytes())))
        .collect()
}

fn observed(core: &[(&str, &str)], mode: &[(&str, &str)]) -> ObservedFiles {
    ObservedFiles {
        core: layer(core),
        mode: layer(mode),
        local_units: Vec::new(),
    }
}

/// A named resource's `resource.json` with a uuid.
fn resource(uuid: &str) -> String {
    format!(
        "{{\n  \"scope\": \"A\",\n  \"version\": 1,\n  \"restricted\": false,\n  \"overridable\": true,\n  \"files\": [\n    \"config.json\"\n  ],\n  \"attributes\": {{\n    \"uuid\": \"{uuid}\",\n    \"enabled\": true\n  }}\n}}"
    )
}

/// A named resource's `resource.json` without a uuid.
fn resource_without_uuid() -> String {
    "{\n  \"scope\": \"A\",\n  \"version\": 1,\n  \"restricted\": false,\n  \"overridable\": true,\n  \"files\": [\n    \"config.json\"\n  ],\n  \"attributes\": {\n    \"enabled\": true\n  }\n}".to_string()
}

const UUID_A: &str = "4a14d749-bb78-47c7-847a-cafa17da4da8";
const UUID_B: &str = "0b9d3c52-6a7e-4f0a-9c1d-2e5f7a8b9c01";
const UUID_C: &str = "7c2e1f90-3b4a-4d5e-8f6a-1b2c3d4e5f60";

fn run(
    m: &Manifest,
    obs: &ObservedFiles,
) -> Result<ignition_core::deploy::pull::PullOutcome, CoreError> {
    let t = target();
    pull(&PullInput {
        gateway: "pulled-gw",
        env: "dev",
        target: &t,
        manifest: m,
        observed: obs,
    })
}

fn skipped_of(outcome: &ignition_core::deploy::pull::PullOutcome) -> Vec<(&str, &str)> {
    outcome
        .skipped
        .iter()
        .map(|s| (s.path.as_str(), s.reason.as_str()))
        .collect()
}

#[test]
fn repo_path_inverts_collection_path() {
    use ignition_core::deploy::layout::{collection_path, repo_path};
    for collection in [
        "ignition/database-translator/POSTGRES/config.json",
        "ignition/general-alarm-settings/config.json",
        "ignition/tag-provider/default/config.json",
        "ignition/tag-group/default/Default/resource.json",
        "ignition/tag-definition/default/A/B/tags.json",
    ] {
        let c = RelPath::new(collection).unwrap();
        let repo = repo_path(&c).expect("maps");
        assert_eq!(collection_path(&repo).unwrap(), c, "{collection}");
    }
    assert!(repo_path(&RelPath::new("ignition/tag-other/P/x.json").unwrap()).is_err());
}

#[test]
fn layering() {
    let a = resource(UUID_A);
    let b = resource(UUID_B);
    let c = resource(UUID_C);
    let obs = observed(
        &[
            // only in core
            ("ignition/database-translator/CORE/resource.json", &a),
            (
                "ignition/database-translator/CORE/config.json",
                "{\"x\": 1}",
            ),
            // in both collections
            ("ignition/database-translator/BOTH/resource.json", &b),
            (
                "ignition/database-translator/BOTH/config.json",
                "{\"x\": 2}",
            ),
        ],
        &[
            // only in the mode collection
            ("ignition/database-translator/MODE/resource.json", &c),
            (
                "ignition/database-translator/MODE/config.json",
                "{\"x\": 3}",
            ),
            // the mode copy of BOTH differs in one file
            ("ignition/database-translator/BOTH/resource.json", &b),
            (
                "ignition/database-translator/BOTH/config.json",
                "{\"x\": 22}",
            ),
        ],
    );
    let m = manifest(&["ignition/database-translator"], &[]);
    let outcome = run(&m, &obs).expect("pull");

    let paths: Vec<&str> = outcome.source.files.keys().map(RelPath::as_str).collect();
    assert_eq!(
        paths,
        [
            "modes/dev/resources/ignition/database-translator/BOTH/config.json",
            "modes/dev/resources/ignition/database-translator/BOTH/resource.json",
            "modes/dev/resources/ignition/database-translator/MODE/config.json",
            "modes/dev/resources/ignition/database-translator/MODE/resource.json",
            "resources/ignition/database-translator/BOTH/config.json",
            "resources/ignition/database-translator/BOTH/resource.json",
            "resources/ignition/database-translator/CORE/config.json",
            "resources/ignition/database-translator/CORE/resource.json",
        ]
    );
    assert!(outcome.skipped.is_empty(), "{:?}", skipped_of(&outcome));
    // bytes kept exactly, the gateway's uuid included (D-12)
    let kept = &outcome.source.files
        [&RelPath::new("resources/ignition/database-translator/CORE/resource.json").unwrap()];
    assert_eq!(kept, a.as_bytes());
    // the render is the live effective view: the mode copy of BOTH wins
    let rendered = &outcome.rendered.tree.files
        [&RelPath::new("ignition/database-translator/BOTH/config.json").unwrap()];
    assert_eq!(rendered, b"{\"x\": 22}");
    assert!(outcome.rendered.uuids_minted.is_empty());
}

#[test]
fn generated_manifest_parses() {
    let live = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(m, t)| ((*m).to_string(), (*t).to_string()))
            .collect()
    };
    let (yaml, skipped) = generate_manifest(
        &live(&[
            ("ignition", "general-alarm-settings"),
            ("ignition", "database-translator"),
            ("ignition", "database-translator"),
            ("ignition", "tag-provider"),
            ("ignition", "tag-definition"),
            ("ignition", "user-source"),
            ("ignition", "api-token"),
            ("com.example.unknown", "thing"),
        ]),
        &[
            "zeta".to_string(),
            "default".to_string(),
            "default".to_string(),
        ],
    );
    let parsed = Manifest::parse("manifest.yaml", yaml.as_bytes()).expect("parses");
    assert_eq!(
        parsed.types,
        [
            "ignition/database-translator",
            "ignition/general-alarm-settings"
        ],
        "byte-sorted, no denied, no rule-less, no tag types:\n{yaml}"
    );
    assert_eq!(parsed.tag_providers, ["default", "zeta"]);
    assert_eq!(
        skipped,
        vec![
            Skipped {
                path: "com.example.unknown/thing".into(),
                reason: "type_without_rule".into()
            },
            Skipped {
                path: "ignition/api-token".into(),
                reason: "denied_type".into()
            },
            Skipped {
                path: "ignition/user-source".into(),
                reason: "denied_type".into()
            },
        ]
    );

    // nothing live: a valid manifest with empty lists
    let (empty, none) = generate_manifest(&[], &[]);
    let parsed = Manifest::parse("manifest.yaml", empty.as_bytes()).expect("empty parses");
    assert!(parsed.types.is_empty() && parsed.tag_providers.is_empty());
    assert!(none.is_empty());

    // a provider name that is a YAML keyword or number stays a string
    let (quoted, _) = generate_manifest(
        &[],
        &["true".to_string(), "123".to_string(), "a: b".to_string()],
    );
    let parsed = Manifest::parse("manifest.yaml", quoted.as_bytes()).expect("quoted parses");
    assert_eq!(parsed.tag_providers, ["123", "a: b", "true"]);

    // an unsafe provider or type name is left out with a reason
    let (_, bad) = generate_manifest(&live(&[("ignition", ".hidden")]), &[".dot".to_string()]);
    assert_eq!(bad.len(), 2, "{bad:?}");
    assert!(bad.iter().all(|s| s.reason.starts_with("render_refused: ")));
}

#[test]
fn embedded_unit_skipped() {
    let a = resource(UUID_A);
    let b = resource(UUID_B);
    let secret = format!(
        "{{\"password\": {{\"type\": \"Embedded\", \"data\": {{\"ciphertext\": \"{CANARY}\"}}}}}}"
    );
    let obs = observed(
        &[
            ("ignition/database-translator/GOOD/resource.json", &a),
            (
                "ignition/database-translator/GOOD/config.json",
                "{\"x\": 1}",
            ),
            ("ignition/database-translator/SECRET/resource.json", &b),
            ("ignition/database-translator/SECRET/config.json", &secret),
        ],
        &[],
    );
    let m = manifest(&["ignition/database-translator"], &[]);
    let outcome = run(&m, &obs).expect("pull");

    assert_eq!(
        skipped_of(&outcome),
        [("ignition/database-translator/SECRET", "embedded_secret")]
    );
    assert!(
        outcome
            .source
            .files
            .keys()
            .all(|p| !p.as_str().contains("SECRET")),
        "the secret unit is not in the output tree"
    );
    let dump = format!("{outcome:?}");
    assert!(!dump.contains(CANARY), "ciphertext leaked into the outcome");
    assert!(
        !dump.contains("SECRET/config.json"),
        "even the file name stays out of the tree"
    );
}

#[test]
fn uuidless_named_resource_not_round_trippable() {
    let obs = observed(
        &[
            (
                "ignition/database-translator/NOUUID/resource.json",
                &resource_without_uuid(),
            ),
            (
                "ignition/database-translator/NOUUID/config.json",
                "{\"x\": 1}",
            ),
        ],
        &[],
    );
    let m = manifest(&["ignition/database-translator"], &[]);
    let outcome = run(&m, &obs).expect("pull");
    assert_eq!(
        skipped_of(&outcome),
        [("ignition/database-translator/NOUUID", "not_round_trippable")]
    );
    assert!(outcome.source.files.is_empty());
    assert!(outcome.rendered.tree.files.is_empty());
    assert!(outcome.rendered.uuids_minted.is_empty());
}

#[test]
fn fixpoint_converges() {
    let a = resource(UUID_A);
    let b = resource(UUID_B);
    let c = resource(UUID_C);
    let secret = format!(
        "{{\"password\": {{\"type\": \"Embedded\", \"data\": {{\"ciphertext\": \"{CANARY}\"}}}}}}"
    );
    let obs = observed(
        &[
            ("ignition/database-translator/GOOD/resource.json", &a),
            (
                "ignition/database-translator/GOOD/config.json",
                "{\"x\": 1}",
            ),
            ("ignition/database-translator/SECRET/resource.json", &b),
            ("ignition/database-translator/SECRET/config.json", &secret),
            // a Windows-illegal character in a live name
            ("ignition/database-translator/bad:name/resource.json", &c),
            (
                "ignition/database-translator/bad:name/config.json",
                "{\"x\": 2}",
            ),
            // a unit render would change (it mints a uuid)
            (
                "ignition/database-translator/NOUUID/resource.json",
                &resource_without_uuid(),
            ),
            (
                "ignition/database-translator/NOUUID/config.json",
                "{\"x\": 3}",
            ),
        ],
        &[],
    );
    let m = manifest(&["ignition/database-translator"], &[]);
    let outcome = run(&m, &obs).expect("pull converges");
    const { assert!(PULL_MAX_ROUNDS >= 3) };

    let skipped = skipped_of(&outcome);
    assert_eq!(skipped.len(), 3, "{skipped:?}");
    assert_eq!(
        skipped[0].0, "ignition/database-translator/NOUUID",
        "{skipped:?}"
    );
    assert_eq!(skipped[0].1, "not_round_trippable");
    assert_eq!(skipped[1].0, "ignition/database-translator/SECRET");
    assert_eq!(skipped[1].1, "embedded_secret");
    assert_eq!(skipped[2].0, "ignition/database-translator/bad:name");
    assert!(skipped[2].1.starts_with("render_refused: "), "{skipped:?}");
    assert!(!format!("{outcome:?}").contains(CANARY));

    // what is left renders clean: only GOOD
    let kept: Vec<&str> = outcome
        .rendered
        .tree
        .files
        .keys()
        .map(RelPath::as_str)
        .collect();
    assert_eq!(
        kept,
        [
            "ignition/database-translator/GOOD/config.json",
            "ignition/database-translator/GOOD/resource.json"
        ]
    );
    // skipped is byte-sorted
    let mut sorted = outcome.skipped.clone();
    sorted.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
    assert_eq!(sorted, outcome.skipped);
}

#[test]
fn core_and_mode_extra_file_not_round_trippable() {
    // core holds an extra file the mode copy lacks: render's per-file
    // overlay would keep it, the gateway's effective view (the mode copy)
    // does not have it
    let a = resource(UUID_A);
    let obs = observed(
        &[
            ("ignition/database-translator/X/resource.json", &a),
            ("ignition/database-translator/X/config.json", "{\"x\": 1}"),
            ("ignition/database-translator/X/extra.json", "{\"e\": 1}"),
        ],
        &[
            ("ignition/database-translator/X/resource.json", &a),
            ("ignition/database-translator/X/config.json", "{\"x\": 2}"),
        ],
    );
    let m = manifest(&["ignition/database-translator"], &[]);
    let outcome = run(&m, &obs).expect("pull");
    assert_eq!(
        skipped_of(&outcome),
        [("ignition/database-translator/X", "not_round_trippable")]
    );
    assert!(outcome.source.files.is_empty());
}

#[test]
fn tags_are_placed_under_tags() {
    let obs = observed(
        &[
            (
                "ignition/tag-provider/default/config.json",
                "{\"name\": \"default\"}",
            ),
            (
                "ignition/tag-provider/default/resource.json",
                "{\"scope\": \"A\", \"version\": 1, \"files\": [\"config.json\"], \"attributes\": {}}",
            ),
            (
                "ignition/tag-definition/default/Folder/tags.json",
                "{\"name\": \"F\"}",
            ),
            (
                "ignition/tag-definition/default/Folder/unary-resource.json",
                "{\"scope\": \"A\", \"version\": 1}",
            ),
        ],
        &[],
    );
    let m = manifest(&[], &["default"]);
    let outcome = run(&m, &obs).expect("pull");
    let paths: Vec<&str> = outcome.source.files.keys().map(RelPath::as_str).collect();
    assert_eq!(
        paths,
        [
            "tags/default/tag-definition/Folder/tags.json",
            "tags/default/tag-definition/Folder/unary-resource.json",
            "tags/default/tag-provider/config.json",
            "tags/default/tag-provider/resource.json",
        ],
        "{:?}",
        skipped_of(&outcome)
    );
    assert!(outcome.skipped.is_empty());
}

#[test]
fn a_finding_about_no_unit_is_returned() {
    // the manifest allowlists a type with no render rule: render names
    // manifest.yaml, which is no unit, so the error is the operator's
    let obs = observed(&[], &[]);
    let m = manifest(&["com.example.unknown/thing"], &[]);
    let err = run(&m, &obs).expect_err("manifest defect");
    assert!(matches!(err, CoreError::DeployTreeInvalid(_)), "{err:?}");
}

#[test]
fn empty_live_tree() {
    let obs = observed(&[], &[]);
    let (yaml, skipped) = generate_manifest(&[], &[]);
    assert!(skipped.is_empty());
    let m = Manifest::parse("manifest.yaml", yaml.as_bytes()).expect("manifest");
    assert!(m.types.is_empty() && m.tag_providers.is_empty());
    let outcome = run(&m, &obs).expect("pull");
    assert!(outcome.source.files.is_empty());
    assert!(outcome.skipped.is_empty());
    assert_eq!(outcome.rendered.tree.render_sha256(), EMPTY_RENDER_SHA256);
}
