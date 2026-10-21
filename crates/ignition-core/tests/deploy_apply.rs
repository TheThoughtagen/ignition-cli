//! The apply engine's suite (Phase 23). It starts with the gate layer over a
//! real plan (`load_declared`, the local transport, `deploy_plan`, then
//! `evaluate_gates`) and goes on to the engine's green path, its no-op and
//! the branches 23-17 will turn into restores, over a local node and the
//! scan-coupled fake gateway.

mod common;

use std::collections::BTreeMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::fake_deploy::{FakeGateway, PairLink};
use common::faulty_deploy::{DeployFaults, FaultyDeploy};
use common::faulty_host::FaultyHost;
use common::forbid_writes::ForbidWrites;
use common::plan_fixture::{GATEWAY, MODE, PlanFixture};
use ignition_core::actions::deploy::{
    Declared, DeployPlanResult, PlanOptions, VerifyOptions, deploy_plan, load_target,
};
use ignition_core::actions::deploy_apply::{
    ApplyPrepared, DeployApplyResult, DeployRollbackResult, PrepareOptions, RollbackPrepareOptions,
    RollbackPrepared, deploy_apply_execute, deploy_apply_prepare, deploy_rollback_execute,
    deploy_rollback_prepare,
};
use ignition_core::client::scan::ScanKind;
use ignition_core::client::{DeployApi, ReqwestGatewayApi};
use ignition_core::deploy::apply::fingerprint::{LEDGER_KEY, fingerprint};
use ignition_core::deploy::apply::gates::{GateFlags, GateOutcome, evaluate_gates};
use ignition_core::deploy::apply::hold::{Clock, HoldModel, ManualClock, SystemClock};
use ignition_core::deploy::apply::host::{
    ApplyHost, BatchReport, BatchSpec, Prepared, ProbeReport,
};
use ignition_core::deploy::apply::journal::{Journal, JournalPhase};
use ignition_core::deploy::apply::preflight::{
    DesignersMode, DesignersPolicy, PreflightInput, PreflightReport, run_preflights,
};
use ignition_core::deploy::apply::record::{RecordState, RunRecord};
use ignition_core::deploy::apply::run_store::RunId;
use ignition_core::deploy::apply::{
    ApplyContext, ApplyEvent, ApplyObserver, ApplyOptions, NoObserver, OutcomeKind, RollbackCause,
    RollbackContext, RollbackKind, RollbackOptions, Steps, write_set,
};
use ignition_core::deploy::ledger::Ledger;
use ignition_core::deploy::pair::{BackupNode, BackupPolicy, UnsyncedReason};
use ignition_core::deploy::target::Target;
use ignition_core::deploy::transport::local::{ApplyFault, LocalFsTransport};
use ignition_core::deploy::transport::{self, TreeReader};
use ignition_core::deploy::tree::{RenderedTree, sha256_hex};
use ignition_core::error::CoreError;
use ignition_core::poll::PollConfig;
use serde_json::{Value, json};

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

// ============================================================================
// The engine over a local node (23-05)
// ============================================================================

const DB_RESOURCE: &str = "ignition/database-translator/POSTGRES/resource.json";
const ALARM_UNIT: &str = "ignition/general-alarm-settings";
const ALARM_CONFIG: &str = "ignition/general-alarm-settings/config.json";
const ALARM_RESOURCE: &str = "ignition/general-alarm-settings/resource.json";
const HOLD_S: u64 = 60;

/// Everything one engine run needs: the tracer repo over a local data
/// directory, a fake gateway coupled to that directory, a wiremock for the
/// log source and a local apply host behind a call recorder.
struct Rig {
    fx: PlanFixture,
    declared: Declared,
    reader: Box<dyn TreeReader>,
    repo: PathBuf,
    fake: FakeGateway,
    api: ReqwestGatewayApi,
    host: FaultyHost<LocalFsTransport>,
    /// Gateway calls to fail once armed (review fix CR-01); none by default.
    faults: DeployFaults,
    /// The backup node's deploy surface, when the test gives the target one.
    backup: Option<Box<dyn DeployApi>>,
    /// The backup's own gateway surface: an address nothing listens on, so a
    /// call the backup handle should never make fails loudly and never
    /// reaches the master's server.
    backup_api: Option<ReqwestGatewayApi>,
    /// The server behind `backup_api` for a backup given by [`with_backup`]: it
    /// answers the backup's redundancy status (24-07), `Cold` by default.
    backup_server: Option<wiremock::MockServer>,
    /// The gateway behind the backup's deploy surface, to seed what its
    /// secret providers list (24-07).
    backup_fake: Option<FakeGateway>,
    /// The backup's file transport (a local data directory), when the test
    /// gives the backup one (24-06).
    backup_reader: Option<LocalFsTransport>,
    /// What a pair rig keeps alive (24-06): the backup's server, gateway and
    /// data directory, and the link between the pair.
    pair: Option<PairParts>,
    _server: wiremock::MockServer,
}

/// The backup half of a pair rig.
struct PairParts {
    link: PairLink,
    server: wiremock::MockServer,
    _dir: tempfile::TempDir,
    _gateway: FakeGateway,
}

/// The system name the rig's host and its REST answer share.
const RIG_GATEWAY_NAME: &str = "rig-gw";

/// Mounts the answers that let every preflight pass, at the lowest priority
/// so that a test's own mock for the same route wins.
async fn mount_passing_preflights(server: &wiremock::MockServer) {
    use wiremock::matchers::{method, path};
    wiremock::Mock::given(method("GET"))
        .and(path("/data/api/v1/gateway-info"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
            json!({"name": RIG_GATEWAY_NAME, "ignitionVersion": "8.3.6 (b2026042713)"}),
        ))
        .with_priority(250)
        .mount(server)
        .await;
    wiremock::Mock::given(method("GET"))
        .and(path("/data/api/v1/designers"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(designers_body(&[])))
        .with_priority(250)
        .mount(server)
        .await;
    wiremock::Mock::given(method("POST"))
        .and(path("/data/api/v1/scan/projects"))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .with_priority(250)
        .mount(server)
        .await;
    mount_gwbk(server, gwbk_zip(), 250).await;
    // The master of a pair is the active node unless a test says otherwise.
    mount_redundancy(server, "Master", "Active", 250).await;
}

/// A redundancy status body in the shape of the C-S3 captures: an `Active`
/// node reports an uptime, a `Cold` one `-1`.
fn redundancy_body(role: &str, level: &str) -> Value {
    json!({
        "role": role,
        "projectState": "Good",
        "activityLevel": level,
        "localId": "192.168.97.3",
        "peerId": "rig-peer",
        "peerConnected": true,
        "hasConfigAccess": true,
        "syncPending": false,
        "failoverPending": false,
        "uptime": if level == "Active" { 29279 } else { -1 },
        "lastSyncTimestamp": 1_791_586_091_827_i64,
    })
}

/// `GET /data/api/v1/redundancy` on `server` answers a node of `role` at
/// activity `level`, at `priority` (lower wins; 5 is wiremock's default).
async fn mount_redundancy(server: &wiremock::MockServer, role: &str, level: &str, priority: u8) {
    use wiremock::matchers::{method, path};
    wiremock::Mock::given(method("GET"))
        .and(path("/data/api/v1/redundancy"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(redundancy_body(role, level)),
        )
        .with_priority(priority)
        .mount(server)
        .await;
}

/// A small valid ZIP standing in for a `.gwbk`: one stored member, so that
/// cutting the file anywhere in the second half loses the central directory.
fn gwbk_zip() -> Vec<u8> {
    gwbk_zip_holding(None)
}

/// [`gwbk_zip`], the member carrying `secret` when given (23-15's canary: a
/// `.gwbk` holds the gateway's secrets).
fn gwbk_zip_holding(secret: Option<&str>) -> Vec<u8> {
    use std::io::Write as _;
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    writer
        .start_file("data.bin", options)
        .expect("member starts");
    writer.write_all(&[7u8; 4096]).expect("member writes");
    if let Some(secret) = secret {
        writer.write_all(secret.as_bytes()).expect("secret writes");
    }
    writer.finish().expect("finalizes").into_inner()
}

/// The gateway backup route answering `body`, at `priority` (lower wins).
async fn mount_gwbk(server: &wiremock::MockServer, body: Vec<u8>, priority: u8) {
    use wiremock::matchers::{method, path};
    wiremock::Mock::given(method("GET"))
        .and(path("/data/api/v1/backup"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .insert_header(
                    "content-type",
                    "application/x-ignition-gateway-backup;charset=utf-8",
                )
                .set_body_bytes(body),
        )
        .with_priority(priority)
        .mount(server)
        .await;
}

/// The designers list body for `(user, project, address)` sessions.
fn designers_body(sessions: &[(&str, &str, &str)]) -> Value {
    let items: Vec<Value> = sessions
        .iter()
        .enumerate()
        .map(|(i, (user, project, address))| {
            json!({
                "id": format!("d-{i}"),
                "user": user,
                "project": project,
                "address": address,
                "uptime": 1,
                "lastcomm": 1,
                "timeout": 1,
                "memory": {},
            })
        })
        .collect();
    json!({
        "items": items,
        "metadata": {"total": items.len(), "matching": items.len(), "limit": -1, "offset": 0}
    })
}

fn local_host(fx: &PlanFixture) -> LocalFsTransport {
    LocalFsTransport::new(fx.data_dir().to_path_buf(), "nodes.master")
        .with_apply(GATEWAY, MODE)
        .expect("apply host")
}

async fn rig_with(fx: PlanFixture, host: LocalFsTransport, fake: Option<FakeGateway>) -> Rig {
    rig_with_logs(fx, host, fake, Vec::new()).await
}

/// `rig_with`, the gateway log source answering `items`.
async fn rig_with_logs(
    fx: PlanFixture,
    host: LocalFsTransport,
    fake: Option<FakeGateway>,
    items: Vec<Value>,
) -> Rig {
    fx.core(
        "ignition/system-properties/config.json",
        json!({"systemName": RIG_GATEWAY_NAME})
            .to_string()
            .as_bytes(),
    );
    let (declared, reader) = fx.load("dev");
    let repo = fx.repo("dev");
    let fake = fake.unwrap_or_else(|| FakeGateway::with_data_dir(fx.data_dir(), MODE));
    let server = wiremock::MockServer::start().await;
    mount_passing_preflights(&server).await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/logs"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "items": items,
            "metadata": {"total": items.len(), "matching": items.len(), "limit": 200, "offset": 0}
        })))
        .mount(&server)
        .await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    Rig {
        fx,
        declared,
        reader,
        repo,
        fake,
        api,
        host: FaultyHost::new(host),
        faults: DeployFaults::default(),
        backup: None,
        backup_api: None,
        backup_server: None,
        backup_fake: None,
        backup_reader: None,
        pair: None,
        _server: server,
    }
}

/// The tracer repo with the render live in the `dev` collection, then one
/// unit changed (POSTGRES, by one byte), one added (the alarm settings are
/// removed from the collection) and a prior ledger listing POSTGRES.
async fn changing_rig() -> Rig {
    changing_rig_with_logs(Vec::new()).await
}

async fn changing_rig_with_logs(items: Vec<Value>) -> Rig {
    changing_rig_faulted(items, None).await
}

/// `changing_rig_with_logs`, the local host carrying `fault` when given.
async fn changing_rig_faulted(items: Vec<Value>, fault: Option<ApplyFault>) -> Rig {
    let fx = PlanFixture::new();
    let (declared, _) = fx.load("dev");
    fx.live_render(MODE, &declared);
    let mut bytes = fx.read(MODE, DB_CONFIG);
    bytes.push(b' ');
    fx.mode(DB_CONFIG, &bytes);
    fx.remove(MODE, ALARM_UNIT);
    fx.ledger(GATEWAY, MODE, &[DB_CONFIG, DB_RESOURCE]);
    let host = local_host(&fx);
    let host = match fault {
        Some(fault) => host.with_apply_fault(fault),
        None => host,
    };
    rig_with_logs(fx, host, None, items).await
}

fn fast_poll() -> PollConfig {
    PollConfig {
        subject: "apply test scan".to_string(),
        interval: Duration::from_millis(20),
        max: Duration::from_millis(20),
        deadline: Duration::from_secs(5),
    }
}

fn options() -> ApplyOptions {
    ApplyOptions {
        base: None,
        gates: GateFlags::default(),
        project: "ign-cli".to_string(),
        hold: HoldModel {
            hold_s: HOLD_S,
            commit_fixed_s: 1.0,
            commit_per_file_s: 0.1,
            p95_scan_s: 1.0,
        },
        poll: fast_poll(),
        verify: VerifyOptions {
            probe_deadline: Duration::from_millis(400),
            probe_interval: Duration::from_millis(20),
            log_settle: Duration::ZERO,
        },
        designers: DesignersPolicy::default(),
        skip_gwbk: false,
        backup: BackupPolicy::default(),
    }
}

/// Records each event the engine reports.
#[derive(Default)]
struct Recorder(Mutex<Vec<ApplyEvent>>);

impl ApplyObserver for Recorder {
    fn event(&self, e: &ApplyEvent) {
        self.0.lock().expect("events").push(e.clone());
    }
}

impl Rig {
    /// The read-only handle on the backup node, when the target has one.
    fn backup_node(&self) -> Option<BackupNode<'_>> {
        self.backup
            .as_ref()
            .zip(self.backup_api.as_ref())
            .map(|(deploy, gateway)| {
                BackupNode::new(
                    gateway,
                    deploy.as_ref(),
                    self.backup_reader.as_ref().map(|r| r as &dyn TreeReader),
                )
            })
    }

    async fn prepare(&self, flags: GateFlags) -> Result<ApplyPrepared, CoreError> {
        deploy_apply_prepare(
            &self.declared,
            self.reader.as_ref(),
            &PrepareOptions {
                base: None,
                gates: flags,
            },
        )
        .await
    }

    async fn run(
        &self,
        prepared: ApplyPrepared,
        observer: &dyn ApplyObserver,
    ) -> Result<DeployApplyResult, CoreError> {
        self.run_with(prepared, observer, &SystemClock, &options())
            .await
    }

    async fn run_with(
        &self,
        prepared: ApplyPrepared,
        observer: &dyn ApplyObserver,
        clock: &dyn Clock,
        opts: &ApplyOptions,
    ) -> Result<DeployApplyResult, CoreError> {
        let deploy = FaultyDeploy::new(self.fake.client("apply"), self.faults.clone());
        let backup = self.backup_node();
        let ctx = ApplyContext {
            declared: &self.declared,
            repo: &self.repo,
            reader: self.reader.as_ref(),
            host: &self.host,
            deploy: &deploy,
            gateway: &self.api,
            backup: backup.as_ref(),
            clock,
            observer,
        };
        deploy_apply_execute(prepared, &ctx, opts).await
    }

    fn deploy_dir(&self) -> PathBuf {
        self.fx
            .data_dir()
            .join("ign-deploy")
            .join(GATEWAY)
            .join(MODE)
    }

    fn run_dir(&self, result: &DeployApplyResult) -> PathBuf {
        PathBuf::from(result.run_dir.as_deref().expect("a run directory"))
    }

    fn record(&self, run_id: &str) -> RunRecord {
        let path = self
            .deploy_dir()
            .join("snapshots")
            .join(run_id)
            .join("record.json");
        RunRecord::parse(
            "record.json",
            &std::fs::read(path).expect("record"),
            GATEWAY,
            MODE,
        )
        .expect("record parses")
    }
}

