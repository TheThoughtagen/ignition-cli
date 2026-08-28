//! The Rig screen's workers (06-06) — the docker-side family:
//! a one-shot status summary worker (the pane's entry read), the raw
//! compose-logs stream worker (the [`Mapping::Streamed`] case the
//! registry kind exists for — `rig logs -f` shown IN-SCREEN), and
//! the action-menu fire helpers riding [`super::spawn_action`]'s
//! locked busy guard + pretty-JSON result modal.
//!
//! NO gateway client from the cockpit's profile world is used: the
//! rig verbs address the rig's OWN derived gateway URL via
//! [`crate::context`]'s rig constructors (04-03's lock, TUI edition)
//! — and every auth-value construction stays confined there (the
//! phase's single-file secrets-confinement answer). The Confirm-gated
//! verbs (reset / restore / trial reset — main.rs's
//! `require_confirmation` set EXACTLY) fire ONLY from update's
//! execute_pending; `down` is deliberately UNGUARDED, like the CLI
//! (compose down keeps volumes).

use ignition_core::actions;
use ignition_core::config;
use ignition_core::error::CoreError;
use ignition_core::rig::DockerCompose;
use tokio::sync::watch;

use crate::context;
use crate::event::AppEvent;
use crate::state::AppState;

/// The rig the cockpit operates: AUTO discovery (the CLI's
/// `--rig`/`IGNITION_RIG` fold is a flag/env form — the cockpit's
/// convention scan covers the same cwd/whk-roots levels).
use ignition_core::rig::RigSelection;

/// The compose tail the stream pane fetches on (re)spawn — the CLI's
/// `--tail` default (200), inherited verbatim.
const LOGS_TAIL: u32 = 200;

/// The CLI's default wait budget for the up/reset/restore verbs —
/// `DEFAULT_WAIT_TIMEOUT_S` (300), inherited verbatim.
const WAIT_TIMEOUT_S: u64 = actions::rig::DEFAULT_WAIT_TIMEOUT_S;

/// Resolve the rig plan inside a worker (I/O lives in workers):
/// config load → AUTO discovery → the [`ignition_core::rig::RigPlan`]
/// every rig action takes. Docker/discovery failures flow the
/// family's own error shapes (exit 7 + the search trail).
async fn resolve_auto_plan() -> Result<ignition_core::rig::RigPlan, CoreError> {
    let config = config::load(&config::config_path())?;
    let runner = DockerCompose;
    ignition_core::rig::resolve_plan(&runner, RigSelection::Auto, &config).await
}

/// The rig's derived gateway URL or the family's no-gateway refusal
/// (a rig with no 8088/443 mapping has no gateway to ask — main.rs's
/// `trial_no_gateway`, TUI edition).
fn rig_url(plan: &ignition_core::rig::RigPlan) -> Result<String, CoreError> {
    actions::rig::gateway_url_from(plan).ok_or_else(|| {
        CoreError::Rig(format!(
            "rig {} publishes no gateway port (target 8088/443) — rig \
             commands address the rig's gateway",
            plan.name
        ))
    })
}

/// One-shot rig status (the pane's entry read and the `r` refresh):
/// reports [`AppEvent::RigStatus`] era-stamped. Busy-guarded so
/// repeated entries/refreshes cannot stack. A DOWN rig is Ok-data
/// (empty services + `ports_free` — exit-0 shape, the pane renders
/// it); docker/discovery failures degrade to the Err state.
pub fn spawn_rig_status(state: &mut AppState) {
    if state.rig.status_busy {
        return;
    }
    let Some(tx) = state.events_tx.clone() else {
        return;
    };
    state.rig.status_busy = true;
    let era = state.era;
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            let result = match resolve_auto_plan().await {
                Ok(plan) => actions::rig::rig_status(&DockerCompose, &plan)
                    .await
                    .map_err(|err| err.to_string()),
                Err(err) => Err(err.to_string()),
            };
            let _ = tx.send(AppEvent::RigStatus { era, result });
        });
    }
}

