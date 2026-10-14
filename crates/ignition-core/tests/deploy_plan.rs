//! `deploy_plan` over the local-filesystem transport (22-01).
//!
//! Every behaviour test drives the full sequence the `deploy plan` verb will
//! use: `load_declared`, `transport::for_node` for `nodes.master`, then
//! `deploy_plan`. No gateway session exists in this file: the plan reads the
//! host's data directory and nothing else.

mod common;

use std::collections::BTreeMap;

use common::plan_fixture::PlanFixture;
use ignition_core::actions::deploy::{DeployPlanResult, deploy_plan, load_declared};
use ignition_core::deploy::layout::RelPath;
use ignition_core::deploy::plan::{CoreRelation, PlanStatus, plan as plan_units};
use ignition_core::deploy::transport::{self, FileBlob, ObservedFiles, READ_MAX_FILE_BYTES};
use ignition_core::deploy::tree::{EMPTY_RENDER_SHA256, RenderedTree, sha256_hex};
use ignition_core::error::CoreError;
use sha2::{Digest, Sha256};

const DB_CONFIG: &str = "ignition/database-translator/POSTGRES/config.json";

async fn plan(fx: &PlanFixture) -> DeployPlanResult {
    let (declared, reader) = fx.load("dev");
    deploy_plan(&declared, reader.as_ref())
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
    let result = deploy_plan(&declared, reader.as_ref()).await.expect("plan");

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

    let result = deploy_plan(&declared, reader.as_ref())
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
    let result = deploy_plan(&declared, reader.as_ref()).await.expect("plan");

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
    let result = deploy_plan(&declared, reader.as_ref()).await.expect("plan");

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
    let result = deploy_plan(&declared, reader.as_ref()).await.expect("plan");

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
        "    local:\n      data_dir: '{}'\n    ssh:\n      host: h\n      user: u\n      os: linux\n      data_dir: /d\n",
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
    let err = deploy_plan(&declared, reader.as_ref())
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
    let err = deploy_plan(&declared, reader.as_ref())
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

fn observed(core: &[(&str, &str)], mode: &[(&str, &str)]) -> ObservedFiles {
    ObservedFiles {
        core: layer(core),
        mode: layer(mode),
        local_units: Vec::new(),
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
    let first = deploy_plan(&declared, reader.as_ref()).await.expect("plan");
    let second = deploy_plan(&declared, reader.as_ref()).await.expect("plan");
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
    let result = deploy_plan(&declared, reader.as_ref()).await.expect("plan");
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
    let moved = deploy_plan(&declared, reader.as_ref()).await.expect("plan");
    assert_ne!(moved.plan_sha256, result.plan_sha256);

    // an unmanaged unit does not
    fx.mode("ignition/database-translator/EXTRA/config.json", b"{}");
    fx.mode("ignition/database-translator/EXTRA/resource.json", b"{}");
    let with_extra = deploy_plan(&declared, reader.as_ref()).await.expect("plan");
    assert_eq!(with_extra.summary.unmanaged, 1);
    assert_eq!(with_extra.plan_sha256, moved.plan_sha256);
    fx.mode(
        "ignition/database-translator/EXTRA/config.json",
        b"{\"x\":1}",
    );
    let edited = deploy_plan(&declared, reader.as_ref()).await.expect("plan");
    assert_eq!(edited.plan_sha256, moved.plan_sha256);
}

#[tokio::test]
async fn local_overrides_warn() {
    let fx = PlanFixture::new();
    let (declared, reader) = fx.load("dev");
    fx.live_render("dev", &declared);
    fx.local("ignition/database-translator/POSTGRES/config.json", b"{}");
    fx.local("ignition/database-translator/POSTGRES/resource.json", b"{}");
    let result = deploy_plan(&declared, reader.as_ref()).await.expect("plan");
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
    let err = deploy_plan(&declared, reader.as_ref())
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
        let err = deploy_plan(&declared, reader.as_ref())
            .await
            .expect_err("a symlink in the live tree is refused");
        assert_eq!(err.code(), "transport_failed");
        assert!(err.to_string().contains("symlink"), "{err}");
    }
}
