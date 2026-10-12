//! Phase 20's live gate (20-06): the `DeployApi` methods listed below against
//! a real, disposable Ignition 8.3.6 gateway on localhost, plus a check that
//! the `FakeDeploy` test double behaves like that gateway.
//!
//! The recorded green run (`20-06-live-gate.log`) predates sections C2 and
//! C3 (filtered list, singleton, per-collection list, `from_record` updates,
//! the `projects` scan kind): they are written but have not been run against
//! a gateway yet (20-REVIEW WR-03, WR-04). Run the gate again before
//! trusting them.
//!
//! The wiremock pins replay captured bytes. This file is the only place the
//! same methods run against a live gateway in one ordered run, and where the
//! shared 20-05 scenario runs on both the gateway and `FakeDeploy` so the two
//! traces can be compared tag for tag.
//!
//! ```text
//! IGNITION_LIVE_DEPLOY=1 IGNITION_LIVE_URL=http://localhost:18290 \
//!   IGNITION_LIVE_TOKEN="$IGN_TOKEN_CAP_A" IGNITION_LIVE_TOKEN_B="$IGN_TOKEN_CAP_B" \
//!   cargo test -p ignition-core --test live_deploy_api -- --ignored --nocapture
//! ```
//!
//! | var | meaning |
//! |---|---|
//! | `IGNITION_LIVE_DEPLOY` | must equal `1`; anything else is a quiet green skip |
//! | `IGNITION_LIVE_URL` | the rig's base URL; loopback hosts only (`localhost`, `127.0.0.1`, `::1`) |
//! | `IGNITION_LIVE_TOKEN` | cap-a's full `name:key` API token |
//! | `IGNITION_LIVE_TOKEN_B` | cap-b's full `name:key` API token |
//!
//! ## Rig recipe
//!
//! ```bash
//! D=crates/ignition-core/tests/fixtures/live-rig-deploy
//! export IGN_DEPLOY_RIG_STATE=$HOME/.cache/ign-deploy-rig   # outside the repo
//! docker compose -p ign-deploy-rig -f $D/compose.yml up -d
//! #   (add -f $D/compose.bind.yml and IGN_DEPLOY_RIG_RESOURCES=$IGN_DEPLOY_RIG_STATE/resources
//! #    for the bind-mounted variant; 20-LIVE-CAPTURES D11 says both boot)
//! until curl -s http://127.0.0.1:18290/StatusPing | grep -q RUNNING; do sleep 5; done
//! bash $D/mint-token.sh cap-a cap-b        # restarts the gateway; wait for RUNNING again
//! source $IGN_DEPLOY_RIG_STATE/tokens.env  # exports IGN_TOKEN_CAP_A and IGN_TOKEN_CAP_B
//! # ... the run command above ...
//! docker compose -p ign-deploy-rig down -v
//! ```
//!
//! The gate is ONE ordered test because state carries between sections: the
//! lock sections need an idle gateway, the mode section must run before the
//! resource section (which writes into the `production` collection), and the
//! differential runs last on names unique to the run.
//!
//! Safety: the URL guard refuses any non-loopback host before a request is
//! made, and a name guard refuses any gateway whose `gateway-info` name is not
//! `ign-deploy-rig` before a write is made. Output carries section results
//! only, never a token or a signature.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ignition_core::client::config_resources::{ResourceCreate, ResourceUpdate};
use ignition_core::client::deploy_api::{resource_list_in_collection, scan_and_wait};
use ignition_core::client::mode::ModeCreate;
use ignition_core::client::query::ListQuery;
use ignition_core::client::scan::{
    LockObservation, LockVerdict, ScanKind, ScanOutcome, lock_verdict,
};
use ignition_core::client::{DeployApi, GatewayApi, ReqwestGatewayApi};
use ignition_core::config::{Credential, Secret};
use ignition_core::error::{CoreError, SignatureReason};
use ignition_core::poll::PollConfig;

mod common;

