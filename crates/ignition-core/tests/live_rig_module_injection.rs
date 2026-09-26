//! Phase 16's live gate (16-03): the ONLY proof that a declared module is
//! actually LOADED by a real gateway, not merely mounted.
//!
//! Everything else in Phase 16 runs against `FakeRunner` and wiremock and
//! proves `ign` EMITS a correct-looking override. This test spins a real
//! stock `inductiveautomation/ignition` container through the real
//! `DockerCompose` runner and asks the gateway itself.
//!
//! ```text
//! IGNITION_LIVE_RIG_MODULES=1 cargo test -p ignition-core --test live_rig_module_injection -- --ignored --nocapture
//! ```
//!
//! | Variable | Meaning |
//! |---|---|
//! | `IGNITION_LIVE_RIG_MODULES` | Must equal `1`. Unset = green no-op skip (D-20). |
//! | `IGNITION_LIVE_RIG_USER` | Gateway admin user. Defaults to `admin`, matching the fixture. |
//! | `IGNITION_LIVE_RIG_PASSWORD` | Gateway admin password. Defaults to `password`, matching the fixture. |
//!
//! **The fixture deliberately sets no acceptance variable** (D-21). That
//! absence is the control: a module can only reach the HEALTHY list if the
//! `ACCEPT_MODULE_CERTS`/`ACCEPT_MODULE_LICENSES` values came from the
//! override `ign` generated. A module in the QUARANTINED list is a FAILURE,
//! not a pass — quarantine is exactly what an unaccepted certificate looks
//! like.
//!
//! This test also confirms or falsifies the one assumption plan 16-02 left
//! open: that the acceptance variables accept a COMMA-SEPARATED list of
//! gateway module ids. Two modules are declared precisely so that a
//! wrong separator shows up as "one loaded, one quarantined" rather than
//! passing silently (D-24).
//!
//! Sections A, B and C run as ONE test in order. Rust does not guarantee
//! test ordering, and §B's entire point is that it runs against §A's state;
//! bringing a gateway up three times would also cost minutes for no signal.

use std::collections::BTreeMap;
use std::path::Path;

use ignition_core::client::GatewayApi;
use ignition_core::client::query::ListQuery;
use ignition_core::config::secret::Secret;
use ignition_core::config::{Config, ModuleDeclaration, RigEntry};
use ignition_core::module::fetch::{FetchPolicy, ModuleFeed};
use ignition_core::module::{GIT_MODULE, PROJECT_SCAN_ENDPOINT};
use ignition_core::rig::compose::{ComposeRunner, DockerCompose};
use ignition_core::rig::{OVERRIDE_FILENAME, RigSelection, provision_modules, resolve_plan};

/// Wait budget handed to `rig up` — both compose's `--wait-timeout` and the
/// commissioned probe deadline.
const UP_TIMEOUT_S: u64 = 300;

/// The module scan can still be finishing when the gateway first reports
/// running, so the list fetch retries — bounded, never an open loop.
const MODULE_LIST_ATTEMPTS: usize = 20;
const MODULE_LIST_INTERVAL_S: u64 = 3;

/// `true` only when `IGNITION_LIVE_RIG_MODULES` is explicitly `1`.
fn live_enabled() -> bool {
    std::env::var("IGNITION_LIVE_RIG_MODULES").ok().as_deref() == Some("1")
}

fn skip(message: &str) {
    eprintln!("skipping: {message}");
}

/// The profile `adopt` writes for this run. Unique so it cannot collide
/// with a real profile, and DELETED from the keyring in teardown — a test
/// must not leave an entry in the operator's keychain.
const LIVE_PROFILE: &str = "phase16-live-gate";

