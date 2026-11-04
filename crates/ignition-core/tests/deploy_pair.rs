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

/// The timestamp `logs_gateway` stamps on every entry (epoch ms).
const LOG_AT: i64 = 1_792_441_700_000;

#[tokio::test]
async fn backup_node_reads_the_sync_exhausted_signal_from_its_log() {
    let deploy = ForbidWrites(FakeGateway::new().client("backup"));

    let (_server, api) =
        logs_gateway(&["Active deployment mode differs from peer node!", SPIKE_LINE]).await;
    let node = BackupNode::new(&api, &deploy, None);
    assert!(
        node.sync_exhausted(LOG_AT - 1_000)
            .await
            .expect("the log reads")
    );

    let (_server, api) = logs_gateway(&["Active deployment mode differs from peer node!"]).await;
    let node = BackupNode::new(&api, &deploy, None);
    assert!(
        !node
            .sync_exhausted(LOG_AT - 1_000)
            .await
            .expect("the log reads")
    );
}

#[tokio::test]
async fn backup_node_ignores_a_sync_exhausted_line_older_than_the_anchor() {
    // WR-01: the line counts only when it was logged at or after the moment
    // the engine anchored this run. An older line (a previous deploy, a
    // recovery that left it in the log) is not the gateway giving up now.
    let deploy = ForbidWrites(FakeGateway::new().client("backup"));
    let (_server, api) = logs_gateway(&[SPIKE_LINE]).await;
    let node = BackupNode::new(&api, &deploy, None);

    assert!(
        !node
            .sync_exhausted(LOG_AT + 1)
            .await
            .expect("the log reads"),
        "a line one millisecond before the anchor is stale"
    );
    assert!(
        node.sync_exhausted(LOG_AT).await.expect("the log reads"),
        "a line stamped exactly at the anchor counts"
    );
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

// ---- quick 261102-cef (A2, O2): a routed verify on a pair -------------------

use common::plan_fixture::{MODE, PlanFixture};
use ignition_core::actions::deploy::{
    Declared, DeployVerifyResult, VerifyNode, VerifyOptions, deploy_verify, deploy_verify_node,
};
use ignition_core::deploy::transport::TreeReader;
use ignition_core::deploy::transport::local::LocalFsTransport;

const CANARY_TAGS: &str = "ignition/tag-definition/default/Deploy/tags.json";
const CANARY_MARKER: &str = "ignition/tag-definition/default/Deploy/unary-resource.json";
const CANARY_TAGS_BYTES: &str = r#"[{"valueSource":"memory","dataType":"String","name":"Canary","defaultValue":"ign","tagType":"AtomicTag"}]"#;
const CANARY_MARKER_BYTES: &str = r#"{"scope":"G","version":1,"restricted":false,"overridable":true,"files":["tags.json"],"attributes":{"config":{}}}"#;

/// The tracer repo with tag provider `default` and the canary unit declared.
fn canary_repo() -> PlanFixture {
    let fx = PlanFixture::new();
    let gw = fx.repo("dev").join("gateways").join("tracer-gw");
    let manifest = gw.join("manifest.yaml");
    let mut yaml = std::fs::read_to_string(&manifest).expect("manifest");
    yaml.push_str("tag_providers:\n  - default\n");
    std::fs::write(&manifest, yaml).expect("write manifest");
    let dir = gw.join("tags/default/tag-definition/Deploy");
    std::fs::create_dir_all(&dir).expect("tag dir");
    std::fs::write(dir.join("tags.json"), CANARY_TAGS_BYTES).expect("tags.json");
    std::fs::write(dir.join("unary-resource.json"), CANARY_MARKER_BYTES).expect("marker");
    fx
}

/// Every rendered file but the canary's into `fx`'s mode collection, the
/// canary into its `core`.
fn live_with_canary_in_core(fx: &PlanFixture, declared: &Declared) {
    for (path, bytes) in &declared.outcome.tree.files {
        if path.as_str().starts_with("ignition/tag-definition/") {
            fx.core(path.as_str(), bytes);
        } else {
            fx.mode(path.as_str(), bytes);
        }
    }
}

fn make_pair(declared: &mut Declared) {
    let mut backup = declared.target.nodes.master.clone();
    backup.profile = "backup".to_string();
    backup.local = None;
    backup.gateway_name = None;
    declared.target.nodes.backup = Some(backup);
}

fn fast() -> VerifyOptions {
    VerifyOptions {
        probe_deadline: std::time::Duration::from_millis(400),
        probe_interval: std::time::Duration::from_millis(20),
        log_settle: std::time::Duration::ZERO,
    }
}

/// A master over `fx` and a backup over its own data directory holding the
/// same mode files and the canary in `core` with the marker pretty-printed,
/// as gw-b writes it (debug Evidence C1).
struct VerifyPair {
    master: FakeGateway,
    master_api: ReqwestGatewayApi,
    backup: FakeGateway,
    backup_api: ReqwestGatewayApi,
    backup_reader: LocalFsTransport,
    _backup_dir: tempfile::TempDir,
    _servers: Vec<wiremock::MockServer>,
}

async fn verify_pair(fx: &PlanFixture) -> VerifyPair {
    let backup_dir = tempfile::tempdir().expect("backup data dir");
    let copy = |from: &std::path::Path, to: &std::path::Path| {
        fn walk(from: &std::path::Path, to: &std::path::Path) {
            std::fs::create_dir_all(to).expect("dir");
            for entry in std::fs::read_dir(from).expect("read").flatten() {
                let target = to.join(entry.file_name());
                if entry.path().is_dir() {
                    walk(&entry.path(), &target);
                } else {
                    std::fs::copy(entry.path(), target).expect("copy");
                }
            }
        }
        walk(from, to);
    };
    copy(
        &fx.data_dir().join("config/resources"),
        &backup_dir.path().join("config/resources"),
    );
    let marker = backup_dir
        .path()
        .join("config/resources/core")
        .join(CANARY_MARKER);
    if marker.exists() {
        let doc: Value = serde_json::from_slice(&std::fs::read(&marker).expect("marker")).unwrap();
        std::fs::write(&marker, serde_json::to_vec_pretty(&doc).unwrap()).expect("pretty");
    }
    let master_server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/logs"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "items": [], "metadata": {"total": 0, "matching": 0, "limit": 200, "offset": 0}
        })))
        .mount(&master_server)
        .await;
    let (backup_server, backup_api) = backup_gateway(settled_gw_b()).await;
    VerifyPair {
        master: FakeGateway::with_data_dir(fx.data_dir(), MODE),
        master_api: ReqwestGatewayApi::for_tests(&master_server.uri(), None),
        backup: FakeGateway::with_data_dir(backup_dir.path(), MODE),
        backup_api,
        backup_reader: LocalFsTransport::new(backup_dir.path().to_path_buf(), "nodes.backup"),
        _backup_dir: backup_dir,
        _servers: vec![master_server, backup_server],
    }
}