fn names(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .map(|read| {
            read.filter_map(Result::ok)
                .filter_map(|e| e.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

fn json_file(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).expect("file")).expect("json")
}

fn event_names(run_dir: &Path) -> Vec<String> {
    std::fs::read_to_string(run_dir.join("events.ndjson"))
        .expect("events")
        .lines()
        .map(|line| {
            serde_json::from_str::<Value>(line).expect("line")["event"]
                .as_str()
                .expect("event")
                .to_string()
        })
        .collect()
}

#[tokio::test]
async fn apply_happy_path_local() {
    let rig = changing_rig().await;
    let old_db = rig.fx.read(MODE, DB_CONFIG);

    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let ApplyPrepared::Ready { plan, prose } = &prepared else {
        panic!("expected Ready, got {prepared:?}");
    };
    assert_eq!((plan.summary.added, plan.summary.changed), (1, 1));
    assert!(prose.contains(&plan.plan_sha256), "{prose}");
    let plan = plan.clone();
    let rendered_db = rig.declared.outcome.tree.files.iter();
    let rendered_db: BTreeMap<&str, &Vec<u8>> = rendered_db.map(|(p, b)| (p.as_str(), b)).collect();

    let recorder = Recorder::default();
    let result = rig.run(prepared, &recorder).await.expect("applied");

    assert_eq!(result.outcome, OutcomeKind::Applied);
    assert!(result.ok);
    assert_eq!(result.slug, None);
    assert_eq!(result.failure_exit_code(), None);
    assert_eq!(result.plan_sha256, plan.plan_sha256);
    let run_id = result.run_id.clone().expect("run id");
    let mut written = vec![
        DB_CONFIG.to_string(),
        DB_RESOURCE.to_string(),
        ALARM_CONFIG.to_string(),
        ALARM_RESOURCE.to_string(),
    ];
    written.sort();
    assert_eq!(result.written, written);
    assert!(result.deleted.is_empty());

    // The collection holds the rendered files.
    for path in &written {
        assert_eq!(
            rig.fx.read(MODE, path),
            **rendered_db.get(path.as_str()).expect("rendered"),
            "{path}"
        );
    }
    assert_ne!(rig.fx.read(MODE, DB_CONFIG), old_db);

    // The ledger: the old files plus the written ones, byte-sorted.
    let ledger_bytes = std::fs::read(rig.deploy_dir().join("ledger.json")).expect("ledger");
    let ledger = Ledger::parse("ledger.json", &ledger_bytes, GATEWAY, MODE).expect("ledger parses");
    assert_eq!(ledger.files, written);
    assert_eq!(ledger.run_id, run_id);
    assert_eq!(ledger.render_sha256, plan.render_sha256);

    // The journal and the staging are gone; the snapshot stays.
    assert!(!rig.deploy_dir().join("journal.json").exists());
    assert!(!rig.deploy_dir().join("staging").join(&run_id).exists());
    let record = rig.record(&run_id);
    assert_eq!(record.state, RecordState::Applied);
    assert_eq!(record.plan_sha256, plan.plan_sha256);
    let live: BTreeMap<String, Option<String>> = record
        .touched
        .iter()
        .map(|entry| {
            let hash = if entry.path == LEDGER_KEY {
                Some(sha256_hex(&ledger_bytes))
            } else {
                Some(sha256_hex(&rig.fx.read(MODE, &entry.path)))
            };
            (entry.path.clone(), hash)
        })
        .collect();
    assert_eq!(record.post_fingerprint, Some(fingerprint(&live)));
    assert_eq!(record.post_map(), live);

    // The run directory.
    let run_dir = rig.run_dir(&result);
    let mut expected_files = vec![
        "events.ndjson".to_string(),
        "plan.json".to_string(),
        "result.json".to_string(),
        "run.json".to_string(),
        "snapshot".to_string(),
        format!("{run_id}.gwbk"),
    ];
    expected_files.sort();
    assert_eq!(names(&run_dir), expected_files);
    assert_eq!(
        json_file(&run_dir.join("result.json"))["outcome"],
        "applied"
    );
    assert_eq!(
        json_file(&run_dir.join("run.json"))["run_id"],
        run_id.as_str()
    );
    assert_eq!(
        json_file(&run_dir.join("plan.json"))["plan_sha256"],
        plan.plan_sha256.as_str()
    );
    for entry in &record.touched {
        let Some(pre) = &entry.pre_sha256 else {
            continue;
        };
        let copy = if entry.path == LEDGER_KEY {
            run_dir.join("snapshot").join("ledger.json")
        } else {
            run_dir.join("snapshot").join("files").join(&entry.path)
        };
        assert_eq!(
            &sha256_hex(&std::fs::read(&copy).expect("pre-image copy")),
            pre,
            "{}",
            entry.path
        );
    }

    // One lock, one commit, one scan, in the confirmed order.
    assert_eq!(rig.fake.lock_acquires(), 1);
    assert_eq!(rig.fake.scan_count(), 1);
    assert_eq!(
        rig.host.calls(),
        [
            "probe",
            "prepare",
            "commit",
            "fingerprint",
            "finalize",
            "fetch_snapshot"
        ]
    );
    let expected = [
        "run_started",
        "gwbk_downloaded",
        "staged",
        "lock_acquired",
        "committed",
        "lock_verdict",
        "scan_done",
        "verify_done",
        "finalized",
    ];
    assert_eq!(event_names(&run_dir), expected);
    let seen = recorder.0.lock().expect("events");
    assert_eq!(seen.len(), expected.len(), "{seen:?}");
    assert!(matches!(
        seen[0],
        ApplyEvent::RunStarted {
            writes: 4,
            deletes: 0,
            ..
        }
    ));
    assert!(matches!(seen[1], ApplyEvent::GwbkDownloaded { .. }));
    assert!(matches!(seen[6], ApplyEvent::ScanDone));
    assert!(matches!(seen[7], ApplyEvent::VerifyDone { ok: true }));
}

#[tokio::test]
async fn empty_plan_is_a_noop() {
    let fx = PlanFixture::new();
    let (declared, _) = fx.load("dev");
    fx.live_render(MODE, &declared);
    let host = local_host(&fx);
    let rig = rig_with(fx, host, None).await;

    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let ApplyPrepared::Noop(result) = &prepared else {
        panic!("expected Noop, got {prepared:?}");
    };
    assert_eq!(result.outcome, OutcomeKind::Noop);
    assert!(result.ok);
    assert_eq!(result.failure_exit_code(), None);
    assert_eq!(
        (result.run_id.as_deref(), result.run_dir.as_deref()),
        (None, None)
    );
    assert!(result.written.is_empty() && result.deleted.is_empty());

    // Executing the no-op touches nothing either.
    let executed = rig.run(prepared, &NoObserver).await.expect("noop");
    assert_eq!(executed.outcome, OutcomeKind::Noop);
    assert_eq!(rig.fake.lock_acquires(), 0);
    assert_eq!(rig.fake.scan_count(), 0);
    assert!(rig.host.calls().is_empty(), "{:?}", rig.host.calls());
    assert!(!rig.repo.join(".ign").exists(), "no run directory");
    assert!(
        !rig.fx.data_dir().join("ign-deploy").exists(),
        "no host state"
    );
}

#[tokio::test]
async fn stale_hash_stops_before_any_host_write() {
    let rig = changing_rig().await;
    let err = rig
        .prepare(GateFlags {
            plan_sha256: Some("0".repeat(64)),
            ..GateFlags::default()
        })
        .await
        .expect_err("a stale hash refuses");
    assert_eq!(err.code(), "plan_stale", "{err}");
    assert!(rig.host.calls().is_empty());
    assert_eq!(rig.fake.lock_acquires(), 0);
    assert!(!rig.repo.join(".ign").exists());
}

#[tokio::test]
async fn write_set_pre_images_follow_where_the_live_copy_is() {
    // The alarm settings exist in `core` only; POSTGRES in the mode
    // collection.
    let fx = PlanFixture::new();
    let (declared, _) = fx.load("dev");
    fx.live_render("core", &declared);
    // The core copy of the alarm settings differs from the render, so the unit
    // is `changed` although the mode collection holds none of it.
    let mut core_alarm = fx.read("core", ALARM_CONFIG);
    core_alarm.push(b' ');
    fx.core(ALARM_CONFIG, &core_alarm);
    let mut bytes = fx.read("core", DB_CONFIG);
    bytes.push(b' ');
    fx.mode(DB_CONFIG, &bytes);
    fx.mode(DB_RESOURCE, &fx.read("core", DB_RESOURCE));
    let (declared, reader) = fx.load("dev");
    let plan = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect("plan");

    let set = write_set(&plan, &declared).expect("write set");
    let pre: BTreeMap<&str, Option<&str>> = set
        .writes
        .iter()
        .map(|w| (w.path.as_str(), w.pre.as_deref()))
        .collect();
    assert_eq!(pre.keys().copied().collect::<Vec<_>>(), {
        let mut all = vec![DB_CONFIG, DB_RESOURCE, ALARM_CONFIG, ALARM_RESOURCE];
        all.sort();
        all
    });
    // Present in the mode collection: the live hash is the pre-image.
    assert_eq!(pre[DB_CONFIG], Some(sha256_hex(&bytes).as_str()));
    // Live in `core` only: absent in the mode collection.
    assert_eq!(pre[ALARM_CONFIG], None);
    assert_eq!(pre[ALARM_RESOURCE], None);
    assert!(set.deletes.is_empty());
    let sorted: Vec<&str> = set.writes.iter().map(|w| w.path.as_str()).collect();
    let mut expect = sorted.clone();
    expect.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    assert_eq!(sorted, expect, "byte-sorted");
}

#[tokio::test]
async fn apply_deletes_what_the_ledger_lists_and_rewrites_the_ledger() {
    let fx = PlanFixture::new();
    let (declared, _) = fx.load("dev");
    fx.live_render(MODE, &declared);
    // A unit an earlier run wrote and the render no longer holds.
    let old = "ignition/database-translator/OLD";
    fx.mode(&format!("{old}/config.json"), b"{}");
    fx.mode(&format!("{old}/resource.json"), b"{}");
    fx.ledger(
        GATEWAY,
        MODE,
        &[
            DB_CONFIG,
            DB_RESOURCE,
            &format!("{old}/config.json"),
            &format!("{old}/resource.json"),
        ],
    );
    let host = local_host(&fx);
    let rig = rig_with(fx, host, None).await;

    let flags = GateFlags {
        allow_destructive: true,
        ..GateFlags::default()
    };
    let prepared = rig.prepare(flags).await.expect("prepare");
    let result = rig.run(prepared, &NoObserver).await.expect("applied");
    assert_eq!(result.outcome, OutcomeKind::Applied);
    assert!(result.written.iter().all(|p| !p.contains("/OLD/")));
    assert_eq!(
        result.deleted,
        [format!("{old}/config.json"), format!("{old}/resource.json")]
    );
    assert!(
        !rig.fx.collection_dir(MODE).join(old).exists(),
        "emptied unit removed"
    );

    let ledger_bytes = std::fs::read(rig.deploy_dir().join("ledger.json")).expect("ledger");
    let ledger = Ledger::parse("ledger.json", &ledger_bytes, GATEWAY, MODE).expect("parses");
    assert!(
        ledger.files.iter().all(|p| !p.contains("/OLD/")),
        "{:?}",
        ledger.files
    );
    assert!(ledger.files.contains(&DB_CONFIG.to_string()));
    let record = rig.record(result.run_id.as_deref().expect("run"));
    let gone: Vec<&str> = record
        .touched
        .iter()
        .filter(|e| e.post_sha256.is_none())
        .map(|e| e.path.as_str())
        .collect();
    assert_eq!(gone.len(), 2, "{gone:?}");
}

// ---- the branches 23-17 turns into restores (the rest follow in Tasks 2 and 3) ---

/// The one `result.json` of a failed run, which names the error code.
fn failed_result(rig: &Rig) -> Value {
    let root = rig.repo.join(".ign/deploy").join(GATEWAY);
    let runs = names(&root);
    assert_eq!(runs.len(), 1, "one run directory: {runs:?}");
    let dir = root.join(&runs[0]);
    assert_eq!(event_names(&dir).last().map(String::as_str), Some("failed"));
    json_file(&dir.join("result.json"))
}

#[tokio::test]
async fn a_journal_of_another_run_names_the_real_env() {
    let rig = changing_rig().await;
    let dir = rig.deploy_dir();
    // The plan refuses over a journal (23-12), so the journal appears between
    // the preparation and the run: the host's own refusal is what the engine
    // sees, and it fills the env.
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    std::fs::create_dir_all(&dir).expect("deploy dir");
    let journal = Journal {
        schema_version: 1,
        run_id: "20261016T000000Z-ffffff".to_string(),
        gateway: GATEWAY.to_string(),
        mode: MODE.to_string(),
        phase: JournalPhase::Writing,
        pre_fingerprint: "0".repeat(64),
        touched: vec![DB_CONFIG.to_string()],
    };
    std::fs::write(dir.join("journal.json"), journal.to_bytes()).expect("journal");

    let err = rig.run(prepared, &NoObserver).await.expect_err("refused");
    let CoreError::DeployJournalPresent { run_id, env, .. } = &err else {
        panic!("expected deploy_journal_present, got {err:?}");
    };
    assert_eq!(run_id, "20261016T000000Z-ffffff");
    assert_eq!(env, "dev", "the engine fills the env the host cannot know");
    assert!(err.to_string().contains("--env dev"), "{err}");
}

// ============================================================================
// Aborts and auto-rollback (23-17)
// ============================================================================

/// Every file of the mode collection (path to SHA-256) plus the ledger: the
/// state an abort must give back.
fn collection_state(rig: &Rig) -> BTreeMap<String, String> {
    fn walk(dir: &Path, rel: &str, out: &mut BTreeMap<String, String>) {
        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in read.filter_map(Result::ok) {
            let name = entry.file_name().into_string().expect("utf-8 name");
            let path = entry.path();
            let key = if rel.is_empty() {
                name
            } else {
                format!("{rel}/{name}")
            };
            if path.is_dir() {
                walk(&path, &key, out);
            } else {
                out.insert(key, sha256_hex(&std::fs::read(&path).expect("read")));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(&rig.fx.collection_dir(MODE), "", &mut out);
    if let Ok(bytes) = std::fs::read(rig.deploy_dir().join("ledger.json")) {
        out.insert(LEDGER_KEY.to_string(), sha256_hex(&bytes));
    }
    out
}

/// A rig whose commit fails after its first move.
async fn failing_move_rig() -> Rig {
    let fx = PlanFixture::new();
    let (declared, _) = fx.load("dev");
    fx.live_render(MODE, &declared);
    let mut bytes = fx.read(MODE, DB_CONFIG);
    bytes.push(b' ');
    fx.mode(DB_CONFIG, &bytes);
    fx.remove(MODE, ALARM_UNIT);
    fx.ledger(GATEWAY, MODE, &[DB_CONFIG, DB_RESOURCE]);
    let host = local_host(&fx).with_apply_fault(ApplyFault::FailMoveAt(1));
    rig_with(fx, host, None).await
}

/// The words every rolled-back result carries.
fn assert_rolled_back(result: &DeployApplyResult, cause: &str) {
    assert_eq!(result.outcome, OutcomeKind::RolledBack, "{result:?}");
    assert!(!result.ok);
    assert_eq!(result.slug, Some("deploy_rolled_back"));
    assert_eq!(
        serde_json::to_value(result.cause).expect("cause"),
        cause,
        "{result:?}"
    );
    assert_eq!(result.failure_exit_code(), Some(6));
    assert!(result.written.is_empty() && result.deleted.is_empty());
}

#[tokio::test]
async fn fault_write_failed_mid_commit_restores() {
    let rig = failing_move_rig().await;
    let before = collection_state(&rig);

    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let recorder = Recorder::default();
    let result = rig
        .run(prepared, &recorder)
        .await
        .expect("a rollback is a result, not an error");

    assert_rolled_back(&result, "write_failed");
    assert_eq!(collection_state(&rig), before, "the snapshot is back");
    let run_id = result.run_id.clone().expect("run id");
    assert!(!rig.deploy_dir().join("journal.json").exists());
    assert!(!rig.deploy_dir().join("staging").join(&run_id).exists());
    assert_eq!(rig.record(&run_id).state, RecordState::RolledBack);
    assert_eq!(
        rig.record(&run_id).rolled_back_by.as_deref(),
        Some(run_id.as_str())
    );

    // The restore reused the held lock; the one scan came after it.
    assert_eq!(rig.fake.lock_acquires(), 1);
    assert_eq!(rig.fake.scan_count(), 1);
    assert_eq!(
        rig.host.calls(),
        [
            "probe",
            "prepare",
            "commit",
            "restore",
            "fingerprint",
            "finalize",
            "fetch_snapshot"
        ]
    );

    let run_dir = rig.run_dir(&result);
    assert_eq!(
        event_names(&run_dir),
        [
            "run_started",
            "gwbk_downloaded",
            "staged",
            "lock_acquired",
            "committed",
            "restore_started",
            "scan_done",
            "restored"
        ]
    );
    let doc = json_file(&run_dir.join("result.json"));
    assert_eq!(doc["outcome"], "rolled_back");
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["slug"], "deploy_rolled_back");
    assert_eq!(doc["cause"], "write_failed");
    let seen = recorder.0.lock().expect("events");
    assert!(matches!(
        seen[5],
        ApplyEvent::RestoreStarted {
            cause: Some(RollbackCause::WriteFailed)
        }
    ));
}

#[tokio::test]
async fn fault_rerun_after_restore_plans_the_same() {
    // Edge DAPL-06 idempotency.
    let rig = failing_move_rig().await;
    let ApplyPrepared::Ready { plan: first, .. } =
        rig.prepare(GateFlags::default()).await.expect("prepare")
    else {
        panic!("expected Ready");
    };
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let result = rig.run(prepared, &NoObserver).await.expect("rolled back");
    assert_rolled_back(&result, "write_failed");

    let ApplyPrepared::Ready { plan: second, .. } = rig
        .prepare(GateFlags::default())
        .await
        .expect("prepare again")
    else {
        panic!("expected Ready again");
    };
    assert_eq!(second.plan_sha256, first.plan_sha256);
    assert_eq!(second.render_sha256, first.render_sha256);
}

/// The `error` code of the one `result.json` of a failed run.
fn failed_error_code(rig: &Rig) -> String {
    failed_result(rig)["error"]["code"]
        .as_str()
        .expect("an error code")
        .to_string()
}

/// One run of a scenario: the rig it ran on, the collection before it, the
/// scan counts seen when `restore` returned, and what the engine answered.
struct Ran {
    rig: Rig,
    before: BTreeMap<String, String>,
    scans_at_restore: Arc<Mutex<Vec<usize>>>,
    result: Result<DeployApplyResult, CoreError>,
}

impl Ran {
    fn rolled_back(&self, cause: &str) -> &DeployApplyResult {
        let result = self.result.as_ref().expect("a rollback is a result");
        assert_rolled_back(result, cause);
        assert_eq!(
            collection_state(&self.rig),
            self.before,
            "the snapshot is back"
        );
        result
    }

    fn error(&self) -> &CoreError {
        self.result.as_ref().expect_err("the run failed")
    }

    fn scans_seen_at_restore(&self) -> Vec<usize> {
        self.scans_at_restore.lock().expect("seen").clone()
    }
}

/// Run `rig` once with the options `opts` and `clock`, recording the scan
/// count when its first `restore` returns.
async fn ran(
    rig: Rig,
    before: BTreeMap<String, String>,
    scans_at_restore: Arc<Mutex<Vec<usize>>>,
    clock: &dyn Clock,
    opts: &ApplyOptions,
) -> Ran {
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let result = rig.run_with(prepared, &NoObserver, clock, opts).await;
    Ran {
        rig,
        before,
        scans_at_restore,
        result,
    }
}

/// `rig` with `after_commit` run once the first commit returned and the scan
/// count recorded once the first restore returned.
fn hooked(
    rig: Rig,
    after_commit: impl Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync + 'static,
) -> (Rig, Arc<Mutex<Vec<usize>>>) {
    let fake = rig.fake.clone();
    let scans_at_restore = Arc::new(Mutex::new(Vec::new()));
    let seen = scans_at_restore.clone();
    let host = rig
        .host
        .clone()
        .after("commit", 1, after_commit)
        .after("restore", 1, move || {
            seen.lock().expect("seen").push(fake.scan_count());
            async {}
        });
    (Rig { host, ..rig }, scans_at_restore)
}

fn quiet_hook() -> Arc<Mutex<Vec<usize>>> {
    Arc::new(Mutex::new(Vec::new()))
}

async fn scenario_write_failed() -> Ran {
    let rig = failing_move_rig().await;
    let before = collection_state(&rig);
    ran(rig, before, quiet_hook(), &SystemClock, &options()).await
}

async fn scenario_lock_expired() -> Ran {
    let rig = changing_rig().await;
    let before = collection_state(&rig);
    let fake = rig.fake.clone();
    let (rig, seen) = hooked(rig, move || {
        fake.tick(HOLD_S as i64 + 1);
        Box::pin(async {})
    });
    ran(rig, before, seen, &SystemClock, &options()).await
}

async fn scenario_local_clock_expired() -> Ran {
    let rig = changing_rig().await;
    let before = collection_state(&rig);
    let clock = Arc::new(ManualClock::new());
    let tick = clock.clone();
    // The gateway still reports a large remaining; only the local count ran out.
    let (rig, seen) = hooked(rig, move || {
        tick.advance(Duration::from_secs(HOLD_S + 1));
        Box::pin(async {})
    });
    ran(rig, before, seen, clock.as_ref(), &options()).await
}

async fn scenario_short_remaining() -> Ran {
    let rig = changing_rig().await;
    let before = collection_state(&rig);
    let clock = Arc::new(ManualClock::new());
    let (fake, tick) = (rig.fake.clone(), clock.clone());
    // The local count ran out and the gateway has a few seconds left: not
    // enough for the restore, so the restore waits for the lapse.
    let (rig, seen) = hooked(rig, move || {
        tick.advance(Duration::from_secs(HOLD_S + 1));
        fake.tick(HOLD_S as i64 - 4);
        Box::pin(async {})
    });
    ran(rig, before, seen, clock.as_ref(), &options()).await
}

async fn scenario_scan_incomplete() -> Ran {
    let rig = changing_rig().await;
    let before = collection_state(&rig);
    rig.fake.stall_scans(1);
    let mut opts = options();
    opts.poll.deadline = Duration::from_secs(1);
    ran(rig, before, quiet_hook(), &SystemClock, &opts).await
}

async fn scenario_pre_image_changed() -> (Ran, String, Vec<u8>) {
    let rig = changing_rig().await;
    let target = rig.fx.collection_dir(MODE).join(DB_RESOURCE);
    let mut edited = std::fs::read(&target).expect("live file");
    edited.push(b'\n');
    let mut expected = collection_state(&rig);
    expected.insert(DB_RESOURCE.to_string(), sha256_hex(&edited));
    let (path, bytes) = (target.clone(), edited.clone());
    let host = rig.host.clone().after("prepare", 1, move || {
        std::fs::write(&path, &bytes).expect("foreign edit");
        async {}
    });
    let rig = Rig { host, ..rig };
    let run = ran(rig, expected, quiet_hook(), &SystemClock, &options()).await;
    (run, DB_RESOURCE.to_string(), edited)
}

async fn scenario_competing_lock() -> Ran {
    let rig = changing_rig().await;
    let before = collection_state(&rig);
    rig.fake
        .client("b")
        .scan_lock_acquire(ScanKind::Config, 1, 600)
        .await
        .expect("B takes the lock");
    ran(rig, before, quiet_hook(), &SystemClock, &options()).await
}

/// A log line saying the written database translator will not be loaded.
fn unloaded_log_line() -> Value {
    json!({
        "timestamp": 1_792_000_000_050_i64,
        "loggerName": "NamedResourceHandler-ignition/database-translator",
        "level": "WARN",
        "message": format!(
            "Resource {DB_UNIT} does not have required UUID attribute, will not be loaded"
        ),
    })
}

async fn scenario_verify_red(
    hook: impl FnOnce(&Rig) -> Option<FaultyHost<LocalFsTransport>>,
) -> Ran {
    let rig = changing_rig_with_logs(vec![unloaded_log_line()]).await;
    let before = collection_state(&rig);
    let rig = match hook(&rig) {
        Some(host) => Rig { host, ..rig },
        None => rig,
    };
    ran(rig, before, quiet_hook(), &SystemClock, &options()).await
}

async fn scenario_restore_fails() -> Ran {
    scenario_verify_red(|rig| Some(rig.host.clone().fail("restore", 1))).await
}

async fn scenario_restore_check_mismatch() -> Ran {
    scenario_verify_red(|rig| {
        let target = rig.fx.collection_dir(MODE).join(DB_CONFIG);
        Some(rig.host.clone().after("restore", 1, move || {
            std::fs::write(&target, b"altered after the restore").expect("alter");
            async {}
        }))
    })
    .await
}

async fn scenario_lock_taken() -> Ran {
    // Our hold lapses and another actor takes the lock before the restore.
    let rig = changing_rig().await;
    let before = collection_state(&rig);
    let fake = rig.fake.clone();
    let host = rig.host.clone().after("commit", 1, move || {
        let fake = fake.clone();
        async move {
            fake.tick(HOLD_S as i64 + 1);
            fake.client("b")
                .scan_lock_acquire(ScanKind::Config, 1, 600)
                .await
                .expect("B takes the lock");
        }
    });
    let rig = Rig { host, ..rig };
    ran(rig, before, quiet_hook(), &SystemClock, &options()).await
}

const LAPSED_EVENTS: [&str; 10] = [
    "run_started",
    "gwbk_downloaded",
    "staged",
    "lock_acquired",
    "committed",
    "lock_verdict",
    "restore_started",
    "lock_acquired",
    "scan_done",
    "restored",
];

#[tokio::test]
async fn fault_lock_expired_before_scan_restores_then_scans() {
    let run = scenario_lock_expired().await;
    let result = run.rolled_back("lock_expired");
    assert_eq!(
        run.scans_seen_at_restore(),
        [0],
        "no scan ran before the restore"
    );
    assert_eq!(run.rig.fake.scan_count(), 1, "the restore's own scan");
    assert_eq!(
        run.rig.fake.lock_acquires(),
        2,
        "a fresh hold for the restore"
    );
    assert_eq!(event_names(&run.rig.run_dir(result)), LAPSED_EVENTS);
}

#[tokio::test]
async fn fault_local_clock_expired_restores_under_held_lock() {
    let run = scenario_local_clock_expired().await;
    run.rolled_back("lock_expired");
    assert_eq!(
        run.rig.fake.lock_acquires(),
        1,
        "no second acquire under a held lock"
    );
    assert_eq!(run.scans_seen_at_restore(), [0]);
    assert_eq!(run.rig.fake.scan_count(), 1);
}

#[tokio::test]
async fn fault_short_remaining_waits_for_the_lapse_then_restores_under_a_fresh_lock() {
    let run = scenario_short_remaining().await;
    run.rolled_back("lock_expired");
    assert_eq!(run.rig.fake.lock_acquires(), 2);
    assert_eq!(run.scans_seen_at_restore(), [0]);
    assert_eq!(run.rig.fake.scan_count(), 1);
}

#[tokio::test]
async fn fault_scan_incomplete_restores() {
    let run = scenario_scan_incomplete().await;
    let result = run.rolled_back("scan_incomplete");
    assert_eq!(
        run.rig.fake.scan_count(),
        2,
        "the stalled scan and the restore's own"
    );
    assert_eq!(event_names(&run.rig.run_dir(result)), LAPSED_EVENTS);
}

#[tokio::test]
async fn fault_pre_image_changed_is_plan_stale() {
    // Edges DAPL-01 and DAPL-05 concurrency: a mode-collection file changed
    // between the plan and the lock.
    let (run, _, _) = scenario_pre_image_changed().await;
    let rig = &run.rig;
    let err = run.error();
    assert_eq!((err.code(), err.exit_code()), ("plan_stale", 6), "{err}");
    assert_eq!(collection_state(rig), run.before, "nothing moved");
    assert!(!rig.deploy_dir().join("journal.json").exists());
    assert!(names(&rig.deploy_dir().join("snapshots")).is_empty());
    assert!(names(&rig.deploy_dir().join("staging")).is_empty());
    assert_eq!(rig.fake.scan_count(), 1, "one no-op scan releases the lock");
    assert_eq!(rig.fake.lock_acquires(), 1);
    assert_eq!(
        rig.host.calls(),
        ["probe", "prepare", "commit", "discard_interrupted"]
    );
    assert_eq!(failed_error_code(rig), "plan_stale");
}

#[tokio::test]
async fn fault_competing_lock_is_scan_lock_held() {
    // Edge DAPL-06 concurrency: another actor holds the lock.
    let run = scenario_competing_lock().await;
    let rig = &run.rig;
    let err = run.error();
    assert_eq!(
        (err.code(), err.exit_code()),
        ("scan_lock_held", 6),
        "{err}"
    );
    assert_eq!(collection_state(rig), run.before);
    assert!(!rig.deploy_dir().join("journal.json").exists());
    assert!(names(&rig.deploy_dir().join("staging")).is_empty());
    assert!(names(&rig.deploy_dir().join("snapshots")).is_empty());
    assert_eq!(rig.fake.scan_count(), 0);
    assert_eq!(
        rig.host.calls(),
        ["probe", "prepare", "discard_interrupted"]
    );
    assert_eq!(failed_error_code(rig), "scan_lock_held");
}

#[tokio::test]
async fn auto_rollback_verify_red_restores() {
    let run = scenario_verify_red(|_| None).await;
    let result = run.rolled_back("verify_failed");
    let rig = &run.rig;
    let run_id = result.run_id.clone().expect("run id");
    assert!(!rig.deploy_dir().join("journal.json").exists());
    assert_eq!(rig.record(&run_id).state, RecordState::RolledBack);
    assert_eq!(
        rig.fake.scan_count(),
        2,
        "the apply's scan and the restore's"
    );
    assert_eq!(
        rig.fake.lock_acquires(),
        2,
        "the apply's scan released the lock"
    );

    let run_dir = rig.run_dir(result);
    let events = event_names(&run_dir);
    assert_eq!(
        events,
        [
            "run_started",
            "gwbk_downloaded",
            "staged",
            "lock_acquired",
            "committed",
            "lock_verdict",
            "scan_done",
            "verify_done",
            "verify_checks",
            "restore_started",
            "lock_acquired",
            "scan_done",
            "restored"
        ]
    );
    // The verdict's checks are in the log by id and status, nothing else.
    let log = std::fs::read_to_string(run_dir.join("events.ndjson")).expect("events");
    let checks: Value = log
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).expect("line"))
        .find(|v| v["event"] == "verify_checks")
        .expect("verify_checks");
    let listed: Vec<(&str, &str)> = checks["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .map(|c| {
            (
                c["id"].as_str().expect("id"),
                c["status"].as_str().expect("status"),
            )
        })
        .collect();
    assert!(listed.contains(&("loaded", "fail")), "{listed:?}");
    assert!(
        !log.contains("UUID attribute"),
        "no gateway text in the log"
    );
    let doc = json_file(&run_dir.join("result.json"));
    assert_eq!(doc["cause"], "verify_failed");
}

#[tokio::test]
async fn auto_rollback_restore_failure_is_rollback_failed() {
    let run = scenario_restore_fails().await;
    let rig = &run.rig;
    let err = run.error();
    let CoreError::RollbackFailed(failure) = err else {
        panic!("expected rollback_failed, got {err:?}");
    };
    assert_eq!(err.exit_code(), 6);
    assert!(!failure.run_id.is_empty());
    let run_dir = failure.run_dir.as_deref().expect("a run directory");
    assert!(Path::new(run_dir).is_dir(), "{run_dir}");
    assert_eq!(
        failure.journal,
        format!("ign-deploy/{GATEWAY}/{MODE}/journal.json")
    );
    let text = err.to_string();
    for part in [failure.run_id.as_str(), run_dir, failure.journal.as_str()] {
        assert!(text.contains(part), "{text}");
    }
    assert!(
        rig.deploy_dir().join("journal.json").exists(),
        "the journal stays"
    );
    assert_ne!(collection_state(rig), run.before, "nothing was put back");
    assert_eq!(failed_error_code(rig), "rollback_failed");
}

#[tokio::test]
async fn auto_rollback_lock_taken_is_rollback_failed() {
    // Edge DAPL-07 concurrency.
    let run = scenario_lock_taken().await;
    let rig = &run.rig;
    let err = run.error();
    let CoreError::RollbackFailed(failure) = err else {
        panic!("expected rollback_failed, got {err:?}");
    };
    assert!(
        failure.cause.contains("scan_lock_held"),
        "{}",
        failure.cause
    );
    assert!(rig.deploy_dir().join("journal.json").exists());
    assert_eq!(rig.fake.scan_count(), 0, "no scan over the partial tree");
    assert!(
        !rig.host.calls().contains(&"restore"),
        "the restore never ran"
    );
    assert_eq!(failed_error_code(rig), "rollback_failed");
}

#[tokio::test]
async fn auto_rollback_restore_check_mismatch_is_rollback_failed() {
    let run = scenario_restore_check_mismatch().await;
    let rig = &run.rig;
    let err = run.error();
    let CoreError::RollbackFailed(failure) = err else {
        panic!("expected rollback_failed, got {err:?}");
    };
    assert!(
        failure.cause.contains("differ from the snapshot"),
        "{}",
        failure.cause
    );
    assert!(failure.cause.contains(DB_CONFIG), "{}", failure.cause);
    assert!(rig.deploy_dir().join("journal.json").exists());
    assert_eq!(failed_error_code(rig), "rollback_failed");
}

#[tokio::test]
async fn never_applied_when_red() {
    // The prohibition: whatever goes wrong, the answer is never `applied`,
    // never `ok`, never exit 0.
    let (stale, _, _) = scenario_pre_image_changed().await;
    let runs = [
        scenario_write_failed().await,
        scenario_lock_expired().await,
        scenario_local_clock_expired().await,
        scenario_short_remaining().await,
        scenario_scan_incomplete().await,
        stale,
        scenario_competing_lock().await,
        scenario_verify_red(|_| None).await,
        scenario_restore_fails().await,
        scenario_lock_taken().await,
        scenario_restore_check_mismatch().await,
    ];
    assert_eq!(runs.len(), 11);
    for (index, run) in runs.iter().enumerate() {
        match &run.result {
            Ok(result) => {
                assert_ne!(result.outcome, OutcomeKind::Applied, "scenario {index}");
                assert!(!result.ok, "scenario {index}");
                assert_eq!(result.failure_exit_code(), Some(6), "scenario {index}");
            }
            Err(e) => assert_eq!(e.exit_code(), 6, "scenario {index}: {e}"),
        }
        // Whatever the run dir holds, its result.json says the same.
        let root = run.rig.repo.join(".ign/deploy").join(GATEWAY);
        for dir in names(&root) {
            let doc = json_file(&root.join(dir).join("result.json"));
            assert_eq!(doc["ok"], false, "scenario {index}: {doc}");
            assert_ne!(doc["outcome"], "applied", "scenario {index}");
        }
    }
}

// ============================================================================
// The preflights (23-07)
// ============================================================================

/// The run directory the one run of `rig` made.
fn only_run_dir(rig: &Rig) -> PathBuf {
    let gateway_dir = rig.repo.join(".ign").join("deploy").join(GATEWAY);
    let runs = names(&gateway_dir);
    assert_eq!(runs.len(), 1, "one run directory, got {runs:?}");
    gateway_dir.join(&runs[0])
}

/// Every file below `dir`, as paths relative to it (empty when `dir` is
/// absent). Directories are not listed.
fn files_below(dir: &Path) -> Vec<String> {
    fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) {
        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in read.filter_map(Result::ok) {
            let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
            if entry.path().is_dir() {
                walk(&entry.path(), &format!("{name}/"), out);
            } else {
                out.push(name);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, "", &mut out);
    out.sort();
    out
}

/// Mounts the designers list answering `sessions`, ahead of the default
/// empty one.
async fn mount_designers(rig: &Rig, sessions: &[(&str, &str, &str)]) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/designers"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(designers_body(sessions)))
        .mount(&rig._server)
        .await;
}

#[tokio::test]
async fn preflight_mode_and_designers_reported_together() {
    let rig = changing_rig().await;
    // The master runs another mode than the target, and a Designer is open.
    rig.fake.set_deployment_mode(Some("production".to_string()));
    mount_designers(&rig, &[("alice", "PlantA", "10.0.0.5:52526")]).await;
    let ledger_before = files_below(&rig.fx.data_dir().join("ign-deploy"));

    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let err = rig
        .run(prepared, &NoObserver)
        .await
        .expect_err("two failed checks refuse");

    assert_eq!(
        (err.code(), err.exit_code()),
        ("deploy_preflight_failed", 6),
        "{err}"
    );
    let CoreError::DeployPreflightFailed { failures } = &err else {
        panic!("expected DeployPreflightFailed, got {err:?}");
    };
    let checks: Vec<&str> = failures.iter().map(|f| f.check.as_str()).collect();
    assert_eq!(checks, ["mode_master", "designers"], "fixed order");
    let shown = err.to_string();
    assert!(
        shown.find("mode_master") < shown.find("designers"),
        "{shown}"
    );
    for part in ["alice", "PlantA", "10.0.0.5:52526"] {
        assert!(shown.contains(part), "{part} in {shown}");
    }

    // Nothing was staged, locked, scanned or written.
    assert_eq!(rig.host.calls(), ["probe"]);
    assert_eq!(rig.fake.lock_acquires(), 0);
    assert_eq!(rig.fake.scan_count(), 0);
    assert_eq!(
        files_below(&rig.fx.data_dir().join("ign-deploy")),
        ledger_before,
        "the probe leaves empty directories and no new file"
    );

    // The run directory records the refusal.
    let run_dir = only_run_dir(&rig);
    let result = json_file(&run_dir.join("result.json"));
    assert_eq!(result["ok"], false);
    assert_eq!(result["error"]["code"], "deploy_preflight_failed");
    let run = json_file(&run_dir.join("run.json"));
    assert_eq!(run["designers"], "fail");
    assert_eq!(
        run["designers_seen"],
        json!([{"user": "alice", "project": "PlantA", "host": "10.0.0.5:52526"}])
    );
    assert!(run_dir.join("events.ndjson").exists());
}

/// Gives the target a backup node whose gateway runs `mode` and reports
/// itself the `Cold` standby (the settled pair of C-S3).
async fn with_backup(rig: &mut Rig, mode: &str) {
    let mut backup = rig.declared.target.nodes.master.clone();
    backup.profile = "backup".to_string();
    backup.local = None;
    backup.gateway_name = None;
    rig.declared.target.nodes.backup = Some(backup);
    let gateway = FakeGateway::new();
    gateway.set_deployment_mode(Some(mode.to_string()));
    rig.backup = Some(Box::new(gateway.client("backup")));
    rig.backup_fake = Some(gateway);
    let server = wiremock::MockServer::start().await;
    mount_redundancy(&server, "Backup", "Cold", 250).await;
    rig.backup_api = Some(ReqwestGatewayApi::for_tests(&server.uri(), None));
    rig.backup_server = Some(server);
}

/// The gateway-info answer names the gateway `name`.
async fn mount_gateway_name(rig: &Rig, name: &str) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/gateway-info"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(json!({"name": name, "ignitionVersion": "8.3.6 (b2026042713)"})),
        )
        .mount(&rig._server)
        .await;
}

/// The project-scan write probe answers `status` as the gateway's error page.
async fn mount_scan_projects(rig: &Rig, status: u16) {
    let route = "/data/api/v1/scan/projects";
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(route))
        .respond_with(wiremock::ResponseTemplate::new(status).set_body_raw(
            common::jetty_error_html(status, route),
            "text/html;charset=iso-8859-1",
        ))
        .mount(&rig._server)
        .await;
}

/// A refused run only ever probed the host: nothing was staged or committed.
fn assert_only_probes(rig: &Rig) {
    let calls = rig.host.calls();
    assert!(
        !calls.is_empty() && calls.iter().all(|c| *c == "probe"),
        "only probe calls, got {calls:?}"
    );
}

/// Runs a prepared apply and returns the preflight refusal's failed check
/// ids, in the order the refusal lists them.
async fn refused_checks(rig: &Rig) -> Vec<String> {
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let err = rig
        .run(prepared, &NoObserver)
        .await
        .expect_err("the preflights refuse");
    let CoreError::DeployPreflightFailed { failures } = &err else {
        panic!("expected DeployPreflightFailed, got {err:?}");
    };
    failures.iter().map(|f| f.check.clone()).collect()
}

