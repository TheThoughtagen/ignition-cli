//! Opt-in live-gateway suite — `#[ignore]`-gated because it needs a real
//! commissioned Ignition 8.3+ gateway. NOT run by CI and not required for
//! plan execution (wiremock covers the contract); run it to close the
//! live-auth gap empirically on a gateway you control:
//!
//! ```text
//! cargo test -p ignition-core --test live_gateway -- --ignored
//! ```
//!
//! Skip behavior: each test reads its env vars at start and returns
//! quietly when they are absent — `-- --ignored` with no envs set is a
//! green no-op, never a failure.
//!
//! ## Rig recipe (verified end-to-end, 02-RESEARCH §Test architecture)
//!
//! ```bash
//! docker run -d --name ign-research -p 18088:8088 \
//!   -e ACCEPT_IGNITION_EULA=Y inductiveautomation/ignition:8.3.6
//! ```
//!
//! 1. Commission via `http://localhost:18088/welcome` (browser): pick
//!    "Ignition" standard → trial mode, create the admin user, Finish
//!    Setup → Start Gateway.
//! 2. UI: Platform → Security → API Keys → Create: **Basic Token**, name
//!    it, **UNCHECK "Require secure connections"** (http rig!), pick a
//!    security level with admin.
//! 3. Copy the FULL `name:key` string the dialog shows — both halves.
//!
//! ## Environment
//!
//! | var | required by | meaning |
//! |---|---|---|
//! | `IGNITION_LIVE_URL` | every test | base URL, e.g. `http://localhost:18088` |
//! | `IGNITION_LIVE_TOKEN` | auth tests | full `name:key` API-token string |
//! | `IGNITION_LIVE_USER` / `IGNITION_LIVE_PASSWORD` | Basic-rejection test | a VALID commissioned user |
//!
//! Later Phase-2 plans APPEND their live checks to this file.

use ignition_core::client::version::below_minimum;
use ignition_core::client::{GatewayApi, ReqwestGatewayApi};
use ignition_core::config::{Credential, Secret};
use ignition_core::error::CoreError;

