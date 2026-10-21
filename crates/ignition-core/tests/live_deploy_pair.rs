//! Phase 24's rig gates for the redundant pair (24-10): an apply whose backup
//! converges, an apply whose backup is cut off and ends
//! `applied_backup_unsynced` without undoing the master, and a verify that goes
//! red on a secret name a node does not hold.
//!
//! The master (gw-a) and the backup (gw-b) are `local:` nodes on the rig's two
//! bind-mounted data directories, so the engine writes gw-a's mode collection
//! with plain file moves and reads both collections back. The backup's REST
//! surface is reached through [`MethodTap`], an in-test HTTP tap in front of
//! gw-b that records the method and path of every request and refuses to
//! forward anything but a read (DPAIR-04: no `ign` path may write to the
//! backup). Nothing here is mocked.
//!
//! ```text
//! cd crates/ignition-core/tests/fixtures/live-rig-pair
//! ./setup-pair.sh up && ./setup-pair.sh mint && ./setup-pair.sh pair && ./setup-pair.sh mode
//! . ~/.cache/ign-pair-rig/tokens.env
//! IGNITION_LIVE_PAIR=1 IGN_PAIR_URL_A=http://localhost:18300 IGN_PAIR_URL_B=http://localhost:18301 \
//!   IGN_PAIR_DATA_A="$HOME/.cache/ign-pair-rig/data-a" IGN_PAIR_DATA_B="$HOME/.cache/ign-pair-rig/data-b" \
//!   cargo test -p ignition-core --test live_deploy_pair -- --ignored --nocapture --test-threads=1
//! ```
//!
//! | var | meaning |
//! |---|---|
//! | `IGNITION_LIVE_PAIR` | must equal `1`; anything else is a quiet green skip |
//! | `IGN_PAIR_URL_A` | gw-a's (the master's) base URL; loopback hosts only |
//! | `IGN_PAIR_URL_B` | gw-b's (the backup's) base URL; loopback hosts only |
//! | `IGN_PAIR_TOKEN_A` | gw-a's full `name:key` API token (never printed); tokens are node-local (C-P4) |
//! | `IGN_PAIR_TOKEN_B` | gw-b's full `name:key` API token (never printed) |
//! | `IGN_PAIR_DATA_A` | the host data directory bind-mounted into gw-a |
//! | `IGN_PAIR_DATA_B` | the host data directory bind-mounted into gw-b |
//! | `IGN_PAIR_MODE` | the rig's active mode, default `production` |
//!
//! Safety: the URL guard refuses a non-loopback host before a request is made,
//! and two name guards refuse a gateway whose `gateway-info` name is not the
//! rig's (`ign-pair-a` for gw-a; gw-b reads the master's name, C-P4, so it may
//! also be `ign-pair-b`). The gates share one pair, so they take a
//! process-wide lock. Every gate removes what it wrote from gw-a's collection,
//! scans, restores the gateway-network link when it cut it and waits for the
//! pair to settle, even when an assertion fails. Output carries counts, run
//! ids, reasons and timings, never a token, a secret value or a file's bytes.

mod common;

use std::collections::BTreeMap;
use std::future::Future;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ignition_core::actions::deploy::{
    Declared, DeployVerifyResult, PlanOptions, VerifyNode, VerifyOptions, deploy_plan,
    deploy_verify_node, load_declared, load_target,
};
use ignition_core::actions::deploy_apply::{
    ApplyPrepared, ApplyRunOptions, ApplySurfaces, DeployApplyResult, PrepareOptions,
    deploy_apply_prepare, deploy_apply_run,
};
use ignition_core::client::deploy_api::{SecretNameListing, scan_and_wait};
use ignition_core::client::scan::ScanKind;
use ignition_core::client::{DeployApi, GatewayApi, ReqwestGatewayApi};
use ignition_core::config::{Credential, Secret};
use ignition_core::deploy::apply::gates::GateFlags;
use ignition_core::deploy::apply::preflight::DesignersPolicy;
use ignition_core::deploy::apply::{ApplyEvent, ApplyObserver, NoObserver, OutcomeKind};
use ignition_core::deploy::pair::{BackupCheckId, BackupNode, UnsyncedReason};
use ignition_core::deploy::transport::{self, TreeReader};
use ignition_core::deploy::verify::CheckStatus;
use ignition_core::poll::PollConfig;

/// gw-a's gateway name; gw-b reads the master's name as well (C-P4).
const NAME_A: &str = "ign-pair-a";
const NAME_B: &str = "ign-pair-b";

/// The gateway directory name in the repo and the environment name.
const GATEWAY_DIR: &str = "pair-gw";
const ENV: &str = "pair";

/// The tracer fixture's gateway directory: a `database-translator` and the
/// alarm settings, four files.
const TRACER_GATEWAY: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/deploy/repos/tracer/gateways/tracer-gw"
);

/// The rig driver (`cut`, `link`, `secrets`, `secrets-rm`).
const SETUP_PAIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/live-rig-pair/setup-pair.sh"
);

/// Units a gate may write into gw-a's mode collection, below
/// `<mode>/ignition/`: removed by the cleanup.
const GATE_UNITS: [&str; 3] = [
    "database-translator/POSTGRES",
    "general-alarm-settings",
    "oauth2-client/CAP24_GATE",
];

/// The tests share one pair; they run one at a time.
static PAIR_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
        .unwrap_or_else(|| panic!("{name} must be set to run the live pair gates"))
}

#[test]
fn local_url_guard_refuses_non_loopback() {
    for raw in [
        "http://10.4.8.15:8088",
        "https://wignvm01.example.local:8043",
        "http://localhost.evil.example:18300",
        "http://127.0.0.2:18300",
        "http://52.154.75.231:8088",
        "not a url",
    ] {
        assert!(local_url_guard(raw).is_err(), "must refuse {raw}");
    }
}

#[test]
fn local_url_guard_accepts_loopback() {
    for raw in [
        "http://localhost:18300",
        "http://127.0.0.1:18301",
        "http://[::1]:18300",
    ] {
        assert!(local_url_guard(raw).is_ok(), "must accept {raw}");
    }
}

// ---- the tap ---------------------------------------------------------------------------

