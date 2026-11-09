//! Phase 26's rig gates for the authenticated tag delete route (26-17, D-13):
//! the `cli/tagDelete` WebDev route is installed on the single-node 8.3.6 rig
//! and called over HTTP with Basic credentials and the route secret. Nothing
//! here is mocked.
//!
//! ```text
//! cd crates/ignition-core/tests/fixtures/live-rig-deploy
//! bash capture23.sh up23 && bash capture23.sh setup23   # a fresh rig, cap-a minted
//! . ~/.cache/ign-deploy-rig/tokens.env
//! IGNITION_LIVE_DEPLOY=1 IGNITION_LIVE_URL=http://localhost:18290 \
//!   IGNITION_LIVE_TOKEN="$IGN_TOKEN_CAP_A" \
//!   IGN_RIG_ADMIN_USER=admin IGN_RIG_ADMIN_PASSWORD=<compose.yml value> \
//!   cargo test -p ignition-core --test live_tag_delete_route -- --ignored --nocapture --test-threads=1
//! ```
//!
//! | var | meaning |
//! |---|---|
//! | `IGNITION_LIVE_DEPLOY` | must equal `1`; anything else is a quiet green skip |
//! | `IGNITION_LIVE_URL` | the rig's base URL; loopback hosts only (`localhost`, `127.0.0.1`, `::1`) |
//! | `IGNITION_LIVE_TOKEN` | cap-a's full `name:key` API token (never printed) |
//! | `IGN_RIG_ADMIN_USER` / `IGN_RIG_ADMIN_PASSWORD` | the rig's admin pair from its compose file (never printed) |
//!
//! Safety: the URL guard refuses a non-loopback host before a request is made
//! and a name guard refuses any gateway whose `gateway-info` name is not
//! `ign-deploy-rig`. Each gate makes one disposable provider, `P26Del`, and a
//! guard deletes it and re-imports the plain always-on bundle into `ign-cli`
//! when the test ends, even by a panic. Output carries names, codes and
//! counts, never a token, password or secret.

use std::time::{Duration, Instant};

use ignition_core::actions::deploy_projects::{GatewayProjects, ProjectGateway};
use ignition_core::client::tags::{CollisionPolicy, TagProviderCreate};
use ignition_core::client::webdev::{TagDeleteInstall, build_deploy_zip, build_deploy_zip_with};
use ignition_core::client::{GatewayApi, ReqwestGatewayApi};
use ignition_core::config::{Credential, Secret};
use ignition_core::error::CoreError;

/// The only gateway these gates may talk to.
const RIG_GATEWAY_NAME: &str = "ign-deploy-rig";

/// The disposable provider each gate makes and removes.
const PROVIDER: &str = "P26Del";

/// The CLI's WebDev project.
const PROJECT: &str = "ign-cli";

/// The route secret header.
const SECRET_HEADER: &str = "X-Ignition-CLI-Secret";

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
        .unwrap_or_else(|| panic!("{name} must be set to run the live tag delete gates"))
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

/// 32 random bytes as 64 lowercase hex, for a per-run route secret.
fn random_hex() -> String {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).expect("random bytes");
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Clone)]
struct Rig {
    url: String,
    token: String,
    admin_user: String,
    admin_password: String,
}

/// One raw route answer: the HTTP status and the body (JSON when it parses).
#[derive(Debug)]
struct Answer {
    status: u16,
    body: Option<serde_json::Value>,
}

impl Answer {
    /// The envelope's error code, when the body is a denial.
    fn code(&self) -> Option<&str> {
        self.body.as_ref()?.get("error")?.get("code")?.as_str()
    }

    fn data(&self) -> &serde_json::Value {
        let body = self.body.as_ref().expect("a JSON body");
        assert_eq!(body["ok"], true, "expected an ok envelope: {body}");
        &body["data"]
    }
}

/// How a route call authenticates.
enum Auth<'a> {
    None,
    Token,
    Basic(&'a str, &'a str),
}