use common::deploy_scenario::{ScenarioTiming, scenario};
use common::fake_deploy::FakeGateway;

/// The only gateway this gate may write to.
const RIG_GATEWAY_NAME: &str = "ign-deploy-rig";

/// The captured `config` of a database connection (C3a).
const DB_CONFIG: &str = include_str!("fixtures/live-rig-deploy/db-connection-config.json");

fn skip(message: &str) {
    eprintln!("skipping: {message}");
}

/// Parse `raw` and accept it only when it points at this machine: an http or
/// https URL whose host is `localhost`, `127.0.0.1` or `::1`. The check is on
/// the parsed host, so `localhost.evil.example` and `127.0.0.2` are refused.
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

fn epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_secs()
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

#[tokio::test]
#[ignore = "live: needs the ign-deploy-rig fixture and IGNITION_LIVE_DEPLOY=1"]
async fn live_deploy_api_end_to_end() {
    if std::env::var("IGNITION_LIVE_DEPLOY").as_deref() != Ok("1") {
        skip("IGNITION_LIVE_DEPLOY is not 1");
        return;
    }
    let url = require_env("IGNITION_LIVE_URL");
    let token_a = require_env("IGNITION_LIVE_TOKEN");
    let token_b = require_env("IGNITION_LIVE_TOKEN_B");
    // Before any request.
    if let Err(why) = local_url_guard(&url) {
        panic!("IGNITION_LIVE_URL refused: {why}");
    }
    let a = ReqwestGatewayApi::for_tests(&url, Some(Credential::Token(Secret::new(token_a))));
    let b = ReqwestGatewayApi::for_tests(&url, Some(Credential::Token(Secret::new(token_b))));

    // Name guard: a read, before any write.
    let info = GatewayApi::gateway_info(&a)
        .await
        .expect("gateway-info read for the name guard");
    assert_eq!(
        info.name.as_deref(),
        Some(RIG_GATEWAY_NAME),
        "refusing to write: this gateway is not the {RIG_GATEWAY_NAME} fixture"
    );
    eprintln!("PASS guard: loopback URL, gateway name is {RIG_GATEWAY_NAME}");

    let kind = ScanKind::Config;

    // Section A: the scan-lock.
    // A hold of 12 s, not 4: C1g recorded a 1.92 s acquire on this emulated
    // rig, and a stall that long between the acquire and the next call must
    // not let the hold lapse before the held-then-refused steps run
    // (20-REVIEW WR-05).
    const LOCK_HOLD_S: u64 = 12;
    a.scan_lock_acquire(kind, 10, LOCK_HOLD_S)
        .await
        .expect("A: cap-a acquires the lock");
    match a.scan_lock_info(kind).await.expect("A: lock info") {
        LockObservation::Held(held) => {
            let actor = held.actor.clone().unwrap_or_default();
            assert!(
                actor.contains(":cap-a:"),
                "A: holder is cap-a, got {actor:?}"
            );
            assert!(
                (1..=LOCK_HOLD_S as i64).contains(&held.remaining),
                "A: remaining {} outside 1..={LOCK_HOLD_S}",
                held.remaining
            );
        }
        LockObservation::Free => panic!("A: lock reads Free right after the acquire"),
    }
    match b.scan_lock_acquire(kind, 2, LOCK_HOLD_S).await {
        Err(err @ CoreError::ScanLockHeld { .. }) => {
            assert_eq!(err.code(), "scan_lock_held");
            let CoreError::ScanLockHeld { actor, .. } = &err else {
                unreachable!()
            };
            assert!(
                actor.as_deref().is_some_and(|x| x.contains(":cap-a:")),
                "A: refusal names cap-a, got {actor:?}"
            );
        }
        other => panic!("A: cap-b acquire must be scan_lock_held, got {other:?}"),
    }
    let mut freed = false;
    for _ in 0..30 {
        let obs = a.scan_lock_info(kind).await.expect("A: expiry poll");
        if matches!(obs, LockObservation::Free) {
            assert_eq!(lock_verdict(&obs, 2), LockVerdict::NotHeld);
            freed = true;
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    assert!(freed, "A: the lock never read Free within 30 s");
    eprintln!("PASS A: lock held by cap-a, cap-b refused naming cap-a, then Free after expiry");

    // Section B: a scan completes by the shared predicate.
    let before = a.scan_status(kind).await.expect("B: status before");
    let outcome = scan_and_wait(
        &a,
        kind,
        PollConfig {
            subject: "live scan".to_string(),
            interval: Duration::from_millis(500),
            max: Duration::from_secs(2),
            deadline: Duration::from_secs(60),
        },
    )
    .await
    .expect("B: scan_and_wait");
    match outcome {
        ScanOutcome::Complete(status) => assert!(
            status.last_scan_timestamp > before.last_scan_timestamp,
            "B: lastScanTimestamp did not advance"
        ),
        ScanOutcome::NotComplete { waited, .. } => {
            panic!("B: scan not complete after {waited:?}")
        }
    }
    eprintln!("PASS B: scan_and_wait Complete, lastScanTimestamp advanced");

    // Section D: modes (before C, which writes into `production`).
    assert_eq!(
        a.deployment_mode().await.expect("D: deployment_mode"),
        None,
        "D: the rig must start with no active mode"
    );
    let created = a
        .mode_create(&ModeCreate {
            name: "production".to_string(),
            title: "Production".to_string(),
            description: "ign 20-06 live gate".to_string(),
        })
        .await
        .expect("D: mode_create");
    assert_eq!(created.name, "production");
    let listed = a.modes().await.expect("D: modes");
    assert!(
        listed.items.iter().any(|mode| mode.name == "production"),
        "D: the new mode is listed"
    );
    assert_eq!(
        a.deployment_mode().await.expect("D: deployment_mode after"),
        None,
        "D: creating a mode must not activate it"
    );
    eprintln!("PASS D: no active mode, production mode created and listed, still no active mode");

    // Section C: resources.
    let run = epoch_secs();
    let name = format!("IGN_LIVE_DB_{run}");
    let config: serde_json::Value = serde_json::from_str(DB_CONFIG).expect("fixture config");
    let outcome = a
        .resource_create(
            "ignition",
            "database-connection",
            &[ResourceCreate::new(name.clone(), config.clone())],
        )
        .await
        .expect("C: create");
    assert!(outcome.success, "C: create reported success:false");

    let s1 = a
        .resource_find("ignition", "database-connection", &name, None)
        .await
        .expect("C: find effective")
        .signature
        .expect("C: effective copy carries a signature");
    let core = a
        .resource_find("ignition", "database-connection", &name, Some("core"))
        .await
        .expect("C: find core");
    assert_eq!(core.name.as_deref(), Some(name.as_str()));

    a.resource_create(
        "ignition",
        "database-connection",
        &[ResourceCreate::new(name.clone(), config.clone()).with_collection("production")],
    )
    .await
    .expect("C: create in the production collection");
    let production = a
        .resource_find("ignition", "database-connection", &name, Some("production"))
        .await
        .expect("C: find production");
    assert_eq!(production.name.as_deref(), Some(name.as_str()));
    let effective = a
        .resource_find("ignition", "database-connection", &name, None)
        .await
        .expect("C: find effective again");
    assert_eq!(
        effective.signature.as_deref(),
        Some(s1.as_str()),
        "C: with no active mode the effective view is still the core copy"
    );

    let mut changed = config.clone();
    changed["poolMaxActive"] = serde_json::json!(9);
    let update = |signature: &str| {
        ResourceUpdate::new(name.clone(), signature, changed.clone()).expect("update item")
    };
    let updated = a
        .resource_update("ignition", "database-connection", &[update(&s1)])
        .await
        .expect("C: update with the current signature");
    assert!(updated.success, "C: update reported success:false");
    let s2 = updated
        .changes
        .first()
        .and_then(|change| change.new_signature.clone())
        .expect("C: update returns a new signature");
    assert_ne!(s2, s1, "C: a write rotates the signature");

    match a
        .resource_update("ignition", "database-connection", &[update(&s1)])
        .await
    {
        Err(CoreError::ResourceSignatureRejected {
            reason: SignatureReason::Stale,
            ..
        }) => {}
        other => panic!("C: a reused signature must be stale, got {other:?}"),
    }
    let after = a
        .resource_find("ignition", "database-connection", &name, None)
        .await
        .expect("C: find after the refusal");
    assert_eq!(
        after.signature.as_deref(),
        Some(s2.as_str()),
        "C: the refused write must leave the signature unchanged"
    );

    // WR-03: update from a record just read (`from_record` carries
    // `collection`, `enabled` and `backupConfig`, which no capture shows in a
    // PUT body), once per copy, with both copies present. The collection-less
    // PUT above must not have touched the production copy if the gateway
    // honours the effective view; what it did is logged, not asserted.
    let production_sig_before = production
        .signature
        .clone()
        .expect("C: production copy carries a signature");
    let production_now = a
        .resource_find("ignition", "database-connection", &name, Some("production"))
        .await
        .expect("C: find production after the collection-less update");
    eprintln!(
        "OBSERVED C: the collection-less PUT {} the production copy",
        if production_now.signature.as_deref() == Some(production_sig_before.as_str()) {
            "left"
        } else {
            "also changed"
        }
    );
    for (target, other, pool) in [("core", "production", 8), ("production", "core", 7)] {
        let record = a
            .resource_find("ignition", "database-connection", &name, Some(target))
            .await
            .unwrap_or_else(|err| panic!("C: find {target}: {err:?}"));
        let other_before = a
            .resource_find("ignition", "database-connection", &name, Some(other))
            .await
            .unwrap_or_else(|err| panic!("C: find {other}: {err:?}"))
            .signature;
        let mut from_record = ResourceUpdate::from_record(&record)
            .unwrap_or_else(|err| panic!("C: from_record {target}: {err:?}"));
        from_record.config["poolMaxActive"] = serde_json::json!(pool);
        let written = a
            .resource_update("ignition", "database-connection", &[from_record])
            .await
            .unwrap_or_else(|err| panic!("C: from_record update of {target}: {err:?}"));
        assert!(written.success, "C: from_record update of {target} failed");
        let target_after = a
            .resource_find("ignition", "database-connection", &name, Some(target))
            .await
            .unwrap_or_else(|err| panic!("C: find {target} after: {err:?}"));
        assert_ne!(
            target_after.signature, record.signature,
            "C: the {target} copy must rotate its signature"
        );
        let other_after = a
            .resource_find("ignition", "database-connection", &name, Some(other))
            .await
            .unwrap_or_else(|err| panic!("C: find {other} after: {err:?}"))
            .signature;
        assert_eq!(
            other_after, other_before,
            "C: updating the {target} copy must leave the {other} copy alone"
        );
    }

    let listed = a
        .resource_list("ignition", "database-connection", &ListQuery::default())
        .await
        .expect("C: list");
    assert!(
        listed
            .items
            .iter()
            .any(|item| item.name.as_deref() == Some(name.as_str())),
        "C: resource_list includes the name"
    );
    let names = a
        .resource_names("ignition", "database-connection")
        .await
        .expect("C: names");
    assert!(
        names.items.iter().any(|item| item.name == name),
        "C: resource_names includes the name"
    );
    // WR-04 (C2): the filtered list. The client sends `name[eq]=v` through
    // reqwest, which percent-encodes the brackets (`name%5Beq%5D=v`); D7 was
    // learned with literal brackets (curl -g). A filter that silently
    // matches nothing is the failure D7 recorded, so assert one match for
    // the created name and none for a name that does not exist.
    let by_name = |wanted: &str| ListQuery {
        filter: Some(format!("name[eq]={wanted}")),
        ..ListQuery::default()
    };
    let hit = a
        .resource_list("ignition", "database-connection", &by_name(&name))
        .await
        .expect("C2: filtered list");
    assert_eq!(
        hit.items.len(),
        1,
        "C2: name[eq] must match the created resource exactly once (percent-encoded brackets)"
    );
    let miss = a
        .resource_list(
            "ignition",
            "database-connection",
            &by_name("IGN_LIVE_NO_SUCH_NAME"),
        )
        .await
        .expect("C2: filtered list, no match");
    assert!(
        miss.items.is_empty(),
        "C2: name[eq] of an absent name matches nothing"
    );

    // WR-04 (C2): the D7 per-collection list and the singleton read.
    for collection in ["core", "production"] {
        let in_collection =
            resource_list_in_collection(&a, "ignition", "database-connection", collection)
                .await
                .unwrap_or_else(|err| panic!("C2: list in {collection}: {err:?}"));
        assert!(
            in_collection
                .iter()
                .any(|record| record.name.as_deref() == Some(name.as_str())),
            "C2: the {collection} collection lists the resource"
        );
    }
    for collection in [None, Some("core")] {
        let singleton = a
            .resource_singleton("ignition", "security-properties", collection)
            .await
            .unwrap_or_else(|err| panic!("C2: singleton {collection:?}: {err:?}"));
        assert_eq!(singleton.resource_type, "ignition/security-properties");
        assert!(singleton.name.is_none(), "C2: a singleton has no name");
        assert!(
            singleton.signature.is_some(),
            "C2: a singleton carries a signature"
        );
    }

    // WR-04 (C3): the `projects` scan kind is wiremock-pinned only; read it
    // live (no lock is taken and no scan is triggered).
    a.scan_status(ScanKind::Projects)
        .await
        .expect("C3: projects scan status");
    a.scan_lock_info(ScanKind::Projects)
        .await
        .expect("C3: projects scan-lock info");

    // D10: the duplicate create is a 409, typed `resource_exists` (exit 6).
    match a
        .resource_create(
            "ignition",
            "database-connection",
            &[ResourceCreate::new(name.clone(), config)],
        )
        .await
    {
        Err(err @ CoreError::ResourceExists { .. }) => {
            assert_eq!(
                err.exit_code(),
                6,
                "C: resource_exists is a target-state exit"
            );
        }
        other => panic!("C: a duplicate create must be resource_exists, got {other:?}"),
    }
    eprintln!(
        "PASS C: create, effective/core/production reads, update, stale refusal leaves signature, list, names, duplicate refused"
    );

    // Section E: the fake behaves like the gateway.
    let fake = FakeGateway::new();
    let (fa, fb) = (fake.client("cap-a"), fake.client("cap-b"));
    let suffix = format!("{run}");
    let live_trace = scenario(&a, &b, "cap-a", &suffix, &ScenarioTiming::live()).await;
    let fake_trace = scenario(&fa, &fb, "cap-a", &suffix, &ScenarioTiming::fake()).await;
    eprintln!("live trace: {live_trace:?}");
    eprintln!("fake trace: {fake_trace:?}");
    assert_eq!(
        live_trace, fake_trace,
        "E: the live trace and the FakeDeploy trace diverge\n live: {live_trace:?}\n fake: {fake_trace:?}"
    );
    eprintln!(
        "PASS E: live trace equals fake trace ({} steps)",
        live_trace.len()
    );

    // Section Z: leave the gateway scanned and idle (RESEARCH Pitfall 9).
    let done = scan_and_wait(
        &a,
        kind,
        PollConfig {
            subject: "live cleanup scan".to_string(),
            interval: Duration::from_millis(500),
            max: Duration::from_secs(2),
            deadline: Duration::from_secs(60),
        },
    )
    .await
    .expect("Z: cleanup scan");
    assert!(
        matches!(done, ScanOutcome::Complete(_)),
        "Z: cleanup scan completes"
    );
    eprintln!("PASS Z: cleanup scan complete");
}