/// An in-test HTTP/1.1 tap on `127.0.0.1:0` in front of gw-b.
///
/// Per client connection it reads one request head (and a body by
/// `Content-Length`), records `(method, path)` and nothing else (no header, no
/// body), and forwards a `GET` or `HEAD` to gw-b with `Connection: close`,
/// copying the answer back until EOF. Any other method is recorded and
/// answered `405` without being forwarded, so a stray write cannot reach gw-b
/// even while the gate that sees it fails (T-24-36).
struct MethodTap {
    addr: SocketAddr,
    seen: Arc<Mutex<Vec<(String, String)>>>,
}

impl MethodTap {
    fn start(upstream_url: &str) -> MethodTap {
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
        let seen: Arc<Mutex<Vec<(String, String)>>> = Arc::default();
        let log = Arc::clone(&seen);
        std::thread::spawn(move || {
            for conn in listener.incoming() {
                let Ok(conn) = conn else { continue };
                let log = Arc::clone(&log);
                let authority = authority.clone();
                std::thread::spawn(move || {
                    let _ = relay(conn, &authority, &log);
                });
            }
        });
        MethodTap { addr, seen }
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Requests seen, by method.
    fn counts(&self) -> BTreeMap<String, usize> {
        let mut out = BTreeMap::new();
        for (method, _) in self.seen.lock().expect("tap log").iter() {
            *out.entry(method.clone()).or_insert(0) += 1;
        }
        out
    }

    fn total(&self) -> usize {
        self.seen.lock().expect("tap log").len()
    }

    /// Every recorded request whose method is not a read.
    fn writes(&self) -> Vec<(String, String)> {
        self.seen
            .lock()
            .expect("tap log")
            .iter()
            .filter(|(method, _)| method != "GET" && method != "HEAD")
            .cloned()
            .collect()
    }

    /// `GET 12, HEAD 0` style summary, methods only.
    fn summary(&self) -> String {
        let counts = self.counts();
        if counts.is_empty() {
            return "no requests".to_string();
        }
        counts
            .iter()
            .map(|(method, n)| format!("{method} {n}"))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Fail unless gw-b received reads only, and at least one of them.
    fn assert_reads_only(&self, what: &str) {
        assert!(
            self.writes().is_empty(),
            "{what}: a write reached the backup: {:?}",
            self.writes()
        );
        assert!(self.total() > 0, "{what}: the tap saw no request at all");
    }
}

/// The index just past the first `\r\n\r\n` of `buf`, if present.
fn head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
}

fn relay(
    mut client: TcpStream,
    upstream: &str,
    log: &Arc<Mutex<Vec<(String, String)>>>,
) -> std::io::Result<()> {
    client.set_read_timeout(Some(Duration::from_secs(120)))?;
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
    let path = target.split('?').next().unwrap_or_default().to_string();
    log.lock().expect("tap log").push((method.clone(), path));

    if method != "GET" && method != "HEAD" {
        client.write_all(
            b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )?;
        return Ok(());
    }

    // The request body (a GET has none in practice), read by Content-Length.
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

    // `localhost` may resolve to ::1 first while the rig publishes 127.0.0.1
    // only: take the first address that accepts.
    let mut up = upstream
        .to_socket_addrs()?
        .find_map(|addr| TcpStream::connect_timeout(&addr, Duration::from_secs(10)).ok())
        .ok_or_else(|| std::io::Error::other("upstream does not accept"))?;
    up.set_read_timeout(Some(Duration::from_secs(120)))?;
    up.write_all(forwarded.as_bytes())?;
    up.write_all(&body)?;
    std::io::copy(&mut up, &mut client)?;
    Ok(())
}

// ---- the pair --------------------------------------------------------------------------

/// What a live test needs once the gate is open.
#[derive(Clone)]
struct Pair {
    url_a: String,
    url_b: String,
    token_a: String,
    token_b: String,
    data_a: PathBuf,
    data_b: PathBuf,
    mode: String,
}

impl Pair {
    fn api_a(&self) -> ReqwestGatewayApi {
        ReqwestGatewayApi::for_tests(
            &self.url_a,
            Some(Credential::Token(Secret::new(self.token_a.clone()))),
        )
    }

    /// gw-b directly, for the harness (settle waits, name listings). The code
    /// under test reaches gw-b through [`MethodTap`] only.
    fn api_b_direct(&self) -> ReqwestGatewayApi {
        self.api_b_at(&self.url_b)
    }

    fn api_b_at(&self, url: &str) -> ReqwestGatewayApi {
        ReqwestGatewayApi::for_tests(
            url,
            Some(Credential::Token(Secret::new(self.token_b.clone()))),
        )
    }

    fn root(data: &Path, mode: &str) -> PathBuf {
        data.join("config").join("resources").join(mode)
    }

    /// Run `setup-pair.sh ARGS` with the rig's base URLs; stdout on success.
    /// The script never prints a token.
    fn script(&self, args: &[&str]) -> Result<String, String> {
        let out = std::process::Command::new("bash")
            .arg(SETUP_PAIR)
            .args(args)
            .env("BASE_A", &self.url_a)
            .env("BASE_B", &self.url_b)
            .output()
            .map_err(|err| format!("setup-pair.sh did not start: {err}"))?;
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if out.status.success() {
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let last = stderr.lines().last().unwrap_or_default();
            Err(format!(
                "setup-pair.sh {} exited {:?}: {last}",
                args.first().copied().unwrap_or_default(),
                out.status.code()
            ))
        }
    }

    /// The link is up, gw-b has nothing queued, both nodes' projects are
    /// `Good` and gw-b is the cold standby.
    async fn is_settled(&self) -> bool {
        let (Ok(a), Ok(b)) = (
            self.api_a().redundancy_status().await,
            self.api_b_direct().redundancy_status().await,
        ) else {
            return false;
        };
        b.peer_connected
            && !b.sync_pending
            && a.project_state.as_deref() == Some("Good")
            && b.project_state.as_deref() == Some("Good")
            && b.activity_level.as_deref() == Some("Cold")
    }

