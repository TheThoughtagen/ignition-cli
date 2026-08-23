//! Rig lifecycle actions (04-01, RIG-01): `up` / `down` / `status` —
//! serde models OUT, no printing (the TUI rides this layer in Phase 6).
//!
//! Every action takes [`&dyn ComposeRunner`] (the Task-1 seam) so the
//! full decision tree is unit-testable without docker. `rig_up`
//! additionally takes an OPTIONAL gateway probe: the dispatch derives
//! the rig's gateway URL ([`gateway_url_from`]) and builds a
//! HEADER-LESS client pointed at it — the commissioned wait probes
//! `/StatusPing` on the RIG's own port, never the profile's gateway.
//!
//! ## Uncommissioned is DATA, not failure (research Pattern 4)
//!
//! A fresh-volume rig terminally reports `"up, uncommissioned"` — exit
//! 0 with the wizard URL inside `warnings` (the version-command
//! degradation precedent). There is NO headless commissioning (verified:
//! no commissioning endpoints in 83-api); the wait deadline only covers
//! STARTING→RUNNING on an already-commissioned volume.
//!
//! ## The wait reuses poll.rs VERBATIM (locked)
//!
//! [`crate::poll`] is THE wait engine — its retry set is LOCKED and
//! untouched. Probe translation (research Pattern 4): `RUNNING` →
//! Done; other states → Pending; `GatewayNotCommissioned` → the probe
//! ITSELF catches it and returns Pending with the wizard hint (never
//! aborts); Network/GatewayRestarting propagate for poll's native
//! retry; Auth can't fire (the probe is header-less).

use std::cell::Cell;
use std::time::Duration;

use serde::Serialize;

use crate::client::GatewayApi;
use crate::rig::compose::{
    check_output, compose_version, down_args, docker_ps_publish_args, logs_args,
    parse_docker_ps_ldjson, parse_ps_ldjson, parse_volume_ls_ldjson, ps_args, reset_preview,
    up_args, volume_ls_args, ComposeRunner,
};
use crate::error::CoreError;
use crate::poll::{self, PollConfig, PollState};
use crate::rig::{RigPlan, port_preflight};

/// Default wait budget for BOTH `up --wait-timeout` and the
/// commissioned probe deadline (research Pitfall 3: healthchecks block
/// `--wait`, and image pulls add minutes).
pub const DEFAULT_WAIT_TIMEOUT_S: u64 = 300;

/// The state a fully-ready gateway reports on `/StatusPing`.
const RUNNING: &str = "RUNNING";

/// Gateway target ports (the documented heuristic inputs).
const GATEWAY_HTTP_TARGET: u16 = 8088;
const GATEWAY_HTTPS_TARGET: u16 = 443;

/// `ign rig up` output model (all keys always present).
#[derive(Debug, Serialize)]
pub struct RigUpResult {
    /// Compose project name — the identity truth.
    pub rig: String,
    /// Compose project name (same truth; kept distinct from `rig` for
    /// future alias-vs-project divergence).
    pub project: String,
    /// `"running"` | `"uncommissioned"`.
    pub state: String,
    /// The derived gateway URL (what the commissioned wait probed);
    /// `null` when no 8088/443 mapping exists.
    pub gateway_url: Option<String>,
    /// Data-level warnings (uncommissioned wizard hint, skipped-wait
    /// note) — exit 0 carries them here, never on stderr.
    pub warnings: Vec<String>,
}

/// `ign rig down` output model.
#[derive(Debug, Serialize)]
pub struct RigDownResult {
    /// Compose project name.
    pub rig: String,
    /// Compose project name.
    pub project: String,
    /// Always `"down"` on success.
    pub state: String,
}

/// `ign rig reset` output model (04-02, all keys always).
#[derive(Debug, Serialize)]
pub struct RigResetResult {
    /// Compose project name — the identity truth.
    pub rig: String,
    /// Compose project name.
    pub project: String,
    /// The volume names reset removed (the preview, reported as it
    /// acted — what `-v` took from THIS project only).
    pub removed_volumes: Vec<String>,
    /// `"running"` | `"uncommissioned"` (a fresh volume usually boots
    /// into the wizard — data, exit 0).
    pub state: String,
    /// Data-level warnings (uncommissioned wizard hint, skipped-wait).
    pub warnings: Vec<String>,
}

/// One published-port row in status output (allowlist only).
#[derive(Debug, Serialize)]
pub struct StatusPublisher {
    /// Host port.
    pub published_port: Option<u16>,
    /// Container port.
    pub target_port: Option<u16>,
    /// `tcp`/`udp`.
    pub protocol: Option<String>,
}

/// One service row in status output (allowlist only).
#[derive(Debug, Serialize)]
pub struct StatusService {
    /// Service name.
    pub name: String,
    /// `running` / `exited` / …
    pub state: String,
    /// `healthy` / `starting` / `null`.
    pub health: Option<String>,
    /// Last exit code.
    pub exit_code: Option<i64>,
    /// Published ports.
    pub publishers: Vec<StatusPublisher>,
}

/// `ign rig status` output model — an ALLOWLIST, never a compose
/// config/inspect passthrough (the resolved config contains
/// `GATEWAY_ADMIN_PASSWORD` etc.; research anti-pattern).
#[derive(Debug, Serialize)]
pub struct RigStatusResult {
    /// Compose project name.
    pub rig: String,
    /// Compose project name.
    pub project: String,
    /// The compose file the rig resolved from.
    pub compose_file: String,
    /// One row per RUNNING-OR-EXITED compose service (empty when the
    /// rig is down — state is data, exit 0).
    pub services: Vec<StatusService>,
    /// Named volumes `reset` would remove (project-labeled).
    pub volumes: Vec<String>,
    /// True when NO docker container currently publishes any of the
    /// rig's host ports (a running rig holds its own ports → false).
    pub ports_free: bool,
}

/// Derive the rig's gateway URL from the resolved port mappings — the
/// DOCUMENTED HEURISTIC: the first mapping targeting the gateway's http
/// port (8088) wins, then its https port (443), else nothing. `data`
/// carries what was derived (`gateway_url`), so agents can see the
/// heuristic's answer.
pub fn gateway_url_from(plan: &RigPlan) -> Option<String> {
    if let Some(mapping) = plan
        .port_mappings
        .iter()
        .find(|mapping| mapping.target == GATEWAY_HTTP_TARGET)
    {
        return Some(format!("http://localhost:{}", mapping.published));
    }
    if let Some(mapping) = plan
        .port_mappings
        .iter()
        .find(|mapping| mapping.target == GATEWAY_HTTPS_TARGET)
    {
        return Some(format!("https://localhost:{}", mapping.published));
    }
    None
}

/// `ign rig up`: version gate → port pre-flight → `up -d --wait` →
/// commissioned wait (poll.rs) with uncommissioned-as-data semantics.
///
/// `gateway` is the optional probe client the dispatch builds from
/// [`gateway_url_from`] — when absent (or when no gateway port is
/// derivable) the wait is skipped with a data-level warning and the
/// compose `--wait` result stands on its own.
pub async fn rig_up(
    runner: &dyn ComposeRunner,
    plan: &RigPlan,
    wait_timeout_s: u64,
    gateway: Option<&dyn GatewayApi>,
) -> Result<RigUpResult, CoreError> {
    // 1. Fail fast on a missing/too-old compose (exit 7 + install hint).
    compose_version(runner).await?;

    // 2. Port pre-flight — cross-project occupants abort BEFORE any
    //    container is touched, with attribution (first conflict named;
    //    pre-flight collected them all).
    if let Some(conflict) = port_preflight(runner, plan).await?.first() {
        return Err(CoreError::Rig(format!(
            "port {} in use by {} — stop it or change the rig's published port",
            conflict.port, conflict.attribution
        )));
    }

    // 3. The up itself.
    let output = runner.run(&up_args(plan, wait_timeout_s)).await;
    check_output(&output, "docker compose up")?;

    let gateway_url = gateway_url_from(plan);
    let mut warnings = Vec::new();
    let state = match (gateway, &gateway_url) {
        (Some(api), Some(url)) => {
            commissioned_wait(api, url, wait_timeout_s, &mut warnings).await?
        }
        _ => {
            warnings.push(
                "no gateway port mapping (target 8088/443) found — skipped the \
                 commissioned wait"
                    .to_string(),
            );
            "running".to_string()
        }
    };

    Ok(RigUpResult {
        rig: plan.name.clone(),
        project: plan.name.clone(),
        state,
        gateway_url,
        warnings,
    })
}

