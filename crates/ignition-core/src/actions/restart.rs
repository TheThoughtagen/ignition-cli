//! Restart + wait actions (02-05, HLTH-09/11) — built on the ONE wait
//! engine ([`crate::poll`], 02-04): serde models OUT, no printing.
//!
//! Every wait anchors on the UNAUTHENTICATED [`GatewayApi::status_ping`]
//! (strictly better than polling gateway-info, 02-RESEARCH §Restart:
//! it separates down-ness from auth failure and answers during the
//! STARTING window — the webserver never drops the connection).
//!
//! ## Probe shape (the 02-04 HRTB pattern, verbatim)
//!
//! Every probe returns `PollState<()>` and its poll-owned state is a
//! `&mut`-borrowing type (the tail's `TailState<'a>` shape): a probe
//! whose state carries no lifetime does not typecheck under poll's
//! `for<'a> FnMut(&'a mut S) -> Probe<'a, T>` bound. The terminal
//! STATE string rides the state itself (an outer [`Cell`] the state
//! mutably borrows — readable after `poll` consumes the state), like
//! the tail's `streamed` counter.
//!
//! ## The ONE shared floor ([`RESTART_FLOOR`])
//!
//! Open Question 4's fast-restart race: a very fast restart could flip
//! back to RUNNING before the first poll observes STARTING, so
//! "observe non-RUNNING once, then RUNNING" alone can false-positive.
//! BOTH restart-aware waits share one mitigation constant — no
//! duplicated literals:
//!
//! - [`restart_and_wait`] sleeps the floor right after the POST: any
//!   RUNNING observed after it is genuine success (the grace window
//!   has passed);
//! - [`wait_restart`] (the STANDALONE arm — deliberately NOT
//!   [`wait_gateway`] semantics) accepts an all-RUNNING poll sequence
//!   as success only once the floor has elapsed: `ign restart` fired
//!   the POST up to ~5 s before `ign wait restart` starts polling, and
//!   the gateway still reports RUNNING inside that grace window.
//!   Observing non-RUNNING→RUNNING short-circuits the floor (the
//!   restart was WITNESSED — floor not needed).
//!
//! The floor is a PARAMETER (tests inject milliseconds; the CLI passes
//! [`RESTART_FLOOR`]) — the one knob both semantics share.
//!
//! NEVER use `restart-tasks/pending` as the progress signal (research:
//! it is required-restart config, not restart status).

use std::cell::Cell;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::client::GatewayApi;
use crate::error::CoreError;
use crate::poll::{self, PollConfig, PollState};

/// The ONE shared floor for both restart-aware waits (must-have
/// key_link): 5 s of post-POST grace before an all-RUNNING poll
/// sequence may report success. Injectable as a parameter; this is the
/// production value the CLI passes.
pub const RESTART_FLOOR: Duration = Duration::from_secs(5);

/// Default wait interval (research §Wait-loop pattern).
pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(2);

/// Default restart budget (research: ~40 s observed; 300 s headroom).
pub const RESTART_TIMEOUT: Duration = Duration::from_secs(300);

/// Default gateway/module readiness budget.
pub const READINESS_TIMEOUT: Duration = Duration::from_secs(120);

/// The state a healthy gateway reports (observed live; the wait's
/// terminal condition).
const RUNNING: &str = "RUNNING";

/// The state a fully-loaded healthy module reports.
const ACTIVE: &str = "ACTIVE";

/// `ign restart` (no `--wait`) output model.
#[derive(Debug, Serialize)]
pub struct RestartResult {
    /// Always `true` — the POST was accepted.
    pub restarted: bool,
}

/// `ign restart --wait` output model.
#[derive(Debug, Serialize)]
pub struct RestartWaitResult {
    /// Always `true` — the POST was accepted.
    pub restarted: bool,
    /// The terminal state observed (`RUNNING`).
    pub state: String,
    /// Seconds from the POST to the terminal state (floor included).
    pub elapsed_secs: u64,
}