    /// Seconds until the pair settles; `Err` when `max` passes first.
    async fn wait_settled(&self, max: Duration) -> Result<u64, String> {
        let started = Instant::now();
        loop {
            if self.is_settled().await {
                return Ok(started.elapsed().as_secs());
            }
            if started.elapsed() >= max {
                return Err(format!(
                    "the pair did not settle within {} s",
                    max.as_secs()
                ));
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    }

    /// Units of this gate present in gw-b's mode collection.
    fn units_on_b(&self) -> Vec<String> {
        let root = Self::root(&self.data_b, &self.mode).join("ignition");
        GATE_UNITS
            .iter()
            .filter(|unit| root.join(unit).exists())
            .map(|unit| (*unit).to_string())
            .collect()
    }

    /// Remove what a gate wrote into gw-a's mode collection and the
    /// `ign-deploy/` directory.
    fn sweep_a(&self) {
        let root = Self::root(&self.data_a, &self.mode).join("ignition");
        for unit in GATE_UNITS {
            let _ = std::fs::remove_dir_all(root.join(unit));
        }
        let _ = std::fs::remove_dir_all(self.data_a.join("ign-deploy"));
    }

    /// Names a provider lists on one node; `None` when it cannot be read.
    async fn names_on(&self, api: &ReqwestGatewayApi, provider: &str) -> Option<Vec<String>> {
        match DeployApi::secret_names(api, provider).await {
            Ok(SecretNameListing::Listed(names)) => Some(names.into_iter().collect()),
            _ => None,
        }
    }
}

/// `None` is the quiet green skip. Every guard runs before the first request.
async fn pair() -> Option<Pair> {
    if std::env::var("IGNITION_LIVE_PAIR").as_deref() != Ok("1") {
        skip("IGNITION_LIVE_PAIR is not 1");
        return None;
    }
    let url_a = require_env("IGN_PAIR_URL_A");
    let url_b = require_env("IGN_PAIR_URL_B");
    let token_a = require_env("IGN_PAIR_TOKEN_A");
    let token_b = require_env("IGN_PAIR_TOKEN_B");
    let data_a = PathBuf::from(require_env("IGN_PAIR_DATA_A"));
    let data_b = PathBuf::from(require_env("IGN_PAIR_DATA_B"));
    let mode = std::env::var("IGN_PAIR_MODE")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "production".to_string());
    for (var, url) in [("IGN_PAIR_URL_A", &url_a), ("IGN_PAIR_URL_B", &url_b)] {
        if let Err(why) = local_url_guard(url) {
            panic!("{var} refused: {why}");
        }
    }
    for (var, dir) in [("IGN_PAIR_DATA_A", &data_a), ("IGN_PAIR_DATA_B", &data_b)] {
        assert!(
            dir.join("config").join("resources").is_dir(),
            "{var} must be the host data dir whose config/resources is bind-mounted"
        );
    }
    let pair = Pair {
        url_a,
        url_b,
        token_a,
        token_b,
        data_a,
        data_b,
        mode,
    };
    let info_a = GatewayApi::gateway_info(&pair.api_a())
        .await
        .expect("gateway-info read of gw-a for the name guard");
    assert_eq!(
        info_a.name.as_deref(),
        Some(NAME_A),
        "refusing: gw-a is not the {NAME_A} fixture"
    );
    let info_b = GatewayApi::gateway_info(&pair.api_b_direct())
        .await
        .expect("gateway-info read of gw-b for the name guard");
    assert!(
        matches!(info_b.name.as_deref(), Some(NAME_A | NAME_B)),
        "refusing: gw-b is not the pair rig's backup"
    );
    Some(pair)
}

fn utc_now() -> String {
    let out = std::process::Command::new("date")
        .args(["-u", "+%FT%TZ"])
        .output()
        .expect("date");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn production_poll(subject: &str) -> PollConfig {
    PollConfig {
        subject: subject.to_string(),
        interval: Duration::from_millis(500),
        max: Duration::from_secs(2),
        deadline: Duration::from_secs(120),
    }
}

/// The cleanup every gate ends with, pass or fail: remove the units it wrote
/// from gw-a, scan, restore the link if it is down, wait for the pair to
/// settle and for gw-b to drop the units. Returns what could not be done.
async fn cleanup(pair: &Pair) -> Result<(), String> {
    let mut problems = Vec::new();
    pair.sweep_a();
    if let Err(err) = scan_and_wait(
        &pair.api_a(),
        ScanKind::Config,
        production_poll("config scan after the gate's cleanup"),
    )
    .await
    {
        problems.push(format!("cleanup scan: {}", err.code()));
    }
    let linked = match pair.api_b_direct().redundancy_status().await {
        Ok(b) => b.peer_connected,
        Err(_) => false,
    };
    let both_active = match (
        pair.api_a().redundancy_status().await,
        pair.api_b_direct().redundancy_status().await,
    ) {
        (Ok(a), Ok(b)) => {
            a.activity_level.as_deref() == Some("Active")
                && b.activity_level.as_deref() == Some("Active")
        }
        _ => false,
    };
    if !linked || both_active {
        let started = Instant::now();
        let pair2 = pair.clone();
        match tokio::task::spawn_blocking(move || pair2.script(&["link"])).await {
            Ok(Ok(line)) => eprintln!(
                "cleanup: link restored in {} s ({line})",
                started.elapsed().as_secs()
            ),
            Ok(Err(why)) => problems.push(why),
            Err(err) => problems.push(format!("link task: {err}")),
        }
    }
    match pair.wait_settled(Duration::from_secs(300)).await {
        Ok(s) => eprintln!("cleanup: pair settled after {s} s"),
        Err(why) => problems.push(why),
    }
    let started = Instant::now();
    while !pair.units_on_b().is_empty() {
        if started.elapsed() > Duration::from_secs(180) {
            problems.push(format!(
                "gw-b still holds {:?} after 180 s",
                pair.units_on_b()
            ));
            break;
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("; "))
    }
}

/// Run `body` in its own task so that a panic still reaches the cleanup, then
/// re-raise it. The pair must be settled and clean when the gate starts.
async fn guarded<F, Fut>(pair: &Pair, body: F)
where
    F: FnOnce(Pair) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    pair.sweep_a();
    if let Err(why) = pair.wait_settled(Duration::from_secs(300)).await {
        panic!("the gate needs a settled pair to start: {why}");
    }
    let joined = tokio::spawn(body(pair.clone())).await;
    let cleaned = cleanup(pair).await;
    if let Err(err) = joined {
        if err.is_panic() {
            if let Err(why) = &cleaned {
                eprintln!("cleanup after the failed gate also failed: {why}");
            }
            std::panic::resume_unwind(err.into_panic());
        }
        panic!("the gate task did not finish: {err}");
    }
    if let Err(why) = cleaned {
        panic!("the gate passed but the cleanup left the rig unsettled: {why}");
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create dir");
    for entry in std::fs::read_dir(from).expect("read dir") {
        let entry = entry.expect("entry");
        let dest = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_dir(&entry.path(), &dest);
        } else {
            std::fs::copy(entry.path(), dest).expect("copy file");
        }
    }
}

/// A git repo whose `.gitignore` holds `.ign/`, whose target has the pair as
/// two `local:` nodes (profiles `pair-a` and `pair-b`) and whose gateway
/// directory is the tracer's two units, plus, with `secret`, one OAuth client
/// whose client secret is a `Referenced` secret `(provider, name)`.
fn repo_for_pair(pair: &Pair, secret: Option<(&str, &str)>) -> tempfile::TempDir {
    let repo = tempfile::tempdir().expect("temp repo");
    std::fs::create_dir_all(repo.path().join("targets")).expect("targets dir");
    let gateway = repo.path().join("gateways").join(GATEWAY_DIR);
    copy_dir(Path::new(TRACER_GATEWAY), &gateway);
    if let Some((provider, name)) = secret {
        let manifest = gateway.join("manifest.yaml");
        let mut text = std::fs::read_to_string(&manifest).expect("manifest");
        text.push_str("  - ignition/oauth2-client\n");
        std::fs::write(&manifest, text).expect("manifest write");
        let unit = gateway.join("resources/ignition/oauth2-client/CAP24_GATE");
        std::fs::create_dir_all(&unit).expect("unit dir");
        let config = serde_json::json!({
            "clientId": "cap24-client",
            "clientSecret": {
                "type": "Referenced",
                "data": {"providerName": provider, "secretName": name}
            },
            "tokenUrl": "http://localhost/token",
            "httpVersion": "HTTP_1_1",
            "httpConnectTimeout": 10000,
            "httpRequestTimeout": 10000
        });
        std::fs::write(
            unit.join("config.json"),
            serde_json::to_vec_pretty(&config).expect("config json"),
        )
        .expect("config.json");
        // Only `uuid` and `enabled` in `attributes`. A `lastModification`
        // block without a valid signature is re-saved by gw-b alone; the
        // `files` check ignores that stamp since the K2 revision, and
        // `live_pair_converges_with_a_stale_signature` covers it.
        std::fs::write(
            unit.join("resource.json"),
            r#"{"scope":"A","version":1,"restricted":false,"overridable":true,"files":["config.json"],"attributes":{"uuid":"c3a24a24-7c1e-4b0e-9a52-0c0ffee24a24","enabled":true}}"#,
        )
        .expect("resource.json");
    }
    let yaml = format!(
        "schema_version: 1\ngateway: {GATEWAY_DIR}\nmode: {mode}\nnodes:\n  master:\n    profile: pair-a\n    local:\n      data_dir: {a}\n  backup:\n    profile: pair-b\n    local:\n      data_dir: {b}\n",
        mode = pair.mode,
        a = pair.data_a.display(),
        b = pair.data_b.display(),
    );
    std::fs::write(repo.path().join("targets").join("pair.yaml"), yaml).expect("target file");
    std::fs::write(repo.path().join(".gitignore"), ".ign/\n").expect(".gitignore");
    let init = std::process::Command::new("git")
        .args(["init", "-q"])
        .arg(repo.path())
        .status()
        .expect("git init");
    assert!(init.success(), "git init failed");
    repo
}

/// Everything a fresh runner builds from the repo alone: the render and a
/// file reader for each node.
struct Context {
    declared: Declared,
    master_reader: Box<dyn TreeReader>,
    backup_reader: Box<dyn TreeReader>,
    repo: PathBuf,
}

impl Context {
    fn new(repo: &Path) -> Context {
        let declared = load_declared(repo, ENV).expect("render of the repo");
        let (target_file, target) = load_target(repo, ENV).expect("target");
        let master_reader =
            transport::for_node(repo, &target_file, "nodes.master", &target.nodes.master)
                .expect("the master has host access");
        let backup = target.nodes.backup.as_ref().expect("a backup node");
        let backup_reader = transport::for_node(repo, &target_file, "nodes.backup", backup)
            .expect("the backup has host access");
        Context {
            declared,
            master_reader,
            backup_reader,
            repo: repo.to_path_buf(),
        }
    }