/// Non-empty `IGNITION_LIVE_URL`, when set.
fn live_url() -> Option<String> {
    std::env::var("IGNITION_LIVE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// Non-empty `IGNITION_LIVE_TOKEN`, when set.
fn live_token() -> Option<String> {
    std::env::var("IGNITION_LIVE_TOKEN")
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn skip(message: &str) {
    eprintln!("skipping: {message}");
}

/// THE live regression for the 02-01 `ignitionVersion` fix: a real 8.3
/// gateway's gateway-info body must deserialize (`ignition_version`
/// non-empty, at/above the 8.3.1 minimum). Requires the token — a
/// commissioned gateway answers header-less gateway-info with 401
/// (verified live 2026-08-21; the 83-api collection's `auth: none` tag
/// does not hold under 8.3 default security).
#[tokio::test]
#[ignore = "opt-in: set IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN to run against a live gateway"]
async fn live_gateway_info_parses() {
    let (Some(url), Some(token)) = (live_url(), live_token()) else {
        skip("IGNITION_LIVE_URL / IGNITION_LIVE_TOKEN not both set");
        return;
    };
    let api = ReqwestGatewayApi::for_tests(&url, Some(Credential::Token(Secret::new(token))));

    let info = api
        .gateway_info()
        .await
        .expect("live gateway-info must deserialize into the corrected GatewayInfo");
    assert!(
        !info.ignition_version.is_empty(),
        "ignitionVersion non-empty: {:?}",
        info.ignition_version
    );
    assert!(
        !below_minimum(&info.ignition_version),
        "live rig below the supported minimum: {}",
        info.ignition_version
    );
}

/// Closes STATE.md's flagged live-auth gap with an executable proof: the
/// `X-Ignition-API-Token: name:key` header authenticates `/data` routes
/// on a real 8.3 gateway (key-only would 401 — the format IS the
/// contract).
#[tokio::test]
#[ignore = "opt-in: set IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN (full name:key string)"]
async fn live_token_auth_works() {
    let (Some(url), Some(token)) = (live_url(), live_token()) else {
        skip("IGNITION_LIVE_URL / IGNITION_LIVE_TOKEN not both set");
        return;
    };
    let api = ReqwestGatewayApi::for_tests(&url, Some(Credential::Token(Secret::new(token))));

    let info = api
        .gateway_info()
        .await
        .expect("token name:key must authenticate 8.3 /data routes (200, not 401/403)");
    assert!(!info.ignition_version.is_empty());
}

/// Documents the verified reality: valid Basic credentials are REJECTED
/// (401) on 8.3 `/data` routes — the enum arm exists for future/legacy
/// surfaces only, and the client warns loudly on every use.
#[tokio::test]
#[ignore = "opt-in: set IGNITION_LIVE_URL + IGNITION_LIVE_USER + IGNITION_LIVE_PASSWORD"]
async fn live_basic_is_rejected() {
    let Some(url) = live_url() else {
        skip("IGNITION_LIVE_URL not set");
        return;
    };
    let (Some(user), Some(password)) = (
        std::env::var("IGNITION_LIVE_USER").ok(),
        std::env::var("IGNITION_LIVE_PASSWORD").ok(),
    ) else {
        skip("IGNITION_LIVE_USER / IGNITION_LIVE_PASSWORD not both set");
        return;
    };
    let api = ReqwestGatewayApi::for_tests(
        &url,
        Some(Credential::Basic(Secret::new(user), Secret::new(password))),
    );

    let err = api
        .gateway_info()
        .await
        .expect_err("Basic auth must be rejected on 8.3 /data routes");
    assert!(
        matches!(err, CoreError::Auth { status: 401, .. }),
        "expected a 401 Auth rejection, got: {err}"
    );
    assert_eq!(err.exit_code(), 5);
    assert!(
        err.hint().expect("hint").contains("API token"),
        "hint steers toward an API token: {:?}",
        err.hint()
    );
}

// ---------------------------------------------------------------------------
// 02-02 inspection additions: read-only live checks (skip gracefully when
// no rig is configured).
// ---------------------------------------------------------------------------

/// `/StatusPing` needs NO token (the unauthenticated readiness anchor):
/// a URL alone must yield a state string — this is the same primitive
/// `ign wait` (02-05) will poll.
#[tokio::test]
#[ignore = "opt-in: set IGNITION_LIVE_URL (no token needed — StatusPing is unauthenticated)"]
async fn live_status_ping_unauthenticated() {
    let Some(url) = live_url() else {
        skip("IGNITION_LIVE_URL not set");
        return;
    };
    let api = ReqwestGatewayApi::for_tests(&url, None);
    let ping = api
        .status_ping()
        .await
        .expect("StatusPing answers without any credential");
    assert!(
        !ping.state.is_empty(),
        "state surfaces verbatim: {:?}",
        ping.state
    );
}

/// The three authed inspection reads against a live gateway: overview
/// parses (uptime ms ≥ 0, cpu a 0–1 fraction), healthy modules are
/// non-empty (the standard image ships dozens), current gauges parse
/// (cpu percent ≥ 0).
#[tokio::test]
#[ignore = "opt-in: set IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN"]
async fn live_inspection_endpoints_parse() {
    let (Some(url), Some(token)) = (live_url(), live_token()) else {
        skip("IGNITION_LIVE_URL / IGNITION_LIVE_TOKEN not both set");
        return;
    };
    let api = ReqwestGatewayApi::for_tests(&url, Some(Credential::Token(Secret::new(token))));

    let overview = api
        .overview()
        .await
        .expect("live overview must deserialize");
    assert!(overview.uptime >= 0, "uptime ms: {}", overview.uptime);
    assert!(
        (0.0..=1.0).contains(&overview.cpu),
        "cpu is a 0–1 fraction: {}",
        overview.cpu
    );

    let modules = api
        .modules(false, &Default::default())
        .await
        .expect("live modules/healthy must deserialize");
    assert!(
        !modules.items.is_empty(),
        "a standard gateway ships healthy modules"
    );
    assert!(modules.items.iter().all(|m| m.state.as_deref() != Some("")));
    let active = modules
        .items
        .iter()
        .filter(|m| m.state.as_deref() == Some("ACTIVE"))
        .count();
    assert!(active > 0, "{active} modules are ACTIVE");

    let gauges = api
        .metrics_current()
        .await
        .expect("live currentGauges must deserialize");
    assert!(gauges.cpu >= 0.0, "cpu percent: {}", gauges.cpu);
}

// ---------------------------------------------------------------------------
// 02-03 additions: sessions + connections live checks (read-only; empty
// is the expected state on a fresh rig — the checks exist to run against
// a gateway WITH sessions/connections, capturing the populated shapes).
// ---------------------------------------------------------------------------

/// The three session-family lists against a live gateway: all must
/// answer the standard envelope (items may be empty on a headless rig —
/// a connected Designer/Perspective session makes them non-empty).
#[tokio::test]
#[ignore = "opt-in: set IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN"]
async fn live_session_families_parse() {
    let (Some(url), Some(token)) = (live_url(), live_token()) else {
        skip("IGNITION_LIVE_URL / IGNITION_LIVE_TOKEN not both set");
        return;
    };
    let api = ReqwestGatewayApi::for_tests(&url, Some(Credential::Token(Secret::new(token))));

    let designers = api
        .designers(&Default::default())
        .await
        .expect("live designers list must deserialize");
    let sessions = api
        .perspective_sessions(&Default::default())
        .await
        .expect("live perspective list must deserialize (trailing-slash path)");
    let clients = api
        .vision_clients(&Default::default())
        .await
        .expect("live vision list must deserialize");
    eprintln!(
        "live sessions: {} designers, {} perspective, {} vision",
        designers.items.len(),
        sessions.items.len(),
        clients.items.len()
    );
}

/// HLTH-05/06 verification step (research Open Question 1): list the
/// connection resource families against a live gateway. EMPTY is fine
/// (the research rig had zero); when a gateway HAS connections, dump the
/// populated `healthchecks` shapes so the passthrough can be upgraded to
/// a typed model — until then the shape stays LOW-confidence.
#[tokio::test]
#[ignore = "opt-in: set IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN (empty lists OK)"]
async fn live_connections() {
    let (Some(url), Some(token)) = (live_url(), live_token()) else {
        skip("IGNITION_LIVE_URL / IGNITION_LIVE_TOKEN not both set");
        return;
    };
    let api = ReqwestGatewayApi::for_tests(&url, Some(Credential::Token(Secret::new(token))));

    let database = api
        .database_connections()
        .await
        .expect("live database-connection resource list must deserialize");
    let opc = api
        .opc_connections()
        .await
        .expect("live opc-connection resource list must deserialize");
    for connection in database.items.iter().chain(opc.items.iter()) {
        // Capture hook: the FIRST gateway with a configured connection
        // prints the populated healthchecks shape here.
        eprintln!(
            "live connection {:?} enabled={} healthchecks={}",
            connection.name, connection.enabled, connection.healthchecks
        );
    }
}

// ---------------------------------------------------------------------------
// 02-04 additions: logs (read-only by default; the level mutations
// behind IGNITION_LIVE_MUTATIONS=1).
// ---------------------------------------------------------------------------

/// Read-only log checks against a live gateway: `logs?limit=1` parses
/// the live entry shape (epoch-ms timestamps) and the logger registry
/// answers (~1250 loggers on a fresh image; limit=200 explicit).
#[tokio::test]
#[ignore = "opt-in: set IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN"]
async fn live_logs_and_loggers() {
    let (Some(url), Some(token)) = (live_url(), live_token()) else {
        skip("IGNITION_LIVE_URL / IGNITION_LIVE_TOKEN not both set");
        return;
    };
    let api = ReqwestGatewayApi::for_tests(&url, Some(Credential::Token(Secret::new(token))));

    use ignition_core::client::logs::LogQuery;
    let page = api
        .logs(&LogQuery {
            sort_by: Some("desc(timestamp)".into()),
            ..LogQuery::default()
        })
        .await
        .expect("live logs query must deserialize");
    eprintln!(
        "live logs: {} of {} total",
        page.items.len(),
        page.metadata.total
    );
    if let Some(newest) = page.items.first() {
        assert!(
            newest.timestamp > 0,
            "epoch-ms timestamp: {}",
            newest.timestamp
        );
        eprintln!(
            "live newest: {} {} {}",
            newest.timestamp, newest.level, newest.logger_name
        );
    }

    let loggers = api
        .loggers(&ignition_core::client::query::ListQuery {
            limit: 200,
            ..Default::default()
        })
        .await
        .expect("live logger registry must deserialize");
    assert!(!loggers.items.is_empty(), "a gateway ships loggers");
    eprintln!("live loggers: first of {}", loggers.metadata.total);
}

/// The level mutations, double-opt-in (mutations are audit-logged
/// server-side): set one logger to its current level, read it back,
/// reset. Pick a harmless logger — the gateway's own GatewayManager.
#[tokio::test]
#[ignore = "opt-in: set IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN + IGNITION_LIVE_MUTATIONS=1"]
async fn live_logger_level_set_and_reset() {
    let (Some(url), Some(token)) = (live_url(), live_token()) else {
        skip("IGNITION_LIVE_URL / IGNITION_LIVE_TOKEN not both set");
        return;
    };
    if std::env::var("IGNITION_LIVE_MUTATIONS").as_deref() != Ok("1") {
        skip("IGNITION_LIVE_MUTATIONS=1 not set (mutations stay off)");
        return;
    }
    let api = ReqwestGatewayApi::for_tests(&url, Some(Credential::Token(Secret::new(token))));

    api.set_logger_level("GatewayManager", "INFO")
        .await
        .expect("set-logger-level must succeed with a token (no CSRF)");
    api.reset_logger_levels()
        .await
        .expect("levelreset must succeed");
}

// ---------------------------------------------------------------------------
// 03-01 addition: projects list (read-only) — optional live truth for
// the list envelope/item shape the moment a token exists (research
// Open Question 2: item shape MEDIUM until captured).
// ---------------------------------------------------------------------------

/// `projects/list` against a live gateway: the envelope must answer;
/// items may be empty on a fresh rig — the check exists to capture the
/// POPULATED item shape (the `extra` passthrough keeps corrections
/// cheap until then).
#[tokio::test]
#[ignore = "opt-in: set IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN (empty list OK)"]
async fn live_projects_list() {
    let (Some(url), Some(token)) = (live_url(), live_token()) else {
        skip("IGNITION_LIVE_URL / IGNITION_LIVE_TOKEN not both set");
        return;
    };
    let api = ReqwestGatewayApi::for_tests(&url, Some(Credential::Token(Secret::new(token))));

    let page = api
        .projects(&Default::default())
        .await
        .expect("live projects/list must deserialize");
    eprintln!(
        "live projects: {} of {} total",
        page.items.len(),
        page.metadata.total
    );
    // Capture hook: dump full records so unmodeled keys surface (the
    // passthrough upgrade path).
    for project in page.items.iter().take(5) {
        eprintln!(
            "live project {:?} parent={:?} inheritable={:?} extra={:?}",
            project.name, project.parent, project.inheritable, project.extra
        );
    }
}

// ---------------------------------------------------------------------------
// 02-05 addition: doctor end-to-end (read-only subset — no --check-write,
// no --webdev-route; those probe mutations/route specifics the rig may
// not have).
// ---------------------------------------------------------------------------

/// The full doctor sequence against a live gateway: url + liveness
/// must be ok on any healthy rig; the checks[] table prints to stderr
/// for eyeballing. Read-only (scan/projects never fires without
// --check-write).
#[tokio::test]
#[ignore = "opt-in: set IGNITION_LIVE_URL (+ IGNITION_LIVE_TOKEN for the authed checks)"]
async fn live_doctor_end_to_end() {
    let Some(url) = live_url() else {
        skip("IGNITION_LIVE_URL not set");
        return;
    };
    let token = live_token();
    let credential = token
        .clone()
        .map(|token| Credential::Token(Secret::new(token)));
    let api = ReqwestGatewayApi::for_tests(&url, credential);
    let opts = ignition_core::actions::doctor::DoctorOptions::default();
    let result = ignition_core::actions::doctor::doctor(&api, &url, token.is_some(), &opts).await;
    for check in &result.checks {
        eprintln!(
            "live doctor: {:<12} {:?} {}",
            check.name, check.status, check.detail
        );
    }
    let by_name = |name: &str| {
        result
            .checks
            .iter()
            .find(|check| check.name == name)
            .unwrap_or_else(|| panic!("{name} row present"))
    };
    assert_eq!(
        by_name("url").status,
        ignition_core::actions::doctor::CheckStatus::Ok
    );
    assert_eq!(
        by_name("liveness").status,
        ignition_core::actions::doctor::CheckStatus::Ok
    );
    if token.is_some() {
        assert_eq!(
            by_name("auth").status,
            ignition_core::actions::doctor::CheckStatus::Ok
        );
    }
}

// ---------------------------------------------------------------------------
// 03-02 addition: export → import round-trip (MUTATION-gated — the
// 02-04 IGNITION_LIVE_MUTATIONS precedent; also the preview of 03-03's
// e2e loop). Optional live truth for the MEDIUM export/import response
// bodies (research Open Question 3): what the import POST actually
// answers prints to stderr.
// ---------------------------------------------------------------------------

/// Non-empty `IGNITION_LIVE_MUTATIONS=1`, when set — the opt-in gate
/// for live tests that CHANGE gateway state.
fn live_mutations_enabled() -> bool {
    std::env::var("IGNITION_LIVE_MUTATIONS").ok().as_deref() == Some("1")
}

/// The full loop on a timestamped scratch project: create → export to
/// a temp file (streaming) → abort-policy import over the existing
/// name (must refuse `project_exists` BEFORE any upload) →
/// overwrite-policy import (must succeed; the outcome prints) →
/// delete cleanup (best-effort).
#[tokio::test]
#[ignore = "opt-in: set IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN + IGNITION_LIVE_MUTATIONS=1"]
async fn live_project_export_import_round_trip() {
    let (Some(url), Some(token)) = (live_url(), live_token()) else {
        skip("IGNITION_LIVE_URL / IGNITION_LIVE_TOKEN not both set");
        return;
    };
    if !live_mutations_enabled() {
        skip("IGNITION_LIVE_MUTATIONS != 1 — mutation-gated");
        return;
    }
    use ignition_core::actions::projects::{self, CollisionPolicy};
    use ignition_core::client::projects::ProjectCreate;

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default();
    let name = format!("ign-live-export-{ts}");
    let api = ReqwestGatewayApi::for_tests(&url, Some(Credential::Token(Secret::new(token))));

    // Create the scratch project (only name + enabled — the server
    // tolerates the partial body).
    api.project_create(&ProjectCreate {
        name: name.clone(),
        enabled: true,
        title: None,
        description: None,
        parent: None,
        inheritable: None,
        default_db: None,
        tag_provider: None,
        user_source: None,
    })
    .await
    .expect("scratch project created");

    // Export streams to a temp file.
    let out = std::env::temp_dir().join(format!("{name}.zip"));
    let export = match api.project_export_to_file(&name, &out).await {
        Ok(meta) => meta,
        Err(err) => {
            let _ = api.project_delete(&name).await; // cleanup
            panic!("live export must stream: {err}");
        }
    };
    eprintln!(
        "live export: {} bytes, disposition {:?}, content-type {:?}",
        export.bytes, export.filename, export.content_type
    );
    let zip = std::fs::read(&out).expect("export file readable");

    // Abort-policy import over the existing name: the ACTION's find
    // pre-check must refuse BEFORE any upload.
    let err = projects::project_import(&api, &name, zip.clone(), CollisionPolicy::Abort)
        .await
        .expect_err("abort over existing must refuse");
    assert!(
        matches!(err, CoreError::ProjectExists { .. }),
        "wrong class: {err}"
    );

    // Overwrite-policy import: must succeed; the opaque outcome prints
    // (the live capture of the MEDIUM response body).
    let result = projects::project_import(&api, &name, zip, CollisionPolicy::Overwrite)
        .await
        .expect("overwrite import succeeds");
    eprintln!("live import outcome: {}", result.outcome);

    // Cleanup (best-effort — failures leave forensic state).
    let _ = api.project_delete(&name).await;
    let _ = std::fs::remove_file(&out);
}

// ---------------------------------------------------------------------------
// 10-05 addition: the SC-5 live gate — ONE guarded EAM write lifecycle
// (scratch create → suspend → verify → resume → delete) end-to-end against
// the REAL WHK controller rig through the REAL action layer
// (actions::eam — the product's correctness path, not raw client calls).
//
// Wire truth cited from 10-LIVE-CAPTURES.md (both 8.3.3 + 8.3.6 rigs):
// - suspend/resume 204 on a Scheduled task with a registered trigger and
//   PERSIST `config.profile.isSuspended` (Decision 1); suspend of an
//   OnDemand task answers the indistinguishable 500 "Task could not be
//   suspended" (§1a) — so the gate flips its own scratch task to
//   Scheduled+cron (capture §6a full-record modify) and retries suspend
//   until the gateway registers the trigger (~80 s captured, §1c).
// - A suspended task vanishes from `scheduled/false`; resume brings it
//   back (§2's same-second observation).
// - A lone-resource delete succeeds WITHOUT `?confirm=` (§3b) and
//   post-delete find answers the config-resource 404 (`not_found`).
// - NO force call anywhere: lifecycle writes only, so an expired trial
//   cannot false-fail the gate (phase pitfall 8).
//
// SAFETY INVARIANT (enforced in code, not convention): the test derives
// its task name from the fixed `ign-live-scratch` prefix + an epoch
// suffix, CREATES the task itself, and asserts the found record's name
// equals the scratch name IMMEDIATELY BEFORE every write (flip, suspend,
// resume, delete). A misconfigured IGNITION_LIVE_URL pointing at a
// production gateway can never touch a real task — the pre-write name
// assertion fails first. No environment-supplied task name is ever used
// for a write.
// ---------------------------------------------------------------------------

/// PRE-WRITE SAFETY GATE: find the record at the scratch name and assert
/// it IS our scratch task before any write may fire. Returns the record
/// (the flip's clone source).
async fn assert_scratch_task(
    api: &dyn GatewayApi,
    scratch: &str,
) -> ignition_core::client::eam::EamTaskRecord {
    let record = api
        .eam_task_find(scratch)
        .await
        .unwrap_or_else(|err| panic!("pre-write find for scratch task failed: {err}"));
    assert_eq!(
        record.name, scratch,
        "PRE-WRITE NAME ASSERTION FAILED: the gateway's record at the scratch name \
         is not our scratch task — refusing every write (misconfigured live URL?)"
    );
    record
}

/// The Jetty-page message the gateway answers while the scheduler trigger
/// has not registered yet (capture §1c — up to ~80 s after the cron lands).
fn is_suspend_trigger_pending(err: &CoreError) -> bool {
    matches!(err, CoreError::Internal(msg) if msg.contains("Task could not be suspended"))
}

/// The scratch task's Drop guard — the wiremock mock-guard pattern for a
/// REMOTE resource: best-effort delete on EVERY path, so a mid-test panic
/// cannot leave scratch tasks behind. Drop is sync, so it builds its own
/// single-thread runtime and a FRESH client (never drives the test
/// runtime's client from a different event loop).
struct ScratchTaskGuard {
    url: String,
    token: String,
    name: String,
    /// Set when the test body itself completed the delete — the Drop
    /// path then only logs (no double delete, honest cleanup log).
    disarmed: bool,
}

impl ScratchTaskGuard {
    fn disarm(&mut self) {
        self.disarmed = true;
    }
}

/// The shared cleanup choreography — find → derive signature → delete →
/// honest logs. Constructed fresh per Drop path (it owns its client and
/// identifiers; never drives the test runtime's client from another loop).
async fn scratch_cleanup_future(url: String, token: String, name: String) {
    let api = ReqwestGatewayApi::for_tests(&url, Some(Credential::Token(Secret::new(token))));
    match api.eam_task_find(&name).await {
        Ok(record) => {
            if let Some(signature) = record.signature.clone() {
                match api.eam_task_delete(&name, &signature, false).await {
                    Ok(_) => eprintln!(
                        "cleanup: scratch task {name:?} best-effort deleted (Drop path)"
                    ),
                    Err(err) => eprintln!(
                        "cleanup: scratch task {name:?} Drop-time delete FAILED — remove it manually: {err}"
                    ),
                }
            }
        }
        Err(_) => eprintln!(
            "cleanup: scratch task {name:?} not found at Drop (already gone)"
        ),
    }
}

impl Drop for ScratchTaskGuard {
    fn drop(&mut self) {
        let name = self.name.clone();
        if self.disarmed {
            eprintln!("cleanup: scratch task {name:?} deleted by the test body (Drop disarmed)");
            return;
        }
        let url = self.url.clone();
        let token = self.token.clone();
        let name_for_cleanup = name.clone();
        let cleanup = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if tokio::runtime::Handle::try_current().is_ok() {
                // Dropped INSIDE the test runtime (the #[tokio::test] unwind
                // case — the UAT gate failure): block_on from here panics
                // "Cannot start a runtime from within a runtime", catch_unwind
                // swallows it, and the scratch task survives. Route the cleanup
                // through the BLOCKING pool instead of tokio::spawn: a spawned
                // async task may never be polled after unwind (runtime shutdown
                // cancels pending tasks), while the blocking pool is JOINED at
                // runtime drop — and a spawn_blocking thread is not an async
                // context, so a fresh runtime + block_on is legal there.
                tokio::runtime::Handle::current()
                    .spawn_blocking(move || {
                        tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                            .expect("cleanup runtime builds")
                            .block_on(scratch_cleanup_future(
                                url.clone(),
                                token.clone(),
                                name_for_cleanup.clone(),
                            ));
                    });
            } else {
                // No ambient runtime (Drop after the test runtime is gone):
                // the original fresh-current-thread path, verbatim.
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("cleanup runtime builds")
                    .block_on(scratch_cleanup_future(url, token, name_for_cleanup));
            }
        }));
        if cleanup.is_err() {
            eprintln!(
                "cleanup: scratch task {:?} Drop cleanup itself panicked — remove it manually",
                name
            );
        }
    }
}

