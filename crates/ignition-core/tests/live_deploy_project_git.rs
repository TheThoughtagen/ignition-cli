//! Phase 27's live gates on a git-module rig (27-03, the phase tracer's last
//! step): the keep-and-advance handover of 27-01/27-02 runs against JGit inside
//! the git module 2.3.4 on an Ignition 8.3.6 gateway. Nothing here is mocked.
//!
//! ```text
//! cd crates/ignition-core/tests/fixtures/live-rig-deploy
//! bash capture27.sh up27 && bash capture27.sh mint27 && bash capture27.sh restart27
//! bash capture27.sh reset27 && bash capture27.sh live27 <log> git_advance_status git_restore_survival
//! ```
//!
//! `live27` sets the variables and tees the output to
//! `$HOME/.cache/ign-git27-rig/evidence/<log>.log`; the equivalent by hand is
//! `IGNITION_LIVE_GIT=1 cargo test -p ignition-core --test live_deploy_project_git
//! -- --ignored --nocapture --test-threads=1 git_advance_status git_restore_survival`
//! with `tokens.env` sourced.
//!
//! | var | meaning |
//! |---|---|
//! | `IGNITION_LIVE_GIT` | must equal `1`; anything else is a quiet green skip |
//! | `IGN_GIT27_RIG_URL`, `IGN_GIT27_RIG_TOKEN` | the rig's loopback URL and API token (from `tokens.env`; the token is never printed) |
//! | `IGN_GIT27_RIG_DATA` | the host directory bound onto the gateway's `data/` (`projects/`, `config/resources/`) |
//! | `IGN_LIVE_EVIDENCE_DIR` | a directory outside the repository for each gate's result JSON |
//! | `IGN_GIT27_RIG_REMOTE` | the rig's bare repository (default `<data>/../repo/capapp.git`) |
//!
//! Safety: the URL guard refuses a non-loopback host before a request is made,
//! and a name guard refuses any gateway whose `gateway-info` name is not
//! `ign-git27-rig`. Output carries names, counts and hashes, never a token or a
//! repository URL.
//!
//! Both gates need the rig in the state `reset27` leaves (CapApp cloned by the
//! module at v1.0.0). `git_restore_survival` runs right after
//! `git_advance_status` in the same invocation (libtest orders by name under
//! `--test-threads=1`) and refuses to start when CapApp's HEAD is not v1.1.0.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use ignition_core::actions::deploy_projects::{GatewayProjects, ProjectGateway};
use ignition_core::actions::script::script_run;
use ignition_core::actions::webdev::webdev_deploy_with;
use ignition_core::client::deploy_api::scan_and_wait;
use ignition_core::client::scan::ScanKind;
use ignition_core::client::{GatewayApi, ReqwestGatewayApi};
use ignition_core::config::{self, Config, Credential, Secret};
use ignition_core::deploy::git_advance::{AdvancePrep, prepare_advance};
use ignition_core::deploy::git_handover::{GitSaveReport, restore_and_advance, save};
use ignition_core::deploy::git_remote::HostGit;
use ignition_core::deploy::git_state::{GitRefs, GitState, GitStateKind, classify};
use ignition_core::deploy::project_artifact::{Artifact, build_artifact};
use ignition_core::deploy::transport::local::LocalFsTransport;
use ignition_core::deploy::transport::projects::{GitEntry, GitHostRead, GitStateWriter};
use ignition_core::poll::PollConfig;

/// The only gateway this gate may talk to.
const RIG_GATEWAY_NAME: &str = "ign-git27-rig";
/// The compose project whose `gw` container the survival gate restarts.
const RIG_COMPOSE_PROJECT: &str = "ign-git27-rig";
/// The project the module manages.
const APP: &str = "CapApp";
/// The user the module's script functions run as.
const APP_USER: &str = "admin";
/// The gateway's name in ledger paths (a label, not a network name).
const GATEWAY: &str = "ign-git27-rig";
/// A file the v1.1.0 seed holds inside a resource folder that its
/// `resource.json` does not list, so the gateway keeps it off disk.
const NOTES: &str = "ignition/script-python/cap/util/NOTES.txt";
/// The save of the advance gate.
const RUN: &str = "20261113T120000Z-live";
/// The webdev project that carries the scriptExec route.
const WEBDEV_PROJECT: &str = "ign-cli";

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
        .unwrap_or_else(|| panic!("{name} must be set to run the live git gates"))
}