async fn verify_both(
    declared: &Declared,
    reader: &dyn TreeReader,
    pair: &VerifyPair,
) -> DeployVerifyResult {
    let master_deploy = ForbidWrites(pair.master.client("verify"));
    let backup_deploy = ForbidWrites(pair.backup.client("verify-backup"));
    let backup = BackupNode::new(&pair.backup_api, &backup_deploy, Some(&pair.backup_reader));
    deploy_verify_node(
        declared,
        "ign-cli",
        VerifyNode::Both,
        Some((&master_deploy, &pair.master_api)),
        reader,
        Some(&backup),
        &fast(),
    )
    .await
    .expect("verdict")
}

fn check_status(result: &DeployVerifyResult, id: &str) -> Value {
    let checks = serde_json::to_value(&result.checks).expect("checks");
    checks
        .as_array()
        .expect("array")
        .iter()
        .find(|c| c["id"] == id)
        .map(|c| c["status"].clone())
        .unwrap_or(Value::Null)
}

fn check_ids(result: &DeployVerifyResult) -> Vec<String> {
    let checks = serde_json::to_value(&result.checks).expect("checks");
    checks
        .as_array()
        .expect("array")
        .iter()
        .map(|c| c["id"].as_str().expect("id").to_string())
        .collect()
}