/// The refusal's detail for `check`.
async fn refusal_detail(rig: &Rig, check: &str) -> String {
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let err = rig
        .run(prepared, &NoObserver)
        .await
        .expect_err("the preflights refuse");
    let CoreError::DeployPreflightFailed { failures } = &err else {
        panic!("expected DeployPreflightFailed, got {err:?}");
    };
    failures
        .iter()
        .find(|f| f.check == check)
        .unwrap_or_else(|| panic!("no failure for {check} in {err}"))
        .detail
        .clone()
}

/// The preflights alone, over `rig`'s parts and the options' hold.
async fn preflights(
    rig: &Rig,
    opts: &ApplyOptions,
    touched_files: usize,
) -> Result<PreflightReport, CoreError> {
    let deploy = rig.fake.client("apply");
    let backup = rig.backup_node();
    let mut steps = Steps::new(None, &NoObserver);
    run_preflights(
        &PreflightInput {
            target: &rig.declared.target,
            declared: Some(&rig.declared),
            master: &deploy,
            backup: backup.as_ref(),
            gateway: &rig.api,
            reader: rig.reader.as_ref(),
            host: &rig.host,
            touched_files,
            touched_paths: &[],
            hold: &opts.hold,
            designers: &opts.designers,
            clock: &SystemClock,
        },
        &mut steps,
    )
    .await
}

/// A host with a limit of its own on touched paths (the PowerShell host's
/// reachable snapshot length, 23-08): it forwards every call to `inner` and
/// refuses, in `probe_paths`, a path that holds `TOO-LONG`.
struct LimitedPaths<'a> {
    inner: &'a dyn ApplyHost,
}

#[async_trait::async_trait]
impl ApplyHost for LimitedPaths<'_> {
    async fn probe(&self) -> Result<ProbeReport, CoreError> {
        self.inner.probe().await
    }
    async fn probe_paths(&self, touched: &[String]) -> Result<ProbeReport, CoreError> {
        let problems: Vec<String> = touched
            .iter()
            .filter(|p| p.contains("TOO-LONG"))
            .map(|p| format!("{p} is too long to copy below the account home"))
            .collect();
        if problems.is_empty() {
            return self.inner.probe().await;
        }
        Ok(ProbeReport {
            writable: false,
            problems,
        })
    }
    async fn prepare(&self, spec: &BatchSpec, tree: &RenderedTree) -> Result<Prepared, CoreError> {
        self.inner.prepare(spec, tree).await
    }
    async fn commit(&self, prepared: &Prepared) -> Result<BatchReport, CoreError> {
        self.inner.commit(prepared).await
    }
    async fn restore(&self, record: &RunRecord) -> Result<BatchReport, CoreError> {
        self.inner.restore(record).await
    }
    async fn fingerprint(
        &self,
        paths: &[String],
    ) -> Result<BTreeMap<String, Option<String>>, CoreError> {
        self.inner.fingerprint(paths).await
    }
    async fn finalize(&self, record: &RunRecord) -> Result<(), CoreError> {
        self.inner.finalize(record).await
    }
    async fn read_record(&self, run_id: &RunId) -> Result<Option<RunRecord>, CoreError> {
        self.inner.read_record(run_id).await
    }
    async fn fetch_snapshot(
        &self,
        run_id: &RunId,
        dest: &Path,
    ) -> Result<BTreeMap<String, String>, CoreError> {
        self.inner.fetch_snapshot(run_id, dest).await
    }
    async fn discard_interrupted(&self, run_id: &RunId) -> Result<(), CoreError> {
        self.inner.discard_interrupted(run_id).await
    }
}

/// 23-08: the paths a run touches reach the host before anything is written, so
/// a host can refuse one it could not read back; the refusal is the
/// `write_host` finding with the host's own words, and a run that touches only
/// fine paths passes.
#[tokio::test]
async fn preflight_write_host_reports_a_path_the_host_cannot_take() {
    let rig = changing_rig().await;
    let deploy = rig.fake.client("apply");
    let limited = LimitedPaths { inner: &rig.host };
    let run = |paths: Vec<String>| {
        let (rig, deploy, limited) = (&rig, &deploy, &limited);
        async move {
            let opts = options();
            let mut steps = Steps::new(None, &NoObserver);
            run_preflights(
                &PreflightInput {
                    target: &rig.declared.target,
                    declared: Some(&rig.declared),
                    master: deploy,
                    backup: None,
                    gateway: &rig.api,
                    reader: rig.reader.as_ref(),
                    host: limited,
                    touched_files: paths.len(),
                    touched_paths: &paths,
                    hold: &opts.hold,
                    designers: &opts.designers,
                    clock: &SystemClock,
                },
                &mut steps,
            )
            .await
            .expect("the preflights answer")
        }
    };
    let bad = "ignition/database-connection/TOO-LONG/config.json".to_string();
    let report = run(vec!["ignition/a/b/config.json".to_string(), bad.clone()]).await;
    let failures = report.failures();
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert_eq!(failures[0].check, "write_host");
    assert!(
        failures[0].detail.starts_with(&bad),
        "{}",
        failures[0].detail
    );
    let report = run(vec!["ignition/a/b/config.json".to_string()]).await;
    assert!(report.passed(), "{:?}", report.failures());
}

#[tokio::test]
async fn preflight_backup_mode_mismatch() {
    let mut rig = changing_rig().await;
    with_backup(&mut rig, "production").await;
    assert_eq!(refused_checks(&rig).await, ["mode_backup"]);
    let detail = refusal_detail(&rig, "mode_backup").await;
    assert!(
        detail.contains("backup") && detail.contains("production"),
        "{detail}"
    );
    assert_eq!(
        rig.host.calls().iter().filter(|c| **c == "prepare").count(),
        0
    );
}

#[tokio::test]
async fn preflight_backup_mode_matching_passes() {
    let mut rig = changing_rig().await;
    with_backup(&mut rig, MODE).await;
    let report = preflights(&rig, &options(), 4).await.expect("preflights");
    assert!(report.passed(), "{report:?}");
    let backup = &report.findings[1];
    assert_eq!(backup.check.as_str(), "mode_backup");
    assert_eq!(backup.status.as_str(), "pass");
}

/// A rig with a backup node, the master and the backup reporting these
/// `activityLevel`s (24-07).
async fn pair_levels(master: &str, backup: &str) -> Rig {
    let mut rig = changing_rig().await;
    with_backup(&mut rig, MODE).await;
    set_levels(&rig, master, backup).await;
    rig
}

/// The master and the backup of `rig` now report these `activityLevel`s.
async fn set_levels(rig: &Rig, master: &str, backup: &str) {
    mount_redundancy(&rig._server, "Master", master, 5).await;
    mount_redundancy(
        rig.backup_server.as_ref().expect("a backup server"),
        "Backup",
        backup,
        5,
    )
    .await;
}

/// One apply run against `rig`: every refusal as `(check, detail)`.
async fn refusals(rig: &Rig) -> Vec<(String, String)> {
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let err = rig
        .run(prepared, &NoObserver)
        .await
        .expect_err("the preflights refuse");
    let CoreError::DeployPreflightFailed { failures } = &err else {
        panic!("expected DeployPreflightFailed, got {err:?}");
    };
    failures
        .iter()
        .map(|f| (f.check.clone(), f.detail.clone()))
        .collect()
}

/// The status `run.json` records for `check`.
fn run_status(run: &Value, check: &str) -> String {
    run["preflights"]
        .as_array()
        .expect("a preflights list")
        .iter()
        .find(|entry| entry["check"] == check)
        .unwrap_or_else(|| panic!("no {check} in {run}"))["status"]
        .as_str()
        .expect("a status")
        .to_string()
}

/// The finding for `check` in `report`.
fn finding_of<'a>(
    report: &'a PreflightReport,
    check: &str,
) -> &'a ignition_core::deploy::apply::preflight::PreflightFinding {
    report
        .findings
        .iter()
        .find(|f| f.check.as_str() == check)
        .unwrap_or_else(|| panic!("no finding for {check}: {report:?}"))
}

#[tokio::test]
async fn preflight_master_active_refuses_after_failover() {
    // C-S3 post-failover: the master is `Cold` and the backup took over.
    let rig = pair_levels("Cold", "Active").await;
    let ledger_before = files_below(&rig.fx.data_dir().join("ign-deploy"));
    let master_before = rig.fx.read(MODE, DB_CONFIG);

    let failed = refusals(&rig).await;

    assert_eq!(failed.len(), 1, "{failed:?}");
    let (check, detail) = &failed[0];
    assert_eq!(check, "master_active");
    for part in ["backup", "Active", "master", "Cold", "switched over"] {
        assert!(detail.contains(part), "{part} in {detail}");
    }
    // Nothing was staged, locked, scanned or written, on either node.
    assert_eq!(rig.host.calls(), ["probe"]);
    assert_eq!(rig.fake.lock_acquires(), 0);
    assert_eq!(rig.fake.scan_count(), 0);
    assert_eq!(rig.fx.read(MODE, DB_CONFIG), master_before);
    assert_eq!(
        files_below(&rig.fx.data_dir().join("ign-deploy")),
        ledger_before
    );
    // run.json records the refusal, and the verdict as a fail.
    let run = json_file(&only_run_dir(&rig).join("run.json"));
    assert_eq!(run_status(&run, "master_active"), "fail");
}

#[tokio::test]
async fn preflight_master_active_refuses_split_brain_and_neither() {
    let rig = pair_levels("Active", "Active").await;
    let failed = refusals(&rig).await;
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert_eq!(failed[0].0, "master_active");
    assert!(failed[0].1.contains("split-brain"), "{}", failed[0].1);
    assert_eq!(rig.fake.lock_acquires(), 0);
    assert_eq!(rig.host.calls(), ["probe"]);

    let rig = pair_levels("Cold", "Cold").await;
    let failed = refusals(&rig).await;
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert_eq!(failed[0].0, "master_active");
    assert!(
        failed[0]
            .1
            .contains("neither node reports activityLevel Active"),
        "{}",
        failed[0].1
    );
    assert_eq!(rig.fake.lock_acquires(), 0);
    assert_eq!(rig.host.calls(), ["probe"]);
}

#[tokio::test]
async fn preflight_master_active_passes_a_cold_backup() {
    // Pitfall 5: a healthy standby reports `Cold` and `uptime: -1`.
    let rig = pair_levels("Active", "Cold").await;
    let report = preflights(&rig, &options(), 4).await.expect("preflights");
    assert!(report.passed(), "{report:?}");
    assert_eq!(finding_of(&report, "master_active").status.as_str(), "pass");
}

#[tokio::test]
async fn preflight_master_active_skipped_without_backup() {
    let rig = changing_rig().await;
    let report = preflights(&rig, &options(), 4).await.expect("preflights");
    let found = finding_of(&report, "master_active");
    assert_eq!(found.status.as_str(), "skipped", "{found:?}");
    // No node was asked.
    let asked = rig
        ._server
        .received_requests()
        .await
        .expect("requests are recorded")
        .iter()
        .filter(|r| r.url.path() == "/data/api/v1/redundancy")
        .count();
    assert_eq!(asked, 0);
}

#[tokio::test]
async fn rollback_runs_master_active() {
    let rig = changing_rig().await;
    let target_run = apply_once(&rig).await;
    let after_apply = collection_state(&rig);
    let fresh = Fresh::new(&rig).await;
    // The pair failed over after the apply: the master is the standby now.
    mount_redundancy(&fresh.server, "Master", "Cold", 5).await;
    let backup_server = wiremock::MockServer::start().await;
    mount_redundancy(&backup_server, "Backup", "Active", 5).await;
    let backup_gateway = FakeGateway::new();
    backup_gateway.set_deployment_mode(Some(MODE.to_string()));
    let backup_deploy = backup_gateway.client("backup");
    let backup_api = ReqwestGatewayApi::for_tests(&backup_server.uri(), None);
    let backup = BackupNode::new(&backup_api, &backup_deploy, None);
    let mut target = fresh.target.clone();
    let mut node = target.nodes.master.clone();
    node.profile = "backup".to_string();
    node.local = None;
    node.gateway_name = None;
    target.nodes.backup = Some(node);

    let prepared = fresh
        .prepare(&rig, &target_run, false)
        .await
        .expect("prepare");
    let deploy = fresh.fake.client("rollback");
    let recorder = Recorder::default();
    let ctx = RollbackContext {
        env: "dev",
        repo: &rig.repo,
        target_file: &fresh.target_file,
        target: &target,
        reader: fresh.reader.as_ref(),
        host: &fresh.host,
        deploy: &deploy,
        gateway: &fresh.api,
        backup: Some(&backup),
        clock: &SystemClock,
        observer: &recorder,
    };
    let err = deploy_rollback_execute(prepared, &ctx, &rollback_options(&target_run, false))
        .await
        .expect_err("a rollback to a standby master is refused");

    let CoreError::DeployPreflightFailed { failures } = &err else {
        panic!("expected DeployPreflightFailed, got {err:?}");
    };
    let checks: Vec<&str> = failures.iter().map(|f| f.check.as_str()).collect();
    assert_eq!(checks, ["master_active"], "{err}");
    // The two checks that need a rendered tree are skipped, never passed.
    let reported: Vec<(String, String)> = recorder
        .0
        .lock()
        .expect("events")
        .iter()
        .find_map(|e| match e {
            ApplyEvent::PreflightChecks { checks } => Some(checks.clone()),
            _ => None,
        })
        .expect("the preflight verdicts were reported");
    let status = |id: &str| {
        reported
            .iter()
            .find(|(check, _)| check == id)
            .unwrap_or_else(|| panic!("no {id} in {reported:?}"))
            .1
            .clone()
    };
    assert_eq!(status("master_active"), "fail");
    assert_eq!(status("backup_read"), "skipped");
    assert_eq!(status("secrets"), "skipped");
    assert_eq!(
        collection_state(&rig),
        after_apply,
        "nothing was written to the host"
    );
    assert_eq!(fresh.fake.lock_acquires(), 0);
    assert_eq!(fresh.fake.scan_count(), 0);
}

// ---- backup_read, secrets (24-07) -------------------------------------------

/// The value the fake keeps beside a secret name; no output may carry it.
const SECRET_CANARY: &str = "canary-secret-value-24-07";

/// The backup gets a data directory holding the live render and a local file
/// transport over it. The fixture is the caller's to keep alive.
fn backup_files(rig: &mut Rig) -> PlanFixture {
    let fx = PlanFixture::new();
    fx.live_render(MODE, &rig.declared);
    rig.backup_reader = Some(LocalFsTransport::new(
        fx.data_dir().to_path_buf(),
        "nodes.backup",
    ));
    fx
}

/// Takes every right on the backup's mode collection; true when the process
/// is held to it (a process that ignores permission bits, root, is not).
#[cfg(unix)]
fn lock_collection(fx: &PlanFixture) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(
        fx.collection_dir(MODE),
        std::fs::Permissions::from_mode(0o000),
    )
    .expect("chmod 0000");
    std::fs::read_dir(fx.collection_dir(MODE)).is_err()
}

#[cfg(unix)]
fn unlock_collection(fx: &PlanFixture) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(
        fx.collection_dir(MODE),
        std::fs::Permissions::from_mode(0o755),
    )
    .expect("chmod back");
}

/// The render references the secret `provider`/`name` (the shape the lint
/// accepts): one more config file in a manifest type.
fn reference_secret(rig: &mut Rig, provider: &str, name: &str) {
    let body = json!({
        "password": {"type": "Referenced", "data": {"providerName": provider, "secretName": name}}
    });
    let path = format!("ignition/database-translator/REF-{provider}-{name}/config.json");
    rig.declared.outcome.tree.files.insert(
        ignition_core::deploy::layout::RelPath::new(&path).expect("path"),
        body.to_string().into_bytes(),
    );
}

/// The render itself declares the secret provider `provider`.
fn declare_secret_provider(rig: &mut Rig, provider: &str) {
    let path = format!("ignition/secret-provider/{provider}/config.json");
    rig.declared.outcome.tree.files.insert(
        ignition_core::deploy::layout::RelPath::new(&path).expect("path"),
        b"{}".to_vec(),
    );
}

/// Every byte the run directory holds, as text.
fn run_dir_text(rig: &Rig) -> String {
    let mut out = String::new();
    for entry in std::fs::read_dir(only_run_dir(rig)).expect("run dir") {
        let path = entry.expect("entry").path();
        if path.is_file() {
            out.push_str(&String::from_utf8_lossy(
                &std::fs::read(path).expect("file"),
            ));
        }
    }
    out
}

#[cfg(unix)]
#[tokio::test]
async fn preflight_backup_read_fails_on_an_unreadable_backup() {
    let mut rig = changing_rig().await;
    with_backup(&mut rig, MODE).await;
    let fx = backup_files(&mut rig);

    // Readable: the check passes and names what it read.
    let report = preflights(&rig, &options(), 4).await.expect("preflights");
    assert!(report.passed(), "{report:?}");
    let found = finding_of(&report, "backup_read");
    assert_eq!(found.status.as_str(), "pass", "{found:?}");
    assert!(
        found.detail.starts_with("read ")
            && found
                .detail
                .contains(" files of the backup's mode collection"),
        "{}",
        found.detail
    );

    // Unreadable: the account's read right is what is checked.
    let held = lock_collection(&fx);
    let failed = if held {
        Some(refusals(&rig).await)
    } else {
        None
    };
    unlock_collection(&fx);
    let Some(failed) = failed else { return };
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert_eq!(failed[0].0, "backup_read");
    assert!(
        failed[0].1.contains("transport_failed"),
        "the detail names the error code: {}",
        failed[0].1
    );
    // Refused before the lock: only probes reached the host (the passing
    // preflights above and the refused run), nothing staged, locked or written.
    assert_only_probes(&rig);
    assert_eq!(rig.fake.lock_acquires(), 0);
    let run = json_file(&only_run_dir(&rig).join("run.json"));
    assert_eq!(run_status(&run, "backup_read"), "fail");
}

#[tokio::test]
async fn preflight_backup_read_skipped_rest_only() {
    let mut rig = changing_rig().await;
    with_backup(&mut rig, MODE).await;
    assert!(rig.backup_reader.is_none());
    let report = preflights(&rig, &options(), 4).await.expect("preflights");
    let found = finding_of(&report, "backup_read");
    assert_eq!(found.status.as_str(), "skipped", "{found:?}");
    assert!(
        found.detail.contains("the file-hash check did not run"),
        "{}",
        found.detail
    );
}

#[tokio::test]
async fn preflight_backup_read_skipped_without_a_backup_node() {
    let rig = changing_rig().await;
    let report = preflights(&rig, &options(), 4).await.expect("preflights");
    assert_eq!(
        finding_of(&report, "backup_read").status.as_str(),
        "skipped"
    );
}

#[tokio::test]
async fn preflight_secrets_missing_on_backup() {
    let mut rig = changing_rig().await;
    with_backup(&mut rig, MODE).await;
    reference_secret(&mut rig, "cap24-file", "CAP24-Name");
    rig.fake
        .seed_secret_with_value("cap24-file", "CAP24-Name", SECRET_CANARY);
    rig.backup_fake
        .as_ref()
        .expect("a backup gateway")
        .seed_secret_names("cap24-file", &[]);

    let failed = refusals(&rig).await;

    assert_eq!(failed.len(), 1, "{failed:?}");
    assert_eq!(failed[0].0, "secrets");
    assert!(
        failed[0].1.contains("backup: cap24-file/CAP24-Name"),
        "{}",
        failed[0].1
    );
    assert!(!failed[0].1.contains("master:"), "{}", failed[0].1);
    // Names only: the value stored beside the name is in no output.
    assert!(!failed[0].1.contains(SECRET_CANARY));
    let text = run_dir_text(&rig);
    assert!(
        !text.contains(SECRET_CANARY),
        "a secret value reached run.json"
    );
    assert_eq!(rig.fake.lock_acquires(), 0);
    assert_eq!(rig.host.calls(), ["probe"]);
    let run = json_file(&only_run_dir(&rig).join("run.json"));
    assert_eq!(run_status(&run, "secrets"), "fail");
}

#[tokio::test]
async fn preflight_secrets_missing_on_master() {
    let mut rig = changing_rig().await;
    with_backup(&mut rig, MODE).await;
    reference_secret(&mut rig, "cap24-file", "CAP24-Name");
    rig.fake.seed_secret_names("cap24-file", &[]);
    rig.backup_fake
        .as_ref()
        .expect("a backup gateway")
        .seed_secret_names("cap24-file", &["CAP24-Name"]);

    let failed = refusals(&rig).await;

    assert_eq!(failed.len(), 1, "{failed:?}");
    assert_eq!(failed[0].0, "secrets");
    assert!(
        failed[0].1.contains("master: cap24-file/CAP24-Name"),
        "{}",
        failed[0].1
    );
    assert!(!failed[0].1.contains("backup:"), "{}", failed[0].1);
}

#[tokio::test]
async fn preflight_secrets_without_a_backup_node_check_the_master_only() {
    let mut rig = changing_rig().await;
    reference_secret(&mut rig, "cap24-file", "CAP24-Name");
    rig.fake.seed_secret_names("cap24-file", &["CAP24-Name"]);
    let report = preflights(&rig, &options(), 4).await.expect("preflights");
    assert!(report.passed(), "{report:?}");
    assert_eq!(finding_of(&report, "secrets").status.as_str(), "pass");

    rig.fake.seed_secret_names("cap24-file", &[]);
    let report = preflights(&rig, &options(), 4).await.expect("preflights");
    let found = finding_of(&report, "secrets");
    assert_eq!(found.status.as_str(), "fail", "{found:?}");
    assert!(found.detail.contains("master: cap24-file/CAP24-Name"));
}

#[tokio::test]
async fn preflight_secrets_names_are_case_sensitive() {
    // K9: `CAP24-Name` and `cap24-name` are two names.
    let mut rig = changing_rig().await;
    with_backup(&mut rig, MODE).await;
    reference_secret(&mut rig, "cap24-file", "CAP24-Name");
    rig.fake.seed_secret_names("cap24-file", &["CAP24-Name"]);
    rig.backup_fake
        .as_ref()
        .expect("a backup gateway")
        .seed_secret_names("cap24-file", &["cap24-name"]);
    let report = preflights(&rig, &options(), 4).await.expect("preflights");
    let found = finding_of(&report, "secrets");
    assert_eq!(found.status.as_str(), "fail", "{found:?}");
    assert!(found.detail.contains("backup: cap24-file/CAP24-Name"));
}

#[tokio::test]
async fn preflight_secrets_undeclared_missing_provider_fails() {
    // Only a provider the render itself declares may be absent (K8).
    let mut rig = changing_rig().await;
    with_backup(&mut rig, MODE).await;
    reference_secret(&mut rig, "ghost", "x");
    let report = preflights(&rig, &options(), 4).await.expect("preflights");
    let found = finding_of(&report, "secrets");
    assert_eq!(found.status.as_str(), "fail", "{found:?}");
    assert!(found.detail.contains("master: ghost/x"), "{}", found.detail);
    assert!(found.detail.contains("backup: ghost/x"), "{}", found.detail);
}

#[tokio::test]
async fn preflight_secrets_provider_created_by_this_deploy() {
    // K8 a: the render declares `newp`, so both nodes answering 404 for it is
    // the first-deploy case; it is skipped by name, not refused.
    let mut rig = changing_rig().await;
    with_backup(&mut rig, MODE).await;
    declare_secret_provider(&mut rig, "newp");
    reference_secret(&mut rig, "newp", "x");
    let report = preflights(&rig, &options(), 4).await.expect("preflights");
    assert!(report.passed(), "{report:?}");
    let found = finding_of(&report, "secrets");
    assert_eq!(found.status.as_str(), "pass", "{found:?}");
    assert!(
        found.detail.contains("newp") && found.detail.contains("skipped"),
        "{}",
        found.detail
    );
}

#[tokio::test]
async fn preflight_secrets_cannot_list() {
    // K7 a: a provider that cannot list (501, or the 500 of a `remote`
    // provider) is named unverifiable and does not fail.
    let mut rig = changing_rig().await;
    with_backup(&mut rig, MODE).await;
    reference_secret(&mut rig, "remote-1", "x");
    rig.fake.seed_secret_names("remote-1", &["x"]);
    rig.backup_fake
        .as_ref()
        .expect("a backup gateway")
        .secret_provider_cannot_list("remote-1");
    let report = preflights(&rig, &options(), 4).await.expect("preflights");
    assert!(report.passed(), "{report:?}");
    let found = finding_of(&report, "secrets");
    assert_eq!(found.status.as_str(), "pass", "{found:?}");
    assert!(
        found.detail.contains("unverifiable") && found.detail.contains("remote-1"),
        "{}",
        found.detail
    );
}

#[tokio::test]
async fn preflight_secrets_skipped_without_references() {
    let mut rig = changing_rig().await;
    with_backup(&mut rig, MODE).await;
    let report = preflights(&rig, &options(), 4).await.expect("preflights");
    let found = finding_of(&report, "secrets");
    assert_eq!(found.status.as_str(), "skipped", "{found:?}");
    // No provider was asked.
    let asked = rig
        ._server
        .received_requests()
        .await
        .expect("requests are recorded")
        .iter()
        .filter(|r| r.url.path().contains("secret-providers"))
        .count();
    assert_eq!(asked, 0);
}

#[tokio::test]
async fn preflight_secrets_listing_that_cannot_be_answered_aborts() {
    // A listing that fails on the wire is its own error after run.json is
    // written, as the Phase 23 preflights do; it is not a failed check.
    let mut rig = changing_rig().await;
    reference_secret(&mut rig, "cap24-file", "CAP24-Name");
    rig.faults = DeployFaults::default().fail("secret_names", 1);
    rig.faults.arm();
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let err = rig
        .run(prepared, &NoObserver)
        .await
        .expect_err("an unanswerable listing aborts");
    assert_eq!((err.code(), err.exit_code()), ("network_error", 4), "{err}");
    assert_eq!(rig.fake.lock_acquires(), 0);
    assert_eq!(rig.faults.failed("secret_names"), 1);
    assert!(only_run_dir(&rig).join("run.json").exists());
}

#[tokio::test]
async fn preflight_no_backup_node_is_skipped_not_passed() {
    // Edge DAPL-04 empty: no backup node means `skipped`, never `pass`; no
    // open Designer session passes in every mode.
    let rig = changing_rig().await;
    let report = preflights(&rig, &options(), 4).await.expect("preflights");
    assert!(report.passed(), "{report:?}");
    let words: Vec<(&str, &str)> = report
        .findings
        .iter()
        .map(|f| (f.check.as_str(), f.status.as_str()))
        .collect();
    assert_eq!(
        words,
        [
            ("mode_master", "pass"),
            ("mode_backup", "skipped"),
            ("wrong_target", "pass"),
            ("write_rest", "pass"),
            ("write_host", "pass"),
            ("designers", "pass"),
            ("hold_budget", "pass"),
            ("master_active", "skipped"),
            ("backup_read", "skipped"),
            ("secrets", "skipped"),
        ]
    );
    assert!(report.designers_seen.is_empty());

    // The applied run records the same in run.json.
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let result = rig.run(prepared, &NoObserver).await.expect("applied");
    let run = json_file(&rig.run_dir(&result).join("run.json"));
    assert_eq!(run["designers"], "fail");
    assert_eq!(run["designers_seen"], json!([]));
    assert_eq!(
        run["preflights"][1],
        json!({"check": "mode_backup", "status": "skipped"})
    );
}

#[tokio::test]
async fn preflight_wrong_target() {
    // The REST name is not the host's systemName.
    let rig = changing_rig().await;
    mount_gateway_name(&rig, "some-other-gateway").await;
    assert_eq!(refused_checks(&rig).await, ["wrong_target"]);
    let detail = refusal_detail(&rig, "wrong_target").await;
    assert!(
        detail.contains("REST name") && detail.contains("systemName"),
        "{detail}"
    );
    assert!(
        !detail.contains("some-other-gateway") && !detail.contains(RIG_GATEWAY_NAME),
        "the detail names sources, not values: {detail}"
    );
    assert_only_probes(&rig);
    assert_eq!(rig.fake.lock_acquires(), 0);
}

