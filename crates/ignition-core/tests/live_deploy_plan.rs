//! Phase 22's rig gates against a real Ignition 8.3.6 gateway whose
//! `config/resources` is bind-mounted: "pull, render, plan is empty" and a
//! check that `plan` takes no scan-lock (22-09), and `deploy verify` green
//! then red (22-10).
//!
//! `deploy_pull` reads the live master through `LocalFsTransport` (the same
//! `TreeReader` `plan` uses) into a temp repo, `load_declared` renders that
//! repo, and `deploy_plan` compares the render with the live files. Success
//! criterion 3 holds when `has_changes` is false. Nothing here uses the write
//! half of the transport and nothing writes to the gateway.
//!
//! ```text
//! IGNITION_LIVE_DEPLOY=1 IGNITION_LIVE_URL=http://localhost:18290 \
//!   IGNITION_LIVE_TOKEN="$IGN_TOKEN_CAP_A" \
//!   IGN_DEPLOY_RIG_DATA="$HOME/.cache/ign-deploy-rig/data" IGN_DEPLOY_RIG_MODE=production \
//!   cargo test -p ignition-core --test live_deploy_plan -- --ignored --nocapture
//! ```
//!
//! | var | meaning |
//! |---|---|
//! | `IGNITION_LIVE_DEPLOY` | must equal `1`; anything else is a quiet green skip |
//! | `IGNITION_LIVE_URL` | the rig's base URL; loopback hosts only (`localhost`, `127.0.0.1`, `::1`) |
//! | `IGNITION_LIVE_TOKEN` | cap-a's full `name:key` API token (used for the scan-lock reads and the name guard only) |
//! | `IGN_DEPLOY_RIG_DATA` | the host data directory whose `config/resources` is bind-mounted into the rig |
//! | `IGN_DEPLOY_RIG_MODE` | the rig's active mode (`production` on the rig recipe) |
//!
//! Rig recipe: `capture22.sh up22`, `capture22.sh setup22`, then
//! `source $IGN_DEPLOY_RIG_STATE/tokens.env`; tear down with
//! `docker compose -p ign-deploy-rig down -v`.
//!
//! `verify_green_then_red_live` writes a rendered tree into the mode collection
//! by plain file writes, triggers a config scan as test code (no verb does
//! that in this phase), and runs `deploy_verify` through the rig session and a
//! `LocalFsTransport`. It then adds a declared `database-connection` whose
//! live copy has no `uuid` (the gateway logs `will not be loaded`, CV1),
//! scans, and expects a red verdict with exit 6. It removes what it wrote.
//!
//! Safety: the URL guard refuses a non-loopback host before a request is made,
//! and a name guard refuses any gateway whose `gateway-info` name is not
//! `ign-deploy-rig`. Output carries counts and unit keys only, never a token
//! or a file's bytes.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ignition_core::actions::deploy::{
    PlanOptions, VerifyOptions, deploy_plan, deploy_pull, deploy_verify, load_declared, load_target,
};
use ignition_core::client::scan::{LockObservation, ScanKind, scan_complete};
use ignition_core::client::{DeployApi, GatewayApi, ReqwestGatewayApi};
use ignition_core::config::{Credential, Secret};
use ignition_core::deploy::plan::PlanStatus;
use ignition_core::deploy::transport::{self, TreeReader};
use ignition_core::deploy::verify::{CheckStatus, VERIFY_FAILED_SLUG};

/// The only gateway this gate may talk to.
const RIG_GATEWAY_NAME: &str = "ign-deploy-rig";

const GATEWAY_DIR: &str = "rig-gw";
const ENV: &str = "rig";

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
        .unwrap_or_else(|| panic!("{name} must be set to run the live deploy gate"))
}