    async fn plan_hash(&self) -> String {
        self.plan().await.plan_sha256
    }

    async fn plan(&self) -> ignition_core::actions::deploy::DeployPlanResult {
        deploy_plan(
            &self.declared,
            self.master_reader.as_ref(),
            &PlanOptions {
                base: None,
                diff: false,
            },
        )
        .await
        .expect("a plan")
    }
}

fn gates_for(hash: &str) -> GateFlags {
    GateFlags {
        plan_sha256: Some(hash.to_string()),
        ..GateFlags::default()
    }
}

/// Collects the engine's events by name.
#[derive(Default)]
struct Events {
    names: Mutex<Vec<String>>,
}

impl Events {
    fn names(&self) -> Vec<String> {
        self.names.lock().expect("events").clone()
    }

    fn count(&self, name: &str) -> usize {
        self.names().iter().filter(|n| *n == name).count()
    }
}

impl ApplyObserver for Events {
    fn event(&self, e: &ApplyEvent) {
        self.names
            .lock()
            .expect("events")
            .push(e.name().to_string());
    }
}

/// Collects the engine's events and cuts the gateway-network link the moment
/// the scan-lock is taken (K3 b: after the preflights read both nodes, before
/// the first write). The cut is `setup-pair.sh cut` (K4 a: `toggleApproval
/// false` on gw-a); the call blocks the engine for its duration.
struct CutAtLock {
    pair: Pair,
    events: Events,
    cut: Mutex<Option<(Duration, Result<String, String>)>>,
}

impl ApplyObserver for CutAtLock {
    fn event(&self, e: &ApplyEvent) {
        self.events.event(e);
        if matches!(e, ApplyEvent::LockAcquired { .. }) {
            let started = Instant::now();
            let outcome = self.pair.script(&["cut"]);
            *self.cut.lock().expect("cut record") = Some((started.elapsed(), outcome));
        }
    }
}

/// The state of run `run_id` in the on-box record below gw-a's data directory.
fn record_state(pair: &Pair, run_id: &str) -> String {
    fn find(dir: &Path, run_id: &str) -> Option<PathBuf> {
        for entry in std::fs::read_dir(dir).ok()?.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                if let Some(found) = find(&path, run_id) {
                    return Some(found);
                }
            } else if path.file_name().is_some_and(|n| n == "record.json")
                && path
                    .parent()
                    .and_then(Path::file_name)
                    .is_some_and(|n| n == run_id)
            {
                return Some(path);
            }
        }
        None
    }
    let path = find(&pair.data_a.join("ign-deploy"), run_id)
        .unwrap_or_else(|| panic!("no on-box record for run {run_id} below gw-a's ign-deploy/"));
    let value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).expect("record.json")).expect("record json");
    value["state"].as_str().expect("record state").to_string()
}

