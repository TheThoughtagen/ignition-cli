//! `deploy_plan` over the local-filesystem transport (22-01).
//!
//! Every behaviour test drives the full sequence the `deploy plan` verb will
//! use: `load_declared`, `transport::for_node` for `nodes.master`, then
//! `deploy_plan`. No gateway session exists in this file: the plan reads the
//! host's data directory and nothing else.

mod common;

use std::collections::BTreeMap;

use common::plan_fixture::{GATEWAY, MODE, MarkUnreadable, PlanFixture};
use ignition_core::actions::deploy::{
    DeployPlanResult, PlanOptions, deploy_plan, deploy_render, load_declared,
};
use ignition_core::deploy::layout::RelPath;
use ignition_core::deploy::ledger::Ledger;
use ignition_core::deploy::plan::{
    CoreRelation, DIFF_MAX_BYTES, DIFF_WITHHELD, Plan, PlanStatus, UnreadableUnit,
    plan as plan_with_ledger,
};
use ignition_core::deploy::transport::{
    self, FileBlob, ObservedFiles, READ_MAX_FILE_BYTES, Scope, Unreadable,
};
use ignition_core::deploy::tree::{EMPTY_RENDER_SHA256, RenderedTree, sha256_hex};
use ignition_core::error::CoreError;
use sha2::{Digest, Sha256};

const DB_CONFIG: &str = "ignition/database-translator/POSTGRES/config.json";

async fn plan(fx: &PlanFixture) -> DeployPlanResult {
    let (declared, reader) = fx.load("dev");
    deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect("plan succeeds")
}

fn statuses(result: &DeployPlanResult) -> Vec<(&str, PlanStatus)> {
    result
        .entries
        .iter()
        .map(|e| (e.key.as_str(), e.status))
        .collect()
}

#[tokio::test]
async fn plan_local_live_equal_to_render_is_empty() {
    let fx = PlanFixture::new();
    let (declared, reader) = fx.load("dev");
    fx.live_render("dev", &declared);
    let result = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect("plan");

    assert_eq!(
        (result.node.as_str(), result.transport.as_str()),
        ("master", "local")
    );
    assert_eq!(result.env, "dev");
    assert!(!result.summary.has_changes, "{:?}", result.summary);
    assert_eq!(result.entries.len(), 2, "{:?}", statuses(&result));
    for entry in &result.entries {
        assert_eq!(entry.status, PlanStatus::Same, "{}", entry.key);
        assert_eq!(entry.live_in, ["mode"], "{}", entry.key);
    }
    assert_eq!(result.render_sha256, declared.outcome.tree.render_sha256());
}

#[tokio::test]
async fn plan_local_byte_change_is_changed() {
    let fx = PlanFixture::new();
    let (declared, reader) = fx.load("dev");
    fx.live_render("dev", &declared);
    let mut bytes = fx.read("dev", DB_CONFIG);
    bytes.push(b' ');
    fx.mode(DB_CONFIG, &bytes);

    let result = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect("changed is not an error");
    assert!(result.summary.has_changes);
    assert_eq!(result.summary.changed, 1);
    let changed = result
        .entries
        .iter()
        .find(|e| e.status == PlanStatus::Changed)
        .expect("one changed entry");
    assert_eq!(changed.key, "ignition/database-translator/POSTGRES");
}

#[tokio::test]
async fn plan_local_core_only_is_same_with_live_in_core() {
    let fx = PlanFixture::new();
    let (declared, reader) = fx.load("dev");
    fx.live_render("core", &declared);
    let result = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect("plan");

    assert!(!result.summary.has_changes);
    assert_eq!(result.entries.len(), 2);
    for entry in &result.entries {
        assert_eq!(entry.status, PlanStatus::Same, "{}", entry.key);
        assert_eq!(entry.live_in, ["core"], "{}", entry.key);
    }
}

#[tokio::test]
async fn plan_local_missing_unit_is_added() {
    let fx = PlanFixture::new();
    let (declared, reader) = fx.load("dev");
    fx.live_render("dev", &declared);
    fx.remove("dev", "ignition/database-translator/POSTGRES");
    let result = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect("plan");

    assert!(result.summary.has_changes);
    assert_eq!(result.summary.added, 1);
    let added = result
        .entries
        .iter()
        .find(|e| e.status == PlanStatus::Added)
        .expect("one added entry");
    assert_eq!(added.key, "ignition/database-translator/POSTGRES");
    assert!(added.live_in.is_empty());
}

#[tokio::test]
async fn plan_local_extra_live_unit_is_unmanaged() {
    let fx = PlanFixture::new();
    let (declared, reader) = fx.load("dev");
    fx.live_render("dev", &declared);
    fx.mode("ignition/database-translator/OTHER/config.json", b"{}");
    fx.mode("ignition/database-translator/OTHER/resource.json", b"{}");
    let result = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect("plan");

    assert!(!result.summary.has_changes, "unmanaged never counts");
    assert_eq!(result.summary.unmanaged, 1);
    let other = result
        .entries
        .iter()
        .find(|e| e.key == "ignition/database-translator/OTHER")
        .expect("unmanaged entry listed");
    assert_eq!(other.status, PlanStatus::Unmanaged);
    assert_eq!(other.live_in, ["mode"]);
}

#[tokio::test]
async fn plan_master_without_host_access_is_refused() {
    let fx = PlanFixture::new();

    // neither block: the declared tree loads, but for_node refuses
    let repo = fx.repo_with_master("dev", "");
    let declared = load_declared(&repo, "dev").expect("a node without host access still loads");
    let err = transport::for_node(
        &repo,
        &declared.target_file,
        "nodes.master",
        &declared.target.nodes.master,
    )
    .err()
    .expect("refused");
    assert!(matches!(err, CoreError::DeployTreeInvalid(..)), "{err}");
    assert_eq!((err.code(), err.exit_code()), ("deploy_tree_invalid", 2));
    assert!(err.to_string().contains("nodes.master"), "{err}");

    // both blocks: load_declared refuses, naming the node
    let both = format!(
        "    local:\n      data_dir: '{}'\n    ssh:\n      host: h\n      user: u\n      os: linux\n      data_dir: /d\n      known_hosts: kh\n",
        fx.data_dir().display()
    );
    let repo = fx.repo_with_master("dev", &both);
    let err = load_declared(&repo, "dev").expect_err("both refused");
    assert_eq!(err.code(), "deploy_tree_invalid");
    assert!(
        err.to_string()
            .contains("nodes.master: set ssh or local, not both"),
        "{err}"
    );
}

#[tokio::test]
async fn plan_unreadable_data_dir_is_transport_failed() {
    let fx = PlanFixture::new();
    let missing = fx.data_dir().join("no-such-data-dir");
    let repo = fx.repo_with_master(
        "dev",
        &format!("    local:\n      data_dir: '{}'\n", missing.display()),
    );
    let declared = load_declared(&repo, "dev").expect("declared");
    let reader = transport::for_node(
        &repo,
        &declared.target_file,
        "nodes.master",
        &declared.target.nodes.master,
    )
    .expect("reader");
    let err = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect_err("an unreadable host is an error, not an empty plan");
    assert_eq!((err.code(), err.exit_code()), ("transport_failed", 4));
    assert!(err.to_string().contains("nodes.master"), "{err}");
    assert!(matches!(err, CoreError::TransportFailed { .. }));
}

#[tokio::test]
async fn plan_missing_config_resources_is_transport_failed() {
    let fx = PlanFixture::new();
    std::fs::remove_dir_all(fx.data_dir().join("config")).expect("remove config");
    let (declared, reader) = fx.load("dev");
    let err = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect_err("error");
    assert_eq!(err.code(), "transport_failed");
}

#[tokio::test]
async fn plan_empty_data_dir_collections_are_empty_not_errors() {
    // collections absent: every rendered unit is `added`
    let fx = PlanFixture::new();
    let result = plan(&fx).await;
    assert_eq!(result.summary.added, 2, "{:?}", statuses(&result));
    assert!(result.summary.has_changes);
}

// ---------------------------------------------------------------------
// Task 2: the engine's semantics, pinned
// ---------------------------------------------------------------------

fn as_str(v: &[(String, String)]) -> Vec<(&str, &str)> {
    v.iter().map(|(p, b)| (p.as_str(), b.as_str())).collect()
}

fn rp(s: &str) -> RelPath {
    RelPath::new(s).expect("valid path")
}

fn rendered(files: &[(&str, &str)]) -> RenderedTree {
    RenderedTree {
        files: files
            .iter()
            .map(|(p, b)| (rp(p), b.as_bytes().to_vec()))
            .collect(),
    }
}

fn layer(files: &[(&str, &str)]) -> BTreeMap<RelPath, FileBlob> {
    files
        .iter()
        .map(|(p, b)| {
            (
                rp(p),
                FileBlob {
                    bytes: b.as_bytes().to_vec(),
                    sha256: sha256_hex(b.as_bytes()),
                },
            )
        })
        .collect()
}

/// The scope the pure-engine tests plan under: every type they use, and
/// tag provider `P`.
fn scope() -> Scope {
    Scope {
        mode: MODE.to_string(),
        types: [
            "database-translator",
            "database-connection",
            "opc-connection",
            "identity-provider",
            "security-zone",
            "store-and-forward-engine",
            "alarm-journal",
        ]
        .iter()
        .map(|t| ("ignition".to_string(), (*t).to_string()))
        .collect(),
        tag_providers: vec!["P".to_string()],
    }
}

/// The exclusion set every real plan carries: the built-in `System` default.
fn default_excluded() -> Vec<String> {
    vec!["System".to_string()]
}

/// The 22-01 plan call: no ledger.
fn plan_units(tree: &RenderedTree, live: &ObservedFiles) -> Plan {
    plan_with_ledger(tree, live, None, &scope(), &default_excluded())
}

fn ledger_of(files: &[&str]) -> Ledger {
    let mut files: Vec<String> = files.iter().map(|f| (*f).to_string()).collect();
    files.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    Ledger {
        schema_version: 1,
        gateway: GATEWAY.to_string(),
        mode: MODE.to_string(),
        render_sha256: "0".repeat(64),
        run_id: "test".to_string(),
        files,
    }
}

fn plan_ledger(tree: &RenderedTree, live: &ObservedFiles, files: &[&str]) -> Plan {
    plan_with_ledger(
        tree,
        live,
        Some(&ledger_of(files)),
        &scope(),
        &default_excluded(),
    )
}

fn deleted(p: &Plan) -> Vec<&str> {
    p.delete_set.files.iter().map(|f| f.as_str()).collect()
}

fn entry<'a>(p: &'a Plan, key: &str) -> &'a ignition_core::deploy::plan::PlanEntry {
    p.entries
        .iter()
        .find(|e| e.key == key)
        .unwrap_or_else(|| panic!("no entry {key}"))
}

fn observed(core: &[(&str, &str)], mode: &[(&str, &str)]) -> ObservedFiles {
    ObservedFiles {
        core: layer(core),
        mode: layer(mode),
        local_units: Vec::new(),
        unreadable: Vec::new(),
    }
}

#[test]
fn tag_folders_are_units() {
    let tree = rendered(&[
        ("ignition/tag-definition/P/unary-resource.json", "{}"),
        (
            "ignition/tag-definition/P/FolderA/unary-resource.json",
            "{}",
        ),
        ("ignition/tag-definition/P/FolderA/tags.json", "[]"),
        (
            "ignition/tag-definition/P/FolderB/unary-resource.json",
            "{}",
        ),
        ("ignition/tag-definition/P/FolderB/tags.json", "[]"),
    ]);
    let p = plan_units(&tree, &ObservedFiles::default());
    let keys: Vec<&str> = p.entries.iter().map(|e| e.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "ignition/tag-definition/P",
            "ignition/tag-definition/P/FolderA",
            "ignition/tag-definition/P/FolderB",
        ]
    );
    let a = &p.entries[1];
    assert_eq!(a.files.len(), 2, "a folder holds its own files only");
    assert_eq!(
        (a.module.as_str(), a.ty.as_str(), a.name.as_deref()),
        ("ignition", "tag-definition", Some("P/FolderA"))
    );
}

#[test]
fn hidden_tag_folder() {
    let both = [
        ("ignition/tag-definition/P/FolderA/tags.json", "[]"),
        (
            "ignition/tag-definition/P/FolderA/unary-resource.json",
            "{}",
        ),
    ];
    let mut core: Vec<(&str, &str)> = both.to_vec();
    core.push(("ignition/tag-definition/P/FolderA/udts.json", "[]"));
    let p = plan_units(&rendered(&both), &observed(&core, &both));
    let entry = &p.entries[0];
    assert_eq!(entry.status, PlanStatus::Same);
    assert_eq!(entry.live_in, ["core", "mode"]);
    assert_eq!(entry.core, CoreRelation::Hidden);

    // the same shape under a non-tag type is just `differs`
    let rj = ("ignition/database-translator/X/resource.json", "{}");
    let plain_core = [rj, ("ignition/database-translator/X/extra.json", "{}")];
    let plain = [rj];
    let p = plan_units(&rendered(&plain), &observed(&plain_core, &plain));
    assert_eq!(p.entries[0].core, CoreRelation::Differs);
}

