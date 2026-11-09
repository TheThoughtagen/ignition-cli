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
use ignition_core::actions::webdev::{TagDeleteRequest, webdev_deploy_with};
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

/// The deploy role and user the matrix creates (neutral, rig-only names).
const DEPLOY_ROLE: &str = "P26TagDeploy";
const DEPLOY_USER: &str = "p26deploy";

/// Every node path in a native export tree (slash-separated).
fn export_paths(tree: &serde_json::Value) -> std::collections::BTreeSet<String> {
    fn walk(node: &serde_json::Value, prefix: &str, out: &mut std::collections::BTreeSet<String>) {
        let Some(children) = node.get("tags").and_then(serde_json::Value::as_array) else {
            return;
        };
        for child in children {
            let Some(name) = child.get("name").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let path = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{prefix}/{name}")
            };
            out.insert(path.clone());
            walk(child, &path, out);
        }
    }
    let mut out = std::collections::BTreeSet::new();
    walk(tree, "", &mut out);
    out
}

/// Runs code on the rig through the installed scriptExec route.
#[derive(Clone)]
struct Exec {
    rig: Rig,
    secret: String,
}

impl Exec {
    async fn run(&self, code: &str) -> serde_json::Value {
        let answer = self
            .rig
            .call_route(
                "scriptExec",
                Auth::None,
                &[(SECRET_HEADER, &self.secret)],
                &serde_json::json!({"action": "exec", "code": code}),
            )
            .await;
        assert_eq!(answer.status, 200, "scriptExec answers 200");
        answer.data()["result"].clone()
    }

