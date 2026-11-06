//! Phase 24's rig gates for the redundant pair (24-10): an apply whose backup
//! converges, an apply whose backup is cut off and ends
//! `applied_backup_unsynced` without undoing the master, and a verify that goes
//! red on a secret name a node does not hold. Debug session
//! deploy-tag-redundancy-sync adds two: an apply of a tag unit to the pair is
//! refused `deploy_pair_tags_refused` with nothing written, and a pair wedged
//! by a tag unit in the mode collection (the pre-fix production shape) is
//! recovered by an apply that deletes it and its emptied type folder, after
//! which a non-tag change reaches the backup without a resync. Quick
//! 261102-cef (A2) routes a pair's tag units into `core`: a tag-definition
//! and a tag-type-definition unit converge, then move away again, with no
//! snapshot mismatch on gw-b; a pair wedged by units in its mode collection
//! recovers by one apply that moves them to `core`; and the refusal is left
//! for the residue an apply cannot delete. Each gate prints one stderr line
//! `LIVE-RAN <gate>` once it is past its skip guard.
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
use ignition_core::deploy::ledger::{Ledger, LedgerRoot, ledger_location};
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

/// The two tag type folders a pair cannot sync from a mode collection
/// (debug deploy-tag-redundancy-sync); a gate never leaves one behind.
const TAG_TYPE_DIRS: [&str; 2] = ["tag-definition", "tag-type-definition"];