#[test]
fn core_relations() {
    let unit = |name: &str, body: &str| {
        [
            (
                format!("ignition/database-translator/{name}/resource.json"),
                "{}".to_string(),
            ),
            (
                format!("ignition/database-translator/{name}/config.json"),
                body.to_string(),
            ),
        ]
    };
    let mut want: Vec<(String, String)> = Vec::new();
    let mut core: Vec<(String, String)> = Vec::new();
    for (name, want_body, core_body) in [
        ("IDENT", Some("a"), Some("a")),
        ("DIFFERS", Some("a"), Some("b")),
        ("NOCORE", Some("a"), None),
        ("UNMANAGED_NOCORE", None, None),
        ("UNMANAGED_CORE", None, Some("c")),
    ] {
        if let Some(body) = want_body {
            want.extend(unit(name, body));
        }
        if let Some(body) = core_body {
            core.extend(unit(name, body));
        }
    }
    // every unit is live in the mode collection; the render's own bodies
    let mode: Vec<(String, String)> = [
        "IDENT",
        "DIFFERS",
        "NOCORE",
        "UNMANAGED_NOCORE",
        "UNMANAGED_CORE",
    ]
    .iter()
    .flat_map(|n| unit(n, "a"))
    .collect();
    let p = plan_units(
        &rendered(&as_str(&want)),
        &observed(&as_str(&core), &as_str(&mode)),
    );
    let by_name: BTreeMap<&str, (PlanStatus, CoreRelation)> = p
        .entries
        .iter()
        .map(|e| (e.name.as_deref().unwrap_or(""), (e.status, e.core)))
        .collect();
    assert_eq!(by_name["IDENT"].1, CoreRelation::Identical);
    assert_eq!(by_name["DIFFERS"].1, CoreRelation::Differs);
    assert_eq!(by_name["NOCORE"].1, CoreRelation::None);
    assert_eq!(
        by_name["UNMANAGED_NOCORE"],
        (PlanStatus::Unmanaged, CoreRelation::None)
    );
    assert_eq!(by_name["UNMANAGED_CORE"].0, PlanStatus::Unmanaged);
    assert_eq!(by_name["UNMANAGED_CORE"].1, CoreRelation::Differs);
    let unmanaged_nocore = p
        .entries
        .iter()
        .find(|e| e.name.as_deref() == Some("UNMANAGED_NOCORE"))
        .expect("entry");
    assert_eq!(
        unmanaged_nocore.live_in,
        ["mode"],
        "live_in still tells where it lives"
    );
}

#[test]
fn whitespace_only_change_is_changed() {
    let r = [
        ("ignition/database-translator/X/resource.json", "{}"),
        (
            "ignition/database-translator/X/config.json",
            "{\"a\":1,\"b\":2}",
        ),
    ];
    let live_ws = [
        ("ignition/database-translator/X/resource.json", "{}"),
        (
            "ignition/database-translator/X/config.json",
            "{\"a\": 1,\"b\":2}",
        ),
    ];
    let live_order = [
        ("ignition/database-translator/X/resource.json", "{}"),
        (
            "ignition/database-translator/X/config.json",
            "{\"b\":2,\"a\":1}",
        ),
    ];
    for live in [&live_ws, &live_order] {
        let p = plan_units(&rendered(&r), &observed(&[], live));
        assert_eq!(
            p.entries[0].status,
            PlanStatus::Changed,
            "byte-exact, not semantic"
        );
    }
    let p = plan_units(&rendered(&r), &observed(&[], &r));
    assert_eq!(p.entries[0].status, PlanStatus::Same);
}

#[test]
fn extra_file_is_changed() {
    let r = [
        ("ignition/database-translator/X/resource.json", "{}"),
        ("ignition/database-translator/X/config.json", "{}"),
    ];
    let mut extra = r.to_vec();
    extra.push(("ignition/database-translator/X/extra.json", "{}"));
    let p = plan_units(&rendered(&r), &observed(&[], &extra));
    assert_eq!(
        p.entries[0].status,
        PlanStatus::Changed,
        "one extra live file"
    );
    let extra_file = p.entries[0]
        .files
        .iter()
        .find(|f| f.path.ends_with("extra.json"))
        .expect("extra file listed");
    assert_eq!(extra_file.desired_sha256, None);
    assert!(extra_file.live_sha256.is_some());

    let p = plan_units(&rendered(&extra), &observed(&[], &r));
    assert_eq!(
        p.entries[0].status,
        PlanStatus::Changed,
        "one missing live file"
    );
}

#[test]
fn empty_render_empty_live() {
    let p = plan_units(&RenderedTree::default(), &ObservedFiles::default());
    assert!(p.entries.is_empty());
    assert!(!p.summary.has_changes);
    let expected = Sha256::digest(
        format!("ign-deploy:plan:v1\nrender_sha256\0{EMPTY_RENDER_SHA256}\n").as_bytes(),
    );
    assert_eq!(p.plan_sha256, format!("{expected:x}"));
}

#[tokio::test]
async fn deterministic() {
    let fx = PlanFixture::new();
    let (declared, reader) = fx.load("dev");
    fx.live_render("dev", &declared);
    fx.mode("ignition/database-translator/ZED/config.json", b"{}");
    fx.mode("ignition/database-translator/ALPHA/config.json", b"{}");
    let first = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect("plan");
    let second = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect("plan");
    assert_eq!(
        serde_json::to_string(&first).expect("json"),
        serde_json::to_string(&second).expect("json")
    );
    assert_eq!(first.plan_sha256, second.plan_sha256);
    let keys: Vec<&str> = first.entries.iter().map(|e| e.key.as_str()).collect();
    let mut sorted = keys.clone();
    sorted.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    assert_eq!(keys, sorted, "entries are in unit-key byte order");
    for entry in &first.entries {
        let paths: Vec<&str> = entry.files.iter().map(|f| f.path.as_str()).collect();
        let mut sorted = paths.clone();
        sorted.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        assert_eq!(paths, sorted, "files are byte-sorted in {}", entry.key);
    }
}

/// Pinned `plan_sha256` of the golden scenario below. Changing the
/// canonical text in `deploy/plan.rs` means re-goldening this value (and
/// Phase 23's recomputation).
const PLAN_GOLDEN: &str = "a89de5043275c1b057289e89cdd78e05bc3d32680ce169e1d885e1a22708c473";

/// `plan_sha256`, recomputed here from the documented line format using
/// only the serialized result (never `plan.rs` code).
fn recompute_plan_sha256(result: &DeployPlanResult) -> String {
    let mut text = String::from("ign-deploy:plan:v1\n");
    text.push_str(&format!("render_sha256\0{}\n", result.render_sha256));
    let mut entries: Vec<_> = result
        .entries
        .iter()
        .filter(|e| e.status != PlanStatus::Unmanaged)
        .collect();
    entries.sort_by(|a, b| a.key.as_bytes().cmp(b.key.as_bytes()));
    for e in entries {
        let status = serde_json::to_value(e.status).expect("status");
        let core = serde_json::to_value(e.core).expect("core");
        text.push_str(&format!(
            "E\0{}\0{}\0{}\0{}\n",
            e.key,
            status.as_str().expect("word"),
            core.as_str().expect("word"),
            e.live_in.join(",")
        ));
        let mut files: Vec<_> = e.files.iter().collect();
        files.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
        for f in files {
            text.push_str(&format!(
                "F\0{}\0{}\0{}\n",
                f.path,
                f.live_sha256.as_deref().unwrap_or("-"),
                f.desired_sha256.as_deref().unwrap_or("-")
            ));
        }
    }
    let mut deletes: Vec<&str> = result.delete_set.files.iter().map(|f| f.as_str()).collect();
    deletes.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    for path in deletes {
        text.push_str(&format!("D\0{path}\n"));
    }
    let mut unreadable: Vec<_> = result.unreadable.iter().collect();
    unreadable.sort_by(|a, b| a.key.as_bytes().cmp(b.key.as_bytes()));
    for u in unreadable {
        text.push_str(&format!(
            "U\0{}\0{}\0{}\0{}\n",
            u.key,
            u.live_in.join(","),
            u8::from(u.declared),
            u8::from(u.in_ledger)
        ));
    }
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

#[tokio::test]
async fn plan_sha256_golden() {
    // one `changed` (POSTGRES bytes differ) and one `added` (the singleton
    // is not live)
    let fx = PlanFixture::new();
    let (declared, reader) = fx.load("dev");
    fx.live_render("dev", &declared);
    let mut bytes = fx.read("dev", DB_CONFIG);
    bytes.push(b' ');
    fx.mode(DB_CONFIG, &bytes);
    fx.remove("dev", "ignition/general-alarm-settings");
    let result = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect("plan");
    assert_eq!((result.summary.changed, result.summary.added), (1, 1));
    assert_eq!(
        result.plan_sha256,
        recompute_plan_sha256(&result),
        "documented text"
    );
    assert_eq!(result.plan_sha256.len(), 64);
    assert_eq!(result.plan_sha256, PLAN_GOLDEN, "re-golden on purpose only");

    // one live byte of a non-unmanaged unit changes the hash
    let mut again = fx.read("dev", DB_CONFIG);
    again.push(b'!');
    fx.mode(DB_CONFIG, &again);
    let moved = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect("plan");
    assert_ne!(moved.plan_sha256, result.plan_sha256);

    // an unmanaged unit does not
    fx.mode("ignition/database-translator/EXTRA/config.json", b"{}");
    fx.mode("ignition/database-translator/EXTRA/resource.json", b"{}");
    let with_extra = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect("plan");
    assert_eq!(with_extra.summary.unmanaged, 1);
    assert_eq!(with_extra.plan_sha256, moved.plan_sha256);
    fx.mode(
        "ignition/database-translator/EXTRA/config.json",
        b"{\"x\":1}",
    );
    let edited = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect("plan");
    assert_eq!(edited.plan_sha256, moved.plan_sha256);
}

#[tokio::test]
async fn local_overrides_warn() {
    let fx = PlanFixture::new();
    let (declared, reader) = fx.load("dev");
    fx.live_render("dev", &declared);
    fx.local("ignition/database-translator/POSTGRES/config.json", b"{}");
    fx.local("ignition/database-translator/POSTGRES/resource.json", b"{}");
    let result = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect("plan");
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.contains("ignition/database-translator/POSTGRES") && w.contains("local")),
        "{:?}",
        result.warnings
    );
    assert!(
        !result.summary.has_changes,
        "a local override is a warning, not a change"
    );
}

#[tokio::test]
async fn caps_and_hostile_tree() {
    // a file one byte over the per-file cap (sparse, so cheap to make)
    let fx = PlanFixture::new();
    let (declared, reader) = fx.load("dev");
    fx.live_render("dev", &declared);
    let big = fx
        .collection_dir("dev")
        .join("ignition/database-translator/BIG");
    std::fs::create_dir_all(&big).expect("dir");
    let file = std::fs::File::create(big.join("config.json")).expect("create");
    file.set_len(READ_MAX_FILE_BYTES + 1).expect("sparse");
    drop(file);
    let err = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect_err("an oversized file is refused, not skipped");
    assert_eq!((err.code(), err.exit_code()), ("transport_failed", 4));
    let shown = err.to_string();
    assert!(shown.contains("nodes.master"), "{shown}");
    assert!(
        shown.contains("ignition/database-translator/BIG/config.json"),
        "{shown}"
    );
    std::fs::remove_dir_all(&big).expect("cleanup");

    // a file exactly at the cap would be read: the boundary is `over`
    // (not exercised: a 64 MiB read is not worth the time)

    #[cfg(unix)]
    {
        let link_dir = fx
            .collection_dir("dev")
            .join("ignition/database-translator/LINK");
        std::fs::create_dir_all(&link_dir).expect("dir");
        std::os::unix::fs::symlink("/etc/hosts", link_dir.join("config.json")).expect("symlink");
        let err = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
            .await
            .expect_err("a symlink in the live tree is refused");
        assert_eq!(err.code(), "transport_failed");
        assert!(err.to_string().contains("symlink"), "{err}");
    }
}

// ---------------------------------------------------------------------
// 22-03: the declared-names ledger, the delete set, removed, flags
// ---------------------------------------------------------------------

const A_CFG: &str = "ignition/database-translator/A/config.json";
const A_RES: &str = "ignition/database-translator/A/resource.json";
const B_CFG: &str = "ignition/database-translator/B/config.json";
const B_RES: &str = "ignition/database-translator/B/resource.json";

#[test]
fn delete_set_is_ledger_minus_rendered() {
    let a = [(A_CFG, "{}"), (A_RES, "{}")];
    let both = [(A_CFG, "{}"), (A_RES, "{}"), (B_CFG, "{}"), (B_RES, "{}")];
    let p = plan_ledger(&rendered(&a), &observed(&[], &both), &both.map(|(p, _)| p));

    // adjacency: A's paths are in the ledger and rendered, never deleted
    assert_eq!(deleted(&p), [B_CFG, B_RES]);
    let b = entry(&p, "ignition/database-translator/B");
    assert_eq!(b.status, PlanStatus::Removed);
    assert!(b.destructive);
    assert_eq!(b.destructive_reason.as_deref(), Some("resource_removed"));
    assert_eq!(b.core, CoreRelation::None);
    assert_eq!(b.live_in, ["mode"]);
    let a = entry(&p, "ignition/database-translator/A");
    assert_eq!(a.status, PlanStatus::Same);
    assert!(!a.destructive && a.destructive_reason.is_none() && !a.lockout_risk);
    assert_eq!(p.summary.removed, 1);
    assert!(p.summary.has_changes);

    let mut sorted = deleted(&p);
    sorted.sort_by(|x, y| x.as_bytes().cmp(y.as_bytes()));
    assert_eq!(deleted(&p), sorted, "byte-sorted");
}

