//! Phase 23's rig gates for the write flow against a real Ignition 8.3.6
//! gateway whose `config/resources` is bind-mounted (23-16): a guarded apply,
//! a rollback from a fresh context by run id, an apply killed mid-write, and
//! an apply whose scan-lock lapses before the scan.
//!
//! The host is a `local:` node on the rig's bind-mounted data directory, so
//! the engine writes the mode collection with plain file moves and the real
//! gateway reads them back through its REST view and its log. The gateway
//! calls (lock, scan, `.gwbk` download, read-back, log) go to the real rig
//! through [`ReqwestGatewayApi`]; nothing here is mocked.
//!
//! ```text
//! cd crates/ignition-core/tests/fixtures/live-rig-deploy
//! bash capture22.sh up22 && bash capture22.sh setup22
//! . ~/.cache/ign-deploy-rig/tokens.env
//! IGNITION_LIVE_DEPLOY=1 IGNITION_LIVE_URL=http://localhost:18290 \
//!   IGNITION_LIVE_TOKEN="$IGN_TOKEN_CAP_A" \
//!   IGN_DEPLOY_RIG_DATA="$HOME/.cache/ign-deploy-rig/data" IGN_DEPLOY_RIG_MODE=production \
//!   cargo test -p ignition-core --test live_deploy_apply -- --ignored --nocapture --test-threads=1
//! ```
//!
//! | var | meaning |
//! |---|---|
//! | `IGNITION_LIVE_DEPLOY` | must equal `1`; anything else is a quiet green skip |
//! | `IGNITION_LIVE_URL` | the rig's base URL; loopback hosts only (`localhost`, `127.0.0.1`, `::1`) |
//! | `IGNITION_LIVE_TOKEN` | cap-a's full `name:key` API token (never printed) |
//! | `IGN_DEPLOY_RIG_DATA` | the host data directory whose `config/resources` is bind-mounted into the rig |
//! | `IGN_DEPLOY_RIG_MODE` | the rig's active mode (`production` on the rig recipe) |
//!
//! Safety: the URL guard refuses a non-loopback host before a request is made
//! and a name guard refuses any gateway whose `gateway-info` name is not
//! `ign-deploy-rig`. Every test removes what it wrote (the declared units
//! from the mode collection and the `ign-deploy/` directory it created) even
//! when an assertion fails, and the tests share one gateway, so they take a
//! process-wide lock. Output carries counts, unit keys, run ids and timings,
//! never a token or a file's bytes.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::faulty_host::FaultyHost;
use ignition_core::actions::deploy::{
    Declared, PlanOptions, VerifyOptions, deploy_plan, load_declared, load_target,
};
use ignition_core::actions::deploy_apply::{
    ApplyPrepared, ApplyRunOptions, ApplySurfaces, DeployApplyResult, DeployRollbackResult,
    PrepareOptions, RollbackPrepareOptions, RollbackRunOptions, deploy_apply_execute,
    deploy_apply_prepare, deploy_apply_run, deploy_rollback_execute, deploy_rollback_prepare,
    deploy_rollback_prepare_for_env, deploy_rollback_run,
};
use ignition_core::client::deploy_api::resource_list_in_collection;
use ignition_core::client::logs::LogQuery;
use ignition_core::client::scan::{LockObservation, ScanKind};
use ignition_core::client::{DeployApi, GatewayApi, ReqwestGatewayApi};
use ignition_core::config::{Credential, Secret};
use ignition_core::deploy::apply::gates::GateFlags;
use ignition_core::deploy::apply::hold::{HOLD_FLOOR_S, HoldModel, SystemClock};
use ignition_core::deploy::apply::journal::Journal;
use ignition_core::deploy::apply::preflight::DesignersPolicy;
use ignition_core::deploy::apply::run_store::RunId;
use ignition_core::deploy::apply::{
    ApplyContext, ApplyOptions, NoObserver, OutcomeKind, RollbackCause, RollbackContext,
    RollbackKind, RollbackOptions,
};
use ignition_core::deploy::transport::local::{ApplyFault, LocalFsTransport};
use ignition_core::deploy::transport::{self, TreeReader, apply_host_for_node};
use ignition_core::deploy::tree::sha256_hex;
use ignition_core::deploy::verify::WILL_NOT_BE_LOADED;
use ignition_core::error::CoreError;
use ignition_core::poll::PollConfig;

/// The only gateway this gate may talk to.
const RIG_GATEWAY_NAME: &str = "ign-deploy-rig";

/// The gateway directory name in the repo and the deploy directory below the
/// data dir.
const GATEWAY_DIR: &str = "rig-gw";
const ENV: &str = "rig";

/// The tracer fixture's gateway directory: a `database-translator` and the
/// alarm settings, four files.
const TRACER_GATEWAY: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/deploy/repos/tracer/gateways/tracer-gw"
);

/// The units the tracer declares, as `(module, type)` and key.
const TRANSLATOR: (&str, &str) = ("ignition", "database-translator");
const TRANSLATOR_UNIT: &str = "ignition/database-translator/POSTGRES";
const ALARM_UNIT: &str = "ignition/general-alarm-settings";

/// The tests share one gateway; they run one at a time.
static RIG_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn skip(message: &str) {
    eprintln!("skipping: {message}");
}

