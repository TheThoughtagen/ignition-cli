//! The shared differential scenario (20-05, RESEARCH Pattern 4).
//!
//! `scenario` drives two `DeployApi` handles through one fixed sequence and
//! returns a trace of short tags, one per step. Run against two
//! `FakeDeploy` handles it must equal the expected trace derived from the
//! 20-LIVE-CAPTURES Decisions; 20-06 runs the SAME function against the
//! live gateway and diffs the two traces, so a place where the fake and the
//! gateway part ways shows up as a differing tag, not a guess.
//!
//! A step never panics: an unexpected `Err` becomes `<step>:err:<code>` and
//! the scenario carries on, so one diff shows every divergence. Consecutive
//! identical tags collapse.

use std::time::{Duration, Instant};

use ignition_core::client::DeployApi;
use ignition_core::client::config_resources::{ResourceCreate, ResourceUpdate};
use ignition_core::client::deploy_api::scan_and_wait;
use ignition_core::client::mode::ModeCreate;
use ignition_core::client::scan::{LockObservation, ScanKind, ScanOutcome};
use ignition_core::error::{CoreError, SignatureReason};
use ignition_core::poll::PollConfig;

/// The captured `config` of a database connection (C3a), shared with the
/// live-rig fixtures.
const DB_CONFIG: &str = include_str!("../fixtures/live-rig-deploy/db-connection-config.json");

/// The clocks of one scenario run.
pub struct ScenarioTiming {
    /// Wait between lock-expiry reads.
    pub poll: Duration,
    /// `holdTimeout` of the scenario's lock, in seconds.
    pub lock_hold_s: u64,
    /// How long the lock may take to expire before the step fails.
    pub expiry_budget: Duration,
    /// The scan wait.
    pub scan: PollConfig,
}

impl ScenarioTiming {
    /// Fast clocks for the in-memory fake.
    pub fn fake() -> Self {
        Self {
            poll: Duration::from_millis(1),
            lock_hold_s: 3,
            expiry_budget: Duration::from_secs(1),
            scan: PollConfig {
                subject: "scenario scan".to_string(),
                interval: Duration::from_millis(1),
                max: Duration::from_millis(5),
                deadline: Duration::from_millis(500),
            },
        }
    }

    /// Real-time clocks for a live gateway (20-06). The hold is long
    /// enough that a 2 s stall between the acquire and the next call (C1g
    /// recorded a 1.92 s acquire on the emulated rig) cannot lapse it
    /// before the held-then-refused steps have run (20-REVIEW WR-05); the
    /// expiry budget covers the whole hold with room to spare.
    pub fn live() -> Self {
        Self {
            poll: Duration::from_secs(1),
            lock_hold_s: 12,
            expiry_budget: Duration::from_secs(30),
            scan: PollConfig {
                subject: "scenario scan".to_string(),
                interval: Duration::from_millis(500),
                max: Duration::from_secs(1),
                deadline: Duration::from_secs(60),
            },
        }
    }
}

/// The scenario's error tag: `<step>:err:<code>`.
fn err_tag(step: &str, err: &CoreError) -> String {
    format!("{step}:err:{}", err.code())
}