#[tokio::test]
async fn preflight_wrong_target_declared_gateway_name() {
    // REST and host agree, but nodes.master.gateway_name names another gateway.
    let mut rig = changing_rig().await;
    rig.declared.target.nodes.master.gateway_name = Some("the-other-site".to_string());
    assert_eq!(refused_checks(&rig).await, ["wrong_target"]);
    let detail = refusal_detail(&rig, "wrong_target").await;
    assert!(detail.contains("gateway_name"), "{detail}");

    // The same name on both sides passes.
    rig.declared.target.nodes.master.gateway_name = Some(RIG_GATEWAY_NAME.to_string());
    let report = preflights(&rig, &options(), 4).await.expect("preflights");
    assert!(report.passed(), "{report:?}");
}

#[tokio::test]
async fn preflight_wrong_target_host_without_a_system_name() {
    let rig = changing_rig().await;
    rig.fx.remove("core", "ignition/system-properties");
    assert_eq!(refused_checks(&rig).await, ["wrong_target"]);
    let detail = refusal_detail(&rig, "wrong_target").await;
    assert!(detail.contains("unreadable"), "{detail}");
}

#[tokio::test]
async fn preflight_write_rest_403() {
    let rig = changing_rig().await;
    mount_scan_projects(&rig, 403).await;
    assert_eq!(refused_checks(&rig).await, ["write_rest"]);
    let detail = refusal_detail(&rig, "write_rest").await;
    assert!(detail.contains("403"), "{detail}");
}

#[cfg(unix)]
#[tokio::test]
async fn preflight_write_host_unwritable() {
    use std::os::unix::fs::PermissionsExt;
    let rig = changing_rig().await;
    let dir = rig.deploy_dir();
    std::fs::create_dir_all(&dir).expect("ign-deploy dir");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).expect("chmod");
    // A process that ignores permission bits (root) cannot be refused this way.
    let enforced = std::fs::write(dir.join("enforced.tmp"), b"").is_err();
    let checks = if enforced {
        Some(refused_checks(&rig).await)
    } else {
        None
    };
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).expect("chmod back");
    if let Some(checks) = checks {
        assert_eq!(checks, ["write_host"]);
        assert_eq!(rig.fake.lock_acquires(), 0);
        assert_eq!(rig.host.calls(), ["probe"]);
    }
}

#[tokio::test]
async fn preflight_hold_budget_boundary() {
    // Edge DAPL-04 adjacency. need(n files) = ceil(2 x (1 + 0.5 n + 1) + 1).
    let rig = changing_rig().await;
    let model = |hold_s| ApplyOptions {
        hold: HoldModel {
            hold_s,
            commit_fixed_s: 1.0,
            commit_per_file_s: 0.5,
            p95_scan_s: 1.0,
        },
        ..options()
    };
    // Four files need exactly 9 s.
    let report = preflights(&rig, &model(9), 4).await.expect("equal passes");
    assert!(report.passed(), "{report:?}");
    // One more file needs 10 s: one second over.
    let report = preflights(&rig, &model(9), 5).await.expect("preflights");
    let failures = report.failures();
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert_eq!(failures[0].check, "hold_budget");
    let detail = &failures[0].detail;
    assert!(detail.contains("5 file(s)"), "{detail}");
    assert!(detail.contains("estimated commit 3.5 s"), "{detail}");
    assert!(detail.contains("hold of 10 s"), "{detail}");
    assert!(detail.contains("budget is 9 s"), "{detail}");

    // Through the engine the run is refused and never clamped to fit.
    let mut tight = options();
    tight.hold = HoldModel {
        hold_s: 8,
        commit_fixed_s: 1.0,
        commit_per_file_s: 0.5,
        p95_scan_s: 1.0,
    };
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let err = rig
        .run_with(prepared, &NoObserver, &SystemClock, &tight)
        .await
        .expect_err("a hold that is too short refuses");
    assert_eq!(err.code(), "deploy_preflight_failed", "{err}");
    assert_eq!(rig.fake.lock_acquires(), 0);
    assert_only_probes(&rig);
}

#[tokio::test]
async fn preflight_all_fail_in_fixed_order() {
    // Edge DAPL-04 ordering: every check failing at once, in the fixed order.
    let mut rig = changing_rig().await;
    with_backup(&mut rig, "production").await;
    // The backup's files cannot be read, and the render references a secret
    // no node has.
    let backup_fx = backup_files(&mut rig);
    #[cfg(unix)]
    let backup_locked = lock_collection(&backup_fx);
    #[cfg(not(unix))]
    let backup_locked = false;
    reference_secret(&mut rig, "cap24-file", "CAP24-Name");
    rig.fake.set_deployment_mode(Some("production".to_string()));
    mount_gateway_name(&rig, "some-other-gateway").await;
    mount_scan_projects(&rig, 403).await;
    mount_designers(&rig, &[("alice", "PlantA", "10.0.0.5:52526")]).await;
    // The backup reports itself active as well (split-brain).
    set_levels(&rig, "Active", "Active").await;
    let mut opts = options();
    opts.hold.hold_s = 1;

    // The host check needs an unwritable ign-deploy directory.
    #[cfg(unix)]
    let restore = {
        use std::os::unix::fs::PermissionsExt;
        let dir = rig.deploy_dir();
        std::fs::create_dir_all(&dir).expect("ign-deploy dir");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).expect("chmod");
        dir
    };
    let report = preflights(&rig, &opts, 4).await.expect("preflights");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&restore, std::fs::Permissions::from_mode(0o700))
            .expect("chmod back");
        unlock_collection(&backup_fx);
    }
    let failed: Vec<&str> = report
        .findings
        .iter()
        .filter(|f| f.status.as_str() == "fail")
        .map(|f| f.check.as_str())
        .collect();
    let expected = [
        "mode_master",
        "mode_backup",
        "wrong_target",
        "write_rest",
        "write_host",
        "designers",
        "hold_budget",
        "master_active",
        "backup_read",
        "secrets",
    ];
    // Without permission enforcement (root) neither the host check nor the
    // backup's read can fail.
    if backup_locked && failed.len() == expected.len() {
        assert_eq!(failed, expected);
    } else {
        let unenforced: Vec<&str> = expected
            .iter()
            .copied()
            .filter(|c| *c != "write_host" && *c != "backup_read")
            .collect();
        assert_eq!(failed, unenforced);
    }
    let all: Vec<&str> = report.findings.iter().map(|f| f.check.as_str()).collect();
    assert_eq!(all, expected, "one finding per check, in the fixed order");
}

#[tokio::test]
async fn preflight_unreachable_gateway_is_network_error() {
    let mut rig = changing_rig().await;
    // Nothing listens on port 1.
    rig.api = ReqwestGatewayApi::for_tests("http://127.0.0.1:1", None);
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let err = rig
        .run(prepared, &NoObserver)
        .await
        .expect_err("an unreachable gateway aborts");
    assert_eq!((err.code(), err.exit_code()), ("network_error", 4), "{err}");
    assert_eq!(rig.fake.lock_acquires(), 0);
    let run_dir = only_run_dir(&rig);
    let result = json_file(&run_dir.join("result.json"));
    assert_eq!(result["ok"], false);
    assert_eq!(result["error"]["code"], "network_error");
    assert!(run_dir.join("run.json").exists());
}

#[tokio::test]
async fn preflight_unreachable_host_is_transport_failed() {
    let mut rig = changing_rig().await;
    rig.host = rig.host.clone().fail("probe", 1);
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let err = rig
        .run(prepared, &NoObserver)
        .await
        .expect_err("an unreachable host aborts");
    assert_eq!(
        (err.code(), err.exit_code()),
        ("transport_failed", 4),
        "{err}"
    );
    let result = json_file(&only_run_dir(&rig).join("result.json"));
    assert_eq!(result["error"]["code"], "transport_failed");
}

#[tokio::test]
async fn preflight_is_idempotent() {
    // Edge DAPL-04 idempotency: the same failing state gives the same
    // failures, each run writes its own run directory and nothing else.
    let rig = changing_rig().await;
    rig.fake.set_deployment_mode(Some("production".to_string()));
    mount_designers(&rig, &[("alice", "PlantA", "10.0.0.5:52526")]).await;
    let host_before = files_below(rig.fx.data_dir());

    let first = refused_checks(&rig).await;
    let second = refused_checks(&rig).await;
    assert_eq!(first, second);
    assert_eq!(first, ["mode_master", "designers"]);

    assert_eq!(
        files_below(rig.fx.data_dir()),
        host_before,
        "the gateway host is unchanged"
    );
    let runs = names(&rig.repo.join(".ign").join("deploy").join(GATEWAY));
    assert_eq!(runs.len(), 2, "one run directory each: {runs:?}");
    assert_eq!(rig.fake.lock_acquires(), 0);
    assert_eq!(rig.fake.scan_count(), 0);
}

/// `options()` with `mode` and a short wait.
fn designers_options(mode: DesignersMode, timeout_ms: u64) -> ApplyOptions {
    ApplyOptions {
        designers: DesignersPolicy {
            mode,
            timeout: Duration::from_millis(timeout_ms),
            interval: Duration::from_millis(20),
        },
        ..options()
    }
}

/// The lines of `events.ndjson` named `event`.
fn event_lines(run_dir: &Path, event: &str) -> Vec<Value> {
    std::fs::read_to_string(run_dir.join("events.ndjson"))
        .expect("events")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("line"))
        .filter(|line| line["event"] == event)
        .collect()
}

#[tokio::test]
async fn preflight_designers_wait_clears() {
    let rig = changing_rig().await;
    // One session for the first two reads, none after.
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/designers"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(designers_body(&[(
                "alice",
                "PlantA",
                "10.0.0.5:52526",
            )])),
        )
        .up_to_n_times(2)
        .mount(&rig._server)
        .await;

    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let opts = designers_options(DesignersMode::Wait, 30_000);
    let result = rig
        .run_with(prepared, &NoObserver, &SystemClock, &opts)
        .await
        .expect("the wait clears and the apply goes ahead");
    assert_eq!(result.outcome, OutcomeKind::Applied);

    let run_dir = rig.run_dir(&result);
    let polls: Vec<u64> = event_lines(&run_dir, "designers_poll")
        .iter()
        .map(|line| line["open"].as_u64().expect("open"))
        .collect();
    assert_eq!(polls, [1, 1, 0], "one event per read");
    let run = json_file(&run_dir.join("run.json"));
    assert_eq!(run["designers"], "wait");
    assert_eq!(run["designers_seen"], json!([]), "nothing open at the end");
}

#[tokio::test]
async fn preflight_designers_wait_times_out() {
    // Edge DAPL-04 adjacency: the wait ends at the timeout with the same
    // detail `fail` gives.
    let rig = changing_rig().await;
    mount_designers(&rig, &[("alice", "PlantA", "10.0.0.5:52526")]).await;

    let failed = preflights(&rig, &designers_options(DesignersMode::Wait, 200), 4)
        .await
        .expect("preflights")
        .failures();
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert_eq!(failed[0].check, "designers");

    let fail_mode = preflights(&rig, &designers_options(DesignersMode::Fail, 200), 4)
        .await
        .expect("preflights")
        .failures();
    assert_eq!(failed, fail_mode, "wait ends with fail's list");
    assert!(
        failed[0]
            .detail
            .contains("alice on PlantA from 10.0.0.5:52526"),
        "{}",
        failed[0].detail
    );
}

#[tokio::test]
async fn preflight_designers_proceed_records_sessions() {
    let rig = changing_rig().await;
    mount_designers(
        &rig,
        &[
            ("alice", "PlantA", "10.0.0.5:52526"),
            ("bob", "PlantB", "10.0.0.6:50001"),
        ],
    )
    .await;

    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let opts = designers_options(DesignersMode::Proceed, 200);
    let result = rig
        .run_with(prepared, &NoObserver, &SystemClock, &opts)
        .await
        .expect("proceed goes ahead");
    assert_eq!(result.outcome, OutcomeKind::Applied);

    let sessions = json!([
        {"user": "alice", "project": "PlantA", "host": "10.0.0.5:52526"},
        {"user": "bob", "project": "PlantB", "host": "10.0.0.6:50001"},
    ]);
    let run_dir = rig.run_dir(&result);
    let run = json_file(&run_dir.join("run.json"));
    assert_eq!(run["designers"], "proceed");
    assert_eq!(run["designers_seen"], sessions);
    let proceeded = event_lines(&run_dir, "designers_proceed");
    assert_eq!(proceeded.len(), 1, "{proceeded:?}");
    assert_eq!(proceeded[0]["sessions"], sessions);
}

#[tokio::test]
async fn preflight_designers_none_pass_everywhere() {
    // Edge DAPL-04 empty: no open session passes in every mode.
    let rig = changing_rig().await;
    for mode in [
        DesignersMode::Fail,
        DesignersMode::Wait,
        DesignersMode::Proceed,
    ] {
        let report = preflights(&rig, &designers_options(mode, 200), 4)
            .await
            .expect("preflights");
        assert!(report.passed(), "{mode:?}: {report:?}");
        assert!(report.designers_seen.is_empty(), "{mode:?}");
    }
}

// ============================================================================
// The .gwbk before staging (23-09, DAPL-05, D-13)
// ============================================================================

/// The requests the rig's gateway received for `path`.
async fn requests_for(rig: &Rig, route: &str) -> Vec<wiremock::Request> {
    rig._server
        .received_requests()
        .await
        .expect("request recording is on")
        .into_iter()
        .filter(|r| r.url.path() == route)
        .collect()
}

#[tokio::test]
async fn snapshot_gwbk_downloaded_private() {
    let rig = changing_rig().await;
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let result = rig.run(prepared, &NoObserver).await.expect("applied");
    assert_eq!(result.outcome, OutcomeKind::Applied);

    let run_id = result.run_id.clone().expect("run id");
    let run_dir = rig.run_dir(&result);
    let file = run_dir.join(format!("{run_id}.gwbk"));
    assert_eq!(
        result.gwbk.as_deref(),
        Some(format!("{run_id}.gwbk").as_str())
    );
    assert!(!result.skip_gwbk);
    assert_eq!(std::fs::read(&file).expect("the .gwbk"), gwbk_zip());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&file).expect("stat").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the .gwbk is owner-only");
    }

    // The whole-gateway restore point: type=all, one request.
    let requests = requests_for(&rig, "/data/api/v1/backup").await;
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert_eq!(requests[0].url.query(), Some("type=all"));

    let run = json_file(&run_dir.join("run.json"));
    assert_eq!(run["skip_gwbk"], false);
    // The download came before the lock (event order).
    let events = event_names(&run_dir);
    let at = |name: &str| events.iter().position(|e| e == name).expect(name);
    assert!(at("gwbk_downloaded") < at("staged"), "{events:?}");
    assert!(at("gwbk_downloaded") < at("lock_acquired"), "{events:?}");
    let downloaded = event_lines(&run_dir, "gwbk_downloaded");
    assert_eq!(downloaded[0]["bytes"], gwbk_zip().len());
}

/// Runs the apply and returns its refusal, asserting that it is the `gwbk`
/// preflight and that nothing was staged, locked or written.
async fn gwbk_refusal(rig: &Rig) -> (String, PathBuf) {
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let err = rig
        .run(prepared, &NoObserver)
        .await
        .expect_err("the .gwbk is refused");
    assert_eq!(
        (err.code(), err.exit_code()),
        ("deploy_preflight_failed", 6)
    );
    let CoreError::DeployPreflightFailed { failures } = &err else {
        panic!("expected DeployPreflightFailed, got {err:?}");
    };
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert_eq!(failures[0].check, "gwbk");
    assert_eq!(rig.fake.lock_acquires(), 0);
    assert_eq!(rig.fake.scan_count(), 0);
    assert_eq!(rig.host.calls(), ["probe"], "nothing staged or written");
    let runs = rig.repo.join(".ign").join("deploy").join(GATEWAY);
    let dirs = names(&runs);
    assert_eq!(dirs.len(), 1, "{dirs:?}");
    let run_dir = runs.join(&dirs[0]);
    for file in ["run.json", "plan.json", "events.ndjson", "result.json"] {
        assert!(run_dir.join(file).is_file(), "{file}");
    }
    assert!(
        !names(&run_dir).iter().any(|n| n.ends_with(".gwbk")),
        "a refused .gwbk is not kept: {:?}",
        names(&run_dir)
    );
    assert_eq!(
        json_file(&run_dir.join("result.json"))["error"]["code"],
        "deploy_preflight_failed"
    );
    (failures[0].detail.clone(), run_dir)
}

#[tokio::test]
async fn snapshot_gwbk_truncated_is_refused() {
    let rig = changing_rig().await;
    let whole = gwbk_zip();
    // The central directory is at the end: half a file cannot open as a ZIP.
    mount_gwbk(&rig._server, whole[..whole.len() / 2].to_vec(), 5).await;
    let (detail, _) = gwbk_refusal(&rig).await;
    assert!(detail.contains("ZIP"), "{detail}");
}

#[tokio::test]
async fn snapshot_gwbk_empty_is_refused() {
    // Edge DAPL-05 empty: a zero-byte .gwbk is refused.
    let rig = changing_rig().await;
    mount_gwbk(&rig._server, Vec::new(), 5).await;
    let (detail, _) = gwbk_refusal(&rig).await;
    assert!(detail.contains("empty"), "{detail}");
}

#[tokio::test]
async fn snapshot_gwbk_download_failure_is_refused() {
    let rig = changing_rig().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/backup"))
        .respond_with(wiremock::ResponseTemplate::new(500))
        .with_priority(5)
        .mount(&rig._server)
        .await;
    let (detail, _) = gwbk_refusal(&rig).await;
    assert!(detail.contains("not downloaded"), "{detail}");
}

#[tokio::test]
async fn snapshot_skip_gwbk_is_recorded() {
    let rig = changing_rig().await;
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let opts = ApplyOptions {
        skip_gwbk: true,
        ..options()
    };
    let result = rig
        .run_with(prepared, &NoObserver, &SystemClock, &opts)
        .await
        .expect("applied");
    assert_eq!(result.outcome, OutcomeKind::Applied);
    assert!(result.skip_gwbk);
    assert_eq!(result.gwbk, None);

    assert!(requests_for(&rig, "/data/api/v1/backup").await.is_empty());
    let run_dir = rig.run_dir(&result);
    assert!(!names(&run_dir).iter().any(|n| n.ends_with(".gwbk")));
    assert_eq!(json_file(&run_dir.join("run.json"))["skip_gwbk"], true);
    assert_eq!(event_lines(&run_dir, "gwbk_skipped").len(), 1);
    assert!(event_lines(&run_dir, "gwbk_downloaded").is_empty());
    assert_eq!(json_file(&run_dir.join("result.json"))["skip_gwbk"], true);
}

// ============================================================================
// Restart-pending, the run-directory snapshot and the run directory (23-09)
// ============================================================================

const PENDING_X: &str = "Module Enabled: x";

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// One applied run on the standard changing rig, the gateway listing
/// `before` until the config scan and `after` from then on.
async fn applied_with_pending(before: &[&str], after: &[&str]) -> (Rig, DeployApplyResult) {
    let rig = changing_rig().await;
    rig.fake
        .set_restart_pending(strings(before), strings(after));
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let result = rig.run(prepared, &NoObserver).await.expect("applied");
    assert_eq!(result.outcome, OutcomeKind::Applied);
    (rig, result)
}

#[tokio::test]
async fn run_dir_restart_pending_reported() {
    // Nothing before, one entry after: the post list and the new entry.
    let (_, result) = applied_with_pending(&[], &[PENDING_X]).await;
    assert_eq!(result.restart_pending, [PENDING_X]);
    assert_eq!(result.restart_pending_new, [PENDING_X]);

    // The same entry before and after: pending, not new.
    let (_, result) = applied_with_pending(&[PENDING_X], &[PENDING_X]).await;
    assert_eq!(result.restart_pending, [PENDING_X]);
    assert!(result.restart_pending_new.is_empty(), "{result:?}");

    // One old, one added by the run.
    let (rig, result) = applied_with_pending(&["a"], &["a", "b"]).await;
    assert_eq!(result.restart_pending, ["a", "b"]);
    assert_eq!(result.restart_pending_new, ["b"]);
    let doc = json_file(&rig.run_dir(&result).join("result.json"));
    assert_eq!(doc["restart_pending"], json!(["a", "b"]));
    assert_eq!(doc["restart_pending_new"], json!(["b"]));

    // Edge DAPL-10 empty: both empty is [], never null, in the result and in
    // result.json.
    let (rig, result) = applied_with_pending(&[], &[]).await;
    assert!(result.restart_pending.is_empty());
    assert!(result.restart_pending_new.is_empty());
    let envelope = serde_json::to_value(&result).expect("serializes");
    assert_eq!(envelope["restart_pending"], json!([]));
    assert_eq!(envelope["restart_pending_new"], json!([]));
    let doc = json_file(&rig.run_dir(&result).join("result.json"));
    assert_eq!(doc["restart_pending"], json!([]));
    assert_eq!(doc["restart_pending_new"], json!([]));
    assert!(
        !result
            .warnings
            .iter()
            .any(|w| w.contains("restart_pending")),
        "{:?}",
        result.warnings
    );
}

#[tokio::test]
async fn run_dir_restart_pending_unavailable_warns() {
    let rig = changing_rig().await;
    rig.fake.deny_restart_pending();
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let result = rig
        .run(prepared, &NoObserver)
        .await
        .expect("a 403 on the pending list is never a failure");
    assert_eq!(result.outcome, OutcomeKind::Applied);
    assert!(result.ok);
    let warned: Vec<&String> = result
        .warnings
        .iter()
        .filter(|w| w.as_str() == "restart_pending_unavailable")
        .collect();
    assert_eq!(warned.len(), 1, "once per run: {:?}", result.warnings);
    assert!(result.restart_pending.is_empty());
    assert!(result.restart_pending_new.is_empty());
}

/// Every request the rig's gateway received that names a restart route.
async fn restart_requests(rig: &Rig) -> Vec<String> {
    rig._server
        .received_requests()
        .await
        .expect("request recording is on")
        .into_iter()
        .filter(|r| r.url.path().contains("restart"))
        .map(|r| format!("{} {}", r.method, r.url.path()))
        .collect()
}

#[tokio::test]
async fn run_dir_never_restarts() {
    // An applied run.
    let applied = changing_rig().await;
    wiremock::Mock::given(wiremock::matchers::path(
        "/data/api/v1/restart-tasks/restart",
    ))
    .respond_with(wiremock::ResponseTemplate::new(200))
    .expect(0)
    .mount(&applied._server)
    .await;
    let prepared = applied
        .prepare(GateFlags::default())
        .await
        .expect("prepare");
    let result = applied.run(prepared, &NoObserver).await.expect("applied");
    assert_eq!(result.outcome, OutcomeKind::Applied);
    assert!(restart_requests(&applied).await.is_empty());
    applied._server.verify().await;

    // A rolled-back run.
    let rolled = failing_move_rig().await;
    wiremock::Mock::given(wiremock::matchers::path(
        "/data/api/v1/restart-tasks/restart",
    ))
    .respond_with(wiremock::ResponseTemplate::new(200))
    .expect(0)
    .mount(&rolled._server)
    .await;
    let prepared = rolled.prepare(GateFlags::default()).await.expect("prepare");
    let result = rolled
        .run(prepared, &NoObserver)
        .await
        .expect("rolled back");
    assert_rolled_back(&result, "write_failed");
    assert!(restart_requests(&rolled).await.is_empty());
    rolled._server.verify().await;
}

