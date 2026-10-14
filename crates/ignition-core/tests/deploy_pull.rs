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

// ---------------------------------------------------------------------
// The action: `deploy_pull_preview`, `deploy_pull` and the round trip.
// ---------------------------------------------------------------------

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use common::plan_fixture::PlanFixture;
use ignition_core::actions::deploy::{
    PlanOptions, deploy_plan, deploy_pull, deploy_pull_preview, load_declared, load_target,
};
use ignition_core::deploy::plan::PlanStatus;
use ignition_core::deploy::transport::{self, Scope, TreeReader};

/// Counts every call a verb makes to the reader.
struct Counting {
    inner: Box<dyn TreeReader>,
    calls: AtomicUsize,
}

#[async_trait::async_trait]
impl TreeReader for Counting {
    async fn list_types(&self, collection: &str) -> Result<Vec<(String, String)>, CoreError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.list_types(collection).await
    }
    async fn read(&self, scope: &Scope) -> Result<ObservedFiles, CoreError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.read(scope).await
    }
    async fn read_ledger(&self, gateway: &str, mode: &str) -> Result<Option<Vec<u8>>, CoreError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.read_ledger(gateway, mode).await
    }
}

fn reader_for(repo: &Path, env: &str) -> Counting {
    let (target_file, target) = load_target(repo, env).expect("target");
    Counting {
        inner: transport::for_node(repo, &target_file, "nodes.master", &target.nodes.master)
            .expect("master has host access"),
        calls: AtomicUsize::new(0),
    }
}

