//! `deploy_plan` over the local-filesystem transport (22-01).
//!
//! Every behaviour test drives the full sequence the `deploy plan` verb will
//! use: `load_declared`, `transport::for_node` for `nodes.master`, then
//! `deploy_plan`. No gateway session exists in this file: the plan reads the
//! host's data directory and nothing else.

mod common;

use common::plan_fixture::PlanFixture;
use ignition_core::actions::deploy::{DeployPlanResult, deploy_plan, load_declared};
use ignition_core::deploy::plan::PlanStatus;
use ignition_core::deploy::transport;
use ignition_core::error::CoreError;

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