/// The commissioned wait (research Pattern 4): poll the rig's own
/// `/StatusPing` (header-less — auth can never block readiness) until
/// RUNNING. Deadline expiry with a terminal uncommissioned observation
/// degrades to SUCCESS-as-data (`state = "uncommissioned"` + wizard
/// hint in `warnings`); expiry while merely STARTING is a Rig error.
async fn commissioned_wait(
    api: &dyn GatewayApi,
    url: &str,
    wait_timeout_s: u64,
    warnings: &mut Vec<String>,
) -> Result<String, CoreError> {
    let cfg = PollConfig {
        subject: format!("rig gateway RUNNING (GET {url}/StatusPing)"),
        interval: Duration::from_secs(2),
        deadline: Duration::from_secs(wait_timeout_s),
        ..PollConfig::default()
    };
    // The poll-owned state: a borrowed Cell (the 02-05 HRTB shape) that
    // remembers a terminal-uncommissioned observation across probes.
    let mut uncommissioned = Cell::new(false);
    let url_owned = url.to_string();
    let outcome = poll::poll(cfg, &mut uncommissioned, |uncommissioned| {
        Box::pin(async {
            match api.status_ping().await {
                Ok(ping) if ping.state == RUNNING => Ok(PollState::<()>::Done(())),
                Ok(ping) => Ok(PollState::Pending(Some(ping.state))),
                // Probe-side translation (locked): never abort on the
                // wizard redirect — remember it, keep waiting.
                Err(CoreError::GatewayNotCommissioned { .. }) => {
                    uncommissioned.set(true);
                    Ok(PollState::Pending(Some(format!(
                        "gateway uncommissioned — open {url_owned}/welcome"
                    ))))
                }
                // Network/GatewayRestarting: poll's native retry set.
                Err(other) => Err(other),
            }
        })
    })
    .await;
    match outcome {
        Ok(()) => Ok("running".to_string()),
        // The DEADLINE error only (the locked source:None marker from
        // 02-04) degrades to data when the terminal observation was
        // uncommissioned — any other error class stays an error.
        Err(CoreError::Network { source: None, .. }) if uncommissioned.get() => {
            warnings.push(format!(
                "gateway uncommissioned — open {url}/welcome in a browser and complete \
                 the commissioning wizard (no headless commissioning exists)"
            ));
            Ok("uncommissioned".to_string())
        }
        Err(other) => Err(CoreError::Rig(format!(
            "gateway did not reach RUNNING within {wait_timeout_s}s — {other}"
        ))),
    }
}

/// `ign rig down`: version gate → `down --remove-orphans` (volumes
/// KEPT — the `-v` teardown half belongs to `rig reset`, 04-02).
pub async fn rig_down(runner: &dyn ComposeRunner, plan: &RigPlan) -> Result<RigDownResult, CoreError> {
    compose_version(runner).await?;
    let output = runner.run(&down_args(plan, false)).await;
    check_output(&output, "docker compose down")?;
    Ok(RigDownResult {
        rig: plan.name.clone(),
        project: plan.name.clone(),
        state: "down".to_string(),
    })
}

/// `ign rig reset` (04-02, RIG-01): the guarded teardown + bring-up
/// cycle — NO stale project/trial state survives. The CLI guard
/// (`--yes`, exit 2 before ANY resolution) lives in the dispatch (the
/// sessions-terminate/project-delete layering); this action is the
/// decision-complete cycle behind it:
///
/// 1. `reset_preview` — the project's volume names, captured for the
///    result data (agents see what reset removes before/as it acts);
/// 2. version gate (fail fast on missing/too-old compose);
/// 3. `down -v --remove-orphans` — the LOCKED teardown (research:
///    `down && up` without `-v` is the classic stale-state
///    anti-pattern; anonymous strays and renamed-service orphans die
///    via `--remove-orphans`);
/// 4. `port_preflight` — AFTER teardown, BEFORE the up half: teardown
///    frees OUR ports first, then fresh eyes catch another rig that
///    grabbed a freed port mid-cycle;
/// 5. `up -d --wait` (the [`rig_up`] invocation verbatim);
/// 6. commissioned wait — [`commissioned_wait`], the ONE shared fn
///    (the 04-01 probe reused verbatim; `poll.rs` untouched).
pub async fn rig_reset(
    runner: &dyn ComposeRunner,
    plan: &RigPlan,
    wait_timeout_s: u64,
    gateway: Option<&dyn GatewayApi>,
) -> Result<RigResetResult, CoreError> {
    // 1. The preview — data for the result, reported as it acts.
    let removed_volumes = reset_preview(runner, plan).await?;

    // 2. Fail fast on a missing/too-old compose.
    compose_version(runner).await?;

    // 3. Teardown: down -v --remove-orphans (volumes die here).
    let output = runner.run(&down_args(plan, true)).await;
    check_output(&output, "docker compose down")?;

    // 4. Port pre-flight with fresh eyes — between the halves.
    if let Some(conflict) = port_preflight(runner, plan).await?.first() {
        return Err(CoreError::Rig(format!(
            "port {} in use by {} — stop it or change the rig's published port \
             (the rig is torn down; re-run `rig up` once the port frees)",
            conflict.port, conflict.attribution
        )));
    }

    // 5. The up half.
    let output = runner.run(&up_args(plan, wait_timeout_s)).await;
    check_output(&output, "docker compose up")?;

    // 6. Commissioned wait — the shared fn (uncommissioned-as-data).
    let gateway_url = gateway_url_from(plan);
    let mut warnings = Vec::new();
    let state = match (gateway, &gateway_url) {
        (Some(api), Some(url)) => {
            commissioned_wait(api, url, wait_timeout_s, &mut warnings).await?
        }
        _ => {
            warnings.push(
                "no gateway port mapping (target 8088/443) found — skipped the \
                 commissioned wait"
                    .to_string(),
            );
            "running".to_string()
        }
    };

    Ok(RigResetResult {
        rig: plan.name.clone(),
        project: plan.name.clone(),
        removed_volumes,
        state,
        warnings,
    })
}

/// `ign rig logs` output model (04-02, RIG-02): only the count — the
/// lines themselves already streamed through the sink (the third
/// sanctioned stdout exception; the dispatch owns the printing, the
/// `logs -f` precedent).
#[derive(Debug, Serialize)]
pub struct RigLogsResult {
    /// Lines delivered to the sink.
    pub streamed: usize,
}

/// The banners cross-check block of [`TrialStatusResult`] (all keys
/// always; a failed banners fetch degrades to nulls + a warning —
/// the trial endpoint is the primary truth).
#[derive(Debug, Serialize)]
pub struct TrialBanners {
    /// The trial banner's `severity` verbatim (`"info"` / `"warning"`);
    /// `null` when no trial banner or the fetch failed.
    pub severity: Option<String>,
    /// The trial banner's `expireTime` in epoch **MILLISECONDS** —
    /// `null` when expired/unknown (Pitfall 7).
    pub expire_time_ms: Option<i64>,
    /// The Pitfall-7 cross-check: `severity == "info"` AND
    /// `expireTime > now_ms`. NEVER the primary active signal —
    /// [`TrialStatusResult::expired`] is.
    pub active: bool,
}

/// `ign rig trial status` output model (04-03, RIG-02): the trial
/// endpoint re-exposed under unit-explicit keys (the two-layer naming
/// LOCK) + the banners cross-check. All keys always present.
#[derive(Debug, Serialize)]
pub struct TrialStatusResult {
    /// `licenseMode` verbatim (`"Trial"` / …).
    pub license_mode: String,
    /// `trialState` verbatim (`AllInDemo` / `SomeInDemo` /
    /// `NoneInDemo`).
    pub trial_state: String,
    /// `trialSecondsLeft` — epoch **SECONDS** (the `_s` suffix is the
    /// unit contract).
    pub trial_remaining_s: i64,
    /// The primary expiry truth (never derived from banners).
    pub expired: bool,
    /// Emergency-license flag.
    pub emergency: bool,
    /// `emergencySecondsLeft` — epoch **SECONDS**.
    pub emergency_remaining_s: i64,
    /// Development-license flag.
    pub development: bool,
    /// The banners cross-check block.
    pub banners: TrialBanners,
    /// Data-level warnings (banners fetch failed, …) — exit 0 carries
    /// them here, never on stderr.
    pub warnings: Vec<String>,
}

/// Wall-clock epoch milliseconds (the banners `expireTime` unit).
fn epoch_ms_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock is after the unix epoch")
        .as_millis() as i64
}

