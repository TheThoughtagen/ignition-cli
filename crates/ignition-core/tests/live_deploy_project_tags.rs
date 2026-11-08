//! Phase 26's rig gate for project-owned tags (26-01, the phase tracer): a
//! native tag body goes through the [`ProjectGateway`] seam and the real
//! client into `POST /data/api/v1/tags/import` on an Ignition 8.3.6 rig, and
//! comes back in the provider's REST export. Nothing here is mocked.
//!
//! ```text
//! cd crates/ignition-core/tests/fixtures/live-rig-deploy
//! bash capture23.sh up23 && bash capture23.sh setup23   # a fresh rig, cap-a minted
//! . ~/.cache/ign-deploy-rig/tokens.env
//! IGNITION_LIVE_DEPLOY=1 IGNITION_LIVE_URL=http://localhost:18290 \
//!   IGNITION_LIVE_TOKEN="$IGN_TOKEN_CAP_A" \
//!   cargo test -p ignition-core --test live_deploy_project_tags -- --ignored --nocapture --test-threads=1
//! ```
//!
//! | var | meaning |
//! |---|---|
//! | `IGNITION_LIVE_DEPLOY` | must equal `1`; anything else is a quiet green skip |
//! | `IGNITION_LIVE_URL` | the rig's base URL; loopback hosts only (`localhost`, `127.0.0.1`, `::1`) |
//! | `IGNITION_LIVE_TOKEN` | cap-a's full `name:key` API token (never printed) |
//!
//! Safety: the URL guard refuses a non-loopback host before a request is made
//! and a name guard refuses any gateway whose `gateway-info` name is not
//! `ign-deploy-rig`. The gate makes one disposable provider, `P26Trace`, and
//! deletes it on entry (a leftover of a cut-off run) and on exit, even when an
//! assertion fails. Output carries names and counts, never a token.

use std::time::Duration;

use ignition_core::actions::deploy_projects::{GatewayProjects, ProjectGateway};
use ignition_core::client::tags::{CollisionPolicy, TagProviderCreate};
use ignition_core::client::{GatewayApi, ReqwestGatewayApi};
use ignition_core::config::{Credential, Secret};
use ignition_core::error::CoreError;

/// The only gateway this gate may talk to.
const RIG_GATEWAY_NAME: &str = "ign-deploy-rig";

/// The disposable provider the tracer makes and removes.
const TRACE_PROVIDER: &str = "P26Trace";

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
        .unwrap_or_else(|| panic!("{name} must be set to run the live project tags gate"))
}

#[test]
fn local_url_guard_refuses_non_loopback() {
    for raw in [
        "http://192.0.2.15:8088",
        "https://gwnode01.example.local:8043",
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

#[derive(Clone)]
struct Rig {
    url: String,
    token: String,
}

impl Rig {
    fn api(&self) -> ReqwestGatewayApi {
        ReqwestGatewayApi::for_tests(
            &self.url,
            Some(Credential::Token(Secret::new(self.token.clone()))),
        )
    }

    /// Delete `provider` if the gateway has it.
    async fn remove_provider(&self, provider: &str) {
        let api = self.api();
        match api.tag_provider_find(provider).await {
            Ok(record) => {
                let signature = record.signature.unwrap_or_default();
                if let Err(err) = api.tag_provider_delete(provider, &signature).await {
                    eprintln!("cleanup: cannot delete provider {provider}: {}", err.code());
                }
            }
            Err(CoreError::NotFound { .. }) => {}
            Err(err) => eprintln!("cleanup: cannot find provider {provider}: {}", err.code()),
        }
    }
}

/// `None` is the quiet green skip. Every guard runs before the first request.
async fn rig() -> Option<Rig> {
    if std::env::var("IGNITION_LIVE_DEPLOY").as_deref() != Ok("1") {
        skip("IGNITION_LIVE_DEPLOY is not 1");
        return None;
    }
    let url = require_env("IGNITION_LIVE_URL");
    let token = require_env("IGNITION_LIVE_TOKEN");
    if let Err(why) = local_url_guard(&url) {
        panic!("IGNITION_LIVE_URL refused: {why}");
    }
    let rig = Rig { url, token };
    let info = GatewayApi::gateway_info(&rig.api())
        .await
        .expect("gateway-info read for the name guard");
    assert_eq!(
        info.name.as_deref(),
        Some(RIG_GATEWAY_NAME),
        "refusing: this gateway is not the {RIG_GATEWAY_NAME} fixture"
    );
    Some(rig)
}

/// Deletes the disposable provider when the test ends, even by a panic. The
/// delete runs on its own thread and runtime, since `Drop` cannot await.
struct ProviderGuard {
    rig: Rig,
    provider: &'static str,
}

impl Drop for ProviderGuard {
    fn drop(&mut self) {
        let rig = self.rig.clone();
        let provider = self.provider;
        let joined = std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("cleanup runtime")
                .block_on(rig.remove_provider(provider));
        })
        .join();
        if joined.is_err() {
            eprintln!("cleanup: the provider delete thread panicked");
        }
    }
}

