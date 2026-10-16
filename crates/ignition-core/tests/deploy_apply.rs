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

use common::fake_deploy::FakeGateway;
use common::faulty_host::FaultyHost;
use common::plan_fixture::{GATEWAY, MODE, PlanFixture};
use ignition_core::actions::deploy::{
    Declared, DeployPlanResult, PlanOptions, VerifyOptions, deploy_plan,
};
use ignition_core::actions::deploy_apply::{
    ApplyPrepared, DeployApplyResult, PrepareOptions, deploy_apply_execute, deploy_apply_prepare,
};
use ignition_core::client::scan::ScanKind;
use ignition_core::client::{DeployApi, ReqwestGatewayApi};
use ignition_core::deploy::apply::fingerprint::{LEDGER_KEY, fingerprint};
use ignition_core::deploy::apply::gates::{GateFlags, GateOutcome, evaluate_gates};
use ignition_core::deploy::apply::hold::{Clock, HoldModel, ManualClock, SystemClock};
use ignition_core::deploy::apply::journal::{Journal, JournalPhase};
use ignition_core::deploy::apply::preflight::{
    DesignersPolicy, PreflightInput, PreflightReport, run_preflights,
};
use ignition_core::deploy::apply::record::{RecordState, RunRecord};
use ignition_core::deploy::apply::{
    ApplyContext, ApplyEvent, ApplyObserver, ApplyOptions, NoObserver, OutcomeKind, RollbackCause,
    Steps, write_set,
};
use ignition_core::deploy::ledger::Ledger;
use ignition_core::deploy::transport::TreeReader;
use ignition_core::deploy::transport::local::{ApplyFault, LocalFsTransport};
use ignition_core::deploy::tree::sha256_hex;
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
    /// The backup node's deploy surface, when the test gives the target one.
    backup: Option<common::fake_deploy::FakeDeploy>,
    _server: wiremock::MockServer,
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
        backup: None,
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
    let fx = PlanFixture::new();
    let (declared, _) = fx.load("dev");
    fx.live_render(MODE, &declared);
    let mut bytes = fx.read(MODE, DB_CONFIG);
    bytes.push(b' ');
    fx.mode(DB_CONFIG, &bytes);
    fx.remove(MODE, ALARM_UNIT);
    fx.ledger(GATEWAY, MODE, &[DB_CONFIG, DB_RESOURCE]);
    let host = local_host(&fx);
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
        let deploy = self.fake.client("apply");
        let ctx = ApplyContext {
            declared: &self.declared,
            repo: &self.repo,
            reader: self.reader.as_ref(),
            host: &self.host,
            deploy: &deploy,
            gateway: &self.api,
            backup_deploy: self.backup.as_ref().map(|b| b as &dyn DeployApi),
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
    assert!(matches!(seen[5], ApplyEvent::ScanDone));
    assert!(matches!(seen[6], ApplyEvent::VerifyDone { ok: true }));
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

    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
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
        seen[4],
        ApplyEvent::RestoreStarted {
            cause: RollbackCause::WriteFailed
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
        .scan_lock_acquire(ScanKind::Config, 0, 600)
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
                .scan_lock_acquire(ScanKind::Config, 0, 600)
                .await
                .expect("B takes the lock");
        }
    });
    let rig = Rig { host, ..rig };
    ran(rig, before, quiet_hook(), &SystemClock, &options()).await
}

const LAPSED_EVENTS: [&str; 9] = [
    "run_started",
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

/// Gives the target a backup node whose gateway runs `mode`.
fn with_backup(rig: &mut Rig, mode: &str) {
    let mut backup = rig.declared.target.nodes.master.clone();
    backup.profile = "backup".to_string();
    backup.local = None;
    backup.gateway_name = None;
    rig.declared.target.nodes.backup = Some(backup);
    let gateway = FakeGateway::new();
    gateway.set_deployment_mode(Some(mode.to_string()));
    rig.backup = Some(gateway.client("backup"));
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
    let mut steps = Steps::new(None, &NoObserver);
    run_preflights(
        &PreflightInput {
            target: &rig.declared.target,
            master: &deploy,
            backup: rig.backup.as_ref().map(|b| b as &dyn DeployApi),
            gateway: &rig.api,
            reader: rig.reader.as_ref(),
            host: &rig.host,
            touched_files,
            hold: &opts.hold,
            designers: &opts.designers,
            clock: &SystemClock,
        },
        &mut steps,
    )
    .await
}

#[tokio::test]
async fn preflight_backup_mode_mismatch() {
    let mut rig = changing_rig().await;
    with_backup(&mut rig, "production");
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
    with_backup(&mut rig, MODE);
    let report = preflights(&rig, &options(), 4).await.expect("preflights");
    assert!(report.passed(), "{report:?}");
    let backup = &report.findings[1];
    assert_eq!(backup.check.as_str(), "mode_backup");
    assert_eq!(backup.status.as_str(), "pass");
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
    with_backup(&mut rig, "production");
    rig.fake.set_deployment_mode(Some("production".to_string()));
    mount_gateway_name(&rig, "some-other-gateway").await;
    mount_scan_projects(&rig, 403).await;
    mount_designers(&rig, &[("alice", "PlantA", "10.0.0.5:52526")]).await;
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
    ];
    // Without permission enforcement (root) the host check cannot fail.
    if failed.len() == expected.len() {
        assert_eq!(failed, expected);
    } else {
        let without_host: Vec<&str> = expected
            .iter()
            .copied()
            .filter(|c| *c != "write_host")
            .collect();
        assert_eq!(failed, without_host);
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
