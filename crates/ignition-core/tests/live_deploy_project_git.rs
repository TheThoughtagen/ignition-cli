//! Phase 27's live gates on a git-module rig (27-03, the phase tracer's last
//! step): the keep-and-advance handover of 27-01/27-02 runs against JGit inside
//! the git module on an Ignition 8.3.6 gateway. The rigs default to module 2.3.5
//! (27-61); 2.3.4 is for `git_pair_detect_live` only. Nothing here is mocked.
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
//! Every gate prints `LIVE-RAN <name>` on stderr as its LAST statement, after
//! every assertion: the line means the gate passed (a gate that skips on its
//! guard, or panics, prints nothing). The rig drivers accept only that line at
//! the end of a log line, for the exact name.
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
    ProjectGateway, ProjectsEvent, ProjectsInput, ProjectsObserver, ProjectsRunOptions,
    ProjectsSurfaces, TAG_VERIFY_POLL_S, TAG_VERIFY_SETTLE_S, TagDeleteSurface,
    deploy_projects_prepare, deploy_projects_run, load_project,
};
use ignition_core::actions::script::script_run;
use ignition_core::actions::webdev::webdev_deploy_with;
use ignition_core::client::deploy_api::scan_and_wait;
use ignition_core::client::query::ListQuery;
use ignition_core::client::scan::ScanKind;
use ignition_core::client::{DeployApi, GatewayApi, ReqwestGatewayApi};
use ignition_core::config::{self, Config, Credential, Secret};
use ignition_core::deploy::apply::preflight::DesignersPolicy;
use ignition_core::deploy::git_advance::{AdvancePrep, GitWrite, prepare_advance};
use ignition_core::deploy::git_handover::{GitSaveReport, restore_and_advance, save};
use ignition_core::deploy::git_remote::HostGit;
use ignition_core::deploy::git_state::{GitHead, GitRefs, GitState, GitStateKind, classify};
use ignition_core::deploy::ledger::{LedgerRoot, git_save_rel};
use ignition_core::deploy::pair::{BACKUP_POLL_S, BackupNode, BackupPolicy};
use ignition_core::deploy::project_artifact::{Artifact, build_artifact, canonical_digest};
use ignition_core::deploy::tag_converge::BackupTags;
use ignition_core::deploy::transport::local::LocalFsTransport;
use ignition_core::deploy::transport::projects::{
    GitEntry, GitHostRead, GitManifest, GitStateWriter,
};
use ignition_core::deploy::transport::{self, projects::project_host_for_node};
use ignition_core::error::CoreError;
use ignition_core::module::GIT_MODULE;
use ignition_core::poll::PollConfig;

mod common;
use common::prod_shape_git::{
    self as prod_shape_git, HOST_LINUX_RESTORE_VERIFY, HOST_LINUX_SAVE, HOST_LINUX_UPLOAD,
};

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

/// The variables that choose which repository a git child acts on. A caller's
/// environment must never steer a test's git commands at another repository
/// (T-27-199, the W1 pattern of `git_remote::SCRUBBED_ENV`).
const REPOSITORY_SELECTION_ENV: [&str; 7] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_NAMESPACE",
    "GIT_COMMON_DIR",
];

/// A host-git command in `dir` with no user or system config and no
/// repository-selection variable. A caller that needs one of them (an index
/// file) sets it on the returned command, after this clears them.
fn git_command(dir: &Path, args: &[&str]) -> Command {
    git_command_with(dir, args, &[])
}

/// [`git_command`] with `injected` variables set before the repository
/// selection is cleared. Only a test passes any: it proves the clearing wins.
fn git_command_with(dir: &Path, args: &[&str], injected: &[(&str, &str)]) -> Command {
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1");
    for (key, value) in injected {
        command.env(key, value);
    }
    for var in REPOSITORY_SELECTION_ENV {
        command.env_remove(var);
    }
    command
}

/// Run host git in `dir` with no user or system config; trimmed stdout.
fn git(dir: &Path, args: &[&str]) -> String {
    let out = git_command(dir, args).output().expect("git runs");
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
    /// The gateway name the target file declares (`ign-git27-rig`, or the pair
    /// rig's master).
    gateway: String,
    /// The backup node of the pair rig (27-25); `None` on the single-node rig.
    backup: Option<PairBackup>,
}

/// The backup node of the pair rig: its loopback URL, API token and the host
/// directory bound onto its `data/projects`.
struct PairBackup {
    url: String,
    token: String,
    data: PathBuf,
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
        gateway: RIG_GATEWAY_NAME.to_string(),
        backup: None,
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
    node_state(&rig.node(), host).await
}

/// [`gateway_state`] over any node's data directory (the pair rig's backup).
async fn node_state(node: &LocalFsTransport, host: &HostGit) -> (GitState, GitRefs) {
    let files = node.read_git_state(APP).await.expect("host read of .git");
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
        head_at(&refs, &c1),
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
        head_at(&refs, &c1),
        "the module's clone must sit at v1.0.0 (run reset27 first)"
    );

    let artifact = build_artifact(&checkout).expect("artifact builds");
    let members: BTreeSet<String> = artifact.digest.members.keys().cloned().collect();
    let prep: AdvancePrep = prepare_advance(&host, &checkout, &state, &refs, &c2, &members)
        .await
        .expect("prepare_advance");
    let saved = save(&node, &node, LedgerRoot::DataDir, GATEWAY, APP, RUN)
        .await
        .expect("save");
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
        head_at(&refs, &advanced.c2),
        "the branch tip is the release commit"
    );
    assert!(
        !head_at(&refs, &advanced.c1),
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
    eprintln!("LIVE-RAN git_advance_status");
}

