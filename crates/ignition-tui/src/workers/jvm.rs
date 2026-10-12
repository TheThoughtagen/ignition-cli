//! The JVM-profiling screen's workers — one-shot spawns over the
//! shared [`ignition_core::actions::jvm`] verbs (the ops.rs spawn
//! shape: screen-owned busy guard, client Arc from `state.client`,
//! config loaded INSIDE the worker — the fire_webdev_status
//! precedent — era captured at spawn, results pre-rendered to
//! display lines so update stays pure).
//!
//! The risk gate rides EVERY action call: `state.jvm.acknowledged`
//! (set only by the typed-`ACKNOWLEDGE` interstitial) is passed down
//! as the action layer's `risk_accepted` — the screen is unreachable
//! without it AND the action enforces it, so no future key-binding
//! mistake can skip the gate. The status poll carries no gate
//! (jfrCheck is read-only).

use std::sync::Arc;

use ignition_core::actions::jvm;
use ignition_core::client::ReqwestGatewayApi;

use crate::event::AppEvent;
use crate::state::AppState;

/// The deployed routes' project (the `fire_webdev_status` default —
/// where `ign webdev deploy` put scriptExec).
const JVM_PROJECT: &str = "ign-cli";

/// The client Arc (the ops.rs twin).
fn client_arc(state: &AppState) -> Option<Arc<ReqwestGatewayApi>> {
    state.client.as_ref().map(|handle| handle.0.clone())
}

/// Load the config INSIDE the worker (the fire_webdev_status
/// precedent — the secret lives in the profile config, and workers
/// own their I/O).
async fn load_config() -> Result<ignition_core::config::Config, ignition_core::error::CoreError> {
    let path = ignition_core::config::config_path();
    ignition_core::config::load(&path)
}

/// `r` / screen-entry poll: the JFR recording list (ungated —
/// checking is how you confirm cleanup).
pub fn spawn_jvm_status(state: &mut AppState) {
    let Some(client) = client_arc(state) else {
        return;
    };
    let Some(profile) = state.profile.clone() else {
        return;
    };
    let Some(tx) = state.events_tx.clone() else {
        return;
    };
    let era = state.era;
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            let result = match load_config().await {
                Ok(config) => jvm::jvm_jfr_status(&*client, &config, &profile, JVM_PROJECT)
                    .await
                    .map(|status| (status.active, status.recordings))
                    .map_err(|err| err.to_string()),
                Err(err) => Err(err.to_string()),
            };
            let _ = tx.send(AppEvent::JvmStatus { era, result });
        });
    }
}

/// The shared one-shot action spawn (the spawn_action shape keyed
/// off `state.jvm.in_flight` — a Jvm action never hijacks the
/// dashboard's busy slot). `render` maps the typed result to display
/// lines inside the worker so update stays pure.
fn spawn_jvm_action<T, F, R>(state: &mut AppState, label: &'static str, fut: F, render: R)
where
    T: Send + 'static,
    F: std::future::Future<Output = Result<T, ignition_core::error::CoreError>> + Send + 'static,
    R: FnOnce(&T) -> Vec<String> + Send + 'static,
{
    if state.jvm.in_flight.is_some() {
        return;
    }
    let Some(tx) = state.events_tx.clone() else {
        return;
    };
    state.jvm.in_flight = Some(label);
    let era = state.era;
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            let result = match fut.await {
                Ok(value) => Ok(render(&value)),
                Err(err) => Err(err.to_string()),
            };
            let _ = tx.send(AppEvent::JvmDone { era, label, result });
        });
    }
}

/// ONE sampling window rendered to display lines — shared by the
/// one-shot `s` action and every live-watch tick.
fn sample_lines(result: &jvm::JvmSampleResult) -> Vec<String> {
    let mut lines = vec![format!(
        "sampled {} times over a {}s window (wall {} ms; {} thread stacks, peak {} live)",
        result.samples,
        result.duration_secs,
        result.wall_ms,
        result.observations,
        result.thread_peak
    )];
    lines.push(String::new());
    lines.push("hot methods (self):".to_string());
    if result.hot_methods.is_empty() {
        lines.push("  (no stacks captured)".to_string());
    }
    for method in &result.hot_methods {
        lines.push(format!(
            "  {:>5.1}%  {:>6}  {}",
            method.pct, method.count, method.frame
        ));
    }
    lines.push(String::new());
    lines.push("heaviest stacks:".to_string());
    for stack in result.top_stacks.iter().take(20) {
        lines.push(format!(
            "  {:>5.1}%  {:>6}  {}",
            stack.pct, stack.count, stack.stack
        ));
    }
    if result.top_stacks.len() > 20 {
        lines.push(format!(
            "  … {} more — `ign jvm sample --json` for the full set",
            result.top_stacks.len() - 20
        ));
    }
    lines
}

/// `s` — the sampling thread profiler at the FIXED safe defaults
/// (30 s @ 20 ms, top 25 — the CLI's flags stay CLI-side; the TUI
/// surface deliberately exposes no way to raise the caps). The gate:
/// `acknowledged` rides down as `risk_accepted`.
pub fn spawn_jvm_sample(state: &mut AppState) {
    let Some(client) = client_arc(state) else {
        return;
    };
    let Some(profile) = state.profile.clone() else {
        return;
    };
    let risk_accepted = state.jvm.acknowledged;
    spawn_jvm_action(
        state,
        "jvm sample",
        async move {
            let config = load_config().await?;
            let opts = jvm::JvmSampleOptions::default();
            jvm::jvm_sample(
                &*client,
                &config,
                &profile,
                JVM_PROJECT,
                &opts,
                risk_accepted,
            )
            .await
        },
        sample_lines,
    );
}

