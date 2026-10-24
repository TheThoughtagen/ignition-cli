//! Phase 25's rig gates for `deploy projects` (25-19): a project built from a
//! checkout-layout repo is created and then replaced on a real Ignition 8.3.6
//! gateway, and the gateway's own re-export is compared with the artifact the
//! tool built (DPRJ-03). The same rig shows the skip, the record-only
//! adoption, each refusal, the children report and a check that sends reads
//! only (DPRJ-05).
//!
//! The host is a `local:` node on the rig's host data root, onto which
//! `compose.projects.yml` binds the gateway's `data/projects` and
//! `data/config/resources`, so the `.git` probe, the system-properties file the
//! wrong-target check reads and the release records all sit where the engine
//! looks. The gateway calls go to the real rig through [`ReqwestGatewayApi`];
//! nothing here is mocked.
//!
//! ```text
//! cd crates/ignition-core/tests/fixtures/live-rig-deploy
//! bash capture25.sh up25          # a fresh rig, an empty data root, cap-a minted
//! . ~/.cache/ign-deploy-rig/tokens.env
//! IGNITION_LIVE_DEPLOY=1 IGNITION_LIVE_URL=http://localhost:18290 \
//!   IGNITION_LIVE_TOKEN="$IGN_TOKEN_CAP_A" \
//!   IGN_DEPLOY_RIG_DATA="$HOME/.cache/ign-deploy-rig/data25" \
//!   cargo test -p ignition-core --test live_deploy_projects -- --ignored --nocapture --test-threads=1
//! ```
//!
//! | var | meaning |
//! |---|---|
//! | `IGNITION_LIVE_DEPLOY` | must equal `1`; anything else is a quiet green skip |
//! | `IGNITION_LIVE_URL` | the rig's base URL; loopback hosts only (`localhost`, `127.0.0.1`, `::1`) |
//! | `IGNITION_LIVE_TOKEN` | cap-a's full `name:key` API token (never printed) |
//! | `IGN_DEPLOY_RIG_DATA` | the host data root whose `projects` and `config/resources` are bound into the rig |
//!
//! Safety: the URL guard refuses a non-loopback host before a request is made
//! and a name guard refuses any gateway whose `gateway-info` name is not
//! `ign-deploy-rig`. Every project the tests make is named `CAP25-*`, and every
//! test removes every `CAP25-*` project, its release record and any test `.git`
//! on entry and on exit, even when an assertion fails. The tests share one
//! gateway, so they take a process-wide lock. Output carries project names,
//! outcomes, counts and hashes, never a token or a file's bytes.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ignition_core::actions::deploy_projects::{
    CHILDREN_POLL_S, CHILDREN_SETTLE_S, DeployProjectsResult, GatewayProjects, NoProjectsObserver,
    ProjectOutcome, ProjectsInput, ProjectsPrepared, ProjectsRunOptions, ProjectsSurfaces,
    deploy_projects_prepare, deploy_projects_run, load_project,
};
use ignition_core::actions::resources::export_zip_bytes;
use ignition_core::client::query::ListQuery;
use ignition_core::client::{GatewayApi, ReqwestGatewayApi};
use ignition_core::config::{Credential, Secret};
use ignition_core::deploy::apply::preflight::DesignersPolicy;
use ignition_core::deploy::project_artifact::canonical_digest;
use ignition_core::deploy::projects::{ProjectAction, ReleaseRecord};
use ignition_core::deploy::transport::projects::project_host_for_node;
use ignition_core::error::CoreError;

/// The only gateway this gate may talk to.
const RIG_GATEWAY_NAME: &str = "ign-deploy-rig";

/// The target's gateway directory name: the release records live below
/// `ign-deploy/<this>/projects/<project>/` on the host data root.
const GATEWAY_DIR: &str = "ign-deploy-rig";
const ENV: &str = "rig";

/// Every project and record this suite makes starts with this.
const PREFIX: &str = "CAP25-";

/// The tests share one gateway; they run one at a time.
static RIG_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
        .unwrap_or_else(|| panic!("{name} must be set to run the live projects gate"))
}