#[test]
fn only_ledger_files_are_ever_deleted() {
    // B is live and not rendered, but the ledger does not list it
    let a = [(A_CFG, "{}"), (A_RES, "{}")];
    let both = [(A_CFG, "{}"), (A_RES, "{}"), (B_CFG, "{}"), (B_RES, "{}")];
    let p = plan_ledger(&rendered(&a), &observed(&[], &both), &[A_CFG, A_RES]);
    assert!(p.delete_set.files.is_empty());
    assert_eq!(
        entry(&p, "ignition/database-translator/B").status,
        PlanStatus::Unmanaged
    );
    assert!(!p.summary.has_changes);
}

#[test]
fn partial_unit_delete_is_changed() {
    let extra = "ignition/database-translator/A/extra.bin";
    let want = [(A_CFG, "{}"), (A_RES, "{}")];
    let live = [(A_CFG, "{}"), (A_RES, "{}"), (extra, "x")];
    let p = plan_ledger(
        &rendered(&want),
        &observed(&[], &live),
        &[A_CFG, A_RES, extra],
    );

    assert_eq!(deleted(&p), [extra]);
    let a = entry(&p, "ignition/database-translator/A");
    assert_eq!(a.status, PlanStatus::Changed, "not removed");
    assert!(a.destructive);
    assert_eq!(a.destructive_reason.as_deref(), Some("files_removed"));
    assert_eq!(p.summary.removed, 0);
    assert_eq!(p.summary.changed, 1);
}

#[test]
fn shadows_removal() {
    let core_only = "ignition/database-translator/B/core-only.json";
    let core = [(B_CFG, "{}"), (core_only, "{}")];
    let mode = [(B_CFG, "{}"), (B_RES, "{}")];
    let p = plan_ledger(
        &RenderedTree::default(),
        &observed(&core, &mode),
        &[B_CFG, B_RES, core_only],
    );
    // the core-only file is in the ledger but not in the mode collection
    assert_eq!(deleted(&p), [B_CFG, B_RES]);
    let b = entry(&p, "ignition/database-translator/B");
    assert_eq!(b.status, PlanStatus::Removed);
    assert_eq!(b.core, CoreRelation::ShadowsRemoval);
    assert_eq!(b.live_in, ["core", "mode"]);
    assert!(!deleted(&p).contains(&core_only));
}

#[test]
fn no_longer_allowlisted_is_warned_not_deleted() {
    let opc = "ignition/opc-connection/X/config.json";
    let mut narrow = scope();
    narrow.types.retain(|(_, t)| t != "opc-connection");
    let live = [(opc, "{}"), (A_CFG, "{}")];
    let ledger = ledger_of(&[A_CFG, opc]);
    let p = plan_with_ledger(
        &RenderedTree::default(),
        &observed(&[], &live),
        Some(&ledger),
        &narrow,
        &default_excluded(),
    );
    assert_eq!(deleted(&p), [A_CFG], "only the still-allowed file");
    assert!(
        p.warnings
            .iter()
            .any(|w| w.contains(opc) && w.contains("no longer allows")),
        "{:?}",
        p.warnings
    );
}

#[test]
fn undeclared_provider_is_warned_not_deleted() {
    let tags = "ignition/tag-definition/Gone/tags.json";
    let live = [(tags, "[]")];
    let p = plan_ledger(&RenderedTree::default(), &observed(&[], &live), &[tags]);
    assert!(p.delete_set.files.is_empty());
    assert!(
        p.warnings.iter().any(|w| w.contains(tags)),
        "{:?}",
        p.warnings
    );
}

#[test]
fn case_variant_is_excluded_with_warning() {
    let rendered_cfg = "ignition/database-translator/x/config.json";
    let rendered_res = "ignition/database-translator/x/resource.json";
    let variant = "ignition/database-translator/X/config.json";
    let want = [(rendered_cfg, "{}"), (rendered_res, "{}")];
    let live = [(rendered_cfg, "{}"), (rendered_res, "{}"), (variant, "{}")];
    let p = plan_ledger(&rendered(&want), &observed(&[], &live), &[variant]);
    assert!(p.delete_set.files.is_empty());
    assert!(
        p.warnings
            .iter()
            .any(|w| w.contains(variant) && w.contains("case")),
        "{:?}",
        p.warnings
    );

    // exact UTF-8 bytes: only ASCII case folds, so a non-ASCII variant is
    // a different path and is deleted
    let upper = "ignition/database-translator/\u{c9}/config.json";
    let lower = "ignition/database-translator/\u{e9}/config.json";
    let live = [(upper, "{}"), (lower, "{}")];
    let p = plan_ledger(&rendered(&[(lower, "{}")]), &observed(&[], &live), &[upper]);
    assert_eq!(deleted(&p), [upper]);
}

#[test]
fn gateway_owned_and_dot_named_ledger_paths_are_never_deleted() {
    // a hand-built ledger (not through Ledger::parse) cannot widen the set
    let cfg = "config-mode.json";
    let dot = "ignition/database-translator/.hidden/config.json";
    let live = [(cfg, "{}"), (dot, "{}")];
    let p = plan_ledger(&RenderedTree::default(), &observed(&[], &live), &[cfg, dot]);
    assert!(p.delete_set.files.is_empty());
}

#[test]
fn destructive_reasons() {
    let tag_unit = [
        ("ignition/tag-definition/P/F/tags.json", "[]"),
        ("ignition/tag-definition/P/F/unary-resource.json", "{}"),
    ];
    let one = |ty: &str, name: Option<&str>| -> Vec<(String, String)> {
        let dir = match name {
            Some(n) => format!("ignition/{ty}/{n}"),
            None => format!("ignition/{ty}"),
        };
        vec![
            (format!("{dir}/config.json"), "{}".to_string()),
            (format!("{dir}/resource.json"), "{}".to_string()),
        ]
    };
    let mut live: Vec<(String, String)> = tag_unit
        .iter()
        .map(|(p, b)| ((*p).to_string(), (*b).to_string()))
        .collect();
    live.extend(one("database-connection", Some("DB")));
    live.extend(one("identity-provider", Some("IDP")));
    live.extend(one("store-and-forward-engine", None));
    live.extend(one("database-translator", Some("T")));
    let live_refs: Vec<(&str, &str)> = live.iter().map(|(p, b)| (p.as_str(), b.as_str())).collect();
    let paths: Vec<&str> = live_refs.iter().map(|(p, _)| *p).collect();

    let p = plan_ledger(&RenderedTree::default(), &observed(&[], &live_refs), &paths);
    let reason = |key: &str| {
        let e = entry(&p, key);
        assert_eq!(e.status, PlanStatus::Removed, "{key}");
        assert!(e.destructive, "{key}");
        e.destructive_reason.clone().expect("reason")
    };
    assert_eq!(reason("ignition/tag-definition/P/F"), "tag_data_loss");
    assert_eq!(
        reason("ignition/database-connection/DB"),
        "connection_removed"
    );
    assert_eq!(
        reason("ignition/identity-provider/IDP"),
        "connection_removed"
    );
    assert_eq!(
        reason("ignition/store-and-forward-engine"),
        "storage_removed"
    );
    assert_eq!(reason("ignition/database-translator/T"), "resource_removed");
    assert!(entry(&p, "ignition/identity-provider/IDP").lockout_risk);
    assert!(!entry(&p, "ignition/database-connection/DB").lockout_risk);
}

#[test]
fn lockout_risk_follows_status_and_type() {
    let idp = |name: &str, body: &str| -> Vec<(String, String)> {
        vec![
            (
                format!("ignition/identity-provider/{name}/config.json"),
                body.to_string(),
            ),
            (
                format!("ignition/identity-provider/{name}/resource.json"),
                "{}".to_string(),
            ),
        ]
    };
    let mut want = idp("CHANGED", "new");
    want.extend(idp("SAME", "s"));
    let mut live = idp("CHANGED", "old");
    live.extend(idp("SAME", "s"));
    live.extend(idp("UNMANAGED", "u"));
    let want_refs: Vec<(&str, &str)> = want.iter().map(|(p, b)| (p.as_str(), b.as_str())).collect();
    let live_refs: Vec<(&str, &str)> = live.iter().map(|(p, b)| (p.as_str(), b.as_str())).collect();
    let p = plan_units(&rendered(&want_refs), &observed(&[], &live_refs));

    assert!(entry(&p, "ignition/identity-provider/CHANGED").lockout_risk);
    assert!(!entry(&p, "ignition/identity-provider/SAME").lockout_risk);
    assert!(
        !entry(&p, "ignition/identity-provider/UNMANAGED").lockout_risk,
        "a live unit the plan never touches cannot lock anyone out"
    );
    // changed without delete-set files is not destructive
    assert!(!entry(&p, "ignition/identity-provider/CHANGED").destructive);

    // a changed security-zone is a lockout risk too
    let zone = [("ignition/security-zone/Z/config.json", "{}")];
    let zone_old = [("ignition/security-zone/Z/config.json", "{ }")];
    let p = plan_units(&rendered(&zone), &observed(&[], &zone_old));
    assert!(entry(&p, "ignition/security-zone/Z").lockout_risk);
}

#[test]
fn no_ledger_no_removals() {
    let live = [(B_CFG, "{}"), (B_RES, "{}")];
    let p = plan_units(&RenderedTree::default(), &observed(&[], &live));
    assert!(p.delete_set.files.is_empty());
    assert_eq!(p.summary.removed, 0);
    assert_eq!(p.summary.unmanaged, 1);
    assert!(p.entries.iter().all(|e| !e.destructive && !e.lockout_risk));
}

#[test]
fn empty_ledger_files_valid() {
    let live = [(B_CFG, "{}"), (B_RES, "{}")];
    let with_empty = plan_ledger(&RenderedTree::default(), &observed(&[], &live), &[]);
    let without = plan_units(&RenderedTree::default(), &observed(&[], &live));
    assert!(with_empty.delete_set.files.is_empty());
    assert_eq!(with_empty.summary.removed, 0);
    assert_eq!(
        with_empty.plan_sha256, without.plan_sha256,
        "an empty delete set adds no D lines"
    );
}

#[tokio::test]
async fn ledger_change_changes_hash() {
    let fx = PlanFixture::new();
    let (declared, reader) = fx.load("dev");
    fx.live_render("dev", &declared);
    fx.mode("ignition/database-translator/OLD/config.json", b"{}");
    fx.mode("ignition/database-translator/OLD/resource.json", b"{}");

    let none = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect("plan");
    assert!(none.delete_set.files.is_empty());

    fx.ledger(GATEWAY, MODE, &[]);
    let empty = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect("plan");
    assert_eq!(empty.plan_sha256, none.plan_sha256);

    fx.ledger(
        GATEWAY,
        MODE,
        &["ignition/database-translator/OLD/config.json"],
    );
    let one = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect("plan");
    assert_ne!(one.plan_sha256, none.plan_sha256, "D lines bind the set");
    assert_eq!(one.delete_set.files.len(), 1);
    assert_eq!(
        one.plan_sha256,
        recompute_plan_sha256(&one),
        "documented text"
    );

    fx.ledger(
        GATEWAY,
        MODE,
        &[
            "ignition/database-translator/OLD/config.json",
            "ignition/database-translator/OLD/resource.json",
        ],
    );
    let two = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect("plan");
    assert_ne!(two.plan_sha256, one.plan_sha256);
    assert_eq!(two.plan_sha256, recompute_plan_sha256(&two));
    assert_eq!(two.summary.removed, 1);

    // idempotent over unchanged inputs
    let again = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect("plan");
    assert_eq!(
        serde_json::to_string(&again).expect("json"),
        serde_json::to_string(&two).expect("json")
    );
}

/// Pinned `plan_sha256` of the golden scenario with a delete set (below).
/// Changing the canonical text in `deploy/plan.rs` means re-goldening this
/// value too. `PLAN_GOLDEN` (no ledger) stays as 22-01 pinned it.
const PLAN_GOLDEN_LEDGER: &str = "5084afccdf21436e192a5e71b8da3b4ba5cafa49fa822552ab393ffdc963f87a";