fn run_options(plan_hash: &str, backup_timeout: Duration) -> ApplyRunOptions {
    ApplyRunOptions {
        base: None,
        gates: gates_for(plan_hash),
        project: "ign-cli".to_string(),
        designers: DesignersPolicy::default(),
        skip_gwbk: false,
        backup_timeout,
    }
}

/// Prepare and run the apply of `ctx` over the pair, the backup reached
/// through `tap`.
async fn apply_over_pair(
    pair: &Pair,
    ctx: &Context,
    tap: &MethodTap,
    backup_timeout: Duration,
    observer: &dyn ApplyObserver,
) -> DeployApplyResult {
    let hash = ctx.plan_hash().await;
    let prepared = deploy_apply_prepare(
        &ctx.declared,
        ctx.master_reader.as_ref(),
        &PrepareOptions {
            base: None,
            gates: gates_for(&hash),
        },
    )
    .await
    .expect("prepare over the pair");
    let ApplyPrepared::Ready { plan, .. } = &prepared else {
        panic!("a clean pair has the tracer to apply: {prepared:?}");
    };
    eprintln!(
        "plan: added {} changed {} removed {} plan_sha256 {}",
        plan.summary.added, plan.summary.changed, plan.summary.removed, plan.plan_sha256
    );
    let master = pair.api_a();
    let backup_api = pair.api_b_at(&tap.url());
    let backup = BackupNode::new(&backup_api, &backup_api, Some(ctx.backup_reader.as_ref()));
    let surfaces = ApplySurfaces {
        deploy: &master,
        backup: Some(&backup),
        gateway: &master,
    };
    deploy_apply_run(
        prepared,
        &ctx.declared,
        &ctx.repo,
        ctx.master_reader.as_ref(),
        &surfaces,
        &run_options(&hash, backup_timeout),
        observer,
    )
    .await
    .expect("the apply ran")
}

/// `deploy verify --node <node>` over the pair, the backup through `tap`.
async fn verify_over_pair(
    pair: &Pair,
    ctx: &Context,
    tap: &MethodTap,
    node: VerifyNode,
) -> DeployVerifyResult {
    let master = pair.api_a();
    let backup_api = pair.api_b_at(&tap.url());
    let backup = BackupNode::new(&backup_api, &backup_api, Some(ctx.backup_reader.as_ref()));
    deploy_verify_node(
        &ctx.declared,
        "ign-cli",
        node,
        Some((&master, &master)),
        ctx.master_reader.as_ref(),
        Some(&backup),
        &VerifyOptions::default(),
    )
    .await
    .expect("the verify ran")
}

fn check_status(result: &DeployVerifyResult, id: BackupCheckId) -> CheckStatus {
    result
        .backup
        .as_ref()
        .expect("a backup report")
        .checks
        .iter()
        .find(|c| c.id == id)
        .unwrap_or_else(|| panic!("no {id:?} check in the backup report"))
        .status
}

// ---- live_pair_converges (the tracer, SC 1, DPAIR-01) ---------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live: needs the ign-pair-rig pair and IGNITION_LIVE_PAIR=1"]
async fn live_pair_converges() {
    let Some(pair) = pair().await else { return };
    let _serial = PAIR_LOCK.lock().await;
    let started = Instant::now();
    eprintln!("live_pair_converges @ {}", utc_now());
    guarded(&pair, |pair| async move {
        let repo = repo_for_pair(&pair, None);
        let ctx = Context::new(repo.path());
        let tap = MethodTap::start(&pair.url_b);
        let events = Events::default();

        let applied = Instant::now();
        let result = apply_over_pair(&pair, &ctx, &tap, Duration::from_secs(120), &events).await;
        let apply_s = applied.elapsed().as_secs();
        let backup = result.backup.as_deref().expect("a backup report");
        eprintln!(
            "apply: outcome {:?} ok {} run {:?} in {apply_s} s; backup converged {} reason {:?} waited_s {} budget_s {} files_checked {} resync {:?}",
            result.outcome,
            result.ok,
            result.run_id,
            backup.converged,
            backup.reason,
            backup.waited_s,
            backup.budget_s,
            backup.files_checked,
            backup.resync,
        );
        eprintln!("apply events: {}", events.names().join(" "));
        for check in &backup.checks {
            eprintln!("backup check {:?}: {:?}", check.id, check.status);
        }
        assert_eq!(result.outcome, OutcomeKind::Applied, "{result:?}");
        assert!(result.ok);
        assert_eq!(result.slug, None);
        assert!(backup.converged, "{backup:?}");
        assert_eq!(backup.reason, None);
        assert!(backup.files_checked > 0, "{backup:?}");
        let files = backup
            .checks
            .iter()
            .find(|c| c.id == BackupCheckId::Files)
            .expect("a files check");
        assert_eq!(files.status, CheckStatus::Pass, "{files:?}");
        for wanted in ["backup_sampled", "backup_wait_started", "backup_verdict"] {
            assert!(events.count(wanted) >= 1, "{wanted} in {:?}", events.names());
        }
        assert_eq!(events.count("restore_started"), 0);

        // The backup was only ever read.
        eprintln!("tap after apply: {} requests ({})", tap.total(), tap.summary());
        tap.assert_reads_only("after the apply");

        let verified = verify_over_pair(&pair, &ctx, &tap, VerifyNode::Both).await;
        let report = verified.backup.as_ref().expect("a backup report");
        eprintln!(
            "verify --node both: ok {} slug {:?} backup converged {} files_checked {} secrets nodes {}",
            verified.ok,
            verified.slug,
            report.converged,
            report.files_checked,
            verified.secrets.as_ref().map_or(0, |s| s.nodes.len()),
        );
        assert!(verified.ok, "{verified:?}");
        assert!(report.converged);
        tap.assert_reads_only("after the verify");
        eprintln!(
            "tap total: {} requests ({}), run id {}",
            tap.total(),
            tap.summary(),
            result.run_id.as_deref().unwrap_or("-")
        );
    })
    .await;
    eprintln!(
        "PASS live_pair_converges in {:.1}s @ {}",
        started.elapsed().as_secs_f64(),
        utc_now()
    );
}

