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
//!
//! ## The whole run on the single node (26-15, `project_tags_end_to_end`)
//!
//! A temp project repo (project `P26Project`, provider `P26Live`, tag groups
//! `Fast` and `Slow`, a UDT type in a `_types_` subfolder and two instances)
//! goes through `ign deploy projects` in-process, wired as the CLI wires it,
//! against a `local:` master on the rig's host data root
//! (`compose.projects.yml`, mode `production`):
//!
//! ```text
//! cd crates/ignition-core/tests/fixtures/live-rig-deploy
//! bash capture25.sh up25     # then create mode production and recreate with
//!                            # IGN_DEPLOY_RIG_MODE_ARG=-Dignition.config.mode=production
//! . ~/.cache/ign-deploy-rig/tokens.env
//! IGNITION_LIVE_DEPLOY=1 IGNITION_LIVE_URL=http://localhost:18290 \
//!   IGNITION_LIVE_TOKEN="$IGN_TOKEN_CAP_A" IGN_DEPLOY_RIG_DATA=<the data root> \
//!   cargo test -p ignition-core --test live_deploy_project_tags project_tags_end_to_end \
//!   -- --ignored --nocapture --test-threads=1
//! ```
//!
//! `IGN_DEPLOY_RIG_DATA` defaults to `$HOME/.cache/ign-deploy-rig/data25`
//! (what `capture25.sh up25` creates). `IGN_LIVE_EVIDENCE_DIR`, when set to a
//! directory outside the repository, receives each step's result JSON (paths,
//! counts and hashes, as `--json` prints them). The gate also installs the
//! plain `ign-cli` WebDev bundle when the rig has no `ign-cli` project (for
//! the `tags` read and the `tagConfig` create) and removes it again.
//!
//! ## The pair (26-15, `project_tags_pair_end_to_end`)
//!
//! ```text
//! cd crates/ignition-core/tests/fixtures/live-rig-pair
//! ./setup-pair.sh up && ./setup-pair.sh mint && ./setup-pair.sh pair && ./setup-pair.sh mode
//! . ~/.cache/ign-pair-rig/tokens.env
//! IGNITION_LIVE_PAIR=1 IGN_PAIR_URL_A=http://localhost:18300 IGN_PAIR_URL_B=http://localhost:18301 \
//!   IGN_PAIR_DATA_A="$HOME/.cache/ign-pair-rig/data-a" IGN_PAIR_DATA_B="$HOME/.cache/ign-pair-rig/data-b" \
//!   cargo test -p ignition-core --test live_deploy_project_tags project_tags_pair_end_to_end \
//!   -- --ignored --nocapture --test-threads=1
//! ```
//!
//! The URL and data variables default to the values above; the tokens come
//! from `IGN_PAIR_TOKEN_A` and `IGN_PAIR_TOKEN_B`. gw-a is a `local:` master;
//! gw-b is reached only through [`MethodTap`], which forwards reads and
//! answers anything else `405` without forwarding it.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ignition_core::actions::deploy_projects::{
    CHILDREN_POLL_S, CHILDREN_SETTLE_S, DeployProjectsResult, GatewayProjects, NoProjectsObserver,
    ProjectGateway, ProjectOutcome, ProjectsInput, ProjectsRunOptions, ProjectsSurfaces,
    TAG_VERIFY_POLL_S, TAG_VERIFY_SETTLE_S, deploy_projects_prepare, deploy_projects_rollback_run,
    deploy_projects_run, load_project, load_project_for_rollback,
};
use ignition_core::client::deploy_api::scan_and_wait;
use ignition_core::client::query::ListQuery;
use ignition_core::client::scan::ScanKind;
use ignition_core::client::tags::{CollisionPolicy, TAGS_IMPORT_TIMEOUT, TagProviderCreate};
use ignition_core::client::webdev::build_deploy_zip;
use ignition_core::client::{DeployApi, GatewayApi, ReqwestGatewayApi};
use ignition_core::config::{Credential, Secret};
use ignition_core::deploy::apply::preflight::DesignersPolicy;
use ignition_core::deploy::native_tags::encode_fs_name;
use ignition_core::deploy::pair::{BACKUP_POLL_S, BackupNode, BackupPolicy};
use ignition_core::deploy::tag_apply::IMPORT_BATCH_MAX_BYTES;
use ignition_core::deploy::tag_canon::canonical_file_bytes;
use ignition_core::deploy::tag_converge::{BackupTags, exports_converged};
use ignition_core::deploy::transport::{self, projects::project_host_for_node};
use ignition_core::error::CoreError;
use ignition_core::poll::PollConfig;
use serde_json::{Value, json};

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

// ============================================================================
// 26-15: the whole run on the single node and on the pair
// ============================================================================

// ---- the tap -----------------------------------------------------------------

/// One request a tap saw: the method, the request target (path and query,
/// names only) and the body length. No header and no body byte is kept.
#[derive(Clone, Debug)]
struct Seen {
    method: String,
    target: String,
    body_len: usize,
}

/// An in-test HTTP/1.1 tap on `127.0.0.1:0` in front of a rig node.
///
/// Per client connection it reads one request head (and a body by
/// `Content-Length`), records a [`Seen`], and forwards the request with
/// `Connection: close`, copying the answer back until EOF. A read-only tap
/// ([`MethodTap::start`]) forwards `GET` and `HEAD` only: any other method is
/// recorded and answered `405` without being forwarded, so a stray write
/// cannot reach the node (T-26-54). A recording tap
/// ([`MethodTap::recording`]) forwards everything, to measure the bodies a
/// run sends.
struct MethodTap {
    addr: SocketAddr,
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl MethodTap {
    fn start(upstream_url: &str) -> MethodTap {
        Self::spawn(upstream_url, false)
    }

    fn recording(upstream_url: &str) -> MethodTap {
        Self::spawn(upstream_url, true)
    }

    fn spawn(upstream_url: &str, forward_writes: bool) -> MethodTap {
        let upstream = local_url_guard(upstream_url).expect("the tap's upstream is loopback");
        let host = upstream
            .host_str()
            .expect("host")
            .trim_matches(['[', ']'])
            .to_string();
        let port = upstream.port_or_known_default().expect("port");
        let authority = format!("{host}:{port}");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind the tap");
        let addr = listener.local_addr().expect("tap address");
        let seen: Arc<Mutex<Vec<Seen>>> = Arc::default();
        let log = Arc::clone(&seen);
        std::thread::spawn(move || {
            for conn in listener.incoming() {
                let Ok(conn) = conn else { continue };
                let log = Arc::clone(&log);
                let authority = authority.clone();
                std::thread::spawn(move || {
                    let _ = relay(conn, &authority, &log, forward_writes);
                });
            }
        });
        MethodTap { addr, seen }
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn all(&self) -> Vec<Seen> {
        self.seen.lock().expect("tap log").clone()
    }

    fn total(&self) -> usize {
        self.seen.lock().expect("tap log").len()
    }

    /// Every recorded request whose method is not `GET`.
    fn non_gets(&self) -> Vec<Seen> {
        self.all()
            .into_iter()
            .filter(|seen| seen.method != "GET")
            .collect()
    }

    /// `GET 12` style summary, methods only.
    fn summary(&self) -> String {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for seen in self.all() {
            *counts.entry(seen.method).or_insert(0) += 1;
        }
        if counts.is_empty() {
            return "no requests".to_string();
        }
        counts
            .iter()
            .map(|(method, n)| format!("{method} {n}"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// The index just past the first `\r\n\r\n` of `buf`, if present.
fn head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
}

fn relay(
    mut client: TcpStream,
    upstream: &str,
    log: &Arc<Mutex<Vec<Seen>>>,
    forward_writes: bool,
) -> std::io::Result<()> {
    client.set_read_timeout(Some(Duration::from_secs(300)))?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let end = loop {
        if let Some(end) = head_end(&buf) {
            break end;
        }
        if buf.len() > 65_536 {
            return Ok(());
        }
        let n = client.read(&mut chunk)?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..end]).to_string();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or_default().to_string();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default().to_string();

    if !forward_writes && method != "GET" && method != "HEAD" {
        log.lock().expect("tap log").push(Seen {
            method,
            target,
            body_len: 0,
        });
        client.write_all(
            b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )?;
        return Ok(());
    }

    let mut forwarded = format!("{request_line}\r\n");
    let mut length = 0usize;
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("host:") || lower.starts_with("connection:") {
            continue;
        }
        if let Some(value) = lower.strip_prefix("content-length:") {
            length = value.trim().parse().unwrap_or(0);
        }
        forwarded.push_str(line);
        forwarded.push_str("\r\n");
    }
    forwarded.push_str(&format!("Host: {upstream}\r\nConnection: close\r\n\r\n"));
    let mut body = buf[end..].to_vec();
    while body.len() < length {
        let n = client.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    log.lock().expect("tap log").push(Seen {
        method,
        target,
        body_len: body.len(),
    });

    // `localhost` may resolve to ::1 first while the rig publishes 127.0.0.1
    // only: take the first address that accepts.
    let mut up = upstream
        .to_socket_addrs()?
        .find_map(|addr| TcpStream::connect_timeout(&addr, Duration::from_secs(10)).ok())
        .ok_or_else(|| std::io::Error::other("upstream does not accept"))?;
    up.set_read_timeout(Some(Duration::from_secs(300)))?;
    up.write_all(forwarded.as_bytes())?;
    up.write_all(&body)?;
    std::io::copy(&mut up, &mut client)?;
    Ok(())
}

// ---- shared helpers ----------------------------------------------------------

/// The WebDev project of the CLI's routes.
const WEBDEV_PROJECT: &str = "ign-cli";

fn token_api(url: &str, token: &str) -> ReqwestGatewayApi {
    ReqwestGatewayApi::for_tests(url, Some(Credential::Token(Secret::new(token.to_string()))))
}

/// An env var with a default when unset or empty.
fn env_or(name: &str, default: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").expect("HOME is set"))
}

/// The repository root (two levels above this crate's manifest).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the repository root")
        .canonicalize()
        .expect("canonical repository root")
}

/// `IGN_LIVE_EVIDENCE_DIR`, when set and outside the repository.
fn evidence_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var("IGN_LIVE_EVIDENCE_DIR").ok()?);
    std::fs::create_dir_all(&dir).ok()?;
    let real = dir.canonicalize().ok()?;
    assert!(
        !real.starts_with(repo_root()),
        "IGN_LIVE_EVIDENCE_DIR must be outside the repository"
    );
    Some(real)
}

/// Save a step's result JSON to the evidence directory, when there is one.
fn save_evidence(name: &str, value: &Value) {
    if let Some(dir) = evidence_dir() {
        let path = dir.join(format!("{name}.json"));
        std::fs::write(&path, serde_json::to_vec_pretty(value).expect("json"))
            .unwrap_or_else(|err| panic!("cannot write {}: {err}", path.display()));
    }
}

fn put(root: &Path, rel: &str, bytes: &[u8]) {
    let target = root.join(rel);
    std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
    std::fs::write(target, bytes).expect("write");
}

/// The node at `path` (slash-separated names) of an export, as JSON.
fn node_in(export: &Value, path: &str) -> Option<Value> {
    node_at(export, path).cloned()
}

fn parse_export(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).expect("the export is JSON")
}