/// Every file (bytes) and symlink (target) below `dir`, keyed by relative path.
fn snapshot(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, rel: &str, out: &mut BTreeMap<String, Vec<u8>>) {
        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in read {
            let entry = entry.expect("entry");
            let name = entry.file_name().to_string_lossy().into_owned();
            let child = if rel.is_empty() {
                name
            } else {
                format!("{rel}/{name}")
            };
            let meta = std::fs::symlink_metadata(entry.path()).expect("meta");
            if meta.file_type().is_symlink() {
                let target = std::fs::read_link(entry.path()).expect("link");
                out.insert(child, format!("-> {}", target.display()).into_bytes());
            } else if meta.is_dir() {
                out.insert(format!("{child}/"), Vec::new());
                walk(&entry.path(), &child, out);
            } else {
                out.insert(child, std::fs::read(entry.path()).expect("read"));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, "", &mut out);
    out
}

fn put(repo: &Path, rel: &str, bytes: &str) {
    let dest = rel
        .split('/')
        .fold(repo.to_path_buf(), |acc, c| acc.join(c));
    std::fs::create_dir_all(dest.parent().expect("parent")).expect("dirs");
    std::fs::write(dest, bytes).expect("write");
}

/// The tracer repo copy with `gateways/tracer-gw` removed entirely.
fn empty_repo(fx: &PlanFixture) -> PathBuf {
    let repo = fx.repo("dev");
    std::fs::remove_dir_all(repo.join("gateways").join("tracer-gw")).expect("remove gateway dir");
    repo
}

/// A live gateway: a core-only resource, a singleton with a mode override, a
/// mode-only resource, a tag provider with a folder, an Embedded-secret unit
/// and a uuid-less named resource, plus a denied and a rule-less type.
fn populate_live(fx: &PlanFixture) {
    let cfg = |n: u32| format!("{{\n  \"x\": {n}\n}}");
    fx.core(
        "ignition/database-translator/CORE/resource.json",
        resource(UUID_A).as_bytes(),
    );
    fx.core(
        "ignition/database-translator/CORE/config.json",
        cfg(1).as_bytes(),
    );
    fx.core(
        "ignition/general-alarm-settings/resource.json",
        b"{\n  \"scope\": \"A\",\n  \"version\": 1,\n  \"files\": [\n    \"config.json\"\n  ]\n}",
    );
    fx.core(
        "ignition/general-alarm-settings/config.json",
        cfg(2).as_bytes(),
    );
    fx.mode(
        "ignition/general-alarm-settings/resource.json",
        b"{\n  \"scope\": \"A\",\n  \"version\": 1,\n  \"files\": [\n    \"config.json\"\n  ]\n}",
    );
    fx.mode(
        "ignition/general-alarm-settings/config.json",
        cfg(3).as_bytes(),
    );
    fx.mode(
        "ignition/database-translator/MODE/resource.json",
        resource(UUID_B).as_bytes(),
    );
    fx.mode(
        "ignition/database-translator/MODE/config.json",
        cfg(4).as_bytes(),
    );
    fx.core(
        "ignition/tag-provider/default/config.json",
        b"{\"name\": \"default\"}",
    );
    fx.core(
        "ignition/tag-provider/default/resource.json",
        b"{\"scope\": \"A\", \"version\": 1}",
    );
    fx.core(
        "ignition/tag-definition/default/Folder/tags.json",
        b"{\"name\": \"Folder\"}",
    );
    fx.core(
        "ignition/tag-definition/default/Folder/unary-resource.json",
        b"{\"scope\": \"A\", \"version\": 1}",
    );
    fx.core(
        "ignition/database-translator/SECRET/resource.json",
        resource(UUID_C).as_bytes(),
    );
    fx.core(
        "ignition/database-translator/SECRET/config.json",
        format!(
            "{{\"password\": {{\"type\": \"Embedded\", \"data\": {{\"ciphertext\": \"{CANARY}\"}}}}}}"
        )
        .as_bytes(),
    );
    fx.core(
        "ignition/database-translator/NOUUID/resource.json",
        resource_without_uuid().as_bytes(),
    );
    fx.core(
        "ignition/database-translator/NOUUID/config.json",
        cfg(5).as_bytes(),
    );
    // types a generated manifest leaves out
    fx.core("ignition/api-token/tok/config.json", b"{}");
    fx.core("com.example.unknown/thing/A/config.json", b"{}");
}

#[tokio::test]
async fn pull_render_plan_round_trip() {
    let fx = PlanFixture::new();
    populate_live(&fx);
    let repo = empty_repo(&fx);
    let reader = reader_for(&repo, "dev");
    let live_before = snapshot(fx.data_dir());

    let result = deploy_pull(&repo, "dev", &reader, false)
        .await
        .expect("pull into an empty repo");
    assert_eq!(
        (
            result.env.as_str(),
            result.gateway.as_str(),
            result.mode.as_str()
        ),
        ("dev", "tracer-gw", "dev")
    );
    assert_eq!(
        (result.node.as_str(), result.transport.as_str()),
        ("master", "local")
    );
    assert_eq!(result.out, "gateways/tracer-gw");
    assert_eq!(result.manifest, "generated");

    // the skip reasons seen: units and types, byte-sorted, no values
    let skipped: Vec<(&str, &str)> = result
        .skipped
        .iter()
        .map(|s| (s.path.as_str(), s.reason.as_str()))
        .collect();
    assert_eq!(
        skipped,
        [
            ("com.example.unknown/thing", "type_without_rule"),
            ("ignition/api-token", "denied_type"),
            ("ignition/database-translator/NOUUID", "not_round_trippable"),
            ("ignition/database-translator/SECRET", "embedded_secret"),
        ]
    );

    // pull never writes to the gateway host
    assert_eq!(snapshot(fx.data_dir()), live_before);

    // render then plan against the same live tree is empty
    let declared = load_declared(&repo, "dev").expect("the pulled tree renders");
    assert_eq!(declared.outcome.tree.render_sha256(), result.render_sha256);
    let live = reader_for(&repo, "dev");
    let planned = deploy_plan(&declared, &live, &PlanOptions::default())
        .await
        .expect("plan");
    assert!(!planned.summary.has_changes, "{:?}", planned.summary);
    let mut same = Vec::new();
    let mut unmanaged = Vec::new();
    for entry in &planned.entries {
        match entry.status {
            PlanStatus::Same => same.push(entry.key.as_str()),
            PlanStatus::Unmanaged => unmanaged.push(entry.key.as_str()),
            other => panic!("{} is {other:?}", entry.key),
        }
    }
    assert_eq!(
        unmanaged,
        [
            "ignition/database-translator/NOUUID",
            "ignition/database-translator/SECRET"
        ]
    );
    assert_eq!(
        same,
        [
            "ignition/database-translator/CORE",
            "ignition/database-translator/MODE",
            "ignition/general-alarm-settings",
            "ignition/tag-definition/default/Folder",
            "ignition/tag-provider/default",
        ]
    );
    assert_eq!(result.files_written, declared_source_files(&repo));

    // the layering: core-only in the base, the override in the overlay
    let written = snapshot(&repo.join("gateways").join("tracer-gw"));
    for path in [
        "manifest.yaml",
        "resources/ignition/database-translator/CORE/config.json",
        "resources/ignition/general-alarm-settings/config.json",
        "modes/dev/resources/ignition/general-alarm-settings/config.json",
        "modes/dev/resources/ignition/database-translator/MODE/config.json",
        "tags/default/tag-definition/Folder/tags.json",
        "tags/default/tag-provider/config.json",
    ] {
        assert!(written.contains_key(path), "{path} not written");
    }
    // the gateway's own bytes, uuid included
    assert_eq!(
        written["resources/ignition/database-translator/CORE/resource.json"],
        resource(UUID_A).as_bytes()
    );
    // no secret anywhere
    for (path, bytes) in &written {
        assert!(
            !String::from_utf8_lossy(bytes).contains(CANARY),
            "{path} holds the ciphertext"
        );
        assert!(!path.contains("SECRET"), "{path}");
    }
    assert!(!format!("{result:?}").contains(CANARY));
}

/// Number of regular files under `resources/`, `tags/` and `modes/` of the
/// pulled gateway directory.
fn declared_source_files(repo: &Path) -> usize {
    snapshot(&repo.join("gateways").join("tracer-gw"))
        .iter()
        .filter(|(path, _)| !path.ends_with('/') && *path != "manifest.yaml")
        .count()
}

#[tokio::test]
async fn pull_into_absent_and_empty_dirs_needs_no_yes() {
    // gateways/ absent
    let fx = PlanFixture::new();
    populate_live(&fx);
    let repo = fx.repo("dev");
    std::fs::remove_dir_all(repo.join("gateways")).expect("remove gateways");
    assert_eq!(deploy_pull_preview(&repo, "dev").unwrap(), None);
    let reader = reader_for(&repo, "dev");
    deploy_pull(&repo, "dev", &reader, false)
        .await
        .expect("absent gateways/");
    assert!(repo.join("gateways/tracer-gw/manifest.yaml").is_file());

    // an empty gateway directory
    let repo = empty_repo(&fx);
    std::fs::create_dir(repo.join("gateways").join("tracer-gw")).expect("empty dir");
    assert_eq!(deploy_pull_preview(&repo, "dev").unwrap(), None);
    let reader = reader_for(&repo, "dev");
    deploy_pull(&repo, "dev", &reader, false)
        .await
        .expect("empty gateway dir");

    // only a manifest and another environment's overlay: nothing is replaced
    let repo = empty_repo(&fx);
    put(
        &repo,
        "gateways/tracer-gw/manifest.yaml",
        "schema_version: 1\ntypes:\n  - ignition/database-translator\n",
    );
    put(
        &repo,
        "gateways/tracer-gw/modes/prod/resources/ignition/database-translator/P/config.json",
        "{}",
    );
    assert_eq!(deploy_pull_preview(&repo, "dev").unwrap(), None);
    let reader = reader_for(&repo, "dev");
    let result = deploy_pull(&repo, "dev", &reader, false)
        .await
        .expect("nothing to replace");
    assert_eq!(result.manifest, "kept");
}

#[tokio::test]
async fn pull_refuses_non_empty_without_yes() {
    let fx = PlanFixture::new();
    populate_live(&fx);
    let repo = fx.repo("dev");
    // 7 existing files across the three subtrees; the tracer repo has 4
    put(
        &repo,
        "gateways/tracer-gw/tags/default/tag-definition/F/tags.json",
        "{}",
    );
    put(
        &repo,
        "gateways/tracer-gw/modes/dev/resources/ignition/database-translator/M/config.json",
        "{}",
    );
    put(
        &repo,
        "gateways/tracer-gw/modes/prod/resources/ignition/database-translator/P/config.json",
        "{}",
    );
    let before = snapshot(&repo);

    let prose = deploy_pull_preview(&repo, "dev")
        .expect("preview")
        .expect("something would be replaced");
    assert!(prose.starts_with("deploy pull would replace gateways/tracer-gw/resources, tags and modes/dev with the live tree of nodes.master (6 existing files, e.g. "), "{prose}");
    assert!(prose.ends_with(", ...)"), "{prose}");

    let reader = reader_for(&repo, "dev");
    let err = deploy_pull(&repo, "dev", &reader, false)
        .await
        .expect_err("refused");
    assert_eq!(err.code(), "confirmation_required");
    assert_eq!(err.exit_code(), 2);
    let message = err.to_string();
    // the first five paths, in resources, tags, modes order
    for shown in [
        "resources/ignition/database-translator/POSTGRES/config.json",
        "resources/ignition/database-translator/POSTGRES/resource.json",
        "resources/ignition/general-alarm-settings/config.json",
        "resources/ignition/general-alarm-settings/resource.json",
        "tags/default/tag-definition/F/tags.json",
    ] {
        assert!(message.contains(shown), "{shown} missing from: {message}");
    }
    assert!(
        !message.contains("modes/dev/resources/ignition/database-translator/M/config.json"),
        "only five paths are named: {message}"
    );
    // refused before the gateway is read and before any write
    assert_eq!(reader.calls.load(Ordering::SeqCst), 0);
    assert_eq!(snapshot(&repo), before);
}

#[tokio::test]
async fn pull_yes_replaces_only_its_subtrees() {
    let fx = PlanFixture::new();
    populate_live(&fx);
    let repo = fx.repo("dev");
    put(
        &repo,
        "gateways/tracer-gw/resources/ignition/database-translator/STALE/config.json",
        "{}",
    );
    put(
        &repo,
        "gateways/tracer-gw/tags/stale/tag-definition/F/tags.json",
        "{}",
    );
    put(
        &repo,
        "gateways/tracer-gw/modes/dev/resources/ignition/database-translator/STALE2/config.json",
        "{}",
    );
    put(
        &repo,
        "gateways/tracer-gw/modes/prod/resources/ignition/database-translator/P/config.json",
        "{\"prod\": true}",
    );
    let manifest_before = std::fs::read(repo.join("gateways/tracer-gw/manifest.yaml")).unwrap();
    let prod_before = snapshot(&repo.join("gateways/tracer-gw/modes/prod"));
    let targets_before = snapshot(&repo.join("targets"));

    let reader = reader_for(&repo, "dev");
    let result = deploy_pull(&repo, "dev", &reader, true)
        .await
        .expect("--yes replaces");
    assert_eq!(result.manifest, "kept");

    let gw = repo.join("gateways/tracer-gw");
    let written = snapshot(&gw);
    for stale in [
        "resources/ignition/database-translator/STALE/config.json",
        "resources/ignition/database-translator/POSTGRES/config.json",
        "tags/stale/tag-definition/F/tags.json",
        "modes/dev/resources/ignition/database-translator/STALE2/config.json",
    ] {
        assert!(!written.contains_key(stale), "{stale} survived");
    }
    assert!(!written.contains_key("tags/stale/"), "{written:?}");
    assert!(
        written.contains_key("resources/ignition/database-translator/CORE/config.json"),
        "the live tree was written"
    );
    // the kept manifest and the other environment are byte-identical
    assert_eq!(
        std::fs::read(gw.join("manifest.yaml")).unwrap(),
        manifest_before
    );
    assert_eq!(snapshot(&gw.join("modes/prod")), prod_before);
    assert_eq!(snapshot(&repo.join("targets")), targets_before);

    // a second pull of an unchanged gateway needs --yes and changes nothing
    let again = snapshot(&gw);
    let reader = reader_for(&repo, "dev");
    deploy_pull(&repo, "dev", &reader, true)
        .await
        .expect("re-pull");
    assert_eq!(snapshot(&gw), again);
}

#[cfg(unix)]
#[tokio::test]
async fn pull_refuses_symlink_in_replaced_subtree() {
    let fx = PlanFixture::new();
    populate_live(&fx);
    let outside = tempfile::TempDir::new().expect("outside");
    std::fs::write(outside.path().join("keep.txt"), "keep").unwrap();

    // a symlink below resources/
    let repo = fx.repo("dev");
    std::os::unix::fs::symlink(
        outside.path(),
        repo.join("gateways/tracer-gw/resources/ignition/link"),
    )
    .expect("symlink");
    let before = snapshot(&repo);
    let outside_before = snapshot(outside.path());
    let reader = reader_for(&repo, "dev");
    for yes in [false, true] {
        let err = deploy_pull(&repo, "dev", &reader, yes)
            .await
            .expect_err("symlink refused");
        assert_eq!(err.code(), "deploy_tree_invalid", "yes={yes}: {err}");
    }
    assert!(deploy_pull_preview(&repo, "dev").is_err());
    assert_eq!(snapshot(&repo), before, "nothing removed");
    assert_eq!(snapshot(outside.path()), outside_before);
    assert_eq!(reader.calls.load(Ordering::SeqCst), 0);

    // a symlinked subtree root
    let repo = fx.repo("dev");
    let gw = repo.join("gateways/tracer-gw");
    std::fs::remove_dir_all(gw.join("resources")).unwrap();
    std::os::unix::fs::symlink(outside.path(), gw.join("resources")).expect("symlink");
    let reader = reader_for(&repo, "dev");
    let err = deploy_pull(&repo, "dev", &reader, true)
        .await
        .expect_err("symlinked resources/ refused");
    assert_eq!(err.code(), "deploy_tree_invalid", "{err}");
    assert_eq!(snapshot(outside.path()), outside_before);

    // a symlinked gateway directory
    let repo = fx.repo("dev");
    std::fs::remove_dir_all(repo.join("gateways/tracer-gw")).unwrap();
    std::os::unix::fs::symlink(outside.path(), repo.join("gateways/tracer-gw")).expect("symlink");
    let reader = reader_for(&repo, "dev");
    let err = deploy_pull(&repo, "dev", &reader, true)
        .await
        .expect_err("symlinked gateway dir refused");
    assert_eq!(err.code(), "deploy_tree_invalid", "{err}");
    assert_eq!(snapshot(outside.path()), outside_before);
}

#[tokio::test]
async fn pull_empty_live_tree() {
    let fx = PlanFixture::new();
    let repo = empty_repo(&fx);
    let reader = reader_for(&repo, "dev");
    let result = deploy_pull(&repo, "dev", &reader, false)
        .await
        .expect("pull");
    assert_eq!(result.files_written, 0);
    assert_eq!(result.manifest, "generated");
    assert!(result.skipped.is_empty());
    assert_eq!(result.render_sha256, EMPTY_RENDER_SHA256);

    let gw = repo.join("gateways/tracer-gw");
    assert!(gw.join("manifest.yaml").is_file());
    // no resource files and no modes/<env>/ for an absent mode collection
    assert_eq!(snapshot(&gw).keys().collect::<Vec<_>>(), ["manifest.yaml"]);

    let declared = load_declared(&repo, "dev").expect("renders");
    assert_eq!(declared.outcome.tree.render_sha256(), EMPTY_RENDER_SHA256);
    let live = reader_for(&repo, "dev");
    let planned = deploy_plan(&declared, &live, &PlanOptions::default())
        .await
        .expect("plan");
    assert!(planned.entries.is_empty());
    assert!(!planned.summary.has_changes);
}

#[tokio::test]
async fn pull_is_read_only_on_a_failed_read() {
    // an unreadable host fails the pull before anything is written
    let fx = PlanFixture::new();
    let repo = empty_repo(&fx);
    std::fs::remove_dir_all(fx.data_dir().join("config")).unwrap();
    let reader = reader_for(&repo, "dev");
    let err = deploy_pull(&repo, "dev", &reader, true)
        .await
        .expect_err("transport failure");
    assert_eq!(err.code(), "transport_failed");
    assert!(
        !repo.join("gateways/tracer-gw").exists(),
        "a failed read writes nothing"
    );
}