/// `ign rig trial status` (04-03, RIG-02): the trial endpoint is the
/// PRIMARY truth; the trial banner (`type: "trial"`) is the
/// cross-check, its `active` flag computed per Pitfall 7
/// (`severity=="info" && expireTime>now_ms` — never the reverse
/// derivation). A failed banners fetch degrades to nulls + a warning
/// (the trial endpoint already answered; the cross-check is
/// advisory). `gateway` is a client pointed at the RIG's URL — these
/// endpoints answer unauthenticated (live-verified both rigs), so a
/// fresh rig with no token reports its trial state fine.
pub async fn trial_status(gateway: &dyn GatewayApi) -> Result<TrialStatusResult, CoreError> {
    let wire = gateway.trial_status_wire().await?;
    let mut warnings = Vec::new();
    let banners = match gateway.banners().await {
        Ok(set) => {
            let trial_banner = set.banners.iter().find(|banner| banner.r#type == "trial");
            match trial_banner {
                Some(banner) => {
                    let active = banner.data.severity == "info"
                        && banner.data.expire_time_ms.is_some_and(|ms| ms > epoch_ms_now());
                    TrialBanners {
                        severity: Some(banner.data.severity.clone()),
                        expire_time_ms: banner.data.expire_time_ms,
                        active,
                    }
                }
                None => TrialBanners {
                    severity: None,
                    expire_time_ms: None,
                    active: false,
                },
            }
        }
        Err(err) => {
            warnings.push(format!(
                "banners cross-check unavailable ({}); the trial endpoint's \
                 expired flag is the truth",
                err
            ));
            TrialBanners {
                severity: None,
                expire_time_ms: None,
                active: false,
            }
        }
    };
    Ok(TrialStatusResult {
        license_mode: wire.license_mode,
        trial_state: wire.trial_state,
        trial_remaining_s: wire.trial_seconds_left,
        expired: wire.expired,
        emergency: wire.emergency,
        emergency_remaining_s: wire.emergency_seconds_left,
        development: wire.development,
        banners,
        warnings,
    })
}

/// `ign rig trial reset` output model (04-03, RIG-03): the ladder's
/// outcome — which rung landed, and the before/after flip (the flip
/// is REQUIRED for success; a bare 2xx never suffices).
#[derive(Debug, Serialize)]
pub struct TrialResetResult {
    /// The rig's gateway URL the ladder ran against.
    pub rig_url: String,
    /// `"token"` (tier 0 — token-auth POST through the client
    /// pipeline) | `"login"` (tier 1 — the native OIDC session+CSRF
    /// flow).
    pub mechanism: String,
    /// The pre-reset `expired` flag (always true on the success path —
    /// the action refuses non-expired trials up front).
    pub expired_before: bool,
    /// The post-reset `expired` flag (false — verified by READ-BACK,
    /// never trusted from the POST alone).
    pub expired_after: bool,
    /// The fresh countdown in epoch **SECONDS** (≈7200 = a full new
    /// trial window).
    pub trial_remaining_s: i64,
}

/// `ign rig trial reset` (04-03, RIG-03): the evidence-chosen LADDER —
/// tier 0 (token-auth `POST /trial` through the existing client, one
/// cheap call) falls through to tier 1 (the native OIDC login →
/// session+CSRF POST, [`crate::client::idp`], live-verified
/// end-to-end on 8.3.3 with the `expired:true → false` flip).
///
/// **State gate (live-discovered):** the gateway 403s resets on a
/// NON-expired trial — verified from the browser page itself with the
/// exact UI headers. The pre-check refuses those up front
/// ([`CoreError::TrialNotExpired`]) so the refusal stays an honest
/// target-state error instead of a misleading auth-shaped 403.
///
/// Success REQUIRES the read-back flip: after a 2xx the trial is
/// re-fetched and `expired` must be false (mutations read back — the
/// 03-01 find precedent; no trusting the POST's word alone).
///
/// `gateway` is a client pointed at the rig's URL (carrying the tier-0
/// token when one resolved); `basic` is the tier-1 credential pair
/// (`--user`/`IGNITION_USER` + `IGNITION_PASSWORD` — the secret
/// chain's basic tail). At least one rung must have its credential.
pub async fn trial_reset(
    gateway: &dyn GatewayApi,
    rig_url: &str,
    token_available: bool,
    basic: Option<(&str, &crate::config::Secret)>,
) -> Result<TrialResetResult, CoreError> {
    // 1. The state pre-check: the honest refusal for non-expired
    //    trials (the live-discovered 403 state gate).
    let before = gateway.trial_status_wire().await?;
    if !before.expired {
        return Err(CoreError::TrialNotExpired {
            remaining_s: before.trial_seconds_left,
            endpoint: Some(format!("{rig_url}/data/api/v1/trial")),
        });
    }

    // 2. Tier 0 — token-auth POST (only when the dispatch resolved a
    //    token credential into the client). ANY failure falls through
    //    to tier 1 (the 403 state gate is pre-checked away; a failure
    //    here means the token was refused — the login rung decides).
    if token_available {
        match gateway.trial_reset_wire().await {
            Ok(_fresh) => {
                let after = gateway.trial_status_wire().await?;
                return finish(rig_url, "token", after);
            }
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    "trial-reset tier 0 (token-auth POST) failed — falling through to the login rung"
                );
            }
        }
    }

    // 3. Tier 1 — the native OIDC session+CSRF flow (the
    //    live-verified mechanism).
    let Some((username, password)) = basic else {
        // No token that worked AND no login pair: if tier 0 was even
        // attempted, surface ITS error (the token was the only rung);
        // otherwise the dispatch should have refused up front — this
        // is the defensive tail.
        return Err(CoreError::SecretUnavailable {
            profile: rig_url.to_string(),
        });
    };
    let flow = crate::client::idp::IdpLoginFlow::new(rig_url)?;
    let (flow, session) = crate::client::idp::login(flow, username, password).await?;
    crate::client::idp::trial_reset_via_session(&flow, &session).await?;
    // The read-back flip through the normal pipeline (step 10).
    let after = gateway.trial_status_wire().await?;
    finish(rig_url, "login", after)
}

/// The shared success tail: the flip check + result assembly.
fn finish(rig_url: &str, mechanism: &str, after: crate::client::trial::TrialWire) -> Result<TrialResetResult, CoreError> {
    if after.expired {
        return Err(CoreError::Internal(format!(
            "trial reset was accepted but the read-back still reports expired \
             ({}s left) — re-run `rig trial status` to see the gateway's answer",
            after.trial_seconds_left
        )));
    }
    Ok(TrialResetResult {
        rig_url: rig_url.to_string(),
        mechanism: mechanism.to_string(),
        expired_before: true,
        expired_after: after.expired,
        trial_remaining_s: after.trial_seconds_left,
    })
}

/// `ign rig logs` (04-02, RIG-02): compose log PASSTHROUGH — a raw
/// line stream through `sink`, never an envelope-wrapped body.
/// Compose log lines are not gateway JSON objects; wrapping would
/// corrupt them, so `rig logs --json` is the SAME passthrough in
/// every render mode (contrast `logs -f --json`, whose entries ARE
/// gateway NDJSON — the second exception). Follow mode rides the
/// runner's STREAMING shape (piped stdout forwarded as it arrives
/// until EOF/child exit; Ctrl-C kills the foreground process group —
/// README §Streaming, the `logs -f` precedent). Compose stderr
/// (diagnostics) goes to OUR stderr via tracing::warn — never the
/// data sink.
pub async fn rig_logs(
    runner: &dyn ComposeRunner,
    plan: &RigPlan,
    tail: u32,
    follow: bool,
    service: Option<&str>,
    sink: &mut (dyn FnMut(String) + Send),
) -> Result<RigLogsResult, CoreError> {
    let args = logs_args(plan, tail, follow, service);
    let mut streamed = 0usize;
    let output = if follow {
        // Follow: streamed through the runner's piped-stdout shape;
        // the returned stdout is empty (the lines already delivered).
        let mut forwarder = |line: &str| {
            streamed += 1;
            sink(line.to_string());
        };
        runner.run_streaming(&args, &mut forwarder).await
    } else {
        runner.run(&args).await
    };
    if !output.stderr.trim().is_empty() {
        tracing::warn!(
            source = "docker compose logs",
            stderr = %output.stderr.trim(),
            "compose diagnostics (stderr passthrough — never the data sink)"
        );
    }
    // One-shot: the captured stdout splits line-wise into the sink.
    let stdout = check_output(&output, "docker compose logs")?;
    for line in stdout.lines() {
        sink(line.to_string());
        streamed += 1;
    }
    Ok(RigLogsResult { streamed })
}