/// The provider's REST export through `api`.
async fn export_of(api: &ReqwestGatewayApi, provider: &str) -> Value {
    parse_export(
        &GatewayProjects(api)
            .tag_export(provider)
            .await
            .unwrap_or_else(|err| panic!("export of {provider}: {}", err.code())),
    )
}

/// One import through the real client, C2 form: `policy` at `path`.
async fn raw_import(
    api: &ReqwestGatewayApi,
    provider: &str,
    path: &str,
    policy: CollisionPolicy,
    body: &Value,
) -> ignition_core::client::tags::TagImportAnswer {
    let started = Instant::now();
    loop {
        match GatewayProjects(api)
            .tag_import(
                provider,
                path,
                policy,
                serde_json::to_vec(body).expect("body"),
            )
            .await
        {
            Ok(answer) => return answer,
            // A provider that just listed may not run yet (422 "provider not
            // found"); nothing was imported, so the same body is sent again.
            Err(err)
                if err.to_string().contains("provider not found")
                    && started.elapsed() < Duration::from_secs(120) =>
            {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Err(err) => panic!("import into {provider}/{path}: {} ({err})", err.code()),
        }
    }
}

/// Delete `provider` if the gateway has it.
async fn delete_provider(api: &ReqwestGatewayApi, provider: &str) {
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

/// Create `provider` and wait until it lists.
async fn fresh_provider(api: &ReqwestGatewayApi, provider: &str) {
    delete_provider(api, provider).await;
    api.tag_provider_create(&[TagProviderCreate::standard(provider)])
        .await
        .unwrap_or_else(|err| panic!("create provider {provider}: {}", err.code()));
    let started = Instant::now();
    while !GatewayProjects(api)
        .tag_provider_names()
        .await
        .expect("provider list")
        .iter()
        .any(|name| name == provider)
    {
        assert!(
            started.elapsed() < Duration::from_secs(180),
            "{provider} never listed"
        );
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    // A provider lists before its tag engine runs: an import then answers 422
    // "provider not found". Its export answering is the readiness signal.
    while GatewayProjects(api).tag_export(provider).await.is_err() {
        assert!(
            started.elapsed() < Duration::from_secs(180),
            "{provider} never exported"
        );
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn project_names(api: &ReqwestGatewayApi) -> Vec<String> {
    api.projects(&ListQuery::default())
        .await
        .expect("projects/list")
        .items
        .into_iter()
        .map(|r| r.name)
        .collect()
}

async fn delete_project(api: &ReqwestGatewayApi, project: &str) {
    if project_names(api).await.iter().any(|n| n == project)
        && let Err(err) = api.project_delete(project).await
    {
        eprintln!("cleanup: cannot delete project {project}: {}", err.code());
    }
}

/// A config scan on `api`, waited for.
async fn config_scan(api: &ReqwestGatewayApi) {
    if let Err(err) = scan_and_wait(
        api,
        ScanKind::Config,
        PollConfig {
            subject: "config scan".to_string(),
            interval: Duration::from_millis(500),
            max: Duration::from_secs(2),
            deadline: Duration::from_secs(180),
        },
    )
    .await
    {
        eprintln!("cleanup: config scan failed ({})", err.code());
    }
}

fn remove_dir(path: &Path) {
    if path.exists()
        && let Err(err) = std::fs::remove_dir_all(path)
    {
        eprintln!("cleanup: cannot remove {}: {err}", path.display());
    }
}

/// Remove every directory named by one of `run_ids` within two levels below
/// `root` (the file engine's run records of the group commits).
fn remove_run_dirs(root: &Path, run_ids: &BTreeSet<String>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if run_ids.contains(&entry.file_name().to_string_lossy().to_string()) {
            remove_dir(&path);
            continue;
        }
        if let Ok(inner) = std::fs::read_dir(&path) {
            for child in inner.flatten() {
                if run_ids.contains(&child.file_name().to_string_lossy().to_string()) {
                    remove_dir(&child.path());
                }
            }
        }
    }
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

// ---- the project repo --------------------------------------------------------

const PROJECT_JSON: &str = r#"{"title":"P26 live","enabled":true,"inheritable":false}"#;
const SCRIPT_RESOURCE: &str = r#"{"scope":"A","version":1,"restricted":false,"overridable":true,"files":["code.py"],"attributes":{}}"#;
const CODE: &str = "def f():\n    return 1\n";

/// The provider settings the gateway holds for a STANDARD provider created
/// with an empty `settings` (C11), so a created provider reads back equal.
fn provider_settings() -> Value {
    json!({
        "schema_version": 1,
        "enabled": true,
        "config": {
            "profile": {
                "type": "STANDARD",
                "allowBackfill": false,
                "enableTagReferenceStore": false
            },
            "settings": {
                "defaultDatasourceName": null,
                "readPermissions": null,
                "readOnly": false,
                "writePermissions": null,
                "editPermissions": null,
                "valuePersistence": null
            }
        }
    })
}

/// A UDT type below `_types_/Family/` (26-08 open point 4): a parameter, a
/// memory member and a member in a nested folder.
fn motor_type(speed_doc: &str, speed_default: i64) -> Value {
    json!({
        "tagType": "UdtType",
        "parameters": {"Line": {"dataType": "Integer", "value": 1}},
        "tags": [
            {"name": "Speed", "tagType": "AtomicTag", "dataType": "Int4",
             "valueSource": "memory", "value": speed_default, "documentation": speed_doc},
            {"name": "Status", "tagType": "Folder", "tags": [
                {"name": "Running", "tagType": "AtomicTag", "dataType": "Boolean",
                 "valueSource": "memory", "value": false}
            ]}
        ]
    })
}

/// An instance of `type_id` with parameter `Line` = `line` and a
/// documentation override on member `Speed`, the other members as stubs.
fn motor_instance(type_id: &str, line: i64, speed_doc: &str) -> Value {
    json!({
        "tagType": "UdtInstance",
        "typeId": type_id,
        "parameters": {"Line": {"dataType": "Integer", "value": line}},
        "tags": [
            {"name": "Speed", "tagType": "AtomicTag", "documentation": speed_doc},
            {"name": "Status", "tagType": "Folder", "tags": [
                {"name": "Running", "tagType": "AtomicTag"}
            ]}
        ]
    })
}

/// An expression tag that runs at its tag group's rate (`TagGroupRate`), so
/// its value moves only when its group executes (Q6).
fn ticker(group: &str) -> Value {
    json!({"dataType": "DateTime", "executionMode": "TagGroupRate", "expression": "now(0)",
           "tagGroup": group, "tagType": "AtomicTag", "valueSource": "expr"})
}

/// The release a step deploys: every owned node by provider-relative path.
fn base_release() -> BTreeMap<String, Value> {
    let mut nodes = BTreeMap::new();
    nodes.insert(
        "_types_/Family/Motor".to_string(),
        motor_type("speed v1", 10),
    );
    // A memory tag carrying `value` and no `defaultValue` (R3-DV).
    nodes.insert(
        "Line1/Setpoint".to_string(),
        json!({"dataType": "Int4", "tagType": "AtomicTag", "value": 5, "valueSource": "memory"}),
    );
    nodes.insert(
        "Line1/Motor1".to_string(),
        motor_instance("Family/Motor", 7, "inst doc"),
    );
    nodes.insert("Line1/Ticker".to_string(), ticker("Fast"));
    nodes.insert("Line1/SlowTicker".to_string(), ticker("Slow"));
    nodes.insert(
        "Line2/Level".to_string(),
        json!({"dataType": "Float8", "documentation": "level", "tagType": "AtomicTag",
               "value": 1.5, "valueSource": "memory"}),
    );
    nodes.insert(
        "Line2/Motor2".to_string(),
        motor_instance("Family/Motor", 2, "m2 doc"),
    );
    nodes
}

/// The groups summary: a Direct group and a Leased one (26-10 open point 3).
fn group_summary() -> Value {
    json!({"tagGroups": [
        {"mode": "Direct", "name": "Fast", "rate": 500},
        {"leasedRate": 250, "mode": "Leased", "name": "Slow", "rate": 60000}
    ]})
}

/// A checkout-layout project repo that owns one provider, and a targets root.
struct TagRepo {
    _scratch: tempfile::TempDir,
    repo: PathBuf,
    targets: PathBuf,
    provider: String,
}

impl TagRepo {
    fn new(project: &str, provider: &str, env: &str, target_yaml: &str) -> TagRepo {
        let scratch = tempfile::tempdir().expect("tempdir");
        let repo = scratch.path().join("repo");
        let targets = scratch.path().join("targets-root");
        std::fs::create_dir_all(&repo).expect("repo dir");
        put(
            &repo,
            "deploy/project.yaml",
            format!(
                "schema_version: 1\nproject: {project}\nenvironments: [{env}]\ntag_providers: [{provider}]\n"
            )
            .as_bytes(),
        );
        put(
            &repo,
            &format!("deploy/tag-providers/{provider}.json"),
            &serde_json::to_vec_pretty(&provider_settings()).expect("settings"),
        );
        put(&repo, ".gitignore", b".ign/\n");
        put(&repo, "project.json", PROJECT_JSON.as_bytes());
        put(
            &repo,
            "ignition/script-python/p26live/code.py",
            CODE.as_bytes(),
        );
        put(
            &repo,
            "ignition/script-python/p26live/resource.json",
            SCRIPT_RESOURCE.as_bytes(),
        );
        put(
            &targets,
            &format!("targets/{env}.yaml"),
            target_yaml.as_bytes(),
        );
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["init", "-q"])
            .status()
            .expect("git runs");
        assert!(status.success(), "git init");
        let this = TagRepo {
            _scratch: scratch,
            repo,
            targets,
            provider: provider.to_string(),
        };
        put(
            &this.repo,
            &format!("tags/{provider}/.tag-groups.json"),
            &serde_json::to_vec_pretty(&group_summary()).expect("groups"),
        );
        this
    }

    /// Make `tags/<P>/` hold exactly `nodes` (and the groups summary).
    fn set_release(&self, nodes: &BTreeMap<String, Value>) {
        let dir = self.repo.join("tags").join(&self.provider);
        for entry in std::fs::read_dir(&dir).expect("tags dir").flatten() {
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                std::fs::remove_dir_all(&path).expect("clear tags");
            } else {
                std::fs::remove_file(&path).expect("clear tags");
            }
        }
        for (path, json) in nodes {
            let encoded: Vec<String> = path.split('/').map(encode_fs_name).collect();
            put(
                &self.repo,
                &format!("tags/{}/{}.json", self.provider, encoded.join("/")),
                &canonical_file_bytes(json),
            );
        }
    }

    fn input(&self, env: &str, release: &str) -> ProjectsInput {
        ProjectsInput {
            repo: self.repo.clone(),
            targets: self.targets.clone(),
            env: env.to_string(),
            release: release.to_string(),
            allow_downgrade: false,
            allow_first_change: None,
            artifact_sha256: None,
            tags_sha256: None,
        }
    }
}

// ---- the deploy, wired as the CLI wires it -------------------------------------

/// Run `ign deploy projects` in-process for `input` against the master `api`:
/// the real client, the real host reads, the master's tree reader and
/// `DeployApi`, the CLI's timings. `rollback_run` is `--rollback-run`;
/// `backup` is the pair's read-only backup handle.
async fn run_projects(
    api: &ReqwestGatewayApi,
    input: &ProjectsInput,
    check: bool,
    rollback_run: Option<&str>,
    backup: Option<&dyn BackupTags>,
    backup_budget: Duration,
) -> Result<DeployProjectsResult, CoreError> {
    let loaded = match rollback_run {
        Some(_) => load_project_for_rollback(input)?,
        None => load_project(input)?,
    };
    let host = project_host_for_node(
        &input.targets,
        &loaded.target_file,
        "nodes.master",
        &loaded.target.nodes.master,
    )?;
    let tree = transport::for_node(
        &input.targets,
        &loaded.target_file,
        "nodes.master",
        &loaded.target.nodes.master,
    )?;
    let gateway = GatewayProjects(api);
    let surfaces = ProjectsSurfaces {
        gateway: &gateway,
        host: host.as_ref(),
        tree: Some(tree.as_ref()),
        apply_host: None,
        deploy: Some(api as &dyn DeployApi),
    };
    let opts = ProjectsRunOptions {
        repo: input.repo.clone(),
        targets_root: input.targets.clone(),
        target_file: loaded.target_file.clone(),
        master: loaded.target.nodes.master.clone(),
        record_writer: None,
        tag_record_writer: None,
        run_writer: None,
        designers: DesignersPolicy::default(),
        children_settle: Duration::from_secs(CHILDREN_SETTLE_S),
        children_poll: Duration::from_secs(CHILDREN_POLL_S),
        tag_verify_settle: Duration::from_secs(TAG_VERIFY_SETTLE_S),
        tag_verify_poll: Duration::from_secs(TAG_VERIFY_POLL_S),
        backup,
        backup_policy: BackupPolicy {
            budget: backup_budget,
            interval: Duration::from_secs(BACKUP_POLL_S),
        },
    };
    if let Some(run) = rollback_run {
        return deploy_projects_rollback_run(
            &loaded,
            input,
            &surfaces,
            &opts,
            run,
            check,
            &NoProjectsObserver,
        )
        .await;
    }
    let prepared = deploy_projects_prepare(&loaded, input, &surfaces).await?;
    if check {
        return Ok(prepared.into_check_result());
    }
    deploy_projects_run(
        prepared,
        &loaded,
        input,
        &surfaces,
        &opts,
        &NoProjectsObserver,
    )
    .await
}

fn result_json(result: &DeployProjectsResult) -> Value {
    serde_json::to_value(result).expect("the result serializes")
}

/// One line per step: outcome, the tag counts, the verify, the rollback and
/// the backup, names and counts only.
fn step_line(step: &str, result: &DeployProjectsResult, ms: u128) -> String {
    let data = result_json(result);
    let tags = &data["tags"];
    format!(
        "{step}: outcome={} action={} ms={ms} added={} changed={} removed={} drift={} kept_values={} carried={} verify_equal={} rollback_cause={} rollback_carried={} rollback_reimported={} backup_converged={} backup_elapsed_s={} backup_resync={}",
        result.outcome.as_str(),
        data["action"],
        tags["added_count"],
        tags["changed_count"],
        tags["removed_count"],
        tags["drift_count"],
        tags["kept_values_count"],
        tags["carried"],
        tags["verify"]["equal"],
        tags["rollback"]["cause"],
        tags["rollback"]["carried"],
        tags["rollback"]["reimported"],
        tags["backup"]["converged"],
        tags["backup"]["elapsed_s"],
        tags["backup"]["resync_sent"],
    )
}

/// The reviewed check, then the apply with its hashes, as a CI caller does.
/// Returns the check and the apply's answer.
#[allow(clippy::too_many_arguments)]
async fn checked_apply(
    step: &str,
    api: &ReqwestGatewayApi,
    repo: &TagRepo,
    env: &str,
    release: &str,
    first_change: bool,
    backup: Option<&dyn BackupTags>,
    backup_budget: Duration,
) -> (
    DeployProjectsResult,
    Result<DeployProjectsResult, CoreError>,
) {
    let mut input = repo.input(env, release);
    if first_change {
        input.allow_first_change = Some("p26-15 first deploy on a disposable rig".to_string());
    }
    let check = run_projects(api, &input, true, None, None, backup_budget)
        .await
        .unwrap_or_else(|err| panic!("{step} check: {} ({err})", err.code()));
    save_evidence(&format!("{step}-check"), &result_json(&check));
    eprintln!("{}", step_line(&format!("{step} check"), &check, 0));
    input.artifact_sha256 = Some(check.artifact_sha256.clone());
    input.tags_sha256 = check.tags.as_ref().map(|t| t.tags_sha256.clone());
    let started = Instant::now();
    let applied = run_projects(api, &input, false, None, backup, backup_budget).await;
    match &applied {
        Ok(result) => {
            save_evidence(step, &result_json(result));
            eprintln!("{}", step_line(step, result, started.elapsed().as_millis()));
        }
        Err(err) => eprintln!(
            "{step}: error code={} after {} ms: {err}",
            err.code(),
            started.elapsed().as_millis()
        ),
    }
    (check, applied)
}

fn tags_of(result: &DeployProjectsResult) -> Value {
    result_json(result)["tags"].clone()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn changed_paths(tags: &Value) -> Vec<String> {
    tags["changed"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|c| c["path"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

// ---- the WebDev routes the single-node gate reads through -----------------------

/// POST `body` to the `cli/<route>` route of the CLI's WebDev project. The
/// plain bundle's routes take no credentials (`require-auth: false`).
async fn route(url: &str, route: &str, body: &Value) -> (u16, Option<Value>) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .expect("client");
    let response = client
        .post(format!(
            "{}/system/webdev/{WEBDEV_PROJECT}/cli/{route}",
            url.trim_end_matches('/')
        ))
        .json(body)
        .send()
        .await
        .expect("the route answers");
    let status = response.status().as_u16();
    let text = response.text().await.expect("body");
    (status, serde_json::from_str(&text).ok())
}

/// Wait until `route` answers its `version` action.
async fn wait_route(url: &str, name: &str) {
    let started = Instant::now();
    loop {
        let (status, body) = route(url, name, &json!({"action": "version"})).await;
        if status == 200 && body.as_ref().is_some_and(|b| b["ok"] == true) {
            eprintln!(
                "route {name} ready after {:.1} s",
                started.elapsed().as_secs_f64()
            );
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(180),
            "the {name} route never answered (last status {status})"
        );
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// `(quality, value, timestamp)` of each path, through the `tags` route.
async fn read_tags(url: &str, paths: &[&str]) -> Vec<(String, String, String)> {
    let (status, body) = route(url, "tags", &json!({"action": "read", "paths": paths})).await;
    let body = body.unwrap_or_else(|| panic!("the tags read answered {status} without JSON"));
    assert_eq!(body["ok"], true, "the tags read: {body}");
    body["data"]["results"]
        .as_array()
        .expect("results")
        .iter()
        .map(|r| {
            (
                r["quality"].as_str().unwrap_or_default().to_string(),
                r["value"].to_string(),
                r["timestamp"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect()
}

// ---- the single-node gate -------------------------------------------------------

const P26_PROJECT: &str = "P26Project";
const P26_PROVIDER: &str = "P26Live";
const P26_BATCH_PROVIDER: &str = "P26Batch";
const P26_VS_PROVIDER: &str = "P26Vs";
const RIG_ENV: &str = "rig";

#[derive(Clone)]
struct ProjectsRig {
    url: String,
    token: String,
    data_dir: PathBuf,
}

impl ProjectsRig {
    fn api(&self) -> ReqwestGatewayApi {
        token_api(&self.url, &self.token)
    }

    fn group_dir(&self) -> PathBuf {
        self.data_dir
            .join("config/resources/production/ignition/tag-group")
            .join(P26_PROVIDER)
    }

    fn records_dir(&self) -> PathBuf {
        self.data_dir.join("ign-deploy").join(RIG_GATEWAY_NAME)
    }
}

/// Everything the single-node gate made, removed on entry and when the test
/// ends, even by a panic.
struct ProjectsCleanup {
    rig: ProjectsRig,
    webdev_installed: Arc<Mutex<bool>>,
    run_ids: Arc<Mutex<BTreeSet<String>>>,
}

impl ProjectsCleanup {
    async fn sweep(rig: ProjectsRig, webdev_installed: bool, run_ids: BTreeSet<String>) {
        let api = rig.api();
        delete_project(&api, P26_PROJECT).await;
        if webdev_installed {
            delete_project(&api, WEBDEV_PROJECT).await;
        }
        for provider in [P26_PROVIDER, P26_BATCH_PROVIDER, P26_VS_PROVIDER] {
            delete_provider(&api, provider).await;
        }
        let had_groups = rig.group_dir().exists();
        remove_dir(&rig.group_dir());
        if had_groups {
            config_scan(&api).await;
        }
        remove_dir(&rig.records_dir().join("projects").join(P26_PROJECT));
        remove_dir(&rig.records_dir().join("tag-providers").join(P26_PROVIDER));
        remove_run_dirs(&rig.records_dir(), &run_ids);
        remove_run_dirs(&rig.records_dir().join("production"), &run_ids);
    }

    fn record_run(&self, result: &DeployProjectsResult) {
        if let Some(id) = &result.run_id {
            self.run_ids.lock().expect("run ids").insert(id.clone());
        }
    }
}

impl Drop for ProjectsCleanup {
    fn drop(&mut self) {
        let rig = self.rig.clone();
        let webdev = *self.webdev_installed.lock().expect("flag");
        let run_ids = self.run_ids.lock().expect("run ids").clone();
        block_on_cleanup(async move {
            ProjectsCleanup::sweep(rig.clone(), webdev, run_ids).await;
            let api = rig.api();
            let left: Vec<String> = GatewayProjects(&api)
                .tag_provider_names()
                .await
                .unwrap_or_default()
                .into_iter()
                .filter(|n| n.starts_with("P26"))
                .collect();
            let projects: Vec<String> = project_names(&api)
                .await
                .into_iter()
                .filter(|n| n == P26_PROJECT || (webdev && n == WEBDEV_PROJECT))
                .collect();
            eprintln!(
                "cleanup: P26 providers left {left:?}, gate projects left {projects:?}, group units left {}",
                rig.group_dir().exists()
            );
        });
    }
}

/// `None` is the quiet green skip. Every guard runs before the first request.
async fn projects_rig() -> Option<ProjectsRig> {
    if std::env::var("IGNITION_LIVE_DEPLOY").as_deref() != Ok("1") {
        skip("IGNITION_LIVE_DEPLOY is not 1");
        return None;
    }
    let url = require_env("IGNITION_LIVE_URL");
    let token = require_env("IGNITION_LIVE_TOKEN");
    if let Err(why) = local_url_guard(&url) {
        panic!("IGNITION_LIVE_URL refused: {why}");
    }
    let default_data = home().join(".cache/ign-deploy-rig/data25");
    let data_dir = PathBuf::from(env_or(
        "IGN_DEPLOY_RIG_DATA",
        &default_data.display().to_string(),
    ));
    assert!(
        data_dir.join("projects").is_dir() && data_dir.join("config/resources").is_dir(),
        "IGN_DEPLOY_RIG_DATA must be the host data root whose projects and config/resources are \
         bound into the rig (compose.projects.yml)"
    );
    let data_dir = data_dir.canonicalize().expect("canonical data root");
    assert!(
        !data_dir.starts_with(repo_root()),
        "IGN_DEPLOY_RIG_DATA must be outside the repository"
    );
    let rig = ProjectsRig {
        url,
        token,
        data_dir,
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

/// The value of `node[key]` as a short string for the log.
fn show(node: &Option<Value>, key: &str) -> String {
    node.as_ref()
        .and_then(|n| n.get(key))
        .map_or_else(|| "<absent>".to_string(), Value::to_string)
}

/// Generate an Abort body of memory tags in folders of 100 below `prefix`,
/// as large as fits under `cap` bytes. Returns the body bytes and the tag
/// count.
fn batch_body(prefix: &str, cap: usize) -> (Vec<u8>, usize) {
    let build = |count: usize| -> Vec<u8> {
        let mut folders = Vec::new();
        let mut i = 0usize;
        while i < count {
            let mut tags = Vec::new();
            for j in i..(i + 100).min(count) {
                tags.push(json!({"name": format!("T{j:05}"), "tagType": "AtomicTag",
                    "dataType": "Int4", "valueSource": "memory", "value": j as i64}));
            }
            folders.push(json!({"name": format!("{prefix}{:03}", i / 100),
                "tagType": "Folder", "tags": tags}));
            i += 100;
        }
        serde_json::to_vec(&json!({"tags": folders})).expect("body")
    };
    let (mut low, mut high) = (1usize, 20_000usize);
    while low < high {
        let mid = (low + high).div_ceil(2);
        if build(mid).len() < cap {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    (build(low), low)
}

/// 26-15 Task 1: the whole project-owned tag deploy on the 8.3.6 single node.
#[tokio::test]
#[ignore = "needs the 8.3.6 rig with compose.projects.yml: see the module doc"]
async fn project_tags_end_to_end() {
    let Some(rig) = projects_rig().await else {
        return;
    };
    eprintln!("LIVE-RAN project_tags_end_to_end");
    let api = rig.api();
    let gateway_started = Instant::now();

    // Leftovers of a cut-off run go first.
    ProjectsCleanup::sweep(rig.clone(), false, BTreeSet::new()).await;
    let webdev_installed = !project_names(&api)
        .await
        .iter()
        .any(|n| n == WEBDEV_PROJECT);
    let cleanup = ProjectsCleanup {
        rig: rig.clone(),
        webdev_installed: Arc::new(Mutex::new(false)),
        run_ids: Arc::default(),
    };
    if webdev_installed {
        let zip = build_deploy_zip(WEBDEV_PROJECT, false, None, false).expect("plain bundle packs");
        api.project_import(WEBDEV_PROJECT, zip, false)
            .await
            .unwrap_or_else(|err| panic!("install the plain bundle: {}", err.code()));
        *cleanup.webdev_installed.lock().expect("flag") = true;
    }
    wait_route(&rig.url, "tags").await;
    wait_route(&rig.url, "tagConfig").await;

    let target_yaml = format!(
        "schema_version: 1\ngateway: {RIG_GATEWAY_NAME}\nmode: production\nnodes:\n  master:\n    \
         profile: rig\n    gateway_name: {RIG_GATEWAY_NAME}\n    local:\n      data_dir: '{}'\n",
        rig.data_dir.display()
    );
    let repo = TagRepo::new(P26_PROJECT, P26_PROVIDER, RIG_ENV, &target_yaml);
    let budget = Duration::from_secs(300);

    // ---- step 1: the first deploy creates the provider, groups, type, tags
    // and the project.
    let v1 = base_release();
    repo.set_release(&v1);
    let (check1, applied) = checked_apply(
        "step1-first-deploy",
        &api,
        &repo,
        RIG_ENV,
        "v1.0.0",
        true,
        None,
        budget,
    )
    .await;
    let tags1 = tags_of(&check1);
    let settings_actions: Vec<Value> = tags1["provider_settings"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| p["action"].clone())
        .collect();
    assert_eq!(
        settings_actions,
        [json!("create")],
        "the provider is created: {}",
        tags1["provider_settings"]
    );
    let step1 = applied.unwrap_or_else(|err| panic!("step 1: {} ({err})", err.code()));
    cleanup.record_run(&step1);
    assert_eq!(step1.outcome, ProjectOutcome::Applied, "step 1 outcome");
    let tags = tags_of(&step1);
    assert_eq!(
        tags["verify"]["equal"], true,
        "step 1 verify: {}",
        tags["verify"]
    );
    assert_eq!(tags["removed_count"], 0);
    assert!(
        rig.group_dir().join("Fast/config.json").is_file()
            && rig.group_dir().join("Slow/config.json").is_file(),
        "the group units are in the mode collection"
    );
    let export1 = export_of(&api, P26_PROVIDER).await;
    let setpoint = node_in(&export1, "Line1/Setpoint");
    eprintln!(
        "step 1 R3-DV: Line1/Setpoint value={} defaultValue={}",
        show(&setpoint, "value"),
        show(&setpoint, "defaultValue")
    );
    assert_eq!(
        show(&setpoint, "defaultValue"),
        "5",
        "R3-DV: the gateway added defaultValue equal to the sent value"
    );
    for path in [
        "_types_/Family/Motor",
        "Line1/Motor1",
        "Line1/Ticker",
        "Line1/SlowTicker",
        "Line2/Level",
        "Line2/Motor2",
    ] {
        assert!(
            node_in(&export1, path).is_some(),
            "{path} is live after step 1"
        );
    }
    let group_units: Vec<String> = ["Fast", "Slow"]
        .iter()
        .map(|g| {
            format!(
                "{g}={}",
                std::fs::read_to_string(rig.group_dir().join(g).join("config.json"))
                    .unwrap_or_default()
                    .trim()
            )
        })
        .collect();
    eprintln!("step 1 group units (mode collection): {group_units:?}");

    // ---- Q6: does the gateway load a deployed group? A control expression
    // tag naming an absent group, imported out of band, beside the deployed
    // ones in `Fast` (Direct 500 ms) and `Slow` (Leased, rate 60 s).
    let control = raw_import(
        &api,
        P26_PROVIDER,
        "",
        CollisionPolicy::Abort,
        &json!({"tags": [{"name": "Q6", "tagType": "Folder", "tags": [
            {"name": "Control", "tagType": "AtomicTag", "dataType": "DateTime",
             "valueSource": "expr", "executionMode": "TagGroupRate", "expression": "now(0)",
             "tagGroup": "NoSuchGroup"}
        ]}]}),
    )
    .await;
    assert!(
        control.is_clean(),
        "the Q6 control imports clean: {control:?}"
    );
    tokio::time::sleep(Duration::from_secs(5)).await;
    let q6_paths = [
        "[P26Live]Line1/Ticker",
        "[P26Live]Line1/SlowTicker",
        "[P26Live]Q6/Control",
    ];
    let mut q6_reads: Vec<Vec<(String, String, String)>> = Vec::new();
    for _ in 0..4 {
        q6_reads.push(read_tags(&rig.url, &q6_paths).await);
        tokio::time::sleep(Duration::from_millis(1500)).await;
    }
    let mut q6_lines = Vec::new();
    for (i, path) in q6_paths.iter().enumerate() {
        let qualities: BTreeSet<String> = q6_reads.iter().map(|r| r[i].0.clone()).collect();
        let values: BTreeSet<String> = q6_reads.iter().map(|r| r[i].1.clone()).collect();
        q6_lines.push(format!(
            "{path}: qualities {qualities:?}, {} distinct value(s) in 4 reads over 6 s",
            values.len()
        ));
    }
    for line in &q6_lines {
        eprintln!("Q6 {line}");
    }

    // ---- step 2: runtime writes (C2 form), a drift tag, then a release that
    // changes a tag's documentation and the type (Q5).
    let runtime = raw_import(
        &api,
        P26_PROVIDER,
        "Line1",
        CollisionPolicy::MergeOverwrite,
        &json!({"tags": [
            {"name": "Setpoint", "tagType": "AtomicTag", "value": 77},
            {"name": "Motor1", "tagType": "UdtInstance", "typeId": "Family/Motor",
             "parameters": {"Line": {"dataType": "Integer", "value": 7}},
             "tags": [{"name": "Speed", "tagType": "AtomicTag",
                       "documentation": "inst doc", "value": 33}]}
        ]}),
    )
    .await;
    assert!(runtime.is_clean(), "the runtime writes: {runtime:?}");
    let drift = raw_import(
        &api,
        P26_PROVIDER,
        "Line1",
        CollisionPolicy::Abort,
        &json!({"tags": [{"name": "Drift", "tagType": "AtomicTag", "dataType": "Int4",
                          "valueSource": "memory", "value": 3}]}),
    )
    .await;
    assert!(drift.is_clean(), "the drift tag: {drift:?}");
    let before2 = export_of(&api, P26_PROVIDER).await;
    assert_eq!(show(&node_in(&before2, "Line1/Setpoint"), "value"), "77");
    let member = node_at(&before2, "Line1/Motor1/Speed").cloned();
    assert_eq!(
        show(&member, "value"),
        "33",
        "the instance member holds its runtime value before step 2"
    );

    let mut v2 = v1.clone();
    v2.insert(
        "Line1/Setpoint".to_string(),
        json!({"dataType": "Int4", "documentation": "setpoint v2", "tagType": "AtomicTag",
               "value": 5, "valueSource": "memory"}),
    );
    v2.insert(
        "_types_/Family/Motor".to_string(),
        motor_type("speed v2", 12),
    );
    repo.set_release(&v2);
    let (check2, applied) = checked_apply(
        "step2-change",
        &api,
        &repo,
        RIG_ENV,
        "v1.1.0",
        false,
        None,
        budget,
    )
    .await;
    let changed2 = changed_paths(&tags_of(&check2));
    assert!(
        changed2.contains(&"[P26Live]Line1/Setpoint".to_string())
            && changed2.contains(&"[P26Live]_types_/Family/Motor".to_string()),
        "step 2 plans the tag and the type change: {changed2:?}"
    );
    let step2 = applied.unwrap_or_else(|err| panic!("step 2: {} ({err})", err.code()));
    cleanup.record_run(&step2);
    assert_eq!(step2.outcome, ProjectOutcome::Applied, "step 2 outcome");
    let step2_run = step2.run_id.clone().expect("step 2 has a run id");
    let after2 = export_of(&api, P26_PROVIDER).await;
    let setpoint = node_in(&after2, "Line1/Setpoint");
    let motor1 = node_in(&after2, "Line1/Motor1");
    let speed = node_at(&after2, "Line1/Motor1/Speed").cloned();
    let type_speed = node_at(&after2, "_types_/Family/Motor/Speed").cloned();
    let q5 = format!(
        "Setpoint value={} documentation={}; Motor1 parameters={}; Motor1/Speed documentation={} value={}; type Speed documentation={} value={}; Drift present={}; Q6/Control present={}",
        show(&setpoint, "value"),
        show(&setpoint, "documentation"),
        show(&motor1, "parameters"),
        show(&speed, "documentation"),
        show(&speed, "value"),
        show(&type_speed, "documentation"),
        show(&type_speed, "value"),
        node_in(&after2, "Line1/Drift").is_some(),
        node_in(&after2, "Q6/Control").is_some(),
    );
    eprintln!("step 2 Q5: {q5}");
    assert_eq!(
        show(&setpoint, "value"),
        "77",
        "D-03: the runtime value survives"
    );
    assert_eq!(show(&setpoint, "documentation"), "\"setpoint v2\"");
    assert_eq!(show(&type_speed, "documentation"), "\"speed v2\"");
    assert_eq!(
        show(&type_speed, "value"),
        "12",
        "the type change is applied"
    );
    assert_eq!(
        motor1
            .as_ref()
            .map(|m| m["parameters"]["Line"]["value"].clone()),
        Some(json!(7)),
        "Q5: the instance keeps its parameter"
    );
    assert_eq!(
        show(&speed, "documentation"),
        "\"inst doc\"",
        "Q5: the instance keeps its member override"
    );
    assert_eq!(
        show(&speed, "value"),
        "33",
        "Q5: the instance member keeps its live value across a type change"
    );
    assert!(
        node_in(&after2, "Line1/Drift").is_some(),
        "the drift tag survives"
    );

    // ---- step 5 (run here: `--rollback-run` reads the latest run's host
    // copy, so it must follow the run it undoes): undo step 2.
    let started = Instant::now();
    let undo = run_projects(
        &api,
        &repo.input(RIG_ENV, ""),
        false,
        Some(&step2_run),
        None,
        budget,
    )
    .await
    .unwrap_or_else(|err| panic!("step 5 rollback-run: {} ({err})", err.code()));
    cleanup.record_run(&undo);
    save_evidence("step5-rollback-run", &result_json(&undo));
    eprintln!(
        "{}",
        step_line("step5-rollback-run", &undo, started.elapsed().as_millis())
    );
    assert!(
        matches!(
            undo.outcome,
            ProjectOutcome::TagsRolledBack | ProjectOutcome::RolledBack
        ),
        "the undo is a rolled-back verdict: {}",
        undo.outcome.as_str()
    );
    let after5 = export_of(&api, P26_PROVIDER).await;
    let setpoint = node_in(&after5, "Line1/Setpoint");
    let speed = node_at(&after5, "Line1/Motor1/Speed").cloned();
    let type_speed = node_at(&after5, "_types_/Family/Motor/Speed").cloned();
    eprintln!(
        "step 5: Setpoint value={} documentation={}; Motor1/Speed value={}; type Speed documentation={} value={}",
        show(&setpoint, "value"),
        show(&setpoint, "documentation"),
        show(&speed, "value"),
        show(&type_speed, "documentation"),
        show(&type_speed, "value"),
    );
    assert_eq!(
        show(&setpoint, "documentation"),
        "<absent>",
        "the old documentation"
    );
    assert_eq!(
        show(&type_speed, "documentation"),
        "\"speed v1\"",
        "the old type"
    );
    assert_eq!(show(&type_speed, "value"), "10", "the old type default");
    assert_eq!(
        show(&setpoint, "value"),
        "77",
        "the live value is kept by the undo"
    );
    assert_eq!(
        show(&speed, "value"),
        "33",
        "the member's live value is kept by the undo"
    );

    // ---- step 3: a release that removes a tag is refused with zero writes.
    let mut v3 = v1.clone();
    v3.remove("Line2/Level");
    repo.set_release(&v3);
    let before3 = export_of(&api, P26_PROVIDER).await;
    let (check3, applied) = checked_apply(
        "step3-removal",
        &api,
        &repo,
        RIG_ENV,
        "v1.2.0",
        false,
        None,
        budget,
    )
    .await;
    assert_eq!(
        strings(&tags_of(&check3)["removed"]),
        ["[P26Live]Line2/Level"],
        "the check lists the removal"
    );
    let err = applied.expect_err("a removal is refused");
    let gate = match &err {
        CoreError::DestructiveChangeRefused { gate, .. } => gate.clone(),
        other => panic!("expected destructive_change_refused, got {}", other.code()),
    };
    eprintln!("step 3: refused {} gate {gate}", err.code());
    assert_eq!(gate, "tag_removal");
    let after3 = export_of(&api, P26_PROVIDER).await;
    assert!(
        exports_converged(&before3, &after3) && exports_converged(&after3, &before3),
        "zero writes: the export is unchanged"
    );

    // ---- step 4: a release that adds a tooltip to the runtime-valued tag
    // (batch Line1, first) and changes Motor2's type to a missing one (batch
    // Line2, 528 on 8.3.6 with the node changed in part) is rolled back.
    let mut v4 = v1.clone();
    v4.insert(
        "Line1/Setpoint".to_string(),
        json!({"dataType": "Int4", "tagType": "AtomicTag", "tooltip": "tip", "value": 5,
               "valueSource": "memory"}),
    );
    v4.insert(
        "Line2/Motor2".to_string(),
        motor_instance("Family/Missing", 2, "m2 doc"),
    );
    repo.set_release(&v4);
    let before4 = export_of(&api, P26_PROVIDER).await;
    let (check4, applied) = checked_apply(
        "step4-forced-failure",
        &api,
        &repo,
        RIG_ENV,
        "v1.3.0",
        false,
        None,
        budget,
    )
    .await;
    eprintln!(
        "step 4 plan changed: {:?}",
        changed_paths(&tags_of(&check4))
    );
    let step4 = applied.unwrap_or_else(|err| panic!("step 4: {} ({err})", err.code()));
    cleanup.record_run(&step4);
    let tags4 = tags_of(&step4);
    eprintln!("step 4 rollback detail: {}", tags4["rollback"]["detail"]);
    assert_eq!(
        step4.outcome,
        ProjectOutcome::TagsRolledBack,
        "step 4 outcome"
    );
    assert_eq!(
        tags4["rollback"]["project_reimported"], false,
        "no project import"
    );
    assert!(
        strings(&tags4["rollback"]["carried"]).contains(&"[P26Live]Line1/Setpoint".to_string()),
        "the rollback carried the runtime-valued tag: {}",
        tags4["rollback"]
    );
    let after4 = export_of(&api, P26_PROVIDER).await;
    let setpoint = node_in(&after4, "Line1/Setpoint");
    eprintln!(
        "step 4: Setpoint tooltip={} value={}; export equal to the pre-image={}",
        show(&setpoint, "tooltip"),
        show(&setpoint, "value"),
        exports_converged(&before4, &after4)
    );
    assert!(
        exports_converged(&before4, &after4) && exports_converged(&after4, &before4),
        "the export equals the pre-image"
    );
    assert_eq!(
        show(&setpoint, "tooltip"),
        "<absent>",
        "the tooltip is gone"
    );
    assert_eq!(
        show(&setpoint, "value"),
        "77",
        "the carried Overwrite kept the step 2 runtime value"
    );

    // ---- step 8 (26-14 batch split, 002d621): a release that adds a new
    // folder whose nested body is over the import cap. The run goes through a
    // recording tap in front of the rig, so every body it sends is measured.
    let bulk_count: usize = env_or("IGN_LIVE_SPLIT_TAGS", "7000")
        .parse()
        .expect("a count");
    let mut v8 = v1.clone();
    for i in 0..bulk_count {
        v8.insert(
            format!("Bulk/T{i:05}"),
            json!({"dataType": "Int4", "tagType": "AtomicTag", "value": i as i64,
                   "valueSource": "memory"}),
        );
    }
    repo.set_release(&v8);
    let master_tap = MethodTap::recording(&rig.url);
    let api_tapped = token_api(&master_tap.url(), &rig.token);
    let (check8, applied) = checked_apply(
        "step8-batch-split",
        &api_tapped,
        &repo,
        RIG_ENV,
        "v1.4.0",
        false,
        None,
        budget,
    )
    .await;
    assert_eq!(
        tags_of(&check8)["added_count"],
        json!(bulk_count + 1),
        "the folder and its tags are added"
    );
    let step8 = applied.unwrap_or_else(|err| panic!("step 8: {} ({err})", err.code()));
    cleanup.record_run(&step8);
    assert_eq!(step8.outcome, ProjectOutcome::Applied, "step 8 outcome");
    assert_eq!(tags_of(&step8)["verify"]["equal"], true, "step 8 verify");
    let imports: Vec<Seen> = master_tap
        .all()
        .into_iter()
        .filter(|seen| seen.method == "POST" && seen.target.starts_with("/data/api/v1/tags/import"))
        .collect();
    let largest = imports.iter().map(|seen| seen.body_len).max().unwrap_or(0);
    let total: usize = imports.iter().map(|seen| seen.body_len).sum();
    eprintln!(
        "step 8 batch split: {} tag import(s) for {} added nodes, largest body {largest} bytes (cap {IMPORT_BATCH_MAX_BYTES}), {total} bytes in all; tap saw {}",
        imports.len(),
        bulk_count + 1,
        master_tap.summary()
    );
    for seen in &imports {
        eprintln!("step 8 import: {} bytes {}", seen.body_len, seen.target);
    }
    assert!(
        total > IMPORT_BATCH_MAX_BYTES,
        "the added folder is over the cap as one body"
    );
    assert!(imports.len() >= 2, "the folder was split");
    assert!(
        largest <= IMPORT_BATCH_MAX_BYTES,
        "no import body is over the cap"
    );
    let bulk = export_of(&api, P26_PROVIDER).await;
    let children = node_at(&bulk, "Bulk")
        .and_then(|folder| folder["tags"].as_array().map(Vec::len))
        .unwrap_or(0);
    eprintln!("step 8: the export holds Bulk with {children} tags");
    assert_eq!(children, bulk_count, "every split batch landed");

    // ---- step 6: one Abort import just under the cap (R3-BATCH), then a
    // larger provider for the export time (26-14 F2).
    fresh_provider(&api, P26_BATCH_PROVIDER).await;
    let extra: usize = env_or("IGN_LIVE_BATCH_EXTRA", "3")
        .parse()
        .expect("a count");
    let mut total_tags = 0usize;
    for b in 0..=extra {
        let (body, count) = batch_body(&format!("B{b}F"), IMPORT_BATCH_MAX_BYTES);
        let size = body.len();
        let started = Instant::now();
        let answer = GatewayProjects(&api)
            .tag_import(P26_BATCH_PROVIDER, "", CollisionPolicy::Abort, body)
            .await
            .unwrap_or_else(|err| panic!("batch {b}: {}", err.code()));
        let elapsed = started.elapsed();
        total_tags += count;
        eprintln!(
            "R3-BATCH batch {b}: {size} bytes (cap {IMPORT_BATCH_MAX_BYTES}), {count} tags, answered in {:.1} s, success_count={:?} failure_count={:?}",
            elapsed.as_secs_f64(),
            answer.success_count,
            answer.failure_count
        );
        assert!(answer.is_clean(), "batch {b} answer: {answer:?}");
        assert!(size < IMPORT_BATCH_MAX_BYTES);
        assert!(
            elapsed < TAGS_IMPORT_TIMEOUT,
            "batch {b} took longer than the import timeout"
        );
    }
    let started = Instant::now();
    let export = GatewayProjects(&api)
        .tag_export(P26_BATCH_PROVIDER)
        .await
        .unwrap_or_else(|err| panic!("the batch provider export: {}", err.code()));
    eprintln!(
        "export time: provider of {total_tags} memory tags, {} bytes, exported in {:.1} s",
        export.len(),
        started.elapsed().as_secs_f64()
    );
    delete_provider(&api, P26_BATCH_PROVIDER).await;

    // ---- step 7: valueSource in exports (26-08 open point 3).
    fresh_provider(&api, P26_VS_PROVIDER).await;
    let a = raw_import(
        &api,
        P26_VS_PROVIDER,
        "",
        CollisionPolicy::Abort,
        &json!({"tags": [
            {"name": "A", "tagType": "AtomicTag", "dataType": "Int4", "value": 1},
            {"name": "B", "tagType": "AtomicTag", "dataType": "Int4", "valueSource": "memory",
             "value": 2},
            {"name": "_types_", "tagType": "Folder", "tags": [
                {"name": "VsType", "tagType": "UdtType", "tags": [
                    {"name": "M", "tagType": "AtomicTag", "dataType": "Int4",
                     "valueSource": "memory", "value": 0}
                ]}
            ]}
        ]}),
    )
    .await;
    assert!(a.is_clean(), "vs a, b and the type: {a:?}");
    let d = raw_import(
        &api,
        P26_VS_PROVIDER,
        "",
        CollisionPolicy::Abort,
        &json!({"tags": [{"name": "D", "tagType": "UdtInstance", "typeId": "VsType",
            "tags": [{"name": "M", "tagType": "AtomicTag", "documentation": "ov", "value": 4}]}]}),
    )
    .await;
    assert!(d.is_clean(), "vs d: {d:?}");
    // 8.3.6's OpenAPI has no tag create route besides tags/import, so (c) is
    // created with system.tag.configure through the tagConfig route.
    let (status, body) = route(
        &rig.url,
        "tagConfig",
        &json!({"action": "configure", "basePath": "[P26Vs]", "collisionPolicy": "a",
                "tags": [{"name": "C", "tagType": "AtomicTag", "dataType": "Int4", "value": 3}]}),
    )
    .await;
    eprintln!(
        "vs-c configure answered {status}: {}",
        body.as_ref()
            .map_or_else(String::new, |b| b["data"].to_string())
    );
    assert_eq!(status, 200);
    assert_eq!(body.as_ref().map(|b| b["ok"].clone()), Some(json!(true)));
    tokio::time::sleep(Duration::from_secs(2)).await;
    let vs = export_of(&api, P26_VS_PROVIDER).await;
    save_evidence("step7-valuesource-export", &vs);
    let has = |path: &str| -> &'static str {
        let node = node_at(&vs, path).unwrap_or_else(|| panic!("{path} is in the export"));
        if node.get("valueSource").is_some() {
            "present"
        } else {
            "absent"
        }
    };
    let lines = [
        format!("vs-a imported without: {}", has("A")),
        format!("vs-b imported memory: {}", has("B")),
        format!("vs-c created over REST: {}", has("C")),
        format!(
            "vs-d UDT member override: {} (type member: {})",
            has("D/M"),
            has("_types_/VsType/M")
        ),
    ];
    for line in &lines {
        eprintln!("{line}");
    }
    delete_provider(&api, P26_VS_PROVIDER).await;

    eprintln!(
        "project_tags_end_to_end: all steps green in {:.0} s",
        gateway_started.elapsed().as_secs_f64()
    );
    drop(cleanup);
}

// ---- the pair gate --------------------------------------------------------------

const PAIR_PROJECT: &str = "P26PairProject";
const PAIR_PROVIDER: &str = "P26Pair";
const PAIR_ENV: &str = "pair";
const PAIR_GATEWAY_DIR: &str = "pair-gw";
const PAIR_NAME_A: &str = "ign-pair-a";
const PAIR_NAME_B: &str = "ign-pair-b";

#[derive(Clone)]
struct PairRig {
    url_a: String,
    url_b: String,
    token_a: String,
    token_b: String,
    data_a: PathBuf,
    data_b: PathBuf,
}

impl PairRig {
    fn api_a(&self) -> ReqwestGatewayApi {
        token_api(&self.url_a, &self.token_a)
    }

    fn api_b_direct(&self) -> ReqwestGatewayApi {
        token_api(&self.url_b, &self.token_b)
    }

    fn group_dir(data: &Path) -> PathBuf {
        data.join("config/resources/production/ignition/tag-group")
            .join(PAIR_PROVIDER)
    }

    /// Tag definition or type definition paths of the provider in a node's
    /// mode collection (R3-MODE: there must be none).
    fn mode_tag_paths(data: &Path) -> Vec<String> {
        ["tag-definition", "tag-type-definition"]
            .iter()
            .map(|kind| {
                data.join("config/resources/production/ignition")
                    .join(kind)
                    .join(PAIR_PROVIDER)
            })
            .filter(|p| p.exists())
            .map(|p| p.display().to_string())
            .collect()
    }
}

async fn pair_rig() -> Option<PairRig> {
    if std::env::var("IGNITION_LIVE_PAIR").as_deref() != Ok("1") {
        skip("IGNITION_LIVE_PAIR is not 1");
        return None;
    }
    let url_a = env_or("IGN_PAIR_URL_A", "http://localhost:18300");
    let url_b = env_or("IGN_PAIR_URL_B", "http://localhost:18301");
    for (var, url) in [("IGN_PAIR_URL_A", &url_a), ("IGN_PAIR_URL_B", &url_b)] {
        if let Err(why) = local_url_guard(url) {
            panic!("{var} refused: {why}");
        }
    }
    let token_a = require_env("IGN_PAIR_TOKEN_A");
    let token_b = require_env("IGN_PAIR_TOKEN_B");
    let state = home().join(".cache/ign-pair-rig");
    let data_a = PathBuf::from(env_or(
        "IGN_PAIR_DATA_A",
        &state.join("data-a").display().to_string(),
    ));
    let data_b = PathBuf::from(env_or(
        "IGN_PAIR_DATA_B",
        &state.join("data-b").display().to_string(),
    ));
    for (var, dir) in [("IGN_PAIR_DATA_A", &data_a), ("IGN_PAIR_DATA_B", &data_b)] {
        assert!(
            dir.join("config/resources").is_dir(),
            "{var} must be the host data dir whose config/resources is bind-mounted"
        );
        assert!(
            !dir.canonicalize()
                .expect("canonical")
                .starts_with(repo_root()),
            "{var} must be outside the repository"
        );
    }
    // The project host's `.git` probe reads `<data>/projects`; gw-a's
    // projects are not bound, so an empty host directory stands in.
    std::fs::create_dir_all(data_a.join("projects")).expect("projects dir");
    let rig = PairRig {
        url_a,
        url_b,
        token_a,
        token_b,
        data_a,
        data_b,
    };
    let info_a = GatewayApi::gateway_info(&rig.api_a())
        .await
        .expect("gateway-info of gw-a for the name guard");
    assert_eq!(
        info_a.name.as_deref(),
        Some(PAIR_NAME_A),
        "refusing: gw-a is not the {PAIR_NAME_A} fixture"
    );
    let info_b = GatewayApi::gateway_info(&rig.api_b_direct())
        .await
        .expect("gateway-info of gw-b for the name guard");
    assert!(
        matches!(info_b.name.as_deref(), Some(PAIR_NAME_A | PAIR_NAME_B)),
        "refusing: gw-b is not the pair rig's backup"
    );
    Some(rig)
}

struct PairCleanup {
    rig: PairRig,
}

impl PairCleanup {
    async fn sweep(rig: PairRig) {
        let api = rig.api_a();
        delete_project(&api, PAIR_PROJECT).await;
        delete_provider(&api, PAIR_PROVIDER).await;
        let had_groups = PairRig::group_dir(&rig.data_a).exists();
        remove_dir(&PairRig::group_dir(&rig.data_a));
        if had_groups {
            config_scan(&api).await;
        }
        remove_dir(&rig.data_a.join("ign-deploy").join(PAIR_GATEWAY_DIR));
    }
}

impl Drop for PairCleanup {
    fn drop(&mut self) {
        let rig = self.rig.clone();
        block_on_cleanup(async move {
            PairCleanup::sweep(rig.clone()).await;
            // gw-b follows the master's delete through the sync.
            let started = Instant::now();
            let mut left_b = true;
            while started.elapsed() < Duration::from_secs(120) {
                left_b = rig
                    .api_b_direct()
                    .tag_provider_find(PAIR_PROVIDER)
                    .await
                    .is_ok()
                    || PairRig::group_dir(&rig.data_b).exists();
                if !left_b {
                    break;
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            eprintln!(
                "cleanup: P26Pair left on gw-a {}, on gw-b {left_b}",
                rig.api_a().tag_provider_find(PAIR_PROVIDER).await.is_ok()
            );
        });
    }
}

/// Seconds until gw-b's export of the provider equals `master` (value keys
/// removed, children keyed by name), polled directly every second.
async fn backup_equal_after(rig: &PairRig, master: &Value, max: Duration) -> Option<f64> {
    let started = Instant::now();
    loop {
        if let Ok(bytes) = GatewayProjects(&rig.api_b_direct())
            .tag_export(PAIR_PROVIDER)
            .await
            && exports_converged(master, &parse_export(&bytes))
        {
            return Some(started.elapsed().as_secs_f64());
        }
        if started.elapsed() >= max {
            return None;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// 26-15 Task 2: apply and rollback converge on gw-b with every gw-b request
/// a GET.
#[tokio::test]
#[ignore = "needs the 8.3.6 pair rig: see the module doc"]
async fn project_tags_pair_end_to_end() {
    let Some(rig) = pair_rig().await else {
        return;
    };
    eprintln!("LIVE-RAN project_tags_pair_end_to_end");
    let started_all = Instant::now();
    PairCleanup::sweep(rig.clone()).await;
    let cleanup = PairCleanup { rig: rig.clone() };
    let api_a = rig.api_a();
    let tap = MethodTap::start(&rig.url_b);
    let api_b_tap = token_api(&tap.url(), &rig.token_b);
    let backup_node = BackupNode::new(&api_b_tap, &api_b_tap, None);
    let backup: Option<&dyn BackupTags> = Some(&backup_node);
    let budget = Duration::from_secs(300);

    let target_yaml = format!(
        "schema_version: 1\ngateway: {PAIR_GATEWAY_DIR}\nmode: production\nnodes:\n  master:\n    \
         profile: pair-a\n    gateway_name: {PAIR_NAME_A}\n    local:\n      data_dir: '{}'\n  \
         backup:\n    profile: pair-b\n    local:\n      data_dir: '{}'\n",
        rig.data_a.display(),
        rig.data_b.display()
    );
    let repo = TagRepo::new(PAIR_PROJECT, PAIR_PROVIDER, PAIR_ENV, &target_yaml);
    let mut v1 = base_release();
    // The Leased group and its tag are the single node's concern (Q6).
    v1.remove("Line1/SlowTicker");
    put(
        &repo.repo,
        &format!("tags/{PAIR_PROVIDER}/.tag-groups.json"),
        &serde_json::to_vec_pretty(
            &json!({"tagGroups": [{"mode": "Direct", "name": "Fast", "rate": 500}]}),
        )
        .expect("groups"),
    );
    repo.set_release(&v1);

    let mut convergence = Vec::new();

    // ---- the first deploy.
    let (_, applied) = checked_apply(
        "pair-step1-first-deploy",
        &api_a,
        &repo,
        PAIR_ENV,
        "v1.0.0",
        true,
        backup,
        budget,
    )
    .await;
    let step1 = applied.unwrap_or_else(|err| panic!("pair step 1: {} ({err})", err.code()));
    let tags = tags_of(&step1);
    assert_eq!(
        step1.outcome,
        ProjectOutcome::Applied,
        "pair step 1 outcome"
    );
    assert_eq!(
        tags["backup"]["converged"], true,
        "gw-b converged: {}",
        tags["backup"]
    );
    convergence.push(format!("apply 1: {} s", tags["backup"]["elapsed_s"]));

    // Carry-in (26-13 O2): does gw-b's export keep `_types_` member values?
    let master = export_of(&api_a, PAIR_PROVIDER).await;
    let started = Instant::now();
    let on_b = export_of(&rig.api_b_direct(), PAIR_PROVIDER).await;
    let look_s = started.elapsed().as_secs_f64();
    let type_member_a = node_at(&master, "_types_/Family/Motor/Speed").cloned();
    let type_member_b = node_at(&on_b, "_types_/Family/Motor/Speed").cloned();
    eprintln!(
        "26-13 O2: type member Speed value on gw-a={} on gw-b={}; one gw-b export took {look_s:.2} s",
        show(&type_member_a, "value"),
        show(&type_member_b, "value"),
    );

    // ---- a runtime write on gw-a (C2 form), then a change deploy.
    let runtime = raw_import(
        &api_a,
        PAIR_PROVIDER,
        "Line1",
        CollisionPolicy::MergeOverwrite,
        &json!({"tags": [{"name": "Setpoint", "tagType": "AtomicTag", "value": 77}]}),
    )
    .await;
    assert!(runtime.is_clean(), "the runtime write: {runtime:?}");
    let mut v2 = v1.clone();
    v2.insert(
        "Line1/Setpoint".to_string(),
        json!({"dataType": "Int4", "documentation": "setpoint v2", "tagType": "AtomicTag",
               "value": 5, "valueSource": "memory"}),
    );
    v2.insert(
        "_types_/Family/Motor".to_string(),
        motor_type("speed v2", 12),
    );
    repo.set_release(&v2);
    let (_, applied) = checked_apply(
        "pair-step2-change",
        &api_a,
        &repo,
        PAIR_ENV,
        "v1.1.0",
        false,
        backup,
        budget,
    )
    .await;
    let step2 = applied.unwrap_or_else(|err| panic!("pair step 2: {} ({err})", err.code()));
    let tags = tags_of(&step2);
    assert_eq!(
        step2.outcome,
        ProjectOutcome::Applied,
        "pair step 2 outcome"
    );
    assert_eq!(
        tags["backup"]["converged"], true,
        "gw-b converged: {}",
        tags["backup"]
    );
    convergence.push(format!("apply 2: {} s", tags["backup"]["elapsed_s"]));
    let after2 = export_of(&api_a, PAIR_PROVIDER).await;
    assert_eq!(
        show(&node_in(&after2, "Line1/Setpoint"), "value"),
        "77",
        "gw-a keeps the runtime value"
    );

    // ---- a forced failure rolls back and gw-b converges back.
    let mut v3 = v2.clone();
    v3.insert(
        "Line1/Setpoint".to_string(),
        json!({"dataType": "Int4", "documentation": "setpoint v2", "tagType": "AtomicTag",
               "tooltip": "tip", "value": 5, "valueSource": "memory"}),
    );
    v3.insert(
        "Line2/Motor2".to_string(),
        motor_instance("Family/Missing", 2, "m2 doc"),
    );
    repo.set_release(&v3);
    let pre = export_of(&api_a, PAIR_PROVIDER).await;
    let (_, applied) = checked_apply(
        "pair-step3-forced-failure",
        &api_a,
        &repo,
        PAIR_ENV,
        "v1.2.0",
        false,
        backup,
        budget,
    )
    .await;
    let step3 = applied.unwrap_or_else(|err| panic!("pair step 3: {} ({err})", err.code()));
    let tags = tags_of(&step3);
    assert_eq!(
        step3.outcome,
        ProjectOutcome::TagsRolledBack,
        "pair step 3 outcome"
    );
    assert_eq!(
        tags["backup"]["converged"], true,
        "gw-b converged back: {}",
        tags["backup"]
    );
    convergence.push(format!("rollback: {} s", tags["backup"]["elapsed_s"]));
    let master_after = export_of(&api_a, PAIR_PROVIDER).await;
    assert!(
        exports_converged(&pre, &master_after),
        "gw-a equals its pre-image"
    );
    let b_equal = backup_equal_after(&rig, &pre, Duration::from_secs(30)).await;
    eprintln!("pair: gw-b equals the pre-image after {b_equal:?} s (direct read)");
    assert!(b_equal.is_some(), "gw-b equals the pre-image");

    // ---- only GETs reached gw-b, and no tag definition path is in either
    // node's mode collection.
    eprintln!(
        "pair: tap saw {} request(s): {}",
        tap.total(),
        tap.summary()
    );
    assert!(tap.total() > 0, "the backup was read");
    assert!(
        tap.non_gets().is_empty(),
        "a non-GET request reached the tap: {:?}",
        tap.non_gets()
    );
    for (node, data) in [("gw-a", &rig.data_a), ("gw-b", &rig.data_b)] {
        let paths = PairRig::mode_tag_paths(data);
        assert!(
            paths.is_empty(),
            "{node} holds tag definition units in its mode collection: {paths:?}"
        );
    }
    eprintln!("pair convergence: {}", convergence.join(", "));
    eprintln!(
        "project_tags_pair_end_to_end: all steps green in {:.0} s",
        started_all.elapsed().as_secs_f64()
    );
    drop(cleanup);
}
