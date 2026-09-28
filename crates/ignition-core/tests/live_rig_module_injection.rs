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
//! | `IGNITION_LIVE_RIG_IMAGE_TAG` | Ignition image tag for the fixture. Defaults to `8.3.3`. |
//!
//! The uninstall contract was probed live on 8.3.3 only, so run the gate on a
//! second pinned 8.3.x before trusting it:
//!
//! ```text
//! IGNITION_LIVE_RIG_MODULES=1 cargo test -p ignition-core --test live_rig_module_injection -- --ignored --nocapture
//! IGNITION_LIVE_RIG_MODULES=1 IGNITION_LIVE_RIG_IMAGE_TAG=8.3.9 cargo test -p ignition-core --test live_rig_module_injection -- --ignored --nocapture
//! ```
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
//! Sections A through E run as ONE test in order. Rust does not guarantee
//! test ordering, and §B's entire point is that it runs against §A's state;
//! bringing a gateway up three times would also cost minutes for no signal.
//!
//! | Section | What it proves |
//! |---|---|
//! | §A | a declared module is MOUNTED and reaches the gateway's healthy list, never quarantine (SC-2, SC-5) |
//! | §B | it survives a container recreate (SC-3) |
//! | §C | undeclaring it makes `rig up` delete the override and report the orphans, naming only what `ign` placed (SC-4) |
//! | §D | `ign`'s own uninstall leaves it ABSENT from the gateway's healthy list — **the only proof of SC-1 in this repository** |
//! | §E | every gateway-installed module `ign` did NOT place is still healthy afterwards (SC-4) |
//!
//! §D is irreversible: a uninstalled module does not come back when
//! re-declared (live-verified). Nothing may run after it that needs those
//! modules present.

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

/// The tag the fixture defaults to. Kept in sync with the `:-` default in
/// `tests/fixtures/live-rig/compose.yml` — the test only REPORTS which image
/// a run exercised. It never sets the variable: `std::env::set_var` is unsafe
/// under edition 2024, and the compose child already inherits the operator's
/// environment.
const DEFAULT_IMAGE_TAG: &str = "8.3.3";

fn image_tag() -> String {
    std::env::var("IGNITION_LIVE_RIG_IMAGE_TAG")
        .ok()
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| DEFAULT_IMAGE_TAG.to_string())
}

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

/// A healthy list that ANSWERED, whatever it contains. [`healthy_module_ids`]
/// retries until the git module appears, which is the wrong polarity once §D
/// has removed it; this one retries only on a failed call. Pitfall 4's
/// post-recreate 401 window is why it retries at all.
async fn list_healthy_ids(api: &dyn GatewayApi) -> Result<Vec<String>, String> {
    let mut last_err = String::from("never attempted");
    for _ in 0..MODULE_LIST_ATTEMPTS {
        match api.modules(false, &ListQuery::default()).await {
            Ok(envelope) => return Ok(envelope.items.iter().map(|m| m.id.clone()).collect()),
            Err(err) => last_err = format!("modules() failed: {err}"),
        }
        tokio::time::sleep(std::time::Duration::from_secs(MODULE_LIST_INTERVAL_S)).await;
    }
    Err(last_err)
}