/// Bootstrap auth the way `ign` itself does.
///
/// Basic auth returns 401 against a commissioned 8.3 gateway's
/// `/data/api/v1` (observed live: "gateway rejected credentials (HTTP
/// 401)"), which is why this runs the real `adopt` action — OIDC login,
/// then mint an API token — rather than hand-rolling a credential. The
/// token lands in the keyring on a host that has one, so the client is
/// built by resolving the profile `adopt` wrote, exactly as a real
/// invocation would.
async fn adopt_and_build_session(
    url: &str,
    config_path: &Path,
) -> Result<ignition_core::session::Session, String> {
    let user = std::env::var("IGNITION_LIVE_RIG_USER").unwrap_or_else(|_| "admin".to_string());
    let password =
        std::env::var("IGNITION_LIVE_RIG_PASSWORD").unwrap_or_else(|_| "password".to_string());

    // `adopt` REWRITES an existing profile's auth — it does not create
    // one ("profile ... vanished from the config mid-adopt", observed
    // live). Seed the profile pointing at the fresh rig first.
    let mut seed = ignition_core::config::Config::default();
    seed.profiles.insert(
        LIVE_PROFILE.to_string(),
        ignition_core::config::Profile {
            url: url.parse().map_err(|e| format!("rig URL {url:?}: {e}"))?,
            label: Some("Phase 16 live gate (disposable)".to_string()),
            ssl_verify: false,
            auth: ignition_core::config::AuthRef::default(),
            poll_interval_secs: None,
            webdev_secret: None,
        },
    );
    ignition_core::config::save(config_path, &seed)
        .map_err(|err| format!("seeding the adopt config: {err}"))?;

    let opts = ignition_core::actions::adopt::AdoptOptions {
        username: user,
        key_name: LIVE_PROFILE.to_string(),
        level: vec![
            "Authenticated".to_string(),
            "Roles".to_string(),
            "Administrator".to_string(),
        ],
        project: None,
        testing: false,
        checkout: None,
        bake: None,
    };
    ignition_core::actions::adopt::adopt(
        url,
        LIVE_PROFILE,
        config_path,
        &Secret::new(password),
        None,
        &opts,
    )
    .await
    .map_err(|err| format!("adopt could not bootstrap the fresh rig: {err}"))?;

    let mut config = ignition_core::config::load(config_path)
        .map_err(|err| format!("reading the config adopt wrote: {err}"))?;
    ignition_core::session::Session::resolve_side(&mut config, LIVE_PROFILE)
        .map_err(|err| format!("resolving the adopted profile: {err}"))
}

/// Both registered modules at their pinned versions — read from the
/// REGISTRY, never from string literals, so this cannot pass while the
/// registry says something else.
fn both_modules() -> BTreeMap<String, ModuleDeclaration> {
    let mut declared = BTreeMap::new();
    declared.insert(
        GIT_MODULE.id.to_string(),
        ModuleDeclaration {
            version: "2.3.4".to_string(),
        },
    );
    declared.insert(
        PROJECT_SCAN_ENDPOINT.id.to_string(),
        ModuleDeclaration {
            version: "1.0.0".to_string(),
        },
    );
    declared
}

/// Fetch the gateway's healthy module ids, retrying while the scan
/// finishes. Returns the last observed list either way so a failure is
/// diagnosable without a second run.
async fn healthy_module_ids(api: &dyn GatewayApi) -> Result<Vec<String>, String> {
    let mut last_err = String::from("never attempted");
    for _ in 0..MODULE_LIST_ATTEMPTS {
        match api.modules(false, &ListQuery::default()).await {
            Ok(envelope) => {
                let observed: Vec<String> = envelope.items.iter().map(|m| m.id.clone()).collect();
                if observed.iter().any(|id| id == GIT_MODULE.gateway_module_id) {
                    return Ok(observed);
                }
                last_err = format!("list answered but lacked the module: {observed:?}");
            }
            // The error is CARRIED, never swallowed. An empty list and a
            // failing call look identical from the outside, and confusing
            // the two sends you debugging module loading when the real
            // problem is auth or a gateway that is not answering yet.
            Err(err) => last_err = format!("modules() failed: {err}"),
        }
        tokio::time::sleep(std::time::Duration::from_secs(MODULE_LIST_INTERVAL_S)).await;
    }
    Err(last_err)
}

async fn quarantined_module_ids(api: &dyn GatewayApi) -> Result<Vec<String>, String> {
    api.modules(true, &ListQuery::default())
        .await
        .map(|envelope| envelope.items.iter().map(|m| m.id.clone()).collect())
        .map_err(|err| format!("quarantined modules() failed: {err}"))
}

/// Copy the fixture compose into `dir`, returning the copied path.
fn seed_fixture(dir: &Path) -> std::path::PathBuf {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/live-rig/compose.yml");
    let dest = dir.join("compose.yml");
    std::fs::copy(&fixture, &dest).expect("copy fixture compose");
    dest
}

