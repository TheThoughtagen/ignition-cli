//! The apply engine's suite (Phase 23). It starts with the gate layer over a
//! real plan: `load_declared`, the local transport, `deploy_plan`, then
//! `evaluate_gates`.

mod common;

use common::plan_fixture::{GATEWAY, MODE, PlanFixture};
use ignition_core::actions::deploy::{DeployPlanResult, PlanOptions, deploy_plan};
use ignition_core::deploy::apply::gates::{GateFlags, GateOutcome, evaluate_gates};
use ignition_core::error::CoreError;

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
    assert!(matches!(err, CoreError::PlanStale { .. }), "{err:?}");
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

#[tokio::test]
async fn gates_extra_delete_changes_hash_before_destructive() {
    let fx = PlanFixture::new();
    let (declared, reader) = fx.load("dev");
    fx.live_render("dev", &declared);
    let mut bytes = fx.read("dev", DB_CONFIG);
    bytes.push(b' ');
    fx.mode(DB_CONFIG, &bytes);
    let a = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect("plan A");

    // After review the ledger gains a file whose unit is no longer rendered:
    // plan B has an extra destructive entry.
    fx.mode("ignition/database-translator/OLD/config.json", b"{}");
    fx.mode("ignition/database-translator/OLD/resource.json", b"{}");
    fx.ledger(
        GATEWAY,
        MODE,
        &["ignition/database-translator/OLD/config.json"],
    );
    let b = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect("plan B");
    assert_ne!(
        a.plan_sha256, b.plan_sha256,
        "the delete set binds the hash"
    );
    assert!(b.entries.iter().any(|e| e.destructive), "B is destructive");

    let reviewed_a = GateFlags {
        plan_sha256: Some(a.plan_sha256.clone()),
        allow_destructive: true,
        allow_drift: None,
    };
    let err = evaluate_gates(&b, &reviewed_a).expect_err("A's review does not cover B");
    assert_eq!(err.code(), "plan_stale", "{err}");

    // B's own hash with the flag is Ready; without the flag it is refused.
    let reviewed_b = GateFlags {
        plan_sha256: Some(b.plan_sha256.clone()),
        allow_destructive: true,
        allow_drift: None,
    };
    assert!(matches!(
        evaluate_gates(&b, &reviewed_b).expect("B reviewed"),
        GateOutcome::Ready { .. }
    ));
    let no_flag = GateFlags {
        allow_destructive: false,
        ..reviewed_b
    };
    let err = evaluate_gates(&b, &no_flag).expect_err("destructive needs its flag");
    assert_eq!(
        (err.code(), err.exit_code()),
        ("destructive_change_refused", 2)
    );
}

#[tokio::test]
async fn gates_are_deterministic() {
    let plan = changed_plan().await;
    let flags = GateFlags {
        plan_sha256: Some(plan.plan_sha256.clone()),
        allow_destructive: true,
        allow_drift: Some("reviewed on the gateway".to_string()),
    };
    let first = evaluate_gates(&plan, &flags).expect("first");
    let second = evaluate_gates(&plan, &flags).expect("second");
    assert_eq!(first, second);
    let (GateOutcome::Ready { prose: a }, GateOutcome::Ready { prose: b }) = (first, second) else {
        panic!("expected Ready twice");
    };
    assert_eq!(a.as_bytes(), b.as_bytes());
}