#[test]
fn local_url_guard_refuses_non_loopback() {
    for raw in [
        "http://10.4.8.15:8088",
        "https://wignvm01.example.local:8043",
        "http://localhost.evil.example:18290",
        "http://127.0.0.2:18290",
        "http://52.154.75.231:8088",
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

// ---- the rig ---------------------------------------------------------------------------

/// What a live test needs once the gate is open.
#[derive(Clone)]
struct Rig {
    url: String,
    token: String,
    data_dir: PathBuf,
}

impl Rig {
    fn api_at(&self, url: &str) -> ReqwestGatewayApi {
        ReqwestGatewayApi::for_tests(
            url,
            Some(Credential::Token(Secret::new(self.token.clone()))),
        )
    }

    /// The rig directly, for the harness and for every run that may write.
    fn api(&self) -> ReqwestGatewayApi {
        self.api_at(&self.url)
    }

    fn projects_dir(&self) -> PathBuf {
        self.data_dir.join("projects")
    }

    /// Where the release record of `project` lives on the host.
    fn record_path(&self, project: &str) -> PathBuf {
        self.data_dir
            .join("ign-deploy")
            .join(GATEWAY_DIR)
            .join("projects")
            .join(project)
            .join("release.json")
    }

    /// The record on the host, parsed as the engine parses it.
    fn read_record(&self, project: &str) -> Option<ReleaseRecord> {
        let bytes = std::fs::read(self.record_path(project)).ok()?;
        let location = self.record_path(project).display().to_string();
        Some(
            ReleaseRecord::parse(&bytes, &location, GATEWAY_DIR, project)
                .expect("the record on the host parses"),
        )
    }

    /// The canonical digest of the gateway's own export of `project`.
    async fn live_digest(&self, project: &str) -> String {
        let zip = export_zip_bytes(&self.api(), project)
            .await
            .unwrap_or_else(|err| panic!("export of {project}: {err}"));
        canonical_digest(&zip)
            .expect("the export has members")
            .sha256
    }

    async fn project_names(&self) -> Vec<String> {
        self.api()
            .projects(&ListQuery::default())
            .await
            .expect("projects/list")
            .items
            .into_iter()
            .map(|r| r.name)
            .collect()
    }

    /// Remove every `CAP25-*` project (children first), its release records
    /// and any directory left below `projects/`. Idempotent; best effort on
    /// the gateway, strict about the leftovers it reports.
    async fn sweep(&self) {
        let api = self.api();
        let mut ours: Vec<_> = match api.projects(&ListQuery::default()).await {
            Ok(page) => page
                .items
                .into_iter()
                .filter(|r| r.name.starts_with(PREFIX))
                .collect(),
            Err(err) => {
                eprintln!("sweep: projects/list failed ({})", err.code());
                Vec::new()
            }
        };
        // A project that has a parent goes before the parent.
        ours.sort_by_key(|r| r.parent.as_deref().unwrap_or("").is_empty());
        for record in &ours {
            if let Err(err) = api.project_delete(&record.name).await {
                eprintln!("sweep: delete {} failed ({})", record.name, err.code());
            }
        }
        self.sweep_host();
    }

    /// The host side of [`Rig::sweep`], which is also what the `Drop` guard
    /// can do without a runtime.
    fn sweep_host(&self) {
        remove_prefixed(&self.projects_dir());
        remove_prefixed(
            &self
                .data_dir
                .join("ign-deploy")
                .join(GATEWAY_DIR)
                .join("projects"),
        );
    }
}

/// Remove the `CAP25-*` entries directly below `dir`.
fn remove_prefixed(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().starts_with(PREFIX) {
            let path = entry.path();
            if let Err(err) = std::fs::remove_dir_all(&path) {
                eprintln!("sweep: cannot remove {}: {err}", path.display());
            }
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
    let data_dir = PathBuf::from(require_env("IGN_DEPLOY_RIG_DATA"));
    if let Err(why) = local_url_guard(&url) {
        panic!("IGNITION_LIVE_URL refused: {why}");
    }
    assert!(
        data_dir.join("projects").is_dir() && data_dir.join("config").join("resources").is_dir(),
        "IGN_DEPLOY_RIG_DATA must be the host data root whose projects and config/resources are \
         bound into the rig (compose.projects.yml)"
    );
    let rig = Rig {
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

/// Sweeps at the start (leftovers of a cut-off run) and again when the test
/// ends, even by a panic.
struct RigGuard {
    rig: Rig,
}

impl RigGuard {
    async fn open(rig: &Rig) -> RigGuard {
        rig.sweep().await;
        RigGuard { rig: rig.clone() }
    }

    /// The checks every test ends with: nothing of ours is left.
    async fn assert_clean(&self) {
        self.rig.sweep().await;
        let left: Vec<String> = self
            .rig
            .project_names()
            .await
            .into_iter()
            .filter(|n| n.starts_with(PREFIX))
            .collect();
        assert!(left.is_empty(), "CAP25 projects left on the rig: {left:?}");
        for dir in [
            self.rig.projects_dir(),
            self.rig
                .data_dir
                .join("ign-deploy")
                .join(GATEWAY_DIR)
                .join("projects"),
        ] {
            let leftovers: Vec<String> = std::fs::read_dir(&dir)
                .map(|entries| {
                    entries
                        .flatten()
                        .map(|e| e.file_name().to_string_lossy().to_string())
                        .filter(|n| n.starts_with(PREFIX))
                        .collect()
                })
                .unwrap_or_default();
            assert!(
                leftovers.is_empty(),
                "CAP25 entries left below {}: {leftovers:?}",
                dir.display()
            );
        }
        eprintln!("cleanup: no CAP25 project, record or directory left on the rig");
    }
}

impl Drop for RigGuard {
    fn drop(&mut self) {
        // The gateway side needs a runtime; a panicking test may have none
        // to spare, so a fresh one runs on its own thread.
        let rig = self.rig.clone();
        let handle = std::thread::spawn(move || {
            if let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                runtime.block_on(rig.sweep());
            } else {
                rig.sweep_host();
            }
        });
        let _ = handle.join();
    }
}

// ---- the project repo ------------------------------------------------------------------

const PROJECT_JSON_PLAIN: &str = r#"{"title":"CAP25 live","enabled":true,"inheritable":false}"#;
const SCRIPT_RESOURCE: &str = r#"{"scope":"A","version":1,"restricted":false,"overridable":true,"files":["code.py"],"attributes":{}}"#;
const VIEW_RESOURCE: &str = r#"{"scope":"G","version":1,"restricted":false,"overridable":true,"files":["view.json"],"attributes":{}}"#;
const VIEW: &str =
    r#"{"custom":{},"params":{},"props":{},"root":{"type":"ia.container.flex","children":[]}}"#;

fn put(root: &Path, rel: &str, bytes: &[u8]) {
    let target = root.join(rel);
    std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
    std::fs::write(target, bytes).expect("write");
}

/// A checkout-layout project repo plus a targets root, on a rig host.
struct Repo {
    _scratch: tempfile::TempDir,
    repo: PathBuf,
    targets: PathBuf,
    project: String,
}

impl Repo {
    /// `parent` is written into `project.json`; `inheritable` lets other
    /// projects inherit from this one.
    fn new(rig: &Rig, project: &str, parent: Option<&str>, inheritable: bool, code: &str) -> Repo {
        assert!(project.starts_with(PREFIX), "{project} is not a CAP25 name");
        let scratch = tempfile::tempdir().expect("tempdir");
        let repo = scratch.path().join("repo");
        let targets = scratch.path().join("targets-root");
        std::fs::create_dir_all(&repo).expect("repo dir");
        put(
            &repo,
            "deploy/project.yaml",
            format!("schema_version: 1\nproject: {project}\nenvironments: [{ENV}]\n").as_bytes(),
        );
        put(&repo, ".gitignore", b".ign/\n");
        put(
            &targets,
            &format!("targets/{ENV}.yaml"),
            format!(
                "schema_version: 1\ngateway: {GATEWAY_DIR}\nmode: production\nnodes:\n  master:\n    \
                 profile: rig\n    gateway_name: {RIG_GATEWAY_NAME}\n    local:\n      data_dir: '{}'\n",
                rig.data_dir.display()
            )
            .as_bytes(),
        );
        let this = Repo {
            _scratch: scratch,
            repo,
            targets,
            project: project.to_string(),
        };
        this.write_project(parent, inheritable, code);
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(&this.repo)
            .args(["init", "-q"])
            .status()
            .expect("git runs");
        assert!(status.success(), "git init");
        this
    }

    fn write_project(&self, parent: Option<&str>, inheritable: bool, code: &str) {
        let project_json = match (parent, inheritable) {
            (Some(parent), _) => format!(
                r#"{{"title":"CAP25 live","enabled":true,"inheritable":false,"parent":"{parent}"}}"#
            ),
            (None, true) => {
                r#"{"title":"CAP25 live","enabled":true,"inheritable":true}"#.to_string()
            }
            (None, false) => PROJECT_JSON_PLAIN.to_string(),
        };
        put(&self.repo, "project.json", project_json.as_bytes());
        put(
            &self.repo,
            "ignition/script-python/cap25live/code.py",
            code.as_bytes(),
        );
        put(
            &self.repo,
            "ignition/script-python/cap25live/resource.json",
            SCRIPT_RESOURCE.as_bytes(),
        );
        put(
            &self.repo,
            "com.inductiveautomation.perspective/views/Cap25Live/view.json",
            VIEW.as_bytes(),
        );
        put(
            &self.repo,
            "com.inductiveautomation.perspective/views/Cap25Live/resource.json",
            VIEW_RESOURCE.as_bytes(),
        );
    }

    /// Change the library only (same parent and inheritance).
    fn set_code(&self, parent: Option<&str>, inheritable: bool, code: &str) {
        self.write_project(parent, inheritable, code);
    }

    fn input(&self, release: &str) -> ProjectsInput {
        ProjectsInput {
            repo: self.repo.clone(),
            targets: self.targets.clone(),
            env: ENV.to_string(),
            release: release.to_string(),
            allow_downgrade: false,
            allow_first_change: None,
            artifact_sha256: None,
        }
    }

    /// The canonical digest of the artifact this repo builds.
    fn artifact_sha(&self, release: &str) -> String {
        load_project(&self.input(release))
            .expect("the repo loads")
            .artifact
            .digest
            .sha256
    }
}

/// What a deploy does: the read-only check, or the confirmed apply.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Check,
    Apply,
}

/// Run `deploy projects` for `input` against the rig reached at `url`, the way
/// the CLI wires it: the real gateway client, the real host, the real record
/// writer, the CLI's own sweep timings.
async fn deploy(
    rig: &Rig,
    url: &str,
    input: &ProjectsInput,
    mode: Mode,
) -> Result<DeployProjectsResult, CoreError> {
    let loaded = load_project(input)?;
    let host = project_host_for_node(
        &input.targets,
        &loaded.target_file,
        "nodes.master",
        &loaded.target.nodes.master,
    )?;
    let api = rig.api_at(url);
    let gateway = GatewayProjects(&api);
    let surfaces = ProjectsSurfaces {
        gateway: &gateway,
        host: host.as_ref(),
    };
    let prepared = deploy_projects_prepare(&loaded, input, &surfaces).await?;
    if mode == Mode::Check {
        return Ok(prepared.into_check_result());
    }
    if let ProjectsPrepared::Unchanged(result) = prepared {
        return Ok(result);
    }
    deploy_projects_run(
        prepared,
        &loaded,
        input,
        &surfaces,
        &ProjectsRunOptions {
            repo: input.repo.clone(),
            targets_root: input.targets.clone(),
            target_file: loaded.target_file.clone(),
            master: loaded.target.nodes.master.clone(),
            record_writer: None,
            designers: DesignersPolicy::default(),
            children_settle: Duration::from_secs(CHILDREN_SETTLE_S),
            children_poll: Duration::from_secs(CHILDREN_POLL_S),
        },
        &NoProjectsObserver,
    )
    .await
}

/// An apply that must complete.
async fn apply_ok(rig: &Rig, repo: &Repo, release: &str) -> DeployProjectsResult {
    let started = Instant::now();
    let result = deploy(rig, &rig.url, &repo.input(release), Mode::Apply)
        .await
        .unwrap_or_else(|err| panic!("{} {release}: {} ({err})", repo.project, err.code()));
    eprintln!(
        "{} {release}: outcome {} action {:?} ({} ms)",
        repo.project,
        result.outcome.as_str(),
        result.action,
        started.elapsed().as_millis()
    );
    result
}

// ---- the gates -------------------------------------------------------------------------

/// DPRJ-01 and DPRJ-03: a checkout-built project is created, then replaced by
/// a changed release, and the gateway's own re-export has the artifact's
/// canonical digest in both runs.
#[tokio::test]
#[ignore = "needs the 8.3.6 rig: see the module doc"]
async fn live_projects_create_then_import() {
    let Some(rig) = rig().await else { return };
    let _lock = RIG_LOCK.lock().await;
    let guard = RigGuard::open(&rig).await;

    let repo = Repo::new(&rig, "CAP25-LIVE", None, false, "def f():\n    return 1\n");
    let v1_artifact = repo.artifact_sha("v1.0.0");

    // Run 1: the project is absent, so it is created (D-13).
    let first = apply_ok(&rig, &repo, "v1.0.0").await;
    assert_eq!(first.outcome, ProjectOutcome::Created);
    assert_eq!(first.action, ProjectAction::Create);
    assert!(first.ok && first.record_written, "create writes the record");
    assert_eq!(first.artifact_sha256, v1_artifact);
    let live_1 = rig.live_digest("CAP25-LIVE").await;
    eprintln!(
        "run 1 created: artifact {} gateway re-export {}",
        &first.artifact_sha256, &live_1
    );
    assert_eq!(
        live_1, first.artifact_sha256,
        "the gateway's re-export equals the artifact after the create"
    );
    let record = rig.read_record("CAP25-LIVE").expect("release record 1");
    assert_eq!(record.release, "v1.0.0");
    assert_eq!(record.artifact_sha256, first.artifact_sha256);

    // Run 2: a changed library, a newer release: an overwrite import.
    repo.set_code(None, false, "def f():\n    return 2\n");
    let v2_artifact = repo.artifact_sha("v1.1.0");
    assert_ne!(v1_artifact, v2_artifact, "the change moves the digest");
    let second = apply_ok(&rig, &repo, "v1.1.0").await;
    assert_eq!(second.outcome, ProjectOutcome::Imported);
    assert_eq!(second.action, ProjectAction::Import);
    assert!(second.ok && second.record_written);
    assert_eq!(second.live_sha256.as_deref(), Some(v1_artifact.as_str()));
    assert_eq!(second.artifact_sha256, v2_artifact);
    assert!(
        second.warnings.is_empty(),
        "no warning expected: {:?}",
        second.warnings
    );
    let live_2 = rig.live_digest("CAP25-LIVE").await;
    eprintln!(
        "run 2 imported: artifact {} gateway re-export {}",
        &second.artifact_sha256, &live_2
    );
    assert_eq!(
        live_2, second.artifact_sha256,
        "the gateway's re-export equals the artifact after the import"
    );
    let run_dir = repo
        .repo
        .join(second.run_dir.as_deref().expect("the import has a run dir"));
    let pre_image = run_dir.join("export/CAP25-LIVE.zip");
    assert!(pre_image.is_file(), "the pre-image export was saved");
    let pre = canonical_digest(&std::fs::read(&pre_image).expect("read pre-image"))
        .expect("pre-image digest")
        .sha256;
    assert_eq!(pre, v1_artifact, "the pre-image is what run 1 deployed");
    let record = rig.read_record("CAP25-LIVE").expect("release record 2");
    assert_eq!(record.release, "v1.1.0");
    assert_eq!(record.artifact_sha256, second.artifact_sha256);
    assert!(
        first.live_sha256.is_none() && first.members.removed == 0,
        "a create has no live side"
    );

    guard.assert_clean().await;
}