    async fn wait_ready(&self) {
        let started = Instant::now();
        loop {
            let answer = self
                .rig
                .call_route(
                    "scriptExec",
                    Auth::None,
                    &[(SECRET_HEADER, &self.secret)],
                    &serde_json::json!({"action": "version"}),
                )
                .await;
            if answer.status == 200 && answer.body.is_some() {
                return;
            }
            assert!(
                matches!(answer.status, 500 | 405) && started.elapsed() < Duration::from_secs(30),
                "scriptExec never answered (last status {})",
                answer.status
            );
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    /// `system.tag.exists` for a path under the provider.
    async fn exists(&self, path: &str) -> bool {
        let code = format!(
            "import system\n_result = bool(system.tag.exists({}))\n",
            serde_json::to_string(&format!("[{PROVIDER}]{path}")).expect("path literal")
        );
        self.run(&code).await.as_bool().expect("a boolean")
    }

    /// Create the deploy role and a user holding only it. The password is
    /// per-run and never printed. Answers the errors the gateway reported.
    async fn create_deploy_user(&self, password: &str) -> serde_json::Value {
        let code = format!(
            "import system\n\
             r1 = system.user.addRole('default', '{DEPLOY_ROLE}')\n\
             u = system.user.getNewUser('default', '{DEPLOY_USER}')\n\
             u.set('password', '{password}')\n\
             u.addRole('{DEPLOY_ROLE}')\n\
             r2 = system.user.addUser('default', u)\n\
             got = system.user.getUser('default', '{DEPLOY_USER}')\n\
             _result = {{'roleErrors': [str(e) for e in r1.getErrors()], \
             'userErrors': [str(e) for e in r2.getErrors()], \
             'roles': [str(x) for x in (got.getRoles() if got is not None else [])]}}\n"
        );
        self.run(&code).await
    }

    async fn remove_deploy_user(&self) {
        let code = format!(
            "import system\n\
             try:\n\
             \tsystem.user.removeUser('default', '{DEPLOY_USER}')\n\
             except:\n\
             \tpass\n\
             try:\n\
             \tsystem.user.removeRole('default', '{DEPLOY_ROLE}')\n\
             except:\n\
             \tpass\n\
             _result = system.user.getUser('default', '{DEPLOY_USER}') is None\n"
        );
        let gone = self.run(&code).await;
        eprintln!("cleanup: deploy user removed={gone}");
    }
}

/// Removes the user and role, then the provider and the routes.
struct MatrixGuard {
    exec: Exec,
}

impl Drop for MatrixGuard {
    fn drop(&mut self) {
        let exec = self.exec.clone();
        block_on_cleanup(async move {
            exec.remove_deploy_user().await;
            exec.rig.remove_provider(PROVIDER).await;
            exec.rig.reimport_plain_bundle().await;
        });
    }
}

/// One matrix row's record line (no secret, password or token).
fn row(name: &str, answer: &Answer) -> String {
    let line = format!(
        "row {name}: http={} code={}",
        answer.status,
        answer.code().unwrap_or("-")
    );
    eprintln!("{line}");
    line
}

/// The matrix: install through `webdev_deploy_with` (scriptExec and
/// tagDelete in one import), create a deploy user through scriptExec, prove
/// every auth and scope layer refuses without deleting anything, then the
/// positive rows (folder, UDT instance, absent path, 500 paths).
#[tokio::test]
#[ignore = "needs the 8.3.6 rig: IGNITION_LIVE_DEPLOY=1"]
async fn tag_delete_route_matrix() {
    let Some(rig) = rig().await else { return };
    eprintln!("LIVE-RAN tag_delete_route_matrix");
    rig.fresh_provider().await;
    let _provider_guard = RouteGuard { rig: rig.clone() };

    // Tags: a keeper, a folder of two, a UDT instance, 500 bulk tags.
    rig.import_tags(serde_json::json!({"tags": [{
        "name": "_types_",
        "tagType": "Folder",
        "tags": [{
            "name": "Ty",
            "tagType": "UdtType",
            "tags": [
                {"name": "M1", "tagType": "AtomicTag", "dataType": "Int4", "valueSource": "memory", "value": 0},
                {"name": "M2", "tagType": "AtomicTag", "dataType": "Int4", "valueSource": "memory", "value": 0}
            ]
        }]
    }]}))
    .await;
    let bulk: Vec<serde_json::Value> = (0..500).map(|i| memory_tag(&format!("T{i:03}"))).collect();
    rig.import_tags(serde_json::json!({"tags": [
        memory_tag("Keep"),
        {"name": "Fold", "tagType": "Folder", "tags": [memory_tag("X"), memory_tag("Y")]},
        {"name": "Inst", "tagType": "UdtInstance", "typeId": "Ty"},
        {"name": "Bulk", "tagType": "Folder", "tags": bulk}
    ]}))
    .await;

    // Install through the action: scriptExec plus tagDelete, one import.
    let work = tempfile::tempdir().expect("tempdir");
    let config_path = work.path().join("config.toml");
    std::fs::write(
        &config_path,
        format!(
            "active = \"rig\"\n\n[profiles.rig]\nurl = \"{}\"\n",
            rig.url
        ),
    )
    .expect("config");
    let repo = work.path().join("repo");
    std::fs::create_dir_all(repo.join("deploy")).expect("repo");
    std::fs::write(
        repo.join("deploy/project.yaml"),
        format!(
            "schema_version: 1\nproject: Demo\nenvironments: [dev]\ntag_providers: [{PROVIDER}]\n"
        ),
    )
    .expect("project.yaml");
    let api = rig.api();
    let result = webdev_deploy_with(
        &api,
        PROJECT,
        true,
        false,
        &config_path,
        "rig",
        false,
        Some(TagDeleteRequest {
            repo: repo.clone(),
            user_source: "default".into(),
            role: DEPLOY_ROLE.into(),
            rotate_secret: false,
        }),
    )
    .await
    .expect("webdev_deploy_with installs both routes");
    eprintln!(
        "installed routes={:?} tag_delete_providers={:?}",
        result.routes, result.tag_delete_providers
    );
    let stored = ignition_core::config::load(&config_path).expect("config reloads");
    let profile = &stored.profiles["rig"];
    let exec_secret = profile.webdev_secret.clone().expect("scriptExec secret");
    let secret = profile
        .webdev_tag_delete_secret
        .clone()
        .expect("tagDelete secret");
    let serialized = serde_json::to_string(&result).expect("result serializes");
    assert!(!serialized.contains(&secret) && !serialized.contains(&exec_secret));

    let exec = Exec {
        rig: rig.clone(),
        secret: exec_secret,
    };
    exec.wait_ready().await;
    // Mixed case, a digit and a symbol: the user source's complexity rule.
    let password = format!("Pw-{}-Z9", random_hex());
    let _guard = MatrixGuard { exec: exec.clone() };
    let setup = exec.create_deploy_user(&password).await;
    eprintln!("user/role setup answer: {setup}");
    assert_eq!(setup["roleErrors"], serde_json::json!([]), "addRole errors");
    assert_eq!(setup["userErrors"], serde_json::json!([]), "addUser errors");
    assert_eq!(setup["roles"], serde_json::json!([DEPLOY_ROLE]));
    eprintln!("user/role setup: system.user");

    let version = rig.wait_version(DEPLOY_USER, &password, &secret).await;
    assert_eq!(version.data()["providers"], serde_json::json!([PROVIDER]));

    let baseline = export_paths(&rig.export().await);
    assert!(baseline.contains("Keep") && baseline.contains("Fold/X"));
    let mut records = Vec::new();
    let delete_keep =
        serde_json::json!({"action": "delete", "provider": PROVIDER, "paths": ["Keep"]});
    let wrong_secret = random_hex();

    // Auth rows: WebDev refusals by HTTP status.
    let answer = rig
        .call(Auth::None, &[(SECRET_HEADER, &secret)], &delete_keep)
        .await;
    records.push(row("no credentials", &answer));
    assert_eq!(answer.status, 401);
    let answer = rig
        .call(Auth::Token, &[(SECRET_HEADER, &secret)], &delete_keep)
        .await;
    records.push(row("API token only", &answer));
    assert_eq!(answer.status, 401);
    let answer = rig
        .call(
            Auth::Basic(DEPLOY_USER, "wrong-password"),
            &[(SECRET_HEADER, &secret)],
            &delete_keep,
        )
        .await;
    records.push(row("Basic wrong password", &answer));
    assert_eq!(answer.status, 401);
    let answer = rig
        .call(
            Auth::Basic(&rig.admin_user, &rig.admin_password),
            &[(SECRET_HEADER, &secret)],
            &delete_keep,
        )
        .await;
    records.push(row("Basic admin without the role", &answer));
    assert!(
        matches!(answer.status, 401 | 403),
        "role denial: {}",
        answer.status
    );
    let role_denial = answer.status;

    // Secret rows: the route's envelope.
    let answer = rig
        .call(Auth::Basic(DEPLOY_USER, &password), &[], &delete_keep)
        .await;
    records.push(row("deploy user without the header", &answer));
    assert_eq!(answer.code(), Some("secret_required"));
    let answer = rig
        .call(
            Auth::Basic(DEPLOY_USER, &password),
            &[(SECRET_HEADER, &wrong_secret)],
            &delete_keep,
        )
        .await;
    records.push(row("wrong secret", &answer));
    assert_eq!(answer.code(), Some("secret_mismatch"));
    let bearer = format!("Bearer {secret}");
    let answer = rig
        .call(
            Auth::None,
            &[("Authorization", bearer.as_str())],
            &delete_keep,
        )
        .await;
    records.push(row("secret only as Authorization: Bearer", &answer));
    assert!(answer.status == 401 || answer.code() == Some("secret_required"));

    // Scope rows.
    let with_secret = [(SECRET_HEADER, secret.as_str())];
    let call = |body: serde_json::Value| {
        let rig = rig.clone();
        let password = password.clone();
        async move {
            rig.call(Auth::Basic(DEPLOY_USER, &password), &with_secret, &body)
                .await
        }
    };
    let answer =
        call(serde_json::json!({"action": "delete", "provider": "Other", "paths": ["Keep"]})).await;
    records.push(row("provider Other", &answer));
    assert_eq!(answer.code(), Some("provider_not_allowed"));
    for bad in ["", "_types_", "[Other]X", "a//b", "../x", "/lead", "trail/"] {
        let answer =
            call(serde_json::json!({"action": "delete", "provider": PROVIDER, "paths": [bad]}))
                .await;
        records.push(row(&format!("path {bad:?}"), &answer));
        assert_eq!(answer.code(), Some("path_invalid"), "{bad:?}");
    }
    let answer = call(
        serde_json::json!({"action": "delete", "provider": PROVIDER, "paths": ["Keep", "a//b"]}),
    )
    .await;
    records.push(row("valid path then an invalid one", &answer));
    assert_eq!(answer.code(), Some("path_invalid"));
    assert!(
        answer.body.as_ref().expect("body")["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("paths[1]"),
        "the message names the index"
    );
    let many: Vec<String> = (0..501).map(|i| format!("Bulk/T{i:03}")).collect();
    let answer =
        call(serde_json::json!({"action": "delete", "provider": PROVIDER, "paths": many})).await;
    records.push(row("501 paths", &answer));
    assert_eq!(answer.code(), Some("too_many_paths"));
    let answer = call(serde_json::json!({"action": "nope"})).await;
    records.push(row("action nope", &answer));
    assert_eq!(answer.code(), Some("unknown_action"));

    let after_refusals = export_paths(&rig.export().await);
    assert_eq!(
        after_refusals, baseline,
        "no refusal row changed the export"
    );
    assert!(exec.exists("Keep").await);
    eprintln!(
        "refusal rows left the export unchanged ({} paths)",
        baseline.len()
    );

    // Positive rows.
    let mut exists_after_true = false;
    let mut per_path_list = true;
    let mut note = |answer: &Answer, label: &str| {
        let data = answer.data();
        for result in data["results"].as_array().expect("results") {
            if result["quality"].is_null() {
                per_path_list = false;
            }
            if result["good"] == true && result["existsAfter"] == true {
                exists_after_true = true;
            }
        }
        let first = &data["results"][0];
        let line = format!(
            "row {label}: http={} results={} first quality={} good={} existsAfter={} elapsedMs={}",
            answer.status,
            data["results"].as_array().map_or(0, Vec::len),
            first["quality"],
            first["good"],
            first["existsAfter"],
            data["elapsedMs"]
        );
        eprintln!("{line}");
        line
    };

    let answer =
        call(serde_json::json!({"action": "delete", "provider": PROVIDER, "paths": ["Fold"]}))
            .await;
    records.push(note(&answer, "folder Fold (children X, Y)"));
    let folder_children_gone = !exec.exists("Fold/X").await && !exec.exists("Fold/Y").await;
    let export = export_paths(&rig.export().await);
    assert!(!export.contains("Fold"), "Fold is gone from the export");
    eprintln!(
        "folder delete removes children: {}",
        if folder_children_gone { "yes" } else { "no" }
    );

    assert!(
        exec.exists("Inst/M1").await,
        "instance member exists before"
    );
    let answer =
        call(serde_json::json!({"action": "delete", "provider": PROVIDER, "paths": ["Inst"]}))
            .await;
    records.push(note(&answer, "UDT instance Inst (members M1, M2)"));
    let instance_members_gone = !exec.exists("Inst/M1").await && !exec.exists("Inst/M2").await;
    assert!(!export_paths(&rig.export().await).contains("Inst"));
    assert!(exec.exists("_types_/Ty").await, "the type definition stays");
    eprintln!(
        "instance delete removes members: {}",
        if instance_members_gone { "yes" } else { "no" }
    );

    let answer = call(
        serde_json::json!({"action": "delete", "provider": PROVIDER, "paths": ["Nope/Missing"]}),
    )
    .await;
    let absent = &answer.data()["results"][0];
    let absent_line = format!(
        "row absent path Nope/Missing: http={} quality={} good={} existsAfter={}",
        answer.status, absent["quality"], absent["good"], absent["existsAfter"]
    );
    eprintln!("{absent_line}");
    records.push(absent_line);

    let five_hundred: Vec<String> = (0..500).map(|i| format!("Bulk/T{i:03}")).collect();
    let started = Instant::now();
    let answer =
        call(serde_json::json!({"action": "delete", "provider": PROVIDER, "paths": five_hundred}))
            .await;
    let seconds = started.elapsed().as_secs_f64();
    records.push(note(&answer, "500 paths Bulk/T000..T499"));
    let results = answer.data()["results"]
        .as_array()
        .expect("results")
        .clone();
    assert_eq!(results.len(), 500);
    assert!(
        results.iter().all(|r| r["existsAfter"] == false),
        "all 500 gone"
    );
    let export = export_paths(&rig.export().await);
    assert!(!export.iter().any(|path| path.starts_with("Bulk/")));
    assert!(
        export.contains("Keep"),
        "the keeper survives every positive row"
    );

    eprintln!(
        "CAPTURE deleteTags return: {}",
        if per_path_list {
            "per-path list"
        } else {
            "none"
        }
    );
    eprintln!("CAPTURE role denial status: {role_denial}");
    eprintln!(
        "CAPTURE folder delete removes children: {}",
        if folder_children_gone { "yes" } else { "no" }
    );
    eprintln!(
        "CAPTURE instance delete removes members: {}",
        if instance_members_gone { "yes" } else { "no" }
    );
    eprintln!(
        "CAPTURE existsAfter after good delete: {}",
        if exists_after_true {
            "true seen"
        } else {
            "false"
        }
    );
    eprintln!("CAPTURE user/role setup: system.user");
    eprintln!("CAPTURE 500-path seconds: {seconds:.1}");
    for record in &records {
        eprintln!("RECORD {record}");
    }
}