/// Run the scenario on `ScanKind::Config`. `a` and `b` are two different
/// API tokens of one gateway; `a_token_name` is the token name inside `a`'s
/// actor string; `suffix` makes the created names unique per run.
pub async fn scenario(
    a: &dyn DeployApi,
    b: &dyn DeployApi,
    a_token_name: &str,
    suffix: &str,
    t: &ScenarioTiming,
) -> Vec<String> {
    let mut trace: Vec<String> = Vec::new();
    let mut push = |tag: String| {
        if trace.last() != Some(&tag) {
            trace.push(tag);
        }
    };
    let a_marker = format!(":{a_token_name}:");
    let kind = ScanKind::Config;

    // a takes the lock (D1: the acquire answers at once).
    match a.scan_lock_acquire(kind, 2, t.lock_hold_s).await {
        Ok(()) => push("lock:acquired".to_string()),
        Err(err) => push(err_tag("lock", &err)),
    }

    // a reads it back: held, by a (D9: the info names the actor; C1c).
    match a.scan_lock_info(kind).await {
        Ok(LockObservation::Held(info)) => {
            let by_a = info.actor.as_deref().is_some_and(|x| x.contains(&a_marker));
            push(if by_a {
                "lock:held-by-a".to_string()
            } else {
                "lock:held-by-other".to_string()
            });
        }
        Ok(LockObservation::Free) => push("lock:free-unexpected".to_string()),
        Err(err) => push(err_tag("lock-info", &err)),
    }

    // b is refused and told who holds it (D1: immediate 409 naming the holder).
    match b.scan_lock_acquire(kind, 2, t.lock_hold_s).await {
        Err(CoreError::ScanLockHeld { actor, .. }) => {
            let by_a = actor.as_deref().is_some_and(|x| x.contains(&a_marker));
            push(if by_a {
                "lock:refused-held-by-a".to_string()
            } else {
                "lock:refused-held-by-other".to_string()
            });
        }
        Err(err) => push(err_tag("lock-b", &err)),
        Ok(()) => push("lock:b-acquired-unexpected".to_string()),
    }

    // a waits for the hold to lapse (D9: 204 once holdTimeout is over).
    let started = Instant::now();
    loop {
        match a.scan_lock_info(kind).await {
            Ok(LockObservation::Free) => {
                push("lock:free".to_string());
                break;
            }
            Ok(LockObservation::Held(_)) if started.elapsed() < t.expiry_budget => {
                tokio::time::sleep(t.poll).await;
            }
            Ok(LockObservation::Held(_)) => {
                push("lock:expiry-timeout".to_string());
                break;
            }
            Err(err) => {
                push(err_tag("lock-expiry", &err));
                break;
            }
        }
    }

    // A scan completes by the shared predicate (D4).
    match scan_and_wait(a, kind, t.scan.clone()).await {
        Ok(ScanOutcome::Complete(_)) => push("scan:complete".to_string()),
        Ok(ScanOutcome::NotComplete { .. }) => push("scan:not-complete".to_string()),
        Err(err) => push(err_tag("scan", &err)),
    }

    let name = format!("IGN_SCENARIO_{suffix}");
    let config: serde_json::Value = serde_json::from_str(DB_CONFIG).expect("fixture config");

    // D10: create needs only name and config.
    match a
        .resource_create(
            "ignition",
            "database-connection",
            &[ResourceCreate::new(name.clone(), config.clone())],
        )
        .await
    {
        Ok(outcome) if outcome.success => push("create:ok".to_string()),
        Ok(_) => push("create:not-success".to_string()),
        Err(err) => push(err_tag("create", &err)),
    }

    // The effective view, read before the writes (C3a "GET find").
    let found = a
        .resource_find("ignition", "database-connection", &name, None)
        .await;
    let old_signature = match &found {
        Ok(record) => match record.signature.clone() {
            Some(signature) => {
                push("find:ok".to_string());
                Some(signature)
            }
            None => {
                push("find:no-signature".to_string());
                None
            }
        },
        Err(err) => {
            push(err_tag("find", err));
            None
        }
    };

    if let Some(signature) = old_signature {
        let mut changed = config.clone();
        changed["poolMaxActive"] = serde_json::json!(9);
        let update =
            |signature: &str| ResourceUpdate::new(name.clone(), signature, changed.clone());

        // D3 + D10: the current signature is accepted.
        match update(&signature) {
            Ok(item) => match a
                .resource_update("ignition", "database-connection", &[item])
                .await
            {
                Ok(outcome) if outcome.success => push("update:ok".to_string()),
                Ok(_) => push("update:not-success".to_string()),
                Err(err) => push(err_tag("update", &err)),
            },
            Err(err) => push(err_tag("update", &err)),
        }

        // D3: the write rotated the signature, so the old one is stale.
        match update(&signature) {
            Ok(item) => match a
                .resource_update("ignition", "database-connection", &[item])
                .await
            {
                Err(CoreError::ResourceSignatureRejected {
                    reason: SignatureReason::Stale,
                    ..
                }) => push("update:stale".to_string()),
                Err(err) => push(err_tag("update-stale", &err)),
                Ok(_) => push("update:stale-accepted-unexpected".to_string()),
            },
            Err(err) => push(err_tag("update-stale", &err)),
        }
    }

    // D10: the duplicate create is a 409, typed `resource_exists`, never Ok.
    match a
        .resource_create(
            "ignition",
            "database-connection",
            &[ResourceCreate::new(name, config)],
        )
        .await
    {
        Ok(_) => push("create-duplicate:ok-unexpected".to_string()),
        Err(err) => push(format!("create-duplicate:{}", err.code())),
    }

    // D6: a mode create is confirmed by the list.
    let mode_name = format!("ignscenario{suffix}");
    match a
        .mode_create(&ModeCreate {
            name: mode_name.clone(),
            title: "Scenario".to_string(),
            description: "ign-cli deploy scenario".to_string(),
        })
        .await
    {
        Ok(_) => push("mode:created".to_string()),
        Err(err) => push(err_tag("mode", &err)),
    }
    // D6: the same mode again is a 409, typed `mode_exists`, never Ok.
    match a
        .mode_create(&ModeCreate {
            name: mode_name.clone(),
            title: "Scenario".to_string(),
            description: "ign-cli deploy scenario".to_string(),
        })
        .await
    {
        Ok(_) => push("mode-duplicate:ok-unexpected".to_string()),
        Err(err) => push(format!("mode-duplicate:{}", err.code())),
    }
    match a.modes().await {
        Ok(page) if page.items.iter().any(|mode| mode.name == mode_name) => {
            push("modes:listed".to_string());
        }
        Ok(_) => push("modes:missing".to_string()),
        Err(err) => push(err_tag("modes", &err)),
    }

    // D5 + T-20-26: defining a mode does not activate it.
    match a.deployment_mode().await {
        Ok(None) => push("deployment-mode:none".to_string()),
        Ok(Some(_)) => push("deployment-mode:some".to_string()),
        Err(err) => push(err_tag("deployment-mode", &err)),
    }

    trace
}