fn pair_warnings(result: &DeployVerifyResult) -> Vec<&String> {
    result
        .warnings
        .iter()
        .filter(|w| w.starts_with("redundant pair:"))
        .collect()
}

/// A pair whose tag unit lives in core, byte-equal to the repo: verify on
/// the master and `--node both` are green, the master `files` check judges
/// core, the backup's `files` check passes over its pretty-printed marker,
/// and there is no pair warning.
#[tokio::test]
async fn pair_verify_core_units_green() {
    let fx = canary_repo();
    let (mut declared, reader) = fx.load("dev");
    live_with_canary_in_core(&fx, &declared);
    make_pair(&mut declared);
    let pair = verify_pair(&fx).await;
    let master_deploy = ForbidWrites(pair.master.client("verify"));
    let master = deploy_verify(
        &declared,
        "ign-cli",
        &master_deploy,
        &pair.master_api,
        reader.as_ref(),
        &fast(),
    )
    .await
    .expect("verdict");
    assert!(master.ok, "{master:#?}");
    assert_eq!(check_status(&master, "files"), "pass");
    assert!(pair_warnings(&master).is_empty(), "{:?}", master.warnings);

    let both = verify_both(&declared, reader.as_ref(), &pair).await;
    assert!(both.ok, "{both:#?}");
    let backup = both.backup.as_ref().expect("backup report");
    let files = backup
        .checks
        .iter()
        .find(|c| c.id == BackupCheckId::Files)
        .expect("files check");
    assert_eq!(files.status, CheckStatus::Pass, "{}", files.detail);
    assert!(pair_warnings(&both).is_empty(), "{:?}", both.warnings);
}

/// O2: the same pair plus a copy of an undeclared tag unit in the master's
/// mode collection (no ledger lists it). Verify's checks, their ids and its
/// exit are what they are without it; `data.warnings` holds exactly one line
/// naming the mode file paths, once under `--node both` too. (Since quick
/// 261103-ell, F2, a mode copy of a declared unit fails `files` instead.)
#[tokio::test]
async fn pair_verify_warns_on_mode_tag_files() {
    let fx = canary_repo();
    let (mut declared, reader) = fx.load("dev");
    live_with_canary_in_core(&fx, &declared);
    make_pair(&mut declared);
    let clean = {
        let pair = verify_pair(&fx).await;
        verify_both(&declared, reader.as_ref(), &pair).await
    };
    fx.mode(OTHER_TAGS, CANARY_TAGS_BYTES.as_bytes());
    fx.mode(OTHER_MARKER, CANARY_MARKER_BYTES.as_bytes());
    let pair = verify_pair(&fx).await;
    let master_deploy = ForbidWrites(pair.master.client("verify"));
    let master = deploy_verify(
        &declared,
        "ign-cli",
        &master_deploy,
        &pair.master_api,
        reader.as_ref(),
        &fast(),
    )
    .await
    .expect("verdict");
    assert_eq!(master.ok, clean.ok, "{master:#?}");
    assert_eq!(check_ids(&master), check_ids(&clean));
    assert_eq!(check_status(&master, "files"), "pass");
    let hits = pair_warnings(&master);
    assert_eq!(hits.len(), 1, "{:?}", master.warnings);
    assert!(
        hits[0].contains(OTHER_TAGS) && hits[0].contains(OTHER_MARKER),
        "{}",
        hits[0]
    );
    assert!(
        hits[0].contains(&format!("{MODE} collection")),
        "{}",
        hits[0]
    );

    let both = verify_both(&declared, reader.as_ref(), &pair).await;
    assert_eq!(both.ok, clean.ok, "{both:#?}");
    assert_eq!(check_ids(&both), check_ids(&clean));
    assert_eq!(pair_warnings(&both).len(), 1, "{:?}", both.warnings);
}