#[test]
fn local_url_guard_refuses_non_loopback() {
    for raw in [
        "http://10.4.8.15:8088",
        "https://wignvm01.example.local:8043",
        "http://localhost.evil.example:18290",
        "http://127.0.0.2:18290",
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
    api: ReqwestGatewayApi,
    token_name: String,
    data_dir: PathBuf,
    mode: String,
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
    let token_name = token.split(':').next().unwrap_or_default().to_string();
    let api = ReqwestGatewayApi::for_tests(&url, Some(Credential::Token(Secret::new(token))));
    let info = GatewayApi::gateway_info(&api)
        .await
        .expect("gateway-info read for the name guard");
    assert_eq!(
        info.name.as_deref(),
        Some(RIG_GATEWAY_NAME),
        "refusing: this gateway is not the {RIG_GATEWAY_NAME} fixture"
    );
    Some(Rig {
        api,
        token_name,
        data_dir,
        mode,
    })
}

/// A temp repo with only `targets/rig.yaml`: the master reads the rig's bind
/// directory through the local transport. `gateways/` is created empty so the
/// pull has a place to write.
fn empty_repo(rig: &Rig) -> tempfile::TempDir {
    let repo = tempfile::tempdir().expect("temp repo");
    std::fs::create_dir_all(repo.path().join("targets")).expect("targets dir");
    std::fs::create_dir_all(repo.path().join("gateways")).expect("gateways dir");
    let yaml = format!(
        "schema_version: 1\ngateway: {GATEWAY_DIR}\nmode: {mode}\nnodes:\n  master:\n    profile: rig\n    local:\n      data_dir: {data}\n",
        mode = rig.mode,
        data = rig.data_dir.display(),
    );
    std::fs::write(repo.path().join("targets").join("rig.yaml"), yaml).expect("target file");
    repo
}

fn reader(repo: &Path) -> Box<dyn TreeReader> {
    let (target_file, target) = load_target(repo, ENV).expect("target");
    transport::for_node(repo, &target_file, "nodes.master", &target.nodes.master)
        .expect("the master has host access")
}

#[tokio::test]
#[ignore = "live: needs the ign-deploy-rig fixture and IGNITION_LIVE_DEPLOY=1"]
async fn pull_render_plan_is_empty_live() {
    let Some(rig) = rig().await else { return };
    let repo = empty_repo(&rig);
    let reader = reader(repo.path());

    let pulled = deploy_pull(repo.path(), ENV, reader.as_ref(), false)
        .await
        .expect("deploy pull from the live data dir");
    eprintln!(
        "pull: {} files written, manifest {}, {} skipped",
        pulled.files_written,
        pulled.manifest,
        pulled.skipped.len()
    );
    for s in &pulled.skipped {
        eprintln!("  skipped {} ({})", s.path, s.reason);
    }
    assert_eq!(pulled.manifest, "generated");
    assert!(
        pulled.files_written > 0,
        "a commissioned rig holds resources; pull wrote none"
    );

    let declared = load_declared(repo.path(), ENV).expect("render of the pulled repo");
    let result = deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect("plan over the same transport");
    eprintln!(
        "plan: added {} changed {} removed {} same {} unmanaged {} has_changes {}",
        result.summary.added,
        result.summary.changed,
        result.summary.removed,
        result.summary.same,
        result.summary.unmanaged,
        result.summary.has_changes
    );

    for entry in &result.entries {
        match entry.status {
            PlanStatus::Same => {}
            PlanStatus::Unmanaged => assert!(
                pulled.skipped.iter().any(|s| s.path == entry.key),
                "unmanaged unit {} was not reported as skipped by pull",
                entry.key
            ),
            other => panic!("unit {} is {other:?}, expected same", entry.key),
        }
    }
    assert!(
        !result.summary.has_changes,
        "pull, render, plan must report no changes"
    );
    assert!(result.summary.same > 0, "no unit was compared");
    eprintln!("PASS pull_render_plan_is_empty_live");
}

#[tokio::test]
#[ignore = "live: needs the ign-deploy-rig fixture and IGNITION_LIVE_DEPLOY=1"]
async fn plan_takes_no_lock_live() {
    let Some(rig) = rig().await else { return };
    let repo = empty_repo(&rig);
    let reader = reader(repo.path());
    deploy_pull(repo.path(), ENV, reader.as_ref(), false)
        .await
        .expect("deploy pull from the live data dir");
    let declared = load_declared(repo.path(), ENV).expect("render of the pulled repo");

    let before = rig
        .api
        .scan_lock_info(ScanKind::Config)
        .await
        .expect("lock info before plan");
    deploy_plan(&declared, reader.as_ref(), &PlanOptions::default())
        .await
        .expect("plan");
    let after = rig
        .api
        .scan_lock_info(ScanKind::Config)
        .await
        .expect("lock info after plan");

    assert_eq!(
        std::mem::discriminant(&before),
        std::mem::discriminant(&after),
        "the scan-lock changed state across plan: before {before:?}, after {after:?}"
    );
    if let LockObservation::Held(held) = &after {
        let actor = held.actor.clone().unwrap_or_default();
        assert!(
            !actor.contains(&format!(":{}:", rig.token_name)),
            "plan left a hold attributed to the test token"
        );
    }
    eprintln!(
        "PASS plan_takes_no_lock_live: lock {} before and after plan",
        match after {
            LockObservation::Free => "free",
            LockObservation::Held(_) => "held (not by the test token)",
        }
    );
}

// ---- verify (22-10) ----------------------------------------------------------------

const VERIFY_GATEWAY_DIR: &str = "rig-gw";
const NOUUID_DB: &str = "ignition/database-connection/VERIFY_NOUUID";
const DB_CONFIG: &str = include_str!("fixtures/live-rig-deploy/db-connection-config.json");
const TRACER_GATEWAY: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/deploy/repos/tracer/gateways/tracer-gw"
);

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