/// `ign wait <target>` output model (gateway / restart / module).
#[derive(Debug, Serialize)]
pub struct WaitResult {
    /// What was waited on: `gateway`, `restart`, or `module <id>`.
    pub target: String,
    /// The terminal state observed (`RUNNING` / `ACTIVE`).
    pub state: String,
    /// Seconds until the terminal state.
    pub elapsed_secs: u64,
}

/// `ign restart` without `--wait`: fire the POST and return — the
/// human-mode advisory line ("READY in ~1 min") belongs to the CLI.
/// Confirmation guarding belongs to the CALLER (guard before any API
/// construction).
pub async fn restart(api: &dyn GatewayApi) -> Result<RestartResult, CoreError> {
    api.restart().await?;
    Ok(RestartResult { restarted: true })
}

/// `ign restart --wait`: POST → sleep the floor (Open Question 4) →
/// poll `/StatusPing` until RUNNING. Timeout (default 300 s) → the
/// poll engine's Network-class deadline error, whose message names the
/// last observed state.
pub async fn restart_and_wait(
    api: &dyn GatewayApi,
    interval: Duration,
    timeout: Duration,
    floor: Duration,
) -> Result<RestartWaitResult, CoreError> {
    api.restart().await?;
    let started = Instant::now();
    // The floor sleeps BEFORE the first poll: post-floor RUNNING is
    // unambiguous success even if the STARTING window was never
    // observed (fast-flip race closed by construction).
    tokio::time::sleep(floor).await;
    let state = wait_state_running(
        api,
        "restart completion (GET /StatusPing)".to_string(),
        interval,
        timeout,
    )
    .await?;
    Ok(RestartWaitResult {
        restarted: true,
        state,
        elapsed_secs: started.elapsed().as_secs(),
    })
}

/// `ign wait gateway`: poll `/StatusPing` until RUNNING. Works with NO
/// credential (the dispatch constructs the client header-less for this
/// command). IMMEDIATE success when already RUNNING is CORRECT here:
/// `wait gateway` answers "is it up", not "did it restart" — the
/// restart-aware variant is [`wait_restart`].
pub async fn wait_gateway(
    api: &dyn GatewayApi,
    interval: Duration,
    timeout: Duration,
) -> Result<WaitResult, CoreError> {
    let started = Instant::now();
    let state = wait_state_running(
        api,
        "gateway readiness (GET /StatusPing)".to_string(),
        interval,
        timeout,
    )
    .await?;
    Ok(WaitResult {
        target: "gateway".to_string(),
        state,
        elapsed_secs: started.elapsed().as_secs(),
    })
}

/// `ign wait restart`: the STANDALONE restart-aware wait (research
/// line 94 + Open Question 4). The moment any non-RUNNING state is
/// observed, keep polling until RUNNING → terminal success (the
/// restart was witnessed; floor not needed). If polls are RUNNING from
/// the start, success is accepted ONLY after `floor` has elapsed —
/// `ign restart` fired the POST up to `floor` before this command
/// started and the gateway still reports RUNNING in that grace window,
/// so immediate success would be a false positive. Deadline → the poll
/// engine's Network-class timeout naming the last observed state.
pub async fn wait_restart(
    api: &dyn GatewayApi,
    interval: Duration,
    timeout: Duration,
    floor: Duration,
) -> Result<WaitResult, CoreError> {
    let started = Instant::now();
    let cfg = PollConfig {
        subject: "restart completion (GET /StatusPing)".to_string(),
        interval,
        deadline: timeout,
        ..PollConfig::default()
    };
    /// Probe scratch (borrowing state — the HRTB shape): did any poll
    /// observe a non-RUNNING state, and where the terminal state goes.
    /// The Cell lives OUTSIDE poll; the state mutably borrows it, so
    /// the result survives poll consuming the state (the tail's
    /// `streamed` pattern).
    struct Witness<'a> {
        seen_non_running: bool,
        final_state: &'a mut Cell<String>,
    }
    let mut final_state = Cell::new(String::new());
    poll::poll(
        cfg,
        Witness {
            seen_non_running: false,
            final_state: &mut final_state,
        },
        |witness| {
            Box::pin(async {
                let ping = api.status_ping().await?;
                if ping.state == RUNNING {
                    if witness.seen_non_running || started.elapsed() >= floor {
                        witness.final_state.set(ping.state);
                        Ok(PollState::<()>::Done(()))
                    } else {
                        // All-RUNNING inside the grace floor: accepting
                        // now would false-positive on `ign restart`'s
                        // ~5 s post-POST window.
                        Ok(PollState::<()>::Pending(Some(format!(
                            "{RUNNING} (all-RUNNING inside the {floor:?} restart grace floor)"
                        ))))
                    }
                } else {
                    witness.seen_non_running = true;
                    Ok(PollState::<()>::Pending(Some(ping.state)))
                }
            })
        },
    )
    .await?;
    Ok(WaitResult {
        target: "restart".to_string(),
        state: final_state.take(),
        elapsed_secs: started.elapsed().as_secs(),
    })
}