#[test]
fn apply_source_never_names_the_restart_route() {
    // T-23-39: the deploy path has no way to restart. The only restart text
    // allowed is the read-only pending list.
    fn walk(dir: &Path, hits: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).expect("read_dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, hits);
            } else if path
                .extension()
                .is_some_and(|e| e == "rs" || e == "sh" || e == "ps1")
            {
                // Code only: a comment may say what the engine never does.
                let text: String = std::fs::read_to_string(&path)
                    .expect("source")
                    .lines()
                    .filter(|l| {
                        let l = l.trim_start();
                        !(l.starts_with("//") || l.starts_with('#'))
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                for needle in ["RESTART_PATH", "restart-tasks/restart", "failover"] {
                    if text.contains(needle) {
                        hits.push(format!("{} names {needle}", path.display()));
                    }
                }
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/deploy");
    let mut hits = Vec::new();
    walk(&root, &mut hits);
    assert!(hits.is_empty(), "{hits:?}");
}

#[tokio::test]
async fn snapshot_run_dir_hashes_equal_record() {
    // Edge DAPL-05 adjacency: a created file has no pre-image.
    let rig = changing_rig().await;
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let result = rig.run(prepared, &NoObserver).await.expect("applied");
    let run_id = result.run_id.clone().expect("run id");
    let run_dir = rig.run_dir(&result);
    let record = rig.record(&run_id);
    let listed = json_file(&run_dir.join("result.json"))["snapshot_sha256"].clone();
    let listed = listed.as_object().expect("result.json lists the hashes");

    let mut with_pre = 0;
    let mut created = 0;
    for entry in &record.touched {
        let key = if entry.path == LEDGER_KEY {
            "ledger.json".to_string()
        } else {
            format!("files/{}", entry.path)
        };
        let copy = run_dir.join("snapshot").join(&key);
        match &entry.pre_sha256 {
            Some(pre) => {
                with_pre += 1;
                assert_eq!(
                    sha256_hex(&std::fs::read(&copy).expect("pre-image copy")),
                    *pre,
                    "{key}"
                );
                assert_eq!(listed.get(&key), Some(&json!(pre)), "{key}");
            }
            None => {
                created += 1;
                assert!(!copy.exists(), "{key} was created, so it has no snapshot");
                assert!(!listed.contains_key(&key), "{key}");
            }
        }
    }
    assert!(with_pre > 0 && created > 0, "{with_pre} / {created}");
    assert!(
        !result.warnings.iter().any(|w| w.starts_with("snapshot_")),
        "{:?}",
        result.warnings
    );
}

#[tokio::test]
async fn snapshot_create_only_run_has_an_empty_files_dir() {
    // Edge DAPL-05 empty: a run that only creates files still has a record.
    let fx = PlanFixture::new();
    let (declared, _) = fx.load("dev");
    fx.live_render(MODE, &declared);
    fx.remove(MODE, ALARM_UNIT);
    let host = local_host(&fx);
    let rig = rig_with(fx, host, None).await;
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let result = rig.run(prepared, &NoObserver).await.expect("applied");
    assert_eq!(result.written.len(), 2, "{:?}", result.written);
    let run_id = result.run_id.clone().expect("run id");
    let record = rig.record(&run_id);
    assert!(
        record.touched.iter().any(|e| e.pre_sha256.is_none()),
        "{record:?}"
    );
    let files = rig.run_dir(&result).join("snapshot").join("files");
    let kept = files_below(&files);
    assert!(kept.is_empty(), "no pre-image to copy: {kept:?}");
    assert_eq!(rig.record(&run_id).state, RecordState::Applied);
}

#[tokio::test]
async fn snapshot_two_runs_two_dirs() {
    // Edge DAPL-05 idempotency (D-08): nothing prunes either run.
    let rig = changing_rig().await;
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let first = rig.run(prepared, &NoObserver).await.expect("first");

    // Change a file again and apply a second time.
    let mut bytes = rig.fx.read(MODE, DB_CONFIG);
    bytes.push(b' ');
    rig.fx.mode(DB_CONFIG, &bytes);
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare 2");
    let second = rig.run(prepared, &NoObserver).await.expect("second");
    assert_eq!(second.outcome, OutcomeKind::Applied);

    let (a, b) = (
        first.run_id.clone().expect("a"),
        second.run_id.clone().expect("b"),
    );
    assert_ne!(a, b);
    assert_ne!(rig.run_dir(&first), rig.run_dir(&second));
    let on_box = names(&rig.deploy_dir().join("snapshots"));
    assert_eq!(on_box.len(), 2, "{on_box:?}");
    for result in [&first, &second] {
        assert!(rig.run_dir(result).join("snapshot").is_dir());
        assert!(rig.run_dir(result).join("run.json").is_file());
    }
    let runs = names(&rig.repo.join(".ign").join("deploy").join(GATEWAY));
    assert_eq!(runs.len(), 2, "{runs:?}");
}

/// Every run-directory file is owner-only (unix) and the directory is 0700.
#[cfg(unix)]
fn assert_private(run_dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mode = |p: &Path| std::fs::metadata(p).expect("stat").permissions().mode() & 0o777;
    assert_eq!(mode(run_dir), 0o700, "{}", run_dir.display());
    for file in files_below(run_dir) {
        assert_eq!(mode(&run_dir.join(&file)), 0o600, "{file}");
    }
}

/// The four files every run past the gates leaves.
fn assert_run_files(run_dir: &Path) {
    for file in ["run.json", "result.json", "plan.json", "events.ndjson"] {
        assert!(
            run_dir.join(file).is_file(),
            "{file} in {}",
            run_dir.display()
        );
    }
    #[cfg(unix)]
    assert_private(run_dir);
}

/// The one run directory a rig has.
fn the_run_dir(rig: &Rig) -> PathBuf {
    let root = rig.repo.join(".ign").join("deploy").join(GATEWAY);
    let runs = names(&root);
    assert_eq!(runs.len(), 1, "one run directory: {runs:?}");
    root.join(&runs[0])
}

fn assert_no_ign_dir(rig: &Rig, what: &str) {
    assert!(
        !rig.repo.join(".ign").exists(),
        "{what} must leave no run directory"
    );
}

#[tokio::test]
async fn run_dir_matrix() {
    // Success.
    let rig = changing_rig().await;
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let result = rig.run(prepared, &NoObserver).await.expect("applied");
    assert_run_files(&rig.run_dir(&result));

    // A preflight failure.
    let rig = changing_rig().await;
    mount_designers(&rig, &[("alice", "PlantA", "10.0.0.5:52526")]).await;
    assert_eq!(refused_checks(&rig).await, ["designers"]);
    assert_run_files(&the_run_dir(&rig));

    // A post-write failure that restored.
    let rig = failing_move_rig().await;
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let result = rig.run(prepared, &NoObserver).await.expect("rolled back");
    assert_rolled_back(&result, "write_failed");
    assert_run_files(&rig.run_dir(&result));

    // A rollback_failed.
    let run = scenario_restore_fails().await;
    assert_eq!(run.error().code(), "rollback_failed");
    assert_run_files(&the_run_dir(&run.rig));

    // Refusals before the run directory leave none. A flag error:
    let rig = changing_rig().await;
    let err = rig
        .prepare(with_hash("not a hash"))
        .await
        .expect_err("a malformed hash");
    assert_eq!(err.code(), "invalid_input");
    assert_no_ign_dir(&rig, "a flag error");
    // A stale hash:
    let err = rig
        .prepare(with_hash(&"0".repeat(64)))
        .await
        .expect_err("stale");
    assert_eq!(err.code(), "plan_stale");
    assert_no_ign_dir(&rig, "plan_stale");
    // A destructive change without its flag:
    let fx = PlanFixture::new();
    let (declared, _) = fx.load("dev");
    fx.live_render(MODE, &declared);
    let old = "ignition/database-translator/OLD";
    fx.mode(&format!("{old}/config.json"), b"{}");
    fx.mode(&format!("{old}/resource.json"), b"{}");
    fx.ledger(
        GATEWAY,
        MODE,
        &[
            &format!("{old}/config.json"),
            &format!("{old}/resource.json"),
        ],
    );
    let host = local_host(&fx);
    let rig = rig_with(fx, host, None).await;
    let err = rig
        .prepare(GateFlags::default())
        .await
        .expect_err("destructive");
    assert_eq!(err.code(), "destructive_change_refused");
    assert_no_ign_dir(&rig, "destructive_change_refused");
    // A no-op:
    let fx = PlanFixture::new();
    let (declared, _) = fx.load("dev");
    fx.live_render(MODE, &declared);
    let host = local_host(&fx);
    let rig = rig_with(fx, host, None).await;
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    assert!(matches!(prepared, ApplyPrepared::Noop(_)));
    let result = rig.run(prepared, &NoObserver).await.expect("noop");
    assert_eq!(result.outcome, OutcomeKind::Noop);
    assert_no_ign_dir(&rig, "a no-op");

    // A repo that does not ignore .ign/: refused before the directory exists.
    let rig = changing_rig().await;
    let git = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&rig.repo)
        .status()
        .expect("git runs");
    assert!(git.success());
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let err = rig
        .run(prepared, &NoObserver)
        .await
        .expect_err("not ignored");
    let CoreError::DeployPreflightFailed { failures } = &err else {
        panic!("expected DeployPreflightFailed, got {err:?}");
    };
    assert_eq!(failures[0].check, "run_dir_not_ignored");
    assert_no_ign_dir(&rig, "run_dir_not_ignored");
}

// ============================================================================
// Manual rollback (23-11)
// ============================================================================

/// A fresh process: everything a rollback runner builds from the target file
/// alone. A new host, reader, fake gateway client and REST server; nothing is
/// shared with the rig that applied except the gateway's data directory.
struct Fresh {
    target_file: String,
    target: Target,
    reader: Box<dyn TreeReader>,
    host: FaultyHost<LocalFsTransport>,
    fake: FakeGateway,
    api: ReqwestGatewayApi,
    server: wiremock::MockServer,
}

impl Fresh {
    async fn new(rig: &Rig) -> Fresh {
        let (target_file, target) = load_target(&rig.repo, "dev").expect("the target file loads");
        let reader = transport::for_node(
            &rig.repo,
            &target_file,
            "nodes.master",
            &target.nodes.master,
        )
        .expect("master has host access");
        let server = wiremock::MockServer::start().await;
        mount_passing_preflights(&server).await;
        let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
        Fresh {
            target_file,
            target,
            reader,
            host: FaultyHost::new(local_host(&rig.fx)),
            fake: FakeGateway::with_data_dir(rig.fx.data_dir(), MODE),
            api,
            server,
        }
    }

    async fn prepare(
        &self,
        rig: &Rig,
        run_id: &str,
        discard: bool,
    ) -> Result<RollbackPrepared, CoreError> {
        deploy_rollback_prepare(
            &rig.repo,
            "dev",
            run_id,
            self.reader.as_ref(),
            &self.host,
            &RollbackPrepareOptions {
                discard_live_changes: discard,
            },
        )
        .await
    }

    async fn run(
        &self,
        rig: &Rig,
        prepared: RollbackPrepared,
        run_id: &str,
        discard: bool,
    ) -> Result<DeployRollbackResult, CoreError> {
        let deploy = self.fake.client("rollback");
        let ctx = RollbackContext {
            env: "dev",
            repo: &rig.repo,
            target_file: &self.target_file,
            target: &self.target,
            reader: self.reader.as_ref(),
            host: &self.host,
            deploy: &deploy,
            gateway: &self.api,
            backup: None,
            clock: &SystemClock,
            observer: &NoObserver,
        };
        deploy_rollback_execute(prepared, &ctx, &rollback_options(run_id, discard)).await
    }

    /// `prepare` then `run`, the way the verb will.
    async fn rollback(
        &self,
        rig: &Rig,
        run_id: &str,
        discard: bool,
    ) -> Result<DeployRollbackResult, CoreError> {
        let prepared = self.prepare(rig, run_id, discard).await?;
        self.run(rig, prepared, run_id, discard).await
    }
}

fn rollback_options(run_id: &str, discard: bool) -> RollbackOptions {
    let base = options();
    RollbackOptions {
        run_id: RunId::parse(run_id).expect("a run id"),
        discard_live_changes: discard,
        designers: DesignersPolicy::default(),
        hold: base.hold,
        poll: base.poll,
    }
}

/// Plan and apply `rig` once; the run id of the applied run.
async fn apply_once(rig: &Rig) -> String {
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let result = rig.run(prepared, &NoObserver).await.expect("applied");
    assert_eq!(result.outcome, OutcomeKind::Applied, "{result:?}");
    result.run_id.expect("a run id")
}

#[tokio::test]
async fn rollback_fresh_process_from_record_only() {
    let rig = changing_rig().await;
    let before = collection_state(&rig);
    let target_run = apply_once(&rig).await;
    assert_ne!(collection_state(&rig), before, "the apply changed the host");

    // The applying runner is gone: no run directory, manifest or render is
    // left to read.
    std::fs::remove_dir_all(rig.repo.join(".ign")).expect("remove the run directories");
    let fresh = Fresh::new(&rig).await;

    let prepared = fresh
        .prepare(&rig, &target_run, false)
        .await
        .expect("prepare");
    let record = prepared.record.as_ref().expect("a record");
    assert_eq!(record.state, RecordState::Applied);
    assert!(prepared.prose.contains(&target_run), "{}", prepared.prose);
    assert!(prepared.prose.contains(&record.plan_sha256));
    assert!(prepared.live_changed.is_empty());
    let result = fresh
        .run(&rig, prepared, &target_run, false)
        .await
        .expect("rolled back");

    assert_eq!(result.outcome, RollbackKind::Restored);
    assert!(result.ok);
    assert_eq!(result.target_run_id, target_run);
    assert_ne!(result.run_id, target_run, "a rollback is a run of its own");
    assert_eq!(result.gateway, GATEWAY);
    assert_eq!(result.mode, MODE);
    assert_eq!(
        result.restored,
        [DB_CONFIG.to_string(), DB_RESOURCE.to_string()]
    );
    assert_eq!(
        result.deleted,
        [ALARM_CONFIG.to_string(), ALARM_RESOURCE.to_string()]
    );
    assert!(result.discarded_live_changes.is_empty());
    assert!(result.restart_pending.is_empty() && result.restart_pending_new.is_empty());

    // The collection and the ledger are what they were before the apply, and
    // the gateway was scanned once.
    assert_eq!(collection_state(&rig), before);
    assert_eq!(fresh.fake.scan_count(), 1);
    assert_eq!(fresh.fake.lock_acquires(), 1);

    // The record, the journal and the staging.
    let record = rig.record(&target_run);
    assert_eq!(record.state, RecordState::RolledBack);
    assert_eq!(
        record.rolled_back_by.as_deref(),
        Some(result.run_id.as_str())
    );
    assert!(!rig.deploy_dir().join("journal.json").exists());
    assert!(!rig.deploy_dir().join("staging").join(&target_run).exists());

    // The rollback's own run directory.
    let run_dir = PathBuf::from(&result.run_dir);
    assert!(run_dir.ends_with(&result.run_id), "{}", result.run_dir);
    assert_eq!(
        names(&run_dir),
        [
            "events.ndjson",
            "plan.json",
            "result.json",
            "run.json",
            "snapshot"
        ]
    );
    let run_json = json_file(&run_dir.join("run.json"));
    assert_eq!(run_json["kind"], "rollback");
    assert_eq!(run_json["run_id"], result.run_id.as_str());
    assert_eq!(run_json["target_run_id"], target_run.as_str());
    assert_eq!(run_json["discard_live_changes"], false);
    let plan_json = json_file(&run_dir.join("plan.json"));
    assert_eq!(plan_json["target_run_id"], target_run.as_str());
    assert_eq!(plan_json["restore"], json!([DB_CONFIG, DB_RESOURCE]));
    assert_eq!(plan_json["delete"], json!([ALARM_CONFIG, ALARM_RESOURCE]));
    let result_json = json_file(&run_dir.join("result.json"));
    assert_eq!(result_json["outcome"], "restored");
    assert_eq!(result_json["ok"], true);
    assert_eq!(result_json["kind"], "rollback");
    assert_eq!(
        event_names(&run_dir),
        [
            "run_started",
            "lock_acquired",
            "restore_started",
            "scan_done",
            "restored"
        ]
    );
    let started = event_lines(&run_dir, "restore_started");
    assert_eq!(started[0]["cause"], Value::Null, "{started:?}");

    // The host calls: nothing but reads before the preflights' probe, one
    // restore, no commit.
    let calls = fresh.host.calls();
    assert_eq!(
        calls.iter().filter(|c| **c == "restore").count(),
        1,
        "{calls:?}"
    );
    assert!(
        !calls.contains(&"commit") && !calls.contains(&"prepare"),
        "{calls:?}"
    );
}

#[tokio::test]
async fn rollback_bad_run_id_is_invalid_input() {
    let rig = changing_rig().await;
    let fresh = Fresh::new(&rig).await;
    for bad in ["../x", "", "a".repeat(129).as_str()] {
        let err = fresh
            .prepare(&rig, bad, false)
            .await
            .expect_err("a bad run id is refused");
        assert_eq!(err.code(), "invalid_input", "{bad:?}: {err}");
        assert_eq!(err.exit_code(), 2);
    }
    // Before any host contact.
    assert!(fresh.host.calls().is_empty());
    assert_eq!(fresh.fake.lock_acquires(), 0);
    assert_eq!(fresh.fake.scan_count(), 0);
    assert!(
        fresh
            .server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
    assert!(!rig.repo.join(".ign").exists());
    assert!(!rig.deploy_dir().join("snapshots").exists());
}

/// One byte appended to a live file of the mode collection.
fn edit_live(rig: &Rig, path: &str) {
    let mut bytes = rig.fx.read(MODE, path);
    bytes.extend_from_slice(b" ");
    rig.fx.mode(path, &bytes);
}

fn record_path(rig: &Rig, run_id: &str) -> PathBuf {
    rig.deploy_dir()
        .join("snapshots")
        .join(run_id)
        .join("record.json")
}

/// The run directories of this rig's repo, byte-sorted.
fn run_dirs(rig: &Rig) -> Vec<String> {
    names(&rig.repo.join(".ign/deploy").join(GATEWAY))
}

/// The calls that wrote nothing: a refusal made before the lock is only ever
/// made of these.
fn only_reads(fresh: &Fresh) {
    let calls = fresh.host.calls();
    for write in [
        "prepare",
        "commit",
        "restore",
        "finalize",
        "discard_interrupted",
    ] {
        assert!(!calls.contains(&write), "{write} was called: {calls:?}");
    }
    assert_eq!(fresh.fake.lock_acquires(), 0, "no lock was taken");
    assert_eq!(fresh.fake.scan_count(), 0, "no scan was asked for");
}

#[tokio::test]
async fn rollback_live_changed_refuses() {
    let rig = changing_rig().await;
    let target_run = apply_once(&rig).await;
    edit_live(&rig, DB_CONFIG);
    let edited = collection_state(&rig);

    let fresh = Fresh::new(&rig).await;
    let err = fresh
        .rollback(&rig, &target_run, false)
        .await
        .expect_err("a changed touched file refuses");
    let CoreError::DeployLiveChanged {
        run_id,
        paths,
        count,
    } = &err
    else {
        panic!("expected deploy_live_changed, got {err:?}");
    };
    assert_eq!(run_id, &target_run);
    assert_eq!(paths, &[DB_CONFIG.to_string()]);
    assert_eq!(*count, 1);
    assert_eq!(err.exit_code(), 6);
    assert!(err.to_string().contains(DB_CONFIG), "{err}");

    // The host is as it was, the record is still `applied`, no run was made.
    assert_eq!(collection_state(&rig), edited);
    assert_eq!(rig.record(&target_run).state, RecordState::Applied);
    assert_eq!(run_dirs(&rig), std::slice::from_ref(&target_run));
    only_reads(&fresh);
}

/// A fresh runner whose host edits `DB_CONFIG` once the rollback's own check
/// (its second `fingerprint`, after the prepare's) has passed: a live edit that
/// lands between the check and the restore (review WR-01).
fn edit_after_the_check(rig: &Rig, fresh: Fresh) -> Fresh {
    let target = rig.fx.collection_dir(MODE).join(DB_CONFIG);
    let host = fresh.host.clone().after("fingerprint", 2, move || {
        let mut bytes = std::fs::read(&target).expect("live file");
        bytes.extend_from_slice(b"\n");
        std::fs::write(&target, bytes).expect("foreign edit");
        async {}
    });
    Fresh { host, ..fresh }
}

#[tokio::test]
async fn rollback_live_edit_after_the_check_is_refused_under_the_lock() {
    let rig = changing_rig().await;
    let target_run = apply_once(&rig).await;
    let fresh = edit_after_the_check(&rig, Fresh::new(&rig).await);

    let err = fresh
        .rollback(&rig, &target_run, false)
        .await
        .expect_err("an edit that landed under the lock refuses");
    let CoreError::DeployLiveChanged { run_id, paths, .. } = &err else {
        panic!("expected deploy_live_changed, got {err:?}");
    };
    assert_eq!(run_id, &target_run);
    assert_eq!(paths, &[DB_CONFIG.to_string()]);

    // The edit is still there: nothing was restored over it. The lock was
    // taken and released by one no-op scan.
    let calls = fresh.host.calls();
    assert!(!calls.contains(&"restore"), "{calls:?}");
    assert_eq!(rig.record(&target_run).state, RecordState::Applied);
    assert_eq!(fresh.fake.lock_acquires(), 1);
    assert_eq!(fresh.fake.scan_count(), 1);
    let edited = std::fs::read(rig.fx.collection_dir(MODE).join(DB_CONFIG)).expect("file");
    assert!(edited.ends_with(b"\n"), "the foreign edit was kept");
}

#[tokio::test]
async fn rollback_live_edit_after_the_check_is_named_when_discarding() {
    let rig = changing_rig().await;
    let before = collection_state(&rig);
    let target_run = apply_once(&rig).await;
    let fresh = edit_after_the_check(&rig, Fresh::new(&rig).await);

    let result = fresh
        .rollback(&rig, &target_run, true)
        .await
        .expect("the flag allows overwriting");
    assert_eq!(result.outcome, RollbackKind::Restored);
    assert_eq!(result.discarded_live_changes, [DB_CONFIG.to_string()]);
    assert_eq!(collection_state(&rig), before, "the snapshot is back");
}

#[tokio::test]
async fn rollback_discard_live_changes_proceeds() {
    let rig = changing_rig().await;
    let before = collection_state(&rig);
    let target_run = apply_once(&rig).await;
    edit_live(&rig, DB_CONFIG);

    let fresh = Fresh::new(&rig).await;
    let prepared = fresh
        .prepare(&rig, &target_run, true)
        .await
        .expect("discarding makes it prepare");
    assert_eq!(prepared.live_changed, [DB_CONFIG.to_string()]);
    assert!(prepared.prose.contains(DB_CONFIG), "{}", prepared.prose);
    let result = fresh
        .run(&rig, prepared, &target_run, true)
        .await
        .expect("restored over the change");

    assert_eq!(result.outcome, RollbackKind::Restored);
    assert_eq!(result.discarded_live_changes, [DB_CONFIG.to_string()]);
    assert_eq!(collection_state(&rig), before, "the snapshot is back");

    // The rollback run records what it overwrote.
    let run_dir = PathBuf::from(&result.run_dir);
    let run_json = json_file(&run_dir.join("run.json"));
    assert_eq!(run_json["discard_live_changes"], true);
    assert_eq!(run_json["discarded_live_changes"], json!([DB_CONFIG]));
    let discarded = event_lines(&run_dir, "live_changes_discarded");
    assert_eq!(discarded.len(), 1, "{discarded:?}");
    assert_eq!(discarded[0]["paths"], json!([DB_CONFIG]));
    assert_eq!(discarded[0]["count"], 1);
    assert_eq!(
        event_names(&run_dir),
        [
            "run_started",
            "live_changes_discarded",
            "lock_acquired",
            "restore_started",
            "scan_done",
            "restored"
        ]
    );
    assert_eq!(
        json_file(&run_dir.join("result.json"))["discarded_live_changes"],
        json!([DB_CONFIG])
    );
}

#[tokio::test]
async fn rollback_unrelated_edit_does_not_block() {
    let rig = changing_rig().await;
    let mut before = collection_state(&rig);
    let target_run = apply_once(&rig).await;

    // A file the run never touched: not part of the touched set (23-02).
    let other = "ignition/not-ours/config.json";
    rig.fx.mode(other, b"{}");
    before.insert(other.to_string(), sha256_hex(b"{}"));

    let fresh = Fresh::new(&rig).await;
    let result = fresh
        .rollback(&rig, &target_run, false)
        .await
        .expect("an unrelated edit does not block");
    assert_eq!(result.outcome, RollbackKind::Restored);
    assert!(result.discarded_live_changes.is_empty());
    assert_eq!(collection_state(&rig), before, "and it is left alone");
}

#[tokio::test]
async fn rollback_older_run_behind_newer_refuses() {
    let mut rig = changing_rig().await;
    let run_a = apply_once(&rig).await;

    // Run B changes the same file to something else.
    let source = rig
        .repo
        .join("gateways")
        .join(GATEWAY)
        .join("resources")
        .join(DB_CONFIG);
    let text = std::fs::read_to_string(&source).expect("the source file");
    assert!(text.contains("varchar(255)"));
    std::fs::write(&source, text.replace("varchar(255)", "varchar(128)")).expect("edit the source");
    let (declared, reader) = rig.fx.load("dev");
    rig.declared = declared;
    rig.reader = reader;
    let run_b = apply_once(&rig).await;
    assert_ne!(run_a, run_b);
    let after_b = collection_state(&rig);

    let fresh = Fresh::new(&rig).await;
    let err = fresh
        .rollback(&rig, &run_a, false)
        .await
        .expect_err("run A is behind run B");
    let CoreError::DeployLiveChanged { paths, count, .. } = &err else {
        panic!("expected deploy_live_changed, got {err:?}");
    };
    assert!(paths.contains(&DB_CONFIG.to_string()), "{paths:?}");
    assert_eq!(*count, paths.len());
    assert_eq!(collection_state(&rig), after_b);
    only_reads(&fresh);

    // The newest run still rolls back.
    let result = fresh
        .rollback(&rig, &run_b, false)
        .await
        .expect("run B rolls back");
    assert_eq!(result.outcome, RollbackKind::Restored);
}

/// Rewrite the on-box record of `run_id` with `edit`, run a rollback and
/// return what it said; the original record is put back afterwards.
async fn rollback_with_record(rig: &Rig, run_id: &str, edit: impl FnOnce(&mut Value)) -> CoreError {
    let path = record_path(rig, run_id);
    let original = std::fs::read(&path).expect("the record");
    let mut doc: Value = serde_json::from_slice(&original).expect("record json");
    edit(&mut doc);
    std::fs::write(&path, serde_json::to_vec_pretty(&doc).expect("json")).expect("rewrite");
    let fresh = Fresh::new(rig).await;
    let err = fresh
        .rollback(rig, run_id, false)
        .await
        .expect_err("an unusable record refuses");
    std::fs::write(&path, original).expect("put the record back");
    only_reads(&fresh);
    err
}

fn assert_invalid(err: &CoreError, run_id: &str, needle: &str) {
    assert!(
        matches!(err, CoreError::DeployRunInvalid { .. }),
        "expected deploy_run_invalid, got {err:?}"
    );
    assert_eq!(err.code(), "deploy_run_invalid");
    assert_eq!(err.exit_code(), 6);
    let text = err.to_string();
    assert!(text.contains(run_id), "the run id in {text}");
    assert!(text.contains(needle), "{needle:?} in {text}");
}

#[tokio::test]
async fn rollback_record_problems() {
    let rig = changing_rig().await;
    let run = apply_once(&rig).await;
    let before = collection_state(&rig);

    // No record: a run id the host has never seen.
    let unknown = "20261016T000000Z-aaaaaa";
    let fresh = Fresh::new(&rig).await;
    let err = fresh
        .rollback(&rig, unknown, false)
        .await
        .expect_err("no record");
    assert_invalid(&err, unknown, "record.json");
    only_reads(&fresh);

    // A record that is not JSON.
    let path = record_path(&rig, &run);
    let original = std::fs::read(&path).expect("the record");
    std::fs::write(&path, b"{ not json").expect("corrupt");
    let fresh = Fresh::new(&rig).await;
    let err = fresh
        .rollback(&rig, &run, false)
        .await
        .expect_err("a corrupt record");
    std::fs::write(&path, &original).expect("put the record back");
    assert_invalid(&err, &run, "record.json");
    only_reads(&fresh);

    // A record for another mode, and for another gateway.
    let err = rollback_with_record(&rig, &run, |doc| doc["mode"] = json!("prod")).await;
    assert_invalid(&err, &run, "mode");
    let err = rollback_with_record(&rig, &run, |doc| doc["gateway"] = json!("other-gw")).await;
    assert_invalid(&err, &run, "gateway");

    // Still `applying`, and no journal on the box.
    let err = rollback_with_record(&rig, &run, |doc| {
        doc["state"] = json!("applying");
        doc["post_fingerprint"] = Value::Null;
    })
    .await;
    assert_invalid(&err, &run, "did not finish");

    // A record that disagrees with itself.
    let err = rollback_with_record(&rig, &run, |doc| {
        doc["touched"][0]["post_sha256"] = json!("0".repeat(64));
    })
    .await;
    assert_invalid(&err, &run, "post_fingerprint");

    // Nothing was written by any of them, and the real record still works.
    assert_eq!(collection_state(&rig), before);
    assert_eq!(rig.record(&run).state, RecordState::Applied);
    let fresh = Fresh::new(&rig).await;
    fresh
        .rollback(&rig, &run, false)
        .await
        .expect("the untouched record rolls back");
}

#[tokio::test]
async fn rollback_run_dir_mismatch() {
    let rig = changing_rig().await;
    let run = apply_once(&rig).await;
    let before = collection_state(&rig);
    let dir = rig.repo.join(".ign/deploy").join(GATEWAY).join(&run);

    // run.json names another plan.
    let run_json = dir.join("run.json");
    let run_json_original = std::fs::read(&run_json).expect("run.json");
    let mut doc: Value = serde_json::from_slice(&run_json_original).expect("json");
    doc["plan_sha256"] = json!("0".repeat(64));
    std::fs::write(&run_json, serde_json::to_vec_pretty(&doc).expect("json")).expect("edit");
    let fresh = Fresh::new(&rig).await;
    let err = fresh
        .rollback(&rig, &run, false)
        .await
        .expect_err("a run.json that disagrees");
    assert_invalid(&err, &run, "plan_sha256");
    assert!(err.to_string().contains("run.json"), "{err}");
    only_reads(&fresh);
    std::fs::write(&run_json, &run_json_original).expect("put run.json back");

    // plan.json names another render.
    let plan_json = dir.join("plan.json");
    let original = std::fs::read(&plan_json).expect("plan.json");
    let mut doc: Value = serde_json::from_slice(&original).expect("json");
    doc["render_sha256"] = json!("1".repeat(64));
    std::fs::write(&plan_json, serde_json::to_vec_pretty(&doc).expect("json")).expect("edit");
    let fresh = Fresh::new(&rig).await;
    let err = fresh
        .rollback(&rig, &run, false)
        .await
        .expect_err("a plan.json that disagrees");
    assert_invalid(&err, &run, "render_sha256");
    assert!(err.to_string().contains("plan.json"), "{err}");
    only_reads(&fresh);
    std::fs::write(&plan_json, &original).expect("put plan.json back");

    // The run directory of a rollback is not a deployment's.
    let mut doc: Value =
        serde_json::from_slice(&std::fs::read(&run_json).expect("run.json")).expect("json");
    doc["kind"] = json!("rollback");
    std::fs::write(&run_json, serde_json::to_vec_pretty(&doc).expect("json")).expect("edit");
    let fresh = Fresh::new(&rig).await;
    let err = fresh
        .rollback(&rig, &run, false)
        .await
        .expect_err("a rollback's run directory");
    assert_invalid(&err, &run, "rollback");
    std::fs::write(&run_json, &run_json_original).expect("put run.json back");

    assert_eq!(collection_state(&rig), before);
    let fresh = Fresh::new(&rig).await;
    fresh
        .rollback(&rig, &run, false)
        .await
        .expect("with the files agreeing it rolls back");
}

#[tokio::test]
async fn rollback_twice_refuses() {
    // Edge DAPL-08 idempotency.
    let rig = changing_rig().await;
    let before = collection_state(&rig);
    let target_run = apply_once(&rig).await;
    let first = Fresh::new(&rig)
        .await
        .rollback(&rig, &target_run, false)
        .await
        .expect("the first rollback");
    assert_eq!(collection_state(&rig), before);
    let record_after_first = std::fs::read(record_path(&rig, &target_run)).expect("record");

    let second = Fresh::new(&rig).await;
    let err = second
        .rollback(&rig, &target_run, false)
        .await
        .expect_err("a second rollback of the same run refuses");
    assert_invalid(
        &err,
        &target_run,
        &format!("already rolled back by {}", first.run_id),
    );
    only_reads(&second);
    assert_eq!(collection_state(&rig), before, "nothing was written");
    assert_eq!(
        std::fs::read(record_path(&rig, &target_run)).expect("record"),
        record_after_first
    );
    assert_eq!(run_dirs(&rig).len(), 2, "no third run directory");
}

fn foreign_journal() -> Journal {
    Journal {
        schema_version: 1,
        run_id: "20261016T000000Z-ffffff".to_string(),
        gateway: GATEWAY.to_string(),
        mode: MODE.to_string(),
        phase: JournalPhase::Writing,
        pre_fingerprint: "0".repeat(64),
        touched: vec![DB_CONFIG.to_string()],
    }
}

#[tokio::test]
async fn rollback_other_journal_refuses() {
    // Edge DAPL-08 concurrency.
    let rig = changing_rig().await;
    let target_run = apply_once(&rig).await;
    let applied = collection_state(&rig);

    // The journal is on the box before the rollback starts ...
    std::fs::write(
        rig.deploy_dir().join("journal.json"),
        foreign_journal().to_bytes(),
    )
    .expect("journal");
    let fresh = Fresh::new(&rig).await;
    let err = fresh
        .rollback(&rig, &target_run, false)
        .await
        .expect_err("another run's journal refuses");
    let CoreError::DeployJournalPresent { run_id, env, .. } = &err else {
        panic!("expected deploy_journal_present, got {err:?}");
    };
    assert_eq!(run_id, "20261016T000000Z-ffffff");
    assert_eq!(env, "dev");
    assert!(err.to_string().contains("--env dev"), "{err}");
    only_reads(&fresh);
    assert_eq!(run_dirs(&rig), std::slice::from_ref(&target_run));

    // ... or appears between the preparation and the run.
    std::fs::remove_file(rig.deploy_dir().join("journal.json")).expect("clear the journal");
    let fresh = Fresh::new(&rig).await;
    let prepared = fresh
        .prepare(&rig, &target_run, false)
        .await
        .expect("prepared while the box is clear");
    std::fs::write(
        rig.deploy_dir().join("journal.json"),
        foreign_journal().to_bytes(),
    )
    .expect("journal");
    let err = fresh
        .run(&rig, prepared, &target_run, false)
        .await
        .expect_err("the run looks again");
    assert_eq!(err.code(), "deploy_journal_present", "{err}");
    only_reads(&fresh);
    assert_eq!(collection_state(&rig), applied);
    assert_eq!(rig.record(&target_run).state, RecordState::Applied);
}

#[tokio::test]
async fn rollback_lock_held_refuses() {
    // Edge DAPL-08 concurrency.
    let rig = changing_rig().await;
    let target_run = apply_once(&rig).await;
    let applied = collection_state(&rig);

    let fresh = Fresh::new(&rig).await;
    fresh
        .fake
        .client("b")
        .scan_lock_acquire(ScanKind::Config, 1, 600)
        .await
        .expect("B takes the lock");
    let err = fresh
        .rollback(&rig, &target_run, false)
        .await
        .expect_err("another actor holds the scan-lock");
    assert_eq!(err.code(), "scan_lock_held", "{err}");
    assert_eq!(err.exit_code(), 6);

    // No write, no scan; the record is untouched. The refusal is in the
    // rollback run's result.
    let calls = fresh.host.calls();
    for write in ["prepare", "commit", "restore", "finalize"] {
        assert!(!calls.contains(&write), "{write} was called: {calls:?}");
    }
    assert_eq!(fresh.fake.scan_count(), 0);
    assert_eq!(collection_state(&rig), applied);
    assert_eq!(rig.record(&target_run).state, RecordState::Applied);
    assert!(!rig.deploy_dir().join("journal.json").exists());
    let runs = run_dirs(&rig);
    assert_eq!(runs.len(), 2, "{runs:?}");
    let failed = runs
        .iter()
        .find(|r| **r != target_run)
        .expect("rollback run");
    let result = json_file(
        &rig.repo
            .join(".ign/deploy")
            .join(GATEWAY)
            .join(failed)
            .join("result.json"),
    );
    assert_eq!(result["ok"], false);
    assert_eq!(result["error"]["code"], "scan_lock_held");
    assert_eq!(result["target_run_id"], target_run.as_str());
}

// ============================================================================
// Interrupted-run recovery (23-12)
// ============================================================================

fn journal_path(rig: &Rig) -> PathBuf {
    rig.deploy_dir().join("journal.json")
}

/// The journal on the box, parsed.
fn read_journal(rig: &Rig) -> Journal {
    Journal::parse(
        "journal.json",
        &std::fs::read(journal_path(rig)).expect("the journal"),
        GATEWAY,
        MODE,
    )
    .expect("the journal parses")
}

/// `plan_sha256` of a plain `deploy plan` of the rig's declared tree.
async fn plan_hash(rig: &Rig) -> String {
    deploy_plan(
        &rig.declared,
        rig.reader.as_ref(),
        &PlanOptions {
            base: None,
            diff: false,
        },
    )
    .await
    .expect("a plan")
    .plan_sha256
}

/// A real apply that was killed after its first move.
struct Killed {
    rig: Arc<Rig>,
    /// The journal it left (`writing`).
    journal: Journal,
    /// The collection and ledger before the apply started.
    before: BTreeMap<String, String>,
    /// `plan_sha256` before the apply started.
    plan_before: String,
}

/// Start a real apply task on a host that hangs after its first move, then
/// abort the task: what is left is what a `kill -9` there leaves.
async fn killed_mid_write() -> Killed {
    let notify = Arc::new(tokio::sync::Notify::new());
    let rig = changing_rig_faulted(
        Vec::new(),
        Some(ApplyFault::HangAfterMove(1, notify.clone())),
    )
    .await;
    kill_apply(rig, notify).await
}

/// Run `rig`'s apply on a task until `notify` fires, then abort the task.
async fn kill_apply(rig: Rig, notify: Arc<tokio::sync::Notify>) -> Killed {
    let rig = Arc::new(rig);
    let before = collection_state(&rig);
    let plan_before = plan_hash(&rig).await;
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let task = tokio::spawn({
        let rig = rig.clone();
        async move { rig.run(prepared, &NoObserver).await }
    });
    tokio::time::timeout(Duration::from_secs(60), notify.notified())
        .await
        .expect("the commit reached the hang");
    task.abort();
    assert!(task.await.expect_err("aborted").is_cancelled());
    let journal = read_journal(&rig);
    Killed {
        rig,
        journal,
        before,
        plan_before,
    }
}

/// `err` is `deploy_journal_present` for `run_id`, and its message holds the
/// exact recovery command.
fn assert_recovery_hint(err: &CoreError, run_id: &str) {
    let CoreError::DeployJournalPresent {
        run_id: named,
        env,
        location,
    } = err
    else {
        panic!("expected deploy_journal_present, got {err:?}");
    };
    assert_eq!(named, run_id);
    assert_eq!(env, "dev");
    assert_eq!(err.exit_code(), 6);
    let text = err.to_string();
    assert!(
        text.contains(&format!("ign deploy rollback --run {run_id} --env dev")),
        "the command in {text}"
    );
    assert!(text.contains(location.as_str()), "the location in {text}");
    assert!(location.ends_with("journal.json"), "{location}");
}

#[tokio::test]
async fn journal_kill_mid_write_then_recover() {
    let Killed {
        rig,
        journal,
        before,
        plan_before,
    } = killed_mid_write().await;
    let run_id = journal.run_id.clone();
    assert_eq!(journal.phase, JournalPhase::Writing);
    let torn = collection_state(&rig);
    assert_ne!(torn, before, "one file moved before the kill");

    // Plan, apply and a rollback of another run all refuse, with the command.
    let err = deploy_plan(
        &rig.declared,
        rig.reader.as_ref(),
        &PlanOptions {
            base: None,
            diff: false,
        },
    )
    .await
    .expect_err("plan refuses over a journal");
    assert_recovery_hint(&err, &run_id);
    let err = rig
        .prepare(GateFlags::default())
        .await
        .expect_err("apply refuses over a journal");
    assert_recovery_hint(&err, &run_id);
    let other = Fresh::new(&rig).await;
    let err = other
        .prepare(&rig, "20261016T000000Z-aaaaaa", false)
        .await
        .expect_err("a rollback of another run refuses");
    assert_recovery_hint(&err, &run_id);
    only_reads(&other);
    assert_eq!(collection_state(&rig), torn, "the refusals wrote nothing");
    assert!(journal_path(&rig).is_file());

    // The recovery is the rollback of the journal's own run, from a fresh
    // process, with no flag.
    let fresh = Fresh::new(&rig).await;
    let result = fresh
        .rollback(&rig, &run_id, false)
        .await
        .expect("the journal's own run recovers");
    assert_eq!(result.outcome, RollbackKind::Restored);
    assert!(result.ok);
    assert_eq!(result.target_run_id, run_id);
    assert!(result.discarded_live_changes.is_empty());

    assert_eq!(collection_state(&rig), before, "the pre-apply tree is back");
    assert!(!journal_path(&rig).exists(), "the journal is removed");
    assert!(
        !rig.deploy_dir().join("staging").join(&run_id).exists(),
        "and the run's staging"
    );
    assert_eq!(fresh.fake.scan_count(), 1, "scanned once");
    let record = rig.record(&run_id);
    assert_eq!(record.state, RecordState::RolledBack);
    assert_eq!(
        record.rolled_back_by.as_deref(),
        Some(result.run_id.as_str())
    );
    assert_eq!(plan_hash(&rig).await, plan_before, "the same plan again");

    // The recovery run records the torn state it found (D-11).
    let run_dir = PathBuf::from(&result.run_dir);
    let found = event_lines(&run_dir, "interrupted_run");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["phase"], "writing");
    assert_eq!(found[0]["action"], "restore");
    assert_eq!(found[0]["touched"], journal.touched.len());
    let expected: serde_json::Map<String, Value> = journal
        .touched
        .iter()
        .map(|path| {
            (
                path.clone(),
                torn.get(path).map_or(Value::Null, |hash| json!(hash)),
            )
        })
        .collect();
    assert_eq!(found[0]["hashes"], Value::Object(expected));
    let run_json = json_file(&run_dir.join("run.json"));
    assert_eq!(run_json["kind"], "rollback");
    assert_eq!(run_json["target_run_id"], run_id.as_str());
    assert_eq!(json_file(&run_dir.join("result.json"))["ok"], true);
}

#[tokio::test]
async fn journal_corrupt_blocks() {
    let rig = changing_rig().await;
    let before = collection_state(&rig);
    std::fs::create_dir_all(rig.deploy_dir()).expect("deploy dir");
    std::fs::write(journal_path(&rig), b"{ this is not a journal").expect("journal");

    let err = deploy_plan(
        &rig.declared,
        rig.reader.as_ref(),
        &PlanOptions {
            base: None,
            diff: false,
        },
    )
    .await
    .expect_err("plan refuses over an unreadable journal");
    assert_invalid(&err, "unknown", "journal.json");
    let err = rig
        .prepare(GateFlags::default())
        .await
        .expect_err("apply refuses");
    assert_invalid(&err, "unknown", "journal.json");
    let fresh = Fresh::new(&rig).await;
    for run in ["20261016T000000Z-aaaaaa", "20261016T000000Z-bbbbbb"] {
        let err = fresh
            .prepare(&rig, run, false)
            .await
            .expect_err("rollback refuses");
        assert_invalid(&err, "unknown", "journal.json");
    }
    only_reads(&fresh);
    assert_eq!(collection_state(&rig), before);
    assert!(!rig.repo.join(".ign").exists(), "no run was made");
}

fn snapshot_dir(rig: &Rig, run_id: &str) -> PathBuf {
    rig.deploy_dir().join("snapshots").join(run_id)
}

fn write_journal_phase(rig: &Rig, phase: JournalPhase) {
    let journal = Journal {
        phase,
        ..read_journal(rig)
    };
    std::fs::write(journal_path(rig), journal.to_bytes()).expect("rewrite the journal");
}

/// Put the pre-image of `path` back from the on-box snapshot: with the one
/// file a killed apply had moved, the tree is as it was before the run.
fn unmove(rig: &Rig, run_id: &str, path: &str) {
    std::fs::copy(
        snapshot_dir(rig, run_id).join("files").join(path),
        rig.fx.collection_dir(MODE).join(path),
    )
    .expect("put the pre-image back");
}

/// Recover `run_id` from a fresh process: its prepare and run.
async fn recover(rig: &Rig, run_id: &str) -> (Fresh, Result<DeployRollbackResult, CoreError>) {
    let fresh = Fresh::new(rig).await;
    let result = fresh.rollback(rig, run_id, false).await;
    (fresh, result)
}

#[tokio::test]
async fn journal_died_before_move_is_cleared() {
    // D-10: no move began and the touched set is what it was before the run.
    for phase in [JournalPhase::Ready, JournalPhase::Snapshotting] {
        let killed = killed_mid_write().await;
        let (rig, run_id) = (killed.rig.clone(), killed.journal.run_id.clone());
        unmove(&rig, &run_id, DB_CONFIG);
        write_journal_phase(&rig, phase);
        assert_eq!(collection_state(&rig), killed.before, "an untouched tree");

        let fresh = Fresh::new(&rig).await;
        let prepared = fresh.prepare(&rig, &run_id, false).await.expect("prepare");
        assert!(prepared.prose.contains("interrupted"), "{}", prepared.prose);
        let result = fresh
            .run(&rig, prepared, &run_id, false)
            .await
            .expect("the run is cleared");

        assert_eq!(result.outcome, RollbackKind::Cleared, "{phase:?}");
        assert!(result.ok);
        assert!(result.restored.is_empty() && result.deleted.is_empty());
        assert_eq!(collection_state(&rig), killed.before, "left as it was");
        assert!(!journal_path(&rig).exists(), "the journal is removed");
        assert!(!snapshot_dir(&rig, &run_id).exists(), "and the snapshot");
        assert!(
            !rig.deploy_dir().join("staging").join(&run_id).exists(),
            "and the staging"
        );
        let calls = fresh.host.calls();
        assert!(
            !calls.contains(&"restore") && !calls.contains(&"finalize"),
            "no restore: {calls:?}"
        );
        assert!(calls.contains(&"discard_interrupted"), "{calls:?}");
        assert_eq!(fresh.fake.scan_count(), 0, "no scan");
        assert_eq!(fresh.fake.lock_acquires(), 0, "no lock");
        let run_dir = PathBuf::from(&result.run_dir);
        let found = event_lines(&run_dir, "interrupted_run");
        assert_eq!(found[0]["action"], "clear", "{found:?}");
        assert_eq!(found[0]["phase"], phase.as_str());
        let result_json = json_file(&run_dir.join("result.json"));
        assert_eq!(result_json["outcome"], "cleared");
        assert_eq!(result_json["ok"], true);

        // A cleared run leaves nothing to plan around.
        assert_eq!(plan_hash(&rig).await, killed.plan_before);
    }
}

#[tokio::test]
async fn journal_before_move_but_changed_restores() {
    // D-10 negative: no move began, but the touched set is not what it was.
    let killed = killed_mid_write().await;
    let (rig, run_id) = (killed.rig.clone(), killed.journal.run_id.clone());
    unmove(&rig, &run_id, DB_CONFIG);
    rig.fx.mode(DB_CONFIG, b"{\"edited\": \"by someone else\"}");
    write_journal_phase(&rig, JournalPhase::Snapshotting);
    assert_ne!(collection_state(&rig), killed.before);

    let (fresh, result) = recover(&rig, &run_id).await;
    let result = result.expect("a changed tree is restored, not cleared");
    assert_eq!(result.outcome, RollbackKind::Restored);
    assert_eq!(
        collection_state(&rig),
        killed.before,
        "the snapshot is back"
    );
    assert!(fresh.host.calls().contains(&"restore"));
    assert_eq!(fresh.fake.scan_count(), 1);
    assert!(!journal_path(&rig).exists());
    let found = event_lines(&PathBuf::from(&result.run_dir), "interrupted_run");
    assert_eq!(found[0]["action"], "restore", "{found:?}");
}

#[tokio::test]
async fn journal_snapshot_lost_is_rollback_failed() {
    // D-12.
    let killed = killed_mid_write().await;
    let (rig, run_id) = (killed.rig.clone(), killed.journal.run_id.clone());
    let torn = collection_state(&rig);
    std::fs::remove_file(snapshot_dir(&rig, &run_id).join("files").join(DB_CONFIG))
        .expect("lose a snapshot file");

    let (fresh, result) = recover(&rig, &run_id).await;
    let err = result.expect_err("a lost snapshot cannot be restored from");
    let CoreError::RollbackFailed(failure) = &err else {
        panic!("expected rollback_failed, got {err:?}");
    };
    assert_eq!(failure.run_id, run_id);
    assert!(failure.journal.ends_with("journal.json"), "{failure:?}");
    let text = err.to_string();
    assert!(text.contains(".gwbk"), "{text}");
    // The retry first, then the intact pre-images on the box (review WR-02);
    // the run directory has no snapshot to offer after a failed restore.
    assert!(
        text.contains(&format!("ign deploy rollback --run {run_id} --env dev")),
        "{text}"
    );
    assert!(
        text.contains(&format!("snapshots/{run_id}/")),
        "the on-box snapshot is named: {text}"
    );
    assert!(!text.contains("run-directory snapshot"), "{text}");
    assert!(text.contains(&failure.journal), "{text}");
    assert!(text.contains(DB_CONFIG), "the path is named: {text}");
    assert_eq!(err.exit_code(), 6);

    // The journal stays and nothing was half restored.
    assert!(journal_path(&rig).is_file(), "the journal stays");
    assert_eq!(read_journal(&rig).run_id, run_id);
    assert_eq!(collection_state(&rig), torn);
    assert!(!fresh.host.calls().contains(&"restore"), "no half restore");
    assert_eq!(fresh.fake.scan_count(), 0);
}

/// A real apply whose `finalize` hangs once the verify was green, then
/// killed: the journal is at `written` and the record still `applying`.
async fn killed_before_finalize() -> Killed {
    let notify = Arc::new(tokio::sync::Notify::new());
    let rig = changing_rig().await;
    let host = rig.host.clone().hang("finalize", 1, notify.clone());
    kill_apply(Rig { host, ..rig }, notify).await
}

#[tokio::test]
async fn journal_without_record_post_part_recovers() {
    // Pitfall 14: the ledger was swapped and the record's post part never
    // written; the journal and the snapshot are enough.
    let killed = killed_before_finalize().await;
    let (rig, run_id) = (killed.rig.clone(), killed.journal.run_id.clone());
    assert_eq!(killed.journal.phase, JournalPhase::Written);
    let record = rig.record(&run_id);
    assert_eq!(record.state, RecordState::Applying);
    assert_eq!(record.post_fingerprint, None);
    let applied = collection_state(&rig);
    assert_ne!(applied, killed.before, "the whole batch had been written");
    assert_ne!(
        applied.get(LEDGER_KEY),
        killed.before.get(LEDGER_KEY),
        "the ledger was swapped"
    );

    let (fresh, result) = recover(&rig, &run_id).await;
    let result = result.expect("recovered");
    assert_eq!(result.outcome, RollbackKind::Restored);
    assert_eq!(collection_state(&rig), killed.before, "files and ledger");
    assert!(!journal_path(&rig).exists());
    assert_eq!(rig.record(&run_id).state, RecordState::RolledBack);
    assert_eq!(fresh.fake.scan_count(), 1);
}

#[tokio::test]
async fn journal_without_record_recovers() {
    // Pitfall 14, the other half: the record itself is lost.
    let killed = killed_mid_write().await;
    let (rig, run_id) = (killed.rig.clone(), killed.journal.run_id.clone());
    std::fs::remove_file(snapshot_dir(&rig, &run_id).join("record.json")).expect("lose the record");

    let (fresh, result) = recover(&rig, &run_id).await;
    let result = result.expect("recovered from the journal and the snapshot");
    assert_eq!(result.outcome, RollbackKind::Restored);
    assert_eq!(collection_state(&rig), killed.before);
    assert_eq!(
        result.restored,
        [DB_CONFIG.to_string(), DB_RESOURCE.to_string()],
        "the pre-images the snapshot holds"
    );
    assert_eq!(
        result.deleted,
        [ALARM_CONFIG.to_string(), ALARM_RESOURCE.to_string()]
    );
    assert!(!journal_path(&rig).exists());
    assert_eq!(rig.record(&run_id).state, RecordState::RolledBack);
    assert_eq!(fresh.fake.scan_count(), 1);

    // An incomplete snapshot cannot be told from a created file: with the
    // record gone, the journal's pre-run fingerprint is the check.
    let killed = killed_mid_write().await;
    let (rig, run_id) = (killed.rig.clone(), killed.journal.run_id.clone());
    std::fs::remove_file(snapshot_dir(&rig, &run_id).join("record.json")).expect("lose the record");
    std::fs::remove_file(snapshot_dir(&rig, &run_id).join("files").join(DB_CONFIG))
        .expect("lose a snapshot file");
    let (_, result) = recover(&rig, &run_id).await;
    let err = result.expect_err("an incomplete snapshot");
    assert_eq!(err.code(), "rollback_failed", "{err}");
    assert!(journal_path(&rig).is_file());
}

#[tokio::test]
async fn journal_after_green_verify_restores() {
    // A9: killed between a green verify and finalize; recovery restores, the
    // documented safe direction.
    let killed = killed_before_finalize().await;
    let (rig, run_id) = (killed.rig.clone(), killed.journal.run_id.clone());
    let killed_dir = rig.repo.join(".ign/deploy").join(GATEWAY).join(&run_id);
    let events = event_names(&killed_dir);
    assert!(events.contains(&"verify_done".to_string()), "{events:?}");
    assert!(!events.contains(&"finalized".to_string()), "{events:?}");
    let verified = event_lines(&killed_dir, "verify_done");
    assert_eq!(verified[0]["ok"], true, "the verify was green");

    let (_, result) = recover(&rig, &run_id).await;
    let result = result.expect("recovered");
    assert_eq!(result.outcome, RollbackKind::Restored);
    assert_eq!(
        collection_state(&rig),
        killed.before,
        "the good deploy is undone"
    );
    assert!(!journal_path(&rig).exists());
}

#[tokio::test]
async fn journal_recover_twice_refuses() {
    // Edge DAPL-09 idempotency.
    let killed = killed_mid_write().await;
    let (rig, run_id) = (killed.rig.clone(), killed.journal.run_id.clone());
    let (_, first) = recover(&rig, &run_id).await;
    let first = first.expect("the first recovery");
    assert_eq!(collection_state(&rig), killed.before);
    let record_after = std::fs::read(record_path(&rig, &run_id)).expect("record");

    let second = Fresh::new(&rig).await;
    let err = second
        .rollback(&rig, &run_id, false)
        .await
        .expect_err("a second recovery refuses");
    assert_invalid(
        &err,
        &run_id,
        &format!("already rolled back by {}", first.run_id),
    );
    only_reads(&second);
    assert_eq!(collection_state(&rig), killed.before, "nothing changed");
    assert_eq!(
        std::fs::read(record_path(&rig, &run_id)).expect("record"),
        record_after
    );

    // After a clear there is no record at all.
    let killed = killed_mid_write().await;
    let (rig, run_id) = (killed.rig.clone(), killed.journal.run_id.clone());
    unmove(&rig, &run_id, DB_CONFIG);
    write_journal_phase(&rig, JournalPhase::Ready);
    let (_, cleared) = recover(&rig, &run_id).await;
    assert_eq!(cleared.expect("cleared").outcome, RollbackKind::Cleared);
    let second = Fresh::new(&rig).await;
    let err = second
        .rollback(&rig, &run_id, false)
        .await
        .expect_err("nothing to recover");
    assert_invalid(&err, &run_id, "record.json");
    only_reads(&second);
    assert_eq!(collection_state(&rig), killed.before);
}

/// 23-17 carry-over: over ssh a dropped connection mid-commit is an `Err`
/// with the on-box state unknown. The engine must not call that "nothing was
/// written": the journal stays, the run is reported as interrupted with the
/// recovery command, and nothing is restored on a guess (the remote script
/// may still be running).
#[tokio::test]
async fn journal_commit_error_after_moves_reports_the_interrupted_run() {
    // One file moved, then the move failed: the journal is at `writing`.
    let rig = changing_rig_faulted(Vec::new(), Some(ApplyFault::FailMoveAt(1))).await;
    let before = collection_state(&rig);
    let host = rig.host.clone().fail_after("commit", 1);
    let rig = Rig { host, ..rig };

    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let err = rig
        .run(prepared, &NoObserver)
        .await
        .expect_err("a commit that errs is not an applied run");
    let journal = read_journal(&rig);
    assert_eq!(journal.phase, JournalPhase::Writing, "the journal stays");
    assert_recovery_hint(&err, &journal.run_id);
    let torn = collection_state(&rig);
    assert_ne!(torn, before, "a file had moved");

    // Nothing was restored or finalized on a guess, and no scan was made.
    let calls = rig.host.calls();
    assert!(
        !calls.contains(&"restore") && !calls.contains(&"finalize"),
        "{calls:?}"
    );
    assert_eq!(rig.fake.scan_count(), 0);

    // The run directory says what happened, including the original failure.
    let result = failed_result(&rig);
    assert_eq!(result["ok"], false);
    assert_eq!(result["error"]["code"], "deploy_journal_present");
    let run_dir = rig
        .repo
        .join(".ign/deploy")
        .join(GATEWAY)
        .join(&journal.run_id);
    let interrupted = event_lines(&run_dir, "commit_interrupted");
    assert_eq!(interrupted.len(), 1, "{interrupted:?}");
    assert_eq!(interrupted[0]["code"], "transport_failed");

    // And `rollback --run` recovers it.
    let (_, recovered) = recover(&rig, &journal.run_id).await;
    let recovered = recovered.expect("recovered");
    assert_eq!(recovered.outcome, RollbackKind::Restored);
    assert_eq!(collection_state(&rig), before);
    assert!(!journal_path(&rig).exists());
}

#[tokio::test]
async fn journal_commit_error_with_nothing_on_the_box_stays_the_original_error() {
    // The host never ran: no journal, so "nothing was written" is true and the
    // transport failure is what the caller gets.
    let rig = changing_rig().await;
    let before = collection_state(&rig);
    let host = rig.host.clone().fail("commit", 1);
    let rig = Rig { host, ..rig };
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let err = rig
        .run(prepared, &NoObserver)
        .await
        .expect_err("the commit failed");
    assert_eq!(err.code(), "transport_failed", "{err}");
    assert!(!journal_path(&rig).exists());
    assert_eq!(collection_state(&rig), before);
    let result = failed_result(&rig);
    assert_eq!(result["error"]["code"], "transport_failed");
}

// ============================================================================
// Every failure after the commit is a restore or a typed refusal (review CR-01)
// ============================================================================

/// `rig` with `faults` armed once its first commit returned: the preflights,
/// the lock and the commit see a healthy gateway, what follows does not.
fn armed_after_commit(rig: Rig, faults: DeployFaults) -> Rig {
    let arm = faults.clone();
    let host = rig.host.clone().after("commit", 1, move || {
        arm.arm();
        async {}
    });
    Rig {
        host,
        faults,
        ..rig
    }
}

/// An apply over `rig` whose gateway calls named by `faults` fail after the
/// commit.
async fn apply_with_post_commit_faults(
    rig: Rig,
    faults: DeployFaults,
) -> (Rig, Result<DeployApplyResult, CoreError>) {
    let rig = armed_after_commit(rig, faults);
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let result = rig.run(prepared, &NoObserver).await;
    (rig, result)
}

/// The one warning of a rolled-back result that names the failed step.
fn post_commit_warning(result: &DeployApplyResult) -> &str {
    result
        .warnings
        .iter()
        .find(|w| w.starts_with("post_commit_error"))
        .map(String::as_str)
        .unwrap_or_else(|| panic!("no post_commit_error warning in {:?}", result.warnings))
}

/// A gateway error before the verdict puts the snapshot back, like a red
/// verify does, and says which step failed.
async fn assert_post_commit_error_restores(method: &'static str, step: &str, cause: &str) {
    let rig = changing_rig().await;
    let before = collection_state(&rig);
    let faults = DeployFaults::default().fail(method, 1);
    let (rig, result) = apply_with_post_commit_faults(rig, faults.clone()).await;

    let result = result.expect("a post-commit error is a restore, not an error");
    assert_eq!(faults.failed(method), 1, "the fault was reached");
    assert_rolled_back(&result, cause);
    assert_eq!(collection_state(&rig), before, "the snapshot is back");
    let warning = post_commit_warning(&result);
    assert!(
        warning.contains(step) && warning.contains("network_error"),
        "{warning}"
    );
    let run_id = result.run_id.clone().expect("run id");
    assert!(!journal_path(&rig).exists(), "the restore finalized");
    assert_eq!(rig.record(&run_id).state, RecordState::RolledBack);
    assert!(rig.host.calls().contains(&"restore"));
}

#[tokio::test]
async fn post_commit_lock_check_error_restores_as_lock_expired() {
    assert_post_commit_error_restores("scan_lock_info", "lock_check", "lock_expired").await;
}

#[tokio::test]
async fn post_commit_scan_error_restores_as_scan_incomplete() {
    assert_post_commit_error_restores("scan_request", "scan", "scan_incomplete").await;
}

#[tokio::test]
async fn post_commit_verify_error_restores_as_verify_failed() {
    assert_post_commit_error_restores("deployment_mode", "verify", "verify_failed").await;
}

#[tokio::test]
async fn post_commit_error_with_the_gateway_gone_is_rollback_failed_with_the_journal() {
    // The lock read fails and so does the restore's own: nothing can be put
    // back, so the run ends as a typed refusal that names the journal.
    let rig = changing_rig().await;
    let faults = DeployFaults::default().fail("scan_lock_info", 100);
    let (rig, result) = apply_with_post_commit_faults(rig, faults).await;
    let err = result.expect_err("the restore could not run");
    let CoreError::RollbackFailed(failure) = &err else {
        panic!("expected rollback_failed, got {err:?}");
    };
    assert!(failure.journal.ends_with("journal.json"), "{failure:?}");
    assert_eq!(err.exit_code(), 6);
    assert!(journal_path(&rig).is_file(), "the journal stays");
    assert!(
        !rig.host.calls().contains(&"restore"),
        "the restore never ran"
    );
}

/// An apply whose host fails `call` (the `nth` time, with `after` when the
/// work was done first) once the verify was green: the deploy is live and
/// verified but not recorded.
async fn green_then_host_error(
    call: &'static str,
    after: bool,
) -> (
    Rig,
    BTreeMap<String, String>,
    Result<DeployApplyResult, CoreError>,
) {
    let rig = changing_rig().await;
    let before = collection_state(&rig);
    let host = if after {
        rig.host.clone().fail_after(call, 1)
    } else {
        rig.host.clone().fail(call, 1)
    };
    let rig = Rig { host, ..rig };
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let result = rig.run(prepared, &NoObserver).await;
    (rig, before, result)
}

/// The interrupted-run error a failure past a green verify ends as: the
/// recovery command with the real env, the tree left as written, nothing
/// restored on a guess.
async fn assert_interrupted_after_green(
    rig: &Rig,
    before: &BTreeMap<String, String>,
    err: &CoreError,
) {
    let journal = read_journal(rig);
    assert_recovery_hint(err, &journal.run_id);
    assert_ne!(&collection_state(rig), before, "the new tree stays live");
    let calls = rig.host.calls();
    assert!(!calls.contains(&"restore"), "{calls:?}");
    let result = failed_result(rig);
    assert_eq!(result["error"]["code"], "deploy_journal_present");
    let run_dir = rig
        .repo
        .join(".ign/deploy")
        .join(GATEWAY)
        .join(&journal.run_id);
    let interrupted = event_lines(&run_dir, "commit_interrupted");
    assert_eq!(interrupted.len(), 1, "{interrupted:?}");
    assert_eq!(interrupted[0]["code"], "transport_failed");
    assert!(event_names(&run_dir).contains(&"verify_done".to_string()));

    // `rollback --run` is the recovery, as for a kill past a green verify.
    let (_, recovered) = recover(rig, &journal.run_id).await;
    assert_eq!(
        recovered.expect("recovered").outcome,
        RollbackKind::Restored
    );
    assert_eq!(&collection_state(rig), before);
    assert!(!journal_path(rig).exists());
}

#[tokio::test]
async fn green_verify_then_fingerprint_error_is_the_interrupted_run() {
    let (rig, before, result) = green_then_host_error("fingerprint", false).await;
    let err = result.expect_err("a deploy that was not recorded is not applied");
    assert_interrupted_after_green(&rig, &before, &err).await;
}

#[tokio::test]
async fn green_verify_then_finalize_error_is_the_interrupted_run() {
    let (rig, before, result) = green_then_host_error("finalize", false).await;
    let err = result.expect_err("a deploy that was not recorded is not applied");
    assert_interrupted_after_green(&rig, &before, &err).await;
}

#[tokio::test]
async fn green_verify_then_a_finalize_that_ran_but_reported_an_error_is_applied() {
    // The connection dropped after the box finished: the record is installed
    // and the journal is gone, so the deploy is complete and says so.
    let (rig, _, result) = green_then_host_error("finalize", true).await;
    let result = result.expect("the box finished the run");
    assert_eq!(result.outcome, OutcomeKind::Applied, "{result:?}");
    assert!(result.ok);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.starts_with("finalize_reported_error")),
        "{:?}",
        result.warnings
    );
    assert!(!journal_path(&rig).exists());
    let run_id = result.run_id.clone().expect("run id");
    assert_eq!(rig.record(&run_id).state, RecordState::Applied);
}