/// An undeclared tag unit's files, for the mode-copy warning of
/// [`pair_verify_warns_on_mode_tag_files`].
const OTHER_TAGS: &str = "ignition/tag-definition/default/Other/tags.json";
const OTHER_MARKER: &str = "ignition/tag-definition/default/Other/unary-resource.json";
/// The canary's unit key.
const CANARY_UNIT: &str = "ignition/tag-definition/default/Deploy";

/// The `files` check's findings of a verify result.
fn files_findings(result: &DeployVerifyResult) -> Vec<Value> {
    let checks = serde_json::to_value(&result.checks).expect("checks");
    checks
        .as_array()
        .expect("array")
        .iter()
        .find(|c| c["id"] == "files")
        .and_then(|c| c["findings"].as_array().cloned())
        .unwrap_or_default()
}

/// The backup checks of a `--node both` result: (id, status) pairs.
fn backup_check_states(result: &DeployVerifyResult) -> Vec<(BackupCheckId, CheckStatus)> {
    result
        .backup
        .as_ref()
        .expect("backup report")
        .checks
        .iter()
        .map(|c| (c.id, c.status))
        .collect()
}

/// quick 261103-ell, F2: the pair of [`pair_verify_core_units_green`] plus a
/// copy of the declared canary in the master's mode collection. The mode copy
/// overrides `core` and keeps the backup from syncing, so the master `files`
/// check fails with one `files_differ` finding for the canary naming both
/// mode paths; the check ids are unchanged, and the pair warning no longer
/// names those paths. `--node both` fails the master `files` check the same
/// way and leaves the backup checks as they were.
#[tokio::test]
async fn pair_verify_fails_on_mode_copy_of_declared_unit() {
    let fx = canary_repo();
    let (mut declared, reader) = fx.load("dev");
    live_with_canary_in_core(&fx, &declared);
    make_pair(&mut declared);
    let clean = {
        let pair = verify_pair(&fx).await;
        verify_both(&declared, reader.as_ref(), &pair).await
    };
    assert!(clean.ok, "{clean:#?}");
    fx.mode(CANARY_TAGS, CANARY_TAGS_BYTES.as_bytes());
    fx.mode(CANARY_MARKER, CANARY_MARKER_BYTES.as_bytes());
    let pair = verify_pair(&fx).await;
    let master_deploy = ForbidWrites(pair.master.client("verify"));
    let master = deploy_verify(
        &declared,
        "ign-cli",
        &master_deploy,
        &pair.master_api,
        reader.as_ref(),
        &fast(),
    )
    .await
    .expect("verdict");
    assert!(!master.ok, "{master:#?}");
    assert_eq!(check_ids(&master), check_ids(&clean));
    assert_eq!(check_status(&master, "files"), "fail");
    let findings = files_findings(&master);
    assert_eq!(findings.len(), 1, "{findings:#?}");
    assert_eq!(findings[0]["code"], "files_differ", "{findings:#?}");
    assert_eq!(findings[0]["subject"], CANARY_UNIT, "{findings:#?}");
    let detail = findings[0]["detail"].as_str().expect("detail");
    assert!(
        detail.contains(CANARY_TAGS) && detail.contains(CANARY_MARKER),
        "{detail}"
    );
    assert!(detail.contains(&format!("{MODE} collection")), "{detail}");
    for warning in pair_warnings(&master) {
        assert!(
            !warning.contains(CANARY_TAGS) && !warning.contains(CANARY_MARKER),
            "{warning}"
        );
    }

    let both = verify_both(&declared, reader.as_ref(), &pair).await;
    assert!(!both.ok, "{both:#?}");
    assert_eq!(check_ids(&both), check_ids(&clean));
    assert_eq!(check_status(&both, "files"), "fail");
    assert_eq!(files_findings(&both), findings);
    assert_eq!(backup_check_states(&both), backup_check_states(&clean));
    for warning in pair_warnings(&both) {
        assert!(
            !warning.contains(CANARY_TAGS) && !warning.contains(CANARY_MARKER),
            "{warning}"
        );
    }
}

