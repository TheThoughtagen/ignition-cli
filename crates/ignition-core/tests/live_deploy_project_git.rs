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

use ignition_core::actions::deploy_projects::{
    CHILDREN_POLL_S, CHILDREN_SETTLE_S, DeployProjectsResult, GatewayProjects, NoProjectsObserver,
    ProjectGateway, ProjectsInput, ProjectsRunOptions, ProjectsSurfaces, TAG_VERIFY_POLL_S,
    TAG_VERIFY_SETTLE_S, TagDeleteSurface, deploy_projects_prepare, deploy_projects_run,
    load_project,
};
use ignition_core::actions::script::script_run;
use ignition_core::actions::webdev::webdev_deploy_with;
use ignition_core::client::deploy_api::scan_and_wait;
use ignition_core::client::scan::ScanKind;
use ignition_core::client::{DeployApi, GatewayApi, ReqwestGatewayApi};
use ignition_core::config::{self, Config, Credential, Secret};
use ignition_core::deploy::apply::preflight::DesignersPolicy;
use ignition_core::deploy::git_advance::{AdvancePrep, prepare_advance};
use ignition_core::deploy::git_handover::{GitSaveReport, restore_and_advance, save};
use ignition_core::deploy::git_remote::HostGit;
use ignition_core::deploy::git_state::{GitRefs, GitState, GitStateKind, classify};
use ignition_core::deploy::pair::{BACKUP_POLL_S, BackupPolicy};
use ignition_core::deploy::project_artifact::{Artifact, build_artifact};
use ignition_core::deploy::transport::local::LocalFsTransport;
use ignition_core::deploy::transport::projects::{GitEntry, GitHostRead, GitStateWriter};
use ignition_core::deploy::transport::{self, projects::project_host_for_node};
use ignition_core::error::CoreError;
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
        None,
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

// ------------------------------------------------------------------- 27-04

/// The gateway's data directory inside the container.
const CONTAINER_DATA: &str = "/usr/local/bin/ignition/data";
/// The file the pushed commits change (a script no resource.json rewrite
/// touches, so a pull is a fast-forward even with the gateway-form drift).
const CODE_PY: &str = "ignition/script-python/cap/util/code.py";