#[test]
fn local_url_guard_refuses_non_loopback() {
    for raw in [
        "http://192.0.2.15:8088",
        "https://gwnode01.example.local:8043",
        "http://localhost.evil.example:18292",
        "http://127.0.0.2:18292",
        "http://127.0.0.1@evil.example:18292",
        "not a url",
    ] {
        assert!(local_url_guard(raw).is_err(), "must refuse {raw}");
    }
}

#[test]
fn local_url_guard_accepts_loopback() {
    for raw in [
        "http://localhost:18292",
        "http://127.0.0.1:18292",
        "http://[::1]:18292",
    ] {
        assert!(local_url_guard(raw).is_ok(), "must accept {raw}");
    }
}

/// Run host git in `dir` with no user or system config; trimmed stdout.
fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        ignition_core::deploy::git_state::redact_text(&String::from_utf8_lossy(&out.stderr))
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

struct Rig {
    url: String,
    token: String,
    /// The host directory bound onto the gateway's `data/`.
    data: PathBuf,
    /// A directory outside the repository for result JSON.
    evidence: PathBuf,
    /// The rig's bare repository.
    remote: PathBuf,
}

impl Rig {
    fn api(&self) -> ReqwestGatewayApi {
        ReqwestGatewayApi::for_tests(
            &self.url,
            Some(Credential::Token(Secret::new(self.token.clone()))),
        )
    }

    fn node(&self) -> LocalFsTransport {
        LocalFsTransport::new(self.data.clone(), "nodes.master")
    }

    fn project_dir(&self) -> PathBuf {
        self.data.join("projects").join(APP)
    }
}