#[tokio::test]
async fn plan_sha256_golden_with_ledger() {
    // the PLAN_GOLDEN scenario plus a removed unit named by the ledger
    let fx = PlanFixture::new();
    let (declared, reader) = fx.load("dev");
    fx.live_render("dev", &declared);
    let mut bytes = fx.read("dev", DB_CONFIG);
    bytes.push(b' ');
    fx.mode(DB_CONFIG, &bytes);
    fx.remove("dev", "ignition/general-alarm-settings");
    fx.mode("ignition/database-translator/OLD/config.json", b"{}");
    fx.mode("ignition/database-translator/OLD/resource.json", b"{}");
    fx.ledger(
        GATEWAY,
        MODE,
        &[
            "ignition/database-translator/OLD/config.json",
            "ignition/database-translator/OLD/resource.json",
        ],
    );
    let result = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect("plan");
    assert_eq!(
        (
            result.summary.changed,
            result.summary.added,
            result.summary.removed
        ),
        (1, 1, 1)
    );
    assert_eq!(result.plan_sha256, recompute_plan_sha256(&result));
    assert_eq!(
        result.plan_sha256, PLAN_GOLDEN_LEDGER,
        "re-golden on purpose only"
    );
    assert_ne!(result.plan_sha256, PLAN_GOLDEN);
}

#[tokio::test]
async fn deploy_plan_refuses_corrupt_ledger() {
    let fx = PlanFixture::new();
    let (declared, reader) = fx.load("dev");
    fx.live_render("dev", &declared);
    fx.mode("ignition/database-translator/OLD/config.json", b"{}");

    let good = ledger_of(&["ignition/database-translator/OLD/config.json"]);
    let mut other_mode = good.clone();
    other_mode.mode = "prod".to_string();
    let mut other_gateway = good.clone();
    other_gateway.gateway = "elsewhere".to_string();

    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("empty", Vec::new()),
        ("garbage", b"not json".to_vec()),
        ("other mode", other_mode.to_bytes()),
        ("other gateway", other_gateway.to_bytes()),
        (
            "config-mode.json",
            ledger_of(&["config-mode.json"]).to_bytes(),
        ),
    ];
    for (name, bytes) in cases {
        fx.ledger_bytes(GATEWAY, MODE, &bytes);
        let err = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
            .await
            .expect_err(name);
        assert_eq!(
            (err.code(), err.exit_code()),
            ("deploy_ledger_invalid", 6),
            "{name}: {err}"
        );
        assert!(
            err.to_string()
                .contains("ign-deploy/tracer-gw/dev/ledger.json"),
            "{name}: {err}"
        );
    }
}

#[tokio::test]
async fn plan_never_writes_the_ledger() {
    let fx = PlanFixture::new();
    let (declared, reader) = fx.load("dev");
    fx.live_render("dev", &declared);
    fx.ledger(
        GATEWAY,
        MODE,
        &["ignition/database-translator/POSTGRES/config.json"],
    );
    let path = fx
        .data_dir()
        .join("ign-deploy")
        .join(GATEWAY)
        .join(MODE)
        .join("ledger.json");
    let before = std::fs::read(&path).expect("ledger");
    let modified = std::fs::metadata(&path)
        .expect("meta")
        .modified()
        .expect("mtime");
    deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect("plan");
    assert_eq!(std::fs::read(&path).expect("ledger"), before);
    assert_eq!(
        std::fs::metadata(&path)
            .expect("meta")
            .modified()
            .expect("mtime"),
        modified
    );
}

// ---- `--base` three-way drift (22-04) --------------------------------------

const DB_RESOURCE: &str = "ignition/database-translator/POSTGRES/resource.json";
const DB_KEY: &str = "ignition/database-translator/POSTGRES";
const SOURCE_DB_CONFIG: &str =
    "gateways/tracer-gw/resources/ignition/database-translator/POSTGRES/config.json";

/// Render `env` dev of the fixture's repo into `<scratch>/base` and return
/// the directory. The scratch dir is returned so it outlives the test body.
fn render_base(fx: &PlanFixture) -> (tempfile::TempDir, std::path::PathBuf) {
    let scratch = tempfile::TempDir::new().expect("scratch");
    let base = scratch.path().join("base");
    deploy_render(&fx.repo("dev"), "dev", Some(&base)).expect("render --out");
    (scratch, base)
}

fn with_base(dir: &std::path::Path) -> PlanOptions {
    PlanOptions {
        base: Some(dir.to_path_buf()),
        diff: false,
    }
}

/// Change the desired side of the repo: the render now differs from a base
/// rendered before this call.
fn edit_source(fx: &PlanFixture) {
    let path = fx.repo("dev").join(SOURCE_DB_CONFIG);
    let text = std::fs::read_to_string(&path).expect("source config");
    assert!(text.contains("\"textType\": \"text\""));
    std::fs::write(
        path,
        text.replace("\"textType\": \"text\"", "\"textType\": \"text2\""),
    )
    .expect("edit source");
}

fn drift_of(result: &DeployPlanResult, key: &str) -> Option<bool> {
    result
        .entries
        .iter()
        .find(|e| e.key == key)
        .unwrap_or_else(|| panic!("entry {key}"))
        .drift
}

async fn plan_with(fx: &PlanFixture, opts: &PlanOptions) -> Result<DeployPlanResult, CoreError> {
    let (declared, reader) = fx.load("dev");
    deploy_plan(&declared, reader.as_ref(), opts).await
}

fn invalid_input(err: &CoreError) -> &str {
    match err {
        CoreError::InvalidInput { reason } => reason,
        other => panic!("expected invalid_input, got {other:?}"),
    }
}

#[tokio::test]
async fn drift_three_way() {
    // base == desired; live edited outside the pipeline: drift
    let fx = PlanFixture::new();
    let (declared, _) = fx.load("dev");
    fx.live_render("dev", &declared);
    let (_scratch, base) = render_base(&fx);
    let mut bytes = fx.read("dev", DB_CONFIG);
    bytes.push(b' ');
    fx.mode(DB_CONFIG, &bytes);
    let result = plan_with(&fx, &with_base(&base)).await.expect("plan");
    assert_eq!(drift_of(&result, DB_KEY), Some(true));
    assert_eq!(
        drift_of(&result, "ignition/general-alarm-settings"),
        Some(false)
    );
    assert_eq!(result.summary.drifted, Some(1));
    let summary = result.base.as_ref().expect("base summary");
    assert_eq!(summary.files, 4);
    assert_eq!(summary.dir, base.to_string_lossy());

    // live == base != desired: a pipeline change not yet applied
    let fx = PlanFixture::new();
    let (declared, _) = fx.load("dev");
    fx.live_render("dev", &declared);
    let (_scratch, base) = render_base(&fx);
    edit_source(&fx);
    let result = plan_with(&fx, &with_base(&base)).await.expect("plan");
    assert_eq!(result.summary.changed, 1);
    assert_eq!(drift_of(&result, DB_KEY), Some(false));
    assert_eq!(result.summary.drifted, Some(0));

    // live == desired != base: the change was applied since the base
    let fx = PlanFixture::new();
    let (_scratch, base) = render_base(&fx);
    edit_source(&fx);
    let (declared, _) = fx.load("dev");
    fx.live_render("dev", &declared);
    let result = plan_with(&fx, &with_base(&base)).await.expect("plan");
    assert_eq!(result.summary.changed, 0);
    assert_eq!(drift_of(&result, DB_KEY), Some(false));
}

#[tokio::test]
async fn drift_for_unit_added_outside_pipeline() {
    let fx = PlanFixture::new();
    let (declared, _) = fx.load("dev");
    fx.live_render("dev", &declared);
    let (_scratch, base) = render_base(&fx);
    fx.mode("ignition/database-translator/EXTRA/config.json", b"{}");
    let result = plan_with(&fx, &with_base(&base)).await.expect("plan");
    let extra = result
        .entries
        .iter()
        .find(|e| e.key == "ignition/database-translator/EXTRA")
        .expect("extra entry");
    assert_eq!(extra.status, PlanStatus::Unmanaged);
    assert_eq!(extra.drift, Some(true));
}

#[tokio::test]
async fn drift_for_unit_deleted_outside_pipeline() {
    // Flagged assumption (DPLN-05): present in base and desired, absent
    // live: an `added` entry that is also drift.
    let fx = PlanFixture::new();
    let (declared, _) = fx.load("dev");
    fx.live_render("dev", &declared);
    let (_scratch, base) = render_base(&fx);
    fx.remove("dev", "ignition/database-translator/POSTGRES");
    let result = plan_with(&fx, &with_base(&base)).await.expect("plan");
    let entry = result
        .entries
        .iter()
        .find(|e| e.key == DB_KEY)
        .expect("entry");
    assert_eq!(entry.status, PlanStatus::Added);
    assert_eq!(entry.drift, Some(true));
}

#[tokio::test]
async fn drift_null_without_base() {
    let fx = PlanFixture::new();
    let (declared, _) = fx.load("dev");
    fx.live_render("dev", &declared);
    fx.mode("ignition/database-translator/EXTRA/config.json", b"{}");
    let result = plan_with(&fx, &PlanOptions::default()).await.expect("plan");
    assert!(result.base.is_none());
    assert_eq!(result.summary.drifted, None);
    for entry in &result.entries {
        assert_eq!(entry.drift, None, "{}", entry.key);
    }
    let json = serde_json::to_value(&result).expect("json");
    assert!(json["base"].is_null());
    assert!(json["entries"][0]["drift"].is_null());
}

#[tokio::test]
async fn hash_independent_of_base() {
    let fx = PlanFixture::new();
    let (declared, _) = fx.load("dev");
    fx.live_render("dev", &declared);
    let (_scratch, base) = render_base(&fx);
    let mut bytes = fx.read("dev", DB_CONFIG);
    bytes.push(b' ');
    fx.mode(DB_CONFIG, &bytes);
    let without = plan_with(&fx, &PlanOptions::default()).await.expect("plan");
    let with = plan_with(&fx, &with_base(&base)).await.expect("plan");
    assert_eq!(drift_of(&with, DB_KEY), Some(true));
    assert_eq!(without.plan_sha256, with.plan_sha256);
}

#[tokio::test]
async fn base_identity_mismatch_refused() {
    for (field, needle) in [("gateway", "gateway"), ("env", "env"), ("mode", "mode")] {
        let fx = PlanFixture::new();
        let (declared, _) = fx.load("dev");
        fx.live_render("dev", &declared);
        let (_scratch, base) = render_base(&fx);
        let marker = base.join("render.json");
        let text = std::fs::read_to_string(&marker).expect("marker");
        let from = match field {
            "gateway" => "\"gateway\": \"tracer-gw\"",
            "env" => "\"env\": \"dev\"",
            _ => "\"mode\": \"dev\"",
        };
        assert!(text.contains(from), "{text}");
        let to = from
            .replace("tracer-gw", "other-gw")
            .replace("\"dev\"", "\"prod\"");
        std::fs::write(&marker, text.replace(from, &to)).expect("tamper");
        let err = plan_with(&fx, &with_base(&base))
            .await
            .expect_err("identity mismatch");
        assert_eq!(err.code(), "invalid_input", "{field}");
        let reason = invalid_input(&err);
        assert!(reason.contains("--base"), "{reason}");
        assert!(reason.contains(needle), "{field}: {reason}");
    }
}

#[tokio::test]
async fn base_modified_refused() {
    async fn refused(tamper: impl Fn(&std::path::Path)) -> String {
        let fx = PlanFixture::new();
        let (declared, _) = fx.load("dev");
        fx.live_render("dev", &declared);
        let (_scratch, base) = render_base(&fx);
        tamper(&base);
        let err = plan_with(&fx, &with_base(&base))
            .await
            .expect_err("a modified base is refused");
        assert_eq!(err.code(), "invalid_input");
        invalid_input(&err).to_string()
    }

    // an extra file
    let reason = refused(|b| std::fs::write(b.join("extra.json"), b"{}").expect("write")).await;
    assert!(reason.contains("extra.json"), "{reason}");

    // a changed byte in a listed file; the marker still says the old hash,
    // which proves the file is re-hashed
    let reason = refused(|b| {
        let path = b.join(DB_CONFIG);
        let mut bytes = std::fs::read(&path).expect("read");
        bytes.push(b' ');
        std::fs::write(path, bytes).expect("write");
    })
    .await;
    assert!(reason.contains(DB_CONFIG), "{reason}");

    // a changed byte AND the marker's per-file hash and size rewritten to
    // match: only the listing digest recorded in the marker still disagrees
    let reason = refused(|b| {
        let path = b.join(DB_CONFIG);
        let mut bytes = std::fs::read(&path).expect("read");
        bytes.push(b' ');
        std::fs::write(&path, &bytes).expect("write");
        let marker = b.join("render.json");
        let mut doc: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&marker).expect("marker")).expect("json");
        for file in doc["files"].as_array_mut().expect("files") {
            if file["path"] == DB_CONFIG {
                file["sha256"] = sha256_hex(&bytes).into();
                file["bytes"] = bytes.len().into();
            }
        }
        std::fs::write(&marker, serde_json::to_vec_pretty(&doc).expect("json")).expect("marker");
    })
    .await;
    assert!(reason.contains("render_sha256"), "{reason}");

    // a leftover temporary file of an interrupted refresh (R-03 / WR-03)
    let reason = refused(|b| {
        std::fs::write(
            b.join("ignition/database-translator/POSTGRES/config.json.ign-render-tmp"),
            b"{",
        )
        .expect("write")
    })
    .await;
    assert!(reason.contains("ign-render-tmp"), "{reason}");

    // a listed file missing
    let reason = refused(|b| std::fs::remove_file(b.join(DB_RESOURCE)).expect("remove")).await;
    assert!(reason.contains(DB_RESOURCE), "{reason}");

    // no marker at all
    let reason = refused(|b| std::fs::remove_file(b.join("render.json")).expect("remove")).await;
    assert!(reason.contains("render.json"), "{reason}");

    // a marker that is not a marker
    let reason =
        refused(|b| std::fs::write(b.join("render.json"), b"{\"x\":1}").expect("write")).await;
    assert!(reason.contains("render.json"), "{reason}");
}