// ---- live_pair_converges_with_a_stale_signature (24-10 finding, K2 revised) ------------

/// The stale signature the gate writes: well-formed hex, not the gateway's for
/// that content.
const STALE_SIGNATURE: &str = "00000000000000000000000000000000000000000000000000000000deadbeef";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live: needs the ign-pair-rig pair and IGNITION_LIVE_PAIR=1"]
async fn live_pair_converges_with_a_stale_signature() {
    let Some(pair) = pair().await else { return };
    let _serial = PAIR_LOCK.lock().await;
    let started = Instant::now();
    eprintln!("live_pair_converges_with_a_stale_signature @ {}", utc_now());
    guarded(&pair, |pair| async move {
        let repo = repo_for_pair(&pair, None);
        // A repo whose resource.json was edited by hand: the stamp is there,
        // the signature is not the gateway's for the content. gw-b alone
        // re-saves such a unit (actor `external`, a new signature).
        let unit = repo
            .path()
            .join("gateways")
            .join(GATEWAY_DIR)
            .join("resources/ignition/database-translator/POSTGRES/resource.json");
        let mut doc: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&unit).expect("fixture resource.json"))
                .expect("fixture is JSON");
        doc["attributes"]["lastModification"] =
            serde_json::json!({"actor": "hand-edit", "timestamp": "2026-10-01T08:00:00Z"});
        doc["attributes"]["lastModificationSignature"] = serde_json::json!(STALE_SIGNATURE);
        std::fs::write(&unit, serde_json::to_vec_pretty(&doc).expect("json")).expect("write");

        let ctx = Context::new(repo.path());
        let tap = MethodTap::start(&pair.url_b);
        let events = Events::default();

        let applied = Instant::now();
        let result = apply_over_pair(&pair, &ctx, &tap, Duration::from_secs(120), &events).await;
        let apply_s = applied.elapsed().as_secs();
        let backup = result.backup.as_deref().expect("a backup report");
        eprintln!(
            "apply: outcome {:?} ok {} in {apply_s} s; backup converged {} reason {:?} waited_s {} files_checked {} resync {:?}",
            result.outcome,
            result.ok,
            backup.converged,
            backup.reason,
            backup.waited_s,
            backup.files_checked,
            backup.resync,
        );
        for check in &backup.checks {
            eprintln!("backup check {:?}: {:?}", check.id, check.status);
        }

        // The scenario is real: gw-a kept the text as written (the stale
        // signature) while gw-b re-saved the unit with a signature of its own.
        let rel = "ignition/database-translator/POSTGRES/resource.json";
        let on_a = std::fs::read_to_string(Pair::root(&pair.data_a, &pair.mode).join(rel))
            .expect("resource.json on gw-a");
        let on_b = std::fs::read_to_string(Pair::root(&pair.data_b, &pair.mode).join(rel))
            .expect("resource.json on gw-b");
        let sig = |text: &str| {
            serde_json::from_str::<serde_json::Value>(text)
                .expect("JSON")["attributes"]["lastModificationSignature"]
                .as_str()
                .map(str::to_string)
        };
        eprintln!(
            "signature kept on gw-a: {}; gw-b re-saved it: {}; actor on gw-b: {:?}",
            sig(&on_a).as_deref() == Some(STALE_SIGNATURE),
            sig(&on_b).as_deref() != Some(STALE_SIGNATURE),
            serde_json::from_str::<serde_json::Value>(&on_b)
                .expect("JSON")["attributes"]["lastModification"]["actor"],
        );
        assert_eq!(sig(&on_a).as_deref(), Some(STALE_SIGNATURE), "gw-a: {on_a}");
        assert_ne!(sig(&on_b).as_deref(), Some(STALE_SIGNATURE), "gw-b: {on_b}");

        assert_eq!(result.outcome, OutcomeKind::Applied, "{result:?}");
        assert!(result.ok);
        assert_eq!(result.slug, None);
        assert!(backup.converged, "{backup:?}");
        assert_eq!(backup.reason, None);
        let files = backup
            .checks
            .iter()
            .find(|c| c.id == BackupCheckId::Files)
            .expect("a files check");
        assert_eq!(files.status, CheckStatus::Pass, "{files:?}");
        tap.assert_reads_only("after the apply");

        let verified = verify_over_pair(&pair, &ctx, &tap, VerifyNode::Both).await;
        assert!(verified.ok, "{verified:?}");
        tap.assert_reads_only("after the verify");
        eprintln!("tap total: {} requests ({})", tap.total(), tap.summary());
    })
    .await;
    eprintln!(
        "PASS live_pair_converges_with_a_stale_signature in {:.1}s @ {}",
        started.elapsed().as_secs_f64(),
        utc_now()
    );
}