/// quick 261103-ell, F3: an empty `ignition/tag-type-definition` folder in
/// the master's mode collection (no file under it, no mode tag file at all)
/// alone keeps the backup from syncing (debug T6). Verify's checks, their ids
/// and its exit are what they are without it; `data.warnings` holds exactly
/// one pair line naming the folder, once under `--node both` too.
#[tokio::test]
async fn pair_verify_warns_on_empty_mode_type_folder() {
    const FOLDER: &str = "ignition/tag-type-definition";
    let fx = canary_repo();
    let (mut declared, reader) = fx.load("dev");
    live_with_canary_in_core(&fx, &declared);
    make_pair(&mut declared);
    let clean = {
        let pair = verify_pair(&fx).await;
        verify_both(&declared, reader.as_ref(), &pair).await
    };
    assert!(pair_warnings(&clean).is_empty(), "{:?}", clean.warnings);
    std::fs::create_dir_all(fx.collection_dir(MODE).join(FOLDER)).expect("empty folder");
    let pair = verify_pair(&fx).await;
    let master_deploy = ForbidWrites(pair.master.client("verify"));
    let master = deploy_verify(
        &declared,
        "ign-cli",
        &master_deploy,
        &pair.master_api,
        reader.as_ref(),
        &fast(),
    )
    .await
    .expect("verdict");
    assert_eq!(master.ok, clean.ok, "{master:#?}");
    assert_eq!(check_ids(&master), check_ids(&clean));
    let hits = pair_warnings(&master);
    assert_eq!(hits.len(), 1, "{:?}", master.warnings);
    assert!(hits[0].contains(FOLDER), "{}", hits[0]);
    assert!(
        hits[0].contains(&format!("{MODE} collection")),
        "{}",
        hits[0]
    );

    let both = verify_both(&declared, reader.as_ref(), &pair).await;
    assert_eq!(both.ok, clean.ok, "{both:#?}");
    assert_eq!(check_ids(&both), check_ids(&clean));
    assert_eq!(pair_warnings(&both).len(), 1, "{:?}", both.warnings);
}

/// quick 261104-7cy, C3: the pair of [`pair_verify_warns_on_mode_tag_files`]
/// plus content of the mode `ignition/tag-definition` folder the plan does not
/// read, beside the files it reads: an empty `default/Empty` and an
/// undeclared provider's `Elsewhere/tags.json`. Verify's checks, their ids and
/// its exit are those of the clean pair and `files` passes; the one pair
/// warning names the undeclared unit's files and the topmost unread paths,
/// once under `--node both` too.
#[tokio::test]
async fn pair_verify_warns_on_unseen_content() {
    const EMPTY: &str = "ignition/tag-definition/default/Empty";
    const ELSEWHERE: &str = "ignition/tag-definition/Elsewhere";
    let fx = canary_repo();
    let (mut declared, reader) = fx.load("dev");
    live_with_canary_in_core(&fx, &declared);
    make_pair(&mut declared);
    let clean = {
        let pair = verify_pair(&fx).await;
        verify_both(&declared, reader.as_ref(), &pair).await
    };
    fx.mode(OTHER_TAGS, CANARY_TAGS_BYTES.as_bytes());
    fx.mode(OTHER_MARKER, CANARY_MARKER_BYTES.as_bytes());
    std::fs::create_dir_all(fx.collection_dir(MODE).join(EMPTY)).expect("empty folder");
    fx.mode(&format!("{ELSEWHERE}/tags.json"), b"{}");
    let pair = verify_pair(&fx).await;
    let master_deploy = ForbidWrites(pair.master.client("verify"));
    let master = deploy_verify(
        &declared,
        "ign-cli",
        &master_deploy,
        &pair.master_api,
        reader.as_ref(),
        &fast(),
    )
    .await
    .expect("verdict");
    assert_eq!(master.ok, clean.ok, "{master:#?}");
    assert_eq!(check_ids(&master), check_ids(&clean));
    assert_eq!(check_status(&master, "files"), "pass");
    let hits = pair_warnings(&master);
    assert_eq!(hits.len(), 1, "{:?}", master.warnings);
    for path in [OTHER_TAGS, OTHER_MARKER, EMPTY, ELSEWHERE] {
        assert!(hits[0].contains(path), "{path}: {}", hits[0]);
    }

    let both = verify_both(&declared, reader.as_ref(), &pair).await;
    assert_eq!(both.ok, clean.ok, "{both:#?}");
    assert_eq!(check_ids(&both), check_ids(&clean));
    assert_eq!(pair_warnings(&both).len(), 1, "{:?}", both.warnings);
}