#[tokio::test]
async fn base_hostile_refused() {
    let fx = PlanFixture::new();
    let (declared, _) = fx.load("dev");
    fx.live_render("dev", &declared);
    let scratch = tempfile::TempDir::new().expect("scratch");

    // absent
    let missing = scratch.path().join("no-such-base");
    let err = plan_with(&fx, &with_base(&missing))
        .await
        .expect_err("absent");
    assert_eq!(err.code(), "invalid_input");
    assert!(invalid_input(&err).contains("--base"), "{err}");

    // a regular file
    let file = scratch.path().join("a-file");
    std::fs::write(&file, b"x").expect("write");
    let err = plan_with(&fx, &with_base(&file)).await.expect_err("file");
    assert_eq!(err.code(), "invalid_input");

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let (_keep, base) = render_base(&fx);

        // a symlinked base
        let link = scratch.path().join("link-to-base");
        symlink(&base, &link).expect("symlink");
        let err = plan_with(&fx, &with_base(&link))
            .await
            .expect_err("symlink base");
        assert_eq!(err.code(), "invalid_input");
        assert!(invalid_input(&err).contains("symlink"), "{err}");

        // a symlink inside the base
        symlink("/etc/hosts", base.join("sneaky")).expect("symlink");
        let err = plan_with(&fx, &with_base(&base))
            .await
            .expect_err("inner symlink");
        assert_eq!(err.code(), "invalid_input");
        assert!(invalid_input(&err).contains("sneaky"), "{err}");
    }
}

#[tokio::test]
async fn bad_base_costs_no_transport_work() {
    // the data dir does not exist: a transport read would be transport_failed
    let fx = PlanFixture::new();
    let missing = fx.data_dir().join("no-such-data-dir");
    let repo = fx.repo_with_master(
        "dev",
        &format!("    local:\n      data_dir: '{}'\n", missing.display()),
    );
    let declared = load_declared(&repo, "dev").expect("declared");
    let reader = transport::for_node(
        &repo,
        &declared.target_file,
        "nodes.master",
        &declared.target.nodes.master,
    )
    .expect("reader");
    let scratch = tempfile::TempDir::new().expect("scratch");
    let err = deploy_plan(
        &declared,
        reader.as_ref(),
        &with_base(&scratch.path().join("absent")),
    )
    .await
    .expect_err("refused");
    assert_eq!(
        err.code(),
        "invalid_input",
        "base is checked before the read"
    );
}

// ---- `--diff` with secret withholding (22-04) -------------------------------

const CANARY: &str = "IGN-CANARY-22-03";
const ALARM_CONFIG: &str = "ignition/general-alarm-settings/config.json";

fn diff_opts() -> PlanOptions {
    PlanOptions {
        base: None,
        diff: true,
    }
}

fn diff_of<'a>(result: &'a DeployPlanResult, key: &str) -> Option<&'a str> {
    result
        .entries
        .iter()
        .find(|e| e.key == key)
        .unwrap_or_else(|| panic!("entry {key}"))
        .diff
        .as_deref()
}

/// Live equals the render, except `path` which holds `bytes` instead.
fn live_with(fx: &PlanFixture, path: &str, bytes: &[u8]) {
    let (declared, _) = fx.load("dev");
    fx.live_render("dev", &declared);
    fx.mode(path, bytes);
}

#[tokio::test]
async fn diff_changed_json_shows_unified_text() {
    let fx = PlanFixture::new();
    let live = String::from_utf8({
        let (declared, _) = fx.load("dev");
        fx.live_render("dev", &declared);
        fx.read("dev", DB_CONFIG)
    })
    .expect("utf-8")
    .replace("\"textType\": \"text\"", "\"textType\": \"textOLD\"");
    fx.mode(DB_CONFIG, live.as_bytes());

    let result = plan_with(&fx, &diff_opts()).await.expect("plan");
    let diff = diff_of(&result, DB_KEY).expect("a changed entry carries a diff");
    assert!(diff.contains(&format!("--- live/{DB_CONFIG}")), "{diff}");
    assert!(diff.contains(&format!("+++ desired/{DB_CONFIG}")), "{diff}");
    assert!(diff.contains("-  \"textType\": \"textOLD\""), "{diff}");
    assert!(diff.contains("+  \"textType\": \"text\""), "{diff}");
    assert!(diff.contains("@@"), "{diff}");
    // an entry that is the same carries none
    assert_eq!(diff_of(&result, "ignition/general-alarm-settings"), None);
}

#[tokio::test]
async fn diff_added_and_removed_entries() {
    let fx = PlanFixture::new();
    let (declared, _) = fx.load("dev");
    fx.live_render("dev", &declared);
    fx.remove("dev", "ignition/database-translator/POSTGRES");
    fx.mode(
        "ignition/database-translator/OLD/config.json",
        b"{\n  \"a\": 1\n}\n",
    );
    fx.ledger(
        GATEWAY,
        MODE,
        &["ignition/database-translator/OLD/config.json"],
    );
    let result = plan_with(&fx, &diff_opts()).await.expect("plan");

    let added = diff_of(&result, DB_KEY).expect("added entry diff");
    assert!(added.contains("+  \"textType\": \"text\""), "{added}");
    assert!(!added.lines().any(|l| l.starts_with("-  ")), "{added}");

    let removed = diff_of(&result, "ignition/database-translator/OLD").expect("removed diff");
    assert!(removed.contains("-  \"a\": 1"), "{removed}");
}

#[tokio::test]
async fn diff_withholds_secret_bearing_member() {
    let fx = PlanFixture::new();
    let live = format!(
        "{{\n  \"extra\": {{\"type\": \"Embedded\", \"data\": {{\"ciphertext\": \"{CANARY}\"}}}}\n}}\n"
    );
    live_with(&fx, DB_CONFIG, live.as_bytes());
    let (declared, _) = fx.load("dev");
    let desired = sha256_hex(&declared.outcome.tree.files[&RelPath::new(DB_CONFIG).unwrap()]);

    let result = plan_with(&fx, &diff_opts()).await.expect("plan");
    let diff = diff_of(&result, DB_KEY).expect("diff");
    assert!(diff.contains(DIFF_WITHHELD), "{diff}");
    assert!(diff.contains(&sha256_hex(live.as_bytes())), "{diff}");
    assert!(diff.contains(&desired), "{diff}");
    assert!(
        !diff.contains(CANARY) && !diff.contains("ciphertext"),
        "{diff}"
    );
    assert!(
        !diff.contains("@@"),
        "no text diff for a withheld member: {diff}"
    );
}

#[tokio::test]
async fn diff_withholds_when_only_the_desired_side_is_secret_bearing() {
    // desired side secret-bearing cannot come from a real render (render
    // refuses it), so drive the engine directly
    let path = RelPath::new(DB_CONFIG).unwrap();
    let secret =
        format!("{{\"k\": {{\"type\": \"Embedded\", \"data\": {{\"c\": \"{CANARY}\"}}}}}}\n");
    let rendered = RenderedTree {
        files: BTreeMap::from([(path.clone(), secret.clone().into_bytes())]),
    };
    let live_bytes = b"{\n  \"k\": 1\n}\n".to_vec();
    let observed = ObservedFiles {
        core: BTreeMap::new(),
        mode: BTreeMap::from([(
            path,
            FileBlob {
                sha256: sha256_hex(&live_bytes),
                bytes: live_bytes,
            },
        )]),
        local_units: Vec::new(),
        unreadable: Vec::new(),
    };
    let scope = Scope {
        mode: MODE.to_string(),
        types: vec![("ignition".into(), "database-translator".into())],
        tag_providers: Vec::new(),
    };
    let mut p = plan_with_ledger(&rendered, &observed, None, &scope, &default_excluded());
    ignition_core::deploy::plan::render_diffs(&mut p, &rendered, &observed);
    let diff = p.entries[0].diff.as_deref().expect("diff");
    assert!(diff.contains(DIFF_WITHHELD), "{diff}");
    assert!(!diff.contains(CANARY), "{diff}");
}

#[tokio::test]
async fn diff_non_json_member_is_hash_only() {
    let fx = PlanFixture::new();
    let (declared, _) = fx.load("dev");
    fx.live_render("dev", &declared);
    let text = "ignition/database-translator/POSTGRES/notes.txt";
    let broken = "ignition/database-translator/POSTGRES/broken.json";
    fx.mode(text, b"password=IGN-CANARY-22-03\n");
    fx.mode(broken, b"{ not json IGN-CANARY-22-03");
    let result = plan_with(&fx, &diff_opts()).await.expect("plan");
    let diff = diff_of(&result, DB_KEY).expect("diff");
    assert!(
        diff.contains(&format!(
            "{text}: differs (live {}, desired -)",
            sha256_hex(b"password=IGN-CANARY-22-03\n")
        )),
        "{diff}"
    );
    assert!(
        diff.contains(&format!("{broken}: differs (live ")),
        "{diff}"
    );
    assert!(!diff.contains(CANARY), "{diff}");
    assert!(!diff.contains("@@"), "{diff}");
}

#[tokio::test]
async fn diff_is_capped() {
    let fx = PlanFixture::new();
    let (declared, _) = fx.load("dev");
    fx.live_render("dev", &declared);
    let mut big = String::from("{\n");
    for i in 0..10_000 {
        big.push_str(&format!("  \"k{i:05}\": \"value-{i:05}\",\n"));
    }
    big.push_str("  \"last\": 0\n}\n");
    fx.mode(DB_CONFIG, big.as_bytes());
    let result = plan_with(&fx, &diff_opts()).await.expect("plan");
    let diff = diff_of(&result, DB_KEY).expect("diff");
    let marker = format!("... [diff truncated at {DIFF_MAX_BYTES} bytes]");
    assert!(
        diff.ends_with(&marker),
        "{}",
        &diff[diff.len().saturating_sub(120)..]
    );
    assert!(
        diff.len() <= DIFF_MAX_BYTES + marker.len() + 1,
        "{}",
        diff.len()
    );
}

#[tokio::test]
async fn diff_absent_without_flag() {
    let fx = PlanFixture::new();
    let live = String::from_utf8({
        let (declared, _) = fx.load("dev");
        fx.live_render("dev", &declared);
        fx.read("dev", DB_CONFIG)
    })
    .expect("utf-8")
    .replace("\"textType\": \"text\"", "\"textType\": \"textOLD\"");
    fx.mode(DB_CONFIG, live.as_bytes());

    let without = plan_with(&fx, &PlanOptions::default()).await.expect("plan");
    for entry in &without.entries {
        assert_eq!(entry.diff, None, "{}", entry.key);
    }
    let json = serde_json::to_value(&without).expect("json");
    assert!(json["entries"][0]["diff"].is_null());

    let with = plan_with(&fx, &diff_opts()).await.expect("plan");
    assert!(diff_of(&with, DB_KEY).is_some());
    assert_eq!(
        without.plan_sha256, with.plan_sha256,
        "diff is hash-excluded"
    );
}

#[tokio::test]
async fn canary_never_leaves_plan() {
    let fx = PlanFixture::new();
    let (declared, _) = fx.load("dev");
    fx.live_render("dev", &declared);
    let (_scratch, base) = render_base(&fx);

    // a changed unit with Embedded ciphertext, a changed unit with a
    // plaintext password key, an unmanaged unit and a removed unit
    fx.mode(
        DB_CONFIG,
        format!(
            "{{\"x\": {{\"type\": \"Embedded\", \"data\": {{\"ciphertext\": \"{CANARY}\"}}}}}}"
        )
        .as_bytes(),
    );
    fx.mode(
        ALARM_CONFIG,
        format!("{{\"password\": \"{CANARY}\"}}").as_bytes(),
    );
    fx.mode(
        "ignition/database-translator/LEAK/config.json",
        format!(
            "{{\"x\": {{\"type\": \"Embedded\", \"data\": {{\"ciphertext\": \"{CANARY}\"}}}}}}"
        )
        .as_bytes(),
    );
    fx.mode(
        "ignition/database-translator/OLD/config.json",
        format!("{{\"password\": \"{CANARY}\"}}").as_bytes(),
    );
    fx.ledger(
        GATEWAY,
        MODE,
        &["ignition/database-translator/OLD/config.json"],
    );

    let opts = PlanOptions {
        base: Some(base),
        diff: true,
    };
    let result = plan_with(&fx, &opts).await.expect("plan");
    for key in [
        DB_KEY,
        "ignition/general-alarm-settings",
        "ignition/database-translator/OLD",
    ] {
        let diff = diff_of(&result, key).expect("diff");
        assert!(diff.contains(DIFF_WITHHELD), "{key}: {diff}");
    }
    assert_eq!(diff_of(&result, "ignition/database-translator/LEAK"), None);

    let json = serde_json::to_string(&result).expect("json");
    let debug = format!("{result:?}");
    let pretty = serde_json::to_string_pretty(&result).expect("json");
    for (name, text) in [("json", &json), ("debug", &debug), ("pretty", &pretty)] {
        assert!(!text.contains(CANARY), "canary leaked into {name}");
        assert!(
            !text.contains("ciphertext"),
            "ciphertext leaked into {name}"
        );
    }
}