/// The unit folders an A2 gate writes into gw-a's `core`, below
/// `core/ignition/` (quick 261102-cef): removed by the cleanup one by one,
/// never a whole core type folder, which holds the rig's own providers.
const CORE_GATE_UNITS: [&str; 2] = [
    "tag-definition/default/Deploy",
    "tag-type-definition/default/DeployTypes",
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
        "http://192.0.2.15:8088",
        "https://gwnode01.example.local:8043",
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

    /// The A2 gate units present in `data`'s `core` (quick 261102-cef).
    fn core_units_on(data: &Path) -> Vec<String> {
        let root = Self::root(data, "core").join("ignition");
        CORE_GATE_UNITS
            .iter()
            .filter(|unit| root.join(unit).exists())
            .map(|unit| (*unit).to_string())
            .collect()
    }

    /// Remove what a gate wrote into gw-a's mode collection and the
    /// `ign-deploy/` directory. True when a tag type folder a pair cannot
    /// sync (`ignition/tag-definition`, `ignition/tag-type-definition`) was
    /// there: the pair then needs a resync to recover (T6e).
    fn sweep_a(&self) -> bool {
        let root = Self::root(&self.data_a, &self.mode).join("ignition");
        for unit in GATE_UNITS {
            let _ = std::fs::remove_dir_all(root.join(unit));
        }
        let mut tags = false;
        for ty in TAG_TYPE_DIRS {
            if root.join(ty).exists() {
                tags = true;
                let _ = std::fs::remove_dir_all(root.join(ty));
            }
        }
        // exactly the A2 gates' unit folders in core, which syncs them away
        let core = Self::root(&self.data_a, "core").join("ignition");
        for unit in CORE_GATE_UNITS {
            let _ = std::fs::remove_dir_all(core.join(unit));
        }
        let _ = std::fs::remove_dir_all(self.data_a.join("ign-deploy"));
        tags
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
    let tags = pair.sweep_a();
    if let Err(err) = scan_and_wait(
        &pair.api_a(),
        ScanKind::Config,
        production_poll("config scan after the gate's cleanup"),
    )
    .await
    {
        problems.push(format!("cleanup scan: {}", err.code()));
    }
    // A tag path left in gw-a's mode collection wedged gw-b's config-resource
    // sync, which no later change retries: the resync to the master starts it
    // again now that the path is gone (T6e).
    if tags {
        match DeployApi::redundancy_resync(&pair.api_a()).await {
            Ok(()) => eprintln!("cleanup: tag paths removed from gw-a, resync sent"),
            Err(err) => problems.push(format!("cleanup resync: {}", err.code())),
        }
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
    while !pair.units_on_b().is_empty() || !Pair::core_units_on(&pair.data_b).is_empty() {
        if started.elapsed() > Duration::from_secs(180) {
            problems.push(format!(
                "gw-b still holds {:?} and in core {:?} after 180 s",
                pair.units_on_b(),
                Pair::core_units_on(&pair.data_b)
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
    eprintln!("LIVE-RAN live_pair_converges");
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
    eprintln!("LIVE-RAN live_pair_converges_with_a_stale_signature");
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
    eprintln!("LIVE-RAN live_pair_unsynced_then_reconverge");
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
    eprintln!("LIVE-RAN live_pair_secret_names");
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

// ---- debug deploy-tag-redundancy-sync: tag units and the pair -------------------------

/// The production canary unit (the one that wedged the 8.3.6 pair), as `ign`
/// v1.6.2 wrote it into the mode collection.
const CANARY_UNIT: &str = "ignition/tag-definition/default/Deploy";
const CANARY_TAGS: &str = "ignition/tag-definition/default/Deploy/tags.json";
const CANARY_MARKER: &str = "ignition/tag-definition/default/Deploy/unary-resource.json";
const CANARY_TAGS_BYTES: &str = r#"[{"valueSource":"memory","dataType":"String","name":"Canary","defaultValue":"ign v1.6.2","tagType":"AtomicTag"}]"#;
const CANARY_MARKER_BYTES: &str = r#"{"scope":"G","version":1,"restricted":false,"overridable":true,"files":["tags.json"],"attributes":{"config":{}}}"#;

/// [`repo_for_pair`] with tag provider `default` allowed and, with
/// `declare`, the canary unit declared under `tags/default/tag-definition/`.
fn repo_with_tags(pair: &Pair, declare: bool) -> tempfile::TempDir {
    let repo = repo_for_pair(pair, None);
    let gateway = repo.path().join("gateways").join(GATEWAY_DIR);
    let manifest = gateway.join("manifest.yaml");
    let mut text = std::fs::read_to_string(&manifest).expect("manifest");
    text.push_str("tag_providers:\n  - default\n");
    std::fs::write(&manifest, text).expect("manifest write");
    if declare {
        let dir = gateway.join("tags/default/tag-definition/Deploy");
        std::fs::create_dir_all(&dir).expect("tag dir");
        std::fs::write(dir.join("tags.json"), CANARY_TAGS_BYTES).expect("tags.json");
        std::fs::write(dir.join("unary-resource.json"), CANARY_MARKER_BYTES).expect("marker");
    }
    repo
}

/// Every file below `dir` with its bytes (relative paths), for a
/// nothing-was-written comparison.
fn files_below(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in read.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                walk(base, &path, out);
            } else if let Ok(bytes) = std::fs::read(&path) {
                let rel = path.strip_prefix(base).expect("below base");
                out.insert(rel.display().to_string(), bytes);
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

/// The `projectState` of each node, `?` when unreadable.
async fn project_states(pair: &Pair) -> (String, String) {
    let a = pair.api_a().redundancy_status().await.ok();
    let b = pair.api_b_direct().redundancy_status().await.ok();
    let word = |state: Option<String>| state.unwrap_or_else(|| "?".to_string());
    (
        word(a.and_then(|s| s.project_state)),
        word(b.and_then(|s| s.project_state)),
    )
}

/// SC (retargeted by quick 261102-cef to the residue): on the real 8.3.6
/// pair a declared tag unit goes to `core`, so what is still refused is a
/// file of it in gw-a's mode collection that no ledger lists (written here
/// without a scan). The apply is refused `deploy_pair_tags_refused` (exit 6)
/// before anything is written: gw-a's mode collection and `core` are
/// byte-identical, no `ign-deploy/` or `.ign/` directory appears, gw-b is
/// never contacted, and the pair stays settled. `deploy plan` on the same
/// target names the unit in a warning. The cleanup removes the file.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live: needs the ign-pair-rig pair and IGNITION_LIVE_PAIR=1"]
async fn live_pair_refuses_a_tag_unit() {
    let Some(pair) = pair().await else { return };
    eprintln!("LIVE-RAN live_pair_refuses_a_tag_unit");
    let _serial = PAIR_LOCK.lock().await;
    let started = Instant::now();
    eprintln!("live_pair_refuses_a_tag_unit @ {}", utc_now());
    guarded(&pair, |pair| async move {
        let repo = repo_with_tags(&pair, true);
        let ctx = Context::new(repo.path());
        let tap = MethodTap::start(&pair.url_b);
        let mode_dir = Pair::root(&pair.data_a, &pair.mode);
        let core_dir = Pair::root(&pair.data_a, "core");
        // the residue: an unledgered file of the canary, never scanned
        let stray = mode_dir.join(CANARY_TAGS);
        std::fs::create_dir_all(stray.parent().expect("parent")).expect("stray dir");
        std::fs::write(&stray, CANARY_TAGS_BYTES).expect("stray file");
        let before = files_below(&mode_dir);
        let core_before = files_below(&core_dir);

        let planned = ctx.plan().await;
        let warned: Vec<&String> = planned
            .warnings
            .iter()
            .filter(|w| w.contains("deploy_pair_tags_refused"))
            .collect();
        eprintln!(
            "plan: added {} plan_sha256 {}; pair-tag warnings {}",
            planned.summary.added,
            planned.plan_sha256,
            warned.len()
        );
        assert_eq!(warned.len(), 1, "{:?}", planned.warnings);
        assert!(warned[0].contains(CANARY_UNIT), "{}", warned[0]);

        let err = deploy_apply_prepare(
            &ctx.declared,
            ctx.master_reader.as_ref(),
            &PrepareOptions {
                base: None,
                gates: gates_for(&planned.plan_sha256),
            },
        )
        .await
        .expect_err("a tag unit's residue on a pair is refused");
        eprintln!(
            "apply: refused {} exit {}: {err}",
            err.code(),
            err.exit_code()
        );
        assert_eq!(err.code(), "deploy_pair_tags_refused");
        assert_eq!(err.exit_code(), 6);
        assert!(err.to_string().contains(CANARY_UNIT), "{err}");
        assert!(err.to_string().contains("nothing was written"), "{err}");

        assert_eq!(
            files_below(&mode_dir),
            before,
            "gw-a's mode collection is unchanged"
        );
        assert_eq!(
            files_below(&core_dir),
            core_before,
            "gw-a's core is unchanged"
        );
        assert!(!pair.data_a.join("ign-deploy").exists(), "no host state");
        assert!(!repo.path().join(".ign").exists(), "no run directory");
        assert_eq!(
            tap.total(),
            0,
            "gw-b was never contacted: {}",
            tap.summary()
        );
        let settled = pair
            .wait_settled(Duration::from_secs(30))
            .await
            .expect("the pair stays settled");
        eprintln!("pair still settled ({settled} s)");
    })
    .await;
    eprintln!(
        "PASS live_pair_refuses_a_tag_unit in {:.1}s @ {}",
        started.elapsed().as_secs_f64(),
        utc_now()
    );
}

/// SC: the recovery through `ign` on the real 8.3.6 pair.
///
/// 1. The pre-fix shape: the canary unit in gw-a's mode collection with a
///    ledger that lists it, as `ign` v1.6.2 left production, then a config
///    scan. The pair wedges: gw-b logs that it gave up syncing, a node
///    reports `projectState` `SyncError` and gw-b never receives the files
///    (the falsification check: a pair that syncs it means Ignition no
///    longer filters mode-collection tags).
/// 2. An apply of a repo that no longer declares the canary: the gates pass
///    (deleting is allowed), the commit deletes both canary files and the
///    emptied folder chain up to and including `ignition/tag-definition`,
///    and the backup converges (an exhausted backup needs the apply's one
///    resync to the master): outcome `applied`, `files` pass.
/// 3. A following non-tag change (the alarm settings) reaches gw-b on its
///    own: outcome `applied`, backup converged with no resync, `files` pass,
///    `verify --node both` green and both nodes `Good`. A resync pushes a
///    diff's files across even while the pair stays wedged (T6d), so the
///    no-resync convergence is what proves the sync runs again.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live: needs the ign-pair-rig pair and IGNITION_LIVE_PAIR=1"]
async fn live_pair_tag_cleanup_converges() {
    let Some(pair) = pair().await else { return };
    eprintln!("LIVE-RAN live_pair_tag_cleanup_converges");
    let _serial = PAIR_LOCK.lock().await;
    let started = Instant::now();
    eprintln!("live_pair_tag_cleanup_converges @ {}", utc_now());
    guarded(&pair, |pair| async move {
        let mode_a = Pair::root(&pair.data_a, &pair.mode);
        let mode_b = Pair::root(&pair.data_b, &pair.mode);

        // 1. The pre-fix shape, then the wedge.
        for (rel, bytes) in [
            (CANARY_TAGS, CANARY_TAGS_BYTES),
            (CANARY_MARKER, CANARY_MARKER_BYTES),
        ] {
            let path = mode_a.join(rel);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("canary dir");
            std::fs::write(path, bytes).expect("canary file");
        }
        let ledger = Ledger {
            schema_version: 1,
            gateway: GATEWAY_DIR.to_string(),
            mode: pair.mode.clone(),
            render_sha256: "0".repeat(64),
            run_id: "pre-fix-canary".to_string(),
            files: vec![CANARY_TAGS.to_string(), CANARY_MARKER.to_string()],
        };
        let ledger_path = ledger_location(LedgerRoot::DataDir, GATEWAY_DIR, &pair.mode)
            .split('/')
            .fold(pair.data_a.clone(), |acc, c| acc.join(c));
        std::fs::create_dir_all(ledger_path.parent().expect("parent")).expect("ledger dir");
        std::fs::write(&ledger_path, ledger.to_bytes()).expect("ledger");
        let since_ms = ignition_core::deploy::pair::wall_clock_ms() - 5_000;
        scan_and_wait(
            &pair.api_a(),
            ScanKind::Config,
            production_poll("config scan of the pre-fix canary"),
        )
        .await
        .expect("the canary scan");
        // The wedge as production has it: gw-b exhausted its config-resource
        // attempts (the TEC-5105 line), so no later master change is retried
        // without a resync (T1b).
        let b_direct = pair.api_b_direct();
        let b_logs = BackupNode::new(&b_direct, &b_direct, None);
        let wedge_started = Instant::now();
        let wedged_s = loop {
            let exhausted = b_logs.sync_exhausted(since_ms).await.unwrap_or(false);
            let (a, b) = project_states(&pair).await;
            if exhausted {
                eprintln!("wedge: gw-b gave up syncing; projectState gw-a {a}, gw-b {b}");
                assert!(
                    a == "SyncError" || b == "SyncError",
                    "a wedged pair reports SyncError (gw-a {a}, gw-b {b})"
                );
                break wedge_started.elapsed().as_secs();
            }
            assert!(
                wedge_started.elapsed() < Duration::from_secs(120),
                "the pre-fix shape did not wedge the pair within 120 s (gw-a {a}, gw-b {b}): \
                 Ignition may now sync mode-collection tags; revisit the refusal"
            );
            tokio::time::sleep(Duration::from_secs(2)).await;
        };
        assert!(
            !mode_b.join(CANARY_TAGS).exists(),
            "gw-b never received the canary"
        );
        eprintln!("pair wedged {wedged_s} s after the canary scan; gw-b has no canary");

        // 2. The recovery apply: the canary is removed, the tracer added.
        let repo = repo_with_tags(&pair, false);
        let ctx = Context::new(repo.path());
        let plan = ctx.plan().await;
        let removed: Vec<&str> = plan
            .entries
            .iter()
            .filter(|e| e.status == ignition_core::deploy::plan::PlanStatus::Removed)
            .map(|e| e.key.as_str())
            .collect();
        eprintln!(
            "recovery plan: added {} removed {:?} delete set {} plan_sha256 {}",
            plan.summary.added,
            removed,
            plan.delete_set.files.len(),
            plan.plan_sha256
        );
        assert_eq!(removed, [CANARY_UNIT], "{:?}", plan.entries);
        let tap = MethodTap::start(&pair.url_b);
        let events = Events::default();
        let prepared = deploy_apply_prepare(
            &ctx.declared,
            ctx.master_reader.as_ref(),
            &PrepareOptions {
                base: None,
                gates: GateFlags {
                    plan_sha256: Some(plan.plan_sha256.clone()),
                    allow_destructive: true,
                    allow_drift: None,
                },
            },
        )
        .await
        .expect("a removed tag unit passes the gates on a pair");
        let master = pair.api_a();
        let backup_api = pair.api_b_at(&tap.url());
        let backup = BackupNode::new(&backup_api, &backup_api, Some(ctx.backup_reader.as_ref()));
        let surfaces = ApplySurfaces {
            deploy: &master,
            backup: Some(&backup),
            gateway: &master,
        };
        let mut opts = run_options(&plan.plan_sha256, Duration::from_secs(45));
        opts.gates.allow_destructive = true;
        let applied = Instant::now();
        let result = deploy_apply_run(
            prepared,
            &ctx.declared,
            &ctx.repo,
            ctx.master_reader.as_ref(),
            &surfaces,
            &opts,
            &events,
        )
        .await
        .expect("the recovery apply ran");
        let report = result.backup.as_deref().expect("a backup report");
        eprintln!(
            "recovery apply: outcome {:?} ok {} run {:?} in {} s; deleted {:?}; backup converged {} reason {:?} waited_s {} resync {:?} files_checked {}",
            result.outcome,
            result.ok,
            result.run_id,
            applied.elapsed().as_secs(),
            result.deleted,
            report.converged,
            report.reason,
            report.waited_s,
            report.resync,
            report.files_checked,
        );
        for check in &report.checks {
            eprintln!("backup check {:?}: {:?} {}", check.id, check.status, check.detail);
        }
        assert_eq!(result.outcome, OutcomeKind::Applied, "{result:?}");
        assert!(result.ok);
        assert_eq!(result.deleted, [CANARY_TAGS, CANARY_MARKER]);
        assert!(
            !mode_a.join("ignition/tag-definition").exists(),
            "the emptied type folder is gone from gw-a"
        );
        assert!(report.converged, "{report:?}");
        let files = report
            .checks
            .iter()
            .find(|c| c.id == BackupCheckId::Files)
            .expect("a files check");
        assert_eq!(files.status, CheckStatus::Pass, "{files:?}");
        tap.assert_reads_only("after the recovery apply");
        // A resync pushes a diff's files across even while the digests still
        // differ (T6d), so `files` alone cannot tell a recovered pair from a
        // wedged one; the pair must also settle (both `Good`, no sync
        // pending), which a wedged pair does not do.
        let settled_s = pair
            .wait_settled(Duration::from_secs(90))
            .await
            .expect("the recovered pair settles");
        eprintln!("recovered pair settled after {settled_s} s");

        // 3. A following non-tag change reaches gw-b.
        let alarm = repo
            .path()
            .join("gateways")
            .join(GATEWAY_DIR)
            .join("resources/ignition/general-alarm-settings/config.json");
        let mut doc: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&alarm).expect("alarm config"))
                .expect("alarm json");
        doc["liveEventLimit"] = serde_json::json!(7);
        std::fs::write(&alarm, serde_json::to_vec_pretty(&doc).expect("json")).expect("write");
        let ctx = Context::new(repo.path());
        let tap = MethodTap::start(&pair.url_b);
        let next = apply_over_pair(&pair, &ctx, &tap, Duration::from_secs(90), &NoObserver).await;
        let report = next.backup.as_deref().expect("a backup report");
        eprintln!(
            "non-tag change: outcome {:?} ok {} written {:?}; backup converged {} reason {:?} waited_s {} resync {:?}",
            next.outcome,
            next.ok,
            next.written,
            report.converged,
            report.reason,
            report.waited_s,
            report.resync,
        );
        assert_eq!(next.outcome, OutcomeKind::Applied, "{next:?}");
        assert!(report.converged, "{report:?}");
        // The pair syncs it on its own: a still-wedged pair converges only
        // through the apply's resync, after the first budget (the revert
        // check of this gate: waited 95 s, resync sent, both SyncError).
        assert_eq!(report.resync, None, "no resync was needed: {report:?}");
        assert_eq!(
            check_status_of(report, BackupCheckId::Files),
            CheckStatus::Pass
        );
        let on_b = std::fs::read(mode_b.join("ignition/general-alarm-settings/config.json"))
            .expect("gw-b holds the alarm settings");
        let on_b: serde_json::Value = serde_json::from_slice(&on_b).expect("json");
        assert_eq!(on_b["liveEventLimit"], 7, "gw-b holds the change");
        let verified = verify_over_pair(&pair, &ctx, &tap, VerifyNode::Both).await;
        eprintln!("verify --node both: ok {} slug {:?}", verified.ok, verified.slug);
        assert!(verified.ok, "{verified:?}");
        tap.assert_reads_only("after the non-tag change");
        let (a, b) = project_states(&pair).await;
        eprintln!("projectState after: gw-a {a}, gw-b {b}");
        assert_eq!((a.as_str(), b.as_str()), ("Good", "Good"));
    })
    .await;
    eprintln!(
        "PASS live_pair_tag_cleanup_converges in {:.1}s @ {}",
        started.elapsed().as_secs_f64(),
        utc_now()
    );
}

fn check_status_of(
    report: &ignition_core::deploy::pair::BackupReport,
    id: BackupCheckId,
) -> CheckStatus {
    report
        .checks
        .iter()
        .find(|c| c.id == id)
        .unwrap_or_else(|| panic!("no {id:?} check"))
        .status
}

// ---- quick 261102-cef (A2): tag units in core on the pair ---------------------------

/// The tag-type-definition unit of the A2 gates (O3): one UDT type in
/// `default/DeployTypes`, its marker listing `udts.json`.
const TYPES_UNIT: &str = "ignition/tag-type-definition/default/DeployTypes";
const TYPES_UDTS: &str = "ignition/tag-type-definition/default/DeployTypes/udts.json";
const TYPES_MARKER: &str = "ignition/tag-type-definition/default/DeployTypes/unary-resource.json";
const TYPES_UDTS_BYTES: &str = r#"[{"name":"DeployMotor","tagType":"UdtType","tags":[{"valueSource":"memory","dataType":"Float8","name":"Speed","tagType":"AtomicTag"}]}]"#;
const TYPES_MARKER_BYTES: &str = r#"{"scope":"G","version":1,"restricted":false,"overridable":true,"files":["udts.json"],"attributes":{"config":{}}}"#;

/// The four files of the two A2 units, with their bytes.
const CORE_UNIT_FILES: [(&str, &str); 4] = [
    (CANARY_TAGS, CANARY_TAGS_BYTES),
    (CANARY_MARKER, CANARY_MARKER_BYTES),
    (TYPES_UDTS, TYPES_UDTS_BYTES),
    (TYPES_MARKER, TYPES_MARKER_BYTES),
];

/// The text gw-b logs when its config-resource digest cannot match (debug
/// deploy-tag-redundancy-sync, T1).
const SNAPSHOT_MISMATCH: &str = "did not match remote system snapshot";

/// [`repo_with_tags`] declaring the canary and the `DeployTypes` unit.
fn repo_with_core_units(pair: &Pair) -> tempfile::TempDir {
    let repo = repo_with_tags(pair, true);
    let dir = repo
        .path()
        .join("gateways")
        .join(GATEWAY_DIR)
        .join("tags/default/tag-type-definition/DeployTypes");
    std::fs::create_dir_all(&dir).expect("types dir");
    std::fs::write(dir.join("udts.json"), TYPES_UDTS_BYTES).expect("udts.json");
    std::fs::write(dir.join("unary-resource.json"), TYPES_MARKER_BYTES).expect("marker");
    repo
}

/// Whether gw-b logged the snapshot-mismatch line since `since_ms` (WARN and
/// above, in the message or the stack).
async fn snapshot_mismatch_seen(pair: &Pair, since_ms: i64) -> bool {
    use ignition_core::client::logs::{DEFAULT_LOG_LIMIT, LogQuery};
    let query = LogQuery {
        start_time: Some(since_ms.max(0)),
        min_level: Some("WARN".to_string()),
        limit: DEFAULT_LOG_LIMIT,
        ..LogQuery::default()
    };
    match GatewayApi::logs(&pair.api_b_direct(), &query).await {
        Ok(envelope) => envelope.items.iter().any(|entry| {
            entry.timestamp >= since_ms
                && (entry.message.contains(SNAPSHOT_MISMATCH)
                    || entry
                        .stack
                        .iter()
                        .any(|line| line.contains(SNAPSHOT_MISMATCH)))
        }),
        Err(err) => panic!("gw-b's log could not be read: {}", err.code()),
    }
}

/// gw-b neither mismatched its snapshot nor gave up syncing since `since_ms`.
async fn assert_no_wedge(pair: &Pair, since_ms: i64, what: &str) {
    let b = pair.api_b_direct();
    let exhausted = BackupNode::new(&b, &b, None)
        .sync_exhausted(since_ms)
        .await
        .expect("gw-b's log reads");
    let mismatch = snapshot_mismatch_seen(pair, since_ms).await;
    eprintln!("{what}: gw-b snapshot mismatch {mismatch}, sync exhausted {exhausted}");
    assert!(
        !mismatch,
        "{what}: gw-b logged a config-resource snapshot mismatch"
    );
    assert!(!exhausted, "{what}: gw-b gave up syncing");
}

/// How gw-b's copy of `rel` in core compares with gw-a's: `byte-equal`,
/// `parsed-equal` (re-serialized), `differs` or `absent`.
fn core_copy_on_b(pair: &Pair, rel: &str) -> &'static str {
    let a = std::fs::read(Pair::root(&pair.data_a, "core").join(rel)).ok();
    let b = std::fs::read(Pair::root(&pair.data_b, "core").join(rel)).ok();
    match (a, b) {
        (_, None) | (None, _) => "absent",
        (Some(a), Some(b)) if a == b => "byte-equal",
        (Some(a), Some(b)) => {
            let parse = |bytes: &[u8]| serde_json::from_slice::<serde_json::Value>(bytes).ok();
            if parse(&a).is_some() && parse(&a) == parse(&b) {
                "parsed-equal"
            } else {
                "differs"
            }
        }
    }
}

/// The alarm-settings change of `repo` (a non-tag unit), as
/// `live_pair_tag_cleanup_converges` makes it.
fn change_alarm(repo: &Path, limit: i64) {
    let alarm = repo
        .join("gateways")
        .join(GATEWAY_DIR)
        .join("resources/ignition/general-alarm-settings/config.json");
    let mut doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&alarm).expect("alarm config")).expect("alarm json");
    doc["liveEventLimit"] = serde_json::json!(limit);
    std::fs::write(&alarm, serde_json::to_vec_pretty(&doc).expect("json")).expect("write");
}

/// A non-tag change after a tag apply reaches gw-b on its own: applied,
/// converged without a resync, `files` pass, both nodes `Good`. How long the
/// backup's first look takes is not part of the contract (quick 261104-7cy,
/// C10).
async fn assert_non_tag_change_syncs(pair: &Pair, repo: &Path, limit: i64, what: &str) {
    change_alarm(repo, limit);
    let ctx = Context::new(repo);
    let tap = MethodTap::start(&pair.url_b);
    let next = apply_over_pair(pair, &ctx, &tap, Duration::from_secs(90), &NoObserver).await;
    let report = next.backup.as_deref().expect("a backup report");
    eprintln!(
        "{what}: outcome {:?} ok {} written {:?}; backup converged {} waited_s {} resync {:?}",
        next.outcome, next.ok, next.written, report.converged, report.waited_s, report.resync,
    );
    assert_eq!(next.outcome, OutcomeKind::Applied, "{next:?}");
    assert!(report.converged, "{report:?}");
    assert_eq!(
        report.resync, None,
        "{what}: no resync was needed: {report:?}"
    );
    assert_eq!(
        check_status_of(report, BackupCheckId::Files),
        CheckStatus::Pass
    );
    tap.assert_reads_only(what);
    pair.wait_settled(Duration::from_secs(90))
        .await
        .expect("the pair settles after the non-tag change");
    let (a, b) = project_states(pair).await;
    eprintln!("{what}: projectState gw-a {a}, gw-b {b}");
    assert_eq!((a.as_str(), b.as_str()), ("Good", "Good"));
}

/// Prepare and run a destructive apply of `ctx` over the pair.
async fn apply_destructive_over_pair(
    pair: &Pair,
    ctx: &Context,
    tap: &MethodTap,
    backup_timeout: Duration,
) -> DeployApplyResult {
    let plan = ctx.plan().await;
    let gates = GateFlags {
        plan_sha256: Some(plan.plan_sha256.clone()),
        allow_destructive: true,
        allow_drift: None,
    };
    let prepared = deploy_apply_prepare(
        &ctx.declared,
        ctx.master_reader.as_ref(),
        &PrepareOptions {
            base: None,
            gates: gates.clone(),
        },
    )
    .await
    .expect("prepare over the pair");
    let master = pair.api_a();
    let backup_api = pair.api_b_at(&tap.url());
    let backup = BackupNode::new(&backup_api, &backup_api, Some(ctx.backup_reader.as_ref()));
    let surfaces = ApplySurfaces {
        deploy: &master,
        backup: Some(&backup),
        gateway: &master,
    };
    let mut opts = run_options(&plan.plan_sha256, backup_timeout);
    opts.gates = gates;
    deploy_apply_run(
        prepared,
        &ctx.declared,
        &ctx.repo,
        ctx.master_reader.as_ref(),
        &surfaces,
        &opts,
        &NoObserver,
    )
    .await
    .expect("the apply ran")
}

fn core_key(rel: &str) -> String {
    format!(".core/{rel}")
}

/// O3 (D1): on the real 8.3.6 pair, a tag-definition unit and a
/// tag-type-definition unit (`udts.json`) are written into gw-a's `core`:
/// the apply ends `applied`, the backup converges with no resync and the
/// `files` check passes over both units, gw-a's mode collection gains no tag
/// type folder, both nodes' `core` hold both units, gw-b logs no snapshot
/// mismatch and both nodes settle `Good`. A following non-tag change syncs
/// on its own. Then a removal apply deletes the units from gw-a's `core`
/// (floor 3: `core/ignition/<type>` stays), gw-b drops them with no resync,
/// and both nodes are `Good` again. Prints whether gw-b re-serializes
/// `udts.json` (P8) and what the removal left in `core` (A3).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live: needs the ign-pair-rig pair and IGNITION_LIVE_PAIR=1"]
async fn live_pair_core_tag_units_converge() {
    let Some(pair) = pair().await else { return };
    eprintln!("LIVE-RAN live_pair_core_tag_units_converge");
    let _serial = PAIR_LOCK.lock().await;
    let started = Instant::now();
    eprintln!("live_pair_core_tag_units_converge @ {}", utc_now());
    guarded(&pair, |pair| async move {
        let since_ms = ignition_core::deploy::pair::wall_clock_ms() - 1_000;
        let mode_a = Pair::root(&pair.data_a, &pair.mode);
        let repo = repo_with_core_units(&pair);
        let ctx = Context::new(repo.path());
        let tap = MethodTap::start(&pair.url_b);
        let events = Events::default();

        let applied = Instant::now();
        let result = apply_over_pair(&pair, &ctx, &tap, Duration::from_secs(120), &events).await;
        let backup = result.backup.as_deref().expect("a backup report");
        eprintln!(
            "core apply: outcome {:?} ok {} run {:?} in {} s; written {:?}; backup converged {} reason {:?} waited_s {} resync {:?} files_checked {}",
            result.outcome,
            result.ok,
            result.run_id,
            applied.elapsed().as_secs(),
            result.written,
            backup.converged,
            backup.reason,
            backup.waited_s,
            backup.resync,
            backup.files_checked,
        );
        for check in &backup.checks {
            eprintln!("backup check {:?}: {:?} {}", check.id, check.status, check.detail);
        }
        for (rel, _) in CORE_UNIT_FILES {
            eprintln!("core copy on gw-b of {rel}: {}", core_copy_on_b(&pair, rel));
        }
        assert_eq!(result.outcome, OutcomeKind::Applied, "{result:?}");
        assert!(result.ok);
        let mut written: Vec<String> = CORE_UNIT_FILES.iter().map(|(rel, _)| core_key(rel)).collect();
        written.sort();
        for key in &written {
            assert!(result.written.contains(key), "{key} not written: {:?}", result.written);
        }
        assert!(backup.converged, "{backup:?}");
        assert_eq!(backup.resync, None, "{backup:?}");
        assert!(backup.files_checked >= 4, "{backup:?}");
        assert_eq!(check_status_of(backup, BackupCheckId::Files), CheckStatus::Pass);
        for ty in TAG_TYPE_DIRS {
            assert!(
                !mode_a.join("ignition").join(ty).exists(),
                "gw-a's mode collection has no {ty} folder"
            );
        }
        for data in [&pair.data_a, &pair.data_b] {
            assert_eq!(
                Pair::core_units_on(data).len(),
                2,
                "both units in core of {}",
                data.display()
            );
        }
        tap.assert_reads_only("after the core apply");
        assert_no_wedge(&pair, since_ms, "after the core apply").await;
        let settled_s = pair
            .wait_settled(Duration::from_secs(90))
            .await
            .expect("the pair settles after the core apply");
        let (a, b) = project_states(&pair).await;
        eprintln!("core apply: settled after {settled_s} s; projectState gw-a {a}, gw-b {b}");
        assert_eq!((a.as_str(), b.as_str()), ("Good", "Good"));

        assert_non_tag_change_syncs(&pair, repo.path(), 9, "non-tag change after the core apply")
            .await;
        assert_no_wedge(&pair, since_ms, "after the non-tag change").await;

        // The removal: neither unit declared any more.
        let gateway = repo.path().join("gateways").join(GATEWAY_DIR);
        std::fs::remove_dir_all(gateway.join("tags/default/tag-definition")).expect("untag");
        std::fs::remove_dir_all(gateway.join("tags/default/tag-type-definition")).expect("untype");
        let ctx = Context::new(repo.path());
        let tap = MethodTap::start(&pair.url_b);
        let removal = apply_destructive_over_pair(&pair, &ctx, &tap, Duration::from_secs(120)).await;
        let report = removal.backup.as_deref().expect("a backup report");
        let core_a = Pair::root(&pair.data_a, "core").join("ignition");
        let core_b = Pair::root(&pair.data_b, "core").join("ignition");
        eprintln!(
            "core removal: outcome {:?} ok {} deleted {:?}; backup converged {} waited_s {} resync {:?}",
            removal.outcome,
            removal.ok,
            removal.deleted,
            report.converged,
            report.waited_s,
            report.resync,
        );
        for ty in TAG_TYPE_DIRS {
            eprintln!(
                "floor 3 (A3): gw-a core/ignition/{ty} kept {}, its default/ kept {}; gw-b core/ignition/{ty} kept {}, its default/ kept {}",
                core_a.join(ty).is_dir(),
                core_a.join(ty).join("default").is_dir(),
                core_b.join(ty).is_dir(),
                core_b.join(ty).join("default").is_dir(),
            );
        }
        assert_eq!(removal.outcome, OutcomeKind::Applied, "{removal:?}");
        for key in &written {
            assert!(removal.deleted.contains(key), "{key} not deleted: {:?}", removal.deleted);
        }
        assert!(Pair::core_units_on(&pair.data_a).is_empty(), "gw-a's core copies are gone");
        assert!(report.converged, "{report:?}");
        assert_eq!(report.resync, None, "{report:?}");
        for ty in TAG_TYPE_DIRS {
            assert!(core_a.join(ty).is_dir(), "core/ignition/{ty} stays on gw-a");
        }
        let gone = Instant::now();
        while !Pair::core_units_on(&pair.data_b).is_empty() {
            assert!(
                gone.elapsed() < Duration::from_secs(90),
                "gw-b still holds {:?} in core",
                Pair::core_units_on(&pair.data_b)
            );
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        tap.assert_reads_only("after the core removal");
        assert_no_wedge(&pair, since_ms, "after the core removal").await;
        pair.wait_settled(Duration::from_secs(90))
            .await
            .expect("the pair settles after the removal");
        let (a, b) = project_states(&pair).await;
        eprintln!("core removal: projectState gw-a {a}, gw-b {b}");
        assert_eq!((a.as_str(), b.as_str()), ("Good", "Good"));
    })
    .await;
    eprintln!(
        "PASS live_pair_core_tag_units_converge in {:.1}s @ {}",
        started.elapsed().as_secs_f64(),
        utc_now()
    );
}

/// O3 (D3): the migration on the real 8.3.6 pair. Both units sit in gw-a's
/// mode collection with a ledger that lists them bare, as a deploy before A2
/// left a pair, and a scan wedges it: gw-b logs the snapshot mismatch (or
/// gives up syncing), which also proves the log check of
/// `live_pair_core_tag_units_converge` can see the line. One apply of the
/// declaring repo with `--allow-destructive` moves both units into `core`:
/// it ends `applied`, both mode type folders are gone from gw-a, both nodes'
/// `core` hold both units, the on-box ledger lists their `.core/` keys and
/// the backup converges (the one apply resync is allowed: the pre-state was
/// wedged). A following non-tag change then syncs on its own, and both
/// nodes are `Good`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live: needs the ign-pair-rig pair and IGNITION_LIVE_PAIR=1"]
async fn live_pair_core_migration_converges() {
    let Some(pair) = pair().await else { return };
    eprintln!("LIVE-RAN live_pair_core_migration_converges");
    let _serial = PAIR_LOCK.lock().await;
    let started = Instant::now();
    eprintln!("live_pair_core_migration_converges @ {}", utc_now());
    guarded(&pair, |pair| async move {
        let mode_a = Pair::root(&pair.data_a, &pair.mode);

        // 1. The pre-A2 shape, then the wedge.
        for (rel, bytes) in CORE_UNIT_FILES {
            let path = mode_a.join(rel);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("unit dir");
            std::fs::write(path, bytes).expect("unit file");
        }
        let mut listed: Vec<String> = CORE_UNIT_FILES.iter().map(|(rel, _)| (*rel).to_string()).collect();
        listed.sort();
        let ledger = Ledger {
            schema_version: 1,
            gateway: GATEWAY_DIR.to_string(),
            mode: pair.mode.clone(),
            render_sha256: "0".repeat(64),
            run_id: "pre-a2-tags".to_string(),
            files: listed,
        };
        let ledger_path = ledger_location(LedgerRoot::DataDir, GATEWAY_DIR, &pair.mode)
            .split('/')
            .fold(pair.data_a.clone(), |acc, c| acc.join(c));
        std::fs::create_dir_all(ledger_path.parent().expect("parent")).expect("ledger dir");
        std::fs::write(&ledger_path, ledger.to_bytes()).expect("ledger");
        let since_ms = ignition_core::deploy::pair::wall_clock_ms() - 5_000;
        scan_and_wait(
            &pair.api_a(),
            ScanKind::Config,
            production_poll("config scan of the pre-A2 tag units"),
        )
        .await
        .expect("the seed scan");
        let b_direct = pair.api_b_direct();
        let b_logs = BackupNode::new(&b_direct, &b_direct, None);
        // The mismatch line first (what proves the log check of
        // `live_pair_core_tag_units_converge` can see it), then the exhausted
        // retries, so the apply starts on a settled wedge and no retry of
        // gw-b's can give up during the apply's backup wait.
        let wedge_started = Instant::now();
        while !snapshot_mismatch_seen(&pair, since_ms).await {
            assert!(
                wedge_started.elapsed() < Duration::from_secs(120),
                "gw-b logged no snapshot mismatch within 120 s of the pre-A2 scan: Ignition may now sync mode-collection tags"
            );
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        let mismatch_s = wedge_started.elapsed().as_secs();
        while !b_logs.sync_exhausted(since_ms).await.unwrap_or(false) {
            assert!(
                wedge_started.elapsed() < Duration::from_secs(300),
                "gw-b did not give up syncing within 300 s of the pre-A2 scan"
            );
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        let (a, b) = project_states(&pair).await;
        eprintln!(
            "wedge: snapshot mismatch after {mismatch_s} s, sync exhausted (TEC-5105) after {} s; projectState gw-a {a}, gw-b {b}",
            wedge_started.elapsed().as_secs()
        );

        // 2. One apply moves both units into core.
        let repo = repo_with_core_units(&pair);
        let ctx = Context::new(repo.path());
        let plan = ctx.plan().await;
        eprintln!(
            "migration plan: added {} changed {} delete set {} plan_sha256 {}",
            plan.summary.added,
            plan.summary.changed,
            plan.delete_set.files.len(),
            plan.plan_sha256
        );
        for unit in [CANARY_UNIT, TYPES_UNIT] {
            let entry = plan
                .entries
                .iter()
                .find(|e| e.key == unit)
                .unwrap_or_else(|| panic!("no plan entry for {unit}"));
            assert!(entry.destructive, "{unit}: a migration is destructive");
        }
        let tap = MethodTap::start(&pair.url_b);
        let applied = Instant::now();
        let result = apply_destructive_over_pair(&pair, &ctx, &tap, Duration::from_secs(45)).await;
        let report = result.backup.as_deref().expect("a backup report");
        eprintln!(
            "migration apply: outcome {:?} ok {} run {:?} in {} s; written {:?} deleted {:?}; backup converged {} reason {:?} waited_s {} resync {:?} files_checked {}",
            result.outcome,
            result.ok,
            result.run_id,
            applied.elapsed().as_secs(),
            result.written,
            result.deleted,
            report.converged,
            report.reason,
            report.waited_s,
            report.resync,
            report.files_checked,
        );
        for check in &report.checks {
            eprintln!("backup check {:?}: {:?} {}", check.id, check.status, check.detail);
        }
        for (rel, _) in CORE_UNIT_FILES {
            eprintln!("core copy on gw-b of {rel}: {}", core_copy_on_b(&pair, rel));
        }
        assert_eq!(result.outcome, OutcomeKind::Applied, "{result:?}");
        assert!(result.ok);
        for (rel, _) in CORE_UNIT_FILES {
            assert!(result.deleted.contains(&rel.to_string()), "{rel} not deleted");
            assert!(result.written.contains(&core_key(rel)), "{rel} not written to core");
        }
        for ty in TAG_TYPE_DIRS {
            assert!(
                !mode_a.join("ignition").join(ty).exists(),
                "gw-a's mode {ty} folder is gone"
            );
        }
        for data in [&pair.data_a, &pair.data_b] {
            assert_eq!(
                Pair::core_units_on(data).len(),
                2,
                "both units in core of {}",
                data.display()
            );
        }
        let on_box = Ledger::parse(
            "ledger.json",
            &std::fs::read(&ledger_path).expect("the on-box ledger"),
            GATEWAY_DIR,
            &pair.mode,
        )
        .expect("the on-box ledger parses");
        for (rel, _) in CORE_UNIT_FILES {
            assert!(on_box.files.contains(&core_key(rel)), "{rel} not ledgered in core");
            assert!(!on_box.files.contains(&rel.to_string()), "{rel} still ledgered bare");
        }
        assert!(report.converged, "{report:?}");
        assert_eq!(check_status_of(report, BackupCheckId::Files), CheckStatus::Pass);
        tap.assert_reads_only("after the migration apply");
        let settled_s = pair
            .wait_settled(Duration::from_secs(120))
            .await
            .expect("the migrated pair settles");
        eprintln!("migrated pair settled after {settled_s} s");

        // 3. A following non-tag change syncs on its own.
        let after_ms = ignition_core::deploy::pair::wall_clock_ms() - 1_000;
        assert_non_tag_change_syncs(&pair, repo.path(), 11, "non-tag change after the migration")
            .await;
        assert_no_wedge(&pair, after_ms, "after the migration's non-tag change").await;
    })
    .await;
    eprintln!(
        "PASS live_pair_core_migration_converges in {:.1}s @ {}",
        started.elapsed().as_secs_f64(),
        utc_now()
    );
}