/// Parse `raw` and accept it only when it points at this machine.
fn local_url_guard(raw: &str) -> Result<url::Url, String> {
    let parsed = url::Url::parse(raw).map_err(|err| format!("not a URL ({err})"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(format!("scheme {:?} is not http or https", parsed.scheme()));
    }
    let loopback = match parsed.host() {
        Some(url::Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(addr)) => addr == std::net::Ipv4Addr::LOCALHOST,
        Some(url::Host::Ipv6(addr)) => addr == std::net::Ipv6Addr::LOCALHOST,
        None => false,
    };
    if loopback {
        Ok(parsed)
    } else {
        Err(format!(
            "host {} is not loopback (localhost, 127.0.0.1, ::1)",
            parsed.host_str().unwrap_or("<none>")
        ))
    }
}

/// A non-empty env var, or a panic naming the variable (never its value).
fn require_env(name: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| panic!("{name} must be set to run the live apply gate"))
}

#[test]
fn local_url_guard_refuses_non_loopback() {
    for raw in [
        "http://10.4.8.15:8088",
        "https://wignvm01.example.local:8043",
        "http://localhost.evil.example:18290",
        "http://127.0.0.2:18290",
        "http://52.154.75.231:8088",
        "not a url",
    ] {
        assert!(local_url_guard(raw).is_err(), "must refuse {raw}");
    }
}

#[test]
fn local_url_guard_accepts_loopback() {
    for raw in [
        "http://localhost:18290",
        "http://127.0.0.1:18290",
        "http://[::1]:18290",
    ] {
        assert!(local_url_guard(raw).is_ok(), "must accept {raw}");
    }
}

/// What a live test needs once the gate is open.
struct Rig {
    url: String,
    token: String,
    data_dir: PathBuf,
    mode: String,
}

impl Rig {
    /// A new REST handle: a test that wants a fresh context builds another.
    fn api(&self) -> ReqwestGatewayApi {
        ReqwestGatewayApi::for_tests(
            &self.url,
            Some(Credential::Token(Secret::new(self.token.clone()))),
        )
    }

    fn mode_root(&self) -> PathBuf {
        self.data_dir
            .join("config")
            .join("resources")
            .join(&self.mode)
    }

    fn deploy_dir(&self) -> PathBuf {
        self.data_dir.join("ign-deploy")
    }
}

/// `None` is the quiet green skip. Every guard runs before the first request.
async fn rig() -> Option<Rig> {
    if std::env::var("IGNITION_LIVE_DEPLOY").as_deref() != Ok("1") {
        skip("IGNITION_LIVE_DEPLOY is not 1");
        return None;
    }
    let url = require_env("IGNITION_LIVE_URL");
    let token = require_env("IGNITION_LIVE_TOKEN");
    let data_dir = PathBuf::from(require_env("IGN_DEPLOY_RIG_DATA"));
    let mode = require_env("IGN_DEPLOY_RIG_MODE");
    if let Err(why) = local_url_guard(&url) {
        panic!("IGNITION_LIVE_URL refused: {why}");
    }
    assert!(
        data_dir.join("config").join("resources").is_dir(),
        "IGN_DEPLOY_RIG_DATA must be the host data dir whose config/resources is bind-mounted"
    );
    let rig = Rig {
        url,
        token,
        data_dir,
        mode,
    };
    let info = GatewayApi::gateway_info(&rig.api())
        .await
        .expect("gateway-info read for the name guard");
    assert_eq!(
        info.name.as_deref(),
        Some(RIG_GATEWAY_NAME),
        "refusing: this gateway is not the {RIG_GATEWAY_NAME} fixture"
    );
    Some(rig)
}

/// Removes what a test wrote into the rig, at the start (leftovers of a cut-off
/// run) and when the test ends, even by a panic: the declared units from the
/// mode collection and the `ign-deploy/` directory.
struct Clean {
    paths: Vec<PathBuf>,
}

impl Clean {
    fn new(rig: &Rig) -> Clean {
        let root = rig.mode_root();
        let clean = Clean {
            paths: vec![
                root.join(TRANSLATOR_UNIT),
                root.join(ALARM_UNIT),
                rig.deploy_dir(),
            ],
        };
        clean.sweep();
        clean
    }