/// A repo whose gateway directory is the tracer's two resources, declared for
/// the rig's mode. `with_db` adds the `database-connection` the red run needs.
fn verify_repo(rig: &Rig, with_db: bool) -> tempfile::TempDir {
    let repo = empty_repo(rig);
    let gw = repo.path().join("gateways").join(GATEWAY_DIR);
    copy_dir(
        &Path::new(TRACER_GATEWAY).join("resources"),
        &gw.join("resources"),
    );
    let mut manifest =
        "schema_version: 1\ntypes:\n  - ignition/database-translator\n  - ignition/general-alarm-settings\n"
            .to_string();
    if with_db {
        manifest.push_str("  - ignition/database-connection\n");
        let dir = gw.join("resources").join(NOUUID_DB);
        std::fs::create_dir_all(&dir).expect("db dir");
        std::fs::write(dir.join("config.json"), DB_CONFIG).expect("db config");
    }
    std::fs::write(gw.join("manifest.yaml"), manifest).expect("manifest");
    repo
}

/// Removes what a verify run wrote into the rig's mode collection, even when
/// an assertion fails.
struct Written(Vec<PathBuf>);

impl Drop for Written {
    fn drop(&mut self) {
        for dir in &self.0 {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

/// Write the rendered files of `declared` into the rig's mode collection by
/// plain file writes, the way a deploy would put them there.
fn place_render(rig: &Rig, declared: &ignition_core::actions::deploy::Declared, strip_uuid: &str) {
    let root = rig
        .data_dir
        .join("config")
        .join("resources")
        .join(&rig.mode);
    for (path, bytes) in &declared.outcome.tree.files {
        let dest = path.components().fold(root.clone(), |acc, c| acc.join(c));
        std::fs::create_dir_all(dest.parent().expect("parent")).expect("create dirs");
        let is_marker = path.as_str() == format!("{strip_uuid}/resource.json");
        if is_marker {
            // The live copy has no `attributes.uuid`: the gateway refuses to
            // load a named resource of this type without one (CV1).
            let mut json: serde_json::Value = serde_json::from_slice(bytes).expect("resource.json");
            json["attributes"]
                .as_object_mut()
                .expect("attributes")
                .remove("uuid");
            std::fs::write(&dest, serde_json::to_vec_pretty(&json).expect("json")).expect("write");
        } else {
            std::fs::write(&dest, bytes).expect("write rendered file");
        }
    }
}

/// Trigger a config scan and wait for it to complete (test code, not a verb).
async fn scan_and_wait(api: &ReqwestGatewayApi) {
    let before = api
        .scan_status(ScanKind::Config)
        .await
        .expect("scan status");
    api.scan_request(ScanKind::Config)
        .await
        .expect("scan request");
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let now = api
            .scan_status(ScanKind::Config)
            .await
            .expect("scan status");
        if scan_complete(&before, &now) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the scan did not complete in 120 s"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

fn print_verdict(label: &str, result: &ignition_core::actions::deploy::DeployVerifyResult) {
    eprintln!(
        "{label}: ok {} slug {:?} exit {:?} resources_checked {}",
        result.ok,
        result.slug,
        result.failure_exit_code(),
        result.resources_checked
    );
    for check in &result.checks {
        let codes: Vec<String> = check
            .findings
            .iter()
            .map(|f| format!("{}:{} ({})", f.code, f.subject, f.detail))
            .collect();
        eprintln!("  {:?} {:?} [{}]", check.id, check.status, codes.join(", "));
    }
    for warning in &result.warnings {
        eprintln!("  warning: {warning}");
    }
}

#[tokio::test]
#[ignore = "live: needs the ign-deploy-rig fixture and IGNITION_LIVE_DEPLOY=1"]
async fn verify_green_then_red_live() {
    let Some(rig) = rig().await else { return };
    let mode_root = rig
        .data_dir
        .join("config")
        .join("resources")
        .join(&rig.mode);
    let _cleanup = Written(vec![
        mode_root.join("ignition/database-translator/POSTGRES"),
        mode_root.join("ignition/general-alarm-settings"),
        mode_root.join(NOUUID_DB),
    ]);
    let opts = VerifyOptions::default();

    // Green: the tracer render placed in the mode collection and scanned.
    let repo = verify_repo(&rig, false);
    let reader = reader(repo.path());
    let declared = load_declared(repo.path(), ENV).expect("render");
    assert_eq!(declared.target.gateway, VERIFY_GATEWAY_DIR);
    place_render(&rig, &declared, "");
    scan_and_wait(&rig.api).await;
    let green = deploy_verify(
        &declared,
        "ign-cli",
        &rig.api,
        &rig.api,
        reader.as_ref(),
        &opts,
    )
    .await
    .expect("verdict on the rig");
    print_verdict("green", &green);
    assert!(green.ok, "a scanned tree must verify green: {green:#?}");
    assert_eq!(green.slug, None);
    assert_eq!(green.failure_exit_code(), None);
    assert_eq!(green.resources_checked, 2);
    assert!(green.checks.iter().all(|c| c.status != CheckStatus::Fail));

    // Red: a declared database-connection whose live copy has no uuid.
    let repo = verify_repo(&rig, true);
    let reader = self::reader(repo.path());
    let declared = load_declared(repo.path(), ENV).expect("render with the connection");
    assert!(
        declared.outcome.tree.files.contains_key(
            &ignition_core::deploy::layout::RelPath::new(&format!("{NOUUID_DB}/resource.json"))
                .unwrap()
        ),
        "the render must carry the connection's resource.json"
    );
    place_render(&rig, &declared, NOUUID_DB);
    scan_and_wait(&rig.api).await;
    let red = deploy_verify(
        &declared,
        "ign-cli",
        &rig.api,
        &rig.api,
        reader.as_ref(),
        &opts,
    )
    .await
    .expect("verdict on the rig");
    print_verdict("red", &red);
    assert!(!red.ok, "a resource the gateway will not load must be red");
    assert_eq!(red.slug, Some(VERIFY_FAILED_SLUG));
    assert_eq!(red.failure_exit_code(), Some(6));
    let codes: Vec<&str> = red
        .checks
        .iter()
        .flat_map(|c| c.findings.iter())
        .filter(|f| f.subject == NOUUID_DB)
        .map(|f| f.code)
        .collect();
    assert!(
        codes.contains(&"unloaded_resource") || codes.contains(&"resource_missing"),
        "expected unloaded_resource or resource_missing for {NOUUID_DB}, got {codes:?}"
    );
    eprintln!("PASS verify_green_then_red_live");
}