/// The opposite polarity of [`healthy_module_ids`]: succeed when the id
/// is ABSENT. Same bounded retry and the same carry-the-error discipline,
/// because a list that fails to answer is NOT an absent module — conflating
/// the two is how §A once reported `healthy: []` for what was really a 401.
///
/// The loop tolerates errors rather than treating the first as fatal: a
/// gateway that has just recreated can answer 401 for a window before its
/// session settles (Pitfall 4).
async fn module_absent_from_healthy(
    api: &dyn GatewayApi,
    gateway_module_id: &str,
) -> Result<Vec<String>, String> {
    let mut last_err = String::from("never attempted");
    for _ in 0..MODULE_LIST_ATTEMPTS {
        match api.modules(false, &ListQuery::default()).await {
            Ok(envelope) => {
                let observed: Vec<String> = envelope.items.iter().map(|m| m.id.clone()).collect();
                if !observed.iter().any(|id| id == gateway_module_id) {
                    return Ok(observed);
                }
                last_err = format!("{gateway_module_id:?} still present: {observed:?}");
            }
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

/// §A mounted+loaded, §B survives recreate, §C undeclaring clears the
/// override and reports the orphans, §D uninstalling removes the module
/// from the gateway (SC-1) — one ordered sequence, torn down on every
/// exit path. §D runs last because it is irreversible: only `rig reset`
/// brings the modules back.
#[ignore = "live: needs Docker and IGNITION_LIVE_RIG_MODULES=1"]
#[tokio::test]
async fn live_rig_module_injection_sequence() {
    if !live_enabled() {
        skip("IGNITION_LIVE_RIG_MODULES is not 1");
        return;
    }

    eprintln!(
        "live gate: inductiveautomation/ignition:{} (set IGNITION_LIVE_RIG_IMAGE_TAG to select another)",
        image_tag()
    );

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
    // NOTE: the override is NOT deleted by hand here any more. Doing that
    // was what masked `write_override`'s dead delete branch for a whole
    // phase — the test performed the cleanup it was supposed to be
    // checking. `rig up` must delete it, through the real seam.
    let bare_config = config_for(compose_file, BTreeMap::new());
    let bare_plan = resolve_plan(runner, RigSelection::Named("live".into()), &bare_config)
        .await
        .map_err(|e| format!("§C resolve_plan: {e}"))?;
    // The REAL undeclared path: provision_modules with nothing declared
    // reads the pre-overwrite override, computes the orphans, and clears
    // the file. Passing ModuleProvisioning::default() here would bypass
    // exactly the code under test.
    let undeclared = provision_modules(
        feed,
        &bare_plan,
        &BTreeMap::new(),
        cache_root,
        FetchPolicy::CacheFirst,
    )
    .await
    .map_err(|e| format!("§C provision_modules: {e}"))?;

    ignition_core::actions::rig::rig_up(runner, &bare_plan, UP_TIMEOUT_S, None, &undeclared)
        .await
        .map_err(|e| format!("§C rig_up: {e}"))?;

    let orphan_ids: Vec<&str> = undeclared
        .orphaned_modules
        .iter()
        .map(|o| o.gateway_module_id.as_str())
        .collect();
    for spec in [&GIT_MODULE, &PROJECT_SCAN_ENDPOINT] {
        if !orphan_ids.contains(&spec.gateway_module_id) {
            return Err(format!(
                "§C: {:?} was provisioned last run and is no longer declared, so it must be \
                 reported as an orphan. reported: {orphan_ids:?}",
                spec.gateway_module_id
            ));
        }
    }
    if orphan_ids.len() != 2 {
        return Err(format!(
            "§C SC-4 (image {}): the orphan report must name ONLY what ign provisioned — reported \
             {orphan_ids:?}, expected exactly the two registry modules",
            image_tag()
        ));
    }

    if override_path.exists() {
        return Err("§C SC-4: an override was recreated for an undeclared rig".to_string());
    }
    // The two modules are STILL INSTALLED at this point, and that is no
    // longer a limitation — it is §D's PRECONDITION.
    //
    // The mount at user-lib/modules is gone, but Ignition installed each
    // accepted module into its data directory, which is the persistent
    // volume. What remains is a GHOST registry entry: present in the
    // healthy list, with an empty name/version and no state. Live-verified
    // during Phase 19 research — and a ghost is exactly what the gateway
    // WILL uninstall, whereas it refuses a module whose .modl is still
    // mounted. That ordering is why §D runs after §C and not before.
    let before = healthy_module_ids(api)
        .await
        .map_err(|e| format!("§D precondition: {e}"))?;
    eprintln!("§D healthy BEFORE uninstall: {before:?}");

    // The SC-4 seed (D-19-08): every module the gateway has installed that
    // `ign` did NOT place. `ign` has no install path other than the mount,
    // so the stock image's own modules ARE that seed — real modules, present
    // for real reasons, and exactly what an over-broad diff or an over-broad
    // uninstall would take.
    let registry_ids = [
        GIT_MODULE.gateway_module_id,
        PROJECT_SCAN_ENDPOINT.gateway_module_id,
    ];
    let seed: Vec<String> = before
        .iter()
        .filter(|id| !registry_ids.contains(&id.as_str()))
        .cloned()
        .collect();
    eprintln!(
        "§E SC-4 seed ({} modules ign did not place): {seed:?}",
        seed.len()
    );
    if seed.is_empty() {
        return Err(format!(
            "§E SC-4 (image {}): the seed set is EMPTY, which VOIDS SC-4 rather than \
             satisfying it — with nothing that ign did not place, 'ign touched nothing it \
             should not' is vacuously true. observed healthy: {before:?}",
            image_tag()
        ));
    }

    // SC-4, first half: ign may only ever nominate what it provisioned itself.
    for id in &seed {
        if orphan_ids.contains(&id.as_str()) {
            return Err(format!(
                "§C SC-4: {id:?} is a gateway-installed module ign never placed, and ign \
                 nominated it as an orphan. orphans: {orphan_ids:?}, seed: {seed:?}"
            ));
        }
    }

    // ---- §D: SC-1 — the module is actually GONE from the gateway ----
    //
    // One call per module, never a batch: batch atomicity was flagged LOW
    // confidence and never probed, so a partial failure would be
    // indistinguishable from a total one (D-19-04).
    for spec in [&GIT_MODULE, &PROJECT_SCAN_ENDPOINT] {
        ignition_core::actions::rig::rig_module_uninstall(api, &plan.name, spec, &BTreeMap::new())
            .await
            .map_err(|e| {
                format!(
                    "§D: uninstalling {:?} failed — a success:false denial surfaces here as \
                 module_uninstall_denied, and must not be swallowed: {e}",
                    spec.gateway_module_id
                )
            })?;
    }

    for spec in [&GIT_MODULE, &PROJECT_SCAN_ENDPOINT] {
        let after = module_absent_from_healthy(api, spec.gateway_module_id)
            .await
            .map_err(|e| {
                format!(
                    "§D SC-1 (image {}): {:?} is STILL in the gateway's healthy list after ign \
                     reported a successful uninstall — the gateway did not perform what ign \
                     reported. {e}",
                    image_tag(),
                    spec.gateway_module_id
                )
            })?;
        eprintln!(
            "§D healthy AFTER uninstalling {}: {after:?}",
            spec.gateway_module_id
        );
    }

    // ---- §E: SC-4 — ign removed NOTHING it did not place ----
    let after_all = list_healthy_ids(api)
        .await
        .map_err(|e| format!("§E: healthy list never answered after §D: {e}"))?;
    for id in &seed {
        if !after_all.contains(id) {
            return Err(format!(
                "§E SC-4 (image {}): {id:?} was installed on the gateway and ign never placed \
                 it, yet it is GONE after ign's uninstall — ign removed a module the user never \
                 asked it to touch. seed: {seed:?}, observed healthy: {after_all:?}",
                image_tag()
            ));
        }
    }
    eprintln!(
        "§E {} of {} seed modules ign did not place survived the uninstall: {after_all:?}",
        seed.len(),
        seed.len()
    );

    // Nothing may follow that assumes these modules can return: an
    // uninstall is not undone by re-declaring (live-verified), and only
    // `rig reset` — destroying the volume — brings the rig back.

    Ok(())
}