/// Find the node at `path` (slash-separated names) in a native export tree.
fn node_at<'a>(tree: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut node = tree;
    for segment in path.split('/') {
        node =
            node.get("tags")?.as_array()?.iter().find(|child| {
                child.get("name").and_then(serde_json::Value::as_str) == Some(segment)
            })?;
    }
    Some(node)
}

/// The tracer: one folder and one memory tag imported with `Abort` at the
/// provider root through `ProjectGateway::tag_import`, a clean answer, and the
/// tag with its value in the provider's REST export straight after.
#[tokio::test]
#[ignore = "needs the 8.3.6 rig: IGNITION_LIVE_DEPLOY=1"]
async fn import_round_trip() {
    let Some(rig) = rig().await else { return };
    eprintln!("LIVE-RAN import_round_trip");
    rig.remove_provider(TRACE_PROVIDER).await;
    let api = rig.api();
    api.tag_provider_create(&[TagProviderCreate::standard(TRACE_PROVIDER)])
        .await
        .expect("the disposable provider is created");
    let _guard = ProviderGuard {
        rig: rig.clone(),
        provider: TRACE_PROVIDER,
    };

    let gateway = GatewayProjects(&api);
    // A freshly created provider may take a moment to list.
    let started = std::time::Instant::now();
    while !gateway
        .tag_provider_names()
        .await
        .expect("provider list")
        .iter()
        .any(|name| name == TRACE_PROVIDER)
    {
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "{TRACE_PROVIDER} never listed"
        );
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    let body = serde_json::json!({"tags": [{
        "name": "F",
        "tagType": "Folder",
        "tags": [{
            "name": "T1",
            "tagType": "AtomicTag",
            "dataType": "Int4",
            "valueSource": "memory",
            "value": 7
        }]
    }]});
    let answer = gateway
        .tag_import(
            TRACE_PROVIDER,
            "",
            CollisionPolicy::Abort,
            serde_json::to_vec(&body).expect("body"),
        )
        .await
        .expect("the import answers");
    eprintln!(
        "import answer: success_count={:?} failure_count={:?} bad_quality={:?}",
        answer.success_count, answer.failure_count, answer.bad_quality
    );
    assert!(answer.is_clean(), "the import answer is clean: {answer:?}");

    let export: serde_json::Value = serde_json::from_slice(
        &gateway
            .tag_export(TRACE_PROVIDER)
            .await
            .expect("the provider exports"),
    )
    .expect("the export is JSON");
    let tag = node_at(&export, "F/T1").expect("F/T1 is in the export straight after the answer");
    assert_eq!(tag["tagType"], "AtomicTag");
    assert_eq!(tag["value"], 7, "the export carries the imported value");
    eprintln!("export holds F/T1 with value 7");
}