// ── The live watch (continuous mode) ───────────────────────────────

/// The watch window's duration (each window is one capped sampling
/// run — the per-window safety contract is unchanged).
pub const WATCH_WINDOW_SECS: u64 = 10;
/// The watch window's dump interval — half the one-shot rate (an
/// open-ended loop earns a gentler cadence).
pub const WATCH_INTERVAL_MS: u64 = 50;

/// `w` — the CONTINUOUS sampler: bounded windows back-to-back, each
/// landing as a [`AppEvent::JvmWatchTick`] that refreshes the result
/// pane. The tail's conventions: a `watch<bool>` shutdown channel
/// (toggle/screen-exit/profile-switch stop it), the busy slot held
/// for the watch's WHOLE life (one-shots refuse while it runs), and
/// a window failure rendered as data (the loop keeps sampling — one
/// bad window is not a death). The risk gate rides every window (the
/// same constant `acknowledged` the one-shot passes).
pub fn spawn_jvm_watch(state: &mut AppState) {
    if state.jvm.in_flight.is_some() || state.jvm.watch_shutdown.is_some() {
        return;
    }
    let Some(client) = client_arc(state) else {
        return;
    };
    let Some(profile) = state.profile.clone() else {
        return;
    };
    let Some(tx) = state.events_tx.clone() else {
        return;
    };
    let risk_accepted = state.jvm.acknowledged;
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
    state.jvm.watch_shutdown = Some(shutdown_tx);
    state.jvm.in_flight = Some("jvm watch (live)");
    let era = state.era;
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            loop {
                let window = async {
                    let config = load_config().await?;
                    let opts = jvm::JvmSampleOptions {
                        duration_secs: WATCH_WINDOW_SECS,
                        interval_ms: WATCH_INTERVAL_MS,
                        ..jvm::JvmSampleOptions::default()
                    };
                    jvm::jvm_sample(
                        &*client,
                        &config,
                        &profile,
                        JVM_PROJECT,
                        &opts,
                        risk_accepted,
                    )
                    .await
                };
                tokio::select! {
                    biased;
                    _ = shutdown_rx.changed() => break,
                    res = window => {
                        let result = res
                            .map(|value| sample_lines(&value))
                            .map_err(|err| err.to_string());
                        if tx.send(AppEvent::JvmWatchTick { era, result }).is_err() {
                            break;
                        }
                    }
                }
            }
        });
    }
}

/// Stop the live watch (toggle-off, screen exit, profile switch):
/// signal the watch and release the busy slot (the tail's stop
/// shape).
pub fn stop_jvm_watch(state: &mut AppState) {
    if let Some(shutdown) = state.jvm.watch_shutdown.take() {
        let _ = shutdown.send(true);
    }
    if state.jvm.in_flight == Some("jvm watch (live)") {
        state.jvm.in_flight = None;
    }
}

/// `f` — start the duration-capped flight recording (the CLI's
/// defaults: 60 s, `default` settings). The gate rides down.
pub fn spawn_jvm_jfr_start(state: &mut AppState) {
    let Some(client) = client_arc(state) else {
        return;
    };
    let Some(profile) = state.profile.clone() else {
        return;
    };
    let risk_accepted = state.jvm.acknowledged;
    spawn_jvm_action(
        state,
        "jvm jfr start",
        async move {
            let config = load_config().await?;
            let opts = jvm::JvmJfrStartOptions::default();
            jvm::jvm_jfr_start(
                &*client,
                &config,
                &profile,
                JVM_PROJECT,
                &opts,
                risk_accepted,
            )
            .await
        },
        |result| {
            let mut lines = vec![format!(
                "started flight recording '{}' ({}s, settings={}) — memory-buffered, auto-stops",
                result.name, result.duration_secs, result.settings
            )];
            if !result.detail.is_empty() {
                lines.push(format!("jfr: {}", result.detail.trim()));
            }
            lines.push("fetch + clean up: F · check: r".to_string());
            lines
        },
    );
}

/// `F` — stop + dump + fetch + remote cleanup (ungated: stopping
/// REDUCES risk and must never be refused).
pub fn spawn_jvm_jfr_stop(state: &mut AppState) {
    let Some(client) = client_arc(state) else {
        return;
    };
    let Some(profile) = state.profile.clone() else {
        return;
    };
    spawn_jvm_action(
        state,
        "jvm jfr stop",
        async move {
            let config = load_config().await?;
            jvm::jvm_jfr_stop(&*client, &config, &profile, JVM_PROJECT, None).await
        },
        |result| {
            let mut lines = vec![format!("stopped flight recording '{}'", result.name)];
            if result.fetched {
                lines.push(format!(
                    "wrote {} ({} bytes)",
                    result.file.as_deref().unwrap_or_default(),
                    result.bytes
                ));
                lines.push(format!(
                    "gateway-side {}: {}",
                    result.remote_file,
                    if result.remote_deleted {
                        "deleted"
                    } else {
                        "NOT deleted — remove it manually"
                    }
                ));
            } else {
                lines.push(format!(
                    "dump is {} bytes — over the 32 MiB fetch cap",
                    result.bytes
                ));
                lines.push(format!(
                    "LEFT ON THE GATEWAY at {} — retrieve + delete it manually",
                    result.remote_file
                ));
            }
            lines
        },
    );
}