/// `ign wait module <id>`: poll `modules/healthy?search=<id>` until the
/// item with that id reports `ACTIVE` (research §Modules). Deadline →
/// the poll engine's Network-class timeout naming the id (the subject)
/// and the last observed state.
pub async fn wait_module(
    api: &dyn GatewayApi,
    module_id: &str,
    interval: Duration,
    timeout: Duration,
) -> Result<WaitResult, CoreError> {
    let started = Instant::now();
    let cfg = PollConfig {
        subject: format!("module {module_id} ACTIVE (GET /data/api/v1/modules/healthy)"),
        interval,
        deadline: timeout,
        ..PollConfig::default()
    };
    let mut final_state = Cell::new(String::new());
    poll::poll(cfg, &mut final_state, |final_state| {
        Box::pin(async {
            let query = crate::client::query::ListQuery {
                search: Some(module_id.to_string()),
                ..Default::default()
            };
            let modules = api.modules(false, &query).await?;
            // search is a substring match over names too — the row
            // must be THE module (id equality).
            if let Some(module) = modules.items.iter().find(|m| m.id == module_id) {
                if module.state.as_deref() == Some(ACTIVE) {
                    final_state.set(ACTIVE.to_string());
                    Ok(PollState::<()>::Done(()))
                } else {
                    Ok(PollState::<()>::Pending(Some(format!(
                        "{} state {}",
                        module.id,
                        module.state.as_deref().unwrap_or("-")
                    ))))
                }
            } else {
                Ok(PollState::<()>::Pending(Some(format!(
                    "{module_id} not present in the healthy module list"
                ))))
            }
        })
    })
    .await?;
    Ok(WaitResult {
        target: format!("module {module_id}"),
        state: final_state.take(),
        elapsed_secs: started.elapsed().as_secs(),
    })
}

/// The shared RUNNING probe: poll `/StatusPing` until `state ==
/// "RUNNING"` (STARTING/unknown = Pending with the observed state —
/// unknown states surface verbatim, research Open Question 3). The
/// terminal state rides the Cell the state borrows (poll's T is `()`).
async fn wait_state_running(
    api: &dyn GatewayApi,
    subject: String,
    interval: Duration,
    timeout: Duration,
) -> Result<String, CoreError> {
    let cfg = PollConfig {
        subject,
        interval,
        deadline: timeout,
        ..PollConfig::default()
    };
    let mut final_state = Cell::new(String::new());
    poll::poll(cfg, &mut final_state, |final_state| {
        Box::pin(async {
            let ping = api.status_ping().await?;
            if ping.state == RUNNING {
                final_state.set(ping.state);
                Ok(PollState::<()>::Done(()))
            } else {
                Ok(PollState::<()>::Pending(Some(ping.state)))
            }
        })
    })
    .await?;
    Ok(final_state.take())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::RESTART_FLOOR;

    /// The must-have key_link pin: the floor BOTH restart-aware waits
    /// share is the literal 5 s — no duplicated literals, ever.
    #[test]
    fn restart_floor_is_five_seconds() {
        assert_eq!(RESTART_FLOOR, Duration::from_secs(5));
    }
}