// ---- `--diff` withholds plaintext secrets on the LIVE side (22-04 review) ---
//
// The render lints were built for the desired (repo) side. The live side is
// gateway-written and never linted, so `--diff` runs one stricter,
// diff-only key-name check over both sides of every parsed `.json` member.

const DIFF_CANARY: &str = "IGN-CANARY-DIFF";

/// The three renderings of a result a canary must stay out of: JSON, pretty
/// JSON and `Debug`. The human form prints only `entry.diff`, so the diff
/// text itself is checked by each test.
fn assert_no_canary(result: &DeployPlanResult) {
    let json = serde_json::to_string(result).expect("json");
    let pretty = serde_json::to_string_pretty(result).expect("json");
    let debug = format!("{result:?}");
    for (name, text) in [("json", &json), ("pretty", &pretty), ("debug", &debug)] {
        assert!(
            !text.contains(DIFF_CANARY),
            "canary leaked into {name}: {text}"
        );
    }
    for entry in &result.entries {
        if let Some(diff) = &entry.diff {
            assert!(
                !diff.contains(DIFF_CANARY),
                "canary in the human diff text: {diff}"
            );
        }
    }
}

#[tokio::test]
async fn diff_withholds_live_non_config_json_with_a_secret_key() {
    let fx = PlanFixture::new();
    let (declared, _) = fx.load("dev");
    fx.live_render("dev", &declared);
    let extra = "ignition/database-translator/POSTGRES/extra.json";
    let token = "ignition/database-translator/POSTGRES/token.json";
    // a differing non-config member of a resource, and two live-only members
    let live_resource = format!("{{\"password\":\"{DIFF_CANARY}\"}}");
    let live_extra = format!("{{\"nested\":{{\"apiKey\":\"{DIFF_CANARY}\"}}}}");
    let live_token = format!("{{\"list\":[{{\"Token\":\"{DIFF_CANARY}\"}}]}}");
    fx.mode(DB_RESOURCE, live_resource.as_bytes());
    fx.mode(extra, live_extra.as_bytes());
    fx.mode(token, live_token.as_bytes());

    let result = plan_with(&fx, &diff_opts()).await.expect("plan");
    let diff = diff_of(&result, DB_KEY).expect("diff");
    for path in [DB_RESOURCE, extra, token] {
        assert!(
            diff.contains(&format!("{path}: {DIFF_WITHHELD} (1); live ")),
            "{path} must be withheld: {diff}"
        );
    }
    assert!(!diff.contains("@@"), "no text diff survives: {diff}");
    assert_no_canary(&result);
}

#[tokio::test]
async fn diff_withholds_live_tag_file_with_a_secret_key() {
    let tags = "ignition/tag-definition/P/F/tags.json";
    let marker = "ignition/tag-definition/P/F/unary-resource.json";
    let rendered = RenderedTree {
        files: BTreeMap::from([
            (RelPath::new(tags).unwrap(), b"[]\n".to_vec()),
            (RelPath::new(marker).unwrap(), b"{}\n".to_vec()),
        ]),
    };
    for (key, secret) in [
        ("password", "IGN-CANARY-DIFF"),
        ("token", "IGN-CANARY-DIFF"),
        ("authToken", "IGN-CANARY-DIFF"),
    ] {
        let live_tags = format!("[{{\"name\":\"t\",\"parameters\":{{\"{key}\":\"{secret}\"}}}}]");
        let live_tags = live_tags.into_bytes();
        let blob = |bytes: &[u8]| FileBlob {
            sha256: sha256_hex(bytes),
            bytes: bytes.to_vec(),
        };
        let observed = ObservedFiles {
            core: BTreeMap::new(),
            mode: BTreeMap::from([
                (RelPath::new(tags).unwrap(), blob(&live_tags)),
                (RelPath::new(marker).unwrap(), blob(b"{}\n")),
            ]),
            local_units: Vec::new(),
            unreadable: Vec::new(),
        };
        let mut p = plan_with_ledger(&rendered, &observed, None, &scope(), &default_excluded());
        ignition_core::deploy::plan::render_diffs(&mut p, &rendered, &observed);
        let entry = p
            .entries
            .iter()
            .find(|e| e.key == "ignition/tag-definition/P/F")
            .expect("tag folder entry");
        let diff = entry.diff.as_deref().expect("diff");
        assert!(
            diff.contains(&format!("{tags}: {DIFF_WITHHELD} (1); live ")),
            "{key}: {diff}"
        );
        assert!(!diff.contains(DIFF_CANARY), "{key}: {diff}");
        assert!(!format!("{p:?}").contains(DIFF_CANARY), "{key}: debug");
    }
}

#[tokio::test]
async fn diff_secret_key_check_covers_the_desired_side_and_the_key_list() {
    // every key-name stem, case-insensitively, on the desired side of a
    // member that exists on both sides
    for key in [
        "password",
        "PASSWD",
        "dbSecret",
        "accessToken",
        "apikey",
        "API_KEY",
        "credentials",
        "privateKey",
        "private_key",
    ] {
        let path = RelPath::new("ignition/database-translator/X/extra.json").unwrap();
        let desired = format!("{{\"{key}\":\"{DIFF_CANARY}\"}}\n").into_bytes();
        let live = b"{\"a\":1}\n".to_vec();
        let rendered = RenderedTree {
            files: BTreeMap::from([(path.clone(), desired)]),
        };
        let observed = ObservedFiles {
            core: BTreeMap::new(),
            mode: BTreeMap::from([(
                path,
                FileBlob {
                    sha256: sha256_hex(&live),
                    bytes: live,
                },
            )]),
            local_units: Vec::new(),
            unreadable: Vec::new(),
        };
        let mut p = plan_with_ledger(&rendered, &observed, None, &scope(), &default_excluded());
        ignition_core::deploy::plan::render_diffs(&mut p, &rendered, &observed);
        let diff = p.entries[0].diff.as_deref().expect("diff");
        assert!(diff.contains(DIFF_WITHHELD), "{key}: {diff}");
        assert!(!diff.contains(DIFF_CANARY), "{key}: {diff}");
    }
}

#[tokio::test]
async fn diff_secret_key_check_ignores_empty_and_null_values_and_benign_keys() {
    let fx = PlanFixture::new();
    let (declared, _) = fx.load("dev");
    fx.live_render("dev", &declared);
    let extra = "ignition/database-translator/POSTGRES/extra.json";
    let benign = "ignition/database-translator/POSTGRES/benign.json";
    // an empty secret-named string and a null are not secrets
    fx.mode(extra, b"{\n  \"password\": \"\",\n  \"token\": null\n}\n");
    fx.mode(benign, b"{\n  \"label\": \"visible-benign-value\"\n}\n");
    let result = plan_with(&fx, &diff_opts()).await.expect("plan");
    let diff = diff_of(&result, DB_KEY).expect("diff");
    assert!(
        diff.contains("+  \"label\"") || diff.contains("-  \"label\""),
        "{diff}"
    );
    assert!(
        diff.contains("visible-benign-value"),
        "benign member still diffs: {diff}"
    );
    assert!(
        diff.contains("\"password\": \"\""),
        "empty value diffs: {diff}"
    );
    assert!(!diff.contains(DIFF_WITHHELD), "{diff}");
}

#[tokio::test]
async fn diff_secret_key_check_leaves_the_plan_hash_alone() {
    let fx = PlanFixture::new();
    let (declared, _) = fx.load("dev");
    fx.live_render("dev", &declared);
    fx.mode(
        DB_RESOURCE,
        format!("{{\"password\":\"{DIFF_CANARY}\"}}").as_bytes(),
    );
    let without = plan_with(&fx, &PlanOptions::default()).await.expect("plan");
    let with = plan_with(&fx, &diff_opts()).await.expect("plan");
    assert_eq!(without.plan_sha256, with.plan_sha256);
    assert_no_canary(&with);
}

// ---------------------------------------------------------------------
// quick-261029-ms3: an excluded tag provider is never deleted
// ---------------------------------------------------------------------

#[test]
fn excluded_provider_ledger_files_are_never_deleted() {
    const MQTT_PROVIDER: &str = "ignition/tag-provider/MQTT Engine/config.json";
    const MQTT_TAGS: &str = "ignition/tag-definition/MQTT Engine/Edge Nodes/tags.json";
    let excluded = vec!["MQTT Engine".to_string(), "System".to_string()];
    let files = [(A_CFG, "{}"), ("ignition/tag-provider/P/config.json", "{}")];
    let tree = rendered(&files);
    let rendered_paths: Vec<&str> = files.iter().map(|(p, _)| *p).collect();
    let mut with_excluded = rendered_paths.clone();
    with_excluded.extend([MQTT_PROVIDER, MQTT_TAGS]);
    let one_warning = |p: &Plan| {
        let hits: Vec<&String> = p
            .warnings
            .iter()
            .filter(|w| w.contains("MQTT Engine"))
            .collect();
        assert_eq!(hits.len(), 1, "{:?}", p.warnings);
        assert!(hits[0].contains("2 file(s)"), "{}", hits[0]);
        assert!(
            !p.warnings.iter().any(|w| w.contains("no longer allows")),
            "the excluded skip runs before in_scope: {:?}",
            p.warnings
        );
    };

    // (a) the realistic case: the scope does not name the provider and the
    // files are not read
    let live = observed(&[], &files);
    let p = plan_with_ledger(
        &tree,
        &live,
        Some(&ledger_of(&with_excluded)),
        &scope(),
        &excluded,
    );
    assert!(deleted(&p).is_empty(), "{:?}", deleted(&p));
    assert!(!p.summary.has_changes, "{:?}", p.summary);
    one_warning(&p);
    let without = plan_with_ledger(
        &tree,
        &live,
        Some(&ledger_of(&rendered_paths)),
        &scope(),
        &excluded,
    );
    assert_eq!(
        p.plan_sha256, without.plan_sha256,
        "no D line, exclusions not hashed"
    );

    // (b) defense in depth: the scope names the provider and its files are
    // live in the mode collection
    let mut wide = scope();
    wide.tag_providers.push("MQTT Engine".to_string());
    let mut live_files = files.to_vec();
    live_files.extend([(MQTT_PROVIDER, "{}"), (MQTT_TAGS, "{}")]);
    let live = observed(&[], &live_files);
    let p = plan_with_ledger(
        &tree,
        &live,
        Some(&ledger_of(&with_excluded)),
        &wide,
        &excluded,
    );
    assert!(deleted(&p).is_empty(), "{:?}", deleted(&p));
    assert!(
        !p.entries.iter().any(|e| e.status == PlanStatus::Removed),
        "{:?}",
        p.entries
    );
    assert!(!p.summary.has_changes, "{:?}", p.summary);
    one_warning(&p);
    let without = plan_with_ledger(
        &tree,
        &live,
        Some(&ledger_of(&rendered_paths)),
        &wide,
        &excluded,
    );
    assert_eq!(
        p.plan_sha256, without.plan_sha256,
        "no D line, exclusions not hashed"
    );
}

/// C4 (PR #31 review): the excluded-provider warning counts a ledger path
/// under the spelling of the exclusion entry it matched (ASCII case fold),
/// not the path's own spelling, so `mqtt engine` and `MQTT Engine` give one
/// count under `MQTT Engine`.
#[test]
fn excluded_provider_ledger_counts_use_the_exclusion_spelling() {
    let excluded = vec!["MQTT Engine".to_string(), "System".to_string()];
    let files = [(A_CFG, "{}"), ("ignition/tag-provider/P/config.json", "{}")];
    let tree = rendered(&files);
    let live = observed(&[], &files);
    let rendered_paths: Vec<&str> = files.iter().map(|(p, _)| *p).collect();
    let mut ledgered = rendered_paths.clone();
    ledgered.extend([
        "ignition/tag-provider/mqtt engine/config.json",
        "ignition/tag-definition/MQTT Engine/Edge Nodes/tags.json",
    ]);
    let p = plan_with_ledger(
        &tree,
        &live,
        Some(&ledger_of(&ledgered)),
        &scope(),
        &excluded,
    );
    let hits: Vec<&String> = p
        .warnings
        .iter()
        .filter(|w| w.to_ascii_lowercase().contains("mqtt engine"))
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "one count per exclusion entry: {:?}",
        p.warnings
    );
    assert!(
        hits[0].contains("2 file(s) under tag provider MQTT Engine,"),
        "{}",
        hits[0]
    );

    // the lower-case spelling alone is still named as the entry spells it
    let mut lower = rendered_paths.clone();
    lower.push("ignition/tag-provider/mqtt engine/config.json");
    let only = plan_with_ledger(&tree, &live, Some(&ledger_of(&lower)), &scope(), &excluded);
    assert!(
        only.warnings
            .iter()
            .any(|w| w.contains("1 file(s) under tag provider MQTT Engine,")),
        "{:?}",
        only.warnings
    );
    assert!(
        !only
            .warnings
            .iter()
            .any(|w| w.contains("tag provider mqtt engine")),
        "{:?}",
        only.warnings
    );
    let without = plan_with_ledger(
        &tree,
        &live,
        Some(&ledger_of(&rendered_paths)),
        &scope(),
        &excluded,
    );
    assert_eq!(p.plan_sha256, without.plan_sha256, "exclusions not hashed");
}