/// `None` is the quiet green skip. Every guard runs before the first request.
async fn rig() -> Option<Rig> {
    if std::env::var("IGNITION_LIVE_GIT").as_deref() != Ok("1") {
        skip("IGNITION_LIVE_GIT is not 1");
        return None;
    }
    let url = require_env("IGN_GIT27_RIG_URL");
    let token = require_env("IGN_GIT27_RIG_TOKEN");
    if let Err(why) = local_url_guard(&url) {
        panic!("IGN_GIT27_RIG_URL refused: {why}");
    }
    let data = PathBuf::from(require_env("IGN_GIT27_RIG_DATA"));
    assert!(
        data.join("projects").is_dir(),
        "IGN_GIT27_RIG_DATA has no projects directory"
    );
    let evidence = PathBuf::from(require_env("IGN_LIVE_EVIDENCE_DIR"));
    std::fs::create_dir_all(&evidence).expect("evidence dir");
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repo root")
        .canonicalize()
        .expect("repo root resolves");
    let evidence_real = evidence.canonicalize().expect("evidence dir resolves");
    assert!(
        !evidence_real.starts_with(&repo_root),
        "IGN_LIVE_EVIDENCE_DIR must be outside the repository"
    );
    let remote = std::env::var("IGN_GIT27_RIG_REMOTE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            data.parent()
                .expect("data dir has a parent")
                .join("repo")
                .join("capapp.git")
        });
    assert!(remote.is_dir(), "the rig's bare repository is missing");
    let rig = Rig {
        url,
        token,
        data,
        evidence,
        remote,
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

/// A release checkout of the rig repository at `tag`.
fn release_checkout(rig: &Rig, tag: &str) -> (tempfile::TempDir, PathBuf, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let checkout = dir.path().join("release");
    git(
        dir.path(),
        &[
            "clone",
            "-q",
            "--no-hardlinks",
            rig.remote.to_str().expect("utf8"),
            checkout.to_str().expect("utf8"),
        ],
    );
    git(&checkout, &["checkout", "-q", tag]);
    let commit = git(&checkout, &["rev-parse", &format!("{tag}^{{commit}}")]);
    (dir, checkout, commit)
}

/// The scriptExec route on the rig, and a config holding its secret.
async fn script_config(api: &ReqwestGatewayApi, url: &str) -> (tempfile::TempDir, Config) {
    let work = tempfile::tempdir().expect("tempdir");
    let path = work.path().join("config.toml");
    std::fs::write(
        &path,
        format!("active = \"rig\"\n\n[profiles.rig]\nurl = \"{url}\"\n"),
    )
    .expect("config");
    webdev_deploy_with(api, WEBDEV_PROJECT, true, false, &path, "rig", false, None)
        .await
        .unwrap_or_else(|err| panic!("webdev_deploy_with: {} ({err})", err.code()));
    let config = config::load(&path).expect("config reloads");
    // A route imported a moment ago answers an error (or still holds the old
    // secret) until the webdev project has loaded.
    let started = Instant::now();
    loop {
        match script_run(api, &config, "rig", WEBDEV_PROJECT, "print 1").await {
            Ok(_) => break,
            Err(err) => {
                assert!(
                    started.elapsed() < Duration::from_secs(240),
                    "the scriptExec route never answered: {}",
                    err.code()
                );
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        }
    }
    (work, config)
}

/// How the module reports a changed file: a file inside a resource (a path
/// starting `ignition` or `com.inductiveautomation.`) is reported as the
/// resource's folder, any other path as itself (`GitManager.hasActor`,
/// `GatewayScriptModule.collectUncommittedChanges`).
fn module_path(file: &str) -> String {
    let in_resource = file.starts_with("ignition") || file.starts_with("com.inductiveautomation.");
    match file.rfind('/') {
        Some(at) if in_resource => file[..at].to_string(),
        _ => file.to_string(),
    }
}

/// What JGit says about CapApp.
#[derive(Debug)]
struct Jgit {
    /// `getUncommittedChanges` resources, as a set.
    paths: BTreeSet<String>,
    /// `type resource` per change, byte-sorted.
    rows: Vec<String>,
    unpushed: u32,
    branch: String,
}

async fn jgit(api: &ReqwestGatewayApi, config: &Config) -> Jgit {
    let code = format!(
        "import system\n\
         for c in system.git.getUncommittedChanges(\"{APP}\", \"{APP_USER}\"):\n    \
             print \"CHANGE\\t%s\\t%s\" % (c.getType(), c.getResource())\n\
         print \"BRANCH_STATUS\\t%s\" % system.git.getBranchStatus(\"{APP}\", \"{APP_USER}\")\n\
         print \"CURRENT_BRANCH\\t%s\" % system.git.getCurrentBranch(\"{APP}\")\n"
    );
    let result = script_run(api, config, "rig", WEBDEV_PROJECT, &code)
        .await
        .unwrap_or_else(|err| panic!("script_run: {} ({err})", err.code()));
    let mut rows = Vec::new();
    let mut paths = BTreeSet::new();
    let mut unpushed = None;
    let mut branch = String::new();
    for line in result.stdout.lines() {
        let mut parts = line.splitn(3, '\t');
        match (parts.next(), parts.next(), parts.next()) {
            (Some("CHANGE"), Some(kind), Some(resource)) => {
                rows.push(format!("{kind} {resource}"));
                paths.insert(resource.to_string());
            }
            (Some("BRANCH_STATUS"), Some(status), None) => {
                let marker = "unpushedCommits=";
                let at = status
                    .find(marker)
                    .unwrap_or_else(|| panic!("no unpushedCommits in the branch status: {status}"));
                let digits: String = status[at + marker.len()..]
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect();
                unpushed = Some(digits.parse().expect("unpushedCommits is a number"));
            }
            (Some("CURRENT_BRANCH"), Some(name), None) => branch = name.to_string(),
            _ => {}
        }
    }
    rows.sort();
    Jgit {
        paths,
        rows,
        unpushed: unpushed.expect("the script printed a branch status"),
        branch,
    }
}

/// `.git` as the host read shows it, classified, with the refs.
async fn gateway_state(rig: &Rig, host: &HostGit) -> (GitState, GitRefs) {
    let files = rig
        .node()
        .read_git_state(APP)
        .await
        .expect("host read of .git");
    files.check_caps().expect("caps");
    let config = host
        .parse_config(
            files
                .config
                .as_deref()
                .expect("the module's clone has a config"),
        )
        .await
        .expect("config parses");
    let refs = host
        .parse_refs(&files)
        .await
        .expect("refs parse")
        .expect("the clone has a HEAD on a branch");
    let state = classify(&files, Some(&config), Some(&refs));
    (state, refs)
}

/// Wait until `.git` is absent (the module deletes it after an import, R-3).
/// Returns the seconds it took, or `None` when it never went away within
/// `limit`.
async fn wait_git_gone(rig: &Rig, limit: Duration) -> Option<f64> {
    let started = Instant::now();
    let node = rig.node();
    while started.elapsed() < limit {
        let files = node.read_git_state(APP).await.expect("probe");
        if files.kind == GitEntry::Absent {
            return Some(started.elapsed().as_secs_f64());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    None
}

/// Every file member of a zip, name to bytes.
fn zip_members(zip: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).expect("zip opens");
    let mut out = BTreeMap::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).expect("entry");
        if entry.is_dir() {
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).expect("member reads");
        out.insert(entry.name().to_string(), bytes);
    }
    out
}

/// Every in-scope file under `dir`, relative, `.git` skipped.
fn disk_in_scope(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, rel: &str, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(root.join(rel)).expect("read_dir") {
            let entry = entry.expect("entry");
            let name = entry.file_name().to_string_lossy().to_string();
            if rel.is_empty() && name == ".git" {
                continue;
            }
            let child = if rel.is_empty() {
                name
            } else {
                format!("{rel}/{name}")
            };
            if entry.file_type().expect("type").is_dir() {
                walk(root, &child, out);
            } else {
                out.insert(
                    child.clone(),
                    std::fs::read(root.join(&child)).expect("read"),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, "", &mut out);
    out.retain(|path, _| {
        path == "project.json"
            || path.starts_with("ignition/")
            || path.starts_with("com.inductiveautomation.")
    });
    out
}

/// The container id of the rig's `gw`, and nothing else.
fn rig_container() -> String {
    let out = Command::new("docker")
        .args([
            "ps",
            "-q",
            "--filter",
            &format!("label=com.docker.compose.project={RIG_COMPOSE_PROJECT}"),
            "--filter",
            "label=com.docker.compose.service=gw",
        ])
        .output()
        .expect("docker runs");
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert!(
        !text.is_empty() && !text.contains('\n'),
        "expected exactly one running gw container of {RIG_COMPOSE_PROJECT}"
    );
    text
}

fn write_evidence(rig: &Rig, name: &str, value: &serde_json::Value) {
    let path = rig.evidence.join(format!("{name}.json"));
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(value).expect("evidence json"),
    )
    .expect("evidence write");
    eprintln!("evidence: {}", path.display());
}

/// Everything a completed import-and-handover leaves for the checks.
struct Advanced {
    artifact: Artifact,
    export: BTreeMap<String, Vec<u8>>,
    report: ignition_core::deploy::git_handover::GitRestoreReport,
    saved: GitSaveReport,
    c1: String,
    c2: String,
    git_gone_secs: Option<f64>,
}

/// `prepare_advance`, `save`, the real overwrite import of the v1.1.0
/// artifact, the read-back export, `restore_and_advance`.
async fn run_handover(rig: &Rig, api: &ReqwestGatewayApi) -> Advanced {
    let host = HostGit::new();
    let node = rig.node();
    let (_keep, checkout, c2) = release_checkout(rig, "v1.1.0");
    let c1 = git(&checkout, &["rev-parse", "v1.0.0^{commit}"]);

    let (state, refs) = gateway_state(rig, &host).await;
    eprintln!(
        "LIVE-STATE kind={:?} reason={:?} head_branch_tip_is_v1_0_0={} remote={} tracked={:?}",
        state.kind,
        state.reason,
        refs.heads.values().any(|id| id == &c1),
        state
            .remote
            .as_ref()
            .map(|r| r.name.as_str())
            .unwrap_or("-"),
        state.tracked
    );
    assert_eq!(
        state.kind,
        GitStateKind::Managed,
        "the module's clone must classify as managed: {state:?}"
    );
    assert!(
        refs.heads.values().any(|id| id == &c1),
        "the module's clone must sit at v1.0.0 (run reset27 first)"
    );

    let artifact = build_artifact(&checkout).expect("artifact builds");
    let members: BTreeSet<String> = artifact.digest.members.keys().cloned().collect();
    let prep: AdvancePrep = prepare_advance(&host, &checkout, &state, &refs, &c2, &members)
        .await
        .expect("prepare_advance");
    let saved = save(&node, &node, GATEWAY, APP, RUN).await.expect("save");
    eprintln!(
        "LIVE-SAVE files={} bytes={}",
        saved.manifest.files, saved.manifest.bytes
    );

    let gateway = GatewayProjects(api);
    gateway
        .project_import(APP, artifact.zip.clone(), true)
        .await
        .unwrap_or_else(|err| panic!("overwrite import: {} ({err})", err.code()));
    let git_gone_secs = wait_git_gone(rig, Duration::from_secs(90)).await;
    eprintln!("LIVE-IMPORT git_removed_after_secs={git_gone_secs:?}");
    // Let the module's own import of the project settle before the read-back.
    tokio::time::sleep(Duration::from_secs(8)).await;
    let export_zip = gateway.export_bytes(APP).await.expect("read-back export");
    let export = zip_members(&export_zip);
    eprintln!("LIVE-EXPORT members={}", export.len());

    let report = restore_and_advance(
        &node,
        &node,
        &host,
        &checkout,
        &state,
        &saved,
        &prep,
        &export_zip,
    )
    .await
    .unwrap_or_else(|failure| {
        panic!(
            "restore_and_advance failed: reason={} paths={:?} detail={}",
            failure.reason, failure.paths, failure.detail
        )
    });
    Advanced {
        artifact,
        export,
        report,
        saved,
        c1,
        c2,
        git_gone_secs,
    }
}

/// G2: JGit inside the module reads the handed-over repo, and what it reports
/// is what ign predicted (D-20, D-25).
#[tokio::test]
#[ignore = "needs the git-module rig: IGNITION_LIVE_GIT=1"]
async fn git_advance_status() {
    let Some(rig) = rig().await else { return };
    eprintln!("LIVE-RAN git_advance_status");
    let api = rig.api();
    let (_work, config) = script_config(&api, &rig.url).await;

    let before = jgit(&api, &config).await;
    eprintln!(
        "LIVE-JGIT before: uncommitted={} unpushed={} branch={}",
        before.paths.len(),
        before.unpushed,
        before.branch
    );

    let advanced = run_handover(&rig, &api).await;
    let report = &advanced.report;
    eprintln!(
        "LIVE-HANDOVER restored={} advanced={} verified={} expected_drift={} dropped={}",
        report.restored,
        report.advanced,
        report.verified,
        report.expected_drift.len(),
        report.dropped.len()
    );
    assert!(
        report.restored && report.advanced && report.verified,
        "{report:?}"
    );

    // JGit's status equals the prediction (as sets: none extra, none missing).
    let after = jgit(&api, &config).await;
    // The module reports a file of a resource as the resource's folder.
    let expected: BTreeSet<String> = report
        .expected_drift
        .iter()
        .map(|file| module_path(file))
        .collect();
    let extra: Vec<&String> = after.paths.difference(&expected).collect();
    let missing: Vec<&String> = expected.difference(&after.paths).collect();
    eprintln!(
        "LIVE-JGIT after: uncommitted={} expected={} extra={} missing={} unpushed={} branch={}",
        after.paths.len(),
        expected.len(),
        extra.len(),
        missing.len(),
        after.unpushed,
        after.branch
    );
    for row in &after.rows {
        eprintln!("LIVE-JGIT-ROW {row}");
    }
    assert!(
        extra.is_empty() && missing.is_empty(),
        "JGit differs from the prediction: extra={extra:?} missing={missing:?}"
    );
    assert_eq!(after.unpushed, 0, "unpushed commits after the advance");
    // A tracked path the import removed must be assume-unchanged, never
    // missing: the module lists missing paths as `Deleted` first.
    assert!(
        after.rows.iter().all(|row| !row.starts_with("Deleted ")),
        "JGit lists a path as deleted: {:?}",
        after.rows
    );
    eprintln!(
        "LIVE-CHECK jgit_equals_expected_drift files={} module_paths={}",
        report.expected_drift.len(),
        expected.len()
    );

    // Disk bytes equal export bytes (D-25).
    let disk = disk_in_scope(&rig.project_dir());
    let export_in_scope: BTreeMap<&String, &Vec<u8>> = advanced.export.iter().collect();
    let mut different = Vec::new();
    for (name, bytes) in &advanced.export {
        match disk.get(name) {
            Some(on_disk) if on_disk == bytes => {}
            _ => different.push(name.clone()),
        }
    }
    let outside: Vec<&String> = disk
        .keys()
        .filter(|name| !export_in_scope.contains_key(name))
        .collect();
    assert!(
        different.is_empty() && outside.is_empty(),
        "disk differs from the export: different={different:?} outside_export={outside:?}"
    );
    eprintln!("LIVE-CHECK disk_equals_export {}", advanced.export.len());

    // The dropped in-scope member: NOTES.txt sits in a resource folder whose
    // resource.json does not list it. The artifact never carries it, the
    // gateway keeps it neither on disk nor in the export, the release tree
    // still holds it, and JGit does not report it as missing or modified.
    assert!(
        !advanced.artifact.digest.members.contains_key(NOTES),
        "the artifact must not carry an unlisted file"
    );
    assert!(!rig.project_dir().join(NOTES).exists(), "NOTES.txt on disk");
    assert!(!advanced.export.contains_key(NOTES), "NOTES.txt in export");
    assert!(
        !after.paths.contains(NOTES) && after.rows.iter().all(|row| !row.starts_with("Deleted ")),
        "JGit must not list NOTES.txt as missing or modified"
    );
    eprintln!(
        "LIVE-CHECK dropped_in_scope_member {NOTES} artifact_member=false report_dropped={:?}",
        report.dropped
    );

    // The release tree still holds NOTES.txt, so the index holds an entry for
    // it; the entry is assume-unchanged (`h`), which is why JGit does not list
    // the absent file as missing.
    let flags = git(&rig.project_dir(), &["ls-files", "-v", "--", NOTES]);
    assert_eq!(
        flags,
        format!("h {NOTES}"),
        "NOTES.txt must be an assume-unchanged index entry"
    );
    eprintln!("LIVE-CHECK dropped_member_assume_unchanged {NOTES} flag=h");

    // HEAD and the tracked ref are the release commit; the save is kept until
    // everything held, then removed by the caller.
    let host = HostGit::new();
    let (state, refs) = gateway_state(&rig, &host).await;
    assert_eq!(state.kind, GitStateKind::Managed, "{state:?}");
    assert!(
        refs.heads.values().any(|id| id == &advanced.c2),
        "the branch tip is the release commit"
    );
    assert!(
        !refs.heads.values().any(|id| id == &advanced.c1),
        "the branch tip moved off v1.0.0"
    );
    rig.node()
        .remove_git_save(GATEWAY, APP, RUN)
        .await
        .expect("remove the save");

    write_evidence(
        &rig,
        "git_advance_status",
        &serde_json::json!({
            "git_removed_after_secs": advanced.git_gone_secs,
            "save_files": advanced.saved.manifest.files,
            "save_bytes": advanced.saved.manifest.bytes,
            "export_members": advanced.export.len(),
            "expected_drift": report.expected_drift,
            "dropped": report.dropped,
            "jgit_rows": after.rows,
            "unpushed": after.unpushed,
            "baseline_uncommitted": before.paths.len(),
            "c1": advanced.c1,
            "c2": advanced.c2,
        }),
    );
}

/// G9: the advanced `.git` survives a restart, a project scan and a status
/// read, and a second overwrite import deletes it again (the module
/// re-registered it, R-7).
#[tokio::test]
#[ignore = "needs the git-module rig: IGNITION_LIVE_GIT=1"]
async fn git_restore_survival() {
    let Some(rig) = rig().await else { return };
    eprintln!("LIVE-RAN git_restore_survival");
    let api = rig.api();
    let host = HostGit::new();
    let (_keep, checkout, c2) = release_checkout(&rig, "v1.1.0");

    let (state, refs) = gateway_state(&rig, &host).await;
    assert!(
        state.kind == GitStateKind::Managed && refs.heads.values().any(|id| id == &c2),
        "refusing to start: CapApp's HEAD is not v1.1.0 (run git_advance_status first)"
    );
    let (_work, config) = script_config(&api, &rig.url).await;
    let before = jgit(&api, &config).await;
    eprintln!(
        "LIVE-JGIT before restart: uncommitted={} unpushed={}",
        before.paths.len(),
        before.unpushed
    );

    // Restart the gateway and wait for it and for CapApp.
    let cid = rig_container();
    let restarted = Command::new("docker")
        .args(["restart", &cid])
        .output()
        .expect("docker restart");
    assert!(restarted.status.success(), "docker restart failed");
    let started = Instant::now();
    loop {
        if GatewayApi::gateway_info(&api).await.is_ok()
            && GatewayProjects(&api)
                .project_find(APP)
                .await
                .map(|found| found.is_some())
                .unwrap_or(false)
        {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(600),
            "the gateway or {APP} did not come back within 600 s"
        );
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    // The module's start sync runs after the project answers.
    tokio::time::sleep(Duration::from_secs(60)).await;
    eprintln!(
        "LIVE-RESTART back_after_secs={:.0}",
        started.elapsed().as_secs_f64()
    );

    let after = jgit(&api, &config).await;
    let (state_after, refs_after) = gateway_state(&rig, &host).await;
    assert_eq!(state_after.kind, GitStateKind::Managed, "{state_after:?}");
    assert!(
        refs_after.heads.values().any(|id| id == &c2),
        "HEAD is still v1.1.0 after the restart"
    );
    assert_eq!(
        after.paths, before.paths,
        "the status changed across the restart"
    );
    assert_eq!(after.unpushed, 0, "unpushed commits after the restart");
    eprintln!(
        "LIVE-CHECK status_unchanged_across_restart {} head_v1_1_0=true",
        after.paths.len()
    );

    // A project scan changes nothing either.
    scan_and_wait(
        &api,
        ScanKind::Projects,
        PollConfig {
            subject: "project scan".to_string(),
            interval: Duration::from_millis(500),
            max: Duration::from_secs(2),
            deadline: Duration::from_secs(180),
        },
    )
    .await
    .unwrap_or_else(|err| panic!("project scan: {}", err.code()));
    tokio::time::sleep(Duration::from_secs(10)).await;
    let scanned = jgit(&api, &config).await;
    assert_eq!(
        scanned.paths, before.paths,
        "the status changed across the scan"
    );
    eprintln!(
        "LIVE-CHECK status_unchanged_across_scan {}",
        scanned.paths.len()
    );

    // A second overwrite import: does the advanced .git survive it?
    let artifact = build_artifact(&checkout).expect("artifact builds");
    GatewayProjects(&api)
        .project_import(APP, artifact.zip.clone(), true)
        .await
        .unwrap_or_else(|err| panic!("second overwrite import: {} ({err})", err.code()));
    let gone = wait_git_gone(&rig, Duration::from_secs(90)).await;
    eprintln!(
        "LIVE-CHECK second_import_git_deleted {} after_secs={gone:?}",
        gone.is_some()
    );

    write_evidence(
        &rig,
        "git_restore_survival",
        &serde_json::json!({
            "uncommitted_before_restart": before.paths.len(),
            "uncommitted_after_restart": after.paths.len(),
            "uncommitted_after_scan": scanned.paths.len(),
            "head_is_v1_1_0": true,
            "second_import_git_deleted": gone.is_some(),
            "second_import_git_deleted_after_secs": gone,
        }),
    );
}