/// The raw compose-logs stream: `rig logs --tail 200 --follow`
/// (service unscoped — the pane is the whole rig) with a sink that
/// forwards each line to the rail as [`AppEvent::RigLogLine`], the
/// whole future `select!`-ed against the pane's shutdown watch so
/// leaving the screen / toggling the pane off stops it even between
/// lines (the 06-03 tail shape — the sink ALREADY has `+ Send`, the
/// 06-01 seam decision). Errors surface as data (the dismissable
/// error modal), never a panic.
pub async fn rig_logs_worker(
    tx: tokio::sync::mpsc::UnboundedSender<AppEvent>,
    mut shutdown: watch::Receiver<bool>,
) {
    let runner = DockerCompose;
    let plan = match resolve_auto_plan().await {
        Ok(plan) => plan,
        Err(err) => {
            let _ = tx.send(AppEvent::Error(format!("rig logs: {err}")));
            return;
        }
    };
    let sink_tx = tx.clone();
    let mut sink = move |line: String| {
        let _ = sink_tx.send(AppEvent::RigLogLine(line));
    };
    tokio::select! {
        result = actions::rig::rig_logs(
            &runner,
            &plan,
            LOGS_TAIL,
            true, // follow — the Streamed mapping's whole point
            None,
            &mut sink,
        ) => {
            if let Err(err) = result {
                let _ = tx.send(AppEvent::Error(format!("rig logs: {err}")));
            }
        }
        // Leaving the screen / toggling the pane off. The
        // dropped-sender case resolves here too — both mean stop.
        _ = shutdown.changed() => {}
    }
}

/// Stop the running stream worker (if any): signal the watch and
/// drop the sender. Idempotent.
pub fn stop_rig_logs(state: &mut AppState) {
    if let Some(shutdown) = state.rig.logs_shutdown.take() {
        let _ = shutdown.send(true);
    }
}

/// Spawn the stream worker for the CURRENT world: a fresh shutdown
/// channel, the pane flag ON, and the ring CLEARED (compose
/// `logs --tail` has no `since` resume — overlapping the tail would
/// double-render; every stream session starts from its own tail).
/// No era bump: the global era belongs to WORLD changes (profile
/// switches), and per-line stamps are locked out (the 06-03
/// decision's twin).
///
/// Outside a tokio runtime (state-machine unit tests) the rails
/// transition stands alone and nothing spawns.
pub fn spawn_rig_logs(state: &mut AppState) {
    let Some(tx) = state.events_tx.clone() else {
        return;
    };
    stop_rig_logs(state); // a prior worker (re-entry) stops first
    state.rig.logs_on = true;
    state.rig.logs.clear();
    state.rig.logs_dropped = 0;
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    state.rig.logs_shutdown = Some(shutdown_tx);
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(rig_logs_worker(tx, shutdown_rx));
    }
}

// ---- Action-menu fire helpers (06-06 Task 1) ----
//
// One helper per rig menu verb, each firing the ignition-core action
// fn AS-IS through [`super::spawn_action`] (the locked busy guard +
// pretty-JSON result modal). The Confirm-gated verbs fire ONLY from
// update's execute_pending — the TUI owns their `--yes`, these arms
// stay unguarded exactly like the actions. `down` fires with NO
// confirm (compose down KEEPS volumes — the CLI's deliberate
// non-guard, main.rs's own dispatch).

/// `ign rig up` — the commissioned-wait probe is a HEADER-LESS
/// client pointed at the rig's own derived gateway URL (never the
/// profile's; main.rs's `commissioned_probe` shape verbatim — `None`
/// when the rig publishes no gateway port, in which case the action
/// skips the wait with a data warning).
pub fn fire_rig_up(state: &mut AppState) {
    super::spawn_action(state, "rig up", async {
        let plan = resolve_auto_plan().await?;
        let probe = actions::rig::gateway_url_from(&plan).and_then(|url| context::rig_client(&url));
        let probe_dyn = probe
            .as_ref()
            .map(|api| api as &dyn ignition_core::client::GatewayApi);
        actions::rig::rig_up(&DockerCompose, &plan, WAIT_TIMEOUT_S, probe_dyn).await
    });
}

