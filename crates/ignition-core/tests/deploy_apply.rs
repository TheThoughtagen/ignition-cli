//! The apply engine's suite (Phase 23). It starts with the gate layer over a
//! real plan: `load_declared`, the local transport, `deploy_plan`, then
//! `evaluate_gates`.

mod common;

use common::plan_fixture::PlanFixture;
use ignition_core::actions::deploy::{DeployPlanResult, PlanOptions, deploy_plan};
use ignition_core::deploy::apply::gates::{GateFlags, GateOutcome, evaluate_gates};

const DB_CONFIG: &str = "ignition/database-translator/POSTGRES/config.json";
const DB_UNIT: &str = "ignition/database-translator/POSTGRES";

/// A plan with exactly one changed unit.
async fn changed_plan() -> DeployPlanResult {
    let fx = PlanFixture::new();
    let (declared, reader) = fx.load("dev");
    fx.live_render("dev", &declared);
    let mut bytes = fx.read("dev", DB_CONFIG);
    bytes.push(b' ');
    fx.mode(DB_CONFIG, &bytes);
    deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect("plan")
}

fn with_hash(hash: &str) -> GateFlags {
    GateFlags {
        plan_sha256: Some(hash.to_string()),
        ..GateFlags::default()
    }
}

#[tokio::test]
async fn gates_stale_hash_refuses_over_a_real_plan() {
    let plan = changed_plan().await;
    let zeros = "0".repeat(64);
    let err = evaluate_gates(&plan, &with_hash(&zeros)).expect_err("a stale hash refuses");
    assert_eq!(err.code(), "plan_stale", "{err}");
    assert_eq!(err.exit_code(), 6);
    let shown = err.to_string();
    assert!(shown.contains(&zeros), "expected hash in {shown}");
    assert!(shown.contains(&plan.plan_sha256), "actual hash in {shown}");
}

#[tokio::test]
async fn gates_matching_hash_is_ready() {
    let plan = changed_plan().await;
    let outcome = evaluate_gates(&plan, &with_hash(&plan.plan_sha256)).expect("matching hash");
    let GateOutcome::Ready { prose } = outcome else {
        panic!("expected Ready, got {outcome:?}");
    };
    assert!(prose.contains("plan_sha256"), "{prose}");
    assert!(prose.contains(&plan.plan_sha256), "{prose}");
    assert!(prose.contains(DB_UNIT), "{prose}");
    assert!(
        !prose.contains("SERIAL NOT NULL"),
        "prose carries no file bytes: {prose}"
    );
}

#[tokio::test]
async fn gates_without_hash_is_ready() {
    let plan = changed_plan().await;
    let outcome = evaluate_gates(&plan, &GateFlags::default()).expect("no hash is the CLI's rule");
    assert!(matches!(outcome, GateOutcome::Ready { .. }), "{outcome:?}");
}