/// G9: the advanced `.git` survives a restart, a project scan and a status
/// read. What a second overwrite import does to it is recorded, not asserted
/// (`second_import_git_deleted`): R-7 expected a deletion, the rig showed the
/// `.git` kept (decision D-G9), and the handover covers both outcomes.
#[tokio::test]
#[ignore = "needs the git-module rig: IGNITION_LIVE_GIT=1"]
async fn git_restore_survival() {
    let Some(rig) = rig().await else { return };
    let api = rig.api();
    let host = HostGit::new();
    let (_keep, checkout, c2) = release_checkout(&rig, "v1.1.0");

    let (state, refs) = gateway_state(&rig, &host).await;
    assert!(
        state.kind == GitStateKind::Managed && head_at(&refs, &c2),
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
        head_at(&refs_after, &c2),
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
    eprintln!("LIVE-RAN git_restore_survival");
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
    let mut command = git_command(dir, args);
    command
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
    eprintln!("LIVE-RAN git_state_probe");
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
    eprintln!("LIVE-RAN git_pull_push_roundtrip");
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
    /// The reviewed `tags_sha256` of a project that owns tag providers.
    tags_sha256: Option<String>,
    /// A run that creates tags accepts that a rollback leaves them behind (the
    /// rig has no tag-delete route).
    allow_manual_undo: bool,
}

/// What an in-process run may add to the shipped wiring: an observer that
/// hears every event, and a git writer that stands in for the one the run
/// builds from the master (a timing wrapper over the same local transport).
#[derive(Default)]
struct Extras<'a> {
    observer: Option<&'a dyn ProjectsObserver>,
    git_writer: Option<&'a dyn GitStateWriter>,
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
    let backup_block = rig
        .backup
        .as_ref()
        .map(|backup| {
            format!(
                "  backup:\n    profile: pair-b\n    local:\n      data_dir: '{}'\n",
                backup.data.display()
            )
        })
        .unwrap_or_default();
    std::fs::write(
        targets.join("targets").join(format!("{ENV}.yaml")),
        format!(
            "schema_version: 1\ngateway: {gateway}\nmode: production\nnodes:\n  master:\n    \
             profile: rig\n    gateway_name: {gateway}\n    local:\n      data_dir: '{}'\n{backup_block}",
            rig.data.display(),
            gateway = rig.gateway,
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
    deploy_in_process_with(rig, check, env, release, flags, step, &Extras::default()).await
}

/// [`deploy_in_process`] with an observer and a git writer (27-24).
async fn deploy_in_process_with(
    rig: &Rig,
    check: bool,
    env: &str,
    release: &str,
    flags: &DeployFlags,
    step: &str,
    extras: &Extras<'_>,
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
        allow_manual_undo: flags.allow_manual_undo,
        artifact_sha256: flags.artifact_sha256.clone(),
        tags_sha256: flags.tags_sha256.clone(),
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
    // The backup of the pair rig (27-25), read only exactly as the CLI builds
    // it: its own client, no file transport (the target's `local` block is read
    // by the run's own host builders).
    let backup_api = rig.backup.as_ref().map(|backup| {
        ReqwestGatewayApi::for_tests(
            &backup.url,
            Some(Credential::Token(Secret::new(backup.token.clone()))),
        )
    });
    let backup_node = backup_api
        .as_ref()
        .map(|api| BackupNode::new(api, api, None));
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
        backup: backup_node.as_ref().map(|node| node as &dyn BackupTags),
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
        git_writer: extras.git_writer,
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
            extras.observer.unwrap_or(&NoProjectsObserver),
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
    /// The canonical digest of the release's artifact.
    artifact_sha256: String,
    /// What JGit lists as uncommitted after the deploy.
    jgit_paths: BTreeSet<String>,
}

/// What `managed_deploy` expects of the gateway's repository and of the run.
#[derive(Clone, Copy)]
struct Expect {
    /// The class `--check` reads: `managed`, or `adoptable` for the partial
    /// repo `setupLocalRepo` leaves (27-17).
    state: &'static str,
    /// The run's outcome word.
    outcome: &'static str,
    /// The project owns tag providers: the apply carries `--tags-sha256`.
    tags: bool,
}

impl Expect {
    /// A module-managed repo and a plain project import.
    const MANAGED: Expect = Expect {
        state: "managed",
        outcome: "imported",
        tags: false,
    };
    /// The partial repo `setupLocalRepo` leaves: adopted by the run.
    const ADOPTABLE: Expect = Expect {
        state: "adoptable",
        outcome: "imported",
        tags: false,
    };
    /// A managed repo and a project that owns tag providers (Phase 26).
    const TAGS: Expect = Expect {
        state: "managed",
        outcome: "applied",
        tags: true,
    };
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
    expect: Expect,
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
        git_node["state"], expect.state,
        "{label}: the module's repo is {}: {git_node}",
        expect.state
    );
    assert!(
        check_json["git"]["expected_drift"].is_null()
            && check_json["git"]["expected_drift_count"].is_null()
            && check_json["git"]["expected_drift_complete"] == false,
        "{label}: --check must report the drift as unknown (W-6): {}",
        check_json["git"]
    );
    eprintln!(
        "LIVE-C4 {label} check: state={} expected_drift=null count=null complete=false action={} dirty_count={} dirty_source={}",
        expect.state, check_json["action"], git_node["dirty_count"], git_node["dirty_source"]
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
    let tags_sha256 = if expect.tags {
        Some(
            check
                .tags
                .as_ref()
                .unwrap_or_else(|| panic!("{label}: the check reports no tags section"))
                .tags_sha256
                .clone(),
        )
    } else {
        None
    };
    let flags = DeployFlags {
        artifact_sha256: Some(check.artifact_sha256.clone()),
        first_change: first,
        tags_sha256,
        allow_manual_undo: expect.tags,
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
        expect.outcome,
        "{label}: outcome (slug {}): {}",
        applied_json["slug"],
        applied_json["git"]
    );
    let node = &applied_json["git"]["nodes"][0];
    let restore = &node["restore"];
    if expect.state == "adoptable" {
        assert_eq!(node["adopted"], true, "{label}: the run adopted the repo");
        assert_eq!(
            node["branch_source"], "git_yaml",
            "{label}: the branch comes from git.yaml"
        );
        eprintln!(
            "LIVE-C4 {label} adoption: adopted=true branch={} branch_source=git_yaml remote={}",
            node["branch"], node["remote"]
        );
    }
    eprintln!(
        "LIVE-C4 {label} apply: outcome={} ms={applied_ms} restored={} advanced={} verified={} post_status={} dropped={} reason={}",
        expect.outcome,
        restore["restored"],
        restore["advanced"],
        restore["verified"],
        restore["post_status"],
        restore["dropped"],
        restore["reason"]
    );
    // The import left the partial repo in place (observed live in 27-24), so
    // there was nothing to put back and `restored` is false for an adoption;
    // the advance and its read-back are what the adoption must show.
    assert!(
        (restore["restored"] == true || expect.state == "adoptable")
            && restore["advanced"] == true
            && restore["verified"] == true,
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
    ManagedOutcome {
        evidence,
        artifact_sha256: check.artifact_sha256.clone(),
        jgit_paths: jg.paths,
    }
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
    let api = rig.api();
    let host = HostGit::new();

    let (state, refs) = gateway_state(&rig, &host).await;
    assert_eq!(state.kind, GitStateKind::Managed, "{state:?}");
    let (_keep, _checkout, c1) = release_checkout(&rig, "v1.0.0");
    assert!(
        head_at(&refs, &c1),
        "the module's clone must sit at v1.0.0 (run reset27 first)"
    );

    let template = managed_deploy(
        &rig,
        &api,
        "c4-template-v1.1.0",
        "v1.1.0",
        true,
        "production",
        Expect::MANAGED,
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
        Expect::MANAGED,
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
    eprintln!("LIVE-RAN git_deploy_managed_e2e");
}

/// 27-23 Task 1, D-17: the managed deploy under each `git.yaml` variant, each
/// from a freshly reset rig (`reset27` under that variant), with the same
/// assertions.
#[tokio::test]
#[ignore = "needs the git-module rig: IGNITION_LIVE_GIT=1"]
async fn git_variants_e2e() {
    let Some(rig) = rig().await else { return };
    let api = rig.api();
    let mut recorded = Vec::new();
    for (mode, label) in [
        ("template", "c4-variant-template-v1.1.0"),
        ("production", "c4-variant-production-v1.1.0"),
    ] {
        driver_reset(mode);
        let outcome = managed_deploy(
            &rig,
            &api,
            label,
            "v1.1.0",
            true,
            "production",
            Expect::MANAGED,
        )
        .await;
        recorded.push(outcome.evidence);
    }
    write_evidence(
        &rig,
        "git_variants_e2e",
        &serde_json::json!({ "variants": recorded }),
    );
    eprintln!("LIVE-RAN git_variants_e2e");
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
    let api = rig.api();
    let host = HostGit::new();
    let (state, refs) = gateway_state(&rig, &host).await;
    assert_eq!(state.kind, GitStateKind::Managed, "{state:?}");
    let (_keep, _checkout, c1) = release_checkout(&rig, "v1.0.0");
    assert!(
        head_at(&refs, &c1),
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
    eprintln!("LIVE-RAN git_refusals_live");
}

// ------------------------------------------------------------------- 27-24
//
// C4 on the single-node rig, second half: the module's own `setupLocalRepo`
// and the adoption that follows, a restart and a pull, production-shape timing,
// an owned tag provider, and the evidence the one human Designer open is
// checked against (D-15). A JGit-versus-`expected_drift` difference is a
// cut-over blocker (D-25): no assertion here is loosened.

/// One step of the rig driver (`capture27.sh SUB ...`); its stdout. The driver
/// checks the loopback URL itself and never prints a token or the git.yaml
/// credential.
fn driver(args: &[&str]) -> String {
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/live-rig-deploy/capture27.sh");
    let out = Command::new("bash")
        .arg(&script)
        .args(args)
        .output()
        .expect("bash runs the rig driver");
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let tail: String = err
            .chars()
            .rev()
            .take(400)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        panic!(
            "capture27.sh {args:?} failed ({}): {}",
            out.status,
            ignition_core::deploy::git_state::redact_text(&tail)
        );
    }
    eprintln!("LIVE-DRIVER {} ok", args.join(" "));
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// The commit CapApp's HEAD names, as ign's own host read shows it.
async fn gateway_head(rig: &Rig) -> String {
    let host = HostGit::new();
    let (_state, refs) = gateway_state(rig, &host).await;
    match refs.head {
        GitHead::Branch {
            commit: Some(commit),
            ..
        }
        | GitHead::Detached { commit } => commit,
        other => panic!("CapApp's HEAD names no commit: {other:?}"),
    }
}

/// Paths JGit lists that the run did not predict, and the other way round.
fn drift_difference(jg: &Jgit, predicted: &[String]) -> (Vec<String>, Vec<String>) {
    let expected: BTreeSet<String> = predicted.iter().map(|file| module_path(file)).collect();
    (
        jg.paths.difference(&expected).cloned().collect(),
        expected.difference(&jg.paths).cloned().collect(),
    )
}

/// 27-42 (T-27-188): the files whose bytes on the rig's project directory
/// differ from the release's, which is what JGit lists once its index holds
/// the release. The export is not read: it renders the gateway's form for
/// files the import left alone (the premise the 2026-11-18 run falsified).
///
/// Paths are relative to the project, `/`-separated, the form of an artifact
/// member (the caller maps them with [`module_path`] for JGit's list and
/// counts the files for the run's `expected_drift_count`). Only regular files
/// are compared; no link is followed; the project's own `.git` is skipped.
/// A release member that is not a regular file on the disk is a deletion and
/// is drift, so a deletion still fails the gate.
fn disk_drift(project_dir: &Path, release: &BTreeMap<String, Vec<u8>>) -> BTreeSet<String> {
    fn walk(root: &Path, rel: &str, out: &mut BTreeMap<String, PathBuf>) {
        let here = if rel.is_empty() {
            root.to_path_buf()
        } else {
            root.join(rel)
        };
        for entry in std::fs::read_dir(&here).expect("read_dir") {
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
            // `DirEntry::file_type` does not follow a link.
            let kind = entry.file_type().expect("file type");
            if kind.is_dir() {
                walk(root, &child, out);
            } else if kind.is_file() {
                out.insert(child, entry.path());
            }
        }
    }
    let mut on_disk = BTreeMap::new();
    walk(project_dir, "", &mut on_disk);
    release
        .iter()
        .filter(|(path, bytes)| match on_disk.get(*path) {
            Some(file) => std::fs::read(file).expect("a project file reads") != **bytes,
            None => true,
        })
        .map(|(path, _)| path.clone())
        .collect()
}

/// 27-42 (Y3 follow-up): the counts of the saved `.git` that differ from the
/// ones measured right before the timed apply, by name. Never a value.
fn saved_count_changes(
    saved: &prod_shape_git::Measured,
    before_apply: &prod_shape_git::Measured,
) -> Vec<&'static str> {
    let mut changed = Vec::new();
    for (name, now, first) in [
        ("saved_packs_changed", saved.packs, before_apply.packs),
        (
            "saved_loose_objects_changed",
            saved.loose,
            before_apply.loose,
        ),
        (
            "saved_tag_refs_changed",
            saved.tag_refs,
            before_apply.tag_refs,
        ),
    ] {
        if now != first {
            changed.push(name);
        }
    }
    changed
}

/// The whole saved-shape compare: the saved `.git` has the configured shape
/// (the names `placed_mismatches` gives, prefixed `saved_`), and its packs,
/// loose objects and tag refs equal the ones measured before the timed apply.
/// `None` for the save is itself a difference.
fn saved_shape_differences(
    saved: Option<&prod_shape_git::Measured>,
    before_apply: &prod_shape_git::Measured,
    emitted: &prod_shape_git::Measured,
    shape: &prod_shape_git::GitShape,
) -> Vec<String> {
    let Some(saved) = saved else {
        return vec!["saved_shape_not_measured".to_string()];
    };
    let mut off: Vec<String> = prod_shape_git::placed_mismatches(saved, emitted, shape)
        .into_iter()
        .map(|name| format!("saved_{name}"))
        .collect();
    off.extend(
        saved_count_changes(saved, before_apply)
            .into_iter()
            .map(str::to_string),
    );
    off
}

/// Record `finding` and print it at once, so a run that dies later still
/// shows what it had found.
fn find(findings: &mut Vec<String>, finding: String) {
    eprintln!("LIVE-SCALE finding: {finding}");
    findings.push(finding);
}

/// Print a row's time beside its budget; the finding when it is over the
/// budget, or when the run left no span for the row (a missing span is a
/// finding too, and the other rows are still printed).
fn within(row: &str, measured: Option<Duration>, budget: Duration) -> Option<String> {
    let Some(measured) = measured else {
        let finding = format!("{row}: the run left no timed span for this row");
        eprintln!(
            "LIVE-SCALE {row}: no span, budget {} ms",
            budget.as_millis()
        );
        return Some(finding);
    };
    eprintln!(
        "LIVE-SCALE {row}: measured {} ms budget {} ms",
        measured.as_millis(),
        budget.as_millis()
    );
    (measured > budget).then(|| {
        format!(
            "{row}: measured {} ms is over the budget of {} ms",
            measured.as_millis(),
            budget.as_millis()
        )
    })
}

/// The run's own git writer calls, timed: the same local transport the run
/// would build from the master, wrapped so each save, restore and advance
/// upload leaves its duration (the run's events carry no durations).
///
/// 27-42 (Y3 follow-up): right after the save span is recorded, outside it,
/// the writer measures the save's `repo` directory below `data` and keeps the
/// result for the gate's saved-shape compare. The measurement lengthens the
/// time between the save and the import, never a timed span.
struct TimedWriter {
    inner: LocalFsTransport,
    /// The rig's data directory (the root of the ledger's save path).
    data: PathBuf,
    spans: std::sync::Mutex<Vec<(&'static str, Duration)>>,
    saved: std::sync::Mutex<Option<prod_shape_git::Measured>>,
}

impl TimedWriter {
    fn new(inner: LocalFsTransport, data: PathBuf) -> TimedWriter {
        TimedWriter {
            inner,
            data,
            spans: std::sync::Mutex::new(Vec::new()),
            saved: std::sync::Mutex::new(None),
        }
    }

    /// The save's `repo` directory as measured after the save span, if the
    /// run saved one.
    fn saved_shape(&self) -> Option<prod_shape_git::Measured> {
        self.saved.lock().expect("saved").clone()
    }

    fn span(&self, name: &'static str) -> Option<Duration> {
        let spans = self.spans.lock().expect("spans");
        let mut hits = spans.iter().filter(|(n, _)| *n == name).map(|(_, d)| *d);
        let first = hits.next()?;
        Some(hits.fold(first, |sum, next| sum + next))
    }

    fn record(&self, name: &'static str, started: Instant) {
        self.spans
            .lock()
            .expect("spans")
            .push((name, started.elapsed()));
    }
}

#[async_trait::async_trait]
impl GitStateWriter for TimedWriter {
    async fn save_git(
        &self,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<GitManifest, CoreError> {
        let started = Instant::now();
        let out = self.inner.save_git(gateway, project, run_id).await;
        self.record("save", started);
        // Outside the span above.
        if out.is_ok() {
            let mut repo = self.data.clone();
            for part in git_save_rel(gateway, project, run_id).split('/') {
                repo.push(part);
            }
            if repo.is_dir() {
                *self.saved.lock().expect("saved") = Some(prod_shape_git::measure_git_dir(&repo));
            }
        }
        out
    }

    async fn restore_git(
        &self,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<GitManifest, CoreError> {
        let started = Instant::now();
        let out = self.inner.restore_git(gateway, project, run_id).await;
        self.record("restore", started);
        out
    }

    async fn write_git_files(&self, project: &str, writes: &[GitWrite]) -> Result<(), CoreError> {
        let started = Instant::now();
        let out = self.inner.write_git_files(project, writes).await;
        self.record("upload", started);
        out
    }

    async fn remove_git_save(
        &self,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<(), CoreError> {
        self.inner.remove_git_save(gateway, project, run_id).await
    }

    async fn probe_write(&self, project: &str, run_id: &str) -> Result<(), CoreError> {
        self.inner.probe_write(project, run_id).await
    }
}

/// Every event of a run with the time it was heard, names only.
struct Timeline {
    start: Instant,
    seen: std::sync::Mutex<Vec<(u128, &'static str)>>,
}

impl Timeline {
    fn new() -> Timeline {
        Timeline {
            start: Instant::now(),
            seen: std::sync::Mutex::new(Vec::new()),
        }
    }
}

impl ProjectsObserver for Timeline {
    fn event(&self, event: &ProjectsEvent) {
        self.seen
            .lock()
            .expect("timeline")
            .push((self.start.elapsed().as_millis(), event.name()));
    }
}

/// 27-24 Task 1 (D-15 automated half, D-13): the production incident's shape.
/// The gateway is stopped, CapApp's `.git` is removed while it is stopped, the
/// gateway starts, and the module's own `setupLocalRepo` runs through the same
/// implementation the Designer hook reaches. The partial repo it leaves is
/// adopted by the next deploy; JGit's status equals `expected_drift`; and the
/// module's `system.git.pull` fast-forwards to a commit pushed afterwards.
#[tokio::test]
#[ignore = "needs the git-module rig: IGNITION_LIVE_GIT=1"]
async fn git_adopt_partial_e2e() {
    let Some(rig) = rig().await else { return };
    let api = rig.api();
    let host = HostGit::new();
    driver_reset("template");
    driver(&["nogit27"]);
    let triggered = driver(&["trigger27"]);
    assert!(
        triggered.contains("TRIGGER setupLocalRepo returned"),
        "the module's setupLocalRepo did not answer: {triggered}"
    );

    // The partial shape G6 recorded: an unborn HEAD, one remote, no refs of its
    // own, no index; the remote's branches are already fetched.
    let (partial, heads, remotes) = classify_now(&rig, &host).await;
    let files = rig
        .node()
        .read_git_state(APP)
        .await
        .expect("host read of .git");
    eprintln!(
        "LIVE-PARTIAL kind={} reason={:?} head={:?} remote={} refs_heads={heads} refs_remotes={remotes} index_present={}",
        partial.kind.as_str(),
        partial.reason,
        partial.head,
        partial
            .remote
            .as_ref()
            .map(|r| r.name.as_str())
            .unwrap_or("-"),
        files.index_present
    );
    assert_eq!(
        partial.kind,
        GitStateKind::Adoptable,
        "setupLocalRepo leaves an adoptable repo: {partial:?}"
    );
    assert!(
        matches!(partial.head, Some(GitHead::Branch { commit: None, .. })),
        "the HEAD is unborn: {:?}",
        partial.head
    );
    assert_eq!(
        partial.remote.as_ref().map(|r| r.name.as_str()),
        Some("origin"),
        "the Designer path names its remote origin"
    );
    assert_eq!(heads, 0, "no local branch");
    assert!(remotes > 0, "the remote's branches are fetched");
    assert!(!files.index_present, "no index");

    // The next deploy adopts it (27-17): imported, adopted, the branch from
    // git.yaml, JGit's status equal to expected_drift, post_status matched.
    let adopted = managed_deploy(
        &rig,
        &api,
        "c4-adopt-v1.1.0",
        "v1.1.0",
        true,
        "production",
        Expect::ADOPTABLE,
    )
    .await;
    let release = gateway_head(&rig).await;
    let (_keep, _checkout, c2) = release_checkout(&rig, "v1.1.0");
    assert_eq!(release, c2, "HEAD is the release after the adoption");

    // The module's pull fast-forwards a commit pushed by someone else.
    let (_work, config) = script_config(&api, &rig.url).await;
    let before = jgit(&api, &config).await;
    assert_eq!(before.paths, adopted.jgit_paths, "the status moved");
    let tip = push_release(&rig, "v1.2.0", "def f():\n    return 4\n");
    module_pull(&api, &config, "adopted")
        .await
        .expect("the module's pull answers");
    tokio::time::sleep(Duration::from_secs(8)).await;
    let head = gateway_head(&rig).await;
    // The release is an ancestor of the pushed commit, so HEAD equal to the tip
    // (not a merge commit of its own) is a fast-forward.
    git(&rig.remote, &["merge-base", "--is-ancestor", &c2, &tip]);
    eprintln!("LIVE-FF adopted head_is_tip={}", head == tip);
    assert_eq!(head, tip, "the pull did not reach the pushed commit");
    let after = jgit(&api, &config).await;
    assert_eq!(after.paths, before.paths, "the pull changed the status");
    assert_eq!(after.unpushed, 0, "unpushed after the pull");
    assert_export_matches(&rig, &api, "adopted", &tip, "def f():\n    return 4\n").await;
    eprintln!("LIVE-CHECK adopt_then_pull_fast_forwards tip={}", &tip[..7]);
    write_evidence(
        &rig,
        "git_adopt_partial_e2e",
        &serde_json::json!({
            "partial_kind": partial.kind.as_str(),
            "partial_remote": partial.remote.as_ref().map(|r| r.name.clone()),
            "adopted": adopted.evidence,
            "pulled_tip": tip,
        }),
    );
    eprintln!("LIVE-RAN git_adopt_partial_e2e");
}

/// 27-24 Task 1 (C3): after a managed deploy a gateway restart runs the
/// module's start sync without reverting the deploy, and `system.git.pull`
/// fast-forwards to a newer remote commit.
#[tokio::test]
#[ignore = "needs the git-module rig: IGNITION_LIVE_GIT=1"]
async fn git_restart_pull_e2e() {
    let Some(rig) = rig().await else { return };
    let api = rig.api();
    driver_reset("template");
    let deployed = managed_deploy(
        &rig,
        &api,
        "c4-restart-v1.1.0",
        "v1.1.0",
        true,
        "production",
        Expect::MANAGED,
    )
    .await;
    let (_keep, _checkout, c2) = release_checkout(&rig, "v1.1.0");
    assert_eq!(gateway_head(&rig).await, c2, "HEAD is the release");
    let (_work, config) = script_config(&api, &rig.url).await;
    let status = jgit(&api, &config).await;

    restart_gateway(&api).await;

    // The start sync did not revert the deploy.
    let (_work, config) = script_config(&api, &rig.url).await;
    let export = GatewayProjects(&api)
        .export_bytes(APP)
        .await
        .expect("export after the restart");
    let live = canonical_digest(&export).expect("the export has members");
    eprintln!(
        "LIVE-C3 after restart: export_equals_artifact={} head_is_release={}",
        live.sha256 == deployed.artifact_sha256,
        gateway_head(&rig).await == c2
    );
    assert_eq!(
        live.sha256, deployed.artifact_sha256,
        "the restart changed the project"
    );
    assert_eq!(
        gateway_head(&rig).await,
        c2,
        "the restart moved the repository's HEAD"
    );
    let after_restart = jgit(&api, &config).await;
    assert_eq!(
        after_restart.paths, status.paths,
        "the restart changed JGit's status"
    );
    assert_eq!(after_restart.unpushed, status.unpushed);
    assert_eq!(after_restart.branch, status.branch);

    // A newer remote commit is fast-forwarded by the module's pull.
    let tip = push_release(&rig, "v1.2.0", "def f():\n    return 4\n");
    module_pull(&api, &config, "after-restart")
        .await
        .expect("the module's pull answers");
    tokio::time::sleep(Duration::from_secs(8)).await;
    let head = gateway_head(&rig).await;
    eprintln!("LIVE-FF after-restart head_is_tip={}", head == tip);
    assert_eq!(head, tip, "the pull did not reach the pushed commit");
    let after_pull = jgit(&api, &config).await;
    assert_eq!(
        after_pull.paths, status.paths,
        "the pull changed the status"
    );
    assert_eq!(after_pull.unpushed, 0, "unpushed after the pull");
    assert_export_matches(&rig, &api, "restart", &tip, "def f():\n    return 4\n").await;
    eprintln!(
        "LIVE-CHECK restart_then_pull_fast_forwards tip={}",
        &tip[..7]
    );
    write_evidence(
        &rig,
        "git_restart_pull_e2e",
        &serde_json::json!({
            "deployed": deployed.evidence,
            "export_equals_artifact_after_restart": true,
            "pulled_tip": tip,
        }),
    );
    eprintln!("LIVE-RAN git_restart_pull_e2e");
}

/// The subject of the one commit `capture27.sh seedprod27` adds on top of the
/// production-shape generator's release (it carries `deploy/project.yaml`).
const PRODSHAPE_DESCRIPTOR_SUBJECT: &str = "release deploy descriptor";

/// 27-24 Task 1 (D-18): the rig remote was seeded by the production-shape
/// generator (`capture27.sh seedprod27`) and the emitted gateway project, its
/// padded `.git` as generated, was put in place (a module clone of the remote
/// would carry none of the padding); the live `.git` is checked against the
/// configured shape (`IGN_PROD_SHAPE_FILE`, `PRODSHAPE_GIT_OUT`). The deploy
/// runs on that project and the handover's save, restore plus verify, and
/// advance upload each finish inside the 27-06 Linux host budgets. No shape
/// value is printed: only times, budgets, event names and difference counts.
#[tokio::test]
#[ignore = "needs the git-module rig seeded by seedprod27: IGNITION_LIVE_GIT=1"]
async fn git_prodscale_rig() {
    let Some(rig) = rig().await else { return };
    let api = rig.api();
    let host = HostGit::new();
    let (state, refs) = gateway_state(&rig, &host).await;
    assert_eq!(state.kind, GitStateKind::Managed, "{state:?}");
    let (_keep, release_tree, release) = release_checkout(&rig, "v1.1.0");
    assert!(
        !head_at(&refs, &release),
        "the module's clone must sit behind the release (run seedprod27 first)"
    );
    // CodeRabbit #58 (W14): the synthetic seed also leaves the clone behind the
    // release, so the remote's identity is checked: only seedprod27 tags v1.1.0
    // on its one descriptor commit above the generator's release.
    assert_eq!(
        git(&release_tree, &["log", "-1", "--format=%s", &release]),
        PRODSHAPE_DESCRIPTOR_SUBJECT,
        "the rig remote is not the production-shape seed (run seedprod27 first)"
    );
    // CodeRabbit #58 follow-up (X3): the live .git must have the configured
    // shape before anything is applied. A module clone of the remote holds one
    // pack and none of the generator's padding, so seedprod27 places the
    // emitted gateway project; this checks that it is what the gateway holds.
    // Only the names of the counts that differ are printed, never a value.
    assert!(
        std::env::var_os("IGN_PROD_SHAPE_FILE").is_some_and(|v| !v.is_empty()),
        "IGN_PROD_SHAPE_FILE is not set: the gate compares the live .git with the configured shape"
    );
    let shape = prod_shape_git::GitShape::from_env();
    let emitted = PathBuf::from(require_env("PRODSHAPE_GIT_OUT"));
    let emitted_git = prod_shape_git::measure_git_dir(
        &emitted.join("data").join("projects").join(APP).join(".git"),
    );
    let live_git = prod_shape_git::measure_git_dir(&rig.project_dir().join(".git"));
    let off = prod_shape_git::placed_mismatches(&live_git, &emitted_git, &shape);
    eprintln!(
        "LIVE-SCALE shape: live .git matches the configured shape={} differing={off:?}",
        off.is_empty()
    );
    assert!(
        off.is_empty(),
        "REFUSING: the live .git does not have the configured shape (differing: {off:?}); run seedprod27 again"
    );
    let (_work, config) = script_config(&api, &rig.url).await;

    let first = DeployFlags {
        first_change: true,
        ..DeployFlags::default()
    };
    let check = deploy_in_process(&rig, true, ENV, "v1.1.0", &first, "c4-scale-check")
        .await
        .unwrap_or_else(|err| panic!("scale check: {} ({err})", err.code()));
    let check_json = serde_json::to_value(&check).expect("check json");
    eprintln!(
        "LIVE-SCALE check: state={} action={} dirty_source={}",
        check_json["git"]["nodes"][0]["state"],
        check_json["action"],
        check_json["git"]["nodes"][0]["dirty_source"]
    );
    assert_eq!(check_json["git"]["nodes"][0]["state"], "managed");

    let flags = DeployFlags {
        artifact_sha256: Some(check.artifact_sha256.clone()),
        first_change: true,
        ..DeployFlags::default()
    };
    // CodeRabbit #60 (Y3): the module's start sync can still write after the
    // check above (wait_clone only proves an index and a ref), and the timed
    // save reads the .git as it is now. So it is measured again right before
    // the timed apply, after every write this gate makes: it must still have
    // the configured shape, and its packs, loose objects and tag refs must
    // equal the first measurement. Names only, never a value.
    let before_apply = prod_shape_git::measure_git_dir(&rig.project_dir().join(".git"));
    let mut changed_since = prod_shape_git::placed_mismatches(&before_apply, &emitted_git, &shape);
    for (name, now, first) in [
        ("packs_changed", before_apply.packs, live_git.packs),
        ("loose_objects_changed", before_apply.loose, live_git.loose),
        ("tag_refs_changed", before_apply.tag_refs, live_git.tag_refs),
    ] {
        if now != first {
            changed_since.push(name);
        }
    }
    eprintln!(
        "LIVE-SCALE shape before the timed apply: unchanged and configured={} differing={changed_since:?}",
        changed_since.is_empty()
    );
    assert!(
        changed_since.is_empty(),
        "REFUSING: the live .git changed or lost the configured shape before the timed apply (differing: {changed_since:?}); wait for the module's start sync and run the gate again"
    );
    let timed = TimedWriter::new(rig.node(), rig.data.clone());
    let timeline = Timeline::new();
    let extras = Extras {
        observer: Some(&timeline),
        git_writer: Some(&timed),
    };
    let started = Instant::now();
    let applied = deploy_in_process_with(&rig, false, ENV, "v1.1.0", &flags, "c4-scale", &extras)
        .await
        .unwrap_or_else(|err| panic!("scale apply: {} ({err})", err.code()));
    let total = started.elapsed();
    let applied_json = serde_json::to_value(&applied).expect("apply json");
    let restore = &applied_json["git"]["nodes"][0]["restore"];
    eprintln!(
        "LIVE-SCALE apply: outcome={} total_ms={} restored={} advanced={} verified={} post_status={} reason={}",
        applied.outcome.as_str(),
        total.as_millis(),
        restore["restored"],
        restore["advanced"],
        restore["verified"],
        restore["post_status"],
        restore["reason"]
    );
    for (at, name) in timeline.seen.lock().expect("timeline").iter() {
        eprintln!("LIVE-SCALE event {name} at {at} ms");
    }

    // 27-42: every finding is recorded and printed as it is found, and the
    // gate fails once at its end naming all of them (the 27-25 rule), so a red
    // run still yields its timings. Nothing below is dropped or loosened.
    let mut findings: Vec<String> = Vec::new();
    if applied.outcome.as_str() != "imported" {
        find(
            &mut findings,
            format!(
                "scale outcome (slug {}): {} is not imported: {}",
                applied_json["slug"],
                applied.outcome.as_str(),
                applied_json["git"]
            ),
        );
    }
    if !(restore["restored"] == true && restore["advanced"] == true && restore["verified"] == true)
    {
        find(
            &mut findings,
            format!("the handover did not complete: {restore}"),
        );
    }
    if restore["post_status"] != "matched" {
        find(
            &mut findings,
            format!("post_status is not matched: {restore}"),
        );
    }

    // The budgets (D-18): none is scaled and none is skipped, and each row is
    // printed whatever the outcome.
    for (row, span, budget) in [
        ("save", timed.span("save"), HOST_LINUX_SAVE),
        (
            "restore_verify",
            timed.span("restore"),
            HOST_LINUX_RESTORE_VERIFY,
        ),
        ("upload", timed.span("upload"), HOST_LINUX_UPLOAD),
    ] {
        if let Some(finding) = within(row, span, budget) {
            find(&mut findings, finding);
        }
    }

    // Y3 follow-up (27-42): the save's `.git`, measured right after the timed
    // save and outside its span, has the configured shape and the counts
    // measured right before the timed apply. Names only, never a value.
    let saved_off = saved_shape_differences(
        timed.saved_shape().as_ref(),
        &before_apply,
        &emitted_git,
        &shape,
    );
    eprintln!(
        "LIVE-SCALE saved shape equals the checked shape={} differing={saved_off:?}",
        saved_off.is_empty()
    );
    if !saved_off.is_empty() {
        find(
            &mut findings,
            format!("the saved .git differs from the checked shape (differing: {saved_off:?})"),
        );
    }

    // JGit says what the run predicted (D-25): a difference blocks the cut-over.
    // `data.git.expected_drift` is byte-sorted and capped; `expected_drift_count`
    // is the full number (the published contract). So: every listed path is in
    // JGit's list, the run's count equals an independent recomputation (the
    // rig's project directory on disk against the release's bytes, 27-42: what
    // JGit reads, not the read-back export), and JGit's list equals that
    // recomputed full set.
    if applied_json["git"]["expected_drift_complete"] != true {
        find(
            &mut findings,
            format!(
                "expected_drift_complete is not true: {}",
                applied_json["git"]["expected_drift_complete"]
            ),
        );
    }
    let predicted = strs(&applied_json["git"]["expected_drift"]);
    let counted = applied_json["git"]["expected_drift_count"].as_u64();
    if counted.is_none() {
        find(
            &mut findings,
            "the run does not report the full drift count".to_string(),
        );
    }
    tokio::time::sleep(Duration::from_secs(10)).await;
    let jg = jgit(&api, &config).await;
    // The listed part is capped, so JGit's list is the longer one: only the
    // listed paths JGit lacks matter here.
    let (_, missing) = drift_difference(&jg, &predicted);
    let (_keep2, checkout, _c) = release_checkout(&rig, "v1.1.0");
    let release_files = zip_members(&build_artifact(&checkout).expect("artifact builds").zip);
    let changed = disk_drift(&rig.project_dir(), &release_files);
    let full: BTreeSet<String> = changed.iter().map(|file| module_path(file)).collect();
    let extra: Vec<&String> = jg.paths.difference(&full).collect();
    let absent: Vec<&String> = full.difference(&jg.paths).collect();
    eprintln!(
        "LIVE-SCALE jgit: listed_paths_missing_from_jgit={} jgit_vs_recomputed extra={} missing={} run_count_equals_recomputed={} unpushed={} branch={}",
        missing.len(),
        extra.len(),
        absent.len(),
        counted == Some(changed.len() as u64),
        jg.unpushed,
        jg.branch
    );
    let jgit_findings = findings.len();
    if !missing.is_empty() {
        find(
            &mut findings,
            format!(
                "CUT-OVER BLOCKER (D-25): {} listed path(s) are not in JGit's status",
                missing.len()
            ),
        );
    }
    if !(extra.is_empty() && absent.is_empty()) {
        find(
            &mut findings,
            format!(
                "CUT-OVER BLOCKER (D-25): JGit differs from the full prediction: extra={} missing={}",
                extra.len(),
                absent.len()
            ),
        );
    }
    if counted != Some(changed.len() as u64) {
        find(
            &mut findings,
            format!(
                "the run's drift count ({counted:?}) differs from the recomputed one ({})",
                changed.len()
            ),
        );
    }
    if jg.unpushed != 0 {
        find(
            &mut findings,
            format!("unpushed commits after the deploy: {}", jg.unpushed),
        );
    }
    if findings.len() == jgit_findings {
        eprintln!("LIVE-CHECK prodscale_jgit_equals_expected_drift");
    }
    write_evidence(
        &rig,
        "git_prodscale_rig",
        &serde_json::json!({
            "outcome": applied.outcome.as_str(),
            "total_ms": total.as_millis(),
            "save_ms": timed.span("save").map(|d| d.as_millis()),
            "restore_verify_ms": timed.span("restore").map(|d| d.as_millis()),
            "upload_ms": timed.span("upload").map(|d| d.as_millis()),
            "post_status": restore["post_status"],
            "saved_shape_differing": saved_off,
            "findings": findings,
        }),
    );
    assert!(
        findings.is_empty(),
        "findings of the production-scale run: {findings:#?}"
    );
    eprintln!("LIVE-RAN git_prodscale_rig");
}

/// The provider an owned-tags release creates.
const TAG_PROVIDER: &str = "CapTags";

/// The atomic tags of a provider export, by count.
fn atomic_tags(export: &serde_json::Value) -> usize {
    match export {
        serde_json::Value::Object(map) => {
            usize::from(map.get("tagType").and_then(|t| t.as_str()) == Some("AtomicTag"))
                + map.values().map(atomic_tags).sum::<usize>()
        }
        serde_json::Value::Array(items) => items.iter().map(atomic_tags).sum(),
        _ => 0,
    }
}

/// 27-24 Task 1 (checker W12): `capture27.sh seedtags27` put release v1.3.0 on
/// the remote's production; it owns the tag provider `CapTags` (two neutral
/// tags). The deploy goes through the Phase 26 owned path on top of the git
/// handover: outcome `applied`, the provider listed with its two tags, JGit's
/// status equal to `expected_drift` and `post_status` matched.
#[tokio::test]
#[ignore = "needs the git-module rig after seedtags27: IGNITION_LIVE_GIT=1"]
async fn git_owned_tags_seed() {
    let Some(rig) = rig().await else { return };
    let api = rig.api();
    let tag = git(&rig.remote, &["tag", "--list", "v1.3.0"]);
    assert_eq!(tag, "v1.3.0", "run `capture27.sh seedtags27` first");
    let deployed = managed_deploy(
        &rig,
        &api,
        "c4-tags-v1.3.0",
        "v1.3.0",
        true,
        "production",
        Expect::TAGS,
    )
    .await;
    let gateway = GatewayProjects(&api);
    let providers = gateway
        .tag_provider_names()
        .await
        .expect("the gateway lists its tag providers");
    assert!(
        providers.iter().any(|p| p == TAG_PROVIDER),
        "the gateway lists the provider {TAG_PROVIDER}: {providers:?}"
    );
    let export: serde_json::Value = serde_json::from_slice(
        &gateway
            .tag_export(TAG_PROVIDER)
            .await
            .expect("the provider's export"),
    )
    .expect("the export is JSON");
    let tags = atomic_tags(&export);
    eprintln!("LIVE-TAGS provider={TAG_PROVIDER} listed=true atomic_tags={tags}");
    assert_eq!(tags, 2, "the provider holds the two neutral tags");
    eprintln!("LIVE-CHECK owned_tags_applied provider={TAG_PROVIDER} tags=2");
    write_evidence(
        &rig,
        "git_owned_tags_seed",
        &serde_json::json!({
            "deployed": deployed.evidence,
            "provider": TAG_PROVIDER,
            "atomic_tags": tags,
        }),
    );
    eprintln!("LIVE-RAN git_owned_tags_seed");
}

/// Every file below a gateway `.git`: its path, then the sha256 of its bytes,
/// or its size for a file below `objects/` (the definition of the manifest
/// digest, so a difference can be named).
fn git_file_map(git_dir: &Path) -> BTreeMap<String, String> {
    use sha2::{Digest, Sha256};
    fn walk(root: &Path, rel: &str, out: &mut BTreeMap<String, String>) {
        for entry in std::fs::read_dir(root.join(rel)).expect("read_dir") {
            let entry = entry.expect("entry");
            let name = entry.file_name().to_string_lossy().to_string();
            let child = if rel.is_empty() {
                name
            } else {
                format!("{rel}/{name}")
            };
            let kind = entry.file_type().expect("type");
            assert!(!kind.is_symlink(), "{child} is a symlink");
            if kind.is_dir() {
                walk(root, &child, out);
            } else if child.starts_with("objects/") {
                out.insert(child, entry.metadata().expect("metadata").len().to_string());
            } else {
                let bytes = std::fs::read(root.join(&child)).unwrap_or_default();
                out.insert(child, format!("{:x}", Sha256::digest(&bytes)));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(git_dir, "", &mut out);
    out
}

/// The three values the human Designer open must leave unchanged (D-15): the
/// project's `.git`, the project's export and the owned provider's export.
async fn designer_snapshot(rig: &Rig, api: &ReqwestGatewayApi) -> serde_json::Value {
    use sha2::{Digest, Sha256};
    let manifest = rig
        .node()
        .git_manifest(APP)
        .await
        .expect("the project's .git manifest");
    let files = git_file_map(&rig.project_dir().join(".git"));
    let gateway = GatewayProjects(api);
    let export = canonical_digest(
        &gateway
            .export_bytes(APP)
            .await
            .expect("the project's export"),
    )
    .expect("the export has members");
    let providers = gateway.tag_provider_names().await.expect("providers");
    assert!(
        providers.iter().any(|p| p == TAG_PROVIDER),
        "the gateway lists no provider {TAG_PROVIDER}: seedtags27 and git_owned_tags_seed come first"
    );
    let tags: serde_json::Value = serde_json::from_slice(
        &gateway
            .tag_export(TAG_PROVIDER)
            .await
            .expect("the provider's export"),
    )
    .expect("the export is JSON");
    let canonical = serde_json::to_vec(&tags).expect("canonical json");
    serde_json::json!({
        "git_manifest": {
            "files": manifest.files,
            "bytes": manifest.bytes,
            "digest": manifest.digest,
        },
        "git_files": files,
        "project_export": {
            "members": export.members.len(),
            "digest": export.sha256,
            "member_digests": export.members,
        },
        "cap_tags": {
            "tags": atomic_tags(&tags),
            "export_digest": format!("{:x}", Sha256::digest(&canonical)),
        },
    })
}

/// The names under `key` whose value differs between two JSON objects.
fn differing(before: &serde_json::Value, after: &serde_json::Value, key: &str) -> Vec<String> {
    let empty = serde_json::Map::new();
    let b = before[key].as_object().unwrap_or(&empty);
    let a = after[key].as_object().unwrap_or(&empty);
    let mut names: BTreeSet<String> = BTreeSet::new();
    for (name, value) in b {
        if a.get(name) != Some(value) {
            names.insert(name.clone());
        }
    }
    for name in a.keys() {
        if !b.contains_key(name) {
            names.insert(name.clone());
        }
    }
    names.into_iter().collect()
}

/// The first 16 characters of a digest, for a line a person reads.
fn short(value: &serde_json::Value) -> String {
    value.as_str().unwrap_or("-").chars().take(16).collect()
}

/// 27-24 Task 2 support (D-15): `capture27.sh designer-evidence before|after`.
/// `before` records the snapshot right after a green deploy of the owned-tags
/// release and refuses when `CapTags` holds fewer than two tags; `after`
/// compares with it once a human has opened the project in the Designer.
#[tokio::test]
#[ignore = "needs the git-module rig after git_owned_tags_seed: IGNITION_LIVE_GIT=1"]
async fn git_designer_evidence() {
    let Some(rig) = rig().await else { return };
    let api = rig.api();
    let mode = std::env::var("IGN_DESIGNER_EVIDENCE").unwrap_or_default();
    let stored = rig.evidence.join("designer-evidence.before.json");
    let now = designer_snapshot(&rig, &api).await;
    let line = format!(
        "git_manifest files={} digest={}; project_export members={} digest={}; CapTags tags={} export_digest={}",
        now["git_manifest"]["files"],
        short(&now["git_manifest"]["digest"]),
        now["project_export"]["members"],
        short(&now["project_export"]["digest"]),
        now["cap_tags"]["tags"],
        short(&now["cap_tags"]["export_digest"]),
    );
    match mode.as_str() {
        "before" => {
            assert!(
                now["cap_tags"]["tags"].as_u64().unwrap_or(0) >= 2,
                "CapTags holds fewer than two tags: the tag evidence would be vacuous"
            );
            std::fs::write(
                &stored,
                serde_json::to_vec_pretty(&now).expect("evidence json"),
            )
            .expect("evidence write");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&stored, std::fs::Permissions::from_mode(0o600))
                    .expect("owner-only evidence");
            }
            eprintln!("designer-evidence before: {line}");
        }
        "after" => {
            let before: serde_json::Value = serde_json::from_slice(
                &std::fs::read(&stored)
                    .expect("no stored `before`: run designer-evidence before first"),
            )
            .expect("the stored evidence is JSON");
            let mut changed: Vec<String> = Vec::new();
            if before["git_manifest"] != now["git_manifest"] {
                let files = differing(&before, &now, "git_files");
                changed.push(format!(
                    ".git manifest (files {} -> {}, differing: {files:?})",
                    before["git_manifest"]["files"], now["git_manifest"]["files"]
                ));
            }
            if before["project_export"]["digest"] != now["project_export"]["digest"] {
                let members: Vec<String> = {
                    let b = before["project_export"]["member_digests"].clone();
                    let a = now["project_export"]["member_digests"].clone();
                    let wrap = |v: serde_json::Value| serde_json::json!({ "m": v });
                    differing(&wrap(b), &wrap(a), "m")
                };
                changed.push(format!("project export (differing members: {members:?})"));
            }
            if before["cap_tags"] != now["cap_tags"] {
                changed.push(format!(
                    "CapTags export (tags {} -> {})",
                    before["cap_tags"]["tags"], now["cap_tags"]["tags"]
                ));
            }
            eprintln!("designer-evidence after: {line}");
            if changed.is_empty() {
                eprintln!("designer-evidence: unchanged");
            } else {
                eprintln!("designer-evidence: changed: {}", changed.join("; "));
                panic!("the Designer open changed: {}", changed.join("; "));
            }
        }
        other => panic!("IGN_DESIGNER_EVIDENCE is before or after, not {other:?}"),
    }
    eprintln!("LIVE-RAN git_designer_evidence");
}

// ---- 27-25 Task 1: the pair rig, the same git module on both nodes ---------------
//
// The pair driver defaults to git module 2.3.5 (27-61); a pair built with
// `IGN_PAIR_MODULE_VER=2.3.4` is for `git_pair_detect_live` only.
//
// `capture27-pair.sh live <log> git_pair_e2e` (it sources `tokens.env` and exports
// `IGNITION_LIVE_PAIR=1`, `IGN_PAIR_DATA_A`, `IGN_PAIR_DATA_B`,
// `IGN_PAIR_REMOTE` and `IGN_LIVE_EVIDENCE_DIR`). Loopback only; both gateways
// are named by the guards below before anything is written. A JGit-versus-
// `expected_drift` difference on either node is a cut-over blocker (D-25): no
// assertion here is loosened.

const PAIR_NAME_A: &str = "ign-pair-a";
const PAIR_NAME_B: &str = "ign-pair-b";

/// The pair rig as a [`Rig`]: gw-a is the master (its `data/projects` is bound
/// onto `IGN_PAIR_DATA_A/projects`), gw-b the backup. `None` is the quiet green
/// skip.
async fn pair_rig() -> Option<Rig> {
    if std::env::var("IGNITION_LIVE_PAIR").as_deref() != Ok("1") {
        skip("IGNITION_LIVE_PAIR is not 1");
        return None;
    }
    let url_a = std::env::var("IGN_PAIR_URL_A").unwrap_or_else(|_| "http://localhost:18300".into());
    let url_b = std::env::var("IGN_PAIR_URL_B").unwrap_or_else(|_| "http://localhost:18301".into());
    for (var, url) in [("IGN_PAIR_URL_A", &url_a), ("IGN_PAIR_URL_B", &url_b)] {
        if let Err(why) = local_url_guard(url) {
            panic!("{var} refused: {why}");
        }
    }
    let token_a = require_env("IGN_PAIR_TOKEN_A");
    let token_b = require_env("IGN_PAIR_TOKEN_B");
    let data_a = PathBuf::from(require_env("IGN_PAIR_DATA_A"));
    let data_b = PathBuf::from(require_env("IGN_PAIR_DATA_B"));
    for (var, dir) in [("IGN_PAIR_DATA_A", &data_a), ("IGN_PAIR_DATA_B", &data_b)] {
        assert!(
            dir.join("projects").is_dir(),
            "{var} has no projects directory"
        );
    }
    let evidence = PathBuf::from(require_env("IGN_LIVE_EVIDENCE_DIR"));
    std::fs::create_dir_all(&evidence).expect("evidence dir");
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repo root")
        .canonicalize()
        .expect("repo root resolves");
    for (what, dir) in [
        ("IGN_LIVE_EVIDENCE_DIR", &evidence),
        ("IGN_PAIR_DATA_A", &data_a),
        ("IGN_PAIR_DATA_B", &data_b),
    ] {
        assert!(
            !dir.canonicalize()
                .expect("resolves")
                .starts_with(&repo_root),
            "{what} must be outside the repository"
        );
    }
    let remote = PathBuf::from(require_env("IGN_PAIR_REMOTE"));
    assert!(remote.is_dir(), "the pair rig's bare repository is missing");
    let rig = Rig {
        url: url_a,
        token: token_a,
        data: data_a,
        evidence,
        remote,
        gateway: PAIR_NAME_A.to_string(),
        backup: Some(PairBackup {
            url: url_b,
            token: token_b,
            data: data_b,
        }),
    };
    let info_a = GatewayApi::gateway_info(&rig.api())
        .await
        .expect("gateway-info of gw-a for the name guard");
    assert_eq!(
        info_a.name.as_deref(),
        Some(PAIR_NAME_A),
        "refusing: gw-a is not the {PAIR_NAME_A} fixture"
    );
    let info_b = GatewayApi::gateway_info(&pair_api_b(&rig))
        .await
        .expect("gateway-info of gw-b for the name guard");
    assert!(
        matches!(info_b.name.as_deref(), Some(PAIR_NAME_A | PAIR_NAME_B)),
        "refusing: gw-b is not the pair rig's backup"
    );
    Some(rig)
}

/// The backup's own client (token auth).
fn pair_api_b(rig: &Rig) -> ReqwestGatewayApi {
    let backup = rig.backup.as_ref().expect("the pair rig has a backup");
    ReqwestGatewayApi::for_tests(
        &backup.url,
        Some(Credential::Token(Secret::new(backup.token.clone()))),
    )
}

/// The commit a classified repo's HEAD names.
fn head_commit(refs: &GitRefs) -> String {
    match &refs.head {
        GitHead::Branch {
            commit: Some(commit),
            ..
        }
        | GitHead::Detached { commit } => commit.clone(),
        other => panic!("HEAD names no commit: {other:?}"),
    }
}

/// Whether `HEAD` names `commit`. Another local branch at `commit` does not
/// count: a gate about HEAD reads HEAD.
fn head_at(refs: &GitRefs, commit: &str) -> bool {
    refs.head.commit() == Some(commit)
}

/// CodeRabbit #58 (W12): HEAD on `production` at v1.0.0 is not "at v1.1.0"
/// because another local branch holds v1.1.0.
#[test]
fn head_at_reads_head_not_any_branch() {
    let (old, new) = ("1".repeat(40), "2".repeat(40));
    let refs = GitRefs {
        head: GitHead::Branch {
            name: "production".to_string(),
            commit: Some(old.clone()),
        },
        heads: BTreeMap::from([
            ("refs/heads/production".to_string(), old.clone()),
            ("refs/heads/other".to_string(), new.clone()),
        ]),
        remotes: BTreeMap::new(),
    };
    assert!(head_at(&refs, &old));
    assert!(
        !head_at(&refs, &new),
        "another branch at the commit is not HEAD"
    );
    let unborn = GitRefs {
        head: GitHead::Branch {
            name: "production".to_string(),
            commit: None,
        },
        ..refs
    };
    assert!(!head_at(&unborn, &old), "an unborn HEAD names no commit");
}

/// 27-42 (T-27-188): the gate's independent drift set is what JGit measures,
/// the rig's project directory against the release's bytes. A file holding
/// the gateway's form is drift, a file holding the release's bytes is not,
/// and `.git` is never read.
#[test]
fn disk_drift_counts_only_files_that_differ_from_the_release() {
    let dir = tempfile::tempdir().expect("tempdir");
    let project = dir.path();
    let kept = "com.inductiveautomation.perspective/views/V/resource.json";
    let rewritten = "ignition/script-python/cap/util/resource.json";
    let release = BTreeMap::from([
        (kept.to_string(), b"{\"a\":1}".to_vec()),
        (rewritten.to_string(), b"{\"a\":1}".to_vec()),
    ]);
    let put = |rel: &str, bytes: &[u8]| {
        let path = project.join(rel);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, bytes).expect("write");
    };
    put(kept, b"{\"a\":1}");
    put(rewritten, b"{ \"a\": 1 }\n");
    // Whatever `.git` holds, even a copy of a release path, is not read.
    put(".git/HEAD", b"ref: refs/heads/production\n");
    put(&format!(".git/{kept}"), b"not the release");
    assert_eq!(
        disk_drift(project, &release),
        BTreeSet::from([rewritten.to_string()]),
        "only the file holding other bytes than the release is drift"
    );
    assert_eq!(module_path(rewritten), "ignition/script-python/cap/util");
}

/// A release member that is not on the disk is a deletion, and a deletion
/// still fails the gate.
#[test]
fn disk_drift_reports_a_release_member_missing_on_disk() {
    let dir = tempfile::tempdir().expect("tempdir");
    let gone = "project.json";
    let release = BTreeMap::from([(gone.to_string(), b"{}".to_vec())]);
    std::fs::create_dir_all(dir.path().join(".git")).expect("git dir");
    assert_eq!(
        disk_drift(dir.path(), &release),
        BTreeSet::from([gone.to_string()])
    );
}

/// 27-42 (Y3 follow-up): the saved shape compare names the counts that
/// changed, never a measured value.
#[test]
fn saved_shape_mismatch_names_only_counts() {
    let checked = prod_shape_git::Measured {
        bytes: 1_000_000,
        files: 90,
        packs: 3,
        loose: 40,
        tag_refs: 7,
    };
    assert!(saved_count_changes(&checked, &checked).is_empty());
    let saved = prod_shape_git::Measured {
        packs: 4,
        ..checked.clone()
    };
    assert_eq!(
        saved_count_changes(&saved, &checked),
        vec!["saved_packs_changed"]
    );
    let everything = prod_shape_git::Measured {
        packs: 1,
        loose: 2,
        tag_refs: 3,
        ..checked.clone()
    };
    assert_eq!(
        saved_count_changes(&everything, &checked),
        vec![
            "saved_packs_changed",
            "saved_loose_objects_changed",
            "saved_tag_refs_changed"
        ]
    );
}

/// Every git event of a pair run with the node it names and the time it was
/// heard (names, nodes and milliseconds only).
struct PairTimeline {
    start: Instant,
    seen: std::sync::Mutex<Vec<(u128, &'static str, String)>>,
}

impl PairTimeline {
    fn new() -> PairTimeline {
        PairTimeline {
            start: Instant::now(),
            seen: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// When the first event `name` for `node` was heard.
    fn at(&self, name: &str, node: &str) -> Option<u128> {
        self.seen
            .lock()
            .expect("timeline")
            .iter()
            .find(|(_, n, who)| *n == name && who == node)
            .map(|(at, _, _)| *at)
    }
}

impl ProjectsObserver for PairTimeline {
    fn event(&self, event: &ProjectsEvent) {
        let node = match event {
            ProjectsEvent::GitStateRead { node, .. }
            | ProjectsEvent::GitSaved { node, .. }
            | ProjectsEvent::GitRestored { node }
            | ProjectsEvent::GitAdvanced { node, .. }
            | ProjectsEvent::GitSaveRemoved { node, .. }
            | ProjectsEvent::GitRestoreFailed { node, .. } => node.clone(),
            _ => "-".to_string(),
        };
        self.seen.lock().expect("timeline").push((
            self.start.elapsed().as_millis(),
            event.name(),
            node,
        ));
    }
}

/// 27-25 Task 1 (C4 pair, D-16, D-19): with the module on both nodes and both
/// clones at v1.0.0, `--check` reports two managed nodes and changes nothing;
/// one apply of v1.1.0 leaves both repositories at the release, with JGit's
/// status on each node equal to `data.git.expected_drift`; a backup repository
/// moved one commit behind by hand is refused `nodes_disagree` live.
#[tokio::test]
#[ignore = "needs the pair rig with the git module on both nodes: IGNITION_LIVE_PAIR=1"]
async fn git_pair_e2e() {
    let Some(rig) = pair_rig().await else { return };
    let backup = rig.backup.as_ref().expect("pair");
    let api_a = rig.api();
    let api_b = pair_api_b(&rig);
    let host = HostGit::new();
    let node_a = LocalFsTransport::new(rig.data.clone(), "nodes.master");
    let node_b = LocalFsTransport::new(backup.data.clone(), "nodes.backup");

    let (_keep, checkout, c2) = release_checkout(&rig, "v1.1.0");
    let c1 = git(&checkout, &["rev-parse", "v1.0.0^{commit}"]);
    let parent = git(&checkout, &["rev-parse", &format!("{c2}~1")]);
    for (label, node) in [("master", &node_a), ("backup", &node_b)] {
        let (state, refs) = node_state(node, &host).await;
        assert_eq!(
            state.kind,
            GitStateKind::Managed,
            "{label}: the module's clone must classify as managed: {state:?}"
        );
        assert_eq!(
            head_commit(&refs),
            c1,
            "{label}: the module's clone must sit at v1.0.0 (run the pair driver's reset first)"
        );
    }
    eprintln!("LIVE-PAIR start: both nodes managed at v1.0.0");

    // The scriptExec route is deployed on the master and follows to the backup
    // by the project sync; JGit is asked on each node through its own route.
    let (_work, config) = script_config(&api_a, &rig.url).await;
    let started = Instant::now();
    loop {
        match script_run(&api_b, &config, "rig", WEBDEV_PROJECT, "print 1").await {
            Ok(_) => break,
            Err(err) => {
                assert!(
                    started.elapsed() < Duration::from_secs(300),
                    "the scriptExec route never answered on the backup: {}",
                    err.code()
                );
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        }
    }
    eprintln!(
        "LIVE-PAIR script route answers on the backup after {} s",
        started.elapsed().as_secs()
    );

    // --check: two managed nodes; reads only on both.
    let gitdir_a = rig.project_dir().join(".git");
    let gitdir_b = backup.data.join("projects").join(APP).join(".git");
    let before = (dir_manifest(&gitdir_a), dir_manifest(&gitdir_b));
    let first = DeployFlags {
        first_change: true,
        ..DeployFlags::default()
    };
    let check = deploy_in_process(&rig, true, ENV, "v1.1.0", &first, "pair-check")
        .await
        .unwrap_or_else(|err| panic!("pair check: {} ({err})", err.code()));
    let check_json = serde_json::to_value(&check).expect("check json");
    let nodes = check_json["git"]["nodes"].as_array().expect("nodes");
    assert_eq!(nodes.len(), 2, "two managed nodes: {}", check_json["git"]);
    assert_eq!(nodes[0]["node"], "nodes.master");
    assert_eq!(nodes[1]["node"], "nodes.backup");
    assert_eq!(nodes[0]["state"], "managed");
    assert_eq!(nodes[1]["state"], "managed");
    assert!(
        check_json["git"]["expected_drift"].is_null()
            && check_json["git"]["expected_drift_complete"] == false,
        "--check reports the drift as unknown: {}",
        check_json["git"]
    );
    let after = (dir_manifest(&gitdir_a), dir_manifest(&gitdir_b));
    assert!(
        before == after,
        "--check changed a node's .git (master equal {}, backup equal {})",
        before.0 == after.0,
        before.1 == after.1
    );
    eprintln!(
        "LIVE-PAIR check: nodes=2 master_state={} backup_state={} master_head_branch_tip_is_v1_0_0={} backup_head_is_v1_0_0={} git_manifests_unchanged=true files_master={} files_backup={}",
        nodes[0]["state"],
        nodes[1]["state"],
        nodes[0]["head"] == serde_json::Value::String(c1.clone()),
        nodes[1]["head"] == serde_json::Value::String(c1.clone()),
        before.0.len(),
        before.1.len()
    );

    // The apply, with the digest the check showed.
    let flags = DeployFlags {
        artifact_sha256: Some(check.artifact_sha256.clone()),
        first_change: true,
        ..DeployFlags::default()
    };
    let timeline = PairTimeline::new();
    let extras = Extras {
        observer: Some(&timeline),
        git_writer: None,
    };
    let started = Instant::now();
    let applied = deploy_in_process_with(&rig, false, ENV, "v1.1.0", &flags, "pair-apply", &extras)
        .await
        .unwrap_or_else(|err| panic!("pair apply: {} ({err})", err.code()));
    let apply_ms = started.elapsed().as_millis();
    let applied_json = serde_json::to_value(&applied).expect("apply json");
    for (at, name, node) in timeline.seen.lock().expect("timeline").iter() {
        eprintln!("LIVE-PAIR event {name} node={node} at {at} ms");
    }
    // Every finding of the run is recorded and the gate goes on, so one
    // difference never hides the next measurement (the JGit comparison and the
    // `nodes_disagree` refusal below); the gate fails at the end naming each.
    let mut problems: Vec<String> = Vec::new();
    if applied.outcome.as_str() != "imported" {
        problems.push(format!(
            "pair outcome is {} (slug {})",
            applied.outcome.as_str(),
            applied_json["slug"]
        ));
    }
    let nodes = applied_json["git"]["nodes"].as_array().expect("nodes");
    assert_eq!(nodes.len(), 2, "{}", applied_json["git"]);
    for (index, label) in ["master", "backup"].iter().enumerate() {
        let restore = &nodes[index]["restore"];
        eprintln!(
            "LIVE-PAIR {label} apply: restored={} advanced={} verified={} post_status={} dropped={} reason={}",
            restore["restored"],
            restore["advanced"],
            restore["verified"],
            restore["post_status"],
            restore["dropped"],
            restore["reason"]
        );
        if !(restore["advanced"] == true && restore["verified"] == true) {
            problems.push(format!("{label}: the handover did not complete: {restore}"));
        }
        if restore["post_status"] != "matched" {
            problems.push(format!(
                "{label}: post_status is {} ({}); the node is local, so the run's own post-advance disk status ran",
                restore["post_status"], nodes[index]["restore"]["post_status_note"]
            ));
        }
        if !strs(&restore["dropped"]).iter().any(|path| path == NOTES) {
            problems.push(format!(
                "{label}: restore.dropped does not name the unlisted member"
            ));
        }
    }
    let wait_ms = match (
        timeline.at("git_advanced", "nodes.master"),
        timeline.at("git_advanced", "nodes.backup"),
    ) {
        (Some(master), Some(backup)) => Some(backup.saturating_sub(master)),
        _ => None,
    };
    eprintln!(
        "LIVE-PAIR backup wait: master_git_advanced_to_backup_git_advanced_ms={wait_ms:?} apply_ms={apply_ms}"
    );

    // Both repositories are at the release.
    for (label, node) in [("master", &node_a), ("backup", &node_b)] {
        let (state, refs) = node_state(node, &host).await;
        if state.kind != GitStateKind::Managed {
            problems.push(format!("{label}: the repository is {:?}", state.kind));
        }
        if head_commit(&refs) != c2 {
            problems.push(format!("{label}: HEAD is not the release"));
        }
    }
    eprintln!("LIVE-PAIR heads checked against the release");

    // JGit inside each node's module says what the run predicted (D-25).
    assert_eq!(applied_json["git"]["expected_drift_complete"], true);
    let predicted = strs(&applied_json["git"]["expected_drift"]);
    assert_eq!(
        predicted.len() as u64,
        applied_json["git"]["expected_drift_count"]
            .as_u64()
            .unwrap_or(u64::MAX),
        "the drift list is not capped here"
    );
    tokio::time::sleep(Duration::from_secs(10)).await;
    let mut jgit_rows = Vec::new();
    for (label, api) in [("master", &api_a), ("backup", &api_b)] {
        let jg = jgit(api, &config).await;
        let (extra, missing) = drift_difference(&jg, &predicted);
        eprintln!(
            "LIVE-PAIR {label} jgit: uncommitted={} expected={} extra={} missing={} unpushed={} branch={}",
            jg.paths.len(),
            predicted.len(),
            extra.len(),
            missing.len(),
            jg.unpushed,
            jg.branch
        );
        for row in &jg.rows {
            eprintln!("LIVE-PAIR {label} jgit-row {row}");
        }
        if !(extra.is_empty() && missing.is_empty()) {
            problems.push(format!(
                "{label}: CUT-OVER BLOCKER (D-25): JGit differs from expected_drift: extra={extra:?} missing={missing:?}"
            ));
        }
        if jg.unpushed != 0 {
            problems.push(format!("{label}: unpushed commits after the deploy"));
        }
        if jg.branch != "production" {
            problems.push(format!("{label}: current branch is {}", jg.branch));
        }
        let deleted: Vec<&String> = jg
            .rows
            .iter()
            .filter(|row| row.starts_with("Deleted "))
            .collect();
        if !deleted.is_empty() {
            problems.push(format!(
                "{label}: CUT-OVER BLOCKER (D-25): JGit lists {} path(s) as Deleted that expected_drift lists as modified: {deleted:?}",
                deleted.len()
            ));
        }
        if extra.is_empty() && missing.is_empty() && deleted.is_empty() {
            eprintln!(
                "LIVE-CHECK pair {label} jgit_equals_expected_drift files={}",
                predicted.len()
            );
        }
        jgit_rows.push(serde_json::json!({
            "node": label,
            "jgit_paths": jg.paths,
            "unpushed": jg.unpushed,
            "branch": jg.branch,
        }));
    }

    // The advance holds on the backup: the redundancy sync does not take the
    // advanced repository away a while later (G5).
    tokio::time::sleep(Duration::from_secs(20)).await;
    let (state_b, refs_b) = node_state(&node_b, &host).await;
    if state_b.kind != GitStateKind::Managed || head_commit(&refs_b) != c2 {
        problems.push("the backup's advanced HEAD is gone 20 s after the run".to_string());
    } else {
        eprintln!("LIVE-PAIR backup repository still at the release 20 s after the run");
    }

    // D-16, D-19: the backup's repository one commit behind the master's is
    // refused before anything is written.
    let backup_ref = gitdir_b.join("refs/heads/production");
    let kept = std::fs::read(&backup_ref).expect("the backup's branch ref is a loose file");
    std::fs::write(&backup_ref, format!("{parent}\n")).expect("move the backup behind");
    let flags_check = DeployFlags::default();
    let outcome = deploy_in_process(&rig, true, ENV, "v1.1.0", &flags_check, "pair-disagree").await;
    // Whatever the check answered, the backup goes back as it was.
    std::fs::write(&backup_ref, &kept).expect("put the backup's ref back");
    let disagree = match outcome {
        Err(CoreError::ProjectGitManaged { reason, .. }) => {
            eprintln!("LIVE-REFUSAL project_git_managed reason={reason}");
            if reason != "nodes_disagree" {
                problems.push(format!(
                    "the refusal reason is {reason}, not nodes_disagree"
                ));
            }
            reason
        }
        Err(other) => {
            problems.push(format!(
                "expected project_git_managed nodes_disagree, got {}: {other}",
                other.code()
            ));
            other.code().to_string()
        }
        Ok(result) => {
            let action = serde_json::to_value(&result).expect("json")["action"].clone();
            problems.push(format!(
                "expected a nodes_disagree refusal, the check answered action {action}"
            ));
            "not refused".to_string()
        }
    };
    let (_, refs_back) = node_state(&node_b, &host).await;
    assert_eq!(head_commit(&refs_back), c2, "the backup's ref is back");

    write_evidence(
        &rig,
        "git_pair_e2e",
        &serde_json::json!({
            "outcome": applied.outcome.as_str(),
            "apply_ms": apply_ms,
            "backup_wait_ms": wait_ms,
            "nodes": nodes
                .iter()
                .map(|n| serde_json::json!({
                    "node": n["node"],
                    "restored": n["restore"]["restored"],
                    "advanced": n["restore"]["advanced"],
                    "verified": n["restore"]["verified"],
                    "post_status": n["restore"]["post_status"],
                }))
                .collect::<Vec<_>>(),
            "expected_drift": predicted,
            "jgit": jgit_rows,
            "nodes_disagree": disagree,
            "problems": problems,
        }),
    );
    assert!(
        problems.is_empty(),
        "findings of the pair run: {problems:#?}"
    );
    eprintln!("LIVE-RAN git_pair_e2e");
}

/// 27-38 (GITDEP-09 detection, never the passing gate): proves ign's
/// `backup_project_json_missing` guard on a real pair that runs the defective
/// git module 2.3.4 on both nodes. With the module's backup `project.json`
/// deletion present, one apply of v1.1.0 must end `git_restore_failed` (slug
/// `deploy_git_restore_failed`, exit 6) with `restore.reason`
/// `backup_project_json_missing` on `nodes.backup` and the backup's save kept,
/// while the master is as strict and green as `git_pair_e2e` demands.
///
/// 27-61: on any other release the check skips before any state read or
/// deploy, because 27-60 showed the fix on 2.3.5 and `git_pair_e2e` asserts it
/// there. The counted proof is `capture27-pair.sh detect`, which needs the
/// anchored `LIVE-DETECT git_pair_detect_live` line, a different line kind from
/// the passing-gate line; a skip prints neither, so it never counts. When the
/// defect does not reproduce on 2.3.4 the check prints
/// `LIVE-DETECT-NA defect not reproduced` and fails.
#[tokio::test]
#[ignore = "needs the pair rig with git module 2.3.4 on both nodes (IGN_PAIR_MODULE_VER=2.3.4): IGNITION_LIVE_PAIR=1"]
async fn git_pair_detect_live() {
    let Some(rig) = pair_rig().await else { return };
    let backup = rig.backup.as_ref().expect("pair");
    let api_a = rig.api();
    let api_b = pair_api_b(&rig);
    // 27-61: the guard's live proof needs the defective release on both nodes.
    let version_a = git_module_version(&api_a, "master").await;
    let version_b = git_module_version(&api_b, "backup").await;
    if !(is_defect_release(&version_a) && is_defect_release(&version_b)) {
        skip(&format!(
            "git module {version_a} on the master and {version_b} on the backup: \
             this check proves ign's backup_project_json_missing guard on \
             {DEFECT_MODULE_RELEASE} only (build the pair with \
             IGN_PAIR_MODULE_VER={DEFECT_MODULE_RELEASE}); git_pair_e2e is the gate \
             on a fixed module"
        ));
        return;
    }
    let host = HostGit::new();
    let node_a = LocalFsTransport::new(rig.data.clone(), "nodes.master");
    let node_b = LocalFsTransport::new(backup.data.clone(), "nodes.backup");

    // 1. The starting state: both nodes managed, HEAD at v1.0.0.
    let (_keep, _checkout, c2) = release_checkout(&rig, "v1.1.0");
    let (_keep0, _checkout0, c1) = release_checkout(&rig, "v1.0.0");
    for (label, node) in [("master", &node_a), ("backup", &node_b)] {
        let (state, refs) = node_state(node, &host).await;
        assert_eq!(
            state.kind,
            GitStateKind::Managed,
            "{label}: the module's clone must classify as managed: {state:?}"
        );
        assert_eq!(
            head_commit(&refs),
            c1,
            "{label}: the module's clone must sit at v1.0.0 (run the pair driver's reset first)"
        );
    }
    eprintln!("LIVE-DETECT-STEP start: both nodes managed at v1.0.0");

    // The scriptExec route follows to the backup by the project sync; JGit is
    // asked on each node through its own route.
    let (_work, config) = script_config(&api_a, &rig.url).await;
    let started = Instant::now();
    loop {
        match script_run(&api_b, &config, "rig", WEBDEV_PROJECT, "print 1").await {
            Ok(_) => break,
            Err(err) => {
                assert!(
                    started.elapsed() < Duration::from_secs(300),
                    "the scriptExec route never answered on the backup: {}",
                    err.code()
                );
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        }
    }

    // 2. The apply of v1.1.0 with the digest a check showed.
    let first = DeployFlags {
        first_change: true,
        ..DeployFlags::default()
    };
    let check = deploy_in_process(&rig, true, ENV, "v1.1.0", &first, "detect-check")
        .await
        .unwrap_or_else(|err| panic!("detect check: {} ({err})", err.code()));
    let flags = DeployFlags {
        artifact_sha256: Some(check.artifact_sha256.clone()),
        first_change: true,
        ..DeployFlags::default()
    };
    let applied = deploy_in_process(&rig, false, ENV, "v1.1.0", &flags, "detect-apply")
        .await
        .unwrap_or_else(|err| panic!("detect apply: {} ({err})", err.code()));
    let applied_json = serde_json::to_value(&applied).expect("apply json");

    // The backup's project.json on disk, read first: a regular file here means
    // the defect did not occur (the module fix is installed).
    let project_json_b = backup.data.join("projects").join(APP).join("project.json");
    let json_state = match std::fs::symlink_metadata(&project_json_b) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => "absent",
        Err(err) => panic!("the backup's project.json cannot be read: {err}"),
        Ok(meta) if meta.is_dir() => "directory",
        Ok(meta) if meta.is_file() => "file",
        Ok(_) => "other",
    };
    if json_state == "file" {
        eprintln!("LIVE-DETECT-NA defect not reproduced");
        panic!(
            "the backup's project.json is a regular file after the deploy; outcome {} (slug {})",
            applied.outcome.as_str(),
            applied_json["slug"]
        );
    }

    let mut problems: Vec<String> = Vec::new();

    // 3. The detection.
    if applied.outcome.as_str() != "git_restore_failed" {
        problems.push(format!(
            "outcome is {}, not git_restore_failed",
            applied.outcome.as_str()
        ));
    }
    if applied_json["slug"] != "deploy_git_restore_failed" {
        problems.push(format!("slug is {}", applied_json["slug"]));
    }
    if applied.failure_exit_code() != Some(6) {
        problems.push(format!(
            "failure_exit_code is {:?}, not Some(6)",
            applied.failure_exit_code()
        ));
    }
    let nodes = applied_json["git"]["nodes"].as_array().expect("nodes");
    assert_eq!(nodes.len(), 2, "{}", applied_json["git"]);
    assert_eq!(nodes[0]["node"], "nodes.master");
    assert_eq!(nodes[1]["node"], "nodes.backup");
    let restore_a = &nodes[0]["restore"];
    let restore_b = &nodes[1]["restore"];
    if restore_b["reason"] != "backup_project_json_missing" {
        problems.push(format!(
            "backup restore.reason is {}, not backup_project_json_missing",
            restore_b["reason"]
        ));
    }
    if restore_b["save_kept"] != true {
        problems.push(format!(
            "backup restore.save_kept is {}",
            restore_b["save_kept"]
        ));
    }
    let save_path = nodes[1]["save"]["path"].as_str().unwrap_or_default();
    let save_repo = backup.data.join(save_path);
    if save_path.is_empty() || !save_repo.is_dir() {
        problems.push(format!(
            "the backup's kept save is not on disk: save.path {save_path:?}"
        ));
    }
    let warned = strs(&applied_json["warnings"]).iter().any(|line| {
        line.contains("backup") && line.contains("project.json") && line.contains("restart")
    });
    if !warned {
        problems.push("no warning names the backup, project.json and the restart".to_string());
    }

    // 4. project.json on the backup: absent or a directory.
    if !matches!(json_state, "absent" | "directory") {
        problems.push(format!("the backup's project.json is {json_state}"));
    }

    // 5. The master, as strict as git_pair_e2e.
    if !(restore_a["advanced"] == true && restore_a["verified"] == true) {
        problems.push(format!(
            "master: the handover did not complete: {restore_a}"
        ));
    }
    if restore_a["reason"] != serde_json::Value::Null {
        problems.push(format!("master: restore.reason is {}", restore_a["reason"]));
    }
    if restore_a["post_status"] != "matched" {
        problems.push(format!(
            "master: post_status is {} ({})",
            restore_a["post_status"], restore_a["post_status_note"]
        ));
    }
    if !strs(&restore_a["dropped"]).iter().any(|path| path == NOTES) {
        problems.push("master: restore.dropped does not name the unlisted member".to_string());
    }
    let (state_a, refs_a) = node_state(&node_a, &host).await;
    if state_a.kind != GitStateKind::Managed || head_commit(&refs_a) != c2 {
        problems.push("master: the repository is not managed at the release".to_string());
    }
    assert_eq!(applied_json["git"]["expected_drift_complete"], true);
    let predicted = strs(&applied_json["git"]["expected_drift"]);
    assert_eq!(
        predicted.len() as u64,
        applied_json["git"]["expected_drift_count"]
            .as_u64()
            .unwrap_or(u64::MAX),
        "the drift list is not capped here"
    );
    tokio::time::sleep(Duration::from_secs(10)).await;
    let jg_a = jgit(&api_a, &config).await;
    let (extra_a, missing_a) = drift_difference(&jg_a, &predicted);
    if !(extra_a.is_empty() && missing_a.is_empty()) {
        problems.push(format!(
            "master: JGit differs from expected_drift: extra={extra_a:?} missing={missing_a:?}"
        ));
    }
    let deleted_a: Vec<&String> = jg_a
        .rows
        .iter()
        .filter(|r| r.starts_with("Deleted "))
        .collect();
    if !deleted_a.is_empty() {
        problems.push(format!("master: JGit lists Deleted rows: {deleted_a:?}"));
    }
    if jg_a.unpushed != 0 {
        problems.push("master: unpushed commits after the deploy".to_string());
    }
    if jg_a.branch != "production" {
        problems.push(format!("master: current branch is {}", jg_a.branch));
    }
    eprintln!(
        "LIVE-DETECT-STEP master jgit: uncommitted={} expected={} extra={} missing={} unpushed={}",
        jg_a.paths.len(),
        predicted.len(),
        extra_a.len(),
        missing_a.len(),
        jg_a.unpushed
    );

    // 6. The backup's JGit list equals expected_drift except one Deleted
    // project.json row, which matches the detection.
    let jg_b = jgit(&api_b, &config).await;
    let (extra_b, missing_b) = drift_difference(&jg_b, &predicted);
    if !(extra_b.is_empty() && missing_b.is_empty()) {
        problems.push(format!(
            "backup: JGit differs from expected_drift: extra={extra_b:?} missing={missing_b:?}"
        ));
    }
    let deleted_b: Vec<&String> = jg_b
        .rows
        .iter()
        .filter(|r| r.starts_with("Deleted "))
        .collect();
    if deleted_b != [&"Deleted project.json".to_string()] {
        problems.push(format!(
            "backup: expected exactly one Deleted project.json row, got {deleted_b:?}"
        ));
    }
    eprintln!(
        "LIVE-DETECT-STEP backup jgit: uncommitted={} extra={} missing={} deleted={:?}",
        jg_b.paths.len(),
        extra_b.len(),
        missing_b.len(),
        deleted_b
    );

    write_evidence(
        &rig,
        "git_pair_detect_live",
        &serde_json::json!({
            "outcome": applied.outcome.as_str(),
            "slug": applied_json["slug"],
            "exit_code": applied.failure_exit_code(),
            "backup_reason": restore_b["reason"],
            "backup_save_kept": restore_b["save_kept"],
            "backup_save_path": save_path,
            "backup_project_json": json_state,
            "master_post_status": restore_a["post_status"],
            "expected_drift": predicted,
            "master_jgit": jg_a.rows,
            "backup_jgit": jg_b.rows,
            "problems": problems,
        }),
    );
    assert!(
        problems.is_empty(),
        "findings of the detection run: {problems:#?}"
    );
    eprintln!("LIVE-DETECT git_pair_detect_live");
}

// ---- 27-25 Task 3: the D-20 normalization of a real project repository ----------
//
// `capture27.sh g2norm <out-dir>` (never run by hand without its guards). The
// release tag of a LOCAL clone of the real project repository is built into an
// artifact exactly as the deploy builds it, imported into the local rig as a
// throwaway project, read back, and compared byte for byte (the definition of
// `data.git.expected_drift`: artifact bytes against the read-back export). The
// files the gateway rewrote are written to `<out-dir>/drift-files.txt`, with
// `<out-dir>/normalize.patch` that replaces each with the gateway-form bytes
// against the release tag. The output directory is outside this repository (the
// private repository); nothing is committed anywhere. Output here is counts
// only: no path, no name and no byte of the real repository is printed.

/// The D-20 normalization patch of `gateway` against the release tree: every
/// member is written as a blob from its raw bytes, so no attribute, filter or
/// line-ending setting of the repository can hide a change (W16, T-27-198).
fn normalization_patch(
    release_checkout: &Path,
    release_tag: &str,
    gateway: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<u8>, String> {
    normalization_patch_with(release_checkout, release_tag, gateway, &|_| {})
}

/// [`normalization_patch`] with `tamper` run on the patch before it is
/// verified. Only a test passes one: it proves a bad patch is refused.
///
/// The patch is built tree to tree, with no work tree and no conversion:
/// 1. a temporary index is read from the release tree;
/// 2. each gateway member is written as a blob (`hash-object -w --no-filters`)
///    and entered in that index with the mode the release tree gives it;
/// 3. `write-tree`, then `diff --binary <release tree> <gateway tree>`;
/// 4. the patch is applied (`apply --cached`) onto a second temporary index
///    from the release tree, and every member's blob is compared byte for byte
///    with the gateway bytes. The result tree must be the gateway tree.
///
/// Errors name the member that differs; a caller printing counts only must not
/// print them.
fn normalization_patch_with(
    release_checkout: &Path,
    release_tag: &str,
    gateway: &BTreeMap<String, Vec<u8>>,
    tamper: &dyn Fn(&mut Vec<u8>),
) -> Result<Vec<u8>, String> {
    if release_tag.is_empty() || release_tag.starts_with('-') {
        return Err("the release tag is not a revision name".to_string());
    }
    let scratch = tempfile::tempdir().map_err(|e| e.kind().to_string())?;
    let build_index = scratch.path().join("index-gateway");
    let check_index = scratch.path().join("index-check");
    let utf8 = |p: &Path| p.to_str().map(str::to_string).ok_or("a non-UTF-8 path");

    let release_tree = String::from_utf8_lossy(&norm_git(
        release_checkout,
        None,
        &["rev-parse", "--verify", &format!("{release_tag}^{{tree}}")],
        None,
    )?)
    .trim()
    .to_string();

    // The modes the release tree gives its files: a member keeps its own.
    let listing = norm_git(
        release_checkout,
        None,
        &["ls-tree", "-r", "-z", &release_tree],
        None,
    )?;
    let mut modes: BTreeMap<Vec<u8>, String> = BTreeMap::new();
    for entry in listing.split(|b| *b == 0).filter(|e| !e.is_empty()) {
        let tab = entry
            .iter()
            .position(|b| *b == b'\t')
            .ok_or("an ls-tree entry has no tab")?;
        let mode =
            String::from_utf8_lossy(&entry[..entry.iter().position(|b| *b == b' ').unwrap_or(0)])
                .to_string();
        modes.insert(entry[tab + 1..].to_vec(), mode);
    }

    // The gateway tree, from raw bytes.
    let build = utf8(&build_index)?;
    norm_git(
        release_checkout,
        Some(&build),
        &["read-tree", &release_tree],
        None,
    )?;
    let mut index_info: Vec<u8> = Vec::new();
    for (path, bytes) in gateway {
        let mode = modes
            .get(path.as_bytes())
            .filter(|mode| matches!(mode.as_str(), "100644" | "100755"))
            .ok_or_else(|| format!("{path}: not a regular file of the release tree"))?;
        let blob = String::from_utf8_lossy(&norm_git(
            release_checkout,
            Some(&build),
            &["hash-object", "-w", "--no-filters", "--stdin"],
            Some(bytes),
        )?)
        .trim()
        .to_string();
        index_info.extend_from_slice(format!("{mode} {blob}\t{path}").as_bytes());
        index_info.push(0);
    }
    norm_git(
        release_checkout,
        Some(&build),
        &["update-index", "-z", "--index-info"],
        Some(&index_info),
    )?;
    let gateway_tree = String::from_utf8_lossy(&norm_git(
        release_checkout,
        Some(&build),
        &["write-tree"],
        None,
    )?)
    .trim()
    .to_string();

    // The patch, tree to tree.
    let mut patch = norm_git(
        release_checkout,
        None,
        &[
            "diff",
            "--binary",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            &release_tree,
            &gateway_tree,
        ],
        None,
    )?;
    if patch.is_empty() {
        // The gateway bytes are the release bytes: nothing to carry.
        return if gateway_tree == release_tree {
            Ok(patch)
        } else {
            Err("the trees differ but the diff is empty".to_string())
        };
    }
    tamper(&mut patch);

    // The check: apply onto a second index, compare every member's blob.
    let check = utf8(&check_index)?;
    norm_git(
        release_checkout,
        Some(&check),
        &["read-tree", &release_tree],
        None,
    )?;
    norm_git(
        release_checkout,
        Some(&check),
        &["apply", "--cached", "--whitespace=nowarn", "-"],
        Some(&patch),
    )
    .map_err(|e| format!("the patch does not apply to the release tree: {e}"))?;
    let result_tree = String::from_utf8_lossy(&norm_git(
        release_checkout,
        Some(&check),
        &["write-tree"],
        None,
    )?)
    .trim()
    .to_string();
    for (path, bytes) in gateway {
        let blob = norm_git(
            release_checkout,
            None,
            &["cat-file", "blob", &format!("{result_tree}:{path}")],
            None,
        )?;
        if &blob != bytes {
            return Err(format!(
                "{path}: the patch does not reproduce the gateway bytes"
            ));
        }
    }
    if result_tree != gateway_tree {
        return Err("the patch changes something other than the listed members".to_string());
    }
    Ok(patch)
}

/// Run host git in `dir` for the normalization patch: no conversion setting,
/// no user or system config, no repository-selection variable (only the index
/// file this function chose); the stdout bytes, or a redacted error.
fn norm_git(
    dir: &Path,
    index: Option<&str>,
    args: &[&str],
    stdin: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    use std::io::Write;
    let mut full = vec!["-c", "core.autocrlf=false", "-c", "core.safecrlf=false"];
    full.extend_from_slice(args);
    let mut command = git_command(dir, &full);
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = command.spawn().map_err(|e| e.kind().to_string())?;
    let mut handle = child.stdin.take().ok_or("no stdin")?;
    if let Some(bytes) = stdin {
        // A child that exits early closes the pipe; its status says why.
        let _ = handle.write_all(bytes);
    }
    drop(handle);
    let out = child.wait_with_output().map_err(|e| e.kind().to_string())?;
    if !out.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.first().copied().unwrap_or(""),
            ignition_core::deploy::git_state::redact_text(&String::from_utf8_lossy(&out.stderr))
        ));
    }
    Ok(out.stdout)
}

/// A scratch repository with `files` committed under the tag `rel`.
#[cfg(test)]
fn norm_fixture(attributes: Option<&str>, files: &[(&str, &[u8])]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = dir.path().join("release");
    std::fs::create_dir_all(&repo).expect("repo dir");
    git(&repo, &["init", "-q"]);
    if let Some(text) = attributes {
        std::fs::write(repo.join(".gitattributes"), text).expect("attributes");
    }
    for (path, bytes) in files {
        let file = repo.join(path);
        std::fs::create_dir_all(file.parent().expect("parent")).expect("dirs");
        std::fs::write(file, bytes).expect("file");
    }
    git(&repo, &["add", "-A"]);
    git(
        &repo,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.invalid",
            "commit",
            "-q",
            "-m",
            "release",
        ],
    );
    git(&repo, &["tag", "rel"]);
    (dir, repo)
}

/// Apply `patch` onto a fresh index read from the tag `rel` and return the
/// blob it leaves at `path`. The check the function runs, made again here.
#[cfg(test)]
fn applied_blob(repo: &Path, patch: &[u8], path: &str) -> Vec<u8> {
    use std::io::Write;
    let scratch = tempfile::tempdir().expect("tempdir");
    let index = scratch.path().join("index");
    let index = index.to_str().expect("utf8");
    let run = |args: &[&str], stdin: Option<&[u8]>| -> Vec<u8> {
        let mut command = git_command(repo, args);
        command
            .env("GIT_INDEX_FILE", index)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let mut child = command.spawn().expect("git spawns");
        let mut handle = child.stdin.take().expect("stdin");
        if let Some(bytes) = stdin {
            handle.write_all(bytes).expect("stdin write");
        }
        drop(handle);
        let out = child.wait_with_output().expect("git runs");
        assert!(out.status.success(), "git {args:?} failed");
        out.stdout
    };
    run(&["read-tree", "rel^{tree}"], None);
    run(
        &["apply", "--cached", "--whitespace=nowarn", "-"],
        Some(patch),
    );
    let tree = String::from_utf8(run(&["write-tree"], None)).expect("utf8");
    let spec = format!("{}:{path}", tree.trim());
    run(&["cat-file", "blob", &spec], None)
}

#[test]
fn normalization_patch_carries_a_crlf_only_change() {
    let (_dir, repo) = norm_fixture(
        Some("* text eol=crlf\n"),
        &[("a/resource.json", b"{\"a\":1}\n")],
    );
    let gateway = BTreeMap::from([("a/resource.json".to_string(), b"{\"a\":1}\r\n".to_vec())]);
    let patch = normalization_patch(&repo, "rel", &gateway).expect("the patch is built");
    assert!(!patch.is_empty(), "a CRLF-only change must not vanish");
    assert_eq!(
        applied_blob(&repo, &patch, "a/resource.json"),
        b"{\"a\":1}\r\n".to_vec(),
        "the patch must carry the gateway bytes exactly"
    );
}

#[test]
fn normalization_patch_refuses_when_the_check_differs() {
    let (_dir, repo) = norm_fixture(None, &[("a/resource.json", b"{\"a\":1}\n")]);
    let gateway = BTreeMap::from([("a/resource.json".to_string(), b"{\"a\":2}\n".to_vec())]);
    let corrupt = |patch: &mut Vec<u8>| {
        let at = patch
            .windows(8)
            .position(|w| w == b"+{\"a\":2}")
            .expect("the added line");
        patch[at + 6] = b'3';
    };
    let err = normalization_patch_with(&repo, "rel", &gateway, &corrupt)
        .expect_err("a patch that does not reproduce the gateway bytes is refused");
    assert!(err.contains("a/resource.json"), "{err}");
    assert_eq!(
        git(&repo, &["status", "--porcelain"]),
        "",
        "a refused patch leaves the repository clean"
    );
}

#[test]
fn normalization_patch_is_empty_when_the_gateway_kept_the_release_bytes() {
    let (_dir, repo) = norm_fixture(None, &[("a/resource.json", b"{\"a\":1}\n")]);
    let gateway = BTreeMap::from([("a/resource.json".to_string(), b"{\"a\":1}\n".to_vec())]);
    let patch =
        normalization_patch(&repo, "rel", &gateway).expect("nothing to carry is not an error");
    assert!(patch.is_empty());
}

#[test]
fn normalization_patch_refuses_a_member_the_release_does_not_hold() {
    let (_dir, repo) = norm_fixture(None, &[("a/resource.json", b"{\"a\":1}\n")]);
    let gateway = BTreeMap::from([("a/other.json".to_string(), b"{}\n".to_vec())]);
    let err =
        normalization_patch(&repo, "rel", &gateway).expect_err("an unknown member is refused");
    assert!(err.contains("a/other.json"), "{err}");
}

#[test]
fn normalization_patch_carries_binary_bytes_and_keeps_the_mode() {
    let bytes: Vec<u8> = (0u8..=255).collect();
    let (_dir, repo) = norm_fixture(
        Some("*.bin -text\n"),
        &[("run.bin", &[1, 2, 3, 0, 4]), ("a/resource.json", b"{}\n")],
    );
    let gateway = BTreeMap::from([("run.bin".to_string(), bytes.clone())]);
    let patch = normalization_patch(&repo, "rel", &gateway).expect("the patch is built");
    assert_eq!(applied_blob(&repo, &patch, "run.bin"), bytes);
}

#[test]
fn live_git_helper_clears_repository_selection() {
    let dir = tempfile::tempdir().expect("tempdir");
    let here = dir.path().join("here");
    let other = dir.path().join("other");
    for repo in [&here, &other] {
        std::fs::create_dir_all(repo).expect("dir");
        git(repo, &["init", "-q"]);
    }
    let foreign_git_dir = other.join(".git");
    let injected = [
        ("GIT_DIR", foreign_git_dir.to_str().expect("utf8")),
        ("GIT_COMMON_DIR", foreign_git_dir.to_str().expect("utf8")),
    ];
    let out = git_command_with(&here, &["rev-parse", "--absolute-git-dir"], &injected)
        .output()
        .expect("git runs");
    assert!(out.status.success());
    let answered = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    assert_eq!(
        answered.canonicalize().expect("resolves"),
        here.join(".git").canonicalize().expect("resolves"),
        "the helper must act on its own directory, not the injected repository"
    );
}

const NORM_PROJECT: &str = "G2Norm";

#[tokio::test]
#[ignore = "needs the git-module rig and a local clone of the real project repository: IGNITION_LIVE_GIT=1, G2_REAL_REPO, G2_NORM_OUT, G2_NORM_TAG"]
async fn git_g2norm() {
    let Some(rig) = rig().await else { return };
    let real = PathBuf::from(require_env("G2_REAL_REPO"));
    let out = PathBuf::from(require_env("G2_NORM_OUT"));
    let tag = require_env("G2_NORM_TAG");
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repo root")
        .canonicalize()
        .expect("repo root resolves");
    assert!(
        !out.canonicalize()
            .expect("the output directory resolves")
            .starts_with(&repo_root),
        "the output directory is inside the repository"
    );
    assert!(
        !real
            .canonicalize()
            .expect("the real repository resolves")
            .starts_with(&repo_root),
        "the real repository is inside this repository"
    );

    // A scratch clone of the local repository (never a fetch), at the tag.
    let scratch = tempfile::tempdir().expect("tempdir");
    let work = scratch.path().join("release");
    git(
        scratch.path(),
        &[
            "clone",
            "-q",
            "--no-hardlinks",
            real.to_str().expect("utf8"),
            work.to_str().expect("utf8"),
        ],
    );
    git(&work, &["checkout", "-q", &tag]);
    let tracked = git(&work, &["ls-files"]).lines().count();
    let artifact = build_artifact(&work).expect("the release builds an artifact");
    let released = zip_members(&artifact.zip);

    // Import as a throwaway project, read back, delete.
    let api = rig.api();
    if GatewayApi::project_find(&api, NORM_PROJECT).await.is_ok() {
        GatewayApi::project_delete(&api, NORM_PROJECT)
            .await
            .expect("an earlier throwaway project is removed");
    }
    GatewayApi::project_import(&api, NORM_PROJECT, artifact.zip.clone(), false)
        .await
        .unwrap_or_else(|err| panic!("the throwaway import failed: {}", err.code()));
    // The gateway loads the imported project before its export is complete.
    tokio::time::sleep(Duration::from_secs(20)).await;
    let export = GatewayProjects(&api)
        .export_bytes(NORM_PROJECT)
        .await
        .map(|bytes| zip_members(&bytes));
    GatewayApi::project_delete(&api, NORM_PROJECT)
        .await
        .expect("the throwaway project is removed");
    let exported = export.expect("the read-back export");

    // The drift: members whose bytes the gateway changed.
    let drift: Vec<(&String, &Vec<u8>)> = exported
        .iter()
        .filter(|(path, bytes)| released.get(*path).is_some_and(|rel| rel != *bytes))
        .collect();
    let dropped = released
        .keys()
        .filter(|path| !exported.contains_key(*path))
        .count();
    let extra = exported
        .keys()
        .filter(|path| !released.contains_key(*path))
        .count();

    // The files and the patch: the release tree with the gateway-form bytes,
    // built from raw bytes tree to tree (W16): no work-tree write, so no
    // attribute or line-ending setting can hide a change. The error text names
    // a member, so none of it is printed here (counts only).
    let gateway: BTreeMap<String, Vec<u8>> = drift
        .iter()
        .map(|(path, bytes)| ((*path).clone(), (*bytes).clone()))
        .collect();
    let patch = normalization_patch(&work, &tag, &gateway)
        .unwrap_or_else(|_| panic!("the normalization patch failed its raw-byte verification"));
    let mut names: Vec<&str> = drift.iter().map(|(path, _)| path.as_str()).collect();
    names.sort_unstable();
    let mut listing = names.join("\n");
    if !listing.is_empty() {
        listing.push('\n');
    }
    std::fs::write(out.join("drift-files.txt"), listing).expect("the file list");
    std::fs::write(out.join("normalize.patch"), &patch).expect("the patch");
    println!(
        "LIVE-G2NORM tracked={tracked} artifact_members={} export_members={} drift={} dropped={dropped} extra={extra} patch_bytes={}",
        released.len(),
        exported.len(),
        drift.len(),
        patch.len()
    );
    println!("LIVE-RAN git_g2norm");
}

// ---- 27-61: the git module version a gate runs against -------------------------

/// 27-61: the detection predicate matches the defective release only, read the
/// way the gateway reports it (`2.3.4 (b0)`).
#[test]
fn is_defect_release_matches_only_the_2_3_4_release() {
    assert!(is_defect_release("2.3.4 (b0)"));
    assert!(is_defect_release("2.3.4"));
    for other in ["2.3.5 (b0)", "2.3.40", "2.3.4-rc1", ""] {
        assert!(
            !is_defect_release(other),
            "{other:?} is not the defect release"
        );
    }
}

/// The git module release with the backup `project.json` deletion (27-38).
const DEFECT_MODULE_RELEASE: &str = "2.3.4";

/// Whether the gateway-reported module `version` (`2.3.4 (b0)`) is the
/// defective release: its first whitespace-separated token is exactly
/// [`DEFECT_MODULE_RELEASE`].
fn is_defect_release(version: &str) -> bool {
    version.split_whitespace().next() == Some(DEFECT_MODULE_RELEASE)
}

/// The git module's version on one node, as the gateway's module list reports
/// it. A failed read or a missing module panics, naming the node.
async fn git_module_version(api: &dyn GatewayApi, node: &str) -> String {
    let listed = api
        .modules(false, &ListQuery::default())
        .await
        .unwrap_or_else(|err| panic!("{node}: the module list failed: {}", err.code()));
    listed
        .items
        .iter()
        .find(|module| module.id == GIT_MODULE.gateway_module_id)
        .map(|module| module.version.clone())
        .unwrap_or_else(|| {
            panic!(
                "{node}: the git module {} is not installed",
                GIT_MODULE.gateway_module_id
            )
        })
}