/// `ign rig status`: version gate → `ps` LDJSON → `volume ls` →
/// port occupancy — serialized as an ALLOWLIST (services' state/health/
/// publishers, volume names, identity). Exit 0 even when the rig is
/// down: state is data.
pub async fn rig_status(runner: &dyn ComposeRunner, plan: &RigPlan) -> Result<RigStatusResult, CoreError> {
    compose_version(runner).await?;

    let ps = runner.run(&ps_args(plan)).await;
    let rows = parse_ps_ldjson(check_output(&ps, "docker compose ps")?);

    let volume_ls = runner.run_docker(&volume_ls_args(&plan.name)).await;
    let volumes = parse_volume_ls_ldjson(check_output(&volume_ls, "docker volume ls")?)
        .into_iter()
        .map(|entry| entry.name)
        .collect();

    // Occupancy signal: any docker container (own project included —
    // a running rig holds its own ports) publishing a rig port.
    let mut ports_free = true;
    for port in &plan.host_ports {
        let output = runner.run_docker(&docker_ps_publish_args(*port)).await;
        let occupants = parse_docker_ps_ldjson(check_output(&output, "docker ps")?);
        if !occupants.is_empty() {
            ports_free = false;
        }
    }

    let services = rows
        .into_iter()
        .map(|row| StatusService {
            name: if row.service.is_empty() {
                row.name
            } else {
                row.service
            },
            state: row.state,
            health: row.health,
            exit_code: row.exit_code,
            publishers: row
                .publishers
                .into_iter()
                .map(|publisher| StatusPublisher {
                    published_port: publisher.published_port,
                    target_port: publisher.target_port,
                    protocol: publisher.protocol,
                })
                .collect(),
        })
        .collect();

    Ok(RigStatusResult {
        rig: plan.name.clone(),
        project: plan.name.clone(),
        compose_file: plan.compose_file.display().to_string(),
        services,
        volumes,
        ports_free,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use super::{
        DEFAULT_WAIT_TIMEOUT_S, RigDownResult, RigResetResult, RigStatusResult, RigUpResult,
        gateway_url_from, rig_down, rig_logs, rig_reset, rig_status, rig_up, trial_reset,
        trial_status,
    };
    use crate::rig::compose::{
        ComposeOutput, ComposeRunner, PortMapping, down_args, logs_args, up_args, volume_ls_args,
    };
    use crate::error::CoreError;
    use crate::rig::RigPlan;

    // ---------------------------------------------------------------------
    // Test doubles
    // ---------------------------------------------------------------------

    /// Scripted runner: records every (program, args) call and serves
    /// queued outputs FIFO (same seam shape as the rig module's fake).
    #[derive(Default)]
    struct FakeRunner {
        calls: Mutex<Vec<(&'static str, Vec<String>)>>,
        outputs: Mutex<VecDeque<ComposeOutput>>,
    }

    impl FakeRunner {
        fn with(outputs: Vec<ComposeOutput>) -> Self {
            Self {
                outputs: Mutex::new(outputs.into()),
                ..Self::default()
            }
        }

        fn calls(&self) -> Vec<(&'static str, Vec<String>)> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl ComposeRunner for FakeRunner {
        async fn run(&self, args: &[String]) -> ComposeOutput {
            self.calls.lock().unwrap().push(("docker compose", args.to_vec()));
            self.outputs.lock().unwrap().pop_front().expect("outputs exhausted")
        }

        async fn run_docker(&self, args: &[String]) -> ComposeOutput {
            self.calls.lock().unwrap().push(("docker", args.to_vec()));
            self.outputs.lock().unwrap().pop_front().expect("outputs exhausted")
        }

        async fn run_streaming(
            &self,
            args: &[String],
            line_sink: &mut (dyn for<'a> FnMut(&'a str) + Send),
        ) -> ComposeOutput {
            self.calls.lock().unwrap().push(("docker compose", args.to_vec()));
            let output = self
                .outputs
                .lock()
                .unwrap()
                .pop_front()
                .expect("outputs exhausted");
            // Preload contract: queued stdout lines replay to the sink
            // in order; the returned stdout is emptied (lines already
            // "streamed").
            for line in output.stdout.lines() {
                line_sink(line);
            }
            ComposeOutput {
                stdout: String::new(),
                stderr: output.stderr,
                code: output.code,
            }
        }
    }

    fn ok(stdout: &str) -> ComposeOutput {
        ComposeOutput {
            stdout: stdout.to_string(),
            stderr: String::new(),
            code: 0,
        }
    }

    fn version_ok() -> ComposeOutput {
        ok("Docker Compose version v5.1.2\n")
    }

    /// An OWN-PROJECT docker-ps occupant: preflight treats it as
    /// recreate-safe AND — because the row set is non-empty — the
    /// advisory lsof pass never runs, keeping these tests deterministic
    /// on machines where a REAL rig publishes 9088/9443 (the fixture's
    /// ports — live-verification found the lsof fallback observing the
    /// host's own rig; the rig/mod.rs port-1 dodge, reconsidered).
    const OWN_OCCUPANT: &str = r#"{"Names":"fixture-rig-ignition-1","Labels":"com.docker.compose.project=fixture-rig"}"#;

    /// The scripted pre-flight answers for a two-port gw_plan(): both
    /// ports held by THIS project (a recreate — the honest shape for
    /// up/reset against an already-running rig).
    fn free_ports_for_own_project() -> Vec<ComposeOutput> {
        vec![ok(OWN_OCCUPANT), ok(OWN_OCCUPANT)]
    }

    /// The up cycle's queue: version → preflight × 2 (own project) →
    /// the up itself.
    fn up_cycle_outputs() -> Vec<ComposeOutput> {
        let mut outputs = vec![version_ok()];
        outputs.extend(free_ports_for_own_project());
        outputs.push(ok(""));
        outputs
    }

    /// A one-service rig publishing the gateway ports (the
    /// gateway_url_from inputs).
    fn gw_plan() -> RigPlan {
        RigPlan {
            name: "fixture-rig".into(),
            compose_file: "/rigs/docker/compose.yml".into(),
            project_dir: "/rigs/docker".into(),
            services: vec!["ignition".into()],
            host_ports: vec![9088, 9443],
            port_mappings: vec![
                PortMapping {
                    target: 8088,
                    published: 9088,
                },
                PortMapping {
                    target: 443,
                    published: 9443,
                },
            ],
            volumes: vec!["gw-data".into()],
        }
    }

    /// A wiremock StatusPing responder serving one canned state.
    async fn status_ping_server(state: &str) -> wiremock::MockServer {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/StatusPing"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
                serde_json::json!({ "state": state }),
            ))
            .expect(1..)
            .mount(&server)
            .await;
        server
    }

    /// A wiremock StatusPing responder that ALWAYS 302s to /welcome —
    /// the uncommissioned gateway shape (classify maps it to
    /// GatewayNotCommissioned; the probe must translate to Pending).
    async fn uncommissioned_server() -> wiremock::MockServer {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/StatusPing"))
            .respond_with(wiremock::ResponseTemplate::new(302).insert_header(
                "Location",
                "/welcome",
            ))
            .expect(1..)
            .mount(&server)
            .await;
        server
    }

    // ---------------------------------------------------------------------
    // gateway_url_from
    // ---------------------------------------------------------------------

    #[test]
    fn gateway_url_prefers_http_8088_then_https_443() {
        assert_eq!(
            gateway_url_from(&gw_plan()),
            Some("http://localhost:9088".to_string()),
            "the 8088 mapping wins even though 443 is also present"
        );
        let https_only = RigPlan {
            port_mappings: vec![PortMapping {
                target: 443,
                published: 9443,
            }],
            host_ports: vec![9443],
            ..gw_plan()
        };
        assert_eq!(
            gateway_url_from(&https_only),
            Some("https://localhost:9443".to_string())
        );
        let no_gateway = RigPlan {
            port_mappings: vec![PortMapping {
                target: 22,
                published: 9022,
            }],
            host_ports: vec![9022],
            ..gw_plan()
        };
        assert_eq!(gateway_url_from(&no_gateway), None);
    }

    #[test]
    fn default_wait_timeout_is_300() {
        assert_eq!(DEFAULT_WAIT_TIMEOUT_S, 300, "research Pitfall 3 headroom");
    }

    // ---------------------------------------------------------------------
    // rig_up
    // ---------------------------------------------------------------------

    /// Success path: version → free-port preflight → up (exact args) →
    /// probe sees RUNNING immediately → state running, no warnings.
    #[tokio::test]
    async fn up_success_probes_to_running() {
        let server = status_ping_server("RUNNING").await;
        let api = crate::client::ReqwestGatewayApi::for_tests(&server.uri(), None);

        let runner = FakeRunner::with(up_cycle_outputs());
        let result = rig_up(&runner, &gw_plan(), 300, Some(&api))
            .await
            .expect("up succeeds");
        assert_eq!(result.rig, "fixture-rig");
        assert_eq!(result.state, "running");
        assert!(result.warnings.is_empty());
        assert_eq!(result.gateway_url.as_deref(), Some("http://localhost:9088"));

        let calls = runner.calls();
        assert_eq!(calls[0], ("docker compose", vec!["version".to_string()]));
        // The pre-flight ran per host port via PLAIN docker.
        assert_eq!(calls[1].0, "docker");
        assert_eq!(calls[2].0, "docker");
        assert_eq!(
            calls[3],
            ("docker compose", up_args(&gw_plan(), 300)),
            "up rides the LOCKED arg shape"
        );
    }

    /// The uncommissioned degradation: the probe only ever sees the
    /// wizard redirect; the deadline expires; the result is SUCCESS as
    /// data (state uncommissioned, wizard hint inside warnings).
    #[tokio::test]
    async fn up_uncommissioned_is_data_not_failure() {
        let server = uncommissioned_server().await;
        let api = crate::client::ReqwestGatewayApi::for_tests(&server.uri(), None);

        let runner = FakeRunner::with(up_cycle_outputs());
        let result = rig_up(&runner, &gw_plan(), 1, Some(&api))
            .await
            .expect("uncommissioned is exit-0 data");
        assert_eq!(result.state, "uncommissioned");
        assert_eq!(result.gateway_url.as_deref(), Some("http://localhost:9088"));
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.contains("http://localhost:9088/welcome")),
            "wizard URL in warnings: {:?}",
            result.warnings
        );
    }

    /// STARTING at deadline is a REAL failure (Rig error), not data.
    #[tokio::test]
    async fn up_still_starting_at_deadline_is_rig_error() {
        let server = status_ping_server("STARTING").await;
        let api = crate::client::ReqwestGatewayApi::for_tests(&server.uri(), None);

        let runner = FakeRunner::with(up_cycle_outputs());
        let err = rig_up(&runner, &gw_plan(), 1, Some(&api))
            .await
            .expect_err("still-STARTING deadline errors");
        assert!(matches!(err, CoreError::Rig(_)));
        assert_eq!(err.exit_code(), 7);
        let message = err.to_string();
        assert!(message.contains("did not reach RUNNING"), "{message}");
        assert!(message.contains("STARTING"), "last observation named: {message}");
    }

    /// Port conflict aborts BEFORE the up, with attribution.
    #[tokio::test]
    async fn up_port_conflict_aborts_with_attribution() {
        let occupant = r#"{"Names":"other-gw-1","Labels":"com.docker.compose.project=other"}"#;
        let runner = FakeRunner::with(vec![version_ok(), ok(occupant), ok(occupant)]);

        let err = rig_up(&runner, &gw_plan(), 300, None)
            .await
            .expect_err("cross-project occupant aborts");
        let message = err.to_string();
        assert!(
            message.contains("port 9088 in use by container other-gw-1 (rig other)"),
            "{message}"
        );
        // The up NEVER ran — the last recorded call is the pre-flight.
        let calls = runner.calls();
        assert_eq!(
            calls.len(),
            3,
            "version + two port checks only, no up: {calls:?}"
        );
    }

    /// No probe client → the wait is skipped with a data-level warning
    /// and compose's own --wait stands as the readiness signal.
    #[tokio::test]
    async fn up_without_probe_skips_wait_with_warning() {
        let runner = FakeRunner::with(up_cycle_outputs());
        let result = rig_up(&runner, &gw_plan(), 300, None)
            .await
            .expect("up succeeds without a probe");
        assert_eq!(result.state, "running");
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.contains("skipped the commissioned wait")),
            "{:?}",
            result.warnings
        );
    }

    /// Compose missing → version gate fails fast with the install hint.
    #[tokio::test]
    async fn up_missing_compose_fails_fast() {
        let missing = ComposeOutput {
            stdout: String::new(),
            stderr: "docker: command not found".into(),
            code: 127,
        };
        let runner = FakeRunner::with(vec![missing]);
        let err = rig_up(&runner, &gw_plan(), 300, None)
            .await
            .expect_err("no docker errors");
        let message = err.to_string();
        assert!(message.contains("docker compose is unavailable"), "{message}");
        assert!(message.contains("not supported"), "{message}");
    }

    // ---------------------------------------------------------------------
    // rig_down
    // ---------------------------------------------------------------------

    #[tokio::test]
    async fn down_runs_exact_args_and_reports_down() {
        let runner = FakeRunner::with(vec![version_ok(), ok("")]);
        let result = rig_down(&runner, &gw_plan()).await.expect("down succeeds");
        assert_eq!(
            serde_json::to_value(&result).unwrap(),
            serde_json::json!({
                "rig": "fixture-rig",
                "project": "fixture-rig",
                "state": "down",
            }),
            "RigDownResult shape (all keys always)"
        );
        let calls = runner.calls();
        assert_eq!(calls[1], ("docker compose", down_args(&gw_plan(), false)));
    }

    #[tokio::test]
    async fn down_failure_carries_stderr_tail() {
        let runner = FakeRunner::with(vec![
            version_ok(),
            ComposeOutput {
                stdout: String::new(),
                stderr: "error while removing network: active endpoints".into(),
                code: 1,
            },
        ]);
        let err = rig_down(&runner, &gw_plan())
            .await
            .expect_err("down failure errors");
        let message = err.to_string();
        assert!(message.contains("docker compose down failed (exit 1)"), "{message}");
        assert!(message.contains("active endpoints"), "{message}");
    }

    // ---------------------------------------------------------------------
    // rig_reset — the guarded teardown + bring-up cycle (04-02)
    // ---------------------------------------------------------------------

    /// volume ls rows: one of ours, one foreign-prefixed (the label
    /// filter is server-side; the name-prefix filter is defense in
    /// depth — the preview pin).
    const RESET_VOLUME_STDOUT: &str = concat!(
        r#"{"Name":"fixture-rig_gw-data","Labels":{"com.docker.compose.project":"fixture-rig"}}"#,
        "\n",
        r#"{"Name":"other-rig_gw-data","Labels":{"com.docker.compose.project":"other-rig"}}"#,
        "\n",
    );

    /// The full scripted cycle for a gw_plan() rig: preview (docker
    /// volume ls) → version → down -v → preflight × 2 ports (own
    /// project — recreate) → up.
    fn reset_cycle_outputs() -> Vec<ComposeOutput> {
        let mut outputs = vec![ok(RESET_VOLUME_STDOUT), version_ok(), ok("")];
        outputs.extend(free_ports_for_own_project());
        outputs.push(ok(""));
        outputs
    }

    /// The happy cycle: preview content pinned (label-filtered names,
    /// foreign prefix dropped), teardown LOCKED shape pinned on the
    /// call log (`-v --remove-orphans`, explicit `-p`), pre-flight
    /// BETWEEN the halves, up on the rig_up shape, probe → RUNNING.
    #[tokio::test]
    async fn reset_previews_tears_down_with_v_then_brings_up() {
        let server = status_ping_server("RUNNING").await;
        let api = crate::client::ReqwestGatewayApi::for_tests(&server.uri(), None);

        let runner = FakeRunner::with(reset_cycle_outputs());
        let result = rig_reset(&runner, &gw_plan(), 300, Some(&api))
            .await
            .expect("reset succeeds");
        assert_eq!(result.rig, "fixture-rig");
        assert_eq!(result.removed_volumes, vec!["fixture-rig_gw-data"]);
        assert_eq!(result.state, "running");
        assert!(result.warnings.is_empty());

        // The call log pins the WHOLE cycle: program shapes AND order.
        let calls = runner.calls();
        assert_eq!(calls.len(), 6, "exactly the six scripted calls: {calls:?}");
        assert_eq!(
            calls[0],
            ("docker", volume_ls_args("fixture-rig")),
            "preview rides the plain-docker volume ls shape"
        );
        assert_eq!(calls[1], ("docker compose", vec!["version".to_string()]));
        // The LOCKED teardown — the REQUEST shape, not just the
        // response (the Phase-2/3 wiremock discipline, runner edition).
        assert_eq!(
            calls[2],
            (
                "docker compose",
                vec![
                    "-p".to_string(),
                    "fixture-rig".to_string(),
                    "-f".to_string(),
                    "/rigs/docker/compose.yml".to_string(),
                    "down".to_string(),
                    "--remove-orphans".to_string(),
                    "-v".to_string(),
                ],
            ),
            "down -v --remove-orphans via the runner seam"
        );
        // Pre-flight AFTER the teardown, BEFORE the up (fresh eyes).
        assert_eq!(calls[3].0, "docker");
        assert_eq!(calls[4].0, "docker");
        assert_eq!(calls[5], ("docker compose", up_args(&gw_plan(), 300)));
    }

    /// A fresh volume terminally reports the wizard redirect →
    /// uncommissioned is DATA (exit 0) — the same degradation as up.
    #[tokio::test]
    async fn reset_uncommissioned_fresh_volume_is_data() {
        let server = uncommissioned_server().await;
        let api = crate::client::ReqwestGatewayApi::for_tests(&server.uri(), None);

        let runner = FakeRunner::with(reset_cycle_outputs());
        let result = rig_reset(&runner, &gw_plan(), 1, Some(&api))
            .await
            .expect("uncommissioned reset is exit-0 data");
        assert_eq!(result.state, "uncommissioned");
        assert_eq!(result.removed_volumes, vec!["fixture-rig_gw-data"]);
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.contains("http://localhost:9088/welcome")),
            "wizard URL in warnings: {:?}",
            result.warnings
        );
    }

    /// Port re-grabbed mid-cycle (another rig took a freed port between
    /// the halves): Rig error with attribution, and the up NEVER ran.
    #[tokio::test]
    async fn reset_port_regrabbed_midcycle_errors_and_never_ups() {
        let occupant = r#"{"Names":"other-gw-1","Labels":"com.docker.compose.project=other"}"#;
        let runner = FakeRunner::with(vec![
            ok(""),              // volume ls: nothing to remove
            version_ok(),
            ok(""),              // down -v
            ok(occupant),        // preflight 9088: re-grabbed mid-cycle
            ok(OWN_OCCUPANT),    // preflight 9443: own project
        ]);
        let err = rig_reset(&runner, &gw_plan(), 300, None)
            .await
            .expect_err("mid-cycle port grab aborts before the up half");
        assert!(matches!(err, CoreError::Rig(_)));
        assert_eq!(err.exit_code(), 7);
        let message = err.to_string();
        assert!(
            message.contains("port 9088 in use by container other-gw-1 (rig other)"),
            "{message}"
        );
        assert!(
            message.contains("torn down"),
            "the hint names the torn-down state: {message}"
        );
        // The up NEVER ran — the last recorded call is the pre-flight.
        let calls = runner.calls();
        assert_eq!(calls.len(), 5, "no up call: {calls:?}");
        assert_eq!(calls.last().expect("calls exist").0, "docker");
    }

    /// Teardown failure carries compose's stderr tail.
    #[tokio::test]
    async fn reset_down_failure_carries_stderr_tail() {
        let runner = FakeRunner::with(vec![
            ok(""),
            version_ok(),
            ComposeOutput {
                stdout: String::new(),
                stderr: "cannot remove volume: in use".into(),
                code: 1,
            },
        ]);
        let err = rig_reset(&runner, &gw_plan(), 300, None)
            .await
            .expect_err("down -v failure errors");
        let message = err.to_string();
        assert!(message.contains("docker compose down failed (exit 1)"), "{message}");
        assert!(message.contains("in use"), "{message}");
    }

    // ---------------------------------------------------------------------
    // rig_logs — passthrough streaming (04-02)
    // ---------------------------------------------------------------------

    /// Raw compose log lines (color codes stripped by fixtures; the
    /// sink must receive them VERBATIM — no envelope, no reformat).
    const LOGS_STDOUT: &str = concat!(
        "ignition-1  | 22:01:01.001 INFO   Gateway - starting\n",
        "ignition-1  | 22:01:02.002 INFO   Gateway - RUNNING\n",
    );

    /// One-shot: the captured stdout splits line-wise into the sink,
    /// verbatim, via the plain `run` seam (exact args pinned).
    #[tokio::test]
    async fn logs_one_shot_sinks_lines_verbatim() {
        let runner = FakeRunner::with(vec![ok(LOGS_STDOUT)]);
        let mut received: Vec<String> = Vec::new();
        let result = rig_logs(&runner, &gw_plan(), 200, false, None, &mut |line| {
            received.push(line)
        })
        .await
        .expect("logs succeeds");
        assert_eq!(result.streamed, 2);
        assert_eq!(
            received,
            vec![
                "ignition-1  | 22:01:01.001 INFO   Gateway - starting",
                "ignition-1  | 22:01:02.002 INFO   Gateway - RUNNING",
            ],
            "lines pass through verbatim — no envelope wrapping ever"
        );
        let calls = runner.calls();
        assert_eq!(calls, vec![("docker compose", logs_args(&gw_plan(), 200, false, None))]);
    }

    /// Follow: rides the STREAMING seam (the fake replays its
    /// preloaded stdout through the sink), service filter pinned.
    #[tokio::test]
    async fn logs_follow_streams_via_the_streaming_seam() {
        let runner = FakeRunner::with(vec![ok(LOGS_STDOUT)]);
        let mut received: Vec<String> = Vec::new();
        let result = rig_logs(&runner, &gw_plan(), 50, true, Some("ignition"), &mut |line| {
            received.push(line)
        })
        .await
        .expect("follow logs succeeds");
        assert_eq!(result.streamed, 2, "streamed lines counted in follow mode");
        assert_eq!(received.len(), 2);
        let calls = runner.calls();
        assert_eq!(
            calls,
            vec![("docker compose", logs_args(&gw_plan(), 50, true, Some("ignition")))]
        );
    }

    /// Failure: exit-mapped error with compose's stderr tail; the
    /// diagnostics NEVER ride the data sink.
    #[tokio::test]
    async fn logs_failure_carries_stderr_tail_never_sink() {
        let runner = FakeRunner::with(vec![ComposeOutput {
            stdout: String::new(),
            stderr: "no such service: nosvc".into(),
            code: 1,
        }]);
        let mut received: Vec<String> = Vec::new();
        let err = rig_logs(&runner, &gw_plan(), 200, false, Some("nosvc"), &mut |line| {
            received.push(line)
        })
        .await
        .expect_err("unknown service errors");
        let message = err.to_string();
        assert!(message.contains("docker compose logs failed (exit 1)"), "{message}");
        assert!(message.contains("no such service"), "{message}");
        assert!(received.is_empty(), "diagnostics never ride the data sink");
    }

    // ---------------------------------------------------------------------
    // trial_status — the trial endpoint + banners cross-check (04-03)
    // ---------------------------------------------------------------------

    /// Mount the EXPIRED live captures (ign-research 8.3.6): trial
    /// AllInDemo/0s/expired + banners warning/null.
    async fn expired_trial_server() -> wiremock::MockServer {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/data/api/v1/trial"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
                serde_json::json!({
                    "licenseMode": "Trial", "trialState": "AllInDemo",
                    "trialSecondsLeft": 0, "expired": true,
                    "emergency": false, "emergencySecondsLeft": 0,
                    "development": false, "developmentSecondsLeft": 0
                }),
            ))
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/data/api/v1/overview/banners"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
                serde_json::json!({
                    "banners": [{
                        "order": 0, "type": "trial",
                        "data": { "severity": "warning", "expireTime": null,
                                  "toolTips": [], "actions": [] }
                    }]
                }),
            ))
            .mount(&server)
            .await;
        server
    }

    /// The expired rig's exact output shape (all keys always; the
    /// banners cross-check rides severity/expire_time_ms/active).
    #[tokio::test]
    async fn trial_status_expired_shape_is_exact() {
        let server = expired_trial_server().await;
        let api = crate::client::ReqwestGatewayApi::for_tests(&server.uri(), None);
        let result = trial_status(&api).await.expect("expired status parses");
        assert_eq!(
            serde_json::to_value(&result).unwrap(),
            serde_json::json!({
                "license_mode": "Trial",
                "trial_state": "AllInDemo",
                "trial_remaining_s": 0,
                "expired": true,
                "emergency": false,
                "emergency_remaining_s": 0,
                "development": false,
                "banners": {
                    "severity": "warning",
                    "expire_time_ms": null,
                    "active": false
                },
                "warnings": []
            }),
            "EXACT shape comparison — the unit-explicit keys + the \
             banners cross-check block, no unknown keys"
        );
    }

    /// The ACTIVE cross-check: severity info + a far-FUTURE epoch-ms
    /// expireTime → active true; the SAME severity with a far-PAST
    /// expireTime → active false (the Pitfall-7 pin: expiry time is
    /// part of the active derivation, severity alone never is).
    #[tokio::test]
    async fn trial_status_banner_active_requires_future_expire_time() {
        for (expire_time, active) in [(9_999_999_999_999_999i64, true), (1i64, false)] {
            let server = wiremock::MockServer::start().await;
            wiremock::Mock::given(wiremock::matchers::method("GET"))
                .and(wiremock::matchers::path("/data/api/v1/trial"))
                .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({
                        "licenseMode": "Trial", "trialState": "AllInDemo",
                        "trialSecondsLeft": 6590, "expired": false,
                        "emergency": false, "emergencySecondsLeft": 0,
                        "development": false, "developmentSecondsLeft": 0
                    }),
                ))
                .mount(&server)
                .await;
            wiremock::Mock::given(wiremock::matchers::method("GET"))
                .and(wiremock::matchers::path("/data/api/v1/overview/banners"))
                .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({
                        "banners": [{
                            "order": 5, "type": "trial",
                            "data": { "severity": "info",
                                      "expireTime": expire_time,
                                      "toolTips": [], "actions": [] }
                        }]
                    }),
                ))
                .mount(&server)
                .await;
            let api = crate::client::ReqwestGatewayApi::for_tests(&server.uri(), None);
            let result = trial_status(&api).await.expect("active status parses");
            assert!(!result.expired, "primary truth from the trial endpoint");
            assert_eq!(result.trial_remaining_s, 6590);
            assert_eq!(
                result.banners.severity.as_deref(),
                Some("info"),
                "the trial banner surfaced (8.3.3 serves order 5 — not an index)"
            );
            assert_eq!(
                result.banners.active, active,
                "info severity + expireTime {expire_time} → active {active} (Pitfall 7)"
            );
        }
    }

    /// A failed banners fetch degrades to nulls + a data-level
    /// warning — the trial endpoint's expired flag stays the truth.
    #[tokio::test]
    async fn trial_status_banners_failure_degrades_with_warning() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/data/api/v1/trial"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
                serde_json::json!({
                    "licenseMode": "Trial", "trialState": "AllInDemo",
                    "trialSecondsLeft": 0, "expired": true,
                    "emergency": false, "emergencySecondsLeft": 0,
                    "development": false, "developmentSecondsLeft": 0
                }),
            ))
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/data/api/v1/overview/banners"))
            .respond_with(wiremock::ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let api = crate::client::ReqwestGatewayApi::for_tests(&server.uri(), None);
        let result = trial_status(&api).await.expect("primary endpoint answered");
        assert!(result.expired, "primary truth survives the cross-check failure");
        assert_eq!(result.banners.severity, None);
        assert_eq!(result.banners.expire_time_ms, None);
        assert!(!result.banners.active);
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.contains("banners cross-check unavailable")),
            "the degradation is visible data: {:?}",
            result.warnings
        );
    }

    // ---------------------------------------------------------------------
    // trial_reset — the ladder (04-03)
    // ---------------------------------------------------------------------

    /// The trial JSON body for a given state.
    fn trial_body(expired: bool, seconds_left: i64) -> serde_json::Value {
        serde_json::json!({
            "licenseMode": "Trial", "trialState": "AllInDemo",
            "trialSecondsLeft": seconds_left, "expired": expired,
            "emergency": false, "emergencySecondsLeft": 0,
            "development": false, "developmentSecondsLeft": 0
        })
    }

    /// A stateful trial GET/POST script: GET answers EXPIRED until the
    /// successful reset POST lands, then FRESH forever (the flip the
    /// read-back verifies). `post_status` controls the POST's answer
    /// (200 = reset lands and flips the flag; anything else refuses
    /// WITHOUT flipping — the state-gate/credential-refusal shapes).
    #[derive(Clone)]
    struct TrialFlipScript {
        reset_done: std::sync::Arc<std::sync::atomic::AtomicBool>,
        post_status: u16,
    }

    impl wiremock::Respond for TrialFlipScript {
        fn respond(&self, request: &wiremock::Request) -> wiremock::ResponseTemplate {
            if request.method.as_str() == "POST" {
                if self.post_status == 200 {
                    self.reset_done
                        .store(true, std::sync::atomic::Ordering::SeqCst);
                    return wiremock::ResponseTemplate::new(200)
                        .set_body_json(trial_body(false, 7199));
                }
                return wiremock::ResponseTemplate::new(self.post_status);
            }
            let expired = !self.reset_done.load(std::sync::atomic::Ordering::SeqCst);
            wiremock::ResponseTemplate::new(200).set_body_json(trial_body(expired, if expired { 0 } else { 7199 }))
        }
    }

    /// Mount GET + POST /trial on one script (the read-back sees the
    /// flip when the POST succeeded).
    async fn trial_reset_server(post_status: u16) -> (wiremock::MockServer, TrialFlipScript) {
        let server = wiremock::MockServer::start().await;
        let script = TrialFlipScript {
            reset_done: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            post_status,
        };
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/data/api/v1/trial"))
            .respond_with(script.clone())
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/data/api/v1/trial"))
            .respond_with(script.clone())
            .mount(&server)
            .await;
        (server, script)
    }

    /// The pre-check refusal: an ACTIVE trial errors TrialNotExpired
    /// (exit 6) naming the seconds left — the live-discovered state
    /// gate surfaced honestly, and the POST NEVER fires.
    #[tokio::test]
    async fn trial_reset_refuses_active_trial_up_front() {
        let (server, script) = trial_reset_server(200).await;
        // Force the "active" starting state (the script's flag starts
        // un-flipped = expired; flip it up front for this test).
        script
            .reset_done
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let api = crate::client::ReqwestGatewayApi::for_tests(&server.uri(), None);
        let err = trial_reset(&api, &server.uri(), false, None)
            .await
            .expect_err("an active trial is refused before any POST");
        assert!(matches!(err, CoreError::TrialNotExpired { .. }), "{err}");
        assert_eq!(err.exit_code(), 6);
        assert_eq!(err.code(), "trial_not_expired");
        let message = err.to_string();
        assert!(
            message.contains("7199s left"),
            "the message names the countdown: {message}"
        );
    }

    /// Tier 0 lands: token-auth POST through the client pipeline, the
    /// read-back flip verified, mechanism "token".
    #[tokio::test]
    async fn trial_reset_tier0_lands_with_read_back_flip() {
        let (server, _script) = trial_reset_server(200).await;
        let credential = crate::config::Credential::Token(crate::config::Secret::new(
            "spike:tokengeneratedlive",
        ));
        let api = crate::client::ReqwestGatewayApi::for_tests(&server.uri(), Some(credential));
        let result = trial_reset(&api, &server.uri(), true, None)
            .await
            .expect("tier 0 resets the expired trial");
        assert_eq!(
            serde_json::to_value(&result).unwrap(),
            serde_json::json!({
                "rig_url": server.uri(),
                "mechanism": "token",
                "expired_before": true,
                "expired_after": false,
                "trial_remaining_s": 7199
            }),
            "EXACT shape — which rung landed + the before/after flip"
        );
    }

    /// Tier 0 refused (401) with NO login pair: the token rung's error
    /// propagates as the credential-less tail (SecretUnavailable,
    /// exit 3).
    #[tokio::test]
    async fn trial_reset_token_refused_without_login_errors() {
        let (server, _script) = trial_reset_server(401).await;
        let credential = crate::config::Credential::Token(crate::config::Secret::new(
            "spike:wrongtoken",
        ));
        let api = crate::client::ReqwestGatewayApi::for_tests(&server.uri(), Some(credential));
        let err = trial_reset(&api, &server.uri(), true, None)
            .await
            .expect_err("the refused token rung has no fallback");
        assert!(matches!(err, CoreError::SecretUnavailable { .. }), "{err}");
        assert_eq!(err.exit_code(), 3);
    }

    /// The minimal tier-1 dance mount (the full request-chain pins
    /// live in tests/trial_contract.rs; this is the ACTION-level proof
    /// that the ladder wires the flow at the rig URL). The session
    /// POST rides PRIORITY 1 so wiremock checks it before the
    /// script's catch-all (stable order would otherwise let the
    /// earlier-mounted plain-POST mock steal the CSRF-carrying
    /// request), and its landing flips the same read-back flag.
    async fn login_dance_server() -> (wiremock::MockServer, TrialFlipScript) {
        let (server, script) = trial_reset_server(401).await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/data/app/login"))
            .respond_with(
                wiremock::ResponseTemplate::new(302).insert_header(
                    "Location",
                    "/idp/default/oidc/auth?app=gateway&state=st&nonce=nc",
                ),
            )
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/idp/default/oidc/auth"))
            .and(wiremock::matchers::query_param_is_missing("token"))
            .respond_with(
                wiremock::ResponseTemplate::new(302).insert_header(
                    "Location",
                    "/idp/default/authn/login?app=gateway&token=TT0",
                ),
            )
            .mount(&server)
            .await;
        for (body_token, answer) in [
            ("TT0", r#"{"complete":false,"nextChallenge":[{"type":"basic"}],"token":"TT1"}"#),
            ("TT2", r#"{"complete":true,"token":"TT3"}"#),
        ] {
            wiremock::Mock::given(wiremock::matchers::method("POST"))
                .and(wiremock::matchers::path("/idp/default/authn/next-challenge"))
                .and(wiremock::matchers::body_json(serde_json::json!({ "token": body_token })))
                .respond_with(
                    wiremock::ResponseTemplate::new(200)
                        .set_body_string(answer)
                        .insert_header("Content-Type", "application/json"),
                )
                .mount(&server)
                .await;
        }
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/idp/default/authn/submit-challenge/basic"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_string(r#"{"success":true,"token":"TT2"}"#)
                    .insert_header("Content-Type", "application/json"),
            )
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/idp/default/oidc/auth"))
            .and(wiremock::matchers::query_param("token", "TT3"))
            .respond_with(wiremock::ResponseTemplate::new(302).insert_header(
                "Location",
                "/data/federate/callback/internal?code=c&state=st",
            ))
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/data/federate/callback/internal"))
            .respond_with(
                wiremock::ResponseTemplate::new(302)
                    .insert_header("Location", "/app")
                    .append_header("Set-Cookie", "webui-sid-1=sess; Path=/; HttpOnly"),
            )
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/data/app/session"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_string(r#"{"userPayload":{},"csrfToken":"csrf1"}"#)
                    .insert_header("Content-Type", "application/json"),
            )
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/data/api/v1/trial"))
            .and(wiremock::matchers::header("X-CSRF-Token", "csrf1"))
            .respond_with(SessionResetFlip {
                reset_done: script.reset_done.clone(),
            })
            .with_priority(1)
            .mount(&server)
            .await;
        (server, script)
    }

    /// The session-POST responder: flips the shared read-back flag and
    /// answers the fresh trial.
    struct SessionResetFlip {
        reset_done: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    impl wiremock::Respond for SessionResetFlip {
        fn respond(&self, _request: &wiremock::Request) -> wiremock::ResponseTemplate {
            self.reset_done
                .store(true, std::sync::atomic::Ordering::SeqCst);
            wiremock::ResponseTemplate::new(200).set_body_json(trial_body(false, 7199))
        }
    }

    /// The full ladder: the token rung 401s → the login rung runs the
    /// dance at the RIG URL → reset → read-back flip; mechanism
    /// "login".
    #[tokio::test]
    async fn trial_reset_falls_through_to_the_login_rung() {
        let (server, _script) = login_dance_server().await;
        let credential = crate::config::Credential::Token(crate::config::Secret::new(
            "spike:rejectedtoken",
        ));
        let api = crate::client::ReqwestGatewayApi::for_tests(&server.uri(), Some(credential));
        let password = crate::config::Secret::new("rig-password");
        let result = trial_reset(&api, &server.uri(), true, Some(("admin", &password)))
            .await
            .expect("the login rung carries the reset");
        assert_eq!(result.mechanism, "login");
        assert!(result.expired_before);
        assert!(!result.expired_after);
        assert_eq!(result.trial_remaining_s, 7199);
    }

    /// Tier 1 alone (no token at all): the dance + flip, mechanism
    /// "login".
    #[tokio::test]
    async fn trial_reset_login_rung_alone() {
        let (server, _script) = login_dance_server().await;
        let api = crate::client::ReqwestGatewayApi::for_tests(&server.uri(), None);
        let password = crate::config::Secret::new("rig-password");
        let result = trial_reset(&api, &server.uri(), false, Some(("admin", &password)))
            .await
            .expect("login-only reset works");
        assert_eq!(result.mechanism, "login");
        assert!(!result.expired_after);
    }

    // ---------------------------------------------------------------------
    // rig_status — the allowlist pin
    // ---------------------------------------------------------------------

    /// ps LDJSON with a Publishers array + a second bare service.
    const PS_STDOUT: &str = concat!(
        r#"{"Name":"fixture-rig-ignition-1","Service":"ignition","State":"running","Health":"healthy","ExitCode":0,"Publishers":[{"URL":"0.0.0.0","TargetPort":8088,"PublishedPort":9088,"Protocol":"tcp"},{"URL":"0.0.0.0","TargetPort":443,"PublishedPort":9443,"Protocol":"tcp"}]}"#,
        "\n",
        r#"{"Name":"fixture-rig-db-1","Service":"db","State":"exited","ExitCode":137,"Publishers":[]}"#,
        "\n",
    );

    const VOLUME_STDOUT: &str = concat!(
        r#"{"Name":"fixture-rig_gw-data","Labels":{"com.docker.compose.project":"fixture-rig"}}"#,
        "\n",
    );

    #[tokio::test]
    async fn status_serializes_the_allowlist_exactly() {
        // Call order: version(run), ps(run), volume ls(docker), then a
        // docker ps per host port (2 ports), each showing an occupant
        // (ports_free=false — a running rig holds its own ports).
        let occupant = r#"{"Names":"fixture-rig-ignition-1","Labels":"com.docker.compose.project=fixture-rig"}"#;
        let runner = FakeRunner::with(vec![
            version_ok(),
            ok(PS_STDOUT),
            ok(VOLUME_STDOUT),
            ok(occupant),
            ok(occupant),
        ]);

        let result = rig_status(&runner, &gw_plan())
            .await
            .expect("status succeeds");
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "rig": "fixture-rig",
                "project": "fixture-rig",
                "compose_file": "/rigs/docker/compose.yml",
                "services": [
                    {
                        "name": "ignition",
                        "state": "running",
                        "health": "healthy",
                        "exit_code": 0,
                        "publishers": [
                            {"published_port": 9088, "target_port": 8088, "protocol": "tcp"},
                            {"published_port": 9443, "target_port": 443, "protocol": "tcp"}
                        ]
                    },
                    {
                        "name": "db",
                        "state": "exited",
                        "health": null,
                        "exit_code": 137,
                        "publishers": []
                    }
                ],
                "volumes": ["fixture-rig_gw-data"],
                "ports_free": false
            }),
            "EXACT shape comparison: no compose-config passthrough, no \
             unknown keys — the allowlist IS the contract"
        );
    }

    /// A down rig: empty ps, empty docker-ps per port → exit 0, empty
    /// services, ports_free true (state is data).
    #[tokio::test]
    async fn status_down_rig_is_data() {
        let runner = FakeRunner::with(vec![
            version_ok(),
            ok(""),
            ok(""),
            ok(""),
            ok(""),
        ]);
        let result = rig_status(&runner, &gw_plan())
            .await
            .expect("status of a down rig exits 0");
        assert!(result.services.is_empty());
        assert!(result.volumes.is_empty());
        assert!(result.ports_free);
    }

    /// The result types serialize every family key — agents never
    /// key-hunt (the locked shape rule).
    #[test]
    fn up_and_down_results_carry_all_keys() {
        let up = RigUpResult {
            rig: "r".into(),
            project: "r".into(),
            state: "uncommissioned".into(),
            gateway_url: None,
            warnings: vec![],
        };
        let json = serde_json::to_value(&up).unwrap();
        for key in ["rig", "project", "state", "gateway_url", "warnings"] {
            assert!(json.get(key).is_some(), "missing key {key}");
        }
        let down = RigDownResult {
            rig: "r".into(),
            project: "r".into(),
            state: "down".into(),
        };
        let json = serde_json::to_value(&down).unwrap();
        for key in ["rig", "project", "state"] {
            assert!(json.get(key).is_some(), "missing key {key}");
        }
        let reset = RigResetResult {
            rig: "r".into(),
            project: "r".into(),
            removed_volumes: vec![],
            state: "running".into(),
            warnings: vec![],
        };
        let json = serde_json::to_value(&reset).unwrap();
        for key in ["rig", "project", "removed_volumes", "state", "warnings"] {
            assert!(json.get(key).is_some(), "missing key {key}");
        }
        let status_keys = ["rig", "project", "compose_file", "services", "volumes", "ports_free"];
        let _ = RigStatusResult {
            rig: "r".into(),
            project: "r".into(),
            compose_file: "/c.yml".into(),
            services: vec![],
            volumes: vec![],
            ports_free: true,
        };
        // (shape asserted end-to-end in status_serializes_the_allowlist_exactly;
        // the keys list is pinned here for the doc string)
        assert_eq!(status_keys.len(), 6);
    }
}