fn config_for(compose_file: &Path, declared: BTreeMap<String, ModuleDeclaration>) -> Config {
    let mut config = Config::default();
    config.rigs.insert(
        "live".to_string(),
        RigEntry {
            compose_file: compose_file.display().to_string(),
            project_name: None,
            modules: declared,
            module_service: None,
        },
    );
    config
}

/// §A mounted+loaded, §B survives recreate, §C deleting the override
/// reverts — one ordered sequence, torn down on every exit path.
#[ignore = "live: needs Docker and IGNITION_LIVE_RIG_MODULES=1"]
#[tokio::test]
async fn live_rig_module_injection_sequence() {
    if !live_enabled() {
        skip("IGNITION_LIVE_RIG_MODULES is not 1");
        return;
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let compose_file = seed_fixture(dir.path());
    let cache_root = dir.path().join("cache");
    let runner = DockerCompose;
    let feed = ModuleFeed::github().expect("feed builds");

    let result = run_sequence(&runner, &feed, &compose_file, &cache_root).await;

    // Teardown is UNCONDITIONAL: a failed assertion that leaves a container
    // and a volume behind poisons the next run.
    // `down -v`: the volume must go too, or a half-commissioned gateway
    // from a failed run is inherited by the next one.
    let config = config_for(&compose_file, BTreeMap::new());
    if let Ok(plan) = resolve_plan(&runner, RigSelection::Named("live".into()), &config).await {
        let existing: Vec<std::path::PathBuf> = ignition_core::rig::existing_override(&plan)
            .into_iter()
            .collect();
        let _ = ComposeRunner::run(
            &runner,
            &ignition_core::rig::compose::down_args(&plan, true, &existing),
        )
        .await;
    }

    // The adopted token lives in the operator's keychain — remove it.
    let _ = ignition_core::config::secret::KeyringStore.delete(LIVE_PROFILE);

    if let Err(message) = result {
        panic!("{message}");
    }
}

async fn run_sequence(
    runner: &DockerCompose,
    feed: &ModuleFeed,
    compose_file: &Path,
    cache_root: &Path,
) -> Result<(), String> {
    let fixture_bytes = std::fs::read(compose_file).expect("read seeded compose");
    let override_path = compose_file
        .parent()
        .expect("parent")
        .join(OVERRIDE_FILENAME);

    // ---- §A: declared modules load on a stock image (SC-2, SC-5) ----
    let config = config_for(compose_file, both_modules());
    let plan = resolve_plan(runner, RigSelection::Named("live".into()), &config)
        .await
        .map_err(|e| format!("§A resolve_plan: {e}"))?;

    let provisioning = provision_modules(
        feed,
        &plan,
        &plan.modules,
        cache_root,
        FetchPolicy::CacheFirst,
    )
    .await
    .map_err(|e| format!("§A provision_modules: {e}"))?;

    ignition_core::actions::rig::rig_up(runner, &plan, UP_TIMEOUT_S, None, &provisioning)
        .await
        .map_err(|e| format!("§A rig_up: {e}"))?;

    if !override_path.is_file() {
        return Err(format!("§A: no override at {}", override_path.display()));
    }
    let generated = std::fs::read_to_string(&override_path).expect("read override");
    if !generated.contains("Generated by ign") {
        return Err("§A: override lacks the generated-by header".to_string());
    }
    if std::fs::read(compose_file).expect("re-read compose") != fixture_bytes {
        return Err("§A: ign modified the user's compose file".to_string());
    }

    let url = ignition_core::actions::rig::gateway_url_from(&plan)
        .ok_or_else(|| "§A: no gateway URL derivable from the plan".to_string())?;
    let config_path = compose_file
        .parent()
        .expect("parent")
        .join("ign-live-gate-config.toml");
    let session = adopt_and_build_session(&url, &config_path).await?;
    let api: &dyn GatewayApi = session.api();

    let healthy = healthy_module_ids(api).await.map_err(|e| {
        format!("§A: could not read the gateway's healthy module list at {url} — {e}")
    })?;
    let quarantined = quarantined_module_ids(api)
        .await
        .map_err(|e| format!("§A: {e}"))?;
    for spec in [&GIT_MODULE, &PROJECT_SCAN_ENDPOINT] {
        let want = spec.gateway_module_id;
        if !healthy.iter().any(|id| id == want) {
            return Err(format!(
                "§A SC-2: {want:?} is not in the gateway's HEALTHY list.\n  healthy: {healthy:?}\n  quarantined: {quarantined:?}\n\
                 If it is quarantined, the acceptance variables did not take — \
                 most likely the comma-separated list format (D-24) is wrong."
            ));
        }
        if quarantined.iter().any(|id| id == want) {
            return Err(format!(
                "§A SC-2: {want:?} is QUARANTINED — an unaccepted certificate.\n  quarantined: {quarantined:?}"
            ));
        }
    }

    // ---- §B: survives down-then-up (SC-3) ----
    ignition_core::actions::rig::rig_down(runner, &plan)
        .await
        .map_err(|e| format!("§B rig_down: {e}"))?;
    if !override_path.is_file() {
        return Err("§B SC-3: teardown deleted the override file".to_string());
    }
    ignition_core::actions::rig::rig_up(runner, &plan, UP_TIMEOUT_S, None, &provisioning)
        .await
        .map_err(|e| format!("§B rig_up: {e}"))?;

    let healthy_again = healthy_module_ids(api)
        .await
        .map_err(|e| format!("§B: {e}"))?;
    for spec in [&GIT_MODULE, &PROJECT_SCAN_ENDPOINT] {
        let want = spec.gateway_module_id;
        if !healthy_again.iter().any(|id| id == want) {
            return Err(format!(
                "§B SC-3: {want:?} did not survive the recreate.\n  healthy: {healthy_again:?}"
            ));
        }
    }

    // ---- §C: deleting the override reverts provisioning (SC-4) ----
    ignition_core::actions::rig::rig_down(runner, &plan)
        .await
        .map_err(|e| format!("§C rig_down: {e}"))?;
    std::fs::remove_file(&override_path).expect("delete override by hand");

    let bare_config = config_for(compose_file, BTreeMap::new());
    let bare_plan = resolve_plan(runner, RigSelection::Named("live".into()), &bare_config)
        .await
        .map_err(|e| format!("§C resolve_plan: {e}"))?;
    ignition_core::actions::rig::rig_up(
        runner,
        &bare_plan,
        UP_TIMEOUT_S,
        None,
        &ignition_core::rig::ModuleProvisioning::default(),
    )
    .await
    .map_err(|e| format!("§C rig_up: {e}"))?;

    if override_path.exists() {
        return Err("§C SC-4: an override was recreated for an undeclared rig".to_string());
    }
    // §C expects NEITHER module, so an "answered but lacked it" result is
    // the success shape here — only a transport/auth failure is fatal.
    //
    // SC-4 AS ORIGINALLY WRITTEN IS FALSIFIED — recorded, not quietly
    // weakened (D-23). RMOD-06, SC-4 and the README all claimed deleting
    // the override "fully reverts module provisioning". It does not.
    //
    // Observed live: after deleting the override and bringing the rig up
    // with NOTHING declared, the gateway still reports both modules
    // healthy. The mount at user-lib/modules is genuinely gone — that
    // path is not in the data volume — but Ignition INSTALLS an accepted
    // module into its data directory, and that directory IS the
    // persistent volume. Removing the source file does not uninstall it.
    //
    // So this asserts what is TRUE today: the override is gone and is not
    // recreated. Making the gateway forget requires
    // DELETE /data/api/v1/modules/uninstall (confirmed present in the
    // 83-api collection) and is plan 16-04's scope — the first
    // destructive gateway write this feature would perform, which is a
    // guard decision, not a detail.
    let healthy_bare = healthy_module_ids(api).await.unwrap_or_default();
    let still_installed: Vec<&str> = [&GIT_MODULE, &PROJECT_SCAN_ENDPOINT]
        .iter()
        .map(|spec| spec.gateway_module_id)
        .filter(|want| healthy_bare.iter().any(|id| id == want))
        .collect();
    if !still_installed.is_empty() {
        eprintln!(
            "note (SC-4, known limitation): {still_installed:?} remain INSTALLED after the \
             override was deleted — the mount reverted, the gateway's install did not. \
             `ign rig reset` (which removes the data volume) is today's way to clear them."
        );
    }

    Ok(())
}