/// `ign rig down` — UNGUARDED (volumes kept; `reset` owns the
/// teardown half).
pub fn fire_rig_down(state: &mut AppState) {
    super::spawn_action(state, "rig down", async {
        let plan = resolve_auto_plan().await?;
        actions::rig::rig_down(&DockerCompose, &plan).await
    });
}

/// `ign rig reset` — the CONFIRMED arm (the TUI owned the `--yes`;
/// the action runs unguarded). The probe rides the up half.
pub fn fire_rig_reset(state: &mut AppState) {
    super::spawn_action(state, "rig reset", async {
        let plan = resolve_auto_plan().await?;
        let probe = actions::rig::gateway_url_from(&plan).and_then(|url| context::rig_client(&url));
        let probe_dyn = probe
            .as_ref()
            .map(|api| api as &dyn ignition_core::client::GatewayApi);
        actions::rig::rig_reset(&DockerCompose, &plan, WAIT_TIMEOUT_S, probe_dyn).await
    });
}

/// `ign rig status` (the menu's result-modal arm — the pretty-JSON
/// display of the same allowlist the pane summarizes).
pub fn fire_rig_status(state: &mut AppState) {
    super::spawn_action(state, "rig status", async {
        let plan = resolve_auto_plan().await?;
        actions::rig::rig_status(&DockerCompose, &plan).await
    });
}

/// `ign rig trial status` — header-less client at the rig's URL (the
/// endpoints answer unauthenticated; fresh-rig friendly).
pub fn fire_rig_trial_status(state: &mut AppState) {
    super::spawn_action(state, "trial status", async {
        let plan = resolve_auto_plan().await?;
        let url = rig_url(&plan)?;
        let api = context::rig_client(&url)
            .ok_or_else(|| CoreError::Rig(format!("cannot build client for {url}")))?;
        actions::rig::trial_status(&api).await
    });
}

/// `ign rig trial reset` — the CONFIRMED arm; the ladder's parts are
/// env-only (tier 0 IGNITION_TOKEN → tier 1 IGNITION_USER +
/// IGNITION_PASSWORD — the typed pair comes from the context
/// confinement home).
pub fn fire_rig_trial_reset(state: &mut AppState) {
    super::spawn_action(state, "trial reset", async {
        let plan = resolve_auto_plan().await?;
        let url = rig_url(&plan)?;
        let (token, basic) = context::rig_trial_ladder()?;
        let api = match token.as_deref() {
            Some(token) => context::rig_client_token(&url, token),
            None => context::rig_client(&url),
        }
        .ok_or_else(|| CoreError::Rig(format!("cannot build client for {url}")))?;
        let basic_ref = basic
            .as_ref()
            .map(|(user, password)| (user.as_str(), password));
        actions::rig::trial_reset(&api, &url, token.is_some(), basic_ref).await
    });
}

/// `ign rig snapshot` — the default timestamped directory (the `-o`
/// override stays on the CLI form; the result modal names the dir).
pub fn fire_rig_snapshot(state: &mut AppState) {
    super::spawn_action(state, "rig snapshot", async {
        let plan = resolve_auto_plan().await?;
        let url = rig_url(&plan)?;
        let token = context::rig_token_only()?;
        let api = context::rig_client_token(&url, &token)
            .ok_or_else(|| CoreError::Rig(format!("cannot build client for {url}")))?;
        actions::rig::rig_snapshot(&api, &plan.name, None).await
    });
}

/// `ign rig restore --file <FILE>` — the CONFIRMED arm; the gwbk
/// pre-checks (missing/empty/non-regular) ride the action's own
/// exit-2 refusals, surfaced in the result modal.
pub fn fire_rig_restore(state: &mut AppState, file: String) {
    super::spawn_action(state, "rig restore", async move {
        let plan = resolve_auto_plan().await?;
        let url = rig_url(&plan)?;
        let token = context::rig_token_only()?;
        let api = context::rig_client_token(&url, &token)
            .ok_or_else(|| CoreError::Rig(format!("cannot build client for {url}")))?;
        actions::rig::rig_restore(&api, &url, std::path::Path::new(&file), WAIT_TIMEOUT_S).await
    });
}

#[cfg(test)]
mod tests {
    use super::{spawn_rig_logs, spawn_rig_status, stop_rig_logs};
    use crate::event::AppEvent;
    use crate::state::AppState;

