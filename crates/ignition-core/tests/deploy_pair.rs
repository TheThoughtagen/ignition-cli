//! `deploy::pair::BackupNode` over a backup gateway (24-05, DPAIR-01/02/03).
//!
//! The backup is only ever read. The deploy surface under the handle is
//! wrapped in `ForbidWrites`, so a write the handle made would panic here.

mod common;

use common::fake_deploy::FakeGateway;
use common::forbid_writes::ForbidWrites;
use ignition_core::client::ReqwestGatewayApi;
use ignition_core::deploy::pair::{
    BackupCheck, BackupCheckId, BackupNode, BackupReport, BackupSignals, redundancy_check,
};
use ignition_core::deploy::verify::CheckStatus;
use serde_json::{Value, json};

/// 24-LIVE-CAPTURES.md `### C-P4 settled`, "GET redundancy on gw-b (settled)":
/// a settled Cold backup (`uptime` -1, `peerConnected` true, `syncPending`
/// false).
fn settled_gw_b() -> Value {
    json!({
        "role": "Backup", "projectState": "Good", "activityLevel": "Cold",
        "localId": "192.168.156.3", "peerConnected": true,
        "peerId": "ign-pair-a-Master", "hasConfigAccess": true,
        "syncPending": false, "failoverPending": false, "uptime": -1,
        "lastSyncTimestamp": 1792441595954i64
    })
}

/// A wiremock backup answering `GET /data/api/v1/redundancy` with `body`.
async fn backup_gateway(body: Value) -> (wiremock::MockServer, ReqwestGatewayApi) {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/redundancy"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    (server, api)
}

#[tokio::test]
async fn backup_node_reports_a_settled_cold_backup() {
    let (_server, api) = backup_gateway(settled_gw_b()).await;
    let deploy = ForbidWrites(FakeGateway::new().client("backup"));
    let node = BackupNode::new(&api, &deploy, None);

    let now = node.status().await.expect("the backup status reads");
    let before = 1_792_441_000_000i64;
    let signals = BackupSignals::from_status(Some(before), &now);
    assert!(signals.advanced(), "the backup timestamp moved past its own sample");
    assert!(signals.linked());

    let check = redundancy_check(&Ok(signals.clone()), true);
    assert_eq!(check.id, BackupCheckId::Redundancy);
    assert_eq!(check.status, CheckStatus::Pass, "{}", check.detail);

    let report = BackupReport {
        converged: true,
        reason: None,
        budget_s: 300,
        waited_s: 5,
        resync: None,
        last_sync_before: signals.last_sync_before,
        last_sync_after: signals.last_sync_now,
        project_state: signals.project_state.clone(),
        files_checked: 0,
        checks: vec![check],
    };
    let value = serde_json::to_value(&report).expect("serializes");
    let mut keys: Vec<&str> = value
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    // V2: exactly these keys.
    assert_eq!(
        keys,
        [
            "budget_s",
            "checks",
            "converged",
            "files_checked",
            "last_sync_after",
            "last_sync_before",
            "project_state",
            "reason",
            "resync",
            "waited_s"
        ]
    );
    assert_eq!(value["project_state"], "Good");
    assert_eq!(value["last_sync_after"], 1_792_441_595_954i64);
    assert_eq!(value["checks"][0]["id"], "redundancy");
    assert_eq!(value["checks"][0]["status"], "pass");
}

#[tokio::test]
async fn backup_node_never_synced_sample() {
    let (_server, api) = backup_gateway(settled_gw_b()).await;
    let deploy = ForbidWrites(FakeGateway::new().client("backup"));
    let node = BackupNode::new(&api, &deploy, None);
    let now = node.status().await.expect("status");

    // -1 before and a real timestamp now: the backup synced for the first time.
    let first = BackupSignals::from_status(Some(-1), &now);
    assert!(first.advanced());
    assert_eq!(first.last_sync_before, None, "-1 reads as never synced");

    // -1 on both samples: nothing advanced.
    let mut never = now.clone();
    never.last_sync_timestamp = Some(-1);
    let signals = BackupSignals::from_status(Some(-1), &never);
    assert!(!signals.advanced());
    assert_eq!(signals.last_sync_now, None);

    // An unchanged timestamp is not an advance, a smaller one neither.
    let ts = now.last_sync_epoch_ms().expect("a timestamp");
    assert!(!BackupSignals::from_status(Some(ts), &now).advanced());
    assert!(!BackupSignals::from_status(Some(ts + 1), &now).advanced());
}

#[tokio::test]
async fn backup_node_disconnected() {
    let mut body = settled_gw_b();
    body["peerConnected"] = json!(false);
    let (_server, api) = backup_gateway(body).await;
    let deploy = ForbidWrites(FakeGateway::new().client("backup"));
    let node = BackupNode::new(&api, &deploy, None);

    let now = node.status().await.expect("status");
    let signals = BackupSignals::from_status(Some(-1), &now);
    assert!(!signals.linked());
    let check: BackupCheck = redundancy_check(&Ok(signals), true);
    assert_eq!(check.status, CheckStatus::Fail);
    assert!(check.detail.contains("peerConnected"), "{}", check.detail);

    // A status that could not be read is its own failure, named as such.
    let unreachable = redundancy_check(&Err("network_error".to_string()), true);
    assert_eq!(unreachable.status, CheckStatus::Fail);
    assert!(unreachable.detail.starts_with("unreachable: network_error"));
}
