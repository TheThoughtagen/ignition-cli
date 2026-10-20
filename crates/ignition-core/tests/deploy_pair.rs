//! `deploy::pair::BackupNode` over a backup gateway (24-05, DPAIR-01/02/03).
//!
//! The backup is only ever read. The deploy surface under the handle is
//! wrapped in `ForbidWrites`, so a write the handle made would panic here.

mod common;

use common::fake_deploy::FakeGateway;
use common::forbid_writes::ForbidWrites;
use ignition_core::client::ReqwestGatewayApi;
use ignition_core::client::deploy_api::SecretNameListing;
use ignition_core::deploy::pair::{
    BackupCheck, BackupCheckId, BackupNode, BackupReport, BackupSignals, redundancy_check,
};
use ignition_core::deploy::transport::Scope;
use ignition_core::deploy::verify::{CheckStatus, Expected};
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
    assert!(
        signals.advanced(),
        "the backup timestamp moved past its own sample"
    );
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

/// 24-LIVE-CAPTURES.md `## C-P8` (not reached on the rig, K5 d) and the spike
/// `results/RIG.md` line: the message text the exhausted state logs, with a
/// canary appended to prove it never leaves the handle.
const SPIKE_LINE: &str = "[config-resource] State sync request failed. Sync operation has failed 5 time(s), which exceeds the max attempt count. No further synchronization attempts will be made. redundant-provider=config-resource PAIR-CANARY-TEXT";

async fn logs_gateway(messages: &[&str]) -> (wiremock::MockServer, ReqwestGatewayApi) {
    let items: Vec<Value> = messages
        .iter()
        .map(|m| {
            json!({"timestamp": 1_792_441_700_000i64, "loggerName": "R.S.SyncManager",
                   "level": "ERROR", "message": m})
        })
        .collect();
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/logs"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "items": items,
            "metadata": {"total": items.len(), "matching": items.len(), "limit": 200, "offset": 0}
        })))
        .mount(&server)
        .await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    (server, api)
}

#[tokio::test]
async fn backup_node_reads_the_sync_exhausted_signal_from_its_log() {
    let deploy = ForbidWrites(FakeGateway::new().client("backup"));

    let (_server, api) =
        logs_gateway(&["Active deployment mode differs from peer node!", SPIKE_LINE]).await;
    let node = BackupNode::new(&api, &deploy, None);
    assert!(node.sync_exhausted().await.expect("the log reads"));

    let (_server, api) = logs_gateway(&["Active deployment mode differs from peer node!"]).await;
    let node = BackupNode::new(&api, &deploy, None);
    assert!(!node.sync_exhausted().await.expect("the log reads"));
}

#[tokio::test]
async fn backup_node_reads_mode_secret_names_and_read_back_through_its_own_surface() {
    let (_server, api) = backup_gateway(settled_gw_b()).await;
    let fake = FakeGateway::new();
    fake.set_deployment_mode(Some("production".to_string()));
    fake.seed_secret_names("file", &["CAP24-Name", "cap24-name"]);
    fake.secret_provider_cannot_list("remote");
    let deploy = ForbidWrites(fake.client("backup"));
    let node = BackupNode::new(&api, &deploy, None);

    // normalized like every other mode read: a name verbatim, no mode as None
    assert_eq!(
        node.deployment_mode().await.expect("mode").as_deref(),
        Some("production")
    );
    fake.set_deployment_mode(Some("   ".to_string()));
    assert_eq!(node.deployment_mode().await.expect("mode"), None);
    fake.set_deployment_mode(None);
    assert_eq!(node.deployment_mode().await.expect("mode"), None);

    // names are exact strings, two cases are two names (K9)
    match node.secret_names("file").await.expect("listing") {
        SecretNameListing::Listed(names) => {
            assert_eq!(names.len(), 2);
            assert!(names.contains("CAP24-Name") && names.contains("cap24-name"));
        }
        other => panic!("expected a listing, got {other:?}"),
    }
    assert_eq!(
        node.secret_names("remote").await.expect("listing"),
        SecretNameListing::CannotList
    );
    assert_eq!(
        node.secret_names("nope").await.expect("listing"),
        SecretNameListing::NoProvider
    );

    // nothing declared, nothing read back
    let expected = Expected {
        mode: "production".to_string(),
        resources: Vec::new(),
        not_checked: Vec::new(),
        declared_ids: Vec::new(),
        tag_units: Vec::new(),
        probes: Vec::new(),
    };
    assert!(
        node.read_back(&expected)
            .await
            .expect("read back")
            .is_empty()
    );
}

#[tokio::test]
async fn backup_node_without_a_transport_has_no_files() {
    let (_server, api) = backup_gateway(settled_gw_b()).await;
    let deploy = ForbidWrites(FakeGateway::new().client("backup"));
    let node = BackupNode::new(&api, &deploy, None);
    assert!(!node.has_files());
    let scope = Scope {
        mode: "production".to_string(),
        types: Vec::new(),
        tag_providers: Vec::new(),
    };
    assert!(
        node.read_files(&scope).await.is_none(),
        "REST-only: no file read"
    );
}