/// A standalone target with its tag unit in the mode collection verifies as
/// before: the unit is judged there, and there is no pair warning.
#[tokio::test]
async fn standalone_verify_no_pair_warning() {
    let fx = canary_repo();
    let (declared, reader) = fx.load("dev");
    fx.live_render(MODE, &declared);
    let pair = verify_pair(&fx).await;
    let master_deploy = ForbidWrites(pair.master.client("verify"));
    let result = deploy_verify(
        &declared,
        "ign-cli",
        &master_deploy,
        &pair.master_api,
        reader.as_ref(),
        &fast(),
    )
    .await
    .expect("verdict");
    assert!(result.ok, "{result:#?}");
    assert_eq!(check_status(&result, "files"), "pass");
    assert!(pair_warnings(&result).is_empty(), "{:?}", result.warnings);
}

// ---- quick 261104-7cy (C4): a pair's diff of a routed unit's mode copy ------

use ignition_core::actions::deploy::{PlanOptions, deploy_plan};
use ignition_core::deploy::plan::PlanStatus;

/// A pair whose canary unit has an older copy in the mode collection and
/// none in `core`: `deploy plan --diff` shows the `.core/` row adding the
/// repo bytes and the mode copy's row removing its live lines. The repo
/// bytes appear once, on a `+` line of the `.core/` section, and the mode
/// copy's section has no `+` line (quick 261104-7cy, C4).
#[tokio::test]
async fn pair_plan_diff_shows_mode_copy_as_removal() {
    let fx = canary_repo();
    let (mut declared, reader) = fx.load("dev");
    for (path, bytes) in &declared.outcome.tree.files {
        if path.as_str() == CANARY_TAGS {
            let old =
                CANARY_TAGS_BYTES.replace(r#""defaultValue":"ign""#, r#""defaultValue":"old""#);
            fx.mode(CANARY_TAGS, old.as_bytes());
        } else {
            fx.mode(path.as_str(), bytes);
        }
    }
    make_pair(&mut declared);
    let opts = PlanOptions {
        diff: true,
        ..Default::default()
    };
    let result = deploy_plan(&declared, reader.as_ref(), &opts)
        .await
        .expect("plan");
    let entry = result
        .entries
        .iter()
        .find(|e| e.key == CANARY_UNIT)
        .expect("canary entry");
    assert_eq!(entry.status, PlanStatus::Added, "{entry:#?}");
    let diff = entry.diff.as_deref().expect("a diff");
    let body: Vec<&str> = diff
        .lines()
        .filter(|l| !l.starts_with("+++") && !l.starts_with("---"))
        .collect();
    let count = |sign: char, needle: &str| {
        body.iter()
            .filter(|l| l.starts_with(sign) && l.contains(needle))
            .count()
    };
    assert_eq!(count('+', r#""defaultValue":"ign""#), 1, "{diff}");
    assert_eq!(count('-', r#""defaultValue":"old""#), 1, "{diff}");
    assert_eq!(count('+', r#""defaultValue":"old""#), 0, "{diff}");
}