// ---- live_pair_unsynced_then_reconverge (SC 2, DPAIR-02, D-17, K3 b) -------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live: needs the ign-pair-rig pair and IGNITION_LIVE_PAIR=1"]
async fn live_pair_unsynced_then_reconverge() {
    let Some(pair) = pair().await else { return };
    let _serial = PAIR_LOCK.lock().await;
    let started = Instant::now();
    eprintln!(
        "live_pair_unsynced_then_reconverge @ {} (K3 b, K4 a)",
        utc_now()
    );
    guarded(&pair, |pair| async move {
        let repo = repo_for_pair(&pair, None);
        let ctx = Context::new(repo.path());
        let tap = MethodTap::start(&pair.url_b);
        let observer = CutAtLock {
            pair: pair.clone(),
            events: Events::default(),
            cut: Mutex::new(None),
        };

        let applied = Instant::now();
        let result = apply_over_pair(&pair, &ctx, &tap, Duration::from_secs(20), &observer).await;
        let apply_s = applied.elapsed().as_secs();
        let (cut_took, cut_outcome) = observer
            .cut
            .lock()
            .expect("cut record")
            .take()
            .expect("the observer saw lock_acquired and cut the link");
        eprintln!(
            "cut at lock_acquired: took {:.1} s, {}",
            cut_took.as_secs_f64(),
            match &cut_outcome {
                Ok(_) => "script ok".to_string(),
                Err(why) => format!("script failed: {why}"),
            }
        );
        cut_outcome.expect("the cut script succeeded");
        let backup = result.backup.as_deref().expect("a backup report");
        eprintln!(
            "apply: outcome {:?} ok {} slug {:?} run {:?} in {apply_s} s; backup converged {} reason {:?} waited_s {} budget_s {} resync {:?}",
            result.outcome,
            result.ok,
            result.slug,
            result.run_id,
            backup.converged,
            backup.reason,
            backup.waited_s,
            backup.budget_s,
            backup.resync,
        );
        eprintln!("apply events: {}", observer.events.names().join(" "));

        assert_eq!(result.outcome, OutcomeKind::AppliedBackupUnsynced, "{result:?}");
        assert!(!result.ok);
        assert_eq!(result.slug, Some("deploy_backup_unsynced"));
        assert_eq!(result.failure_exit_code(), Some(6));
        assert!(!backup.converged);
        assert_eq!(backup.reason, Some(UnsyncedReason::NotSyncing), "{backup:?}");
        assert!(backup.resync.is_some(), "one resync is recorded: {backup:?}");
        assert_eq!(observer.events.count("backup_resync"), 1, "one resync request");
        assert_eq!(observer.events.count("restore_started"), 0, "no restore");
        assert_eq!(observer.events.count("restored"), 0, "no restore");

        // The master keeps its applied tree: its record says applied and its
        // files equal the render.
        let run_id = result.run_id.clone().expect("a run id");
        let state = record_state(&pair, &run_id);
        eprintln!("master record state: {state}");
        assert_eq!(state, "applied");
        let after = ctx.plan().await;
        assert!(
            !after.summary.has_changes,
            "gw-a's files equal the render: {:?}",
            after.summary
        );
        tap.assert_reads_only("after the unsynced apply");

        // Restore the link, wait for the pair to settle, then the backup
        // must converge on its own.
        let relinked = Instant::now();
        let pair2 = pair.clone();
        let line = tokio::task::spawn_blocking(move || pair2.script(&["link"]))
            .await
            .expect("link task")
            .expect("the link script");
        let link_s = relinked.elapsed().as_secs();
        eprintln!("link restored after {link_s} s ({line})");
        let settled_s = pair
            .wait_settled(Duration::from_secs(300))
            .await
            .expect("the pair settles after the link is restored");
        eprintln!("pair settled {settled_s} s after the link command returned");

        let waited = Instant::now();
        let converged_s = loop {
            let verified = verify_over_pair(&pair, &ctx, &tap, VerifyNode::Backup).await;
            let report = verified.backup.as_ref().expect("a backup report");
            eprintln!(
                "reconverge poll +{} s: backup converged {} reason {:?} redundancy {:?} files {:?}",
                waited.elapsed().as_secs(),
                report.converged,
                report.reason,
                check_status(&verified, BackupCheckId::Redundancy),
                check_status(&verified, BackupCheckId::Files),
            );
            if report.converged {
                break waited.elapsed().as_secs();
            }
            assert!(
                waited.elapsed() < Duration::from_secs(180),
                "the backup did not re-converge within 180 s: {report:?}"
            );
            tokio::time::sleep(Duration::from_secs(10)).await;
        };
        eprintln!("backup re-converged {converged_s} s after the settle wait began");
        tap.assert_reads_only("after the re-converge");
        eprintln!("tap total: {} requests ({})", tap.total(), tap.summary());
    })
    .await;
    eprintln!(
        "PASS live_pair_unsynced_then_reconverge in {:.1}s @ {}",
        started.elapsed().as_secs_f64(),
        utc_now()
    );
}

// ---- live_pair_secret_names (SC 3, DPAIR-03) -------------------------------------------

/// The canary value `setup-pair.sh secrets` writes into the container; it must
/// never appear in a result.
const CANARY_VALUE: &str = "CAP24-CANARY-VALUE";