// ---------------------------------------------------------------------
// quick-261029-ms3: an over-long Windows live path is unreadable, never
// absent
// ---------------------------------------------------------------------

const B_KEY: &str = "ignition/database-translator/B";

fn unreadable_file(collection: &str, path: &str, length: usize) -> Unreadable {
    Unreadable {
        collection: collection.to_string(),
        path: rp(path),
        length,
    }
}

/// `observed(core, mode)` plus over-limit files.
fn observed_unreadable(
    core: &[(&str, &str)],
    mode: &[(&str, &str)],
    unreadable: Vec<Unreadable>,
) -> ObservedFiles {
    let mut live = observed(core, mode);
    live.unreadable = unreadable;
    live
}

#[test]
fn unreadable_declared_unit_is_not_an_entry_and_is_a_change() {
    let tree = rendered(&[(A_CFG, "{}"), (A_RES, "{}"), (B_CFG, "{}"), (B_RES, "{}")]);
    // A is equal live; B's resource.json is readable, its config.json is not
    let live = observed_unreadable(
        &[],
        &[(A_CFG, "{}"), (A_RES, "{}"), (B_RES, "{}")],
        vec![unreadable_file("mode", B_CFG, 262)],
    );
    let p = plan_units(&tree, &live);
    assert!(
        !p.entries.iter().any(|e| e.key == B_KEY),
        "no partial unit: {:?}",
        p.entries
    );
    assert_eq!(
        entry(&p, "ignition/database-translator/A").status,
        PlanStatus::Same
    );
    assert_eq!(
        p.unreadable,
        vec![UnreadableUnit {
            key: B_KEY.to_string(),
            live_in: vec!["mode".to_string()],
            declared: true,
            in_ledger: false,
            paths: vec![B_CFG.to_string()],
            longest: 262,
        }]
    );
    assert_eq!(p.summary.unreadable, 1);
    assert_eq!(
        (p.summary.same, p.summary.added, p.summary.changed),
        (1, 0, 0)
    );
    assert!(p.summary.has_changes, "a declared unread unit is a change");
}

#[test]
fn unreadable_ledger_file_is_never_deleted() {
    const MQTT_TAGS: &str = "ignition/tag-definition/MQTT Engine/Edge Nodes/tags.json";
    let excluded = vec!["MQTT Engine".to_string(), "System".to_string()];
    let tree = rendered(&[(A_CFG, "{}"), (A_RES, "{}")]);
    // B is not rendered: without the guard its readable resource.json would
    // be deleted and its unreadable config.json would read as absent
    let live = observed_unreadable(
        &[],
        &[(A_CFG, "{}"), (A_RES, "{}"), (B_RES, "{}")],
        vec![unreadable_file("mode", B_CFG, 262)],
    );
    let ledger = ledger_of(&[A_CFG, A_RES, B_CFG, B_RES, MQTT_TAGS]);
    let p = plan_with_ledger(&tree, &live, Some(&ledger), &scope(), &excluded);
    assert!(deleted(&p).is_empty(), "{:?}", deleted(&p));
    assert!(!p.entries.iter().any(|e| e.key == B_KEY), "{:?}", p.entries);
    assert_eq!(p.unreadable.len(), 1, "{:?}", p.unreadable);
    assert_eq!(p.unreadable[0].key, B_KEY);
    assert!(!p.unreadable[0].declared);
    assert!(p.unreadable[0].in_ledger, "the ledger names the unit");
    assert!(p.summary.has_changes, "an unread ledgered unit is a change");
    assert_eq!(p.summary.removed, 0);
    // Task 1's excluded skip composes: one warning, never counted here
    let mqtt: Vec<&String> = p
        .warnings
        .iter()
        .filter(|w| w.contains("MQTT Engine"))
        .collect();
    assert_eq!(mqtt.len(), 1, "{:?}", p.warnings);
    assert!(mqtt[0].contains("1 file(s)"), "{}", mqtt[0]);

    // the ledger naming only the unreadable file itself still marks it
    let only = ledger_of(&[A_CFG, A_RES, B_CFG]);
    let p = plan_with_ledger(&tree, &live, Some(&only), &scope(), &default_excluded());
    assert!(deleted(&p).is_empty(), "{:?}", deleted(&p));
    assert!(p.unreadable[0].in_ledger);
}

#[test]
fn unreadable_undeclared_unit_is_reported_but_not_a_change() {
    let tree = rendered(&[(A_CFG, "{}"), (A_RES, "{}")]);
    let live = observed_unreadable(
        &[],
        &[(A_CFG, "{}"), (A_RES, "{}"), (B_RES, "{}")],
        vec![unreadable_file("mode", B_CFG, 262)],
    );
    let p = plan_units(&tree, &live);
    assert!(!p.summary.has_changes, "{:?}", p.summary);
    assert_eq!(p.summary.unreadable, 1);
    assert_eq!(p.summary.unmanaged, 0, "never an unmanaged entry either");
    assert!(!p.entries.iter().any(|e| e.key == B_KEY), "{:?}", p.entries);
    let hits: Vec<&String> = p.warnings.iter().filter(|w| w.contains(B_KEY)).collect();
    assert_eq!(hits.len(), 1, "{:?}", p.warnings);
    assert!(hits[0].contains("262 characters"), "{}", hits[0]);
    assert!(hits[0].contains("unreadable"), "{}", hits[0]);

    // the U line keeps the plan distinct from one without the unit
    let without = plan_units(&tree, &observed(&[], &[(A_CFG, "{}"), (A_RES, "{}")]));
    assert!(without.unreadable.is_empty());
    assert_ne!(p.plan_sha256, without.plan_sha256);
}

#[test]
fn unreadable_core_unit_is_wholly_unreadable() {
    // a unit unreadable in core is not planned from its readable mode copy
    let tree = rendered(&[(B_CFG, "{}"), (B_RES, "{}")]);
    let live = observed_unreadable(
        &[(B_RES, "{}")],
        &[(B_CFG, "{}"), (B_RES, "{}")],
        vec![unreadable_file("core", B_CFG, 300)],
    );
    let p = plan_units(&tree, &live);
    assert!(p.entries.is_empty(), "{:?}", p.entries);
    assert_eq!(p.unreadable.len(), 1);
    assert_eq!(p.unreadable[0].live_in, ["core", "mode"]);
    assert_eq!(p.unreadable[0].longest, 300);
    assert!(p.unreadable[0].declared);
}

/// Pinned `plan_sha256` of the `PLAN_GOLDEN` scenario plus one live-only
/// unit whose file the host cannot read (one `U` line).
const PLAN_GOLDEN_UNREADABLE: &str =
    "ef3832a4703a23afa5b7da611ab61662f665d8bd0513355bc1b4bee0bbbc36ae";

#[tokio::test]
async fn plan_sha256_golden_with_unreadable() {
    let fx = PlanFixture::new();
    let (declared, reader) = fx.load("dev");
    fx.live_render("dev", &declared);
    let mut bytes = fx.read("dev", DB_CONFIG);
    bytes.push(b' ');
    fx.mode(DB_CONFIG, &bytes);
    fx.remove("dev", "ignition/general-alarm-settings");
    fx.mode("ignition/database-translator/OLD/config.json", b"{}");
    let reader = MarkUnreadable::new(
        reader,
        "mode",
        "ignition/database-translator/OLD/config.json",
        262,
    );
    let result = deploy_plan(&declared, &reader, &PlanOptions::default())
        .await
        .expect("plan");
    assert_eq!((result.summary.changed, result.summary.added), (1, 1));
    assert_eq!(result.summary.unreadable, 1);
    assert_eq!(
        result.unreadable,
        vec![UnreadableUnit {
            key: "ignition/database-translator/OLD".to_string(),
            live_in: vec!["mode".to_string()],
            declared: false,
            in_ledger: false,
            paths: vec!["ignition/database-translator/OLD/config.json".to_string()],
            longest: 262,
        }]
    );
    assert_eq!(result.plan_sha256, recompute_plan_sha256(&result));
    assert_ne!(
        result.plan_sha256, PLAN_GOLDEN,
        "readable, OLD would be unmanaged and unhashed"
    );
    assert_eq!(
        result.plan_sha256, PLAN_GOLDEN_UNREADABLE,
        "re-golden on purpose only"
    );
}

// ---------------------------------------------------------------------
// quick-261030-8m8: root-level tags (CD5a) and nested type folders (CD5b)
// ---------------------------------------------------------------------

/// A ledgered root-level `tags.json` and nested `A/B/udts.json` under an
/// excluded provider never enter the delete set, with or without the
/// provider in scope, and change no hashed plan line. The same files under a
/// kept provider are deleted like any other tag file (control).
#[test]
fn cd5_excluded_root_ledger_file_never_deleted() {
    const ROOT_MARKER: &str = "ignition/tag-definition/MQTT Engine/unary-resource.json";
    const ROOT_TAGS: &str = "ignition/tag-definition/MQTT Engine/tags.json";
    const NESTED: &str = "ignition/tag-type-definition/MQTT Engine/A/B/udts.json";
    let excluded = vec!["MQTT Engine".to_string(), "System".to_string()];
    let files = [(A_CFG, "{}"), ("ignition/tag-provider/P/config.json", "{}")];
    let tree = rendered(&files);
    let rendered_paths: Vec<&str> = files.iter().map(|(p, _)| *p).collect();
    let mut with_excluded = rendered_paths.clone();
    with_excluded.extend([ROOT_MARKER, ROOT_TAGS, NESTED]);

    let mut wide = scope();
    wide.tag_providers.push("MQTT Engine".to_string());
    let mut live_files = files.to_vec();
    live_files.extend([(ROOT_MARKER, "{}"), (ROOT_TAGS, "[]"), (NESTED, "[]")]);
    for (scope, live) in [
        (scope(), observed(&[], &files)),
        (wide, observed(&[], &live_files)),
    ] {
        let p = plan_with_ledger(
            &tree,
            &live,
            Some(&ledger_of(&with_excluded)),
            &scope,
            &excluded,
        );
        assert!(deleted(&p).is_empty(), "{:?}", deleted(&p));
        assert!(!p.summary.has_changes, "{:?}", p.summary);
        let without = plan_with_ledger(
            &tree,
            &live,
            Some(&ledger_of(&rendered_paths)),
            &scope,
            &excluded,
        );
        assert_eq!(p.plan_sha256, without.plan_sha256, "exclusions not hashed");
    }

    // control: under the kept provider P the same layouts are deleted
    const P_ROOT: &str = "ignition/tag-definition/P/tags.json";
    const P_NESTED: &str = "ignition/tag-type-definition/P/A/B/udts.json";
    let mut ledgered = rendered_paths.clone();
    ledgered.extend([P_ROOT, P_NESTED]);
    let mut live_files = files.to_vec();
    live_files.extend([(P_ROOT, "[]"), (P_NESTED, "[]")]);
    let p = plan_ledger(&tree, &observed(&[], &live_files), &ledgered);
    assert_eq!(deleted(&p), [P_ROOT, P_NESTED]);
    assert!(p.summary.has_changes);
}

// ---- 261030-8m8 review: tag units that lose live tag data (CR-01, CR-02) ----

/// A gateway-format tag marker (`unary-resource.json`, the CD5 captures)
/// listing `files`; `extra` changes the bytes, never the meaning.
fn tag_marker(files: &[&str], extra: &str) -> String {
    let list: Vec<String> = files.iter().map(|f| format!("\"{f}\"")).collect();
    format!(
        "{{\"scope\":\"G\",\"version\":1,\"restricted\":false,\"overridable\":true,\"files\":[{}],\"attributes\":{{\"config\":{{{extra}}}}}}}",
        list.join(",")
    )
}

const ROOT_MARKER: &str = "ignition/tag-definition/P/unary-resource.json";
const ROOT_TAGS: &str = "ignition/tag-definition/P/tags.json";
const ROOT_UDTS: &str = "ignition/tag-definition/P/udts.json";
const AB_MARKER: &str = "ignition/tag-type-definition/P/A/B/unary-resource.json";
const AB_UDTS: &str = "ignition/tag-type-definition/P/A/B/udts.json";