    fn sweep(&self) {
        for path in &self.paths {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

impl Drop for Clean {
    fn drop(&mut self) {
        self.sweep();
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create dir");
    for entry in std::fs::read_dir(from).expect("read dir") {
        let entry = entry.expect("entry");
        let dest = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_dir(&entry.path(), &dest);
        } else {
            std::fs::copy(entry.path(), dest).expect("copy file");
        }
    }
}

/// A git repo whose `.gitignore` holds `.ign/`, whose target is the rig as a
/// `local:` node and whose gateway directory is the tracer's two units.
fn apply_repo(rig: &Rig) -> tempfile::TempDir {
    let repo = tempfile::tempdir().expect("temp repo");
    std::fs::create_dir_all(repo.path().join("targets")).expect("targets dir");
    copy_dir(
        Path::new(TRACER_GATEWAY),
        &repo.path().join("gateways").join(GATEWAY_DIR),
    );
    let yaml = format!(
        "schema_version: 1\ngateway: {GATEWAY_DIR}\nmode: {mode}\nnodes:\n  master:\n    profile: rig\n    local:\n      data_dir: {data}\n",
        mode = rig.mode,
        data = rig.data_dir.display(),
    );
    std::fs::write(repo.path().join("targets").join("rig.yaml"), yaml).expect("target file");
    std::fs::write(repo.path().join(".gitignore"), ".ign/\n").expect(".gitignore");
    let init = std::process::Command::new("git")
        .args(["init", "-q"])
        .arg(repo.path())
        .status()
        .expect("git init");
    assert!(init.success(), "git init failed");
    repo
}

/// Everything a fresh runner builds from the repo alone.
struct Context {
    declared: Declared,
    reader: Box<dyn TreeReader>,
    repo: PathBuf,
}

impl Context {
    fn new(repo: &Path) -> Context {
        let declared = load_declared(repo, ENV).expect("render of the repo");
        let (target_file, target) = load_target(repo, ENV).expect("target");
        let reader = transport::for_node(repo, &target_file, "nodes.master", &target.nodes.master)
            .expect("the master has host access");
        Context {
            declared,
            reader,
            repo: repo.to_path_buf(),
        }
    }

    /// `plan_sha256` of a plain `deploy plan`.
    async fn plan_hash(&self) -> String {
        deploy_plan(
            &self.declared,
            self.reader.as_ref(),
            &PlanOptions {
                base: None,
                diff: false,
            },
        )
        .await
        .expect("a plan")
        .plan_sha256
    }
}

/// `path` → sha256 of every file below the rig's mode collection, and each
/// directory as `path/` → empty: the state a rollback must give back.
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
                // A directory below `<module>/<type>` is the engine's to
                // remove when it empties; `<module>/<type>` and above are
                // never removed (the remove-emptied rule of the host ops),
                // so they are not part of the state a rollback gives back.
                if key.split('/').count() >= 3 {
                    out.insert(format!("{key}/"), String::new());
                }
                walk(&path, &key, out);
            } else if let Ok(bytes) = std::fs::read(&path) {
                out.insert(key, sha256_hex(&bytes));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(&rig.mode_root(), "", &mut out);
    out
}

/// The names of the database translators the gateway holds in the mode
/// collection (its REST view), sorted.
async fn translators_in_mode(rig: &Rig, api: &dyn DeployApi) -> Vec<String> {
    let mut names: Vec<String> =
        resource_list_in_collection(api, TRANSLATOR.0, TRANSLATOR.1, &rig.mode)
            .await
            .expect("REST list of the translators")
            .into_iter()
            .filter_map(|r| r.name)
            .collect();
    names.sort();
    names
}

/// Messages of WARN-or-higher gateway log entries since `since_ms` that say
/// the rule from CV1: `will not be loaded`.
async fn will_not_be_loaded_since(api: &dyn GatewayApi, since_ms: i64) -> Vec<String> {
    let query = LogQuery {
        start_time: Some(since_ms),
        min_level: Some("WARN".to_string()),
        limit: 500,
        ..LogQuery::default()
    };
    api.logs(&query)
        .await
        .expect("the gateway log")
        .items
        .into_iter()
        .filter(|entry| entry.message.contains(WILL_NOT_BE_LOADED))
        .map(|entry| entry.message)
        .collect()
}

/// The production scan wait: the same numbers as `scan_poll` in
/// `actions/deploy_apply.rs`.
fn production_poll(subject: &str) -> PollConfig {
    PollConfig {
        subject: subject.to_string(),
        interval: Duration::from_millis(500),
        max: Duration::from_secs(2),
        deadline: Duration::from_secs(120),
    }
}

fn gates_for(hash: &str) -> GateFlags {
    GateFlags {
        plan_sha256: Some(hash.to_string()),
        ..GateFlags::default()
    }
}

fn utc_now() -> String {
    let out = std::process::Command::new("date")
        .args(["-u", "+%FT%TZ"])
        .output()
        .expect("date");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A short hold for the fault tests: the floor, with a commit model small
/// enough that the D-03 refusal passes for four files.
fn short_hold(hold_s: u64) -> HoldModel {
    HoldModel {
        hold_s,
        commit_fixed_s: 0.5,
        commit_per_file_s: 0.01,
        p95_scan_s: 1.0,
    }
}

fn engine_options(hold: HoldModel, gates: GateFlags) -> ApplyOptions {
    ApplyOptions {
        base: None,
        gates,
        project: "ign-cli".to_string(),
        hold,
        poll: production_poll("config scan after apply"),
        verify: VerifyOptions::default(),
        designers: DesignersPolicy::default(),
        skip_gwbk: false,
    }
}

// ---- live_apply_then_rollback (the tracer) ------------------------------------------

#[tokio::test]
#[ignore = "live: needs the ign-deploy-rig fixture and IGNITION_LIVE_DEPLOY=1"]
async fn live_apply_then_rollback() {
    let Some(rig) = rig().await else { return };
    let _serial = RIG_LOCK.lock().await;
    let _clean = Clean::new(&rig);
    let started = Instant::now();
    eprintln!("live_apply_then_rollback @ {}", utc_now());

    let repo = apply_repo(&rig);
    let before = collection_state(&rig);
    let translators_before = translators_in_mode(&rig, &rig.api()).await;

    // The first plan, then the apply with the production entry (the measured
    // hold, the production scan wait, the factory host).
    let (run_id, plan0, applied_state) = {
        let ctx = Context::new(repo.path());
        let plan0 = ctx.plan_hash().await;
        let prepared = deploy_apply_prepare(
            &ctx.declared,
            ctx.reader.as_ref(),
            &PrepareOptions {
                base: None,
                gates: gates_for(&plan0),
            },
        )
        .await
        .expect("prepare over the rig");
        let ApplyPrepared::Ready { plan, .. } = &prepared else {
            panic!("a clean rig has the tracer to apply: {prepared:?}");
        };
        eprintln!(
            "plan: added {} changed {} removed {} plan_sha256 {}",
            plan.summary.added, plan.summary.changed, plan.summary.removed, plan.plan_sha256
        );
        assert!(plan.summary.has_changes, "{:?}", plan.summary);

        let api = rig.api();
        let surfaces = ApplySurfaces {
            deploy: &api,
            backup_deploy: None,
            gateway: &api,
        };
        let opts = ApplyRunOptions {
            base: None,
            gates: gates_for(&plan0),
            project: "ign-cli".to_string(),
            designers: DesignersPolicy::default(),
            skip_gwbk: false,
        };
        let result: DeployApplyResult = deploy_apply_run(
            prepared,
            &ctx.declared,
            &ctx.repo,
            ctx.reader.as_ref(),
            &surfaces,
            &opts,
            &NoObserver,
        )
        .await
        .expect("the apply ran");
        eprintln!(
            "apply: outcome {:?} run {:?} written {} deleted {} gwbk {:?}",
            result.outcome,
            result.run_id,
            result.written.len(),
            result.deleted.len(),
            result.gwbk
        );
        assert_eq!(result.outcome, OutcomeKind::Applied, "{result:?}");
        assert!(result.ok);
        assert_eq!(result.cause, None);
        for warning in &result.warnings {
            eprintln!("apply warning: {warning}");
        }
        let run_id = result.run_id.clone().expect("a run id");

        // The run directory is complete and holds the .gwbk.
        let run_dir = PathBuf::from(result.run_dir.as_deref().expect("a run directory"));
        for name in ["events.ndjson", "plan.json", "result.json", "run.json"] {
            assert!(
                run_dir.join(name).is_file(),
                "{name} in {}",
                run_dir.display()
            );
        }
        let gwbk = run_dir.join(result.gwbk.as_deref().expect("a .gwbk name"));
        let head = std::fs::read(&gwbk).expect("the .gwbk");
        assert!(head.starts_with(b"PK"), "the .gwbk is a ZIP");
        eprintln!("gwbk: {} bytes", head.len());

        // The events name the green verify and the finalize, in order.
        let events = std::fs::read_to_string(run_dir.join("events.ndjson")).expect("events");
        let names: Vec<String> = events
            .lines()
            .map(|line| {
                serde_json::from_str::<serde_json::Value>(line).expect("event line")["event"]
                    .as_str()
                    .expect("event name")
                    .to_string()
            })
            .collect();
        eprintln!("apply events: {}", names.join(" "));
        for wanted in [
            "lock_acquired",
            "committed",
            "scan_done",
            "verify_done",
            "finalized",
        ] {
            assert!(names.iter().any(|n| n == wanted), "{wanted} in {names:?}");
        }
        assert!(
            !names.iter().any(|n| n == "restore_started"),
            "a green apply restores nothing: {names:?}"
        );
        let verify_line = events
            .lines()
            .find(|l| l.contains("\"verify_done\""))
            .expect("verify_done");
        assert!(
            verify_line.contains("\"ok\":true"),
            "verify_done says ok: {verify_line}"
        );

        // The gateway holds the tracer's translator, and the tree is on disk.
        let after = collection_state(&rig);
        assert_ne!(after, before, "the apply changed the host");
        let names = translators_in_mode(&rig, &api).await;
        assert!(
            names.iter().any(|n| n == "POSTGRES"),
            "the REST view lists POSTGRES after the apply: {names:?}"
        );
        (run_id, plan0, after)
    };
    // Every handle above is dropped. Take the applying runner's run
    // directories away too: a rollback needs the on-box record only.
    std::fs::remove_dir_all(repo.path().join(".ign")).expect("remove the run directories");

    // A fresh context: new reader, new REST handle, a host from the factory.
    let ctx = Context::new(repo.path());
    let api = rig.api();
    let scan_before = api
        .scan_status(ScanKind::Config)
        .await
        .expect("scan status before the rollback");
    let prepared = deploy_rollback_prepare_for_env(
        &ctx.repo,
        ENV,
        &run_id,
        ctx.reader.as_ref(),
        &RollbackPrepareOptions::default(),
    )
    .await
    .expect("rollback prepare from the on-box record");
    assert!(prepared.prose.contains(&run_id), "{}", prepared.prose);
    assert!(
        prepared.live_changed.is_empty(),
        "{:?}",
        prepared.live_changed
    );
    let surfaces = ApplySurfaces {
        deploy: &api,
        backup_deploy: None,
        gateway: &api,
    };
    let rolled = deploy_rollback_run(
        prepared,
        ENV,
        &ctx.repo,
        ctx.reader.as_ref(),
        &surfaces,
        &RollbackRunOptions::default(),
        &NoObserver,
    )
    .await
    .expect("the rollback ran");
    eprintln!(
        "rollback: outcome {:?} run {} of {} restored {} deleted {}",
        rolled.outcome,
        rolled.run_id,
        rolled.target_run_id,
        rolled.restored.len(),
        rolled.deleted.len()
    );
    assert_eq!(rolled.outcome, RollbackKind::Restored);
    assert!(rolled.ok);
    assert_eq!(rolled.target_run_id, run_id);
    assert_ne!(rolled.run_id, run_id, "a rollback is a run of its own");
    assert!(!rolled.deleted.is_empty(), "the added files are removed");

    // A completed restore scan, and the tree back as it was.
    let scan_after = api
        .scan_status(ScanKind::Config)
        .await
        .expect("scan status after the rollback");
    assert!(!scan_after.scan_active, "no scan is left running");
    assert!(
        scan_after.last_scan_timestamp > scan_before.last_scan_timestamp,
        "the rollback scanned: {} -> {}",
        scan_before.last_scan_timestamp,
        scan_after.last_scan_timestamp
    );
    assert_ne!(applied_state, collection_state(&rig));
    assert_eq!(collection_state(&rig), before, "the pre-apply tree is back");
    assert_eq!(translators_in_mode(&rig, &api).await, translators_before);

    // CV1: no `will not be loaded` line since the restore scan began.
    let since =
        scan_after.last_scan_timestamp - scan_after.last_scan_duration.unwrap_or(30_000) - 1_000;
    tokio::time::sleep(Duration::from_secs(6)).await;
    let bad = will_not_be_loaded_since(&api, since).await;
    assert!(bad.is_empty(), "lines since the restore scan: {bad:?}");

    // The plan equals the first plan.
    let again = Context::new(repo.path()).plan_hash().await;
    assert_eq!(again, plan0, "deploy plan equals the pre-apply plan");
    eprintln!(
        "PASS live_apply_then_rollback in {:.1}s (plan {plan0})",
        started.elapsed().as_secs_f64()
    );
}

// ---- live_kill_mid_write_then_recover -------------------------------------------------

/// The recovery of an interrupted run: `rollback --run <its own id>` from a
/// fresh context (reader, host from the factory, REST handle), no flag. The
/// dead apply's scan-lock may still be held (`killed_at` is when it died), so
/// the first attempt is made at once and what the gateway does is printed; on
/// `scan_lock_held` the recovery waits for the lock to read free and tries
/// again.
async fn recover_after_kill(
    rig: &Rig,
    repo: &Path,
    run_id: &str,
    killed_at: Instant,
) -> DeployRollbackResult {
    let fresh = Context::new(repo);
    let api = rig.api();
    let (target_file, target) = load_target(&fresh.repo, ENV).expect("target");
    let host = apply_host_for_node(
        &fresh.repo,
        &target_file,
        "nodes.master",
        &target.nodes.master,
        &target.gateway,
        &target.mode,
    )
    .expect("factory host");
    let recover = || async {
        let prepared = deploy_rollback_prepare(
            &fresh.repo,
            ENV,
            run_id,
            fresh.reader.as_ref(),
            host.as_ref(),
            &RollbackPrepareOptions::default(),
        )
        .await?;
        let rctx = RollbackContext {
            env: ENV,
            repo: &fresh.repo,
            target_file: &target_file,
            target: &target,
            reader: fresh.reader.as_ref(),
            host: host.as_ref(),
            deploy: &api,
            gateway: &api,
            backup_deploy: None,
            clock: &SystemClock,
            observer: &NoObserver,
        };
        deploy_rollback_execute(
            prepared,
            &rctx,
            &RollbackOptions {
                run_id: RunId::parse(run_id).expect("run id"),
                discard_live_changes: false,
                designers: DesignersPolicy::default(),
                hold: short_hold(30),
                poll: production_poll("config scan after rollback"),
            },
        )
        .await
    };
    match recover().await {
        Ok(done) => {
            eprintln!(
                "recovery at once ({:.1}s after the kill): ok",
                killed_at.elapsed().as_secs_f64()
            );
            done
        }
        Err(err) => {
            eprintln!(
                "recovery at once ({:.1}s after the kill) failed: {} / {err}",
                killed_at.elapsed().as_secs_f64(),
                err.code()
            );
            assert_eq!(
                err.code(),
                "scan_lock_held",
                "only the dead lock may refuse"
            );
            // Wait for the dead apply's hold to lapse, then recover.
            let deadline = Instant::now() + Duration::from_secs(90);
            loop {
                match api.scan_lock_info(ScanKind::Config).await {
                    Ok(LockObservation::Free) => break,
                    Ok(LockObservation::Held(_)) | Err(_) => {}
                }
                assert!(Instant::now() < deadline, "the lock never lapsed");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            eprintln!(
                "lock free {:.1}s after the kill",
                killed_at.elapsed().as_secs_f64()
            );
            recover().await.expect("recovery after the hold lapsed")
        }
    }
}

/// A host that hangs after its first move; the apply runs on a task and the
/// task is aborted there, which leaves what a `kill -9` there leaves.
#[tokio::test]
#[ignore = "live: needs the ign-deploy-rig fixture and IGNITION_LIVE_DEPLOY=1"]
async fn live_kill_mid_write_then_recover() {
    let Some(rig) = rig().await else { return };
    let _serial = RIG_LOCK.lock().await;
    let _clean = Clean::new(&rig);
    let started = Instant::now();
    eprintln!("live_kill_mid_write_then_recover @ {}", utc_now());

    let repo = apply_repo(&rig);
    let before = collection_state(&rig);
    let ctx = Arc::new(Context::new(repo.path()));
    let plan_before = ctx.plan_hash().await;

    // The apply, on a task, with a hold of 30 s so the dead apply's lock
    // lapses within the test.
    let notify = Arc::new(tokio::sync::Notify::new());
    let host = LocalFsTransport::new(rig.data_dir.clone(), "nodes.master")
        .with_apply(GATEWAY_DIR, &rig.mode)
        .expect("apply host")
        .with_apply_fault(ApplyFault::HangAfterMove(1, notify.clone()));
    let host = Arc::new(FaultyHost::new(host));
    let prepared = deploy_apply_prepare(
        &ctx.declared,
        ctx.reader.as_ref(),
        &PrepareOptions {
            base: None,
            gates: gates_for(&plan_before),
        },
    )
    .await
    .expect("prepare");
    let task = tokio::spawn({
        let ctx = ctx.clone();
        let host = host.clone();
        let api = rig.api();
        let plan_before = plan_before.clone();
        async move {
            let run_ctx = ApplyContext {
                declared: &ctx.declared,
                repo: &ctx.repo,
                reader: ctx.reader.as_ref(),
                host: host.as_ref(),
                deploy: &api,
                gateway: &api,
                backup_deploy: None,
                clock: &SystemClock,
                observer: &NoObserver,
            };
            deploy_apply_execute(
                prepared,
                &run_ctx,
                &engine_options(short_hold(30), gates_for(&plan_before)),
            )
            .await
        }
    });
    tokio::time::timeout(Duration::from_secs(180), notify.notified())
        .await
        .expect("the commit reached the hang");
    task.abort();
    assert!(task.await.expect_err("aborted").is_cancelled());
    let killed_at = Instant::now();
    eprintln!("killed after the first move @ {}", utc_now());

    // The journal is on the host, at `writing`, naming the run.
    let deploy_dir = rig.deploy_dir().join(GATEWAY_DIR).join(&rig.mode);
    let journal = Journal::parse(
        "journal.json",
        &std::fs::read(deploy_dir.join("journal.json")).expect("the journal"),
        GATEWAY_DIR,
        &rig.mode,
    )
    .expect("the journal parses");
    let run_id = journal.run_id.clone();
    eprintln!("journal: run {run_id} phase {}", journal.phase.as_str());
    let torn = collection_state(&rig);
    assert_ne!(torn, before, "one file moved before the kill");

    // Plan, apply and a rollback of another run all refuse, and write nothing.
    let err = deploy_plan(
        &ctx.declared,
        ctx.reader.as_ref(),
        &PlanOptions {
            base: None,
            diff: false,
        },
    )
    .await
    .expect_err("plan refuses over a journal");
    assert_journal_refusal(&err, &run_id);
    let err = deploy_apply_prepare(
        &ctx.declared,
        ctx.reader.as_ref(),
        &PrepareOptions::default(),
    )
    .await
    .expect_err("apply refuses over a journal");
    assert_journal_refusal(&err, &run_id);
    let err = deploy_rollback_prepare_for_env(
        &ctx.repo,
        ENV,
        "20261016T000000Z-aaaaaa",
        ctx.reader.as_ref(),
        &RollbackPrepareOptions::default(),
    )
    .await
    .expect_err("a rollback of another run refuses");
    assert_journal_refusal(&err, &run_id);
    assert_eq!(collection_state(&rig), torn, "the refusals wrote nothing");
    assert!(deploy_dir.join("journal.json").is_file());

    // Recovery is the rollback of the journal's own run, from a fresh
    // context, with no flag.
    let result = recover_after_kill(&rig, repo.path(), &run_id, killed_at).await;
    assert_eq!(result.outcome, RollbackKind::Restored);
    assert!(result.ok);
    assert_eq!(result.target_run_id, run_id);

    assert_eq!(collection_state(&rig), before, "the pre-apply tree is back");
    assert!(
        !deploy_dir.join("journal.json").exists(),
        "the journal is removed"
    );
    assert_eq!(
        translators_in_mode(&rig, &rig.api()).await,
        Vec::<String>::new(),
        "the gateway holds none of the units"
    );
    assert_eq!(
        Context::new(repo.path()).plan_hash().await,
        plan_before,
        "the plan is back to the start"
    );
    // D-11: the recovery run records the torn state it found.
    let events = std::fs::read_to_string(PathBuf::from(&result.run_dir).join("events.ndjson"))
        .expect("events");
    assert!(
        events.lines().any(|l| l.contains("\"interrupted_run\"")),
        "an interrupted_run event"
    );
    eprintln!(
        "PASS live_kill_mid_write_then_recover in {:.1}s",
        started.elapsed().as_secs_f64()
    );
}

fn assert_journal_refusal(err: &CoreError, run_id: &str) {
    let CoreError::DeployJournalPresent { run_id: named, .. } = err else {
        panic!("expected deploy_journal_present, got {err:?}");
    };
    assert_eq!(named, run_id);
    assert_eq!(err.exit_code(), 6);
    assert!(
        err.to_string()
            .contains(&format!("ign deploy rollback --run {run_id} --env {ENV}")),
        "the recovery command in {err}"
    );
}

// ---- live_lock_lapse_restores ---------------------------------------------------------

/// The hold lapses between the commit and the scan: the engine finds the lock
/// gone, restores the host and scans once, so the gateway never loads the
/// partial tree (C2e: a lapsed lock does not trigger a scan on its own).
#[tokio::test]
#[ignore = "live: needs the ign-deploy-rig fixture and IGNITION_LIVE_DEPLOY=1"]
async fn live_lock_lapse_restores() {
    let Some(rig) = rig().await else { return };
    let _serial = RIG_LOCK.lock().await;
    let _clean = Clean::new(&rig);
    let started = Instant::now();
    eprintln!("live_lock_lapse_restores @ {}", utc_now());

    let repo = apply_repo(&rig);
    let before = collection_state(&rig);
    let ctx = Context::new(repo.path());
    let plan_before = ctx.plan_hash().await;
    let api = Arc::new(rig.api());

    // After the commit returns, sleep past the hold, and look at the gateway
    // from outside while the unscanned files sit in the collection.
    let seen: Arc<std::sync::Mutex<Vec<String>>> = Arc::default();
    let lapse_scan_ts = Arc::new(std::sync::atomic::AtomicI64::new(0));
    let hold_s = HOLD_FLOOR_S;
    let host = LocalFsTransport::new(rig.data_dir.clone(), "nodes.master")
        .with_apply(GATEWAY_DIR, &rig.mode)
        .expect("apply host");
    let host = FaultyHost::new(host).after("commit", 1, {
        let api = api.clone();
        let seen = seen.clone();
        let lapse_scan_ts = lapse_scan_ts.clone();
        let mode = rig.mode.clone();
        move || {
            let api = api.clone();
            let seen = seen.clone();
            let lapse_scan_ts = lapse_scan_ts.clone();
            let mode = mode.clone();
            async move {
                let scan_at_commit = api.scan_status(ScanKind::Config).await.expect("scan status");
                tokio::time::sleep(Duration::from_secs(hold_s + 3)).await;
                let scan_after_sleep =
                    api.scan_status(ScanKind::Config).await.expect("scan status");
                let lock = api.scan_lock_info(ScanKind::Config).await.expect("lock info");
                lapse_scan_ts.store(
                    scan_after_sleep.last_scan_timestamp,
                    std::sync::atomic::Ordering::SeqCst,
                );
                let visible: Vec<String> =
                    resource_list_in_collection(api.as_ref(), TRANSLATOR.0, TRANSLATOR.1, &mode)
                        .await
                        .expect("REST list")
                        .into_iter()
                        .filter_map(|r| r.name)
                        .collect();
                let mut seen = seen.lock().expect("seen");
                seen.push(format!(
                    "scan timestamp across the lapse: {} -> {} (active {}); lock {}; REST translators in the mode: {:?}",
                    scan_at_commit.last_scan_timestamp,
                    scan_after_sleep.last_scan_timestamp,
                    scan_after_sleep.scan_active,
                    match lock {
                        LockObservation::Free => "free".to_string(),
                        LockObservation::Held(_) => "held".to_string(),
                    },
                    visible,
                ));
                seen.push(format!(
                    "scan unchanged across the lapse: {}",
                    scan_at_commit.last_scan_timestamp == scan_after_sleep.last_scan_timestamp
                ));
                seen.push(format!("translators_visible: {}", visible.len()));
            }
        }
    });

    let prepared = deploy_apply_prepare(
        &ctx.declared,
        ctx.reader.as_ref(),
        &PrepareOptions {
            base: None,
            gates: gates_for(&plan_before),
        },
    )
    .await
    .expect("prepare");
    let run_ctx = ApplyContext {
        declared: &ctx.declared,
        repo: &ctx.repo,
        reader: ctx.reader.as_ref(),
        host: &host,
        deploy: api.as_ref(),
        gateway: api.as_ref(),
        backup_deploy: None,
        clock: &SystemClock,
        observer: &NoObserver,
    };
    let result = deploy_apply_execute(
        prepared,
        &run_ctx,
        &engine_options(short_hold(hold_s), gates_for(&plan_before)),
    )
    .await
    .expect("the apply returned a verdict");
    for line in seen.lock().expect("seen").iter() {
        eprintln!("C2e in the engine: {line}");
    }
    eprintln!(
        "apply: outcome {:?} cause {:?} ok {} slug {:?}",
        result.outcome, result.cause, result.ok, result.slug
    );
    assert_eq!(result.outcome, OutcomeKind::RolledBack, "{result:?}");
    assert_eq!(result.cause, Some(RollbackCause::LockExpired));
    assert!(!result.ok);
    assert_eq!(result.failure_exit_code(), Some(6));

    // The host is back, the gateway was scanned after the restore, and it
    // holds none of the partial tree.
    assert_eq!(collection_state(&rig), before, "the pre-apply tree is back");
    let status = api
        .scan_status(ScanKind::Config)
        .await
        .expect("scan status");
    assert!(!status.scan_active);
    assert!(
        status.last_scan_timestamp > lapse_scan_ts.load(std::sync::atomic::Ordering::SeqCst),
        "the restore scanned after the lapse: {} -> {}",
        lapse_scan_ts.load(std::sync::atomic::Ordering::SeqCst),
        status.last_scan_timestamp
    );
    assert_eq!(
        translators_in_mode(&rig, api.as_ref()).await,
        Vec::<String>::new(),
        "the REST view holds none of the units after the restore scan"
    );
    let since = status.last_scan_timestamp - status.last_scan_duration.unwrap_or(30_000) - 1_000;
    tokio::time::sleep(Duration::from_secs(6)).await;
    let bad = will_not_be_loaded_since(api.as_ref(), since).await;
    assert!(bad.is_empty(), "lines since the restore scan: {bad:?}");
    assert_eq!(
        ctx.plan_hash().await,
        plan_before,
        "the plan is back to the start"
    );
    {
        let seen = seen.lock().expect("seen");
        assert!(
            seen.iter()
                .any(|l| l == "scan unchanged across the lapse: true"),
            "C2e: the lapse alone did not scan: {seen:?}"
        );
        assert!(
            seen.iter().any(|l| l == "translators_visible: 0"),
            "C2e: the unscanned file was not loaded: {seen:?}"
        );
    }
    eprintln!(
        "PASS live_lock_lapse_restores in {:.1}s",
        started.elapsed().as_secs_f64()
    );
}

// ---- live_kill_after_green_verify_then_recover (A9) ------------------------------------

/// The window the README calls A9: the scan loaded the tree, the verify came
/// back green, and the runner dies before `finalize`. The journal is still on
/// the host, the gateway holds a good deploy, and the recovery restores it
/// (D-09: the journal decides, not the verdict).
#[tokio::test]
#[ignore = "live: needs the ign-deploy-rig fixture and IGNITION_LIVE_DEPLOY=1"]
async fn live_kill_after_green_verify_then_recover() {
    let Some(rig) = rig().await else { return };
    let _serial = RIG_LOCK.lock().await;
    let _clean = Clean::new(&rig);
    let started = Instant::now();
    eprintln!("live_kill_after_green_verify_then_recover @ {}", utc_now());

    let repo = apply_repo(&rig);
    let before = collection_state(&rig);
    let ctx = Arc::new(Context::new(repo.path()));
    let plan_before = ctx.plan_hash().await;

    let notify = Arc::new(tokio::sync::Notify::new());
    let host = LocalFsTransport::new(rig.data_dir.clone(), "nodes.master")
        .with_apply(GATEWAY_DIR, &rig.mode)
        .expect("apply host");
    let host = Arc::new(FaultyHost::new(host).hang("finalize", 1, notify.clone()));
    let prepared = deploy_apply_prepare(
        &ctx.declared,
        ctx.reader.as_ref(),
        &PrepareOptions {
            base: None,
            gates: gates_for(&plan_before),
        },
    )
    .await
    .expect("prepare");
    let task = tokio::spawn({
        let ctx = ctx.clone();
        let host = host.clone();
        let api = rig.api();
        let plan_before = plan_before.clone();
        async move {
            let run_ctx = ApplyContext {
                declared: &ctx.declared,
                repo: &ctx.repo,
                reader: ctx.reader.as_ref(),
                host: host.as_ref(),
                deploy: &api,
                gateway: &api,
                backup_deploy: None,
                clock: &SystemClock,
                observer: &NoObserver,
            };
            deploy_apply_execute(
                prepared,
                &run_ctx,
                &engine_options(short_hold(30), gates_for(&plan_before)),
            )
            .await
        }
    });
    tokio::time::timeout(Duration::from_secs(180), notify.notified())
        .await
        .expect("the run reached finalize");
    task.abort();
    assert!(task.await.expect_err("aborted").is_cancelled());
    let killed_at = Instant::now();
    eprintln!(
        "killed past the green verify, before finalize @ {}",
        utc_now()
    );

    // A good deploy is live: the files are on the host, the scan loaded them.
    let deploy_dir = rig.deploy_dir().join(GATEWAY_DIR).join(&rig.mode);
    let journal = Journal::parse(
        "journal.json",
        &std::fs::read(deploy_dir.join("journal.json")).expect("the journal"),
        GATEWAY_DIR,
        &rig.mode,
    )
    .expect("the journal parses");
    let run_id = journal.run_id.clone();
    eprintln!("journal: run {run_id} phase {}", journal.phase.as_str());
    assert_ne!(collection_state(&rig), before, "the tree is applied");
    let live = translators_in_mode(&rig, &rig.api()).await;
    eprintln!("REST translators in the mode while the journal stands: {live:?}");
    assert!(
        live.iter().any(|n| n == "POSTGRES"),
        "the gateway loaded the applied tree: {live:?}"
    );
    let err = deploy_apply_prepare(
        &ctx.declared,
        ctx.reader.as_ref(),
        &PrepareOptions::default(),
    )
    .await
    .expect_err("a new apply refuses over the journal");
    assert_journal_refusal(&err, &run_id);

    let result = recover_after_kill(&rig, repo.path(), &run_id, killed_at).await;
    assert_eq!(result.outcome, RollbackKind::Restored);
    assert_eq!(result.target_run_id, run_id);
    assert_eq!(collection_state(&rig), before, "the pre-apply tree is back");
    assert!(!deploy_dir.join("journal.json").exists());
    assert_eq!(
        translators_in_mode(&rig, &rig.api()).await,
        Vec::<String>::new(),
        "the good deploy is gone from the gateway after the restore scan"
    );
    assert_eq!(
        Context::new(repo.path()).plan_hash().await,
        plan_before,
        "the plan is back to the start"
    );
    eprintln!(
        "PASS live_kill_after_green_verify_then_recover in {:.1}s",
        started.elapsed().as_secs_f64()
    );
}
