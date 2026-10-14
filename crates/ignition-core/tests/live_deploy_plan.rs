//! Phase 22's rig gate (22-09): "pull, render, plan is empty" against a real
//! Ignition 8.3.6 gateway whose `config/resources` is bind-mounted, and a
//! check that `plan` takes no scan-lock.
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
//! Safety: the URL guard refuses a non-loopback host before a request is made,
//! and a name guard refuses any gateway whose `gateway-info` name is not
//! `ign-deploy-rig`. Output carries counts and unit keys only, never a token
//! or a file's bytes.

use std::path::{Path, PathBuf};

use ignition_core::actions::deploy::{
    PlanOptions, deploy_plan, deploy_pull, load_declared, load_target,
};
use ignition_core::client::scan::{LockObservation, ScanKind};
use ignition_core::client::{DeployApi, GatewayApi, ReqwestGatewayApi};
use ignition_core::config::{Credential, Secret};
use ignition_core::deploy::plan::PlanStatus;
use ignition_core::deploy::transport::{self, TreeReader};

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