/// `git` with an explicit environment and optional stdin; trimmed stdout.
fn git_with(dir: &Path, env: &[(&str, &str)], args: &[&str], stdin: Option<&[u8]>) -> String {
    use std::io::Write;
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdin(if stdin.is_some() {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::null()
        })
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    for (key, value) in env {
        command.env(key, value);
    }
    let mut child = command.spawn().expect("git spawns");
    if let Some(bytes) = stdin {
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(bytes)
            .expect("stdin write");
    }
    let out = child.wait_with_output().expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        ignition_core::deploy::git_state::redact_text(&String::from_utf8_lossy(&out.stderr))
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Run `code` through the scriptExec route; the stdout, or a short redacted
/// error text (a refusal the module throws is an observation, not a panic).
async fn try_script(
    api: &ReqwestGatewayApi,
    config: &Config,
    code: &str,
) -> Result<String, String> {
    match script_run(api, config, "rig", WEBDEV_PROJECT, code).await {
        Ok(result) => Ok(result.stdout),
        Err(err) => {
            let text =
                ignition_core::deploy::git_state::redact_text(&format!("{}: {err}", err.code()));
            Err(text.replace('\n', " ").chars().take(360).collect())
        }
    }
}

/// The reflection that reaches the module's script module (research Probe).
const MODULE_PREAMBLE: &str = "import system\n\
    from com.inductiveautomation.ignition.gateway import IgnitionGateway\n\
    hook = IgnitionGateway.get().getModule(\"com.axone_io.ignition.git\")\n\
    f = hook.getClass().getDeclaredField(\"scriptModule\")\n\
    f.setAccessible(True)\n\
    sm = f.get(hook)\n";

/// What the host read of `.git` makes of the repo now; never panics.
async fn classify_now(rig: &Rig, host: &HostGit) -> (GitState, usize, usize) {
    let files = rig
        .node()
        .read_git_state(APP)
        .await
        .expect("host read of .git");
    let config = match files.config.as_deref() {
        Some(bytes) => host.parse_config(bytes).await.ok(),
        None => None,
    };
    let refs = if files.head.is_some() {
        host.parse_refs(&files).await.ok().flatten()
    } else {
        None
    };
    let state = classify(&files, config.as_ref(), refs.as_ref());
    let heads = refs.as_ref().map_or(0, |r| r.heads.len());
    let remotes = refs.as_ref().map_or(0, |r| r.remotes.len());
    (state, heads, remotes)
}

/// Observation only: what `GitHostRead::read_git_state` and `classify` make of
/// CapApp's `.git` right now (G6 shapes). Always green once the rig answers.
#[tokio::test]
#[ignore = "needs the git-module rig: IGNITION_LIVE_GIT=1"]
async fn git_state_probe() {
    let Some(rig) = rig().await else { return };
    eprintln!("LIVE-RAN git_state_probe");
    let host = HostGit::new();
    let (state, heads, remotes) = classify_now(&rig, &host).await;
    let files = rig
        .node()
        .read_git_state(APP)
        .await
        .expect("host read of .git");
    eprintln!(
        "LIVE-CLASSIFY kind={} reason={:?} head={:?} remote={} tracked={:?} markers={:?} index_present={} loose_refs={} refs_heads={heads} refs_remotes={remotes}",
        state.kind.as_str(),
        state.reason,
        state.head,
        state
            .remote
            .as_ref()
            .map(|r| r.name.as_str())
            .unwrap_or("-"),
        state.tracked,
        files.markers,
        files.index_present,
        files.loose_refs.len()
    );
}

/// Push a new commit on `production` of the rig remote (a temp clone changes
/// `code.py`); returns the new commit id.
fn push_next_commit(rig: &Rig, body: &str, message: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let work = dir.path().join("w");
    let remote = rig.remote.to_str().expect("utf8");
    git(
        dir.path(),
        &[
            "clone",
            "-q",
            "--no-hardlinks",
            "-b",
            "production",
            remote,
            work.to_str().expect("utf8"),
        ],
    );
    std::fs::write(work.join(CODE_PY), body).expect("code.py");
    git(&work, &["add", "-A"]);
    git(
        &work,
        &[
            "-c",
            "user.name=rig",
            "-c",
            "user.email=rig@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            message,
        ],
    );
    git(&work, &["push", "-q", remote, "HEAD:refs/heads/production"]);
    // The git server container reads this tree as another user.
    let _ = Command::new("chmod")
        .args(["-R", "a+rX", remote])
        .status()
        .expect("chmod");
    git(&work, &["rev-parse", "HEAD"])
}

/// Restart the gateway and wait for it, the project and the module's sync.
async fn restart_gateway(api: &ReqwestGatewayApi) {
    let cid = rig_container();
    let restarted = Command::new("docker")
        .args(["restart", &cid])
        .output()
        .expect("docker restart");
    assert!(restarted.status.success(), "docker restart failed");
    let started = Instant::now();
    loop {
        if GatewayApi::gateway_info(api).await.is_ok()
            && GatewayProjects(api)
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
    tokio::time::sleep(Duration::from_secs(75)).await;
    eprintln!(
        "LIVE-RESTART back_after_secs={:.0}",
        started.elapsed().as_secs_f64()
    );
}

/// The production git.yaml of `capture27.sh yaml27 production` (all import
/// flags off), written in place.
fn write_production_yaml(rig: &Rig) {
    let text = "- repo_uri: http://git-server/git/capapp.git\n\
        \x20 repo_branch: production\n\
        \x20 ignition_projectName: CapApp\n\
        \x20 ignition_userName: admin\n\
        \x20 ignition_inheritable: false\n\
        \x20 user_name: rig\n\
        \x20 user_email: rig@example.invalid\n\
        \x20 user_password: not-checked\n\
        \x20 commissioning_importThemes: false\n\
        \x20 commissioning_importTags: false\n\
        \x20 commissioning_importImages: false\n\
        \x20 commissioning_enforceBranch: false\n\
        \x20 tags_importOnStartup: false\n\
        \x20 production_mode: true\n\
        \x20 production_branch: production\n\
        \x20 production_tagPattern: \"v*\"\n";
    std::fs::write(rig.data.join("git.yaml"), text).expect("git.yaml write");
    eprintln!("LIVE-YAML production variant written (the credential key is not shown)");
}

/// The module's `pull` through the script function; the error is the module's
/// refusal text.
async fn module_pull(api: &ReqwestGatewayApi, config: &Config, label: &str) -> Result<(), String> {
    let code = format!(
        "import system\nprint \"PULL %s\" % system.git.pull(\"{APP}\", \"{APP_USER}\", False, False, False)\n"
    );
    let outcome = try_script(api, config, &code).await;
    match &outcome {
        Ok(out) => eprintln!("LIVE-PULL {label} ok stdout={}", out.trim()),
        Err(text) => eprintln!("LIVE-PULL {label} refused_or_failed {text}"),
    }
    outcome.map(|_| ())
}

/// After a pull: HEAD is `tip`, one parent, and that parent is `before`
/// (a fast-forward); returns the export of the project.
fn assert_fast_forward(rig: &Rig, label: &str, before: &str, tip: &str) {
    let proj = rig.project_dir();
    assert!(
        proj.join(".git").is_dir(),
        "{label}: .git is gone after the pull"
    );
    let head = git(&proj, &["rev-parse", "HEAD"]);
    let parents = git(&proj, &["rev-list", "--parents", "-n", "1", "HEAD"]);
    let ids: Vec<&str> = parents.split_whitespace().collect();
    eprintln!(
        "LIVE-FF {label} head_is_tip={} parents={} parent_is_before={} merge_commit={}",
        head == tip,
        ids.len().saturating_sub(1),
        ids.get(1) == Some(&before),
        ids.len() > 2
    );
    assert_eq!(head, tip, "{label}: HEAD did not reach the pushed commit");
    assert_eq!(ids.len(), 2, "{label}: not a single-parent commit");
    assert_eq!(ids[1], before, "{label}: not a fast-forward from {before}");
}

/// The read-back export against the release at `tip`: same member names, every
/// `.py` byte-identical; counts the resource.json and project.json members
/// the gateway re-serialises.
async fn assert_export_matches(
    rig: &Rig,
    api: &ReqwestGatewayApi,
    label: &str,
    tip: &str,
    expect_code: &str,
) {
    let (_keep, checkout, _c) = release_checkout(rig, tip);
    let artifact = build_artifact(&checkout).expect("artifact builds");
    let wanted = zip_members(&artifact.zip);
    let export = zip_members(
        &GatewayProjects(api)
            .export_bytes(APP)
            .await
            .expect("export"),
    );
    let wanted_names: BTreeSet<&String> = wanted.keys().collect();
    let export_names: BTreeSet<&String> = export.keys().collect();
    let mut py_differs = Vec::new();
    let mut json_reserialised = 0usize;
    for (name, bytes) in &wanted {
        match export.get(name) {
            Some(got) if got == bytes => {}
            Some(_) if name.ends_with(".py") => py_differs.push(name.clone()),
            Some(_) => json_reserialised += 1,
            None => {}
        }
    }
    eprintln!(
        "LIVE-EXPORT-MATCH {label} members_equal={} py_identical={} json_reserialised={json_reserialised} code_py_is_expected={}",
        wanted_names == export_names,
        py_differs.is_empty(),
        export
            .get(CODE_PY)
            .map(|b| String::from_utf8_lossy(b).trim().to_string())
            == Some(expect_code.trim().to_string())
    );
    assert_eq!(wanted_names, export_names, "{label}: export members differ");
    assert!(
        py_differs.is_empty(),
        "{label}: scripts differ: {py_differs:?}"
    );
    assert_eq!(
        export
            .get(CODE_PY)
            .map(|b| String::from_utf8_lossy(b).trim().to_string()),
        Some(expect_code.trim().to_string()),
        "{label}: the gateway does not hold the pulled script"
    );
}

/// G7 (A5): after ign's advance, `system.git.pull` fast-forwards a commit pushed
/// by someone else; the same on a partial `setupLocalRepo` state adopted by
/// hand; and in the production variant the module commits the drift and pushes,
/// leaving the files outside the artifact scope in the remote tree (R-8).
#[tokio::test]
#[ignore = "needs the git-module rig: IGNITION_LIVE_GIT=1"]
async fn git_pull_push_roundtrip() {
    let Some(rig) = rig().await else { return };
    eprintln!("LIVE-RAN git_pull_push_roundtrip");
    let api = rig.api();
    let host = HostGit::new();
    let (_work, config) = script_config(&api, &rig.url).await;

    // 1. ign's advance to v1.1.0 (as git_advance_status does).
    let advanced = run_handover(&rig, &api).await;
    assert!(
        advanced.report.restored && advanced.report.advanced && advanced.report.verified,
        "{:?}",
        advanced.report
    );
    let drift = jgit(&api, &config).await;
    eprintln!(
        "LIVE-JGIT after advance: uncommitted={} unpushed={} branch={}",
        drift.paths.len(),
        drift.unpushed,
        drift.branch
    );
    assert_eq!(drift.unpushed, 0);

    // 2. Pull half, template variant: a commit pushed by someone else.
    let tip_a = push_next_commit(&rig, "def f():\n    return 4\n", "release 1.2.0");
    module_pull(&api, &config, "template-after-advance")
        .await
        .expect("the module's pull answers");
    tokio::time::sleep(Duration::from_secs(8)).await;
    assert_fast_forward(&rig, "advance", &advanced.c2, &tip_a);
    let after_pull = jgit(&api, &config).await;
    eprintln!(
        "LIVE-JGIT after pull: uncommitted={} unpushed={} same_set_as_before={}",
        after_pull.paths.len(),
        after_pull.unpushed,
        after_pull.paths == drift.paths
    );
    assert_eq!(after_pull.paths, drift.paths, "the pull changed the status");
    assert_eq!(after_pull.unpushed, 0, "unpushed after the pull");
    assert_export_matches(&rig, &api, "advance", &tip_a, "def f():\n    return 4\n").await;
    eprintln!(
        "LIVE-CHECK pull_fast_forwards_after_advance tip={}",
        &tip_a[..7]
    );

    // 3. Pull half on a partial state adopted by hand (research R-9 recipe).
    let cid = rig_container();
    let removed = Command::new("docker")
        .args([
            "exec",
            "-u",
            "root",
            &cid,
            "sh",
            "-c",
            &format!("rm -rf {CONTAINER_DATA}/projects/{APP}/.git"),
        ])
        .output()
        .expect("docker exec");
    assert!(removed.status.success(), "removing .git failed");
    let trigger = format!(
        "{MODULE_PREAMBLE}sm.setupLocalRepo(\"{APP}\", \"{APP_USER}\")\nprint \"TRIGGER done\"\n"
    );
    let triggered = try_script(&api, &config, &trigger).await;
    eprintln!("LIVE-TRIGGER setupLocalRepo {triggered:?}");
    triggered.expect("setupLocalRepo answers");
    let (partial, heads, remotes) = classify_now(&rig, &host).await;
    eprintln!(
        "LIVE-PARTIAL kind={} reason={:?} head={:?} remote={} refs_heads={heads} refs_remotes={remotes}",
        partial.kind.as_str(),
        partial.reason,
        partial.head,
        partial
            .remote
            .as_ref()
            .map(|r| r.name.as_str())
            .unwrap_or("-")
    );
    let proj = rig.project_dir();
    let gitdir = proj.join(".git");
    let gd = format!("--git-dir={}", gitdir.display());
    let tip = git(&rig.remote, &["rev-parse", "refs/heads/production"]);
    assert_eq!(tip, tip_a, "the remote's production is the pulled commit");
    git(&proj, &[&gd, "update-ref", "refs/heads/production", &tip]);
    git(
        &proj,
        &[&gd, "symbolic-ref", "HEAD", "refs/heads/production"],
    );
    git(
        &proj,
        &[&gd, "config", "branch.production.remote", "origin"],
    );
    git(
        &proj,
        &[
            &gd,
            "config",
            "branch.production.merge",
            "refs/heads/production",
        ],
    );
    let index = gitdir.join("index");
    let index_env = [("GIT_INDEX_FILE", index.to_str().expect("utf8"))];
    git_with(&proj, &index_env, &[&gd, "read-tree", &tip], None);
    let listed = git(&proj, &[&gd, "ls-tree", "-r", "--name-only", &tip]);
    let mut absent = Vec::new();
    for path in listed.lines() {
        if !proj.join(path).exists() {
            absent.push(path.to_string());
        }
    }
    let stdin: Vec<u8> = absent
        .iter()
        .flat_map(|p| p.bytes().chain(std::iter::once(0)))
        .collect();
    git_with(
        &proj,
        &index_env,
        &[&gd, "update-index", "--assume-unchanged", "-z", "--stdin"],
        Some(&stdin),
    );
    eprintln!(
        "LIVE-ADOPT head=production tip={} absent_assume_unchanged={}",
        &tip[..7],
        absent.len()
    );
    let (adopted, _h, _r) = classify_now(&rig, &host).await;
    eprintln!(
        "LIVE-ADOPT classify kind={} reason={:?} tracked={:?}",
        adopted.kind.as_str(),
        adopted.reason,
        adopted.tracked
    );
    let adopted_status = jgit(&api, &config).await;
    eprintln!(
        "LIVE-JGIT after adoption: uncommitted={} unpushed={} same_set_as_before={}",
        adopted_status.paths.len(),
        adopted_status.unpushed,
        adopted_status.paths == drift.paths
    );
    assert_eq!(
        adopted_status.paths, drift.paths,
        "JGit's status differs after the hand adoption"
    );
    let tip_b = push_next_commit(&rig, "def f():\n    return 5\n", "release 1.3.0");
    module_pull(&api, &config, "template-after-adoption")
        .await
        .expect("the module's pull answers");
    tokio::time::sleep(Duration::from_secs(8)).await;
    assert_fast_forward(&rig, "adoption", &tip_a, &tip_b);
    let after_adopt_pull = jgit(&api, &config).await;
    assert_eq!(after_adopt_pull.paths, drift.paths);
    assert_eq!(
        after_adopt_pull.unpushed, 0,
        "unpushed after the adopted pull"
    );
    assert_export_matches(&rig, &api, "adoption", &tip_b, "def f():\n    return 5\n").await;
    eprintln!(
        "LIVE-CHECK pull_fast_forwards_after_adoption tip={}",
        &tip_b[..7]
    );

    // 4. Production variant: refusal with drift, then commit the drift and push.
    write_production_yaml(&rig);
    restart_gateway(&api).await;
    let (_work2, config) = script_config(&api, &rig.url).await;
    let before_prod = jgit(&api, &config).await;
    eprintln!(
        "LIVE-JGIT production start: uncommitted={} unpushed={}",
        before_prod.paths.len(),
        before_prod.unpushed
    );
    assert_eq!(
        before_prod.paths, drift.paths,
        "the restart changed the status"
    );
    let pull_refused = module_pull(&api, &config, "production-with-drift").await;
    let push_code =
        format!("{MODULE_PREAMBLE}print \"PUSH %s\" % sm.push(\"{APP}\", \"{APP_USER}\")\n");
    let push_with_drift = try_script(&api, &config, &push_code).await;
    eprintln!("LIVE-PUSH production-with-drift {push_with_drift:?}");
    eprintln!(
        "LIVE-PROD drift_blocks pull_refused={} push_refused={}",
        pull_refused.is_err(),
        push_with_drift.is_err()
    );
    let remote_before = git(&rig.remote, &["rev-parse", "refs/heads/production"]);
    let literal: Vec<String> = before_prod
        .paths
        .iter()
        .map(|p| format!("\"{p}\""))
        .collect();
    let commit_code = format!(
        "{MODULE_PREAMBLE}print \"COMMIT %s\" % sm.commit(\"{APP}\", \"{APP_USER}\", [{}], \"normalize gateway form\")\n",
        literal.join(", ")
    );
    let committed = try_script(&api, &config, &commit_code).await;
    eprintln!("LIVE-COMMIT {committed:?}");
    committed.expect("the module's commit answers");
    let after_commit = jgit(&api, &config).await;
    eprintln!(
        "LIVE-JGIT after commit: uncommitted={} unpushed={}",
        after_commit.paths.len(),
        after_commit.unpushed
    );
    let changed = git(
        &proj,
        &["diff-tree", "--no-commit-id", "--name-status", "-r", "HEAD"],
    );
    let deleted_paths: Vec<&str> = changed
        .lines()
        .filter(|l| l.starts_with('D'))
        .filter_map(|l| l.split_once('\t').map(|(_, path)| path))
        .collect();
    let deleted = deleted_paths.len();
    eprintln!(
        "LIVE-COMMIT files={} deleted={deleted} deleted_paths={deleted_paths:?}",
        changed.lines().count()
    );
    // Observed (27-04): the module's `add` of a resource folder stages the
    // absence of an in-scope member the gateway dropped (NOTES.txt, an
    // assume-unchanged entry) as a deletion. Anything else deleted, or a file
    // outside the artifact scope, would be a different and worse result.
    assert!(
        deleted_paths.iter().all(|p| *p == NOTES),
        "the module's commit deleted files other than the dropped in-scope member: {changed}"
    );
    eprintln!(
        "LIVE-CHECK drift_commit_deletes_dropped_member {}",
        deleted_paths.contains(&NOTES)
    );
    assert!(
        after_commit.paths.is_empty(),
        "the status is not clean after committing the drift: {:?}",
        after_commit.rows
    );
    assert_eq!(after_commit.unpushed, 1, "one commit to push");
    let pushed = try_script(&api, &config, &push_code).await;
    eprintln!("LIVE-PUSH production-after-commit {pushed:?}");
    pushed.expect("the module's push in production mode succeeds once the status is clean");
    tokio::time::sleep(Duration::from_secs(3)).await;
    let remote_after = git(&rig.remote, &["rev-parse", "refs/heads/production"]);
    let local_head = git(&proj, &["rev-parse", "HEAD"]);
    eprintln!(
        "LIVE-PUSH remote_moved={} remote_is_local_head={}",
        remote_after != remote_before,
        remote_after == local_head
    );
    assert_ne!(remote_after, remote_before, "the remote tip did not move");
    assert_eq!(
        remote_after, local_head,
        "the remote tip is not the pushed HEAD"
    );
    let tree = git(
        &rig.remote,
        &["ls-tree", "-r", "--name-only", "refs/heads/production"],
    );
    let outside = [
        "README.md",
        "tags/default/tags.json",
        "images/logo.txt",
        "deploy/project.yaml",
        ".gitignore",
    ];
    let missing: Vec<&str> = outside
        .iter()
        .copied()
        .filter(|p| !tree.lines().any(|l| l == *p))
        .collect();
    eprintln!(
        "LIVE-CHECK remote_tree_keeps_out_of_scope_files kept={} missing={missing:?} total_files={}",
        outside.len() - missing.len(),
        tree.lines().count()
    );
    assert!(
        missing.is_empty(),
        "the pushed tree lost files: {missing:?}"
    );
    let final_status = jgit(&api, &config).await;
    eprintln!(
        "LIVE-JGIT final: uncommitted={} unpushed={}",
        final_status.paths.len(),
        final_status.unpushed
    );

    rig.node()
        .remove_git_save(GATEWAY, APP, RUN)
        .await
        .expect("remove the save");
    write_evidence(
        &rig,
        "git_pull_push_roundtrip",
        &serde_json::json!({
            "advance_tip": advanced.c2,
            "pulled_tip_after_advance": tip_a,
            "pulled_tip_after_adoption": tip_b,
            "partial_kind": partial.kind.as_str(),
            "partial_reason": partial.reason,
            "absent_assume_unchanged": absent.len(),
            "drift_paths": drift.rows,
            "production_pull_refused_with_drift": pull_refused.is_err(),
            "production_push_refused_with_drift": push_with_drift.is_err(),
            "remote_before": remote_before,
            "remote_after": remote_after,
            "committed_deleted_files": deleted,
        }),
    );
}

// ------------------------------------------------------------------- 27-23
//
// C4 on the single-node rig: the shipped `ign deploy projects`, wired in
// process as the CLI wires it, against the module-managed CapApp. Every gate
// compares what JGit inside the module says with what the run predicted
// (`data.git.expected_drift`); a difference is a cut-over blocker (D-25), so
// the assertions here are never loosened.

/// The target file's environment.
const ENV: &str = "rig";
/// The origin URL the module holds in CapApp's `.git/config` (the git-server
/// container's name). The test checkout keeps it as `remote.origin.url` so the
/// run's `url_key` comparison matches, and reaches the server through
/// `insteadOf` onto the host path of the same bare repository.
const GATEWAY_ORIGIN: &str = "http://git-server/git/capapp.git";
const GATEWAY_ORIGIN_BASE: &str = "http://git-server/git/";
/// The reason every override of these gates carries.
const REASON: &str = "27-23 live gate on a disposable rig";

/// The flags of one in-process run, as the CLI fills them.
#[derive(Default, Clone)]
struct DeployFlags {
    artifact_sha256: Option<String>,
    discard_gateway_changes: Option<String>,
    allow_git_sync_hazards: Option<String>,
    first_change: bool,
}

/// A temp release checkout and targets root, alive for one run.
struct Deployment {
    _scratch: tempfile::TempDir,
    repo: PathBuf,
    targets: PathBuf,
}

/// A checkout of the rig remote with HEAD on `release` (a tag), whose origin is
/// the gateway's own URL, and a targets root whose master is a `local:` node on
/// the rig's data directory.
fn deployment(rig: &Rig, release: &str) -> Deployment {
    let scratch = tempfile::tempdir().expect("tempdir");
    let repo = scratch.path().join("repo");
    let targets = scratch.path().join("targets-root");
    let remote = rig.remote.to_str().expect("utf8");
    git(
        scratch.path(),
        &[
            "clone",
            "-q",
            "--no-hardlinks",
            remote,
            repo.to_str().expect("utf8"),
        ],
    );
    git(&repo, &["remote", "set-url", "origin", GATEWAY_ORIGIN]);
    let base = format!(
        "{}/",
        rig.remote
            .parent()
            .expect("the bare repository has a parent")
            .display()
    );
    git(
        &repo,
        &[
            "config",
            &format!("url.{base}.insteadOf"),
            GATEWAY_ORIGIN_BASE,
        ],
    );
    git(&repo, &["checkout", "-q", release]);
    // ign's own run directory is under `.ign/`.
    let exclude = repo.join(".git/info/exclude");
    std::fs::create_dir_all(exclude.parent().expect("info dir")).expect("info dir");
    std::fs::write(&exclude, ".ign/\n").expect("exclude");
    std::fs::create_dir_all(targets.join("targets")).expect("targets dir");
    std::fs::write(
        targets.join("targets").join(format!("{ENV}.yaml")),
        format!(
            "schema_version: 1\ngateway: {RIG_GATEWAY_NAME}\nmode: production\nnodes:\n  master:\n    \
             profile: rig\n    gateway_name: {RIG_GATEWAY_NAME}\n    local:\n      data_dir: '{}'\n",
            rig.data.display()
        ),
    )
    .expect("target file");
    Deployment {
        _scratch: scratch,
        repo,
        targets,
    }
}

/// `ign deploy projects` in process, as the CLI wires it: the real client, the
/// real host reads over a `local:` master on the rig data root, the master's
/// tree reader and `DeployApi`, the CLI's timings. `check` is `--check`. The
/// result JSON goes to the evidence directory under `step`.
async fn deploy_in_process(
    rig: &Rig,
    check: bool,
    env: &str,
    release: &str,
    flags: &DeployFlags,
    step: &str,
) -> Result<DeployProjectsResult, CoreError> {
    let api = rig.api();
    let dep = deployment(rig, release);
    let input = ProjectsInput {
        repo: dep.repo.clone(),
        targets: dep.targets.clone(),
        env: env.to_string(),
        release: release.to_string(),
        allow_downgrade: false,
        allow_first_change: flags.first_change.then(|| REASON.to_string()),
        discard_gateway_changes: flags.discard_gateway_changes.clone(),
        allow_git_sync_hazards: flags.allow_git_sync_hazards.clone(),
        allow_destructive: false,
        allow_manual_undo: false,
        artifact_sha256: flags.artifact_sha256.clone(),
        tags_sha256: None,
    };
    let loaded = load_project(&input)?;
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
    let gateway = GatewayProjects(&api);
    let surfaces = ProjectsSurfaces {
        gateway: &gateway,
        host: host.as_ref(),
        tree: Some(tree.as_ref()),
        apply_host: None,
        deploy: Some(&api as &dyn DeployApi),
        tag_delete: TagDeleteSurface::NotConfigured { missing: vec![] },
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
        backup: None,
        backup_policy: BackupPolicy {
            budget: Duration::from_secs(300),
            interval: Duration::from_secs(BACKUP_POLL_S),
        },
        tag_delete_ready: Duration::from_secs(
            ignition_core::deploy::tag_delete::TAG_DELETE_READY_TIMEOUT_S,
        ),
        tag_delete_poll: Duration::from_secs(
            ignition_core::deploy::tag_delete::TAG_DELETE_READY_POLL_S,
        ),
        git_writer: None,
    };
    let prepared = deploy_projects_prepare(&loaded, &input, &surfaces).await?;
    let result = if check {
        prepared.into_check_result()
    } else {
        deploy_projects_run(
            prepared,
            &loaded,
            &input,
            &surfaces,
            &opts,
            &NoProjectsObserver,
        )
        .await?
    };
    write_evidence(
        rig,
        step,
        &serde_json::to_value(&result).expect("serializes"),
    );
    Ok(result)
}

/// Every file under `dir`, relative path to bytes (a `.git` of the rig's size:
/// a few dozen small files).
fn dir_manifest(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, rel: &str, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(root.join(rel)).expect("read_dir") {
            let entry = entry.expect("entry");
            let name = entry.file_name().to_string_lossy().to_string();
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
                    std::fs::read(root.join(&child)).unwrap_or_default(),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, "", &mut out);
    out
}

/// The strings of a JSON array, or empty.
fn strs(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// A refusal's gate and keys, or a panic naming what came instead.
fn refusal(outcome: Result<DeployProjectsResult, CoreError>, step: &str) -> (String, Vec<String>) {
    match outcome {
        Err(CoreError::DestructiveChangeRefused { gate, keys, .. }) => (gate, keys),
        Err(other) => panic!(
            "{step}: expected a destructive_change_refused, got {}: {other}",
            other.code()
        ),
        Ok(result) => panic!(
            "{step}: expected a refusal, the run answered outcome {}",
            result.outcome.as_str()
        ),
    }
}

/// The module's `commit` of `paths` (module resource names) through the script
/// function reached by reflection; the stdout, or the module's error text.
async fn module_commit(
    api: &ReqwestGatewayApi,
    config: &Config,
    paths: &[String],
    message: &str,
) -> Result<String, String> {
    let literal: Vec<String> = paths.iter().map(|p| format!("\"{p}\"")).collect();
    let code = format!(
        "{MODULE_PREAMBLE}print \"COMMIT %s\" % sm.commit(\"{APP}\", \"{APP_USER}\", [{}], \"{message}\")\n",
        literal.join(", ")
    );
    try_script(api, config, &code).await
}

/// The module's `push` of CapApp.
async fn module_push(api: &ReqwestGatewayApi, config: &Config) -> Result<String, String> {
    let code = format!("{MODULE_PREAMBLE}print \"PUSH %s\" % sm.push(\"{APP}\", \"{APP_USER}\")\n");
    try_script(api, config, &code).await
}

/// Push a release commit and its annotated tag `tag` to the rig remote's
/// `production` (a temp clone changes `code.py`); the new commit id.
fn push_release(rig: &Rig, tag: &str, body: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let work = dir.path().join("w");
    let remote = rig.remote.to_str().expect("utf8");
    git(
        dir.path(),
        &[
            "clone",
            "-q",
            "--no-hardlinks",
            "-b",
            "production",
            remote,
            work.to_str().expect("utf8"),
        ],
    );
    std::fs::write(work.join(CODE_PY), body).expect("code.py");
    git(&work, &["add", "-A"]);
    let ident = [
        "-c",
        "user.name=rig",
        "-c",
        "user.email=rig@example.invalid",
        "-c",
        "commit.gpgsign=false",
        "-c",
        "tag.gpgsign=false",
    ];
    let mut commit = ident.to_vec();
    commit.extend(["commit", "-q", "-m", tag]);
    git(&work, &commit);
    let mut tagging = ident.to_vec();
    tagging.extend(["tag", "-a", tag, "-m", tag]);
    git(&work, &tagging);
    git(
        &work,
        &[
            "push",
            "-q",
            remote,
            "HEAD:refs/heads/production",
            &format!("refs/tags/{tag}"),
        ],
    );
    // The git server container reads this tree as another user.
    let _ = Command::new("chmod")
        .args(["-R", "a+rX", remote])
        .status()
        .expect("chmod");
    git(&work, &["rev-parse", "HEAD"])
}

/// The rig driver's `reset27` under a `git.yaml` variant: CapApp re-cloned by
/// the module at v1.0.0, the remote's production at v1.1.0. The name guard of
/// [`rig`] has already run; the driver checks the URL again itself.
fn driver_reset(mode: &str) {
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/live-rig-deploy/capture27.sh");
    let status = Command::new("bash")
        .arg(&script)
        .arg("reset27")
        .env("GIT27_YAML_MODE", mode)
        .status()
        .expect("bash runs the rig driver");
    assert!(status.success(), "capture27.sh reset27 ({mode}) failed");
}

/// `git.yaml` in place: the template shape, with the production variant
/// switched on when `production` and exactly one import flag when `flag` names
/// it (`importImages`, `importTags`, `importThemes`, `tags_importOnStartup`).
fn write_git_yaml(rig: &Rig, production: bool, flag: Option<&str>) {
    let on = |name: &str| flag == Some(name);
    let mut text = format!(
        "- repo_uri: {GATEWAY_ORIGIN}\n\
         \x20 repo_branch: production\n\
         \x20 ignition_projectName: {APP}\n\
         \x20 ignition_userName: admin\n\
         \x20 ignition_inheritable: false\n\
         \x20 user_name: rig\n\
         \x20 user_email: rig@example.invalid\n\
         \x20 user_password: not-checked\n\
         \x20 commissioning_importThemes: {}\n\
         \x20 commissioning_importTags: {}\n\
         \x20 commissioning_importImages: {}\n\
         \x20 commissioning_enforceBranch: false\n\
         \x20 tags_importOnStartup: {}\n",
        on("importThemes"),
        on("importTags"),
        on("importImages"),
        on("tags_importOnStartup"),
    );
    if production {
        text.push_str(
            "  production_mode: true\n  production_branch: production\n  production_tagPattern: \"v*\"\n",
        );
    } else {
        text.push_str("  production_mode: false\n");
    }
    std::fs::write(rig.data.join("git.yaml"), text).expect("git.yaml write");
    eprintln!(
        "LIVE-YAML production={production} flag={} written (the credential key is not shown)",
        flag.unwrap_or("none")
    );
}

/// What a managed deploy leaves, for the evidence and the captures.
struct ManagedOutcome {
    evidence: serde_json::Value,
}

/// One managed deploy of `release` as the shipped path does it, with every
/// assertion of 27-23 Task 1: `--check` (state managed, drift unknown, `.git`
/// untouched), the apply with `--artifact-sha256` (imported, `post_status`
/// matched, the dropped member), then JGit's status against
/// `data.git.expected_drift` as sets, 0 unpushed, and the branch.
async fn managed_deploy(
    rig: &Rig,
    api: &ReqwestGatewayApi,
    label: &str,
    release: &str,
    first: bool,
    branch: &str,
) -> ManagedOutcome {
    let (_work, config) = script_config(api, &rig.url).await;
    let gitdir = rig.project_dir().join(".git");

    // --check: reads only.
    let manifest_before = dir_manifest(&gitdir);
    let flags = DeployFlags {
        first_change: first,
        ..DeployFlags::default()
    };
    let check = deploy_in_process(rig, true, ENV, release, &flags, &format!("{label}-check"))
        .await
        .unwrap_or_else(|err| panic!("{label} check: {} ({err})", err.code()));
    let check_json = serde_json::to_value(&check).expect("check json");
    let git_node = &check_json["git"]["nodes"][0];
    assert_eq!(
        git_node["state"], "managed",
        "{label}: the module's repo is managed: {git_node}"
    );
    assert!(
        check_json["git"]["expected_drift"].is_null()
            && check_json["git"]["expected_drift_count"].is_null()
            && check_json["git"]["expected_drift_complete"] == false,
        "{label}: --check must report the drift as unknown (W-6): {}",
        check_json["git"]
    );
    eprintln!(
        "LIVE-C4 {label} check: state=managed expected_drift=null count=null complete=false action={} dirty_count={} dirty_source={}",
        check_json["action"], git_node["dirty_count"], git_node["dirty_source"]
    );
    let manifest_after = dir_manifest(&gitdir);
    let changed: Vec<&String> = manifest_before
        .iter()
        .filter(|(path, bytes)| manifest_after.get(*path) != Some(*bytes))
        .map(|(path, _)| path)
        .chain(
            manifest_after
                .keys()
                .filter(|path| !manifest_before.contains_key(*path)),
        )
        .collect();
    assert!(
        changed.is_empty(),
        "{label}: --check changed the gateway's .git: {changed:?}"
    );
    eprintln!(
        "LIVE-C4 {label} check_left_git_manifest_unchanged files={}",
        manifest_before.len()
    );

    // The apply, with the digest the check showed.
    let flags = DeployFlags {
        artifact_sha256: Some(check.artifact_sha256.clone()),
        first_change: first,
        ..DeployFlags::default()
    };
    let started = Instant::now();
    let applied = deploy_in_process(rig, false, ENV, release, &flags, label)
        .await
        .unwrap_or_else(|err| panic!("{label} apply: {} ({err})", err.code()));
    let applied_ms = started.elapsed().as_millis();
    let applied_json = serde_json::to_value(&applied).expect("apply json");
    assert_eq!(
        applied.outcome.as_str(),
        "imported",
        "{label}: outcome (slug {}): {}",
        applied_json["slug"],
        applied_json["git"]
    );
    let node = &applied_json["git"]["nodes"][0];
    let restore = &node["restore"];
    eprintln!(
        "LIVE-C4 {label} apply: outcome=imported ms={applied_ms} restored={} advanced={} verified={} post_status={} dropped={} reason={}",
        restore["restored"],
        restore["advanced"],
        restore["verified"],
        restore["post_status"],
        restore["dropped"],
        restore["reason"]
    );
    assert!(
        restore["restored"] == true && restore["advanced"] == true && restore["verified"] == true,
        "{label}: the handover did not complete: {restore}"
    );
    assert_eq!(
        restore["post_status"], "matched",
        "{label}: the master is a local node, so the run's own post-advance disk status ran (D-25): {restore}"
    );
    let dropped = strs(&restore["dropped"]);
    assert_eq!(applied_json["git"]["expected_drift_complete"], true);
    let predicted = strs(&applied_json["git"]["expected_drift"]);
    assert_eq!(
        predicted.len() as u64,
        applied_json["git"]["expected_drift_count"]
            .as_u64()
            .unwrap_or(u64::MAX),
        "{label}: the drift list is not capped here"
    );

    // JGit inside the module says what the run predicted.
    tokio::time::sleep(Duration::from_secs(10)).await;
    let jg = jgit(api, &config).await;
    let expected: BTreeSet<String> = predicted.iter().map(|file| module_path(file)).collect();
    let extra: Vec<&String> = jg.paths.difference(&expected).collect();
    let missing: Vec<&String> = expected.difference(&jg.paths).collect();
    eprintln!(
        "LIVE-C4 {label} jgit: uncommitted={} expected={} extra={} missing={} unpushed={} branch={}",
        jg.paths.len(),
        expected.len(),
        extra.len(),
        missing.len(),
        jg.unpushed,
        jg.branch
    );
    for row in &jg.rows {
        eprintln!("LIVE-C4 {label} jgit-row {row}");
    }
    for path in &predicted {
        eprintln!("LIVE-C4 {label} drift-path {path}");
    }
    assert!(
        extra.is_empty() && missing.is_empty(),
        "{label}: CUT-OVER BLOCKER (D-25): JGit differs from expected_drift: extra={extra:?} missing={missing:?}"
    );
    assert_eq!(jg.unpushed, 0, "{label}: unpushed commits after the deploy");
    assert_eq!(jg.branch, branch, "{label}: current branch");
    assert!(
        !jg.paths.contains(NOTES) && jg.rows.iter().all(|row| !row.ends_with(NOTES)),
        "{label}: the dropped member must be in no JGit list"
    );
    assert!(
        jg.rows.iter().all(|row| !row.starts_with("Deleted ")),
        "{label}: JGit lists a path as deleted: {:?}",
        jg.rows
    );
    eprintln!(
        "LIVE-CHECK {label} jgit_equals_expected_drift files={}",
        predicted.len()
    );
    assert!(
        dropped.iter().any(|path| path == NOTES),
        "{label}: restore.dropped must name the unlisted member {NOTES} (checker B3): {dropped:?}"
    );

    // The save is the run's own area; the gate leaves the rig tidy for the
    // next run.
    let evidence = serde_json::json!({
        "label": label,
        "release": release,
        "outcome": applied.outcome.as_str(),
        "check_expected_drift_null": check_json["git"]["expected_drift"].is_null(),
        "post_status": restore["post_status"],
        "dropped": dropped,
        "expected_drift": predicted,
        "jgit_paths": jg.paths,
        "jgit_rows": jg.rows,
        "unpushed": jg.unpushed,
        "branch": jg.branch,
        "apply_ms": applied_ms,
        "git_manifest_files": manifest_before.len(),
    });
    write_evidence(rig, &format!("{label}-summary"), &evidence);
    ManagedOutcome { evidence }
}

/// Commit the drift through the module and push (production mode): the push
/// must succeed and the remote tip's tree must still hold everything outside
/// the artifact scope (R-8).
async fn commit_drift_and_push(
    rig: &Rig,
    api: &ReqwestGatewayApi,
    label: &str,
) -> serde_json::Value {
    let (_work, config) = script_config(api, &rig.url).await;
    let before = jgit(api, &config).await;
    let paths: Vec<String> = before.paths.iter().cloned().collect();
    let remote_before = git(&rig.remote, &["rev-parse", "refs/heads/production"]);
    let committed = module_commit(api, &config, &paths, "normalize gateway form").await;
    eprintln!("LIVE-COMMIT {label} {committed:?}");
    committed.expect("the module's commit answers");
    let after_commit = jgit(api, &config).await;
    eprintln!(
        "LIVE-C4 {label} after_commit: uncommitted={} unpushed={}",
        after_commit.paths.len(),
        after_commit.unpushed
    );
    assert!(
        after_commit.paths.is_empty(),
        "{label}: the status is not clean after committing the drift: {:?}",
        after_commit.rows
    );
    assert_eq!(after_commit.unpushed, 1, "{label}: one commit to push");
    let pushed = module_push(api, &config).await;
    eprintln!("LIVE-PUSH {label} {pushed:?}");
    pushed.expect("the module's push in production mode succeeds once the status is clean");
    tokio::time::sleep(Duration::from_secs(3)).await;
    let remote_after = git(&rig.remote, &["rev-parse", "refs/heads/production"]);
    let local_head = git(&rig.project_dir(), &["rev-parse", "HEAD"]);
    assert_ne!(
        remote_after, remote_before,
        "{label}: the remote tip did not move"
    );
    assert_eq!(
        remote_after, local_head,
        "{label}: the remote tip is not the pushed HEAD"
    );
    let tree = git(
        &rig.remote,
        &["ls-tree", "-r", "--name-only", "refs/heads/production"],
    );
    let outside = [
        "README.md",
        "tags/default/tags.json",
        "images/logo.txt",
        "deploy/project.yaml",
        ".gitignore",
    ];
    let missing: Vec<&str> = outside
        .iter()
        .copied()
        .filter(|p| !tree.lines().any(|l| l == *p))
        .collect();
    eprintln!(
        "LIVE-CHECK {label} push_succeeds_and_remote_tree_keeps_out_of_scope_files kept={} missing={missing:?} total_files={}",
        outside.len() - missing.len(),
        tree.lines().count()
    );
    assert!(
        missing.is_empty(),
        "{label}: the pushed tree lost files: {missing:?}"
    );
    let final_status = jgit(api, &config).await;
    assert!(final_status.paths.is_empty());
    assert_eq!(final_status.unpushed, 0);
    serde_json::json!({
        "label": label,
        "committed_paths": paths,
        "remote_moved": true,
        "remote_is_local_head": true,
        "kept_out_of_scope_files": outside.len() - missing.len(),
        "final_uncommitted": final_status.paths.len(),
        "final_unpushed": final_status.unpushed,
    })
}

/// 27-23 Task 1, template variant: a managed project at v1.0.0 is deployed to
/// v1.1.0; then, under the production variant, a v1.2.0 pushed to the remote's
/// `production` is deployed and the module commits the drift and pushes.
#[tokio::test]
#[ignore = "needs the git-module rig: IGNITION_LIVE_GIT=1"]
async fn git_deploy_managed_e2e() {
    let Some(rig) = rig().await else { return };
    eprintln!("LIVE-RAN git_deploy_managed_e2e");
    let api = rig.api();
    let host = HostGit::new();

    let (state, refs) = gateway_state(&rig, &host).await;
    assert_eq!(state.kind, GitStateKind::Managed, "{state:?}");
    let (_keep, _checkout, c1) = release_checkout(&rig, "v1.0.0");
    assert!(
        refs.heads.values().any(|id| id == &c1),
        "the module's clone must sit at v1.0.0 (run reset27 first)"
    );

    let template = managed_deploy(
        &rig,
        &api,
        "c4-template-v1.1.0",
        "v1.1.0",
        true,
        "production",
    )
    .await;

    // Production variant over the same gateway: the module restarts with
    // production_mode on, a v1.2.0 reaches the remote, the run deploys it.
    write_production_yaml(&rig);
    restart_gateway(&api).await;
    let tip = push_release(&rig, "v1.2.0", "def f():\n    return 4\n");
    eprintln!("LIVE-C4 production pushed v1.2.0 tip={}", &tip[..7]);
    let production = managed_deploy(
        &rig,
        &api,
        "c4-production-v1.2.0",
        "v1.2.0",
        false,
        "production",
    )
    .await;
    let pushed = commit_drift_and_push(&rig, &api, "c4-production-v1.2.0").await;

    write_evidence(
        &rig,
        "git_deploy_managed_e2e",
        &serde_json::json!({
            "template": template.evidence,
            "production": production.evidence,
            "production_push": pushed,
        }),
    );
}

/// 27-23 Task 1, D-17: the managed deploy under each `git.yaml` variant, each
/// from a freshly reset rig (`reset27` under that variant), with the same
/// assertions.
#[tokio::test]
#[ignore = "needs the git-module rig: IGNITION_LIVE_GIT=1"]
async fn git_variants_e2e() {
    let Some(rig) = rig().await else { return };
    eprintln!("LIVE-RAN git_variants_e2e");
    let api = rig.api();
    let mut recorded = Vec::new();
    for (mode, label) in [
        ("template", "c4-variant-template-v1.1.0"),
        ("production", "c4-variant-production-v1.1.0"),
    ] {
        driver_reset(mode);
        let outcome = managed_deploy(&rig, &api, label, "v1.1.0", true, "production").await;
        recorded.push(outcome.evidence);
    }
    write_evidence(
        &rig,
        "git_variants_e2e",
        &serde_json::json!({ "variants": recorded }),
    );
}

/// Make the gateway's live project differ from its repository the way a
/// Designer save does: the file is written on disk and the gateway scans.
async fn live_edit(rig: &Rig, api: &ReqwestGatewayApi, rel: &str, body: &str) {
    std::fs::write(rig.project_dir().join(rel), body).expect("live edit write");
    scan_and_wait(
        api,
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
}

/// Restores the template `git.yaml` when the gate ends, even by a panic.
struct YamlGuard<'a>(&'a Rig);

impl Drop for YamlGuard<'_> {
    fn drop(&mut self) {
        write_git_yaml(self.0, false, None);
    }
}

/// 27-23 Task 2: the live refusals match the engine (D-01, D-21), and an
/// override over a never-pushed Designer-style commit lists what it would
/// discard from the gateway's own objects (D-24).
#[tokio::test]
#[ignore = "needs the git-module rig: IGNITION_LIVE_GIT=1"]
async fn git_refusals_live() {
    let Some(rig) = rig().await else { return };
    eprintln!("LIVE-RAN git_refusals_live");
    let api = rig.api();
    let host = HostGit::new();
    let (state, refs) = gateway_state(&rig, &host).await;
    assert_eq!(state.kind, GitStateKind::Managed, "{state:?}");
    let (_keep, _checkout, c1) = release_checkout(&rig, "v1.0.0");
    assert!(
        refs.heads.values().any(|id| id == &c1),
        "the module's clone must sit at v1.0.0 (run reset27 first)"
    );
    let (_work, config) = script_config(&api, &rig.url).await;
    let mut seen = Vec::new();

    // Baseline: a clean pushed repository behind the release is not a refusal.
    let first = DeployFlags {
        first_change: true,
        ..DeployFlags::default()
    };
    let baseline = deploy_in_process(&rig, true, ENV, "v1.1.0", &first, "c4-refusals-baseline")
        .await
        .unwrap_or_else(|err| panic!("baseline check: {} ({err})", err.code()));
    let baseline_json = serde_json::to_value(&baseline).expect("json");
    eprintln!(
        "LIVE-C4 refusals baseline: action={} dirty_count={} dirty_source={}",
        baseline_json["action"],
        baseline_json["git"]["nodes"][0]["dirty_count"],
        baseline_json["git"]["nodes"][0]["dirty_source"]
    );
    assert_eq!(baseline_json["git"]["nodes"][0]["dirty_count"], 0);

    // git_sync_hazard (D-21): importTags with a tags/ directory present
    // refuses; importImages is advisory (G3 recorded no wipe, D-G3).
    let yaml_guard = YamlGuard(&rig);
    write_git_yaml(&rig, false, Some("importTags"));
    let (gate, keys) = refusal(
        deploy_in_process(&rig, true, ENV, "v1.1.0", &first, "c4-refusals-hazard").await,
        "importTags hazard",
    );
    eprintln!("LIVE-REFUSAL git_sync_hazard gate={gate} keys={keys:?}");
    assert_eq!(gate, "git_sync_hazard");
    assert!(keys.iter().any(|k| k == "import_tags"), "{keys:?}");
    seen.push(format!("{gate}:{}", keys.join(",")));
    write_git_yaml(&rig, false, Some("importImages"));
    let advisory = deploy_in_process(&rig, true, ENV, "v1.1.0", &first, "c4-refusals-images")
        .await
        .unwrap_or_else(|err| panic!("importImages must not refuse: {} ({err})", err.code()));
    let warnings = serde_json::to_value(&advisory).expect("json")["warnings"].clone();
    eprintln!("LIVE-C4 refusals importImages is advisory: warnings={warnings}");
    assert!(
        strs(&warnings)
            .iter()
            .any(|w| w.to_ascii_lowercase().contains("image")),
        "the importImages hazard is a warning: {warnings}"
    );
    drop(yaml_guard);

    // git_dirty: a live change that is not committed.
    let edited = "def f():\n    return 11\n";
    live_edit(&rig, &api, CODE_PY, edited).await;
    let (gate, keys) = refusal(
        deploy_in_process(&rig, true, ENV, "v1.1.0", &first, "c4-refusals-dirty").await,
        "git_dirty",
    );
    eprintln!("LIVE-REFUSAL git_dirty gate={gate} keys={keys:?}");
    assert_eq!(gate, "git_dirty");
    assert!(keys.iter().any(|k| k == CODE_PY), "{keys:?}");
    seen.push(format!("{gate}:{}", keys.join(",")));

    // git_unpushed: the module commits the edit and does not push.
    let committed = module_commit(
        &api,
        &config,
        &["ignition/script-python/cap/util".to_string()],
        "designer break fix",
    )
    .await;
    eprintln!("LIVE-COMMIT break-fix {committed:?}");
    committed.expect("the module's commit answers");
    let head = git(&rig.project_dir(), &["rev-parse", "HEAD"]);
    let (gate, keys) = refusal(
        deploy_in_process(&rig, true, ENV, "v1.1.0", &first, "c4-refusals-unpushed").await,
        "git_unpushed",
    );
    eprintln!("LIVE-REFUSAL git_unpushed gate={gate} keys={keys:?}");
    assert_eq!(gate, "git_unpushed");
    assert!(
        keys.iter()
            .any(|k| head.starts_with(k.as_str()) || k.starts_with(&head[..7])),
        "the refusal names the unpushed commit {}: {keys:?}",
        &head[..7]
    );
    seen.push(format!("{gate}:{}", &head[..7]));

    // D-24: with the override over the unpushed commit and a second live
    // change after it, --check lists the dirty paths read from the gateway's
    // own objects (the commit is on no remote).
    live_edit(&rig, &api, CODE_PY, "def f():\n    return 12\n").await;
    let over = DeployFlags {
        discard_gateway_changes: Some(REASON.to_string()),
        ..first.clone()
    };
    let would = deploy_in_process(&rig, true, ENV, "v1.1.0", &over, "c4-refusals-override")
        .await
        .unwrap_or_else(|err| panic!("override check: {} ({err})", err.code()));
    let would_json = serde_json::to_value(&would).expect("json");
    let wd = &would_json["git"]["would_discard"];
    let node = &would_json["git"]["nodes"][0];
    eprintln!(
        "LIVE-C4 refusals D-24: would_discard.dirty={} dirty_source={} paths={} commits={}",
        wd["dirty"], node["dirty_source"], wd["paths"], wd["commits"]
    );
    assert_eq!(wd["dirty"], "listed", "{wd}");
    assert_eq!(node["dirty_source"], "gateway_objects", "{node}");
    assert!(
        strs(&wd["paths"]).iter().any(|p| p == CODE_PY),
        "the live change's path is listed: {wd}"
    );
    assert!(
        strs(&wd["commits"])
            .iter()
            .any(|c| head.starts_with(c.as_str()) || c.starts_with(&head[..7])),
        "the unpushed commit is listed: {wd}"
    );

    // git_not_merged: the commit reaches a branch the release lacks.
    let dir = tempfile::tempdir().expect("tempdir");
    let work = dir.path().join("w");
    let remote = rig.remote.to_str().expect("utf8");
    git(
        dir.path(),
        &[
            "clone",
            "-q",
            "--no-hardlinks",
            remote,
            work.to_str().expect("utf8"),
        ],
    );
    git(
        &work,
        &[
            "-c",
            "safe.directory=*",
            "fetch",
            "-q",
            rig.project_dir().to_str().expect("utf8"),
            "refs/heads/production:refs/heads/gateway-break-fix",
        ],
    );
    git(
        &work,
        &[
            "push",
            "-q",
            remote,
            "refs/heads/gateway-break-fix:refs/heads/hotfix",
        ],
    );
    let _ = Command::new("chmod")
        .args(["-R", "a+rX", remote])
        .status()
        .expect("chmod");
    let (gate, keys) = refusal(
        deploy_in_process(&rig, true, ENV, "v1.1.0", &first, "c4-refusals-not-merged").await,
        "git_not_merged",
    );
    eprintln!("LIVE-REFUSAL git_not_merged gate={gate} keys={keys:?}");
    assert_eq!(gate, "git_not_merged");
    assert!(
        keys.iter()
            .any(|k| head.starts_with(k.as_str()) || k.starts_with(&head[..7])),
        "the refusal names the commit the release lacks: {keys:?}"
    );
    seen.push(format!("{gate}:{}", &head[..7]));

    write_evidence(
        &rig,
        "git_refusals_live",
        &serde_json::json!({
            "refusals": seen,
            "d24": {
                "would_discard": wd,
                "dirty_source": node["dirty_source"],
            },
        }),
    );
}