    fn armed_state() -> AppState {
        let mut state = AppState::new();
        state.events_tx = Some(tokio::sync::mpsc::unbounded_channel().0);
        state
    }

    /// The one-shot status rail arms its busy guard without a runtime
    /// (and refuses to stack) — the state-machine half.
    #[test]
    fn rig_status_rail_arms_busy_without_a_runtime() {
        let mut state = armed_state();
        spawn_rig_status(&mut state);
        assert!(state.rig.status_busy, "status load armed");
        spawn_rig_status(&mut state);
        assert!(state.rig.status_busy, "busy guard refuses to stack");
    }

    /// The stream rails transition without a runtime: spawn clears +
    /// flags the pane and arms the shutdown channel; stop clears it.
    #[test]
    fn rig_logs_rails_stand_alone_without_a_runtime() {
        let mut state = armed_state();
        state.rig.logs_on = false;
        state.rig.logs.push_back("stale".into());

        spawn_rig_logs(&mut state);
        assert!(state.rig.logs_on, "pane flag on");
        assert!(state.rig.logs.is_empty(), "ring cleared at spawn");
        assert!(state.rig.logs_shutdown.is_some(), "shutdown rail armed");

        stop_rig_logs(&mut state);
        assert!(state.rig.logs_shutdown.is_none(), "rail cleared");
    }

    /// The key_link proof (the registry's Streamed mapping): compose
    /// lines a FakeRunner serves reach the rail as
    /// `AppEvent::RigLogLine` — `rig_logs`'s `run_streaming` sink →
    /// the pane's ring, WITHOUT docker. The follow mode rides the
    /// runner's streaming shape (preload contract: queued stdout
    /// lines replay to the sink in order).
    #[tokio::test]
    async fn rig_logs_sink_forwards_compose_lines_to_the_rail() {
        use ignition_core::rig::RigPlan;
        use ignition_core::rig::compose::{ComposeOutput, ComposeRunner};

        struct OneShotRunner(ComposeOutput);
        #[async_trait::async_trait]
        impl ComposeRunner for OneShotRunner {
            async fn run(&self, _args: &[String]) -> ComposeOutput {
                ComposeOutput {
                    stdout: String::new(),
                    stderr: String::new(),
                    code: 0,
                }
            }
            async fn run_docker(&self, _args: &[String]) -> ComposeOutput {
                ComposeOutput {
                    stdout: String::new(),
                    stderr: String::new(),
                    code: 0,
                }
            }
            async fn run_streaming(
                &self,
                _args: &[String],
                line_sink: &mut (dyn for<'a> FnMut(&'a str) + Send),
            ) -> ComposeOutput {
                for line in self.0.stdout.lines() {
                    line_sink(line);
                }
                ComposeOutput {
                    stdout: String::new(),
                    stderr: String::new(),
                    code: 0,
                }
            }
        }

        let plan = RigPlan {
            name: "fixture-rig".into(),
            compose_file: "/rigs/docker/compose.yml".into(),
            project_dir: "/rigs/docker".into(),
            services: vec!["ignition".into()],
            host_ports: vec![9088],
            port_mappings: vec![],
            volumes: vec!["gw-data".into()],
        };
        let runner = OneShotRunner(ComposeOutput {
            stdout: "gw line one\ngw line two\n".to_string(),
            stderr: String::new(),
            code: 0,
        });

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut sink = move |line: String| {
            let _ = tx.send(AppEvent::RigLogLine(line));
        };
        let result =
            ignition_core::actions::rig::rig_logs(&runner, &plan, 200, true, None, &mut sink)
                .await
                .expect("streamed logs succeed");
        assert_eq!(result.streamed, 2, "both compose lines streamed");

        match rx.recv().await {
            Some(AppEvent::RigLogLine(line)) => assert_eq!(line, "gw line one"),
            other => panic!("expected RigLogLine, got {other:?}"),
        }
        match rx.recv().await {
            Some(AppEvent::RigLogLine(line)) => assert_eq!(line, "gw line two"),
            other => panic!("expected RigLogLine, got {other:?}"),
        }
    }
}