/// Review WR-05: a commit that errs while the box cannot be read is not
/// evidence that nothing was written. The error keeps its code and gains the
/// recovery command; the event log says the journal's state is unknown.
#[tokio::test]
async fn journal_commit_error_with_an_unreadable_box_names_the_recovery() {
    let rig = changing_rig().await;
    let host = rig.host.clone().fail("commit", 1);
    let rig = Rig { host, ..rig };
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    // A journal nobody can parse stands in for a box that cannot be read.
    std::fs::create_dir_all(rig.deploy_dir()).expect("deploy dir");
    std::fs::write(journal_path(&rig), b"not a journal").expect("garbage");

    let err = rig
        .run(prepared, &NoObserver)
        .await
        .expect_err("the commit failed");
    assert_eq!(err.code(), "transport_failed", "{err}");
    assert_eq!(err.exit_code(), 4);
    let run_id = failed_run_id(&rig);
    let text = err.to_string();
    assert!(
        text.contains(&format!("ign deploy rollback --run {run_id} --env dev")),
        "the recovery command in {text}"
    );
    assert!(text.contains("may be on the host"), "{text}");
    let run_dir = rig.repo.join(".ign/deploy").join(GATEWAY).join(&run_id);
    let interrupted = event_lines(&run_dir, "commit_interrupted");
    assert_eq!(interrupted.len(), 1, "{interrupted:?}");
    let code = interrupted[0]["code"].as_str().expect("code");
    assert!(
        code.starts_with("transport_failed") && code.contains("unknown"),
        "{code}"
    );
}

