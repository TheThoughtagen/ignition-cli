//! The apply engine's suite (Phase 23). It starts with the gate layer over a
//! real plan (`load_declared`, the local transport, `deploy_plan`, then
//! `evaluate_gates`) and goes on to the engine's green path, its no-op and
//! the branches 23-17 will turn into restores, over a local node and the
//! scan-coupled fake gateway.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
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
use ignition_core::client::ReqwestGatewayApi;
use ignition_core::deploy::apply::fingerprint::{LEDGER_KEY, fingerprint};
use ignition_core::deploy::apply::gates::{GateFlags, GateOutcome, evaluate_gates};
use ignition_core::deploy::apply::hold::{Clock, HoldModel, SystemClock};
use ignition_core::deploy::apply::journal::{Journal, JournalPhase};
use ignition_core::deploy::apply::record::{RecordState, RunRecord};
use ignition_core::deploy::apply::{
    ApplyContext, ApplyEvent, ApplyObserver, ApplyOptions, NoObserver, OutcomeKind, RollbackCause,
    write_set,
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
    _server: wiremock::MockServer,
}

fn local_host(fx: &PlanFixture) -> LocalFsTransport {
    LocalFsTransport::new(fx.data_dir().to_path_buf(), "nodes.master")
        .with_apply(GATEWAY, MODE)
        .expect("apply host")
}

async fn rig_with(fx: PlanFixture, host: LocalFsTransport, fake: Option<FakeGateway>) -> Rig {
    let (declared, reader) = fx.load("dev");
    let repo = fx.repo("dev");
    let fake = fake.unwrap_or_else(|| FakeGateway::with_data_dir(fx.data_dir(), MODE));
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/logs"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "items": [],
            "metadata": {"total": 0, "matching": 0, "limit": 200, "offset": 0}
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
        _server: server,
    }
}

/// The tracer repo with the render live in the `dev` collection, then one
/// unit changed (POSTGRES, by one byte), one added (the alarm settings are
/// removed from the collection) and a prior ledger listing POSTGRES.
async fn changing_rig() -> Rig {
    let fx = PlanFixture::new();
    let (declared, _) = fx.load("dev");
    fx.live_render(MODE, &declared);
    let mut bytes = fx.read(MODE, DB_CONFIG);
    bytes.push(b' ');
    fx.mode(DB_CONFIG, &bytes);
    fx.remove(MODE, ALARM_UNIT);
    fx.ledger(GATEWAY, MODE, &[DB_CONFIG, DB_RESOURCE]);
    let host = local_host(&fx);
    rig_with(fx, host, None).await
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
async fn a_lock_that_lapsed_before_the_scan_is_never_applied() {
    let rig = changing_rig().await;
    let fake = rig.fake.clone();
    let host = rig.host.clone().after("commit", 1, move || {
        let fake = fake.clone();
        async move { fake.tick(HOLD_S as i64 + 1) }
    });
    let rig = Rig { host, ..rig };

    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let err = rig
        .run(prepared, &NoObserver)
        .await
        .expect_err("not applied");
    assert_eq!(err.code(), "internal", "{err}");
    assert!(err.to_string().contains("lock_expired"), "{err}");
    assert_eq!(
        rig.fake.scan_count(),
        0,
        "the scan is not triggered on a lapsed lock"
    );
    assert_eq!(failed_result(&rig)["ok"], false);
}

#[tokio::test]
async fn a_red_verify_is_never_applied() {
    // A gateway that never loads the new files: its REST view stays empty.
    let fx = PlanFixture::new();
    let (declared, _) = fx.load("dev");
    fx.live_render(MODE, &declared);
    let mut bytes = fx.read(MODE, DB_CONFIG);
    bytes.push(b' ');
    fx.mode(DB_CONFIG, &bytes);
    let host = local_host(&fx);
    let blind = FakeGateway::new();
    blind.set_deployment_mode(Some(MODE.to_string()));
    let rig = rig_with(fx, host, Some(blind)).await;

    let prepared = rig.prepare(GateFlags::default()).await.expect("prepare");
    let err = rig
        .run(prepared, &NoObserver)
        .await
        .expect_err("not applied");
    assert_eq!(err.code(), "internal", "{err}");
    assert!(err.to_string().contains("verify_failed"), "{err}");
    assert_eq!(rig.fake.scan_count(), 1);
    assert_eq!(failed_result(&rig)["ok"], false);
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