/// Poll both nodes until `provider` lists `name` (or no longer lists it) on
/// each; returns the seconds each node took.
async fn wait_name(
    pair: &Pair,
    provider: &str,
    name: &str,
    present: bool,
    max: Duration,
) -> Result<(u64, u64), String> {
    let started = Instant::now();
    let (mut a_s, mut b_s) = (None, None);
    loop {
        if a_s.is_none() {
            let names = pair.names_on(&pair.api_a(), provider).await;
            if names.is_some_and(|n| n.iter().any(|x| x == name) == present) {
                a_s = Some(started.elapsed().as_secs());
            }
        }
        if b_s.is_none() {
            let names = pair.names_on(&pair.api_b_direct(), provider).await;
            if names.is_some_and(|n| n.iter().any(|x| x == name) == present) {
                b_s = Some(started.elapsed().as_secs());
            }
        }
        if let (Some(a), Some(b)) = (a_s, b_s) {
            return Ok((a, b));
        }
        if started.elapsed() >= max {
            return Err(format!(
                "after {} s: gw-a {:?}, gw-b {:?} (name present = {present})",
                max.as_secs(),
                a_s,
                b_s
            ));
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live: needs the ign-pair-rig pair and IGNITION_LIVE_PAIR=1"]
async fn live_pair_secret_names() {
    let Some(pair) = pair().await else { return };
    let _serial = PAIR_LOCK.lock().await;
    let started = Instant::now();
    eprintln!("live_pair_secret_names @ {}", utc_now());
    guarded(&pair, |pair| async move {
        // A name of this run's own, so a leftover of an earlier run cannot
        // make the gate pass. Provider names hold no `-`.
        let provider = "CAP24_GATE_SP";
        let epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_secs();
        let name = format!("CAP24-Gate-{epoch}");
        let script = |args: Vec<String>| {
            let pair = pair.clone();
            async move {
                tokio::task::spawn_blocking(move || {
                    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                    pair.script(&refs)
                })
                .await
                .expect("script task")
            }
        };

        // C-P7: a name made on gw-a reaches gw-b, so the red state is made by
        // removing it again. First the name on both nodes, the tree that
        // references it applied (the secrets preflight refuses a name a node
        // lacks), the verify green.
        let made = script(vec![
            "secrets".into(),
            "a".into(),
            provider.into(),
            name.clone(),
        ])
        .await
        .expect("make the secret name on gw-a");
        eprintln!("setup: {made}");
        let (a_s, b_s) = wait_name(&pair, provider, &name, true, Duration::from_secs(180))
            .await
            .expect("the name reaches both nodes");
        eprintln!("name listed on gw-a after {a_s} s, on gw-b after {b_s} s");

        let repo = repo_for_pair(&pair, Some((provider, &name)));
        let ctx = Context::new(repo.path());
        let tap = MethodTap::start(&pair.url_b);
        let result =
            apply_over_pair(&pair, &ctx, &tap, Duration::from_secs(120), &NoObserver).await;
        eprintln!(
            "apply: outcome {:?} ok {} backup converged {:?} warnings {}",
            result.outcome,
            result.ok,
            result.backup.as_ref().map(|b| b.converged),
            result.warnings.len(),
        );
        assert_eq!(result.outcome, OutcomeKind::Applied, "{result:?}");
        tap.assert_reads_only("after the apply with a secret reference");

        let green = verify_over_pair(&pair, &ctx, &tap, VerifyNode::Both).await;
        let secrets = green.secrets.as_ref().expect("a secrets report");
        eprintln!(
            "verify (name on both): ok {} secrets {:?}",
            green.ok,
            secrets
                .nodes
                .iter()
                .map(|n| (n.node.as_str(), n.status, n.missing.len()))
                .collect::<Vec<_>>()
        );
        assert!(green.ok, "{green:?}");
        assert!(secrets.nodes.iter().all(|n| n.status == CheckStatus::Pass));
        assert_eq!(secrets.nodes.len(), 2);

        // Red: the name is gone (made absent on gw-a, which replicates).
        let removed = script(vec![
            "secrets-rm".into(),
            "a".into(),
            provider.into(),
            name.clone(),
        ])
        .await
        .expect("remove the secret name on gw-a");
        eprintln!("setup: {removed}");
        let (a_s, b_s) = wait_name(&pair, provider, &name, false, Duration::from_secs(180))
            .await
            .expect("the removal reaches both nodes");
        eprintln!("name gone on gw-a after {a_s} s, on gw-b after {b_s} s");
        let red = verify_over_pair(&pair, &ctx, &tap, VerifyNode::Both).await;
        let report = red.secrets.as_ref().expect("a secrets report");
        eprintln!(
            "verify (name missing): ok {} slug {:?} secrets {:?}",
            red.ok,
            red.slug,
            report
                .nodes
                .iter()
                .map(|n| (
                    n.node.as_str(),
                    n.status,
                    n.missing
                        .iter()
                        .map(|m| format!("{}/{}", m.provider, m.name))
                        .collect::<Vec<_>>()
                ))
                .collect::<Vec<_>>()
        );
        assert!(!red.ok, "{red:?}");
        assert_eq!(red.slug, Some("deploy_verify_failed"));
        assert_eq!(red.failure_exit_code(), Some(6));
        for node in &report.nodes {
            assert_eq!(node.status, CheckStatus::Fail, "{node:?}");
            assert!(
                node.missing
                    .iter()
                    .any(|m| m.provider == provider && m.name == name),
                "{} names {provider}/{name} as missing: {node:?}",
                node.node
            );
        }
        let shown = format!(
            "{red:?}{}",
            serde_json::to_string(&red).expect("verify json")
        );
        assert!(
            !shown.contains(CANARY_VALUE),
            "no secret value in the verdict"
        );

        // Green again: the name is made on gw-a once more.
        let remade = script(vec![
            "secrets".into(),
            "a".into(),
            provider.into(),
            name.clone(),
        ])
        .await
        .expect("make the secret name again");
        eprintln!("setup: {remade}");
        let (a_s, b_s) = wait_name(&pair, provider, &name, true, Duration::from_secs(180))
            .await
            .expect("the name reaches both nodes again");
        eprintln!("name listed again on gw-a after {a_s} s, on gw-b after {b_s} s");
        let again = verify_over_pair(&pair, &ctx, &tap, VerifyNode::Both).await;
        let secrets = again.secrets.as_ref().expect("a secrets report");
        eprintln!(
            "verify (name on both again): ok {} secrets {:?}",
            again.ok,
            secrets
                .nodes
                .iter()
                .map(|n| (n.node.as_str(), n.status, n.missing.len()))
                .collect::<Vec<_>>()
        );
        assert!(again.ok, "{again:?}");
        assert!(secrets.nodes.iter().all(|n| n.status == CheckStatus::Pass));
        let shown = format!(
            "{again:?}{}",
            serde_json::to_string(&again).expect("verify json")
        );
        assert!(
            !shown.contains(CANARY_VALUE),
            "no secret value in the verdict"
        );
        tap.assert_reads_only("across the secret gate");
        eprintln!("tap total: {} requests ({})", tap.total(), tap.summary());
    })
    .await;
    eprintln!(
        "PASS live_pair_secret_names in {:.1}s @ {}",
        started.elapsed().as_secs_f64(),
        utc_now()
    );
}