/// The run id of the one run directory of a failed run.
fn failed_run_id(rig: &Rig) -> String {
    let runs = names(&rig.repo.join(".ign/deploy").join(GATEWAY));
    assert_eq!(runs.len(), 1, "one run directory: {runs:?}");
    runs[0].clone()
}

/// Review WR-04: a commit that runs past the hold stops at its time budget
/// instead of moving files under a lapsed lock, and the run ends as a restore
/// with the cause `lock_expired`.
#[tokio::test]
async fn a_commit_that_outruns_the_hold_stops_and_restores_as_lock_expired() {
    // Each move takes 1.1 s on this host and the hold is 3 s, so the budget
    // (hold less the 2 s margin) is 1 s: the first move lands, the second never
    // starts.
    let rig = changing_rig_faulted(
        Vec::new(),
        Some(ApplyFault::SlowMoves(Duration::from_millis(1100))),
    )
    .await;
    let before = collection_state(&rig);
    let mut opts = options();
    opts.hold = HoldModel {
        hold_s: 3,
        commit_fixed_s: 0.1,
        commit_per_file_s: 0.01,
        p95_scan_s: 0.1,
    };
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let result = rig
        .run_with(prepared, &NoObserver, &SystemClock, &opts)
        .await
        .expect("an overrun is a restore, not an error");

    assert_rolled_back(&result, "lock_expired");
    assert_eq!(collection_state(&rig), before, "the snapshot is back");
    let run_dir = rig.run_dir(&result);
    let committed = event_lines(&run_dir, "committed");
    assert_eq!(committed.len(), 1, "{committed:?}");
    assert_eq!(committed[0]["moved"], 1, "one move landed, then it stopped");
    assert!(
        event_lines(&run_dir, "lock_verdict").is_empty(),
        "the stop went straight to the restore"
    );
    assert_eq!(
        rig.record(result.run_id.as_deref().unwrap()).state,
        RecordState::RolledBack
    );
    assert!(!journal_path(&rig).exists());
}

// ============================================================================
// The secret canary over the on-box files and an aborted run (23-15, DSURF-03)
// ============================================================================

/// The 23-15 canary. Seeded in a secret field of the live resource the apply
/// replaces, in the `.gwbk` the gateway serves and in gateway log lines.
const CANARY: &str = "IGN-CANARY-23-15";

fn holds_canary(bytes: &[u8]) -> bool {
    bytes.windows(CANARY.len()).any(|w| w == CANARY.as_bytes())
}

/// A rig whose live `POSTGRES` config holds [`CANARY`] in an `Embedded`
/// secret (the apply replaces it), whose `.gwbk` carries it and whose log
/// source returns an unrelated line naming it, plus `extra_logs`.
async fn canary_rig(extra_logs: Vec<Value>, fault: Option<ApplyFault>) -> Rig {
    let fx = PlanFixture::new();
    let (declared, _) = fx.load("dev");
    fx.live_render(MODE, &declared);
    fx.mode(
        DB_CONFIG,
        format!(r#"{{"x":{{"type":"Embedded","data":{{"ciphertext":"{CANARY}"}}}}}}"#).as_bytes(),
    );
    fx.remove(MODE, ALARM_UNIT);
    fx.ledger(GATEWAY, MODE, &[DB_CONFIG, DB_RESOURCE]);
    let host = local_host(&fx);
    let host = match fault {
        Some(fault) => host.with_apply_fault(fault),
        None => host,
    };
    let mut logs = vec![json!({
        "timestamp": 1_792_000_000_010_i64,
        "loggerName": "Unrelated.Logger",
        "level": "WARN",
        "message": format!("connection refused for password={CANARY}"),
    })];
    logs.extend(extra_logs);
    let rig = rig_with_logs(fx, host, None, logs).await;
    mount_gwbk(&rig._server, gwbk_zip_holding(Some(CANARY)), 1).await;
    rig
}

/// The run-directory files that hold [`CANARY`], skipping the two entries
/// that legitimately hold secrets (D-05): `snapshot/` and `*.gwbk`.
fn run_dir_canary_leaks(run_dir: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, found: &mut Vec<String>, seen: &mut usize) {
        for entry in std::fs::read_dir(dir).expect("read_dir") {
            let path = entry.expect("entry").path();
            let rel = path.strip_prefix(root).expect("below root");
            let exempt = rel.starts_with("snapshot")
                || (rel.components().count() == 1 && rel.extension().is_some_and(|e| e == "gwbk"));
            if exempt {
                continue;
            }
            if path.is_dir() {
                walk(root, &path, found, seen);
            } else {
                *seen += 1;
                if holds_canary(&std::fs::read(&path).expect("read")) {
                    found.push(rel.display().to_string());
                }
            }
        }
    }
    let mut found = Vec::new();
    let mut seen = 0;
    walk(run_dir, run_dir, &mut found, &mut seen);
    assert!(
        seen >= 3,
        "the scan walked the run files, not nothing: {seen}"
    );
    found
}

/// The on-box files of `run_id` that must be secret-free: the journal bytes
/// given (read while it existed), `record.json` and `ledger.json`.
fn assert_on_box_clean(rig: &Rig, run_id: &str, journal: Option<&[u8]>, what: &str) {
    if let Some(journal) = journal {
        assert!(!journal.is_empty(), "{what}: the journal was read");
        assert!(!holds_canary(journal), "{what}: canary in journal.json");
    }
    for rel in [
        format!("snapshots/{run_id}/record.json"),
        "ledger.json".to_string(),
    ] {
        let bytes = std::fs::read(rig.deploy_dir().join(&rel)).expect("on-box file");
        assert!(!holds_canary(&bytes), "{what}: canary in {rel}");
    }
}

/// The seeds are real: the pre-image the run keeps and the `.gwbk` hold the
/// canary, so the scans above skip something that matters.
fn assert_exempt_holds_canary(run_dir: &Path, run_id: &str) {
    let pre_image = run_dir.join("snapshot/files").join(DB_CONFIG);
    assert!(
        holds_canary(&std::fs::read(&pre_image).expect("pre-image")),
        "the snapshot holds the replaced secret"
    );
    let gwbk = run_dir.join(format!("{run_id}.gwbk"));
    assert!(
        holds_canary(&std::fs::read(&gwbk).expect(".gwbk")),
        "the .gwbk holds the canary"
    );
}

#[tokio::test]
async fn canary_on_box_files_clean() {
    // applied: the journal is read at the point it exists (after the commit,
    // before the finalize removes it).
    let rig = canary_rig(Vec::new(), None).await;
    let journal: Arc<Mutex<Vec<u8>>> = Arc::default();
    let journal_file = rig.deploy_dir().join("journal.json");
    let captured = journal.clone();
    let host = rig.host.clone().after("commit", 1, move || {
        *captured.lock().expect("journal") = std::fs::read(&journal_file).unwrap_or_default();
        async {}
    });
    let rig = Rig { host, ..rig };
    let recorder = Recorder::default();
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let result = rig.run(prepared, &recorder).await.expect("applied");
    assert_eq!(result.outcome, OutcomeKind::Applied);
    let run_id = result.run_id.clone().expect("run id");
    let journal = journal.lock().expect("journal").clone();
    assert_on_box_clean(&rig, &run_id, Some(&journal), "applied");
    let run_dir = rig.run_dir(&result);
    assert_eq!(run_dir_canary_leaks(&run_dir), Vec::<String>::new());
    assert_exempt_holds_canary(&run_dir, &run_id);
    let serialized = serde_json::to_string(&result).expect("result serializes");
    assert!(!serialized.contains(CANARY), "canary in the result");
    let events = format!("{:?}", recorder.0.lock().expect("events"));
    assert!(!events.contains(CANARY), "canary in the observer events");

    // restored: a fresh-process rollback of that run.
    let restored = Fresh::new(&rig)
        .await
        .rollback(&rig, &run_id, false)
        .await
        .expect("restored");
    assert_eq!(restored.outcome, RollbackKind::Restored);
    assert_on_box_clean(&rig, &run_id, None, "restored");
    assert_eq!(
        run_dir_canary_leaks(Path::new(&restored.run_dir)),
        Vec::<String>::new()
    );
    let serialized = serde_json::to_string(&restored).expect("result serializes");
    assert!(
        !serialized.contains(CANARY),
        "canary in the rollback result"
    );

    // rolled back: a red verify (the unloaded line carries the canary).
    let mut red = unloaded_log_line();
    red["message"] = json!(format!("{} {CANARY}", red["message"].as_str().expect("m")));
    let rig = canary_rig(vec![red], None).await;
    let journal: Arc<Mutex<Vec<u8>>> = Arc::default();
    let journal_file = rig.deploy_dir().join("journal.json");
    let captured = journal.clone();
    let host = rig.host.clone().after("commit", 1, move || {
        *captured.lock().expect("journal") = std::fs::read(&journal_file).unwrap_or_default();
        async {}
    });
    let rig = Rig { host, ..rig };
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let result = rig.run(prepared, &NoObserver).await.expect("a verdict");
    assert_eq!(result.outcome, OutcomeKind::RolledBack);
    let run_id = result.run_id.clone().expect("run id");
    let journal = journal.lock().expect("journal").clone();
    assert_on_box_clean(&rig, &run_id, Some(&journal), "rolled_back");
    let run_dir = rig.run_dir(&result);
    assert_eq!(run_dir_canary_leaks(&run_dir), Vec::<String>::new());
    let serialized = serde_json::to_string(&result).expect("result serializes");
    assert!(
        !serialized.contains(CANARY),
        "canary in the rolled-back result"
    );
}

#[tokio::test]
async fn canary_aborted_run_clean() {
    // Edge DSURF-03 concurrency: an apply killed after its first move leaves
    // a journal, a record and a partial events.ndjson. None holds the canary.
    let notify = Arc::new(tokio::sync::Notify::new());
    let rig = canary_rig(
        Vec::new(),
        Some(ApplyFault::HangAfterMove(1, notify.clone())),
    )
    .await;
    let Killed { rig, journal, .. } = kill_apply(rig, notify).await;
    assert_eq!(journal.phase, JournalPhase::Writing);

    let journal_bytes = std::fs::read(journal_path(&rig)).expect("the journal");
    assert_on_box_clean(&rig, &journal.run_id, Some(&journal_bytes), "aborted");
    let run_dir = rig
        .repo
        .join(".ign/deploy")
        .join(GATEWAY)
        .join(&journal.run_id);
    let events = std::fs::read(run_dir.join("events.ndjson")).expect("partial events");
    assert!(!events.is_empty(), "the aborted run began its events");
    assert!(!holds_canary(&events), "canary in the partial events");
    assert_eq!(run_dir_canary_leaks(&run_dir), Vec::<String>::new());
    // The aborted run really was mid-write over the secret: the on-box
    // snapshot (a secret-bearing copy, exempt like the run directory's) holds
    // the pre-image the move replaced.
    let pre_image = rig
        .deploy_dir()
        .join("snapshots")
        .join(&journal.run_id)
        .join("files")
        .join(DB_CONFIG);
    assert!(
        holds_canary(&std::fs::read(&pre_image).expect("a pre-image on the box")),
        "the pre-image holds the secret the move replaced"
    );
}

// ============================================================================
// The backup phase (24-06)
// ============================================================================

/// Gives the target a backup node that is the other half of a redundant pair
/// (`PairLink`): its own gateway over its own data directory, its status and
/// log behind a wiremock server, its deploy surface behind `ForbidWrites`.
/// `files` gives the backup a local file transport over that directory;
/// `sync_after` is how many status reads the backup takes to sync.
async fn with_pair(rig: &mut Rig, files: bool, sync_after: usize) -> PairLink {
    use wiremock::matchers::{method, path};
    with_backup(rig, MODE).await;
    let dir = tempfile::tempdir().expect("backup data dir");
    let gateway = FakeGateway::with_data_dir(dir.path(), MODE);
    let link = PairLink::new(&gateway, rig.fx.data_dir(), dir.path(), MODE);
    link.sync_after(sync_after);
    rig.fake.with_pair(&link);
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(method("GET"))
        .and(path("/data/api/v1/redundancy"))
        .respond_with(link.status_responder())
        .mount(&server)
        .await;
    wiremock::Mock::given(method("GET"))
        .and(path("/data/api/v1/logs"))
        .respond_with(link.logs_responder())
        .mount(&server)
        .await;
    rig.backup = Some(Box::new(ForbidWrites(gateway.client("backup"))));
    rig.backup_fake = Some(gateway.clone());
    rig.backup_api = Some(ReqwestGatewayApi::for_tests(&server.uri(), None));
    rig.backup_reader =
        files.then(|| LocalFsTransport::new(dir.path().to_path_buf(), "nodes.backup"));
    rig.pair = Some(PairParts {
        link: link.clone(),
        server,
        _dir: dir,
        _gateway: gateway,
    });
    link
}

/// `options()` with a backup wait of `budget_ms` milliseconds per budget,
/// polled every 20 ms.
fn backup_options(budget_ms: u64) -> ApplyOptions {
    ApplyOptions {
        backup: BackupPolicy {
            budget: Duration::from_millis(budget_ms),
            interval: Duration::from_millis(20),
        },
        ..options()
    }
}

/// Records each event and lets a test act on it as it happens.
struct Hooked {
    seen: Mutex<Vec<ApplyEvent>>,
    hook: Box<dyn Fn(&ApplyEvent) + Send + Sync>,
}

impl Hooked {
    fn new(hook: impl Fn(&ApplyEvent) + Send + Sync + 'static) -> Self {
        Hooked {
            seen: Mutex::new(Vec::new()),
            hook: Box::new(hook),
        }
    }
}

impl ApplyObserver for Hooked {
    fn event(&self, e: &ApplyEvent) {
        (self.hook)(e);
        self.seen.lock().expect("events").push(e.clone());
    }
}

/// Prepare and run an apply with `opts`.
async fn apply_with(
    rig: &Rig,
    opts: &ApplyOptions,
    observer: &dyn ApplyObserver,
) -> DeployApplyResult {
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    rig.run_with(prepared, observer, &SystemClock, opts)
        .await
        .expect("the run ends with a verdict")
}

/// The event names with each run of `backup_poll` collapsed to one.
fn collapsed_events(run_dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in event_names(run_dir) {
        if name == "backup_poll" && out.last().is_some_and(|last| last == "backup_poll") {
            continue;
        }
        out.push(name);
    }
    out
}

#[tokio::test]
async fn backup_converges_after_apply() {
    let mut rig = changing_rig().await;
    let link = with_pair(&mut rig, false, 2).await;
    let result = apply_with(&rig, &backup_options(2000), &NoObserver).await;

    assert_eq!(result.outcome, OutcomeKind::Applied, "{result:?}");
    assert!(result.ok);
    assert_eq!(result.slug, None);
    assert_eq!(result.failure_exit_code(), None);
    let backup = result.backup.as_ref().expect("a backup report");
    assert!(backup.converged, "{backup:?}");
    assert_eq!(backup.reason, None);
    assert_eq!(backup.resync, None);
    let (before, after) = (backup.last_sync_before, backup.last_sync_after);
    assert!(
        before.is_some() && after > before,
        "{before:?} -> {after:?}"
    );
    assert_eq!(after, Some(link.timestamp()));
    assert_eq!(rig.fake.resyncs(), 0);

    // The same object is in result.json; a REST-only backup warns on the
    // file check and is still converged (D-04).
    let run_dir = rig.run_dir(&result);
    let written = json_file(&run_dir.join("result.json"));
    assert_eq!(written["outcome"], "applied");
    assert_eq!(written["ok"], true);
    assert_eq!(written["backup"], serde_json::to_value(backup).unwrap());
    assert_eq!(written["backup"]["converged"], true);
    assert_eq!(written["backup"]["reason"], Value::Null);
    assert_eq!(written["backup"]["resync"], Value::Null);
    let checks = written["backup"]["checks"].as_array().expect("checks");
    let ids: Vec<&str> = checks.iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["redundancy", "mode", "read_back", "files"]);
    assert_eq!(checks[0]["status"], "pass");
    assert_eq!(checks[3]["status"], "warn");
    assert!(
        checks[3]["detail"]
            .as_str()
            .unwrap()
            .contains("the file-hash check did not run"),
        "{}",
        checks[3]["detail"]
    );

    // The backup events come after the master is finalized.
    let events = collapsed_events(&run_dir);
    assert_eq!(
        events,
        [
            "run_started",
            "backup_sampled",
            "gwbk_downloaded",
            "staged",
            "lock_acquired",
            "committed",
            "lock_verdict",
            "scan_done",
            "verify_done",
            "finalized",
            "backup_wait_started",
            "backup_poll",
            "backup_verdict",
        ]
    );
    assert!(
        event_names(&run_dir)
            .iter()
            .filter(|n| *n == "backup_poll")
            .count()
            >= 3,
        "the backup took three reads to sync"
    );
    let verdict = &event_lines(&run_dir, "backup_verdict")[0];
    assert_eq!(verdict["converged"], true);
    assert_eq!(verdict["reason"], Value::Null);
}

#[tokio::test]
async fn backup_phase_is_read_only() {
    let mut rig = changing_rig().await;
    with_pair(&mut rig, false, 1).await;
    // `ForbidWrites` on the backup's deploy surface panics on any write.
    let result = apply_with(&rig, &backup_options(2000), &NoObserver).await;
    assert_eq!(result.outcome, OutcomeKind::Applied, "{result:?}");

    let pair = rig.pair.as_ref().expect("pair");
    let requests = pair.server.received_requests().await.expect("recording");
    assert!(!requests.is_empty(), "the backup was read");
    for request in &requests {
        assert_eq!(request.method.as_str(), "GET", "{}", request.url);
    }
    assert_eq!(rig.fake.resyncs(), 0, "no resync was needed");
    assert_eq!(pair.link.resyncs(), 0);
}

#[tokio::test]
async fn backup_wait_holds_no_lock() {
    let mut rig = changing_rig().await;
    with_pair(&mut rig, false, 2).await;
    let fake = rig.fake.clone();
    let held_at_wait = Arc::new(Mutex::new(Vec::<bool>::new()));
    let sink = Arc::clone(&held_at_wait);
    let observer = Hooked::new(move |event| {
        if matches!(
            event,
            ApplyEvent::BackupWaitStarted { .. } | ApplyEvent::BackupPoll { .. }
        ) {
            sink.lock().expect("sink").push(fake.config_lock_held());
        }
    });
    let result = apply_with(&rig, &backup_options(2000), &observer).await;
    assert_eq!(result.outcome, OutcomeKind::Applied, "{result:?}");

    let held = held_at_wait.lock().expect("sink");
    assert!(held.len() >= 4, "the wait started and polled: {held:?}");
    assert!(held.iter().all(|h| !h), "no lock is held in the wait");
    // The wait spent no hold budget: one lock for the whole run.
    assert_eq!(rig.fake.lock_acquires(), 1);
}

#[tokio::test]
async fn no_backup_node_reports_null() {
    let rig = changing_rig().await;
    let result = apply_with(&rig, &backup_options(200), &NoObserver).await;
    assert_eq!(result.outcome, OutcomeKind::Applied, "{result:?}");
    assert_eq!(result.backup, None);
    let run_dir = rig.run_dir(&result);
    assert_eq!(
        json_file(&run_dir.join("result.json"))["backup"],
        Value::Null
    );
    assert!(
        event_names(&run_dir)
            .iter()
            .all(|n| !n.starts_with("backup_")),
        "{:?}",
        event_names(&run_dir)
    );
}

/// The master was never undone by the backup phase: the on-box record says
/// `applied`, no restore event was logged and the collection holds the
/// render (D-08, DPAIR-02).
fn assert_master_stands(rig: &Rig, result: &DeployApplyResult) {
    let run_id = result.run_id.as_deref().expect("a run id");
    assert_eq!(rig.record(run_id).state, RecordState::Applied);
    let events = event_names(&rig.run_dir(result));
    assert!(
        !events
            .iter()
            .any(|n| n == "restore_started" || n == "restored"),
        "{events:?}"
    );
    assert!(!rig.deploy_dir().join("journal.json").exists());
    assert!(!result.written.is_empty());
    for path in &result.written {
        let rendered = rig
            .declared
            .outcome
            .tree
            .files
            .iter()
            .find(|(p, _)| p.as_str() == path)
            .map(|(_, bytes)| bytes.clone())
            .expect("rendered");
        assert_eq!(rig.fx.read(MODE, path), rendered, "{path}");
    }
}