/// CR-01, mode orphan: a first deploy (no ledger) whose provider-root unit
/// lacks a `udts.json` the mode collection holds. The delete set is empty,
/// but the rewritten marker no longer lists the live file, so the gateway
/// drops every root UDT instance (CD5c). The entry is destructive.
#[test]
fn cr01_root_unit_dropping_a_live_member_is_destructive() {
    let only_tags = tag_marker(&["tags.json"], "");
    let both = tag_marker(&["tags.json", "udts.json"], "");
    let want = [(ROOT_MARKER, only_tags.as_str()), (ROOT_TAGS, "[]")];
    let live = [
        (ROOT_MARKER, both.as_str()),
        (ROOT_TAGS, "[]"),
        (ROOT_UDTS, "[]"),
    ];
    let p = plan_units(&rendered(&want), &observed(&[], &live));
    assert!(p.delete_set.files.is_empty(), "no ledger, no deletes");
    let e = entry(&p, "ignition/tag-definition/P");
    assert_eq!(e.status, PlanStatus::Changed);
    assert!(e.destructive, "drops a live udts.json");
    assert_eq!(e.destructive_reason.as_deref(), Some("tag_data_loss"));
    assert!(
        p.warnings
            .iter()
            .any(|w| w.contains("ignition/tag-definition/P") && w.contains("udts.json")),
        "{:?}",
        p.warnings
    );

    // with a ledger that does not list the orphan the answer is the same
    let p = plan_ledger(
        &rendered(&want),
        &observed(&[], &live),
        &[ROOT_MARKER, ROOT_TAGS],
    );
    assert!(p.delete_set.files.is_empty());
    assert!(entry(&p, "ignition/tag-definition/P").destructive);

    // control: the same unit keeping every live member is not destructive
    let want = [
        (ROOT_MARKER, both.as_str()),
        (ROOT_TAGS, "[1]"),
        (ROOT_UDTS, "[]"),
    ];
    let p = plan_units(&rendered(&want), &observed(&[], &live));
    let e = entry(&p, "ignition/tag-definition/P");
    assert_eq!(e.status, PlanStatus::Changed);
    assert!(!e.destructive && e.destructive_reason.is_none());
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
}

/// CR-01, effective copy only: the loss is judged against what the gateway
/// runs now, the effective live copy (the mode copy when one exists, else
/// `core`). A first deploy of a nested type folder over a `core` copy that
/// holds `udts.json` drops those UDT types and is destructive. A later deploy
/// whose mode copy already omits the core `udts.json` loses nothing more: it
/// is `hidden` (informational, CR-02) but not destructive. The same holds for
/// the provider-root unit.
#[test]
fn cr01_judges_the_effective_copy_not_hidden_core() {
    let empty_new = tag_marker(&[], "\"x\":1");
    let empty_old = tag_marker(&[], "");
    let core_marker = tag_marker(&["udts.json"], "");
    let want = [(AB_MARKER, empty_new.as_str())];
    let mode = [(AB_MARKER, empty_old.as_str())];
    let core = [(AB_MARKER, core_marker.as_str()), (AB_UDTS, "[]")];

    // first deploy: the effective copy is core, which holds udts.json
    let p = plan_units(&rendered(&want), &observed(&core, &[]));
    let e = entry(&p, "ignition/tag-type-definition/P/A/B");
    assert_eq!(
        (e.status, e.core),
        (PlanStatus::Changed, CoreRelation::Hidden)
    );
    assert!(e.destructive, "a first deploy drops the core UDT types");
    assert_eq!(e.destructive_reason.as_deref(), Some("tag_data_loss"));
    assert!(
        p.warnings
            .iter()
            .any(|w| w.contains("ignition/tag-type-definition/P/A/B") && w.contains(AB_UDTS)),
        "{:?}",
        p.warnings
    );

    // later deploy: the mode copy already omits the core udts.json
    let p = plan_units(&rendered(&want), &observed(&core, &mode));
    let e = entry(&p, "ignition/tag-type-definition/P/A/B");
    assert_eq!(
        (e.status, e.core),
        (PlanStatus::Changed, CoreRelation::Hidden)
    );
    assert!(!e.destructive, "nothing the gateway runs now is dropped");
    assert!(e.destructive_reason.is_none());
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);

    // a `same` unit is not destructive either
    let p = plan_units(&rendered(&mode), &observed(&core, &mode));
    let e = entry(&p, "ignition/tag-type-definition/P/A/B");
    assert_eq!((e.status, e.core), (PlanStatus::Same, CoreRelation::Hidden));
    assert!(!e.destructive);

    // the provider-root unit: a mode copy that already omits the core
    // udts.json is not destructive; one that still holds it is
    let only_tags = tag_marker(&["tags.json"], "");
    let only_tags_old = tag_marker(&["tags.json"], "\"x\":1");
    let both = tag_marker(&["tags.json", "udts.json"], "");
    let root_core = [
        (ROOT_MARKER, both.as_str()),
        (ROOT_TAGS, "[]"),
        (ROOT_UDTS, "[]"),
    ];
    let root_want = [(ROOT_MARKER, only_tags.as_str()), (ROOT_TAGS, "[1]")];
    let root_mode = [(ROOT_MARKER, only_tags_old.as_str()), (ROOT_TAGS, "[]")];
    let p = plan_units(&rendered(&root_want), &observed(&root_core, &root_mode));
    let e = entry(&p, "ignition/tag-definition/P");
    assert_eq!(
        (e.status, e.core),
        (PlanStatus::Changed, CoreRelation::Hidden)
    );
    assert!(!e.destructive);
    let p = plan_units(&rendered(&root_want), &observed(&root_core, &root_core));
    assert!(entry(&p, "ignition/tag-definition/P").destructive);

    // the same shape under a non-tag type is not destructive
    let rj = ("ignition/database-translator/X/resource.json", "{}");
    let want = [
        rj,
        ("ignition/database-translator/X/config.json", "{\"a\":1}"),
    ];
    let live = [
        rj,
        ("ignition/database-translator/X/config.json", "{}"),
        ("ignition/database-translator/X/extra.json", "{}"),
    ];
    let p = plan_units(&rendered(&want), &observed(&live, &live));
    assert!(!entry(&p, "ignition/database-translator/X").destructive);
}

/// CR-01: a destructive tag entry needs `--allow-destructive` like a
/// `removed` one: the apply gate refuses it without the flag.
#[test]
fn cr01_dropped_member_entry_needs_allow_destructive() {
    use ignition_core::deploy::apply::gates::{GateFlags, GateOutcome, evaluate_gates};
    let only_tags = tag_marker(&["tags.json"], "");
    let both = tag_marker(&["tags.json", "udts.json"], "");
    let want = [(ROOT_MARKER, only_tags.as_str()), (ROOT_TAGS, "[]")];
    let live = [
        (ROOT_MARKER, both.as_str()),
        (ROOT_TAGS, "[]"),
        (ROOT_UDTS, "[]"),
    ];
    let tree = rendered(&want);
    let p = plan_units(&tree, &observed(&[], &live));
    let result = DeployPlanResult {
        env: "dev".to_string(),
        gateway: GATEWAY.to_string(),
        mode: MODE.to_string(),
        node: "master".to_string(),
        transport: "local".to_string(),
        render_sha256: tree.render_sha256(),
        plan_sha256: p.plan_sha256,
        summary: p.summary,
        entries: p.entries,
        delete_set: p.delete_set,
        base: None,
        warnings: p.warnings,
        excluded_tag_providers: default_excluded(),
        unreadable: p.unreadable,
    };
    let err = evaluate_gates(&result, &GateFlags::default()).expect_err("gated");
    assert_eq!(
        (err.code(), err.exit_code()),
        ("destructive_change_refused", 2)
    );
    assert!(
        err.to_string().contains("ignition/tag-definition/P"),
        "{err}"
    );
    let flags = GateFlags {
        allow_destructive: true,
        ..GateFlags::default()
    };
    let GateOutcome::Ready { prose } = evaluate_gates(&result, &flags).expect("allowed") else {
        panic!("expected Ready");
    };
    assert!(
        prose.contains("changed ignition/tag-definition/P [destructive]"),
        "{prose}"
    );
}

/// CR-02: a first deploy of a tag unit over `core` (no mode copy yet) is
/// `hidden` when the rendered unit lacks a `core` file. After the apply the
/// rendered unit is the mode copy and hides `core` by folder (CD5a S4,
/// CD5-CTL), exactly as on a later deploy.
#[test]
fn cr02_first_deploy_over_core_is_hidden() {
    let empty = tag_marker(&[], "");
    let core_marker = tag_marker(&["udts.json"], "");
    let want = [(AB_MARKER, empty.as_str())];
    let core = [(AB_MARKER, core_marker.as_str()), (AB_UDTS, "[]")];
    let p = plan_units(&rendered(&want), &observed(&core, &[]));
    let e = entry(&p, "ignition/tag-type-definition/P/A/B");
    assert_eq!(e.live_in, ["core"]);
    assert_eq!(e.status, PlanStatus::Changed);
    assert_eq!(e.core, CoreRelation::Hidden);
    assert!(e.destructive);
    assert_eq!(e.destructive_reason.as_deref(), Some("tag_data_loss"));
    assert!(
        p.warnings
            .iter()
            .any(|w| w.contains("ignition/tag-type-definition/P/A/B") && w.contains(AB_UDTS)),
        "{:?}",
        p.warnings
    );

    // the provider-root unit without the core udts.json
    let only_tags = tag_marker(&["tags.json"], "");
    let both = tag_marker(&["tags.json", "udts.json"], "");
    let want = [(ROOT_MARKER, only_tags.as_str()), (ROOT_TAGS, "[]")];
    let core = [
        (ROOT_MARKER, both.as_str()),
        (ROOT_TAGS, "[]"),
        (ROOT_UDTS, "[]"),
    ];
    let p = plan_units(&rendered(&want), &observed(&core, &[]));
    let e = entry(&p, "ignition/tag-definition/P");
    assert_eq!(
        (e.status, e.core),
        (PlanStatus::Changed, CoreRelation::Hidden)
    );
    assert!(e.destructive);

    // controls: every core file rendered, bytes differ, is `differs`; a
    // byte-equal render is `same` / `identical`; neither is destructive
    let want = [
        (ROOT_MARKER, both.as_str()),
        (ROOT_TAGS, "[1]"),
        (ROOT_UDTS, "[]"),
    ];
    let p = plan_units(&rendered(&want), &observed(&core, &[]));
    let e = entry(&p, "ignition/tag-definition/P");
    assert_eq!(
        (e.status, e.core),
        (PlanStatus::Changed, CoreRelation::Differs)
    );
    assert!(!e.destructive);
    let p = plan_units(&rendered(&core), &observed(&core, &[]));
    let e = entry(&p, "ignition/tag-definition/P");
    assert_eq!(
        (e.status, e.core),
        (PlanStatus::Same, CoreRelation::Identical)
    );
    assert!(!e.destructive);
}

// ---- 261030-8m8 review: a live tag folder named like a tag file (WR-04) ----

/// A PlanFixture whose repo declares provider `P` with a root-level
/// `tags.json` beside its provider-root marker (CD5a).
fn cd5d_fixture() -> PlanFixture {
    let fx = PlanFixture::new();
    let gw = fx.repo("dev").join("gateways").join(GATEWAY);
    let manifest = gw.join("manifest.yaml");
    let mut yaml = std::fs::read_to_string(&manifest).expect("manifest");
    yaml.push_str("tag_providers:\n  - P\n");
    std::fs::write(&manifest, yaml).expect("write manifest");
    let dir = gw.join("tags/P/tag-definition");
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(
        dir.join("unary-resource.json"),
        tag_marker(&["tags.json"], ""),
    )
    .expect("write marker");
    std::fs::write(dir.join("tags.json"), "[]").expect("write tags");
    fx
}

/// WR-04: the gateway itself creates a tag folder named `tags.json` (CD5d),
/// a directory where the provider-root `tags.json` file belongs. Plan (and
/// so apply, which plans first) refuses fail-closed, naming the live folder,
/// instead of discovering the clash when the commit renames onto it.
#[tokio::test]
async fn wr04_live_member_named_tag_folder_refuses_the_plan() {
    for (collection, put) in [
        ("dev", PlanFixture::mode as fn(&PlanFixture, &str, &[u8])),
        ("core", PlanFixture::core as fn(&PlanFixture, &str, &[u8])),
    ] {
        let fx = cd5d_fixture();
        put(
            &fx,
            "ignition/tag-definition/P/tags.json/unary-resource.json",
            tag_marker(&["tags.json"], "").as_bytes(),
        );
        put(&fx, "ignition/tag-definition/P/tags.json/tags.json", b"[]");
        let (declared, reader) = fx.load("dev");
        let err = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
            .await
            .expect_err("a live CD5d folder is refused");
        assert_eq!(
            (err.code(), err.exit_code()),
            ("deploy_tree_invalid", 2),
            "{collection}: {err}"
        );
        let CoreError::DeployTreeInvalid(details) = &err else {
            panic!("{err:?}");
        };
        let want = format!("config/resources/{collection}/ignition/tag-definition/P/tags.json/");
        let hit = details
            .findings
            .iter()
            .find(|f| f.file == want)
            .unwrap_or_else(|| panic!("no finding on {want}: {:?}", details.findings));
        assert!(hit.message.contains("CD5d"), "{}", hit.message);
        assert_eq!(details.findings.len(), 1, "one per folder");
    }

    // control: a folder whose name only resembles a member file plans
    let fx = cd5d_fixture();
    fx.mode(
        "ignition/tag-definition/P/tags_json/unary-resource.json",
        tag_marker(&["tags.json"], "").as_bytes(),
    );
    fx.mode("ignition/tag-definition/P/tags_json/tags.json", b"[]");
    plan(&fx).await;
}