impl Rig {
    fn api(&self) -> ReqwestGatewayApi {
        ReqwestGatewayApi::for_tests(
            &self.url,
            Some(Credential::Token(Secret::new(self.token.clone()))),
        )
    }

    fn route_url(&self, route: &str) -> String {
        format!(
            "{}/system/webdev/{PROJECT}/cli/{route}",
            self.url.trim_end_matches('/')
        )
    }

    /// POST `body` to the tagDelete route with `auth` and `headers`.
    async fn call(
        &self,
        auth: Auth<'_>,
        headers: &[(&str, &str)],
        body: &serde_json::Value,
    ) -> Answer {
        self.call_route("tagDelete", auth, headers, body).await
    }

    async fn call_route(
        &self,
        route: &str,
        auth: Auth<'_>,
        headers: &[(&str, &str)],
        body: &serde_json::Value,
    ) -> Answer {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("client");
        let mut request = client.post(self.route_url(route)).json(body);
        match auth {
            Auth::None => {}
            Auth::Token => request = request.header("X-Ignition-API-Token", &self.token),
            Auth::Basic(user, password) => request = request.basic_auth(user, Some(password)),
        }
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let response = request.send().await.expect("the route answers");
        let status = response.status().as_u16();
        let text = response.text().await.expect("body");
        Answer {
            status,
            body: serde_json::from_str(&text).ok(),
        }
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

    /// Re-import the plain always-on bundle (no scriptExec, no tagDelete).
    async fn reimport_plain_bundle(&self) {
        let zip = build_deploy_zip(PROJECT, false, None, false).expect("plain bundle packs");
        if let Err(err) = self.api().project_import(PROJECT, zip, true).await {
            eprintln!("cleanup: cannot re-import the plain bundle: {}", err.code());
        }
    }

    /// Create the disposable provider fresh and wait until it lists.
    async fn fresh_provider(&self) {
        self.remove_provider(PROVIDER).await;
        let api = self.api();
        api.tag_provider_create(&[TagProviderCreate::standard(PROVIDER)])
            .await
            .expect("the disposable provider is created");
        let gateway = GatewayProjects(&api);
        let started = Instant::now();
        while !gateway
            .tag_provider_names()
            .await
            .expect("provider list")
            .iter()
            .any(|name| name == PROVIDER)
        {
            assert!(
                started.elapsed() < Duration::from_secs(120),
                "{PROVIDER} never listed"
            );
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    /// Import a native tag body at the provider root with `Abort`.
    async fn import_tags(&self, body: serde_json::Value) {
        let api = self.api();
        let answer = GatewayProjects(&api)
            .tag_import(
                PROVIDER,
                "",
                CollisionPolicy::Abort,
                serde_json::to_vec(&body).expect("body"),
            )
            .await
            .expect("the import answers");
        assert!(answer.is_clean(), "the import answer is clean: {answer:?}");
    }

    /// The provider's REST export.
    async fn export(&self) -> serde_json::Value {
        let api = self.api();
        serde_json::from_slice(
            &GatewayProjects(&api)
                .tag_export(PROVIDER)
                .await
                .expect("the provider exports"),
        )
        .expect("the export is JSON")
    }

    /// Wait until the tagDelete `version` action answers 200 (retrying the
    /// 500 and 405 a freshly imported project gives, every 2 s up to 30 s).
    async fn wait_version(&self, user: &str, password: &str, secret: &str) -> Answer {
        let started = Instant::now();
        loop {
            let answer = self
                .call(
                    Auth::Basic(user, password),
                    &[(SECRET_HEADER, secret)],
                    &serde_json::json!({"action": "version"}),
                )
                .await;
            if answer.status == 200 && answer.body.is_some() {
                eprintln!("route ready after {:.1} s", started.elapsed().as_secs_f64());
                return answer;
            }
            assert!(
                matches!(answer.status, 500 | 405),
                "unexpected status while waiting for the route: {}",
                answer.status
            );
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "the tagDelete route never answered (last status {})",
                answer.status
            );
            tokio::time::sleep(Duration::from_secs(2)).await;
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
    let rig = Rig {
        url,
        token,
        admin_user: require_env("IGN_RIG_ADMIN_USER"),
        admin_password: require_env("IGN_RIG_ADMIN_PASSWORD"),
    };
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

/// Runs a cleanup future on its own thread and runtime, since `Drop` cannot
/// await.
fn block_on_cleanup<F>(future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let joined = std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("cleanup runtime")
            .block_on(future);
    })
    .join();
    if joined.is_err() {
        eprintln!("cleanup: the cleanup thread panicked");
    }
}

/// Deletes the provider and re-imports the plain bundle when a gate ends.
struct RouteGuard {
    rig: Rig,
}

impl Drop for RouteGuard {
    fn drop(&mut self) {
        let rig = self.rig.clone();
        block_on_cleanup(async move {
            rig.remove_provider(PROVIDER).await;
            rig.reimport_plain_bundle().await;
        });
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

fn memory_tag(name: &str) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "tagType": "AtomicTag",
        "dataType": "Int4",
        "valueSource": "memory",
        "value": 1
    })
}

/// The tracer: one tag deleted through the installed route with Basic admin
/// and the secret; its sibling stays.
#[tokio::test]
#[ignore = "needs the 8.3.6 rig: IGNITION_LIVE_DEPLOY=1"]
async fn tag_delete_route_tracer() {
    let Some(rig) = rig().await else { return };
    eprintln!("LIVE-RAN tag_delete_route_tracer");
    rig.fresh_provider().await;
    let _guard = RouteGuard { rig: rig.clone() };
    rig.import_tags(serde_json::json!({"tags": [{
        "name": "Scratch",
        "tagType": "Folder",
        "tags": [memory_tag("A"), memory_tag("B")]
    }]}))
    .await;

    let secret = random_hex();
    let install = TagDeleteInstall {
        secret: secret.clone(),
        user_source: "default".to_string(),
        role: "Administrator".to_string(),
        providers: vec![PROVIDER.to_string()],
    };
    let zip = build_deploy_zip_with(PROJECT, false, None, false, Some(&install)).expect("pack");
    rig.api()
        .project_import(PROJECT, zip, true)
        .await
        .expect("the route bundle imports");

    let version = rig
        .wait_version(&rig.admin_user, &rig.admin_password, &secret)
        .await;
    let data = version.data();
    assert_eq!(data["providers"], serde_json::json!([PROVIDER]));
    assert_eq!(data["maxPaths"], 500);
    assert_eq!(data["maxPathChars"], 1024);
    eprintln!(
        "version: routeVersion={} minCli={}",
        data["routeVersion"], data["minCli"]
    );

    let answer = rig
        .call(
            Auth::Basic(&rig.admin_user, &rig.admin_password),
            &[(SECRET_HEADER, &secret)],
            &serde_json::json!({"action": "delete", "provider": PROVIDER, "paths": ["Scratch/A"]}),
        )
        .await;
    assert_eq!(answer.status, 200);
    let data = answer.data();
    let results = data["results"].as_array().expect("results");
    assert_eq!(results.len(), 1, "one result per path: {data}");
    assert_eq!(results[0]["path"], "Scratch/A");
    eprintln!(
        "delete Scratch/A: quality={} good={} existsAfter={}",
        results[0]["quality"], results[0]["good"], results[0]["existsAfter"]
    );

    let export = rig.export().await;
    assert!(
        node_at(&export, "Scratch/A").is_none(),
        "Scratch/A is gone from the export"
    );
    assert!(
        node_at(&export, "Scratch/B").is_some(),
        "Scratch/B is still in the export"
    );
    eprintln!("export lacks Scratch/A and holds Scratch/B");
}