/// The unsynced verdict of `result`, with the checks every such run shares.
fn assert_unsynced(rig: &Rig, result: &DeployApplyResult, reason: UnsyncedReason) {
    assert_eq!(
        result.outcome,
        OutcomeKind::AppliedBackupUnsynced,
        "{result:?}"
    );
    assert!(!result.ok);
    assert_eq!(result.slug, Some("deploy_backup_unsynced"));
    assert_eq!(result.cause, None);
    assert_eq!(result.failure_exit_code(), Some(6));
    let backup = result.backup.as_ref().expect("a backup report");
    assert!(!backup.converged, "{backup:?}");
    assert_eq!(backup.reason, Some(reason), "{backup:?}");
    assert_master_stands(rig, result);

    let written = json_file(&rig.run_dir(result).join("result.json"));
    assert_eq!(written["outcome"], "applied_backup_unsynced");
    assert_eq!(written["ok"], false);
    assert_eq!(written["slug"], "deploy_backup_unsynced");
    assert_eq!(written["cause"], Value::Null);
    assert_eq!(written["backup"]["reason"], reason.as_str());
    let verdict = &event_lines(&rig.run_dir(result), "backup_verdict")[0];
    assert_eq!(verdict["converged"], false);
    assert_eq!(verdict["reason"], reason.as_str());
}

#[tokio::test]
async fn backup_unsynced_not_syncing() {
    let mut rig = changing_rig().await;
    let link = with_pair(&mut rig, false, 0).await;
    link.cut();
    let result = apply_with(&rig, &backup_options(200), &NoObserver).await;

    assert_unsynced(&rig, &result, UnsyncedReason::NotSyncing);
    let resync = result
        .backup
        .as_ref()
        .unwrap()
        .resync
        .as_ref()
        .expect("a resync");
    assert!(resync.ok);
    assert_eq!(resync.code, None);
    assert_eq!(rig.fake.resyncs(), 1);
    assert_eq!(link.resyncs(), 1);
    let events = event_lines(&rig.run_dir(&result), "backup_resync");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["ok"], true);
}

#[tokio::test]
async fn backup_converges_after_resync() {
    let mut rig = changing_rig().await;
    let link = with_pair(&mut rig, false, usize::MAX).await;
    let result = apply_with(&rig, &backup_options(200), &NoObserver).await;

    assert_eq!(result.outcome, OutcomeKind::Applied, "{result:?}");
    assert!(result.ok);
    let backup = result.backup.as_ref().expect("a backup report");
    assert!(backup.converged, "{backup:?}");
    assert_eq!(backup.reason, None);
    let resync = backup.resync.as_ref().expect("the resync that helped");
    assert!(resync.ok);
    assert_eq!(link.resyncs(), 1);
    assert_eq!(rig.fake.resyncs(), 1);
    assert_master_stands(&rig, &result);
    let events = collapsed_events(&rig.run_dir(&result));
    let at = |name: &str| events.iter().position(|n| n == name).unwrap();
    assert!(at("backup_resync") > at("backup_wait_started"));
    assert!(at("backup_verdict") > at("backup_resync"));
}

#[tokio::test]
async fn backup_unsynced_unreachable() {
    let mut rig = changing_rig().await;
    let link = with_pair(&mut rig, false, 0).await;
    let cut = link.clone();
    // The sample is read; the backup's REST then stops answering.
    let observer = Hooked::new(move |event| {
        if matches!(event, ApplyEvent::BackupSampled { .. }) {
            cut.unreachable();
        }
    });
    let result = apply_with(&rig, &backup_options(200), &observer).await;

    assert_unsynced(&rig, &result, UnsyncedReason::Unreachable);
    let backup = result.backup.as_ref().unwrap();
    let detail = &backup.checks[0].detail;
    assert!(detail.starts_with("unreachable: "), "{detail}");
    let code = detail.trim_start_matches("unreachable: ");
    assert!(
        !code.is_empty() && code.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
        "a code, not a message: {detail}"
    );
}

#[tokio::test]
async fn backup_unsynced_content_differs() {
    let mut rig = changing_rig().await;
    let link = with_pair(&mut rig, false, 0).await;
    link.diverge(DB_CONFIG);
    let result = apply_with(&rig, &backup_options(200), &NoObserver).await;

    assert_unsynced(&rig, &result, UnsyncedReason::ContentDiffers);
    let backup = result.backup.as_ref().unwrap();
    // The sync happened: only the content is wrong.
    assert!(
        backup.last_sync_after > backup.last_sync_before,
        "{backup:?}"
    );
    let read_back = backup
        .checks
        .iter()
        .find(|c| c.id.word() == "read_back")
        .expect("a read_back check");
    assert_eq!(read_back.status.word(), "fail", "{read_back:?}");
    assert!(
        read_back
            .findings
            .iter()
            .any(|f| f.subject.contains("POSTGRES")),
        "the check names the resource: {read_back:?}"
    );
}

#[tokio::test]
async fn backup_resync_failure_is_recorded() {
    // A transport error on the one resync POST. The stub fails EVERY call, so
    // a second attempt would be counted too.
    let mut rig = changing_rig().await;
    let link = with_pair(&mut rig, false, 0).await;
    link.cut();
    rig.faults = DeployFaults::default().fail("redundancy_resync", usize::MAX);
    rig.faults.arm();
    let result = apply_with(&rig, &backup_options(1000), &NoObserver).await;

    assert_unsynced(&rig, &result, UnsyncedReason::NotSyncing);
    let backup = result.backup.as_ref().unwrap();
    let resync = backup
        .resync
        .as_ref()
        .expect("the failed resync is recorded");
    assert!(!resync.ok);
    let code = resync.code.as_deref().expect("a code");
    assert!(
        !code.is_empty() && !code.contains("faulty-gateway"),
        "{code}"
    );
    assert_eq!(rig.faults.failed("redundancy_resync"), 1, "one attempt");
    assert_eq!(rig.fake.resyncs(), 0, "the stub intercepted the POST");
    assert_eq!(link.resyncs(), 0);
    let run_dir = rig.run_dir(&result);
    let resyncs = event_lines(&run_dir, "backup_resync");
    assert_eq!(resyncs.len(), 1);
    assert_eq!(resyncs[0]["ok"], false);
    assert_eq!(resyncs[0]["code"], code);
    // The second budget still ran: looks after the resync, and the wait
    // covers both budgets of one second.
    let names = event_names(&run_dir);
    let resync_at = names.iter().position(|n| n == "backup_resync").unwrap();
    assert!(
        names[resync_at..].iter().any(|n| n == "backup_poll"),
        "{names:?}"
    );
    assert!(backup.waited_s >= 2, "waited {} s", backup.waited_s);
}

#[tokio::test]
async fn backup_resync_refused_is_not_resent() {
    // A non-2xx answer (the error an HTTP 500 maps to) on the one POST.
    let mut rig = changing_rig().await;
    let link = with_pair(&mut rig, false, 0).await;
    link.cut();
    link.refuse_resync();
    let result = apply_with(&rig, &backup_options(300), &NoObserver).await;

    assert_unsynced(&rig, &result, UnsyncedReason::NotSyncing);
    assert_eq!(
        link.resyncs(),
        1,
        "every attempt is counted, refused ones too"
    );
    let resync = result
        .backup
        .as_ref()
        .unwrap()
        .resync
        .as_ref()
        .expect("recorded");
    assert!(!resync.ok);
    let code = resync.code.as_deref().expect("a code");
    assert!(
        !code.contains("500") && !code.contains("Server Error"),
        "the code, never the message: {code}"
    );
    let run_dir = rig.run_dir(&result);
    assert_eq!(event_lines(&run_dir, "backup_resync").len(), 1);
    let names = event_names(&run_dir);
    let resync_at = names.iter().position(|n| n == "backup_resync").unwrap();
    assert!(names[resync_at..].iter().any(|n| n == "backup_poll"));
}

#[tokio::test]
async fn backup_timestamp_frozen() {
    // K1 a: the content syncs but the timestamp never moves, so the backup
    // is not proven synced.
    let mut rig = changing_rig().await;
    let link = with_pair(&mut rig, false, 0).await;
    link.freeze_timestamp();
    let result = apply_with(&rig, &backup_options(200), &NoObserver).await;

    assert_unsynced(&rig, &result, UnsyncedReason::NotSyncing);
    let backup = result.backup.as_ref().unwrap();
    assert_eq!(backup.last_sync_after, backup.last_sync_before);
}

#[test]
fn backup_slug_is_not_a_core_error() {
    use ignition_core::deploy::apply::BACKUP_UNSYNCED_SLUG;
    assert_eq!(BACKUP_UNSYNCED_SLUG, "deploy_backup_unsynced");
    assert!(!CoreError::DEPLOY_SLUGS.contains(&BACKUP_UNSYNCED_SLUG));
    assert_eq!(CoreError::DEPLOY_SLUGS.len(), 12);
}

#[tokio::test]
async fn backup_null_for_noop_and_rolled_back() {
    // A noop with a paired backup: nothing is sent to either node.
    let fx = PlanFixture::new();
    let (declared, _) = fx.load("dev");
    fx.live_render(MODE, &declared);
    let host = local_host(&fx);
    let mut rig = rig_with(fx, host, None).await;
    with_pair(&mut rig, false, 0).await;
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let result = rig
        .run_with(prepared, &NoObserver, &SystemClock, &backup_options(200))
        .await
        .expect("noop");
    assert_eq!(result.outcome, OutcomeKind::Noop);
    assert_eq!(result.backup, None);
    let pair = rig.pair.as_ref().unwrap();
    assert!(
        pair.server.received_requests().await.unwrap().is_empty(),
        "a noop reads no node"
    );

    // A rolled-back run (a red verify) has no backup phase either.
    let mut rig = changing_rig_with_logs(vec![unloaded_log_line()]).await;
    with_pair(&mut rig, false, 0).await;
    let result = apply_with(&rig, &backup_options(200), &NoObserver).await;
    assert_eq!(result.outcome, OutcomeKind::RolledBack, "{result:?}");
    assert_eq!(result.backup, None);
    let run_dir = rig.run_dir(&result);
    assert_eq!(
        json_file(&run_dir.join("result.json"))["backup"],
        Value::Null
    );
    // The sample came before the first write; nothing else of the phase ran.
    for name in event_names(&run_dir) {
        assert!(
            !matches!(
                name.as_str(),
                "backup_wait_started" | "backup_poll" | "backup_resync" | "backup_verdict"
            ),
            "{name}"
        );
    }
    assert_eq!(rig.fake.resyncs(), 0);
}

#[tokio::test]
async fn backup_sample_failure_aborts_before_any_write() {
    // V5: a pre-apply sample that cannot be read ends the run with its own
    // error, before the .gwbk, the staging, the lock or any write.
    let mut rig = changing_rig().await;
    let link = with_pair(&mut rig, false, 0).await;
    link.unreachable();
    let before = collection_state(&rig);
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let outcome = rig
        .run_with(prepared, &NoObserver, &SystemClock, &backup_options(200))
        .await;
    assert!(outcome.is_err(), "{outcome:?}");
    assert_eq!(collection_state(&rig), before, "nothing was written");
    assert_eq!(rig.fake.lock_acquires(), 0);
    assert_eq!(rig.fake.scan_count(), 0);
    assert_eq!(rig.host.calls(), ["probe"]);
}

#[tokio::test]
async fn backup_cut_at_lock_acquired_ends_unsynced() {
    // K3 b: the unsynced gate cuts the link from an observer when the engine
    // reports `lock_acquired` -- after the preflights read both nodes and the
    // sample was taken, before the first write.
    let mut rig = changing_rig().await;
    let link = with_pair(&mut rig, false, 0).await;
    let old_db = rig.fx.read(MODE, DB_CONFIG);
    let (cutter, fake) = (link.clone(), rig.fake.clone());
    let data_dir = rig.fx.data_dir().to_path_buf();
    let at_cut = Arc::new(Mutex::new(None::<(Vec<u8>, bool)>));
    let sink = Arc::clone(&at_cut);
    let observer = Hooked::new(move |event| {
        if matches!(event, ApplyEvent::LockAcquired { .. }) {
            let live = std::fs::read(data_dir.join("config/resources").join(MODE).join(DB_CONFIG))
                .expect("the master file");
            *sink.lock().expect("sink") = Some((live, fake.config_lock_held()));
            cutter.cut();
        }
    });
    let result = apply_with(&rig, &backup_options(200), &observer).await;

    assert_unsynced(&rig, &result, UnsyncedReason::NotSyncing);
    let (live, held) = at_cut
        .lock()
        .expect("sink")
        .clone()
        .expect("lock_acquired was delivered");
    assert_eq!(live, old_db, "no write had happened at lock_acquired");
    assert!(held, "the lock was held at the event");
    let seen = observer.seen.lock().expect("events");
    let position = |pred: fn(&ApplyEvent) -> bool| seen.iter().position(pred).unwrap();
    let sampled = position(|e| matches!(e, ApplyEvent::BackupSampled { .. }));
    let locked = position(|e| matches!(e, ApplyEvent::LockAcquired { .. }));
    let committed = position(|e| matches!(e, ApplyEvent::Committed { .. }));
    assert!(sampled < locked && locked < committed, "{seen:?}");
}

/// The phase's `files` check in `result.json`.
fn files_check_of(result: &DeployApplyResult) -> Value {
    let report = serde_json::to_value(result.backup.as_ref().expect("a backup report")).unwrap();
    report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "files")
        .expect("a files check")
        .clone()
}

#[tokio::test]
async fn backup_files_equal_master() {
    let mut rig = changing_rig().await;
    with_pair(&mut rig, true, 0).await;
    let result = apply_with(&rig, &backup_options(2000), &NoObserver).await;

    assert_eq!(result.outcome, OutcomeKind::Applied, "{result:?}");
    let backup = result.backup.as_ref().expect("a backup report");
    assert!(backup.converged, "{backup:?}");
    let files = files_check_of(&result);
    assert_eq!(files["status"], "pass", "{files}");
    assert_eq!(backup.files_checked, rig.declared.outcome.tree.files.len());
    assert!(backup.files_checked >= 4);
    assert_eq!(rig.fake.resyncs(), 0);
}

#[tokio::test]
async fn backup_files_differ() {
    let mut rig = changing_rig().await;
    let link = with_pair(&mut rig, true, 0).await;
    // One extra byte on the backup after the sync; the REST view is intact.
    link.tamper(DB_RESOURCE);
    let result = apply_with(&rig, &backup_options(200), &NoObserver).await;

    assert_unsynced(&rig, &result, UnsyncedReason::ContentDiffers);
    let files = files_check_of(&result);
    assert_eq!(files["status"], "fail", "{files}");
    let findings = files["findings"].as_array().unwrap();
    assert!(
        findings
            .iter()
            .any(|f| f["subject"] == DB_RESOURCE && f["code"] == "file_differs"),
        "{files}"
    );
    // Paths only: no byte of any file is in the report.
    let report = serde_json::to_string(result.backup.as_ref().unwrap()).unwrap();
    assert!(!report.contains("SERIAL NOT NULL"), "{report}");
}

#[tokio::test]
async fn backup_delete_left_behind() {
    let fx = PlanFixture::new();
    let (declared, _) = fx.load("dev");
    fx.live_render(MODE, &declared);
    // A unit an earlier run wrote and the render no longer holds: the plan
    // deletes it from the master.
    let old = "ignition/database-translator/OLD";
    fx.mode(&format!("{old}/config.json"), b"{}");
    fx.mode(&format!("{old}/resource.json"), b"{}");
    fx.ledger(
        GATEWAY,
        MODE,
        &[
            DB_CONFIG,
            DB_RESOURCE,
            &format!("{old}/config.json"),
            &format!("{old}/resource.json"),
        ],
    );
    let host = local_host(&fx);
    let mut rig = rig_with(fx, host, None).await;
    let link = with_pair(&mut rig, true, 0).await;
    // The backup still has the unit after it syncs.
    link.keep(&format!("{old}/config.json"));
    link.keep(&format!("{old}/resource.json"));
    let prepared = rig
        .prepare(GateFlags {
            allow_destructive: true,
            ..GateFlags::default()
        })
        .await
        .expect("prepare");
    let result = rig
        .run_with(prepared, &NoObserver, &SystemClock, &backup_options(200))
        .await
        .expect("the run ends with a verdict");

    assert_eq!(result.deleted.len(), 2, "{result:?}");
    assert_eq!(
        result.outcome,
        OutcomeKind::AppliedBackupUnsynced,
        "{result:?}"
    );
    let backup = result.backup.as_ref().unwrap();
    assert_eq!(
        backup.reason,
        Some(UnsyncedReason::ContentDiffers),
        "{backup:?}"
    );
    let files = files_check_of(&result);
    assert_eq!(files["status"], "fail", "{files}");
    let unexpected: Vec<&str> = files["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["code"] == "file_unexpected")
        .map(|f| f["subject"].as_str().unwrap())
        .collect();
    assert_eq!(
        unexpected,
        [format!("{old}/config.json"), format!("{old}/resource.json")]
    );
}

#[tokio::test]
async fn backup_sync_exhausted_ends_early() {
    // TEC-5105, K6 a: on the signal the wait ends, the one resync is sent,
    // the final evaluation runs and the run reports `sync_exhausted`, with no
    // second budget. The master is never undone.
    let mut rig = changing_rig().await;
    let link = with_pair(&mut rig, false, 0).await;
    link.exhaust(false);
    let result = apply_with(&rig, &backup_options(2000), &NoObserver).await;

    assert_unsynced(&rig, &result, UnsyncedReason::SyncExhausted);
    let backup = result.backup.as_ref().unwrap();
    // `not_syncing` is also true here (nothing synced); the precedence picks
    // the gateway giving up.
    assert!(!backup.checks[0].findings.is_empty());
    assert_eq!(link.resyncs(), 1, "the one resync is still sent");
    assert_eq!(rig.fake.resyncs(), 1);
    // One look finds the line, one final evaluation confirms it: neither
    // budget (two seconds each) ran out.
    let run_dir = rig.run_dir(&result);
    let polls = event_names(&run_dir)
        .iter()
        .filter(|n| *n == "backup_poll")
        .count();
    assert_eq!(polls, 2);
    assert!(backup.waited_s < 2, "waited {} s", backup.waited_s);
    assert_eq!(event_lines(&run_dir, "backup_resync").len(), 1);
}

#[tokio::test]
async fn backup_ignores_a_stale_sync_exhausted_line() {
    // WR-01: a sync-exhausted line logged before this run (here five minutes
    // ago, inside the old ten-minute window) is not the gateway giving up
    // now. The backup syncs on its second look; no resync is sent and the
    // verdict is converged.
    let mut rig = changing_rig().await;
    let link = with_pair(&mut rig, false, 2).await;
    link.stale_exhausted_line(300_000);
    let result = apply_with(&rig, &backup_options(2000), &NoObserver).await;

    assert_eq!(result.outcome, OutcomeKind::Applied, "{result:?}");
    let backup = result.backup.as_ref().expect("a backup report");
    assert!(backup.converged, "{backup:?}");
    assert_eq!(backup.reason, None);
    assert_eq!(backup.resync, None);
    assert_eq!(link.resyncs(), 0, "a stale line sends no resync");
    assert_eq!(rig.fake.resyncs(), 0);
}

#[tokio::test]
async fn backup_interrupt_leaves_master_applied() {
    // Edge DPAIR-01 concurrency: a run interrupted during the backup wait
    // leaves the master as verified and finalized.
    let mut rig = changing_rig().await;
    let link = with_pair(&mut rig, false, 0).await;
    link.cut();
    let waiting = Arc::new(tokio::sync::Notify::new());
    let signal = Arc::clone(&waiting);
    let observer = Hooked::new(move |event| {
        if matches!(event, ApplyEvent::BackupWaitStarted { .. }) {
            signal.notify_one();
        }
    });
    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let opts = backup_options(60_000);
    let run = rig.run_with(prepared, &observer, &SystemClock, &opts);
    tokio::select! {
        _ = run => panic!("a 60 s wait does not end by itself"),
        () = async {
            waiting.notified().await;
            tokio::time::sleep(Duration::from_millis(150)).await;
        } => {}
    }

    // The future was dropped mid-wait.
    let root = rig.repo.join(".ign/deploy").join(GATEWAY);
    let runs = names(&root);
    assert_eq!(runs.len(), 1, "{runs:?}");
    let run_dir = root.join(&runs[0]);
    assert_eq!(rig.record(&runs[0]).state, RecordState::Applied);
    assert!(!rig.deploy_dir().join("journal.json").exists());
    assert!(!run_dir.join("result.json").exists());
    let last = event_names(&run_dir).pop().expect("events");
    assert!(
        last == "backup_wait_started" || last == "backup_poll",
        "the log ends in the wait: {last}"
    );
    assert!(
        !event_names(&run_dir)
            .iter()
            .any(|n| n == "restore_started" || n == "restored")
    );
    for path in [DB_CONFIG, DB_RESOURCE, ALARM_CONFIG, ALARM_RESOURCE] {
        let rendered = rig
            .declared
            .outcome
            .tree
            .files
            .iter()
            .find(|(p, _)| p.as_str() == path)
            .map(|(_, bytes)| bytes.clone())
            .expect("rendered");
        assert_eq!(rig.fx.read(MODE, path), rendered, "{path}");
    }
}

#[tokio::test]
async fn backup_repeat_apply_is_noop() {
    // Edge DPAIR-01 idempotency: the same render a second time is a noop that
    // sends nothing to either node.
    let mut rig = changing_rig().await;
    with_pair(&mut rig, false, 0).await;
    let first = apply_with(&rig, &backup_options(2000), &NoObserver).await;
    assert_eq!(first.outcome, OutcomeKind::Applied, "{first:?}");
    let seen = rig
        .pair
        .as_ref()
        .unwrap()
        .server
        .received_requests()
        .await
        .unwrap()
        .len();
    let (locks, scans) = (rig.fake.lock_acquires(), rig.fake.scan_count());

    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let second = rig
        .run_with(prepared, &NoObserver, &SystemClock, &backup_options(2000))
        .await
        .expect("noop");
    assert_eq!(second.outcome, OutcomeKind::Noop);
    assert_eq!(second.backup, None);
    let pair = rig.pair.as_ref().unwrap();
    assert_eq!(pair.server.received_requests().await.unwrap().len(), seen);
    assert_eq!(pair.link.resyncs(), 0);
    assert_eq!(
        (rig.fake.lock_acquires(), rig.fake.scan_count()),
        (locks, scans),
        "no request reached the master either"
    );
}

// ============================================================================
// The secret canary over the backup's surfaces (24-09, DSURF-03)
// ============================================================================

/// The 24-09 canary. Seeded in a backup-only resource file, in a rendered
/// backup file that differs from the master's, in an extra member of the
/// backup's redundancy status and in a backup log entry of an unrelated
/// logger: everything the runner reads from the backup.
const CANARY_PAIR: &str = "IGN-CANARY-24";

fn holds_pair_canary(bytes: &[u8]) -> bool {
    bytes
        .windows(CANARY_PAIR.len())
        .any(|w| w == CANARY_PAIR.as_bytes())
}

/// Every non-exempt run-directory file that holds [`CANARY_PAIR`], and how
/// many files were walked.
fn run_dir_pair_canary_leaks(run_dir: &Path) -> (Vec<String>, usize) {
    fn walk(root: &Path, dir: &Path, found: &mut Vec<String>, seen: &mut usize) {
        for entry in std::fs::read_dir(dir).expect("read_dir") {
            let path = entry.expect("entry").path();
            let rel = path.strip_prefix(root).expect("below root");
            let exempt = rel.starts_with("snapshot")
                || (rel.components().count() == 1 && rel.extension().is_some_and(|e| e == "gwbk"));
            if exempt {
                continue;
            }
            if path.is_dir() {
                walk(root, &path, found, seen);
            } else {
                *seen += 1;
                if holds_pair_canary(&std::fs::read(&path).expect("read")) {
                    found.push(rel.display().to_string());
                }
            }
        }
    }
    let mut found = Vec::new();
    let mut seen = 0;
    walk(run_dir, run_dir, &mut found, &mut seen);
    (found, seen)
}

/// Seed [`CANARY_PAIR`] where the backup could hand it to the runner. With
/// `rendered` the backup's copy of the translator config is a secret-bearing
/// document that differs from the master's (the files check reads it and
/// reports the path); without it a backup-only unit that a sync leaves alone
/// carries the secret and every rendered file stays equal.
fn seed_pair_canary(rig: &Rig, link: &PairLink, rendered: bool) -> PathBuf {
    let secret =
        format!(r#"{{"x":{{"type":"Embedded","data":{{"ciphertext":"{CANARY_PAIR}"}}}}}}"#);
    let root = rig
        .pair
        .as_ref()
        .expect("a pair rig")
        ._dir
        .path()
        .join("config/resources")
        .join(MODE);
    let seeded = if rendered {
        link.keep(DB_CONFIG);
        root.join(DB_CONFIG)
    } else {
        let leak = "ignition/database-translator/LEAK/config.json";
        link.keep(leak);
        let path = root.join(leak);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("leak dir");
        path
    };
    std::fs::write(&seeded, secret).expect("seed the backup file");
    link.status_extra("peerNote", json!(format!("token={CANARY_PAIR}")));
    link.log_entry(json!({
        "timestamp": 1_792_000_000_020_i64,
        "loggerName": "Unrelated.Logger",
        "level": "WARN",
        "message": format!("connection refused for password={CANARY_PAIR}"),
    }));
    seeded
}

#[tokio::test]
async fn canary_backup_surfaces() {
    // converged: every rendered file equal, the secret in a backup-only unit
    // the files check never compares but the reader lists.
    let mut rig = changing_rig().await;
    let link = with_pair(&mut rig, true, 2).await;
    let seeded = seed_pair_canary(&rig, &link, false);
    let recorder = Recorder::default();
    let result = apply_with(&rig, &backup_options(2000), &recorder).await;
    assert_eq!(result.outcome, OutcomeKind::Applied, "{result:?}");
    assert!(result.backup.as_ref().expect("a backup report").converged);
    assert!(
        holds_pair_canary(&std::fs::read(&seeded).expect("seeded file")),
        "the backup file still holds the canary: the seed was real"
    );
    let served = rig
        .pair
        .as_ref()
        .unwrap()
        .server
        .received_requests()
        .await
        .unwrap();
    assert!(
        served
            .iter()
            .any(|r| r.url.path() == "/data/api/v1/redundancy"),
        "the status that carries the canary was read"
    );
    let serialized = serde_json::to_string(&result).expect("result serializes");
    assert!(!serialized.contains(CANARY_PAIR), "canary in the result");
    let events = format!("{:?}", recorder.0.lock().expect("events"));
    assert!(
        !events.contains(CANARY_PAIR),
        "canary in the observer events"
    );
    let (leaks, seen) = run_dir_pair_canary_leaks(&rig.run_dir(&result));
    assert!(seen >= 3, "the scan walked the run files: {seen}");
    assert_eq!(leaks, Vec::<String>::new());

    // unsynced: the link is down, so the backup's rendered file keeps the
    // secret-bearing document and differs; the report names the path only.
    let mut rig = changing_rig().await;
    let link = with_pair(&mut rig, true, 0).await;
    link.cut();
    let seeded = seed_pair_canary(&rig, &link, true);
    let recorder = Recorder::default();
    let result = apply_with(&rig, &backup_options(200), &recorder).await;
    assert_unsynced(&rig, &result, UnsyncedReason::NotSyncing);
    assert!(
        holds_pair_canary(&std::fs::read(&seeded).expect("seeded file")),
        "the differing backup file holds the canary: the seed was real"
    );
    let files = files_check_of(&result);
    assert_eq!(
        files["status"], "fail",
        "the canary file was compared: {files}"
    );
    assert!(
        files["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["subject"] == DB_CONFIG && f["code"] == "file_differs"),
        "{files}"
    );
    let served = rig
        .pair
        .as_ref()
        .unwrap()
        .server
        .received_requests()
        .await
        .unwrap();
    assert!(
        served.iter().any(|r| r.url.path() == "/data/api/v1/logs"),
        "the log that carries the canary was read"
    );
    let serialized = serde_json::to_string(&result).expect("result serializes");
    assert!(
        !serialized.contains(CANARY_PAIR),
        "canary in the unsynced result"
    );
    let events = format!("{:?}", recorder.0.lock().expect("events"));
    assert!(
        !events.contains(CANARY_PAIR),
        "canary in the observer events"
    );
    let (leaks, seen) = run_dir_pair_canary_leaks(&rig.run_dir(&result));
    assert!(seen >= 3, "the scan walked the run files: {seen}");
    assert_eq!(leaks, Vec::<String>::new());
}