/// THE SC-5 GATE: one guarded EAM write lifecycle end-to-end.
#[tokio::test]
#[ignore = "opt-in: set IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN (a WHK controller rig with EAM installMode=Controller)"]
async fn live_eam_write_lifecycle() {
    let (Some(url), Some(token)) = (live_url(), live_token()) else {
        skip("IGNITION_LIVE_URL / IGNITION_LIVE_TOKEN not both set");
        return;
    };
    let api =
        ReqwestGatewayApi::for_tests(&url, Some(Credential::Token(Secret::new(token.clone()))));

    // Scratch isolation: fixed prefix + epoch suffix — unique per run,
    // created by this test, written only after the name assertion.
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default();
    let scratch = format!("ign-live-scratch-{ts}");
    let mut guard = ScratchTaskGuard {
        url: url.clone(),
        token: token.clone(),
        name: scratch.clone(),
        disarmed: false,
    };

    // Controller-gate diagnostic FIRST (one cheap read): a 403
    // "configured as a controller" here is RIG CONFIG, not code —
    // fail with the operator-facing message instead of a confusing
    // write-path error later.
    if let Err(CoreError::EamNotController { .. }) = api.eam_tasks_scheduled(false).await {
        panic!(
            "the live URL is NOT an EAM controller — every /data/eam/api/v1/* operation \
             refuses until the EAM module's installMode is flipped to Controller \
             (headless recipe: 10-RIG-NOTES.md). This is a rig-config error, not a code bug."
        );
    }

    // 1. CREATE — the action layer (eam_backup + OnDemand is the unguarded
    //    ladder cell; the composer ALWAYS sends config.settings — the
    //    422 trap). Zero --target values default to ["_controller"].
    let created = ignition_core::actions::eam::eam_task_create(
        &api,
        &scratch,
        "eam_backup",
        &[],
        &[],
        None,
        "OnDemand",
    )
    .await
    .expect("scratch task create through the action layer must succeed");
    assert_eq!(created.name, scratch);
    assert_eq!(
        created.task_type, "eam_backup",
        "created type echoes the request"
    );
    eprintln!(
        "gate step create: scratch task {scratch:?} created (eam_backup/OnDemand, action layer)"
    );

    // 2. CREATE READ-BACK + first NAME ASSERTION: the found record IS the
    //    scratch task, carries the profile type, and config.settings rode
    //    the create body (the 422 trap did not fire).
    let record = assert_scratch_task(&api, &scratch).await;
    assert_eq!(
        record.config.get("profile").and_then(|p| p.get("type")),
        Some(&serde_json::Value::String("eam_backup".to_string())),
        "created record's profile.type"
    );
    assert!(
        record.config.get("settings").is_some(),
        "config.settings present on the created record (create 422 trap avoided)"
    );

    // 3. FLIP the scratch task to Scheduled + cron — capture §6a verbatim:
    //    the FULL-RECORD clone (every key find answered, null healthcheck
    //    placeholder dropped) mutated at profile.scheduleMode/profile.
    //    scheduleDetails, PUT carrying the ORIGINAL signature. Suspend of
    //    an OnDemand task is the captured 500 (§1a) — the trigger must
    //    exist, and this flip is the capture-proven path to a suspendable
    //    task. (actions::eam::TaskChange deliberately cannot set
    //    scheduleDetails — capture-honest scope — so the flip rides the
    //    client's §6a-pinned full-record modify.)
    let mut body = serde_json::to_value(&record).expect("find record serializes for the clone");
    if body.get("scheduledTaskState") == Some(&serde_json::Value::Null)
        && let Some(map) = body.as_object_mut()
    {
        map.remove("scheduledTaskState");
    }
    {
        let profile = body
            .get_mut("config")
            .and_then(|config| config.get_mut("profile"))
            .expect("the created record carries config.profile");
        profile["scheduleMode"] = serde_json::Value::String("Scheduled".to_string());
        // The capture-proven cron (§6a/§12 — every 30 s).
        profile["scheduleDetails"] = serde_json::Value::String("0/30 * * * * ?".to_string());
    }
    assert_scratch_task(&api, &scratch).await; // pre-write name assertion
    api.eam_task_modify(&body)
        .await
        .expect("schedule flip PUT (capture §6a full-record echo-modify shape) must land");
    eprintln!(
        "gate step flip: {scratch:?} is Scheduled with cron \"0/30 * * * * ?\" — waiting for \
         the gateway to register the trigger (captured ~80 s, §1c)"
    );

    // 4. SUSPEND — the action layer, retried while the trigger is still
    //    registering (the captured late-500, §1c). Every attempt re-runs
    //    the pre-write name assertion FIRST.
    let suspend = {
        let mut result = None;
        for attempt in 1..=7 {
            assert_scratch_task(&api, &scratch).await; // pre-write name assertion
            match ignition_core::actions::eam::eam_task_suspend(&api, &scratch).await {
                Ok(res) => {
                    result = Some(res);
                    break;
                }
                Err(err) if attempt < 7 && is_suspend_trigger_pending(&err) => {
                    eprintln!(
                        "gate step suspend: attempt {attempt} — trigger not registered yet \
                         (captured 500 \"Task could not be suspended\", §1c); retrying in 30 s"
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                }
                Err(err) => panic!("suspend failed (attempt {attempt}): {err}"),
            }
        }
        result.expect("suspend must succeed once the trigger registers (≤ ~3 min)")
    };
    assert_eq!(suspend.task, scratch);
    assert!(suspend.fired, "suspend actually rode the wire");
    assert_eq!(
        suspend.config_suspended,
        Some(true),
        "capture Decision 1: suspend PERSISTS isSuspended=true into the definition"
    );
    eprintln!(
        "gate step suspend: 204 + read-back isSuspended=true (previous state {:?})",
        suspend.previous_state
    );

    // 5. SCHEDULED VOCABULARY: a suspended task leaves scheduled/false
    //    EVENTUALLY, not immediately (capture §2 + the UAT gate run: the
    //    row lingered ~48 s post-suspend, sometimes as a grace row
    //    taskState="Suspended" still listed in scheduled/false). The check
    //    is a deadline-bounded POLL — never a single-shot absence assert
    //    (the UAT gap this closes). Modeled on the resume-reappear loop
    //    below. Only the scratch task's own row is asserted on — every
    //    other row belongs to the rig and is never touched.
    let mut grace_seen = false;
    let mut last_state: Option<String> = None;
    let mut vanished_secs: Option<u64> = None;
    let poll_start = std::time::Instant::now();
    let deadline = poll_start + std::time::Duration::from_secs(90);
    while vanished_secs.is_none() {
        let scheduled = api
            .eam_tasks_scheduled(false)
            .await
            .expect("scheduled/false read must answer on a controller");
        match scheduled.iter().find(|row| row.name == scratch) {
            Some(row) => {
                last_state = Some(row.task_state.clone());
                if row.task_state == "Suspended" {
                    grace_seen = true;
                    eprintln!(
                        "gate step verify: grace-period row: taskState=Suspended still listed in \
                         scheduled/false — transient, UAT rig 2026-09-10"
                    );
                } else {
                    eprintln!(
                        "gate step verify: scratch row still listed in scheduled/false \
                         (taskState={:?}) — polling until it vanishes (capture §2)",
                        row.task_state
                    );
                }
            }
            None => {
                vanished_secs = Some(poll_start.elapsed().as_secs());
                break;
            }
        }
        if std::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }
    let vanished_secs = vanished_secs.unwrap_or_else(|| {
        panic!(
            "suspended scratch task must vanish from scheduled/false within ~90s (capture §2) — \
             grace row seen: {grace_seen}; last observed taskState: {:?}",
            last_state.as_deref().unwrap_or("(row never observed after suspend)")
        )
    });
    eprintln!(
        "gate step verify: suspended task absent from scheduled/false (capture §2) — vanished \
         after ~{vanished_secs}s (grace row seen: {grace_seen})"
    );

    // 6. RESUME — the action layer (fires unconditionally per §1b honesty;
    //    here the task IS suspended so it is the real inverse, §1d).
    assert_scratch_task(&api, &scratch).await; // pre-write name assertion
    let resume = ignition_core::actions::eam::eam_task_resume(&api, &scratch)
        .await
        .expect("resume of the suspended scratch task must succeed (capture §1d 204)");
    assert_eq!(resume.task, scratch);
    assert!(resume.fired, "resume actually rode the wire");
    assert_eq!(
        resume.config_suspended,
        Some(false),
        "capture Decision 1: resume syncs isSuspended back to false"
    );
    // … and the task is schedulable again (the row reappears — §2 observed
    // it same-second; retry tolerance for load).
    let mut back = false;
    for attempt in 1..=4 {
        let scheduled = api
            .eam_tasks_scheduled(false)
            .await
            .expect("scheduled/false read must answer");
        if scheduled.iter().any(|row| row.name == scratch) {
            back = true;
            break;
        }
        eprintln!("gate step resume: task not back in scheduled/false yet (attempt {attempt})");
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }
    assert!(
        back,
        "resumed scratch task must reappear in scheduled/false (capture §2)"
    );
    eprintln!("gate step resume: 204 + read-back isSuspended=false; task back in scheduled/false");

    // 7. DELETE — the action layer (find-derived signature, ?collection=core,
    //    NO confirm by default per Decision 3 — the lone-resource shape §3b).
    assert_scratch_task(&api, &scratch).await; // pre-write name assertion
    let deleted = ignition_core::actions::eam::eam_task_delete(&api, &scratch)
        .await
        .expect("delete of the lone scratch task must succeed (capture §3b)");
    assert_eq!(deleted.task, scratch);
    assert!(deleted.deleted, "delete outcome success:true (capture §3b)");
    assert!(
        !deleted.changes.is_empty(),
        "changes[] carries the deleted resource's final signature (capture §9)"
    );

    // 8. POST-DELETE PROOF: find must answer the config-resource 404
    //    (not_found, exit 6) — the scratch task is GONE, cleanup proven.
    let err = api
        .eam_task_find(&scratch)
        .await
        .expect_err("post-delete find must answer not_found");
    assert!(
        matches!(err, CoreError::NotFound { .. }),
        "expected not_found after delete, got: {err}"
    );
    eprintln!("gate step delete: deleted=true; post-delete find = not_found — cleanup proven");

    // The test body completed the delete itself — disarm the Drop guard.
    guard.disarm();
}

// ---------------------------------------------------------------------------
// 10-06 addition: the UAT-gate failure shape, replayed against wiremock —
// NOT ignored (this is a mechanism test; no live rig needed). A guard
// dropped during unwind INSIDE the #[tokio::test] runtime must still fire
// its remote cleanup: the old Drop path block_on'd from inside the runtime
// ("Cannot start a runtime from within a runtime"), catch_unwind swallowed
// the panic, and the scratch task survived. The fix routes cleanup through
// spawn_blocking (joined at runtime drop). This test proves the fix at the
// REQUEST level: after catching the unwind, the cleanup find + delete must
// ARRIVE at the mock.
// ---------------------------------------------------------------------------

/// The scratch name rides the gate's hyphenated style — which the ONE
/// locked per-segment encoder over-encodes to `%2D` (eam_contract's
/// discipline pin; the server decodes before matching).
const UNWIND_SCRATCH: &str = "ign-live-scratch-unwind";
const UNWIND_SIGNATURE: &str = "sigunwindproof";
const UNWIND_FIND_PATH: &str = "/data/api/v1/resources/find/com.inductiveautomation.eam/eam-tasks/ign%2Dlive%2Dscratch%2Dunwind";
const UNWIND_DELETE_PATH: &str =
    "/data/api/v1/resources/com.inductiveautomation.eam/eam-tasks/ign%2Dlive%2Dscratch%2Dunwind/sigunwindproof";

#[tokio::test]
async fn guard_drop_during_unwind_inside_runtime_still_cleans_up() {
    use wiremock::matchers::{method, path, query_param};

    let server = wiremock::MockServer::start().await;

    // (1) The find route (200 record with a signature — the find fixture
    //     shape from the eam contract tests) and the delete route
    //     (200 success:true — the §3b lone-resource shape).
    wiremock::Mock::given(method("GET"))
        .and(path(UNWIND_FIND_PATH))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
            serde_json::json!({
                "name": UNWIND_SCRATCH,
                "collection": "eam-tasks",
                "type": "com.inductiveautomation.eam",
                "config": {"profile": {"type": "eam_backup", "scheduleMode": "OnDemand"}},
                "signature": UNWIND_SIGNATURE,
                "scheduledTaskState": {
                    "currentState": "IDLE",
                    "details": {"owner": "eam", "nextScheduled": null}
                }
            }),
        ))
        .mount(&server)
        .await;
    wiremock::Mock::given(method("DELETE"))
        .and(path(UNWIND_DELETE_PATH))
        .and(query_param("collection", "core"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true,
                "changes": [{"name": UNWIND_SCRATCH,
                             "type": "com.inductiveautomation.eam/eam-tasks",
                             "collection": "core",
                             "newSignature": "postunwindproofsignature"}],
                "problem": null,
                "references": []
            })),
        )
        .mount(&server)
        .await;

    // (2) The guard pointed at the wiremock URL.
    let guard = ScratchTaskGuard {
        url: server.uri(),
        token: "unwind:proof".to_string(),
        name: UNWIND_SCRATCH.to_string(),
        disarmed: false,
    };

    // (3) The failure shape: a scope OWNING the guard panics — the guard
    //     drops mid-unwind INSIDE the test runtime (exactly the UAT gate
    //     run's failure shape).
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _scope = guard;
        panic!("mid-test failure shape from the UAT gate run");
    }));
    assert!(result.is_err(), "the panic must actually fire");

    // (4) Request-level proof that the nested-runtime panic is gone: the
    //     cleanup find + delete requests ARRIVED at the mock (poll — the
    //     blocking-pool cleanup thread needs a beat; the test runtime is
    //     still alive so the pool runs).
    let mut arrived = false;
    for _ in 0..10 {
        if let Some(requests) = server.received_requests().await {
            let find = requests
                .iter()
                .any(|r| r.method.as_str() == "GET" && r.url.path() == UNWIND_FIND_PATH);
            let delete = requests
                .iter()
                .any(|r| r.method.as_str() == "DELETE" && r.url.path() == UNWIND_DELETE_PATH);
            if find && delete {
                arrived = true;
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    assert!(
        arrived,
        "cleanup find + delete must hit the mock after a mid-runtime unwind drop — \
         the old nested-runtime panic would leave zero requests"
    );
}
